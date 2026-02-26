use anyhow::Result;

use crate::commands::CommandReport;
use crate::moon::watcher;

#[derive(Debug, Clone, Default)]
pub struct MoonWatchOptions {
    pub once: bool,
    pub daemon: bool,
    pub distill_now: bool,
}

pub fn run(opts: &MoonWatchOptions) -> Result<CommandReport> {
    let mut report = CommandReport::new("moon-watch");

    if opts.once && opts.daemon {
        report.issue("invalid flags: use only one of --once or --daemon");
        return Ok(report);
    }
    if opts.daemon && opts.distill_now {
        report.issue("invalid flags: --distill-now is only valid with --once");
        return Ok(report);
    }

    if opts.daemon
        && let Ok(exe) = std::env::current_exe()
    {
        let exe_str = exe.display().to_string();
        if exe_str.contains("target/debug")
            || exe_str.contains("target/release")
            || exe_str.contains("target\\debug")
            || exe_str.contains("target\\release")
        {
            report.issue(
                "CRITICAL: Running the background daemon via `cargo run` is disabled for stability.",
            );
            report.issue(
                "Cargo run holds file locks and causes severe CPU/IO spikes when the daemon restarts.",
            );
            report.issue("Please install the binary to your path first: `cargo install --path .`");
            report.issue(
                "Then start the daemon using the compiled binary: `moon moon-watch --daemon`",
            );
            return Ok(report);
        }
    }

    if opts.daemon {
        report.detail("starting moon watcher in daemon mode");
        watcher::run_daemon()?;
        return Ok(report);
    }

    let cycle = if opts.distill_now {
        watcher::run_once_with_options(watcher::WatchRunOptions {
            force_distill_now: true,
        })?
    } else {
        watcher::run_once()?
    };
    report.detail("moon watcher cycle completed");
    report.detail(format!("state_file={}", cycle.state_file));
    report.detail(format!(
        "heartbeat_epoch_secs={}",
        cycle.heartbeat_epoch_secs
    ));
    report.detail(format!("poll_interval_secs={}", cycle.poll_interval_secs));
    report.detail(format!("threshold.trigger={}", cycle.trigger_threshold));
    report.detail(format!(
        "compaction.authority={}",
        cycle.compaction_authority
    ));
    if let Some(v) = cycle.compaction_emergency_ratio {
        report.detail(format!("compaction.emergency_ratio={v}"));
    }
    if let Some(v) = cycle.compaction_recover_ratio {
        report.detail(format!("compaction.recover_ratio={v}"));
    }
    report.detail(format!("distill.mode={}", cycle.distill_mode));
    report.detail(format!("distill.idle_secs={}", cycle.distill_idle_secs));
    report.detail(format!(
        "distill.max_per_cycle={}",
        cycle.distill_max_per_cycle
    ));
    report.detail(format!("embed.mode={}", cycle.embed_mode));
    report.detail(format!("embed.idle_secs={}", cycle.embed_idle_secs));
    report.detail(format!(
        "embed.max_docs_per_cycle={}",
        cycle.embed_max_docs_per_cycle
    ));
    report.detail(format!(
        "retention.active_days={}",
        cycle.retention_active_days
    ));
    report.detail(format!("retention.warm_days={}", cycle.retention_warm_days));
    report.detail(format!("retention.cold_days={}", cycle.retention_cold_days));
    report.detail(format!("usage.session_id={}", cycle.usage.session_id));
    report.detail(format!("usage.provider={}", cycle.usage.provider));
    report.detail(format!("usage.used_tokens={}", cycle.usage.used_tokens));
    report.detail(format!("usage.max_tokens={}", cycle.usage.max_tokens));
    report.detail(format!("usage.ratio={:.4}", cycle.usage.usage_ratio));
    report.detail(format!("triggers={}", cycle.triggers.join(",")));
    report.detail(format!(
        "inbound_watch.enabled={}",
        cycle.inbound_watch.enabled
    ));
    report.detail(format!(
        "inbound_watch.watched_paths={}",
        cycle.inbound_watch.watched_paths.join(",")
    ));
    report.detail(format!(
        "inbound_watch.detected_files={}",
        cycle.inbound_watch.detected_files
    ));
    report.detail(format!(
        "inbound_watch.triggered_events={}",
        cycle.inbound_watch.triggered_events
    ));
    report.detail(format!(
        "inbound_watch.failed_events={}",
        cycle.inbound_watch.failed_events
    ));
    for event in &cycle.inbound_watch.events {
        report.detail(format!(
            "inbound_watch.event={} status={} message={}",
            event.file_path, event.status, event.message
        ));
    }

    if let Some(archive) = cycle.archive {
        report.detail(format!("archive.path={}", archive.record.archive_path));
        if let Some(projection_path) = &archive.record.projection_path {
            report.detail(format!("archive.projection_path={projection_path}"));
        }
        if let Some(filtered_noise_count) = archive.record.projection_filtered_noise_count {
            report.detail(format!(
                "archive.filtered_noise_count={filtered_noise_count}"
            ));
        }
        report.detail(format!("archive.indexed={}", archive.record.indexed));
        report.detail(format!("archive.deduped={}", archive.deduped));
        report.detail(format!(
            "archive.ledger_path={}",
            archive.ledger_path.display()
        ));
    }
    if let Some(result) = cycle.compaction_result {
        report.detail(format!("compaction.result={result}"));
    }
    if let Some(distill) = cycle.distill {
        report.detail(format!("distill.provider={}", distill.provider));
        report.detail(format!("distill.summary_path={}", distill.summary_path));
    }
    if let Some(result) = cycle.embed_result {
        report.detail(format!("embed.result={result}"));
    }
    if let Some(result) = cycle.archive_retention_result {
        report.detail(format!("archive_retention.result={result}"));
    }
    if let Some(continuity) = cycle.continuity {
        report.detail(format!("continuity.map_path={}", continuity.map_path));
        report.detail(format!(
            "continuity.target_session_id={}",
            continuity.target_session_id
        ));
        report.detail(format!("continuity.rollover_ok={}", continuity.rollover_ok));
    }

    Ok(report)
}
