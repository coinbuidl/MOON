use crate::moon::paths::MoonPaths;
use crate::moon::util::now_epoch_secs;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs;
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContinuityMap {
    pub source_session_id: String,
    pub target_session_id: String,
    pub archive_refs: Vec<String>,
    pub daily_memory_refs: Vec<String>,
    pub key_decisions: Vec<String>,
    pub generated_at_epoch_secs: u64,
}

#[derive(Debug, Clone)]
pub struct ContinuityOutcome {
    pub map_path: String,
    pub target_session_id: String,
    pub rollover_ok: bool,
}

fn try_rollover() -> Result<String> {
    let enabled = std::env::var("MOON_ENABLE_SESSION_ROLLOVER")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !enabled {
        anyhow::bail!(
            "session rollover disabled by default; set MOON_ENABLE_SESSION_ROLLOVER=true"
        );
    }

    if let Ok(cmdline) = std::env::var("MOON_SESSION_ROLLOVER_CMD") {
        let parts: Vec<&str> = cmdline.split_whitespace().collect();
        if parts.is_empty() {
            anyhow::bail!("MOON_SESSION_ROLLOVER_CMD is empty");
        }
        let mut cmd = Command::new(parts[0]);
        if parts.len() > 1 {
            cmd.args(&parts[1..]);
        }
        let out = cmd.output()?;
        if !out.status.success() {
            anyhow::bail!(
                "rollover command failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        let stdout = String::from_utf8_lossy(&out.stdout).to_string();
        if let Ok(json) = serde_json::from_str::<Value>(&stdout)
            && let Some(id) = json.get("id").and_then(Value::as_str)
        {
            return Ok(id.to_string());
        }
        return Ok(format!("external-{}", now_epoch_secs()?));
    }

    let out = Command::new("openclaw")
        .args(["sessions", "new", "--json"])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let stdout = String::from_utf8_lossy(&o.stdout).to_string();
            if let Ok(json) = serde_json::from_str::<Value>(&stdout)
                && let Some(id) = json.get("id").and_then(Value::as_str)
            {
                return Ok(id.to_string());
            }
            Ok(format!("openclaw-{}", now_epoch_secs()?))
        }
        Ok(o) => anyhow::bail!(
            "openclaw session rollover failed: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        ),
        Err(err) => Err(err.into()),
    }
}

pub fn build_continuity(
    paths: &MoonPaths,
    source_session_id: &str,
    archive_ref: &str,
    daily_memory_ref: &str,
    key_decisions: Vec<String>,
) -> Result<ContinuityOutcome> {
    let ts = now_epoch_secs()?;
    let (target_session_id, rollover_ok) = match try_rollover() {
        Ok(id) => (id, true),
        Err(_) => (format!("pending-{}", ts), false),
    };

    let map = ContinuityMap {
        source_session_id: source_session_id.to_string(),
        target_session_id: target_session_id.clone(),
        archive_refs: vec![archive_ref.to_string()],
        daily_memory_refs: vec![daily_memory_ref.to_string()],
        key_decisions,
        generated_at_epoch_secs: ts,
    };

    let dir = paths.moon_home.join("continuity");
    fs::create_dir_all(&dir).with_context(|| format!("failed to create {}", dir.display()))?;
    let file = dir.join(format!("continuity-{}.json", ts));
    fs::write(&file, format!("{}\n", serde_json::to_string_pretty(&map)?))
        .with_context(|| format!("failed to write {}", file.display()))?;

    Ok(ContinuityOutcome {
        map_path: file.display().to_string(),
        target_session_id,
        rollover_ok,
    })
}
