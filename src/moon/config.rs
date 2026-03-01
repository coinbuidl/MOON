use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;

mod generated_env_allowlist {
    include!(concat!(env!("OUT_DIR"), "/moon_env_allowlist.rs"));
}

pub const SECRET_ENV_KEYS: [&str; 4] = [
    "GEMINI_API_KEY",
    "OPENAI_API_KEY",
    "ANTHROPIC_API_KEY",
    "AI_API_KEY",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoonThresholds {
    pub trigger_ratio: f64,
}

impl Default for MoonThresholds {
    fn default() -> Self {
        Self {
            trigger_ratio: 0.85,
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
            cooldown_secs: 60,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoonDistillConfig {
    pub max_per_cycle: u64,
    #[serde(default = "default_residential_timezone")]
    pub residential_timezone: String,
    #[serde(default)]
    pub topic_discovery: bool,
    #[serde(default)]
    pub chunk_bytes: Option<String>,
    #[serde(default)]
    pub max_chunks: Option<u64>,
    #[serde(default)]
    pub model_context_tokens: Option<u64>,
}

fn default_residential_timezone() -> String {
    "UTC".to_string()
}

impl Default for MoonDistillConfig {
    fn default() -> Self {
        Self {
            max_per_cycle: 1,
            residential_timezone: "UTC".to_string(),
            topic_discovery: false,
            chunk_bytes: None,
            max_chunks: None,
            model_context_tokens: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoonRetentionConfig {
    pub active_days: u64,
    pub warm_days: u64,
    pub cold_days: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoonEmbedConfig {
    pub mode: String,
    pub idle_secs: u64,
    pub cooldown_secs: u64,
    pub max_docs_per_cycle: u64,
    pub min_pending_docs: u64,
    pub max_cycle_secs: u64,
}

impl Default for MoonEmbedConfig {
    fn default() -> Self {
        Self {
            mode: "auto".to_string(),
            idle_secs: 0,
            cooldown_secs: 60,
            max_docs_per_cycle: 25,
            min_pending_docs: 1,
            max_cycle_secs: 300,
        }
    }
}

impl Default for MoonRetentionConfig {
    fn default() -> Self {
        Self {
            active_days: 7,
            warm_days: 30,
            cold_days: 31,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum MoonContextWindowMode {
    #[default]
    Inherit,
    Fixed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum MoonContextPruneMode {
    #[default]
    Disabled,
    Guarded,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum MoonContextCompactionAuthority {
    #[default]
    Moon,
    Openclaw,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MoonContextConfig {
    pub window_mode: MoonContextWindowMode,
    pub window_tokens: Option<u64>,
    pub prune_mode: MoonContextPruneMode,
    pub compaction_authority: MoonContextCompactionAuthority,
    pub compaction_start_ratio: f64,
    pub compaction_emergency_ratio: f64,
    pub compaction_recover_ratio: f64,
}

impl Default for MoonContextConfig {
    fn default() -> Self {
        Self {
            window_mode: MoonContextWindowMode::Inherit,
            window_tokens: None,
            prune_mode: MoonContextPruneMode::Disabled,
            compaction_authority: MoonContextCompactionAuthority::Moon,
            compaction_start_ratio: 0.50,
            compaction_emergency_ratio: 0.90,
            // Legacy field retained for backward compatibility; compaction
            // trigger logic no longer depends on recover ratio.
            compaction_recover_ratio: 0.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MoonConfig {
    pub thresholds: MoonThresholds,
    pub watcher: MoonWatcherConfig,
    pub inbound_watch: MoonInboundWatchConfig,
    pub distill: MoonDistillConfig,
    pub retention: MoonRetentionConfig,
    pub embed: MoonEmbedConfig,
    pub context: Option<MoonContextConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PartialMoonConfig {
    thresholds: Option<PartialMoonThresholds>,
    watcher: Option<MoonWatcherConfig>,
    inbound_watch: Option<MoonInboundWatchConfig>,
    distill: Option<MoonDistillConfig>,
    retention: Option<MoonRetentionConfig>,
    embed: Option<MoonEmbedConfig>,
    context: Option<MoonContextConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PartialMoonThresholds {
    trigger_ratio: Option<f64>,
    archive_ratio: Option<f64>,
    #[serde(alias = "prune_ratio")]
    compaction_ratio: Option<f64>,
    #[serde(rename = "archive_ratio_trigger_enabled")]
    _archive_ratio_trigger_enabled: Option<bool>,
}

fn env_or_f64_first(vars: &[&str], fallback: f64) -> f64 {
    for var in vars {
        if let Ok(v) = env::var(var) {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(parsed) = trimmed.parse::<f64>() {
                return parsed;
            }
        }
    }
    fallback
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

fn normalize_embed_mode(raw: &str) -> String {
    if raw.eq_ignore_ascii_case("auto")
        || raw.eq_ignore_ascii_case("idle")
        || raw.eq_ignore_ascii_case("manual")
    {
        "auto".to_string()
    } else {
        raw.trim().to_string()
    }
}

fn validate(cfg: &MoonConfig) -> Result<()> {
    let trigger = cfg.thresholds.trigger_ratio;
    if !(trigger > 0.0 && trigger <= 1.0) {
        return Err(anyhow!("invalid trigger ratio: require 0 < trigger <= 1.0"));
    }
    if cfg.watcher.poll_interval_secs == 0 {
        return Err(anyhow!(
            "invalid watcher poll interval: must be >= 1 second"
        ));
    }
    if cfg.inbound_watch.event_mode.trim().is_empty() {
        return Err(anyhow!("invalid inbound event mode: cannot be empty"));
    }
    if cfg.distill.max_per_cycle == 0 {
        return Err(anyhow!("invalid distill max per cycle: must be >= 1"));
    }
    if let Some(max_chunks) = cfg.distill.max_chunks
        && max_chunks == 0
    {
        return Err(anyhow!("invalid distill max_chunks: must be >= 1"));
    }
    if let Some(chunk_bytes) = &cfg.distill.chunk_bytes {
        let trimmed = chunk_bytes.trim();
        if !trimmed.is_empty()
            && !trimmed.eq_ignore_ascii_case("auto")
            && trimmed.parse::<usize>().ok().filter(|v| *v > 0).is_none()
        {
            return Err(anyhow!(
                "invalid distill chunk_bytes: use `auto` or a positive integer"
            ));
        }
    }
    if cfg.retention.active_days == 0 {
        return Err(anyhow!("invalid retention active days: must be >= 1"));
    }
    if cfg.retention.warm_days < cfg.retention.active_days {
        return Err(anyhow!(
            "invalid retention windows: require active_days <= warm_days"
        ));
    }
    if cfg.retention.cold_days <= cfg.retention.warm_days {
        return Err(anyhow!(
            "invalid retention windows: require warm_days < cold_days"
        ));
    }
    if cfg.embed.mode != "auto" {
        return Err(anyhow!(
            "invalid embed mode: use `auto` (legacy aliases: `idle`, `manual`)"
        ));
    }
    if cfg.embed.cooldown_secs == 0 {
        return Err(anyhow!("invalid embed cooldown secs: must be >= 1"));
    }
    if cfg.embed.max_docs_per_cycle == 0 {
        return Err(anyhow!("invalid embed max docs per cycle: must be >= 1"));
    }
    if cfg.embed.min_pending_docs == 0 {
        return Err(anyhow!("invalid embed min pending docs: must be >= 1"));
    }
    if cfg.embed.max_cycle_secs == 0 {
        return Err(anyhow!("invalid embed max cycle secs: must be >= 1"));
    }
    if let Some(context) = &cfg.context {
        if matches!(context.window_mode, MoonContextWindowMode::Fixed) {
            let Some(window_tokens) = context.window_tokens else {
                return Err(anyhow!(
                    "invalid context config: window_tokens is required when window_mode=fixed"
                ));
            };
            if window_tokens < 16_000 {
                return Err(anyhow!(
                    "invalid context config: window_tokens must be >= 16000 when window_mode=fixed"
                ));
            }
        }
        if !(context.compaction_start_ratio > 0.0 && context.compaction_start_ratio <= 1.0) {
            return Err(anyhow!(
                "invalid context config: require 0 < compaction_start_ratio <= 1.0"
            ));
        }
        if !(context.compaction_emergency_ratio > 0.0 && context.compaction_emergency_ratio <= 1.0)
        {
            return Err(anyhow!(
                "invalid context config: require 0 < compaction_emergency_ratio <= 1.0"
            ));
        }
        if !(context.compaction_recover_ratio >= 0.0 && context.compaction_recover_ratio < 1.0) {
            return Err(anyhow!(
                "invalid context config: require 0 <= compaction_recover_ratio < 1.0"
            ));
        }
        if context.compaction_start_ratio > context.compaction_emergency_ratio {
            return Err(anyhow!(
                "invalid context config: require compaction_start_ratio <= compaction_emergency_ratio"
            ));
        }
    }
    Ok(())
}

pub fn resolve_config_path() -> Option<PathBuf> {
    if let Ok(custom) = env::var("MOON_CONFIG_PATH") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    if let Ok(home_override) = env::var("MOON_HOME") {
        let trimmed = home_override.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed).join("moon").join("moon.toml"));
        }
    }

    let home = dirs::home_dir()?;
    Some(home.join("moon").join("moon").join("moon.toml"))
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
    if let Some(thresholds) = parsed.thresholds
        && let Some(trigger_ratio) = thresholds
            .trigger_ratio
            .or(thresholds.compaction_ratio)
            .or(thresholds.archive_ratio)
    {
        base.thresholds.trigger_ratio = trigger_ratio;
    }
    if let Some(watcher) = parsed.watcher {
        base.watcher = watcher;
    }
    if let Some(inbound_watch) = parsed.inbound_watch {
        base.inbound_watch = inbound_watch;
    }
    if let Some(distill) = parsed.distill {
        base.distill = distill;
    }
    if let Some(retention) = parsed.retention {
        base.retention = retention;
    }
    if let Some(embed) = parsed.embed {
        base.embed = embed;
    }
    if let Some(context) = parsed.context {
        base.context = Some(context);
    }
    Ok(())
}

pub fn load_config() -> Result<MoonConfig> {
    let mut cfg = MoonConfig::default();
    merge_file_config(&mut cfg)?;

    cfg.thresholds.trigger_ratio = env_or_f64_first(
        &[
            "MOON_TRIGGER_RATIO",
            "MOON_THRESHOLD_COMPACTION_RATIO",
            "MOON_THRESHOLD_PRUNE_RATIO",
            "MOON_THRESHOLD_ARCHIVE_RATIO",
        ],
        cfg.thresholds.trigger_ratio,
    );
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
    cfg.distill.max_per_cycle = env_or_u64("MOON_DISTILL_MAX_PER_CYCLE", cfg.distill.max_per_cycle);
    cfg.distill.residential_timezone = env_or_string(
        "MOON_RESIDENTIAL_TIMEZONE",
        &cfg.distill.residential_timezone,
    );
    cfg.distill.topic_discovery = env_or_bool("MOON_TOPIC_DISCOVERY", cfg.distill.topic_discovery);
    cfg.retention.active_days = env_or_u64("MOON_RETENTION_ACTIVE_DAYS", cfg.retention.active_days);
    cfg.retention.warm_days = env_or_u64("MOON_RETENTION_WARM_DAYS", cfg.retention.warm_days);
    cfg.retention.cold_days = env_or_u64("MOON_RETENTION_COLD_DAYS", cfg.retention.cold_days);
    cfg.embed.mode = env_or_string("MOON_EMBED_MODE", &cfg.embed.mode);
    cfg.embed.idle_secs = env_or_u64("MOON_EMBED_IDLE_SECS", cfg.embed.idle_secs);
    cfg.embed.cooldown_secs = env_or_u64("MOON_EMBED_COOLDOWN_SECS", cfg.embed.cooldown_secs);
    cfg.embed.max_docs_per_cycle = env_or_u64(
        "MOON_EMBED_MAX_DOCS_PER_CYCLE",
        cfg.embed.max_docs_per_cycle,
    );
    cfg.embed.min_pending_docs =
        env_or_u64("MOON_EMBED_MIN_PENDING_DOCS", cfg.embed.min_pending_docs);
    cfg.embed.max_cycle_secs = env_or_u64("MOON_EMBED_MAX_CYCLE_SECS", cfg.embed.max_cycle_secs);
    cfg.embed.mode = normalize_embed_mode(&cfg.embed.mode);

    validate(&cfg)?;
    audit_env_vars();
    Ok(cfg)
}

pub fn mask_secret(secret: &str) -> String {
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        return "[UNSET]".to_string();
    }

    let chars = trimmed.chars().collect::<Vec<_>>();
    if chars.len() < 8 {
        return "[SET]".to_string();
    }

    let first3 = chars.iter().take(3).collect::<String>();
    let last4 = chars[chars.len().saturating_sub(4)..]
        .iter()
        .collect::<String>();
    format!("{first3}...{last4}")
}

pub fn masked_env_secret(var: &str) -> String {
    match env::var(var) {
        Ok(v) => mask_secret(&v),
        Err(_) => "[UNSET]".to_string(),
    }
}

fn env_allowlist() -> &'static [&'static str] {
    generated_env_allowlist::GENERATED_MOON_ENV_ALLOWLIST
}

fn levenshtein_distance(left: &str, right: &str) -> usize {
    if left == right {
        return 0;
    }
    if left.is_empty() {
        return right.chars().count();
    }
    if right.is_empty() {
        return left.chars().count();
    }

    let left_chars = left.chars().collect::<Vec<_>>();
    let right_chars = right.chars().collect::<Vec<_>>();
    let mut prev_row = (0..=right_chars.len()).collect::<Vec<_>>();
    let mut curr_row = vec![0usize; right_chars.len() + 1];

    for (i, lc) in left_chars.iter().enumerate() {
        curr_row[0] = i + 1;
        for (j, rc) in right_chars.iter().enumerate() {
            let cost = if lc == rc { 0 } else { 1 };
            curr_row[j + 1] = std::cmp::min(
                std::cmp::min(curr_row[j] + 1, prev_row[j + 1] + 1),
                prev_row[j] + cost,
            );
        }
        prev_row.clone_from_slice(&curr_row);
    }

    prev_row[right_chars.len()]
}

fn nearest_allowed_env_key<'a>(candidate: &str, allowlist: &'a [&str]) -> Option<&'a str> {
    let mut best: Option<(usize, &str)> = None;
    for allowed in allowlist {
        let distance = levenshtein_distance(candidate, allowed);
        match best {
            Some((best_distance, _)) if distance >= best_distance => {}
            _ => best = Some((distance, allowed)),
        }
    }
    let (distance, key) = best?;
    if distance <= 4 { Some(key) } else { None }
}

fn audit_env_vars() {
    let allowlist = env_allowlist();

    for (key, _) in env::vars() {
        if key.starts_with("MOON_") && !allowlist.contains(&key.as_str()) {
            if let Some(suggestion) = nearest_allowed_env_key(&key, allowlist) {
                eprintln!(
                    "WARN: unrecognized environment variable: {key}. Did you mean `{suggestion}`?"
                );
            } else {
                eprintln!("WARN: unrecognized environment variable: {key}");
            }
        }
    }
}

fn has_explicit_context_policy_env() -> bool {
    for var in ["MOON_CONFIG_PATH", "MOON_HOME"] {
        if let Ok(v) = env::var(var)
            && !v.trim().is_empty()
        {
            return true;
        }
    }
    false
}

pub fn load_context_policy_if_explicit_env() -> Result<Option<MoonContextConfig>> {
    if !has_explicit_context_policy_env() {
        return Ok(None);
    }
    Ok(load_config()?.context)
}

#[cfg(test)]
mod tests {
    use super::mask_secret;

    #[test]
    fn mask_secret_unset_and_short_values() {
        assert_eq!(mask_secret(""), "[UNSET]");
        assert_eq!(mask_secret("short"), "[SET]");
    }

    #[test]
    fn mask_secret_keeps_prefix_and_suffix() {
        assert_eq!(mask_secret("sk-1234567890abcdef"), "sk-...cdef");
    }
}
