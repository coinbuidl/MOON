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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MoonConfig {
    pub thresholds: MoonThresholds,
    pub watcher: MoonWatcherConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PartialMoonConfig {
    thresholds: Option<MoonThresholds>,
    watcher: Option<MoonWatcherConfig>,
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

    validate(&cfg)?;
    Ok(cfg)
}
