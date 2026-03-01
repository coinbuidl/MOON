use crate::commands::CommandReport;
use crate::moon::config::{SECRET_ENV_KEYS, load_config, masked_env_secret, resolve_config_path};
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct MoonConfigOptions {
    pub show: bool,
}

pub fn run(opts: &MoonConfigOptions) -> Result<CommandReport> {
    let mut report = CommandReport::new("moon-config");
    let cfg = load_config()?;

    if opts.show {
        report.detail(
            "resolution.order=defaults -> moon.toml overrides -> environment overrides".to_string(),
        );
        let config_path = resolve_config_path();
        match config_path {
            Some(path) if path.exists() => {
                report.detail(format!("resolution.moon_toml={}", path.display()));
            }
            Some(path) => {
                report.detail(format!("resolution.moon_toml=missing ({})", path.display()));
            }
            None => {
                report.detail("resolution.moon_toml=unresolved".to_string());
            }
        }

        report.detail(format!(
            "thresholds.trigger_ratio={}",
            cfg.thresholds.trigger_ratio
        ));
        report.detail(format!(
            "watcher.poll_interval_secs={}",
            cfg.watcher.poll_interval_secs
        ));
        report.detail(format!(
            "watcher.cooldown_secs={}",
            cfg.watcher.cooldown_secs
        ));
        report.detail(format!(
            "inbound_watch.enabled={}",
            cfg.inbound_watch.enabled
        ));
        report.detail(format!(
            "inbound_watch.recursive={}",
            cfg.inbound_watch.recursive
        ));
        report.detail(format!(
            "inbound_watch.event_mode={}",
            cfg.inbound_watch.event_mode
        ));
        report.detail(format!(
            "inbound_watch.watch_paths={:?}",
            cfg.inbound_watch.watch_paths
        ));
        report.detail(format!(
            "distill.max_per_cycle={}",
            cfg.distill.max_per_cycle
        ));
        report.detail(format!(
            "distill.residential_timezone={}",
            cfg.distill.residential_timezone
        ));
        report.detail(format!(
            "distill.topic_discovery={}",
            cfg.distill.topic_discovery
        ));
        report.detail(format!("distill.chunk_bytes={:?}", cfg.distill.chunk_bytes));
        report.detail(format!("distill.max_chunks={:?}", cfg.distill.max_chunks));
        report.detail(format!(
            "distill.model_context_tokens={:?}",
            cfg.distill.model_context_tokens
        ));
        report.detail(format!(
            "retention.active_days={}",
            cfg.retention.active_days
        ));
        report.detail(format!("retention.warm_days={}", cfg.retention.warm_days));
        report.detail(format!("retention.cold_days={}", cfg.retention.cold_days));
        report.detail(format!("embed.mode={}", cfg.embed.mode));
        report.detail(format!("embed.idle_secs={}", cfg.embed.idle_secs));
        report.detail(format!("embed.cooldown_secs={}", cfg.embed.cooldown_secs));
        report.detail(format!(
            "embed.max_docs_per_cycle={}",
            cfg.embed.max_docs_per_cycle
        ));
        report.detail(format!(
            "embed.min_pending_docs={}",
            cfg.embed.min_pending_docs
        ));
        report.detail(format!("embed.max_cycle_secs={}", cfg.embed.max_cycle_secs));

        if let Some(context) = &cfg.context {
            report.detail(format!("context.window_mode={:?}", context.window_mode));
            report.detail(format!("context.window_tokens={:?}", context.window_tokens));
            report.detail(format!("context.prune_mode={:?}", context.prune_mode));
            report.detail(format!(
                "context.compaction_authority={:?}",
                context.compaction_authority
            ));
            report.detail(format!(
                "context.compaction_start_ratio={}",
                context.compaction_start_ratio
            ));
            report.detail(format!(
                "context.compaction_emergency_ratio={}",
                context.compaction_emergency_ratio
            ));
        }

        for key in SECRET_ENV_KEYS {
            report.detail(format!("secret.{key}={}", masked_env_secret(key)));
        }
    }

    Ok(report)
}
