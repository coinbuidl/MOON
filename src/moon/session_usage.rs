use crate::moon::paths::MoonPaths;
use crate::moon::snapshot::latest_session_file;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionUsageSnapshot {
    pub session_id: String,
    pub used_tokens: u64,
    pub max_tokens: u64,
    pub usage_ratio: f64,
    pub captured_at_epoch_secs: u64,
    pub provider: String,
}

pub trait SessionUsageProvider {
    fn name(&self) -> &'static str;
    fn collect(&self, paths: &MoonPaths) -> Result<SessionUsageSnapshot>;
}

pub struct OpenClawUsageProvider;
pub struct SessionFileUsageProvider;

fn epoch_now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_secs())
}

fn usage_ratio(used: u64, max: u64) -> f64 {
    if max == 0 {
        return 0.0;
    }
    (used as f64) / (max as f64)
}

fn to_snapshot(
    session_id: String,
    used_tokens: u64,
    max_tokens: u64,
    provider: &str,
) -> Result<SessionUsageSnapshot> {
    let max = if max_tokens == 0 { 1 } else { max_tokens };
    Ok(SessionUsageSnapshot {
        session_id,
        used_tokens,
        max_tokens: max,
        usage_ratio: usage_ratio(used_tokens, max),
        captured_at_epoch_secs: epoch_now()?,
        provider: provider.to_string(),
    })
}

fn parse_u64(v: Option<&Value>) -> Option<u64> {
    v.and_then(Value::as_u64)
}

fn find_u64(root: &Value, paths: &[&[&str]]) -> Option<u64> {
    for path in paths {
        let mut cursor = root;
        let mut ok = true;
        for part in *path {
            let Some(next) = cursor.get(*part) else {
                ok = false;
                break;
            };
            cursor = next;
        }
        if ok && let Some(val) = cursor.as_u64() {
            return Some(val);
        }
    }
    None
}

fn session_id_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("session")
        .to_string()
}

fn estimate_tokens_from_text(raw: &str) -> u64 {
    // Keep estimator consistent with plugin's rough budget logic (chars/4 baseline).
    ((raw.chars().count() as u64) / 4).max(1)
}

fn resolve_openclaw_bin() -> Result<PathBuf> {
    if let Ok(custom) = env::var("OPENCLAW_BIN") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }
    which::which("openclaw").context("openclaw not in PATH")
}

fn openclaw_usage_args() -> Vec<String> {
    if let Ok(custom) = env::var("MOON_OPENCLAW_USAGE_ARGS") {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return trimmed
                .split_whitespace()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
        }
    }

    vec!["sessions".into(), "current".into(), "--json".into()]
}

fn parse_openclaw_usage(raw: &str) -> Result<(String, u64, u64)> {
    let parsed: Value = serde_json::from_str(raw).context("invalid OpenClaw usage JSON")?;

    let session_id = parsed
        .get("sessionId")
        .and_then(Value::as_str)
        .or_else(|| parsed.get("id").and_then(Value::as_str))
        .unwrap_or("current")
        .to_string();

    let used = find_u64(
        &parsed,
        &[
            &["usage", "totalTokens"],
            &["usage", "inputTokens"],
            &["tokenUsage", "total"],
            &["context", "usedTokens"],
        ],
    )
    .or_else(|| parse_u64(parsed.get("usedTokens")))
    .context("OpenClaw usage payload missing used token fields")?;

    let max = find_u64(
        &parsed,
        &[
            &["limits", "maxTokens"],
            &["context", "maxTokens"],
            &["tokenUsage", "max"],
        ],
    )
    .or_else(|| parse_u64(parsed.get("maxTokens")))
    .unwrap_or(200_000);

    Ok((session_id, used, max))
}

impl SessionUsageProvider for OpenClawUsageProvider {
    fn name(&self) -> &'static str {
        "openclaw"
    }

    fn collect(&self, _paths: &MoonPaths) -> Result<SessionUsageSnapshot> {
        let bin = resolve_openclaw_bin()?;
        let args = openclaw_usage_args();
        let output = Command::new(&bin)
            .args(&args)
            .output()
            .with_context(|| format!("failed to run `{}`", bin.display()))?;

        if !output.status.success() {
            anyhow::bail!(
                "OpenClaw usage command failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }

        let raw = String::from_utf8_lossy(&output.stdout).to_string();
        let (session_id, used, max) = parse_openclaw_usage(&raw)?;
        to_snapshot(session_id, used, max, self.name())
    }
}

impl SessionUsageProvider for SessionFileUsageProvider {
    fn name(&self) -> &'static str {
        "session-file"
    }

    fn collect(&self, paths: &MoonPaths) -> Result<SessionUsageSnapshot> {
        let Some(source) = latest_session_file(&paths.openclaw_sessions_dir)? else {
            anyhow::bail!("no source session file found in openclaw sessions dir");
        };

        let raw = fs::read_to_string(&source)
            .with_context(|| format!("failed to read {}", source.display()))?;
        let estimated = estimate_tokens_from_text(&raw);
        let session_id = session_id_from_path(&source);

        to_snapshot(session_id, estimated, 200_000, self.name())
    }
}

pub fn collect_usage(paths: &MoonPaths) -> Result<SessionUsageSnapshot> {
    let primary = OpenClawUsageProvider;
    if let Ok(snapshot) = primary.collect(paths) {
        return Ok(snapshot);
    }

    let fallback = SessionFileUsageProvider;
    fallback.collect(paths)
}

#[cfg(test)]
mod tests {
    use super::parse_openclaw_usage;

    #[test]
    fn parse_openclaw_usage_accepts_nested_payload() {
        let raw = r#"{"id":"abc","usage":{"totalTokens":4200},"limits":{"maxTokens":10000}}"#;
        let parsed = parse_openclaw_usage(raw).expect("parse should succeed");
        assert_eq!(parsed.0, "abc");
        assert_eq!(parsed.1, 4200);
        assert_eq!(parsed.2, 10000);
    }
}
