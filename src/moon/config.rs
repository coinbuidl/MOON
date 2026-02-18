use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoonThresholds {
    pub archive_ratio: f64,
    pub prune_ratio: f64,
    pub distill_ratio: f64,
}

impl Default for MoonThresholds {
    fn default() -> Self {
        Self {
            archive_ratio: 0.80,
            prune_ratio: 0.85,
            distill_ratio: 0.90,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoonWatcherConfig {
    pub poll_interval_secs: u64,
    pub cooldown_secs: u64,
}

impl Default for MoonWatcherConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 30,
            cooldown_secs: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoonInboundWatchConfig {
    pub enabled: bool,
    pub recursive: bool,
    pub watch_paths: Vec<String>,
    pub event_mode: String,
}

impl Default for MoonInboundWatchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            recursive: true,
            watch_paths: Vec::new(),
            event_mode: "now".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MoonConfig {
    pub thresholds: MoonThresholds,
    pub watcher: MoonWatcherConfig,
    pub inbound_watch: MoonInboundWatchConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PartialMoonConfig {
    thresholds: Option<MoonThresholds>,
    watcher: Option<MoonWatcherConfig>,
    inbound_watch: Option<MoonInboundWatchConfig>,
}

fn env_or_f64(var: &str, fallback: f64) -> f64 {
    match env::var(var) {
        Ok(v) => v.trim().parse::<f64>().ok().unwrap_or(fallback),
        Err(_) => fallback,
    }
}

fn env_or_u64(var: &str, fallback: u64) -> u64 {
    match env::var(var) {
        Ok(v) => v.trim().parse::<u64>().ok().unwrap_or(fallback),
        Err(_) => fallback,
    }
}

fn env_or_bool(var: &str, fallback: bool) -> bool {
    match env::var(var) {
        Ok(v) => {
            let trimmed = v.trim();
            match trimmed {
                "1" | "true" | "TRUE" | "yes" | "on" => true,
                "0" | "false" | "FALSE" | "no" | "off" => false,
                _ => fallback,
            }
        }
        Err(_) => fallback,
    }
}

fn env_or_string(var: &str, fallback: &str) -> String {
    match env::var(var) {
        Ok(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => fallback.to_string(),
    }
}

fn env_or_csv_paths(var: &str, fallback: &[String]) -> Vec<String> {
    match env::var(var) {
        Ok(v) => {
            let out = v
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            if out.is_empty() {
                fallback.to_vec()
            } else {
                out
            }
        }
        Err(_) => fallback.to_vec(),
    }
}

fn validate(cfg: &MoonConfig) -> Result<()> {
    let a = cfg.thresholds.archive_ratio;
    let p = cfg.thresholds.prune_ratio;
    let d = cfg.thresholds.distill_ratio;
    if !(a > 0.0 && p > a && d > p && d <= 1.0) {
        return Err(anyhow!(
            "invalid moon thresholds: require 0 < archive < prune < distill <= 1.0"
        ));
    }
    if cfg.watcher.poll_interval_secs == 0 {
        return Err(anyhow!(
            "invalid watcher poll interval: must be >= 1 second"
        ));
    }
    if cfg.inbound_watch.event_mode.trim().is_empty() {
        return Err(anyhow!("invalid inbound event mode: cannot be empty"));
    }
    Ok(())
}

fn resolve_config_path() -> Option<PathBuf> {
    if let Ok(custom) = env::var("MOON_CONFIG_PATH") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    let home = dirs::home_dir()?;
    Some(home.join(".lilac_metaflora").join("moon.toml"))
}

fn merge_file_config(base: &mut MoonConfig) -> Result<()> {
    let Some(path) = resolve_config_path() else {
        return Ok(());
    };
    if !path.exists() {
        return Ok(());
    }

    let raw = fs::read_to_string(&path)?;
    let parsed: PartialMoonConfig = toml::from_str(&raw)
        .map_err(|err| anyhow!("failed to parse moon config {}: {err}", path.display()))?;
    if let Some(thresholds) = parsed.thresholds {
        base.thresholds = thresholds;
    }
    if let Some(watcher) = parsed.watcher {
        base.watcher = watcher;
    }
    if let Some(inbound_watch) = parsed.inbound_watch {
        base.inbound_watch = inbound_watch;
    }
    Ok(())
}

pub fn load_config() -> Result<MoonConfig> {
    let mut cfg = MoonConfig::default();
    merge_file_config(&mut cfg)?;

    cfg.thresholds.archive_ratio =
        env_or_f64("MOON_THRESHOLD_ARCHIVE_RATIO", cfg.thresholds.archive_ratio);
    cfg.thresholds.prune_ratio =
        env_or_f64("MOON_THRESHOLD_PRUNE_RATIO", cfg.thresholds.prune_ratio);
    cfg.thresholds.distill_ratio =
        env_or_f64("MOON_THRESHOLD_DISTILL_RATIO", cfg.thresholds.distill_ratio);
    cfg.watcher.poll_interval_secs =
        env_or_u64("MOON_POLL_INTERVAL_SECS", cfg.watcher.poll_interval_secs);
    cfg.watcher.cooldown_secs = env_or_u64("MOON_COOLDOWN_SECS", cfg.watcher.cooldown_secs);
    cfg.inbound_watch.enabled =
        env_or_bool("MOON_INBOUND_WATCH_ENABLED", cfg.inbound_watch.enabled);
    cfg.inbound_watch.recursive =
        env_or_bool("MOON_INBOUND_RECURSIVE", cfg.inbound_watch.recursive);
    cfg.inbound_watch.event_mode =
        env_or_string("MOON_INBOUND_EVENT_MODE", &cfg.inbound_watch.event_mode);
    cfg.inbound_watch.watch_paths =
        env_or_csv_paths("MOON_INBOUND_WATCH_PATHS", &cfg.inbound_watch.watch_paths);

    validate(&cfg)?;
    Ok(cfg)
}
