use crate::moon::paths::MoonPaths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::path::PathBuf;
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

fn resolve_openclaw_bin() -> Result<PathBuf> {
    let custom =
        env::var("OPENCLAW_BIN").context("OPENCLAW_BIN is required and must point to openclaw")?;
    let trimmed = custom.trim();
    if trimmed.is_empty() {
        anyhow::bail!("OPENCLAW_BIN is required and cannot be empty");
    }
    let path = PathBuf::from(trimmed);
    if !path.exists() {
        anyhow::bail!(
            "OPENCLAW_BIN is set but path does not exist: {}",
            path.display()
        );
    }
    if !path.is_file() {
        anyhow::bail!("OPENCLAW_BIN is set but is not a file: {}", path.display());
    }
    Ok(path)
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

fn openclaw_sessions_args() -> Vec<String> {
    vec!["sessions".into(), "--json".into()]
}

fn parse_openclaw_usage(raw: &str) -> Result<(String, u64, u64)> {
    let parsed: Value = serde_json::from_str(raw).context("invalid OpenClaw usage JSON")?;

    if let Some(sessions) = parsed.get("sessions").and_then(Value::as_array) {
        let latest = sessions
            .iter()
            .filter_map(|entry| {
                let used = find_u64(entry, &[&["totalTokens"], &["inputTokens"]])?;
                let updated = entry.get("updatedAt").and_then(Value::as_u64).unwrap_or(0);
                Some((updated, entry, used))
            })
            .max_by_key(|(updated, _, _)| *updated)
            .context("OpenClaw sessions payload missing used token fields")?;

        let session_id = latest
            .1
            .get("key")
            .and_then(Value::as_str)
            .or_else(|| latest.1.get("sessionId").and_then(Value::as_str))
            .or_else(|| latest.1.get("id").and_then(Value::as_str))
            .unwrap_or("current")
            .to_string();

        let used = latest.2;
        let max = find_u64(latest.1, &[&["contextTokens"], &["maxTokens"]]).unwrap_or(200_000);

        return Ok((session_id, used, max));
    }

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

fn parse_openclaw_sessions(raw: &str) -> Result<Vec<(String, u64, u64)>> {
    let parsed: Value = serde_json::from_str(raw).context("invalid OpenClaw sessions JSON")?;
    let sessions = parsed
        .get("sessions")
        .and_then(Value::as_array)
        .context("OpenClaw sessions payload missing sessions array")?;

    let mut out = Vec::with_capacity(sessions.len());
    for entry in sessions {
        let Some(used) = find_u64(
            entry,
            &[
                &["totalTokens"],
                &["inputTokens"],
                &["usage", "totalTokens"],
                &["usage", "inputTokens"],
            ],
        ) else {
            continue;
        };

        let session_id = entry
            .get("key")
            .and_then(Value::as_str)
            .or_else(|| entry.get("sessionId").and_then(Value::as_str))
            .or_else(|| entry.get("id").and_then(Value::as_str))
            .unwrap_or("current")
            .to_string();

        let max = find_u64(
            entry,
            &[&["contextTokens"], &["maxTokens"], &["limits", "maxTokens"]],
        )
        .unwrap_or(200_000);

        out.push((session_id, used, max));
    }

    if out.is_empty() {
        anyhow::bail!("OpenClaw sessions payload missing used token fields");
    }

    Ok(out)
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

pub fn collect_usage(paths: &MoonPaths) -> Result<SessionUsageSnapshot> {
    let primary = OpenClawUsageProvider;
    primary.collect(paths)
}

pub fn collect_openclaw_usages() -> Result<Vec<SessionUsageSnapshot>> {
    let bin = resolve_openclaw_bin()?;
    let args = openclaw_sessions_args();
    let output = Command::new(&bin)
        .args(&args)
        .output()
        .with_context(|| format!("failed to run `{}`", bin.display()))?;

    if !output.status.success() {
        anyhow::bail!(
            "OpenClaw sessions command failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    let raw = String::from_utf8_lossy(&output.stdout).to_string();
    let snapshots = parse_openclaw_sessions(&raw)?;
    let captured_at_epoch_secs = epoch_now()?;

    Ok(snapshots
        .into_iter()
        .map(|(session_id, used_tokens, max_tokens)| {
            let max = if max_tokens == 0 { 1 } else { max_tokens };
            SessionUsageSnapshot {
                session_id,
                used_tokens,
                max_tokens: max,
                usage_ratio: usage_ratio(used_tokens, max),
                captured_at_epoch_secs,
                provider: "openclaw".to_string(),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{parse_openclaw_sessions, parse_openclaw_usage};

    #[test]
    fn parse_openclaw_usage_accepts_nested_payload() {
        let raw = r#"{"id":"abc","usage":{"totalTokens":4200},"limits":{"maxTokens":10000}}"#;
        let parsed = parse_openclaw_usage(raw).expect("parse should succeed");
        assert_eq!(parsed.0, "abc");
        assert_eq!(parsed.1, 4200);
        assert_eq!(parsed.2, 10000);
    }

    #[test]
    fn parse_openclaw_usage_accepts_sessions_payload() {
        let raw = r#"{
            "path":"x",
            "sessions":[
                {"key":"older","updatedAt":1000,"totalTokens":1200,"contextTokens":32000},
                {"key":"newer","updatedAt":2000,"totalTokens":86000,"contextTokens":64000}
            ]
        }"#;
        let parsed = parse_openclaw_usage(raw).expect("parse should succeed");
        assert_eq!(parsed.0, "newer");
        assert_eq!(parsed.1, 86000);
        assert_eq!(parsed.2, 64000);
    }

    #[test]
    fn parse_openclaw_sessions_returns_all_entries() {
        let raw = r#"{
            "path":"x",
            "sessions":[
                {"key":"agent:main:discord:channel:1","updatedAt":1000,"totalTokens":1200,"contextTokens":32000},
                {"key":"agent:main:whatsapp:+614","updatedAt":2000,"totalTokens":86000,"contextTokens":64000}
            ]
        }"#;
        let parsed = parse_openclaw_sessions(raw).expect("parse should succeed");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].0, "agent:main:discord:channel:1");
        assert_eq!(parsed[0].1, 1200);
        assert_eq!(parsed[0].2, 32000);
        assert_eq!(parsed[1].0, "agent:main:whatsapp:+614");
        assert_eq!(parsed[1].1, 86000);
        assert_eq!(parsed[1].2, 64000);
    }

    #[test]
    fn parse_openclaw_usage_skips_sessions_without_token_fields() {
        let raw = r#"{
            "path":"x",
            "sessions":[
                {"key":"missing","updatedAt":2000},
                {"key":"good","updatedAt":1000,"totalTokens":86000,"contextTokens":64000}
            ]
        }"#;
        let parsed = parse_openclaw_usage(raw).expect("parse should succeed");
        assert_eq!(parsed.0, "good");
        assert_eq!(parsed.1, 86000);
        assert_eq!(parsed.2, 64000);
    }

    #[test]
    fn parse_openclaw_sessions_skips_entries_without_token_fields() {
        let raw = r#"{
            "path":"x",
            "sessions":[
                {"key":"missing"},
                {"key":"good","totalTokens":2000,"contextTokens":32000}
            ]
        }"#;
        let parsed = parse_openclaw_sessions(raw).expect("parse should succeed");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].0, "good");
        assert_eq!(parsed[0].1, 2000);
        assert_eq!(parsed[0].2, 32000);
    }
}
