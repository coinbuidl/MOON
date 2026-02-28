use anyhow::Result;
use crate::commands::CommandReport;
use crate::moon::config::load_config;

#[derive(Debug, Clone)]
pub struct MoonConfigOptions {
    pub show: bool,
}

pub fn run(opts: &MoonConfigOptions) -> Result<CommandReport> {
    let mut report = CommandReport::new("moon-config");
    let cfg = load_config()?;

    if opts.show {
        report.detail(format!("thresholds.trigger_ratio={}", cfg.thresholds.trigger_ratio));
        report.detail(format!("watcher.poll_interval_secs={}", cfg.watcher.poll_interval_secs));
        report.detail(format!("watcher.cooldown_secs={}", cfg.watcher.cooldown_secs));
        report.detail(format!("inbound_watch.enabled={}", cfg.inbound_watch.enabled));
        report.detail(format!("inbound_watch.recursive={}", cfg.inbound_watch.recursive));
        report.detail(format!("inbound_watch.event_mode={}", cfg.inbound_watch.event_mode));
        report.detail(format!("inbound_watch.watch_paths={:?}", cfg.inbound_watch.watch_paths));
        report.detail(format!("distill.mode={}", cfg.distill.mode));
        report.detail(format!("distill.idle_secs={}", cfg.distill.idle_secs));
        report.detail(format!("distill.max_per_cycle={}", cfg.distill.max_per_cycle));
        report.detail(format!("distill.residential_timezone={}", cfg.distill.residential_timezone));
        report.detail(format!("distill.topic_discovery={}", cfg.distill.topic_discovery));
        report.detail(format!("retention.active_days={}", cfg.retention.active_days));
        report.detail(format!("retention.warm_days={}", cfg.retention.warm_days));
        report.detail(format!("retention.cold_days={}", cfg.retention.cold_days));
        report.detail(format!("embed.mode={}", cfg.embed.mode));
        report.detail(format!("embed.idle_secs={}", cfg.embed.idle_secs));
        report.detail(format!("embed.cooldown_secs={}", cfg.embed.cooldown_secs));
        report.detail(format!("embed.max_docs_per_cycle={}", cfg.embed.max_docs_per_cycle));
        report.detail(format!("embed.min_pending_docs={}", cfg.embed.min_pending_docs));
        report.detail(format!("embed.max_cycle_secs={}", cfg.embed.max_cycle_secs));
        
        if let Some(context) = &cfg.context {
            report.detail(format!("context.window_mode={:?}", context.window_mode));
            report.detail(format!("context.window_tokens={:?}", context.window_tokens));
            report.detail(format!("context.prune_mode={:?}", context.prune_mode));
            report.detail(format!("context.compaction_authority={:?}", context.compaction_authority));
            report.detail(format!("context.compaction_start_ratio={}", context.compaction_start_ratio));
            report.detail(format!("context.compaction_emergency_ratio={}", context.compaction_emergency_ratio));
        }
    }

    Ok(report)
}
