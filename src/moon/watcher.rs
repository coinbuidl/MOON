use crate::moon::archive::{
    ArchivePipelineOutcome, archive_and_index, read_ledger_records, remove_ledger_records,
};
use crate::moon::audit;
use crate::moon::channel_archive_map;
use crate::moon::config::{MoonRetentionConfig, load_config};
use crate::moon::continuity::{ContinuityOutcome, build_continuity};
use crate::moon::distill::{
    DistillInput, DistillOutput, archive_file_size, distill_chunk_bytes, load_archive_excerpt,
    run_chunked_archive_distillation, run_distillation,
};
use crate::moon::inbound_watch::{self, InboundWatchOutcome};
use crate::moon::paths::resolve_paths;
use crate::moon::qmd;
use crate::moon::session_usage::{
    SessionUsageSnapshot, collect_openclaw_usage_batch, collect_usage,
};
use crate::moon::snapshot::latest_session_file;
use crate::moon::state::{load, save};
use crate::moon::thresholds::{TriggerKind, evaluate};
use crate::moon::warn;
use crate::openclaw::gateway;
use anyhow::{Context, Result};
use chrono::{Local, TimeZone};
use fs2::FileExt;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

const DEFAULT_HIGH_TOKEN_ALERT_THRESHOLD: u64 = 1_000_000;
const MAX_HIGH_TOKEN_ALERT_SESSIONS: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DistillTriggerMode {
    Manual,
    Idle,
}

impl DistillTriggerMode {
    fn from_config_mode(raw: &str) -> Self {
        // Reserved for future trigger extensions (for example archive_event).
        if raw.eq_ignore_ascii_case("idle") {
            Self::Idle
        } else {
            Self::Manual
        }
    }
}

#[derive(Debug, Clone)]
pub struct WatchCycleOutcome {
    pub state_file: String,
    pub heartbeat_epoch_secs: u64,
    pub poll_interval_secs: u64,
    pub archive_threshold: f64,
    pub archive_trigger_enabled: bool,
    pub compaction_threshold: f64,
    pub distill_mode: String,
    pub distill_idle_secs: u64,
    pub distill_max_per_cycle: u64,
    pub distill_archive_grace_hours: u64,
    pub retention_active_days: u64,
    pub retention_warm_days: u64,
    pub retention_cold_days: u64,
    pub usage: SessionUsageSnapshot,
    pub triggers: Vec<String>,
    pub inbound_watch: InboundWatchOutcome,
    pub archive: Option<ArchivePipelineOutcome>,
    pub compaction_result: Option<String>,
    pub distill: Option<DistillOutput>,
    pub continuity: Option<ContinuityOutcome>,
    pub archive_retention_result: Option<String>,
}

fn run_archive_if_needed(
    paths: &crate::moon::paths::MoonPaths,
    trigger_set: &[TriggerKind],
) -> Result<Option<ArchivePipelineOutcome>> {
    let needs_archive = trigger_set
        .iter()
        .any(|t| matches!(t, TriggerKind::Archive));
    if !needs_archive {
        return Ok(None);
    }

    let Some(source) = latest_session_file(&paths.openclaw_sessions_dir)? else {
        anyhow::bail!("no source session file found in openclaw sessions dir");
    };

    let out = archive_and_index(paths, &source, "history")?;
    Ok(Some(out))
}

fn is_compaction_channel_session(session_id: &str) -> bool {
    session_id.contains(":discord:channel:") || session_id.contains(":whatsapp:")
}

fn is_cooldown_ready(last_epoch: Option<u64>, now_epoch: u64, cooldown_secs: u64) -> bool {
    match last_epoch {
        None => true,
        Some(last) => now_epoch.saturating_sub(last) >= cooldown_secs,
    }
}

fn high_token_alert_threshold() -> u64 {
    match env::var("MOON_HIGH_TOKEN_ALERT_THRESHOLD") {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return DEFAULT_HIGH_TOKEN_ALERT_THRESHOLD;
            }
            trimmed
                .parse::<u64>()
                .ok()
                .unwrap_or(DEFAULT_HIGH_TOKEN_ALERT_THRESHOLD)
        }
        Err(_) => DEFAULT_HIGH_TOKEN_ALERT_THRESHOLD,
    }
}

fn resolve_session_file_from_id(sessions_dir: &Path, session_id: &str) -> Option<PathBuf> {
    if session_id.trim().is_empty() {
        return None;
    }
    let jsonl = sessions_dir.join(format!("{session_id}.jsonl"));
    if jsonl.exists() && jsonl.is_file() {
        return Some(jsonl);
    }

    let json = sessions_dir.join(format!("{session_id}.json"));
    if json.exists() && json.is_file() {
        return Some(json);
    }

    None
}

fn load_session_source_map(sessions_dir: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let store = sessions_dir.join("sessions.json");
    if !store.exists() {
        return Ok(BTreeMap::new());
    }

    let raw = fs::read_to_string(&store)
        .with_context(|| format!("failed to read {}", store.display()))?;
    let parsed: Value = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse {}", store.display()))?;
    let object = parsed
        .as_object()
        .context("sessions.json should be an object map keyed by session key")?;

    let mut out = BTreeMap::new();
    for (key, entry) in object {
        let Some(session_id) = entry
            .get("sessionId")
            .and_then(Value::as_str)
            .or_else(|| entry.get("id").and_then(Value::as_str))
        else {
            continue;
        };
        if let Some(source) = resolve_session_file_from_id(sessions_dir, session_id) {
            out.insert(key.clone(), source);
        }
    }

    Ok(out)
}

fn cleanup_expired_distilled_archives(
    paths: &crate::moon::paths::MoonPaths,
    state: &mut crate::moon::state::MoonState,
    now_epoch_secs: u64,
    retention: &MoonRetentionConfig,
) -> Result<Option<String>> {
    let ledger = match read_ledger_records(paths) {
        Ok(records) => records,
        Err(err) => {
            warn::emit(
                "LEDGER_READ_FAILED",
                "archive-retention",
                "read-ledger",
                "na",
                "na",
                "na",
                "retry-next-cycle",
                "ledger-read-failed",
                &format!("{err:#}"),
            );
            return Ok(Some(format!(
                "retention_active_days={} retention_warm_days={} retention_cold_days={} removed=0 missing=0 failed=1 map_removed=0 ledger_removed=0 qmd_updated=false reason=ledger-read-failed",
                retention.active_days, retention.warm_days, retention.cold_days
            )));
        }
    };
    let ledger_by_archive = ledger
        .into_iter()
        .map(|r| (r.archive_path, r.created_at_epoch_secs))
        .collect::<BTreeMap<_, _>>();

    let seconds_per_day = 86_400u64;
    let mut active_count = 0usize;
    let mut warm_count = 0usize;
    let mut cold_candidates = 0usize;
    let mut purge_paths = BTreeSet::new();
    let mut removed_files = 0usize;
    let mut missing_files = 0usize;
    let mut failed = 0usize;

    let candidates = state
        .distilled_archives
        .iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect::<Vec<_>>();

    for (archive_path, distilled_at) in candidates {
        let Some(created_at) = ledger_by_archive.get(&archive_path).copied() else {
            warn::emit(
                "LEDGER_READ_FAILED",
                "archive-retention",
                "lookup-ledger-record",
                "na",
                &archive_path,
                "na",
                "skip-current-archive",
                "archive-path-missing-in-ledger",
                "missing-ledger-record",
            );
            continue;
        };

        let age_days = now_epoch_secs
            .saturating_sub(created_at)
            .saturating_div(seconds_per_day);
        if age_days <= retention.active_days {
            active_count += 1;
            continue;
        }
        if age_days <= retention.warm_days || age_days < retention.cold_days {
            warm_count += 1;
            continue;
        }
        cold_candidates += 1;

        if now_epoch_secs.saturating_sub(distilled_at) < seconds_per_day {
            // Require at least one day from distill marker before delete to reduce race risk.
            continue;
        }
        if Path::new(&archive_path).exists() {
            match fs::remove_file(&archive_path) {
                Ok(_) => {
                    removed_files += 1;
                    purge_paths.insert(archive_path.clone());
                    state.distilled_archives.remove(&archive_path);
                }
                Err(err) => {
                    failed += 1;
                    warn::emit(
                        "RETENTION_DELETE_FAILED",
                        "archive-retention",
                        "delete-archive",
                        "na",
                        &archive_path,
                        "na",
                        "retry-next-cycle",
                        "remove-file-failed",
                        &format!("{err:#}"),
                    );
                }
            }
        } else {
            missing_files += 1;
            purge_paths.insert(archive_path.clone());
            state.distilled_archives.remove(&archive_path);
        }
    }

    if purge_paths.is_empty() && failed == 0 {
        return Ok(None);
    }

    let map_removed = channel_archive_map::remove_by_archive_paths(paths, &purge_paths)?;
    let ledger_removed = remove_ledger_records(paths, &purge_paths)?;
    let qmd_updated = if !purge_paths.is_empty() {
        qmd::update(&paths.qmd_bin).is_ok()
    } else {
        false
    };

    Ok(Some(format!(
        "retention_active_days={} retention_warm_days={} retention_cold_days={} active={} warm={} cold_candidates={} removed={} missing={} failed={} map_removed={} ledger_removed={} qmd_updated={}",
        retention.active_days,
        retention.warm_days,
        retention.cold_days,
        active_count,
        warm_count,
        cold_candidates,
        removed_files,
        missing_files,
        failed,
        map_removed,
        ledger_removed,
        qmd_updated
    )))
}

fn day_key_for_epoch(epoch_secs: u64) -> String {
    Local
        .timestamp_opt(epoch_secs as i64, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| "1970-01-01".to_string())
}

fn acquire_daemon_lock() -> Result<File> {
    let paths = resolve_paths()?;
    fs::create_dir_all(&paths.logs_dir)
        .with_context(|| format!("failed to create {}", paths.logs_dir.display()))?;

    let lock_path = paths.logs_dir.join("moon-watch.daemon.lock");
    let mut lock_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
        .with_context(|| format!("failed to open daemon lock {}", lock_path.display()))?;

    match lock_file.try_lock_exclusive() {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::WouldBlock => {
            anyhow::bail!(
                "moon watcher daemon already running (lock: {})",
                lock_path.display()
            );
        }
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to lock daemon file {}", lock_path.display()));
        }
    }

    lock_file
        .set_len(0)
        .with_context(|| format!("failed to truncate daemon lock {}", lock_path.display()))?;
    writeln!(&mut lock_file, "{}", std::process::id())
        .with_context(|| format!("failed to write daemon lock {}", lock_path.display()))?;

    Ok(lock_file)
}

fn extract_key_decisions(summary: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in summary.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let normalized = trimmed
            .trim_start_matches("- ")
            .trim_start_matches("* ")
            .trim();
        if normalized.is_empty() {
            continue;
        }
        let lower = normalized.to_ascii_lowercase();
        if lower.contains("decision")
            || lower.contains("rule")
            || lower.contains("milestone")
            || lower.contains("next")
        {
            out.push(normalized.to_string());
        }
        if out.len() >= 8 {
            break;
        }
    }
    out
}

pub fn run_once() -> Result<WatchCycleOutcome> {
    let paths = resolve_paths()?;
    let cfg = load_config()?;
    let mut state = load(&paths)?;
    let inbound_watch = inbound_watch::process(&paths, &cfg, &mut state)?;

    let mut usage_batch_note = None;
    let usage_batch = match collect_openclaw_usage_batch() {
        Ok(batch) => Some(batch),
        Err(err) => {
            usage_batch_note = Some(format!("batch-scan failed: {err:#}"));
            None
        }
    };
    let usage = match &usage_batch {
        Some(batch) => batch.current.clone(),
        None => collect_usage(&paths)?,
    };
    state.last_heartbeat_epoch_secs = usage.captured_at_epoch_secs;
    state.last_session_id = Some(usage.session_id.clone());
    state.last_usage_ratio = Some(usage.usage_ratio);
    state.last_provider = Some(usage.provider.clone());

    let high_token_threshold = high_token_alert_threshold();
    if high_token_threshold > 0 {
        let mut high_token_sessions = Vec::<SessionUsageSnapshot>::new();
        if let Some(batch) = &usage_batch {
            high_token_sessions = batch
                .sessions
                .iter()
                .filter(|snapshot| snapshot.used_tokens >= high_token_threshold)
                .cloned()
                .collect::<Vec<_>>();
        } else if usage.used_tokens >= high_token_threshold {
            high_token_sessions.push(usage.clone());
        }

        if !high_token_sessions.is_empty() {
            high_token_sessions.sort_by(|left, right| right.used_tokens.cmp(&left.used_tokens));
            let preview = high_token_sessions
                .iter()
                .take(MAX_HIGH_TOKEN_ALERT_SESSIONS)
                .map(|snapshot| {
                    format!(
                        "{}:{}:{:.4}",
                        snapshot.session_id, snapshot.used_tokens, snapshot.usage_ratio
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            audit::append_event(
                &paths,
                "watcher",
                "alert",
                &format!(
                    "high-token usage threshold={} sessions={} top={}",
                    high_token_threshold,
                    high_token_sessions.len(),
                    preview
                ),
            )?;
        }
    }

    let triggers = evaluate(&cfg, &state, &usage);
    let trigger_names = triggers
        .iter()
        .map(|t| match t {
            TriggerKind::Archive => "archive".to_string(),
            TriggerKind::Compaction => "compaction".to_string(),
        })
        .collect::<Vec<_>>();

    let mut archive_out = None;
    let mut compaction_result = None;
    let mut distill_out = None;
    let mut continuity_out = None;
    let mut archive_retention_result = None;
    let compaction_cooldown_ready = is_cooldown_ready(
        state.last_compaction_trigger_epoch_secs,
        usage.captured_at_epoch_secs,
        cfg.watcher.cooldown_secs,
    );

    let mut compaction_targets = Vec::<SessionUsageSnapshot>::new();
    let mut compaction_notes = Vec::<String>::new();

    if let Some(note) = usage_batch_note {
        compaction_notes.push(note);
    }

    if usage.provider == "openclaw" {
        if let Some(batch) = &usage_batch {
            compaction_targets = batch
                .sessions
                .iter()
                .filter(|s| {
                    is_compaction_channel_session(&s.session_id)
                        && s.usage_ratio >= cfg.thresholds.compaction_ratio
                })
                .cloned()
                .collect();
        } else if usage.usage_ratio >= cfg.thresholds.compaction_ratio
            && is_compaction_channel_session(&usage.session_id)
        {
            compaction_targets.push(usage.clone());
        }
    } else if usage.usage_ratio >= cfg.thresholds.compaction_ratio
        && is_compaction_channel_session(&usage.session_id)
    {
        compaction_targets.push(usage.clone());
    }

    let mut compaction_source_map = BTreeMap::new();
    if !compaction_targets.is_empty() {
        match load_session_source_map(&paths.openclaw_sessions_dir) {
            Ok(map) => compaction_source_map = map,
            Err(err) => compaction_notes.push(format!("source_map failed: {err:#}")),
        }
    }

    if !triggers.is_empty() {
        audit::append_event(
            &paths,
            "watcher",
            "triggered",
            &format!(
                "usage_ratio={:.4}, triggers={:?}",
                usage.usage_ratio, trigger_names
            ),
        )?;
    }

    if inbound_watch.detected_files > 0 || inbound_watch.failed_events > 0 {
        audit::append_event(
            &paths,
            "inbound_watch",
            if inbound_watch.failed_events == 0 {
                "ok"
            } else {
                "degraded"
            },
            &format!(
                "detected={} triggered={} failed={} watched_paths={}",
                inbound_watch.detected_files,
                inbound_watch.triggered_events,
                inbound_watch.failed_events,
                inbound_watch.watched_paths.join(",")
            ),
        )?;
    }

    if let Some(archive) = run_archive_if_needed(&paths, &triggers)? {
        state.last_archive_trigger_epoch_secs = Some(usage.captured_at_epoch_secs);
        audit::append_event(
            &paths,
            "archive",
            if archive.record.indexed {
                "ok"
            } else {
                "degraded"
            },
            &format!(
                "archive={} indexed={} deduped={}",
                archive.record.archive_path, archive.record.indexed, archive.deduped
            ),
        )?;
        archive_out = Some(archive);
    }

    if !compaction_targets.is_empty() && !compaction_cooldown_ready {
        let skip_note = format!(
            "skipped reason=cooldown targets={} cooldown_secs={}",
            compaction_targets.len(),
            cfg.watcher.cooldown_secs
        );
        audit::append_event(&paths, "compaction", "skipped", &skip_note)?;
        compaction_result = Some(skip_note);
    } else if !compaction_targets.is_empty() {
        state.last_compaction_trigger_epoch_secs = Some(usage.captured_at_epoch_secs);
        let mut outcomes = Vec::new();
        let mut failed = 0usize;
        let mut succeeded = 0usize;

        for note in &compaction_notes {
            outcomes.push(format!("note={note}"));
        }

        for target in &compaction_targets {
            let Some(source_path) = compaction_source_map.get(&target.session_id) else {
                failed += 1;
                outcomes.push(format!(
                    "failed key={} ratio={:.4} used={} max={} reason=archive-source-not-found",
                    target.session_id, target.usage_ratio, target.used_tokens, target.max_tokens
                ));
                continue;
            };

            let archived = match archive_and_index(&paths, source_path, "history") {
                Ok(out) => out,
                Err(err) => {
                    failed += 1;
                    outcomes.push(format!(
                        "failed key={} ratio={:.4} used={} max={} reason=archive-failed error={err:#}",
                        target.session_id, target.usage_ratio, target.used_tokens, target.max_tokens
                    ));
                    continue;
                }
            };

            audit::append_event(
                &paths,
                "archive",
                if archived.record.indexed {
                    "ok"
                } else {
                    "degraded"
                },
                &format!(
                    "scope=pre-compaction key={} source={} archive={} indexed={} deduped={}",
                    target.session_id,
                    archived.record.source_path,
                    archived.record.archive_path,
                    archived.record.indexed,
                    archived.deduped
                ),
            )?;

            if !archived.record.indexed {
                failed += 1;
                outcomes.push(format!(
                    "failed key={} ratio={:.4} used={} max={} reason=index-failed archive={}",
                    target.session_id,
                    target.usage_ratio,
                    target.used_tokens,
                    target.max_tokens,
                    archived.record.archive_path
                ));
                continue;
            }

            let mapped = match channel_archive_map::upsert(
                &paths,
                &target.session_id,
                &archived.record.source_path,
                &archived.record.archive_path,
            ) {
                Ok(record) => record,
                Err(err) => {
                    failed += 1;
                    outcomes.push(format!(
                        "failed key={} ratio={:.4} used={} max={} reason=channel-archive-map-failed archive={} error={err:#}",
                        target.session_id,
                        target.usage_ratio,
                        target.used_tokens,
                        target.max_tokens,
                        archived.record.archive_path
                    ));
                    continue;
                }
            };

            let line = match gateway::run_sessions_compact(&target.session_id) {
                Ok(summary) => {
                    succeeded += 1;
                    audit::append_event(
                        &paths,
                        "compaction",
                        "ok",
                        &format!(
                            "key={} archived={} result={}",
                            target.session_id, mapped.archive_path, summary
                        ),
                    )?;
                    format!(
                        "ok key={} ratio={:.4} used={} max={} archived={} {}",
                        target.session_id,
                        target.usage_ratio,
                        target.used_tokens,
                        target.max_tokens,
                        mapped.archive_path,
                        summary
                    )
                }
                Err(err) => {
                    failed += 1;
                    audit::append_event(
                        &paths,
                        "compaction",
                        "degraded",
                        &format!(
                            "key={} archived={} error={err:#}",
                            target.session_id, mapped.archive_path
                        ),
                    )?;
                    format!(
                        "failed key={} ratio={:.4} used={} max={} archived={} error={err:#}",
                        target.session_id,
                        target.usage_ratio,
                        target.used_tokens,
                        target.max_tokens,
                        mapped.archive_path
                    )
                }
            };
            outcomes.push(line);
        }

        let compact_result = format!(
            "targets={} succeeded={} failed={} {}",
            compaction_targets.len(),
            succeeded,
            failed,
            outcomes.join(" | ")
        );

        let status = if failed > 0 { "degraded" } else { "ok" };

        audit::append_event(&paths, "compaction", status, &compact_result)?;
        compaction_result = Some(compact_result);
    } else if !compaction_notes.is_empty() {
        audit::append_event(
            &paths,
            "compaction",
            "degraded",
            &format!("skipped reason=no-targets {}", compaction_notes.join(" | ")),
        )?;
        compaction_result = Some(format!(
            "skipped reason=no-targets {}",
            compaction_notes.join(" | ")
        ));
    }

    let mut distill_notes = Vec::<String>::new();
    let mut distill_candidates = Vec::<crate::moon::archive::ArchiveRecord>::new();
    let distill_chunk_trigger_bytes = distill_chunk_bytes() as u64;

    let distill_trigger_mode = DistillTriggerMode::from_config_mode(&cfg.distill.mode);
    if matches!(distill_trigger_mode, DistillTriggerMode::Idle) {
        if !compaction_targets.is_empty() {
            distill_notes.push("skipped reason=compaction-active".to_string());
        } else if !is_cooldown_ready(
            state.last_distill_trigger_epoch_secs,
            usage.captured_at_epoch_secs,
            cfg.watcher.cooldown_secs,
        ) {
            distill_notes.push(format!(
                "skipped reason=cooldown cooldown_secs={}",
                cfg.watcher.cooldown_secs
            ));
        } else {
            match read_ledger_records(&paths) {
                Ok(mut ledger) => {
                    if ledger.is_empty() {
                        distill_notes.push("skipped reason=no-archives".to_string());
                    } else {
                        let latest_archive_epoch = ledger
                            .iter()
                            .map(|r| r.created_at_epoch_secs)
                            .max()
                            .unwrap_or(0);
                        let idle_for = usage
                            .captured_at_epoch_secs
                            .saturating_sub(latest_archive_epoch);
                        if idle_for < cfg.distill.idle_secs {
                            distill_notes.push(format!(
                                "skipped reason=not-idle idle_for_secs={} idle_required_secs={}",
                                idle_for, cfg.distill.idle_secs
                            ));
                        } else {
                            ledger.sort_by_key(|r| r.created_at_epoch_secs);
                            let pending = ledger
                                .into_iter()
                                .filter(|record| {
                                    record.indexed
                                        && !state
                                            .distilled_archives
                                            .contains_key(&record.archive_path)
                                        && Path::new(&record.archive_path).exists()
                                })
                                .collect::<Vec<_>>();

                            if pending.is_empty() {
                                distill_notes
                                    .push("skipped reason=no-undistilled-archives".to_string());
                            } else if let Some(first_pending) = pending.first() {
                                let day_key =
                                    day_key_for_epoch(first_pending.created_at_epoch_secs);
                                for record in pending {
                                    if day_key_for_epoch(record.created_at_epoch_secs) != day_key {
                                        continue;
                                    }
                                    distill_candidates.push(record);
                                    if distill_candidates.len()
                                        >= cfg.distill.max_per_cycle as usize
                                    {
                                        break;
                                    }
                                }
                                distill_notes.push(format!(
                                    "selected_day={} selected={} chunk_trigger_bytes={} oversized_archives=chunked",
                                    day_key,
                                    distill_candidates.len(),
                                    distill_chunk_trigger_bytes
                                ));
                            } else {
                                distill_notes
                                    .push("skipped reason=no-undistilled-archives".to_string());
                            }
                        }
                    }
                }
                Err(err) => {
                    warn::emit(
                        "LEDGER_READ_FAILED",
                        "distill-selection",
                        "read-ledger",
                        "na",
                        "na",
                        "na",
                        "retry-next-cycle",
                        "ledger-read-failed",
                        &format!("{err:#}"),
                    );
                    distill_notes.push(format!("skipped reason=ledger-read-failed error={err:#}"))
                }
            }
        }
    } else {
        distill_notes.push("skipped reason=manual-mode".to_string());
    }

    if !distill_candidates.is_empty() {
        if !distill_notes.is_empty() {
            let selection_status = if distill_notes.iter().any(|note| {
                note.contains("archive-too-large") || note.contains("archive-stat-failed")
            }) {
                "degraded"
            } else {
                "ok"
            };
            audit::append_event(
                &paths,
                "distill",
                selection_status,
                &format!("selection {}", distill_notes.join(" | ")),
            )?;
        }

        for record in distill_candidates {
            let archive_path = record.archive_path.clone();
            let archive_size = match archive_file_size(&archive_path) {
                Ok(bytes) => bytes,
                Err(err) => {
                    audit::append_event(
                        &paths,
                        "distill",
                        "degraded",
                        &format!(
                            "mode=idle archive={} source={} session={} reason=archive-stat-failed error={err:#}",
                            record.archive_path, record.source_path, record.session_id
                        ),
                    )?;
                    continue;
                }
            };
            if archive_size > distill_chunk_trigger_bytes {
                let chunked_input = DistillInput {
                    session_id: record.session_id.clone(),
                    archive_path: archive_path.clone(),
                    archive_text: String::new(),
                    archive_epoch_secs: Some(record.created_at_epoch_secs),
                };
                match run_chunked_archive_distillation(&paths, &chunked_input) {
                    Ok(chunked) => {
                        let status = if chunked.truncated { "degraded" } else { "ok" };
                        audit::append_event(
                            &paths,
                            "distill",
                            status,
                            &format!(
                                "mode=idle-chunked archive={} source={} session={} bytes={} chunk_trigger_bytes={} chunk_count={} chunk_target_bytes={} truncated={}",
                                record.archive_path,
                                record.source_path,
                                record.session_id,
                                archive_size,
                                distill_chunk_trigger_bytes,
                                chunked.chunk_count,
                                chunked.chunk_target_bytes,
                                chunked.truncated
                            ),
                        )?;

                        let distill = DistillOutput {
                            provider: chunked.provider,
                            summary: chunked.summary,
                            summary_path: chunked.summary_path,
                            audit_log_path: chunked.audit_log_path,
                            created_at_epoch_secs: chunked.created_at_epoch_secs,
                        };

                        state.last_distill_trigger_epoch_secs = Some(usage.captured_at_epoch_secs);
                        state
                            .distilled_archives
                            .insert(archive_path.clone(), usage.captured_at_epoch_secs);

                        match build_continuity(
                            &paths,
                            &record.session_id,
                            &record.archive_path,
                            &distill.summary_path,
                            extract_key_decisions(&distill.summary),
                        ) {
                            Ok(outcome) => {
                                audit::append_event(
                                    &paths,
                                    "continuity",
                                    if outcome.rollover_ok {
                                        "ok"
                                    } else {
                                        "degraded"
                                    },
                                    &format!(
                                        "archive={} session={} map={} target={} rollover_ok={}",
                                        record.archive_path,
                                        record.session_id,
                                        outcome.map_path,
                                        outcome.target_session_id,
                                        outcome.rollover_ok
                                    ),
                                )?;
                                continuity_out = Some(outcome);
                            }
                            Err(err) => {
                                warn::emit(
                                    "CONTINUITY_FAILED",
                                    "continuity",
                                    "build-continuity",
                                    &record.session_id,
                                    &record.archive_path,
                                    &record.source_path,
                                    "retry-next-cycle",
                                    "continuity-build-failed",
                                    &format!("{err:#}"),
                                );
                                audit::append_event(
                                    &paths,
                                    "continuity",
                                    "degraded",
                                    &format!(
                                        "archive={} session={} error={err:#}",
                                        record.archive_path, record.session_id
                                    ),
                                )?;
                            }
                        }
                        distill_out = Some(distill);
                    }
                    Err(err) => {
                        warn::emit(
                            "DISTILL_CHUNKED_FAILED",
                            "distill",
                            "chunked-distill",
                            &record.session_id,
                            &record.archive_path,
                            &record.source_path,
                            "retry-next-cycle",
                            "chunked-distillation-failed",
                            &format!("{err:#}"),
                        );
                        audit::append_event(
                            &paths,
                            "distill",
                            "degraded",
                            &format!(
                                "mode=idle-chunked archive={} source={} session={} bytes={} chunk_trigger_bytes={} error={err:#}",
                                record.archive_path,
                                record.source_path,
                                record.session_id,
                                archive_size,
                                distill_chunk_trigger_bytes
                            ),
                        )?;
                    }
                }
                continue;
            }

            let archive_text = match load_archive_excerpt(&archive_path) {
                Ok(text) => text,
                Err(err) => {
                    audit::append_event(
                        &paths,
                        "distill",
                        "degraded",
                        &format!(
                            "mode=idle archive={} source={} session={} reason=archive-read-failed error={err:#}",
                            record.archive_path, record.source_path, record.session_id
                        ),
                    )?;
                    continue;
                }
            };

            let input = DistillInput {
                session_id: record.session_id.clone(),
                archive_path: archive_path.clone(),
                archive_text,
                archive_epoch_secs: Some(record.created_at_epoch_secs),
            };

            match run_distillation(&paths, &input) {
                Ok(distill) => {
                    state.last_distill_trigger_epoch_secs = Some(usage.captured_at_epoch_secs);
                    state
                        .distilled_archives
                        .insert(archive_path.clone(), usage.captured_at_epoch_secs);
                    audit::append_event(
                        &paths,
                        "distill",
                        "ok",
                        &format!(
                            "mode=idle archive={} source={} session={} bytes={}",
                            record.archive_path,
                            record.source_path,
                            record.session_id,
                            archive_size
                        ),
                    )?;

                    match build_continuity(
                        &paths,
                        &record.session_id,
                        &record.archive_path,
                        &distill.summary_path,
                        extract_key_decisions(&distill.summary),
                    ) {
                        Ok(outcome) => {
                            audit::append_event(
                                &paths,
                                "continuity",
                                if outcome.rollover_ok {
                                    "ok"
                                } else {
                                    "degraded"
                                },
                                &format!(
                                    "archive={} session={} map={} target={} rollover_ok={}",
                                    record.archive_path,
                                    record.session_id,
                                    outcome.map_path,
                                    outcome.target_session_id,
                                    outcome.rollover_ok
                                ),
                            )?;
                            continuity_out = Some(outcome);
                        }
                        Err(err) => {
                            warn::emit(
                                "CONTINUITY_FAILED",
                                "continuity",
                                "build-continuity",
                                &record.session_id,
                                &record.archive_path,
                                &record.source_path,
                                "retry-next-cycle",
                                "continuity-build-failed",
                                &format!("{err:#}"),
                            );
                            audit::append_event(
                                &paths,
                                "continuity",
                                "degraded",
                                &format!(
                                    "archive={} session={} error={err:#}",
                                    record.archive_path, record.session_id
                                ),
                            )?;
                        }
                    }
                    distill_out = Some(distill);
                }
                Err(err) => {
                    warn::emit(
                        "DISTILL_FAILED",
                        "distill",
                        "run-distill",
                        &record.session_id,
                        &record.archive_path,
                        &record.source_path,
                        "retry-next-cycle",
                        "distillation-failed",
                        &format!("{err:#}"),
                    );
                    audit::append_event(
                        &paths,
                        "distill",
                        "degraded",
                        &format!(
                            "mode=idle archive={} source={} session={} bytes={} error={err:#}",
                            record.archive_path,
                            record.source_path,
                            record.session_id,
                            archive_size
                        ),
                    )?;
                }
            }
        }
    } else if !distill_notes.is_empty() {
        audit::append_event(&paths, "distill", "skipped", &distill_notes.join(" | "))?;
    }

    if let Some(summary) = cleanup_expired_distilled_archives(
        &paths,
        &mut state,
        usage.captured_at_epoch_secs,
        &cfg.retention,
    )? {
        let status = if summary.contains("failed=") && !summary.contains("failed=0") {
            "degraded"
        } else {
            "ok"
        };
        audit::append_event(&paths, "archive-retention", status, &summary)?;
        archive_retention_result = Some(summary);
    }

    let file = save(&paths, &state)?;

    Ok(WatchCycleOutcome {
        state_file: file.display().to_string(),
        heartbeat_epoch_secs: state.last_heartbeat_epoch_secs,
        poll_interval_secs: cfg.watcher.poll_interval_secs,
        archive_threshold: cfg.thresholds.archive_ratio,
        archive_trigger_enabled: cfg.thresholds.archive_ratio_trigger_enabled,
        compaction_threshold: cfg.thresholds.compaction_ratio,
        distill_mode: cfg.distill.mode.clone(),
        distill_idle_secs: cfg.distill.idle_secs,
        distill_max_per_cycle: cfg.distill.max_per_cycle,
        distill_archive_grace_hours: cfg.distill.archive_grace_hours,
        retention_active_days: cfg.retention.active_days,
        retention_warm_days: cfg.retention.warm_days,
        retention_cold_days: cfg.retention.cold_days,
        usage,
        triggers: trigger_names,
        inbound_watch,
        archive: archive_out,
        compaction_result,
        distill: distill_out,
        continuity: continuity_out,
        archive_retention_result,
    })
}

pub fn run_daemon() -> Result<()> {
    let _daemon_lock = acquire_daemon_lock()?;
    let mut consecutive_failures = 0u32;
    loop {
        match run_once() {
            Ok(cycle) => {
                consecutive_failures = 0;
                let sleep_for = Duration::from_secs(cycle.poll_interval_secs.max(1));
                thread::sleep(sleep_for);
            }
            Err(err) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                let base_secs = load_config()
                    .map(|cfg| cfg.watcher.poll_interval_secs.max(1))
                    .unwrap_or(30);
                let exponent = consecutive_failures.saturating_sub(1).min(4);
                let multiplier = 1u64 << exponent;
                let retry_in_secs = base_secs.saturating_mul(multiplier).min(300);

                if let Ok(paths) = resolve_paths() {
                    let _ = audit::append_event(
                        &paths,
                        "watcher",
                        "degraded",
                        &format!(
                            "daemon cycle failed retry_in_secs={} consecutive_failures={} error={err:#}",
                            retry_in_secs, consecutive_failures
                        ),
                    );
                }

                eprintln!(
                    "moon watcher cycle failed; retrying in {}s: {err:#}",
                    retry_in_secs
                );
                thread::sleep(Duration::from_secs(retry_in_secs));
            }
        }
    }
}
