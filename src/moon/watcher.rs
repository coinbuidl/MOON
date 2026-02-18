use crate::moon::archive::{ArchivePipelineOutcome, archive_and_index};
use crate::moon::audit;
use crate::moon::config::load_config;
use crate::moon::continuity::{ContinuityOutcome, build_continuity};
use crate::moon::distill::{DistillInput, DistillOutput, run_distillation};
use crate::moon::inbound_watch::{self, InboundWatchOutcome};
use crate::moon::paths::resolve_paths;
use crate::moon::prune;
use crate::moon::session_usage::{SessionUsageSnapshot, collect_usage};
use crate::moon::snapshot::latest_session_file;
use crate::moon::state::{load, save};
use crate::moon::thresholds::{TriggerKind, evaluate};
use anyhow::{Context, Result};
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct WatchCycleOutcome {
    pub state_file: String,
    pub heartbeat_epoch_secs: u64,
    pub poll_interval_secs: u64,
    pub archive_threshold: f64,
    pub prune_threshold: f64,
    pub distill_threshold: f64,
    pub usage: SessionUsageSnapshot,
    pub triggers: Vec<String>,
    pub inbound_watch: InboundWatchOutcome,
    pub archive: Option<ArchivePipelineOutcome>,
    pub prune_config_path: Option<String>,
    pub distill: Option<DistillOutput>,
    pub continuity: Option<ContinuityOutcome>,
}

fn run_archive_if_needed(
    paths: &crate::moon::paths::MoonPaths,
    trigger_set: &[TriggerKind],
) -> Result<Option<ArchivePipelineOutcome>> {
    let needs_archive = trigger_set.iter().any(|t| {
        matches!(
            t,
            TriggerKind::Archive | TriggerKind::Prune | TriggerKind::Distill
        )
    });
    if !needs_archive {
        return Ok(None);
    }

    let Some(source) = latest_session_file(&paths.openclaw_sessions_dir)? else {
        anyhow::bail!("no source session file found in openclaw sessions dir");
    };

    let out = archive_and_index(paths, &source, "history")?;
    Ok(Some(out))
}

fn extract_key_decisions(summary: &str) -> Vec<String> {
    summary
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .take(8)
        .map(ToOwned::to_owned)
        .collect()
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
            TriggerKind::Prune => "prune".to_string(),
            TriggerKind::Distill => "distill".to_string(),
        })
        .collect::<Vec<_>>();

    let mut archive_out = None;
    let mut prune_config_path = None;
    let mut distill_out = None;
    let mut continuity_out = None;

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

    if triggers.contains(&TriggerKind::Prune) {
        let config_path = prune::apply_aggressive_profile(&paths, "oc-token-optim")?;
        state.last_prune_trigger_epoch_secs = Some(usage.captured_at_epoch_secs);
        audit::append_event(
            &paths,
            "prune",
            "ok",
            &format!("applied aggressive profile at {}", config_path),
        )?;
        prune_config_path = Some(config_path);
    }

    if triggers.contains(&TriggerKind::Distill) {
        let archive_path = archive_out
            .as_ref()
            .map(|a| a.record.archive_path.clone())
            .context("distill trigger requires archive result")?;
        let archive_text = std::fs::read_to_string(&archive_path).unwrap_or_else(|_| {
            std::fs::read(&archive_path)
                .ok()
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default()
        });

        let input = DistillInput {
            session_id: usage.session_id.clone(),
            archive_path: archive_path.clone(),
            archive_text,
        };
        let distill = run_distillation(&paths, &input)?;
        state.last_distill_trigger_epoch_secs = Some(usage.captured_at_epoch_secs);

        let continuity = build_continuity(
            &paths,
            &usage.session_id,
            &archive_path,
            &distill.summary_path,
            extract_key_decisions(&distill.summary),
        )?;

        audit::append_event(
            &paths,
            "continuity",
            if continuity.rollover_ok {
                "ok"
            } else {
                "degraded"
            },
            &format!(
                "map={} target_session={} rollover_ok={}",
                continuity.map_path, continuity.target_session_id, continuity.rollover_ok
            ),
        )?;

        distill_out = Some(distill);
        continuity_out = Some(continuity);
    }

    let file = save(&paths, &state)?;

    Ok(WatchCycleOutcome {
        state_file: file.display().to_string(),
        heartbeat_epoch_secs: state.last_heartbeat_epoch_secs,
        poll_interval_secs: cfg.watcher.poll_interval_secs,
        archive_threshold: cfg.thresholds.archive_ratio,
        prune_threshold: cfg.thresholds.prune_ratio,
        distill_threshold: cfg.thresholds.distill_ratio,
        usage,
        triggers: trigger_names,
        inbound_watch,
        archive: archive_out,
        prune_config_path,
        distill: distill_out,
        continuity: continuity_out,
    })
}

pub fn run_daemon() -> Result<()> {
    loop {
        let cycle = run_once()?;
        let sleep_for = Duration::from_secs(cycle.poll_interval_secs);
        thread::sleep(sleep_for);
    }
}
