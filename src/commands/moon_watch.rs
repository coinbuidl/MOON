use anyhow::Result;

use crate::commands::CommandReport;
use crate::moon::watcher;

#[derive(Debug, Clone, Default)]
pub struct MoonWatchOptions {
    pub once: bool,
    pub daemon: bool,
}

pub fn run(opts: &MoonWatchOptions) -> Result<CommandReport> {
    let mut report = CommandReport::new("moon-watch");

    if opts.once && opts.daemon {
        report.issue("invalid flags: use only one of --once or --daemon");
        return Ok(report);
    }

    if opts.daemon {
        report.detail("starting moon watcher in daemon mode");
        watcher::run_daemon()?;
        return Ok(report);
    }

    let cycle = watcher::run_once()?;
    report.detail("moon watcher cycle completed");
    report.detail(format!("state_file={}", cycle.state_file));
    report.detail(format!(
        "heartbeat_epoch_secs={}",
        cycle.heartbeat_epoch_secs
    ));
    report.detail(format!("poll_interval_secs={}", cycle.poll_interval_secs));
    report.detail(format!("threshold.archive={}", cycle.archive_threshold));
    report.detail(format!("threshold.prune={}", cycle.prune_threshold));
    report.detail(format!("threshold.distill={}", cycle.distill_threshold));
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
        report.detail(format!("archive.indexed={}", archive.record.indexed));
        report.detail(format!("archive.deduped={}", archive.deduped));
        report.detail(format!(
            "archive.ledger_path={}",
            archive.ledger_path.display()
        ));
    }
    if let Some(path) = cycle.prune_config_path {
        report.detail(format!("prune.config_path={path}"));
    }
    if let Some(distill) = cycle.distill {
        report.detail(format!("distill.provider={}", distill.provider));
        report.detail(format!("distill.summary_path={}", distill.summary_path));
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
