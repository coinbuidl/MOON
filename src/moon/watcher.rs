use crate::moon::archive::{
    ArchivePipelineOutcome, archive_and_index, read_ledger_records, remove_ledger_records,
};
use crate::moon::audit;
use crate::moon::channel_archive_map;
use crate::moon::config::load_config;
use crate::moon::continuity::ContinuityOutcome;
use crate::moon::distill::{DistillInput, DistillOutput, run_distillation};
use crate::moon::inbound_watch::{self, InboundWatchOutcome};
use crate::moon::paths::resolve_paths;
use crate::moon::qmd;
use crate::moon::session_usage::{SessionUsageSnapshot, collect_openclaw_usages, collect_usage};
use crate::moon::snapshot::latest_session_file;
use crate::moon::state::{load, save};
use crate::moon::thresholds::{TriggerKind, evaluate};
use crate::openclaw::gateway;
use anyhow::{Context, Result};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

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
    grace_hours: u64,
) -> Result<Option<String>> {
    let grace_secs = grace_hours.saturating_mul(3600);
    if grace_secs == 0 {
        return Ok(Some("skipped reason=grace-disabled".to_string()));
    }

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
        if now_epoch_secs.saturating_sub(distilled_at) < grace_secs {
            continue;
        }

        if Path::new(&archive_path).exists() {
            match fs::remove_file(&archive_path) {
                Ok(_) => {
                    removed_files += 1;
                    purge_paths.insert(archive_path.clone());
                    state.distilled_archives.remove(&archive_path);
                }
                Err(_) => {
                    failed += 1;
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
        "grace_hours={} removed={} missing={} failed={} map_removed={} ledger_removed={} qmd_updated={}",
        grace_hours, removed_files, missing_files, failed, map_removed, ledger_removed, qmd_updated
    )))
}

pub fn run_once() -> Result<WatchCycleOutcome> {
    let paths = resolve_paths()?;
    let cfg = load_config()?;
    let mut state = load(&paths)?;
    let inbound_watch = inbound_watch::process(&paths, &cfg, &mut state)?;

    let usage = collect_usage(&paths)?;
    state.last_heartbeat_epoch_secs = usage.captured_at_epoch_secs;
    state.last_session_id = Some(usage.session_id.clone());
    state.last_usage_ratio = Some(usage.usage_ratio);
    state.last_provider = Some(usage.provider.clone());

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
    let continuity_out = None;
    let mut archive_retention_result = None;
    let compaction_cooldown_ready = is_cooldown_ready(
        state.last_compaction_trigger_epoch_secs,
        usage.captured_at_epoch_secs,
        cfg.watcher.cooldown_secs,
    );

    let mut compaction_targets = Vec::<SessionUsageSnapshot>::new();
    let mut compaction_notes = Vec::<String>::new();

    if usage.provider == "openclaw" {
        match collect_openclaw_usages() {
            Ok(snapshots) => {
                compaction_targets = snapshots
                    .into_iter()
                    .filter(|s| {
                        is_compaction_channel_session(&s.session_id)
                            && s.usage_ratio >= cfg.thresholds.compaction_ratio
                    })
                    .collect();
            }
            Err(err) => {
                compaction_notes.push(format!("scan failed: {err:#}"));
                if usage.usage_ratio >= cfg.thresholds.compaction_ratio
                    && is_compaction_channel_session(&usage.session_id)
                {
                    compaction_targets.push(usage.clone());
                }
            }
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

    if cfg.distill.mode == "idle" {
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
                            for record in ledger {
                                if !record.indexed {
                                    continue;
                                }
                                if state.distilled_archives.contains_key(&record.archive_path) {
                                    continue;
                                }
                                if !Path::new(&record.archive_path).exists() {
                                    continue;
                                }
                                distill_candidates.push(record);
                                if distill_candidates.len() >= cfg.distill.max_per_cycle as usize {
                                    break;
                                }
                            }

                            if distill_candidates.is_empty() {
                                distill_notes
                                    .push("skipped reason=no-undistilled-archives".to_string());
                            }
                        }
                    }
                }
                Err(err) => {
                    distill_notes.push(format!("skipped reason=ledger-read-failed error={err:#}"))
                }
            }
        }
    } else {
        distill_notes.push("skipped reason=manual-mode".to_string());
    }

    if !distill_candidates.is_empty() {
        for record in distill_candidates {
            let archive_path = record.archive_path.clone();
            let archive_text = std::fs::read_to_string(&archive_path).unwrap_or_else(|_| {
                std::fs::read(&archive_path)
                    .ok()
                    .map(|b| String::from_utf8_lossy(&b).to_string())
                    .unwrap_or_default()
            });

            let input = DistillInput {
                session_id: record.session_id.clone(),
                archive_path: archive_path.clone(),
                archive_text,
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
                            "mode=idle archive={} source={} session={}",
                            record.archive_path, record.source_path, record.session_id
                        ),
                    )?;
                    distill_out = Some(distill);
                }
                Err(err) => {
                    audit::append_event(
                        &paths,
                        "distill",
                        "degraded",
                        &format!(
                            "mode=idle archive={} source={} session={} error={err:#}",
                            record.archive_path, record.source_path, record.session_id
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
        cfg.distill.archive_grace_hours,
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
    loop {
        let cycle = run_once()?;
        let sleep_for = Duration::from_secs(cycle.poll_interval_secs);
        thread::sleep(sleep_for);
    }
}
