use crate::moon::archive::{
    ArchivePipelineOutcome, archive_and_index, projection_path_for_archive, read_ledger_records,
    remove_ledger_records,
};
use crate::moon::audit;
use crate::moon::channel_archive_map;
use crate::moon::config::{
    MoonContextCompactionAuthority, MoonContextConfig, MoonRetentionConfig, load_config,
};
use crate::moon::continuity::{ContinuityOutcome, build_continuity};
use crate::moon::distill::{
    DistillInput, DistillOutput, archive_file_size, distill_chunk_bytes, load_archive_excerpt,
    run_chunked_archive_distillation, run_distillation,
};
use crate::moon::embed::{self, EmbedCaller, EmbedRunError, EmbedRunOptions};
use crate::moon::inbound_watch::{self, InboundWatchOutcome};
use crate::moon::paths::resolve_paths;
use crate::moon::qmd;
use crate::moon::session_usage::{
    SessionUsageSnapshot, collect_openclaw_usage_batch, collect_usage,
};
use crate::moon::snapshot::latest_session_file;
use crate::moon::state::{load, save};
use crate::moon::thresholds::{TriggerKind, evaluate, evaluate_context_compaction_candidate};
use crate::moon::warn::{self, WarnEvent};
use crate::openclaw::gateway;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use chrono::{Local, TimeZone, Utc};
use chrono_tz::Tz;
use fs2::FileExt;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

const DEFAULT_HIGH_TOKEN_ALERT_THRESHOLD: u64 = 1_000_000;
const MAX_HIGH_TOKEN_ALERT_SESSIONS: usize = 5;
const BUILD_UUID: &str = env!("BUILD_UUID");

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DaemonLockPayload {
    pid: u32,
    started_at_epoch_secs: u64,
    build_uuid: String,
    moon_home: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DistillTriggerMode {
    Manual,
    Idle,
    Daily,
}

impl DistillTriggerMode {
    fn from_config_mode(raw: &str) -> Self {
        // Reserved for future trigger extensions (for example archive_event).
        if raw.eq_ignore_ascii_case("idle") {
            Self::Idle
        } else if raw.eq_ignore_ascii_case("daily") {
            Self::Daily
        } else {
            Self::Manual
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WatchRunOptions {
    pub force_distill_now: bool,
}

#[derive(Debug, Clone)]
pub struct WatchCycleOutcome {
    pub state_file: String,
    pub heartbeat_epoch_secs: u64,
    pub poll_interval_secs: u64,
    pub trigger_threshold: f64,
    pub compaction_authority: String,
    pub compaction_emergency_ratio: Option<f64>,
    pub compaction_recover_ratio: Option<f64>,
    pub distill_mode: String,
    pub distill_idle_secs: u64,
    pub distill_max_per_cycle: u64,
    pub embed_mode: String,
    pub embed_idle_secs: u64,
    pub embed_max_docs_per_cycle: u64,
    pub retention_active_days: u64,
    pub retention_warm_days: u64,
    pub retention_cold_days: u64,
    pub usage: SessionUsageSnapshot,
    pub triggers: Vec<String>,
    pub inbound_watch: InboundWatchOutcome,
    pub archive: Option<ArchivePipelineOutcome>,
    pub compaction_result: Option<String>,
    pub distill: Option<DistillOutput>,
    pub embed_result: Option<String>,
    pub continuity: Option<ContinuityOutcome>,
    pub archive_retention_result: Option<String>,
}

type DistillCandidate = (crate::moon::archive::ArchiveRecord, String);
type DistillSelection = (Vec<DistillCandidate>, Vec<String>);

fn residential_tz_name(cfg: &crate::moon::config::MoonConfig) -> String {
    let name = cfg.distill.residential_timezone.trim();
    if name.is_empty() {
        "UTC".to_string()
    } else {
        name.to_string()
    }
}

fn parse_residential_tz(cfg: &crate::moon::config::MoonConfig) -> Tz {
    residential_tz_name(cfg)
        .parse::<Tz>()
        .unwrap_or(chrono_tz::UTC)
}

fn day_key_for_epoch_in_timezone(epoch_secs: u64, tz: Tz) -> String {
    let dt = tz
        .timestamp_opt(epoch_secs as i64, 0)
        .single()
        .unwrap_or_else(|| tz.from_utc_datetime(&Utc::now().naive_utc()));
    dt.format("%Y-%m-%d").to_string()
}

fn run_archive_if_needed(
    paths: &crate::moon::paths::MoonPaths,
    trigger_set: &[TriggerKind],
    compaction_targets_present: bool,
) -> Result<Option<ArchivePipelineOutcome>> {
    // Compaction path already archives each target source before compacting.
    if compaction_targets_present {
        return Ok(None);
    }

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

fn unified_layer1_last_trigger_epoch(state: &crate::moon::state::MoonState) -> Option<u64> {
    match (
        state.last_archive_trigger_epoch_secs,
        state.last_compaction_trigger_epoch_secs,
    ) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(v), None) | (None, Some(v)) => Some(v),
        (None, None) => None,
    }
}

fn compaction_authority_name(policy: Option<&MoonContextConfig>) -> String {
    match policy.map(|p| &p.compaction_authority) {
        Some(MoonContextCompactionAuthority::Moon) => "moon".to_string(),
        Some(MoonContextCompactionAuthority::Openclaw) => "openclaw".to_string(),
        None => "legacy-moon".to_string(),
    }
}

fn effective_compaction_start_ratio(
    cfg: &crate::moon::config::MoonConfig,
    policy: Option<&MoonContextConfig>,
) -> f64 {
    if let Some(policy) = policy
        && matches!(
            policy.compaction_authority,
            MoonContextCompactionAuthority::Moon
        )
    {
        return policy.compaction_start_ratio;
    }
    cfg.thresholds.trigger_ratio
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

fn resolve_distill_source_path(record: &crate::moon::archive::ArchiveRecord) -> Option<PathBuf> {
    if let Some(path) = record.projection_path.as_deref() {
        let trimmed = path.trim();
        if !trimmed.is_empty() {
            let projection = PathBuf::from(trimmed);
            if projection.exists() {
                return Some(projection);
            }
        }
    }

    let fallback = projection_path_for_archive(&record.archive_path);
    if fallback.exists() {
        return Some(fallback);
    }

    let legacy = Path::new(&record.archive_path).with_extension("md");
    if legacy.exists() {
        return Some(legacy);
    }

    None
}

fn is_distillable_archive_record(record: &crate::moon::archive::ArchiveRecord) -> bool {
    let source_path = Path::new(&record.source_path);
    let archive_path = Path::new(&record.archive_path);

    let source_file = source_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let archive_file = archive_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let source_ext = source_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let archive_ext = archive_path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    // `sessions.json` snapshots and lock artifacts are metadata/noise, not conversation history.
    if source_file == "sessions.json" {
        return false;
    }
    if source_ext == "lock"
        || archive_ext == "lock"
        || source_file.ends_with(".lock")
        || archive_file.ends_with(".lock")
    {
        return false;
    }
    if archive_ext == "json" && archive_file.starts_with("sessions-") {
        return false;
    }

    true
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
            warn::emit(WarnEvent {
                code: "LEDGER_READ_FAILED",
                stage: "archive-retention",
                action: "read-ledger",
                session: "na",
                archive: "na",
                source: "na",
                retry: "retry-next-cycle",
                reason: "ledger-read-failed",
                err: &format!("{err:#}"),
            });
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
    let mut projection_removed = 0usize;
    let mut projection_missing = 0usize;
    let mut projection_failed = 0usize;

    let candidates = state
        .distilled_archives
        .iter()
        .map(|(k, v)| (k.clone(), *v))
        .collect::<Vec<_>>();

    for (archive_path, distilled_at) in candidates {
        let Some(created_at) = ledger_by_archive.get(&archive_path).copied() else {
            warn::emit(WarnEvent {
                code: "LEDGER_READ_FAILED",
                stage: "archive-retention",
                action: "lookup-ledger-record",
                session: "na",
                archive: &archive_path,
                source: "na",
                retry: "skip-current-archive",
                reason: "archive-path-missing-in-ledger",
                err: "missing-ledger-record",
            });
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
        let projection_path = projection_path_for_archive(&archive_path);
        let projection_path_display = projection_path.display().to_string();

        if Path::new(&archive_path).exists() {
            match fs::remove_file(&archive_path) {
                Ok(_) => {
                    removed_files += 1;
                    purge_paths.insert(archive_path.clone());
                    state.distilled_archives.remove(&archive_path);
                    match fs::remove_file(&projection_path) {
                        Ok(_) => projection_removed += 1,
                        Err(err) if err.kind() == ErrorKind::NotFound => {
                            projection_missing += 1;
                        }
                        Err(err) => {
                            projection_failed += 1;
                            warn::emit(WarnEvent {
                                code: "RETENTION_DELETE_FAILED",
                                stage: "archive-retention",
                                action: "delete-projection",
                                session: "na",
                                archive: &archive_path,
                                source: &projection_path_display,
                                retry: "retry-next-cycle",
                                reason: "remove-projection-file-failed",
                                err: &format!("{err:#}"),
                            });
                        }
                    }
                }
                Err(err) => {
                    failed += 1;
                    warn::emit(WarnEvent {
                        code: "RETENTION_DELETE_FAILED",
                        stage: "archive-retention",
                        action: "delete-archive",
                        session: "na",
                        archive: &archive_path,
                        source: "na",
                        retry: "retry-next-cycle",
                        reason: "remove-file-failed",
                        err: &format!("{err:#}"),
                    });
                }
            }
        } else {
            missing_files += 1;
            purge_paths.insert(archive_path.clone());
            state.distilled_archives.remove(&archive_path);
            match fs::remove_file(&projection_path) {
                Ok(_) => projection_removed += 1,
                Err(err) if err.kind() == ErrorKind::NotFound => {
                    projection_missing += 1;
                }
                Err(err) => {
                    projection_failed += 1;
                    warn::emit(WarnEvent {
                        code: "RETENTION_DELETE_FAILED",
                        stage: "archive-retention",
                        action: "delete-projection",
                        session: "na",
                        archive: &archive_path,
                        source: &projection_path_display,
                        retry: "retry-next-cycle",
                        reason: "remove-projection-file-failed",
                        err: &format!("{err:#}"),
                    });
                }
            }
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
        "retention_active_days={} retention_warm_days={} retention_cold_days={} active={} warm={} cold_candidates={} removed={} missing={} failed={} projection_removed={} projection_missing={} projection_failed={} map_removed={} ledger_removed={} qmd_updated={}",
        retention.active_days,
        retention.warm_days,
        retention.cold_days,
        active_count,
        warm_count,
        cold_candidates,
        removed_files,
        missing_files,
        failed,
        projection_removed,
        projection_missing,
        projection_failed,
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

fn select_pending_distill_candidates(
    paths: &crate::moon::paths::MoonPaths,
    state: &crate::moon::state::MoonState,
    max_per_cycle: u64,
    distill_chunk_trigger_bytes: u64,
) -> Result<DistillSelection> {
    let mut notes = Vec::new();
    let mut distill_candidates = Vec::<(crate::moon::archive::ArchiveRecord, String)>::new();

    let mut ledger = read_ledger_records(paths)?;
    if ledger.is_empty() {
        notes.push("skipped reason=no-archives".to_string());
        return Ok((distill_candidates, notes));
    }

    ledger.sort_by_key(|r| r.created_at_epoch_secs);
    let mut pending = Vec::new();
    let mut skipped_non_distillable = 0usize;
    for record in ledger {
        if !record.indexed
            || state.distilled_archives.contains_key(&record.archive_path)
            || !Path::new(&record.archive_path).exists()
        {
            continue;
        }

        if !is_distillable_archive_record(&record) {
            skipped_non_distillable = skipped_non_distillable.saturating_add(1);
            continue;
        }

        let Some(distill_source_path) = resolve_distill_source_path(&record) else {
            warn::emit(WarnEvent {
                code: "DISTILL_SOURCE_MISSING",
                stage: "distill-selection",
                action: "resolve-distill-source",
                session: &record.session_id,
                archive: &record.archive_path,
                source: &record.source_path,
                retry: "retry-next-cycle",
                reason: "projection-md-missing",
                err: "projection-md-not-found",
            });
            continue;
        };
        pending.push((record, distill_source_path.display().to_string()));
    }

    if pending.is_empty() {
        notes.push("skipped reason=no-undistilled-archives".to_string());
        if skipped_non_distillable > 0 {
            notes.push(format!(
                "skipped_non_distillable_archives={}",
                skipped_non_distillable
            ));
        }
        return Ok((distill_candidates, notes));
    }

    if let Some((first_pending, _)) = pending.first() {
        let day_key = day_key_for_epoch(first_pending.created_at_epoch_secs);
        for (record, distill_source_path) in pending {
            if day_key_for_epoch(record.created_at_epoch_secs) != day_key {
                continue;
            }
            distill_candidates.push((record, distill_source_path));
            if distill_candidates.len() >= max_per_cycle as usize {
                break;
            }
        }
        notes.push(format!(
            "selected_day={} selected={} chunk_trigger_bytes={} oversized_archives=chunked",
            day_key,
            distill_candidates.len(),
            distill_chunk_trigger_bytes
        ));
        if skipped_non_distillable > 0 {
            notes.push(format!(
                "skipped_non_distillable_archives={}",
                skipped_non_distillable
            ));
        }
    } else {
        notes.push("skipped reason=no-undistilled-archives".to_string());
    }

    Ok((distill_candidates, notes))
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

    let now = crate::moon::util::now_epoch_secs()?;

    match lock_file.try_lock_exclusive() {
        Ok(()) => {
            // We got the lock. Write the payload.
            let payload = DaemonLockPayload {
                pid: std::process::id(),
                started_at_epoch_secs: now,
                build_uuid: BUILD_UUID.to_string(),
                moon_home: paths.moon_home.display().to_string(),
            };
            lock_file.set_len(0)?;
            lock_file.write_all(format!("{}\n", serde_json::to_string(&payload)?).as_bytes())?;
            lock_file.flush()?;
        }
        Err(err) if err.kind() == ErrorKind::WouldBlock => {
            // Lock is held. Check if it's stale or mismatched.
            let raw = fs::read_to_string(&lock_path).ok();
            let payload: Option<DaemonLockPayload> =
                raw.and_then(|r| serde_json::from_str(&r).ok());

            if let Some(p) = payload {
                let pid_alive = crate::moon::util::pid_alive(p.pid);

                if !pid_alive || p.build_uuid != BUILD_UUID {
                    // Stale or mismatched. We should ideally auto-remediate if !pid_alive.
                    // But for now, just report the mismatch as per MIP.
                    if p.build_uuid != BUILD_UUID {
                        anyhow::bail!(
                            "moon watcher binary mismatch (running: {}, disk: {}). Please restart the daemon.",
                            p.build_uuid,
                            BUILD_UUID
                        );
                    }
                }
            }

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
    run_once_with_options(WatchRunOptions::default())
}

pub fn run_once_with_options(run_opts: WatchRunOptions) -> Result<WatchCycleOutcome> {
    let paths = resolve_paths()?;
    let cfg = load_config()?;
    let mut state = load(&paths)?;
    // Legacy field retained for backward-compatible state parsing; no longer used
    // for compaction trigger decisions.
    state.compaction_hysteresis_active.clear();
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

    let context_policy = cfg.context.as_ref();
    let effective_trigger_threshold = effective_compaction_start_ratio(&cfg, context_policy);
    let compaction_authority = compaction_authority_name(context_policy);

    let triggers = if let Some(policy) = context_policy {
        match policy.compaction_authority {
            MoonContextCompactionAuthority::Moon => {
                if usage.usage_ratio >= policy.compaction_start_ratio
                    && (is_cooldown_ready(
                        unified_layer1_last_trigger_epoch(&state),
                        usage.captured_at_epoch_secs,
                        cfg.watcher.cooldown_secs,
                    ) || usage.usage_ratio >= policy.compaction_emergency_ratio)
                {
                    vec![TriggerKind::Archive, TriggerKind::Compaction]
                } else {
                    Vec::new()
                }
            }
            MoonContextCompactionAuthority::Openclaw => Vec::new(),
        }
    } else {
        evaluate(&cfg, &state, &usage)
    };
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
    let mut embed_result: Option<String> = None;
    let mut continuity_out = None;
    let mut archive_retention_result = None;
    let compaction_cooldown_ready = is_cooldown_ready(
        unified_layer1_last_trigger_epoch(&state),
        usage.captured_at_epoch_secs,
        cfg.watcher.cooldown_secs,
    );

    let mut compaction_targets = Vec::<SessionUsageSnapshot>::new();
    let mut compaction_notes = Vec::<String>::new();
    let mut compaction_has_archivable_targets = false;
    let mut cooldown_gate_handled_during_selection = false;

    if let Some(note) = usage_batch_note {
        compaction_notes.push(note);
    }

    if let Some(policy) = context_policy {
        if matches!(
            policy.compaction_authority,
            MoonContextCompactionAuthority::Openclaw
        ) {
            compaction_result = Some("skipped reason=authority-openclaw".to_string());
        } else {
            cooldown_gate_handled_during_selection = true;
            let mut candidate_sessions = Vec::<SessionUsageSnapshot>::new();
            if usage.provider == "openclaw" {
                if let Some(batch) = &usage_batch {
                    candidate_sessions = batch
                        .sessions
                        .iter()
                        .filter(|s| is_compaction_channel_session(&s.session_id))
                        .cloned()
                        .collect();
                } else if is_compaction_channel_session(&usage.session_id) {
                    candidate_sessions.push(usage.clone());
                }
            } else if is_compaction_channel_session(&usage.session_id) {
                candidate_sessions.push(usage.clone());
            }

            let mut blocked_cooldown = 0usize;
            let mut bypassed_cooldown = 0usize;
            for candidate in candidate_sessions {
                let decision = evaluate_context_compaction_candidate(
                    candidate.usage_ratio,
                    policy.compaction_start_ratio,
                    policy.compaction_emergency_ratio,
                    compaction_cooldown_ready,
                );
                if decision.should_compact {
                    if decision.bypassed_cooldown {
                        bypassed_cooldown += 1;
                    }
                    compaction_targets.push(candidate);
                    continue;
                }
                if candidate.usage_ratio >= policy.compaction_start_ratio
                    && !compaction_cooldown_ready
                {
                    blocked_cooldown += 1;
                }
            }
            compaction_notes.push(format!(
                "policy=start_ratio={:.4} emergency_ratio={:.4}",
                policy.compaction_start_ratio, policy.compaction_emergency_ratio,
            ));
            if blocked_cooldown > 0 {
                compaction_notes.push(format!("cooldown_blocked={blocked_cooldown}"));
            }
            if bypassed_cooldown > 0 {
                compaction_notes.push(format!("cooldown_bypassed={bypassed_cooldown}"));
            }
        }
    } else if usage.provider == "openclaw" {
        if let Some(batch) = &usage_batch {
            compaction_targets = batch
                .sessions
                .iter()
                .filter(|s| {
                    is_compaction_channel_session(&s.session_id)
                        && s.usage_ratio >= cfg.thresholds.trigger_ratio
                })
                .cloned()
                .collect();
        } else if usage.usage_ratio >= cfg.thresholds.trigger_ratio
            && is_compaction_channel_session(&usage.session_id)
        {
            compaction_targets.push(usage.clone());
        }
    } else if usage.usage_ratio >= cfg.thresholds.trigger_ratio
        && is_compaction_channel_session(&usage.session_id)
    {
        compaction_targets.push(usage.clone());
    }

    let mut compaction_source_map = BTreeMap::new();
    if !compaction_targets.is_empty() {
        match load_session_source_map(&paths.openclaw_sessions_dir) {
            Ok(map) => {
                compaction_source_map = map;
                compaction_has_archivable_targets = compaction_targets
                    .iter()
                    .any(|target| compaction_source_map.contains_key(&target.session_id));
            }
            Err(err) => compaction_notes.push(format!("source_map failed: {err:#}")),
        }
    }

    if let Some(archive) =
        run_archive_if_needed(&paths, &triggers, compaction_has_archivable_targets)?
    {
        state.last_archive_trigger_epoch_secs = Some(usage.captured_at_epoch_secs);
        archive_out = Some(archive);
    }

    if !compaction_targets.is_empty()
        && !compaction_cooldown_ready
        && !cooldown_gate_handled_during_selection
    {
        let skip_note = format!(
            "skipped reason=cooldown targets={} cooldown_secs={}",
            compaction_targets.len(),
            cfg.watcher.cooldown_secs
        );
        compaction_result = Some(skip_note);
    } else if !compaction_targets.is_empty() {
        state.last_compaction_trigger_epoch_secs = Some(usage.captured_at_epoch_secs);
        state.last_archive_trigger_epoch_secs = Some(usage.captured_at_epoch_secs);
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
                    let index_note = match gateway::run_sessions_index_note(
                        &target.session_id,
                        &mapped.archive_path,
                        archived.record.projection_path.as_deref(),
                        &archived.record.source_path,
                        &archived.record.content_hash,
                        &archived.record.indexed_collection,
                    ) {
                        Ok(note) => note,
                        Err(err) => {
                            warn::emit(WarnEvent {
                                code: "INDEX_NOTE_FAILED",
                                stage: "compaction",
                                action: "write-index-note",
                                session: &target.session_id,
                                archive: &mapped.archive_path,
                                source: &archived.record.source_path,
                                retry: "retry-next-cycle",
                                reason: "chat-send-index-note-failed",
                                err: &format!("{err:#}"),
                            });
                            format!("index_note_failed error={err:#}")
                        }
                    };
                    format!(
                        "ok key={} ratio={:.4} used={} max={} archived={} {} {}",
                        target.session_id,
                        target.usage_ratio,
                        target.used_tokens,
                        target.max_tokens,
                        mapped.archive_path,
                        summary,
                        index_note
                    )
                }
                Err(err) => {
                    failed += 1;
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
    } else if compaction_result.is_none() && !compaction_notes.is_empty() {
        compaction_result = Some(format!(
            "skipped reason=no-targets {}",
            compaction_notes.join(" | ")
        ));
    }

    let mut distill_notes = Vec::<String>::new();
    let mut distill_candidates = Vec::<(crate::moon::archive::ArchiveRecord, String)>::new();
    let distill_chunk_trigger_bytes = distill_chunk_bytes() as u64;

    let distill_trigger_mode = DistillTriggerMode::from_config_mode(&cfg.distill.mode);
    let residential_tz = parse_residential_tz(&cfg);
    let current_day_key =
        day_key_for_epoch_in_timezone(usage.captured_at_epoch_secs, residential_tz);
    let last_distill_day_key = state
        .last_distill_trigger_epoch_secs
        .map(|epoch| day_key_for_epoch_in_timezone(epoch, residential_tz));

    if run_opts.force_distill_now {
        if !compaction_targets.is_empty() {
            distill_notes.push("skipped reason=compaction-active".to_string());
        } else {
            distill_notes.push("manual_trigger=true".to_string());
            match select_pending_distill_candidates(
                &paths,
                &state,
                cfg.distill.max_per_cycle,
                distill_chunk_trigger_bytes,
            ) {
                Ok((candidates, notes)) => {
                    distill_candidates = candidates;
                    distill_notes.extend(notes);
                }
                Err(err) => {
                    warn::emit(WarnEvent {
                        code: "LEDGER_READ_FAILED",
                        stage: "distill-selection",
                        action: "read-ledger",
                        session: "na",
                        archive: "na",
                        source: "na",
                        retry: "retry-next-cycle",
                        reason: "ledger-read-failed",
                        err: &format!("{err:#}"),
                    });
                    distill_notes.push(format!("skipped reason=ledger-read-failed error={err:#}"))
                }
            }
        }
    } else if matches!(distill_trigger_mode, DistillTriggerMode::Idle) {
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
                Ok(ledger) => {
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
                            match select_pending_distill_candidates(
                                &paths,
                                &state,
                                cfg.distill.max_per_cycle,
                                distill_chunk_trigger_bytes,
                            ) {
                                Ok((candidates, notes)) => {
                                    distill_candidates = candidates;
                                    distill_notes.extend(notes);
                                }
                                Err(err) => {
                                    warn::emit(WarnEvent {
                                        code: "LEDGER_READ_FAILED",
                                        stage: "distill-selection",
                                        action: "read-ledger",
                                        session: "na",
                                        archive: "na",
                                        source: "na",
                                        retry: "retry-next-cycle",
                                        reason: "ledger-read-failed",
                                        err: &format!("{err:#}"),
                                    });
                                    distill_notes.push(format!(
                                        "skipped reason=ledger-read-failed error={err:#}"
                                    ));
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    warn::emit(WarnEvent {
                        code: "LEDGER_READ_FAILED",
                        stage: "distill-selection",
                        action: "read-ledger",
                        session: "na",
                        archive: "na",
                        source: "na",
                        retry: "retry-next-cycle",
                        reason: "ledger-read-failed",
                        err: &format!("{err:#}"),
                    });
                    distill_notes.push(format!("skipped reason=ledger-read-failed error={err:#}"))
                }
            }
        }
    } else if matches!(distill_trigger_mode, DistillTriggerMode::Daily) {
        if !compaction_targets.is_empty() {
            distill_notes.push("skipped reason=compaction-active".to_string());
        } else if last_distill_day_key.as_deref() == Some(current_day_key.as_str()) {
            distill_notes.push(format!(
                "skipped reason=already-attempted-today day_key={} timezone={}",
                current_day_key,
                residential_tz_name(&cfg)
            ));
        } else {
            match read_ledger_records(&paths) {
                Ok(ledger) => {
                    if ledger.is_empty() {
                        // Count this as today's daily attempt to avoid repeated no-op cycles.
                        state.last_distill_trigger_epoch_secs = Some(usage.captured_at_epoch_secs);
                        distill_notes.push(format!(
                            "skipped reason=no-archives day_key={} timezone={}",
                            current_day_key,
                            residential_tz_name(&cfg)
                        ));
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
                                "skipped reason=not-idle day_key={} timezone={} idle_for_secs={} idle_required_secs={}",
                                current_day_key,
                                residential_tz_name(&cfg),
                                idle_for,
                                cfg.distill.idle_secs
                            ));
                        } else {
                            distill_notes.push(format!(
                                "daily_trigger day_key={} timezone={} idle_for_secs={} idle_required_secs={}",
                                current_day_key,
                                residential_tz_name(&cfg),
                                idle_for,
                                cfg.distill.idle_secs
                            ));
                            // Daily mode is once per residential day after idle guard.
                            state.last_distill_trigger_epoch_secs =
                                Some(usage.captured_at_epoch_secs);
                            match select_pending_distill_candidates(
                                &paths,
                                &state,
                                cfg.distill.max_per_cycle,
                                distill_chunk_trigger_bytes,
                            ) {
                                Ok((candidates, notes)) => {
                                    distill_candidates = candidates;
                                    distill_notes.extend(notes);
                                }
                                Err(err) => {
                                    warn::emit(WarnEvent {
                                        code: "LEDGER_READ_FAILED",
                                        stage: "distill-selection",
                                        action: "read-ledger",
                                        session: "na",
                                        archive: "na",
                                        source: "na",
                                        retry: "retry-next-cycle",
                                        reason: "ledger-read-failed",
                                        err: &format!("{err:#}"),
                                    });
                                    distill_notes.push(format!(
                                        "skipped reason=ledger-read-failed error={err:#}"
                                    ))
                                }
                            }
                        }
                    }
                }
                Err(err) => {
                    warn::emit(WarnEvent {
                        code: "LEDGER_READ_FAILED",
                        stage: "distill-selection",
                        action: "read-ledger",
                        session: "na",
                        archive: "na",
                        source: "na",
                        retry: "retry-next-cycle",
                        reason: "ledger-read-failed",
                        err: &format!("{err:#}"),
                    });
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

        for (record, distill_source_path) in distill_candidates {
            let archive_path = record.archive_path.clone();
            let archive_size = match archive_file_size(&distill_source_path) {
                Ok(bytes) => bytes,
                Err(err) => {
                    audit::append_event(
                        &paths,
                        "distill",
                        "degraded",
                        &format!(
                            "mode=idle archive={} distill_source={} source={} session={} reason=archive-stat-failed error={err:#}",
                            record.archive_path,
                            distill_source_path,
                            record.source_path,
                            record.session_id
                        ),
                    )?;
                    continue;
                }
            };
            if archive_size > distill_chunk_trigger_bytes {
                let chunked_input = DistillInput {
                    session_id: record.session_id.clone(),
                    archive_path: distill_source_path.clone(),
                    archive_text: String::new(),
                    archive_epoch_secs: Some(record.created_at_epoch_secs),
                };
                match run_chunked_archive_distillation(&paths, &chunked_input) {
                    Ok(chunked) => {
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
                                continuity_out = Some(outcome);
                            }
                            Err(err) => {
                                warn::emit(WarnEvent {
                                    code: "CONTINUITY_FAILED",
                                    stage: "continuity",
                                    action: "build-continuity",
                                    session: &record.session_id,
                                    archive: &record.archive_path,
                                    source: &record.source_path,
                                    retry: "retry-next-cycle",
                                    reason: "continuity-build-failed",
                                    err: &format!("{err:#}"),
                                });
                            }
                        }
                        distill_out = Some(distill);
                    }
                    Err(err) => {
                        warn::emit(WarnEvent {
                            code: "DISTILL_CHUNKED_FAILED",
                            stage: "distill",
                            action: "chunked-distill",
                            session: &record.session_id,
                            archive: &record.archive_path,
                            source: &record.source_path,
                            retry: "retry-next-cycle",
                            reason: "chunked-distillation-failed",
                            err: &format!("{err:#}"),
                        });
                        audit::append_event(
                            &paths,
                            "distill",
                            "degraded",
                            &format!(
                                "mode=idle-chunked archive={} distill_source={} source={} session={} bytes={} chunk_trigger_bytes={} error={err:#}",
                                record.archive_path,
                                distill_source_path,
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

            let archive_text = match load_archive_excerpt(&distill_source_path) {
                Ok(text) => text,
                Err(err) => {
                    audit::append_event(
                        &paths,
                        "distill",
                        "degraded",
                        &format!(
                            "mode=idle archive={} distill_source={} source={} session={} reason=archive-read-failed error={err:#}",
                            record.archive_path,
                            distill_source_path,
                            record.source_path,
                            record.session_id
                        ),
                    )?;
                    continue;
                }
            };

            let input = DistillInput {
                session_id: record.session_id.clone(),
                archive_path: distill_source_path.clone(),
                archive_text,
                archive_epoch_secs: Some(record.created_at_epoch_secs),
            };

            match run_distillation(&paths, &input) {
                Ok(distill) => {
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
                            continuity_out = Some(outcome);
                        }
                        Err(err) => {
                            warn::emit(WarnEvent {
                                code: "CONTINUITY_FAILED",
                                stage: "continuity",
                                action: "build-continuity",
                                session: &record.session_id,
                                archive: &record.archive_path,
                                source: &record.source_path,
                                retry: "retry-next-cycle",
                                reason: "continuity-build-failed",
                                err: &format!("{err:#}"),
                            });
                        }
                    }
                    distill_out = Some(distill);
                }
                Err(err) => {
                    warn::emit(WarnEvent {
                        code: "DISTILL_FAILED",
                        stage: "distill",
                        action: "run-distill",
                        session: &record.session_id,
                        archive: &record.archive_path,
                        source: &record.source_path,
                        retry: "retry-next-cycle",
                        reason: "distillation-failed",
                        err: &format!("{err:#}"),
                    });
                    audit::append_event(
                        &paths,
                        "distill",
                        "degraded",
                        &format!(
                            "mode=idle archive={} distill_source={} source={} session={} bytes={} error={err:#}",
                            record.archive_path,
                            distill_source_path,
                            record.source_path,
                            record.session_id,
                            archive_size
                        ),
                    )?;
                }
            }
        }
    }

    let embed_started = Instant::now();
    let embed_run_opts = EmbedRunOptions {
        collection_name: "history".to_string(),
        max_docs: cfg.embed.max_docs_per_cycle as usize,
        dry_run: false,
        caller: EmbedCaller::Watcher,
        max_cycle_secs: Some(cfg.embed.max_cycle_secs),
    };
    match embed::run(&paths, &mut state, &cfg.embed, &embed_run_opts) {
        Ok(summary) => {
            // Only log when something meaningful happened: work was done, a real skip
            // reason occurred (cooldown / locked / capability-missing), or degraded.
            // skip_reason="none" with embedded_docs=0 is a pure no-op — suppress the noise.
            let is_noop = summary.skip_reason == "none" && summary.embedded_docs == 0;
            if !is_noop || summary.degraded {
                let line = format!(
                    "mode={} capability={} selected={} embedded={} pending_before={} pending_after={} degraded={} skip_reason={}",
                    summary.mode,
                    summary.capability,
                    summary.selected_docs,
                    summary.embedded_docs,
                    summary.pending_before,
                    summary.pending_after,
                    summary.degraded,
                    summary.skip_reason
                );
                let status = if summary.degraded { "degraded" } else { "ok" };
                let _ = audit::append_event(&paths, "embed", status, &line);
                embed_result = Some(line);
            }

            if summary.skip_reason == "locked" {
                warn::emit(WarnEvent {
                    code: "EMBED_LOCKED",
                    stage: "embed",
                    action: "acquire-lock",
                    session: &usage.session_id,
                    archive: "na",
                    source: "na",
                    retry: "retry-next-cycle",
                    reason: "embed-lock-active",
                    err: "embed-lock-active",
                });
            } else if summary.skip_reason == "capability-missing" {
                warn::emit(WarnEvent {
                    code: "EMBED_CAPABILITY_MISSING",
                    stage: "embed",
                    action: "check-capability",
                    session: &usage.session_id,
                    archive: "na",
                    source: "na",
                    retry: "retry-next-cycle",
                    reason: "embed-capability-missing",
                    err: "qmd-embed-capability-missing",
                });
            }
        }
        Err(err) => {
            let (code, action, reason) = match &err {
                EmbedRunError::CapabilityMissing(_) => (
                    "EMBED_CAPABILITY_MISSING",
                    "check-capability",
                    "capability-missing",
                ),
                EmbedRunError::Locked(_) => ("EMBED_LOCKED", "acquire-lock", "embed-lock-active"),
                EmbedRunError::StatusFailed(_) => {
                    ("EMBED_STATUS_FAILED", "run-embed", "embed-status-failed")
                }
                EmbedRunError::Failed(_) => ("EMBED_FAILED", "run-embed", "embed-failed"),
            };
            warn::emit(WarnEvent {
                code,
                stage: "embed",
                action,
                session: &usage.session_id,
                archive: "na",
                source: "na",
                retry: "retry-next-cycle",
                reason,
                err: &format!("{err}"),
            });
            let line = format!("failed error={err}");
            let _ = audit::append_event(&paths, "embed", "degraded", &line);
            embed_result = Some(line);
        }
    }

    if embed_started.elapsed().as_secs() > cfg.embed.max_cycle_secs {
        warn::emit(WarnEvent {
            code: "EMBED_FAILED",
            stage: "embed",
            action: "run-embed",
            session: &usage.session_id,
            archive: "na",
            source: "na",
            retry: "retry-next-cycle",
            reason: "timeout",
            err: "embed-run-exceeded-max-cycle-secs",
        });
        let timeout_note = format!("timeout max_cycle_secs={}", cfg.embed.max_cycle_secs);
        let _ = audit::append_event(&paths, "embed", "degraded", &timeout_note);
        if let Some(current) = embed_result.take() {
            embed_result = Some(format!("{current} {timeout_note}"));
        } else {
            embed_result = Some(timeout_note);
        }
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
        trigger_threshold: effective_trigger_threshold,
        compaction_authority,
        compaction_emergency_ratio: context_policy.map(|policy| policy.compaction_emergency_ratio),
        compaction_recover_ratio: context_policy.map(|policy| policy.compaction_recover_ratio),
        distill_mode: cfg.distill.mode.clone(),
        distill_idle_secs: cfg.distill.idle_secs,
        distill_max_per_cycle: cfg.distill.max_per_cycle,
        embed_mode: cfg.embed.mode.clone(),
        embed_idle_secs: cfg.embed.idle_secs,
        embed_max_docs_per_cycle: cfg.embed.max_docs_per_cycle,
        retention_active_days: cfg.retention.active_days,
        retention_warm_days: cfg.retention.warm_days,
        retention_cold_days: cfg.retention.cold_days,
        usage,
        triggers: trigger_names,
        inbound_watch,
        archive: archive_out,
        compaction_result,
        distill: distill_out,
        embed_result,
        continuity: continuity_out,
        archive_retention_result,
    })
}

pub fn run_daemon() -> Result<()> {
    let _daemon_lock = acquire_daemon_lock().map_err(|err| {
        if let Ok(paths) = resolve_paths() {
            let _ = audit::append_event(
                &paths,
                "daemon",
                "failed",
                &format!(
                    "code={} reason=lock-acquisition-failed err={err:#}",
                    crate::error::MoonErrorCode::E001Locked.as_str()
                ),
            );
        }
        anyhow::anyhow!("failed to acquire lock: {err:#}")
    })?;

    let shutdown = Arc::new(AtomicBool::new(false));
    let r = shutdown.clone();
    ctrlc::set_handler(move || {
        r.store(true, Ordering::SeqCst);
        eprintln!("\nmoon: shutdown signal received, finishing current cycle...");
    })
    .with_context(|| "failed to set shutdown signal handler")?;

    let mut consecutive_failures = 0u32;
    let mut consecutive_panics = 0u32;

    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }

        let cycle_result = std::panic::catch_unwind(|| {
            run_once_with_options(WatchRunOptions::default())
        });

        match cycle_result {
            Ok(Ok(cycle)) => {
                consecutive_failures = 0;
                consecutive_panics = 0;
                let sleep_for_secs = cycle.poll_interval_secs.max(1);
                
                // Responsive sleep: check shutdown flag every second.
                for _ in 0..sleep_for_secs {
                    if shutdown.load(Ordering::SeqCst) {
                        break;
                    }
                    thread::sleep(Duration::from_secs(1));
                }
            }
            Ok(Err(err)) => {
                consecutive_failures = consecutive_failures.saturating_add(1);
                consecutive_panics = 0;
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
                
                for _ in 0..retry_in_secs {
                    if shutdown.load(Ordering::SeqCst) {
                        break;
                    }
                    thread::sleep(Duration::from_secs(1));
                }
            }
            Err(panic_err) => {
                consecutive_panics = consecutive_panics.saturating_add(1);
                consecutive_failures = 0;

                let panic_msg = if let Some(s) = panic_err.downcast_ref::<&str>() {
                    s.to_string()
                } else if let Some(s) = panic_err.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown-panic-payload".to_string()
                };

                if let Ok(paths) = resolve_paths() {
                    let _ = audit::append_event(
                        &paths,
                        "watcher",
                        "alert",
                        &format!(
                            "DAEMON_PANIC consecutive_panics={} error={}",
                            consecutive_panics, panic_msg
                        ),
                    );
                }

                eprintln!(
                    "moon watcher panicked (count: {}); error: {}",
                    consecutive_panics, panic_msg
                );

                if consecutive_panics >= 3 {
                    if let Ok(paths) = resolve_paths() {
                        let _ = audit::append_event(
                            &paths,
                            "watcher",
                            "alert",
                            "DAEMON_PANIC_HALT after 3 consecutive panics",
                        );
                    }
                    anyhow::bail!("DAEMON_PANIC_HALT: consecutive panic threshold reached");
                }

                // Wait a bit before retrying after a panic (responsive).
                for _ in 0..30 {
                    if shutdown.load(Ordering::SeqCst) {
                        break;
                    }
                    thread::sleep(Duration::from_secs(1));
                }
            }
        }
    }

    eprintln!("moon: graceful shutdown complete.");
    Ok(())
}
