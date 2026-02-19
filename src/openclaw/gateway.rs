use anyhow::{Context, Result};
use serde_json::Value;
use std::env;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::thread;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

fn ensure_executable_path(path: &Path) -> Result<()> {
    let meta = fs::metadata(path)
        .with_context(|| format!("openclaw binary path does not exist: {}", path.display()))?;
    if !meta.is_file() {
        anyhow::bail!("openclaw binary path is not a file: {}", path.display());
    }
    Ok(())
}

fn resolve_openclaw_bin() -> Result<String> {
    let custom =
        env::var("OPENCLAW_BIN").context("OPENCLAW_BIN is required and must point to openclaw")?;
    let trimmed = custom.trim();
    if trimmed.is_empty() {
        anyhow::bail!("OPENCLAW_BIN is required and cannot be empty");
    }
    ensure_executable_path(Path::new(trimmed))?;
    Ok(trimmed.to_string())
}

fn run_openclaw(args: &[&str]) -> Result<Output> {
    let bin = resolve_openclaw_bin()?;
    let out = Command::new(&bin)
        .args(args)
        .output()
        .with_context(|| format!("failed to run `{bin} {}`", args.join(" ")))?;
    Ok(out)
}

pub fn run_openclaw_retry(args: &[&str], retries: usize) -> Result<Output> {
    let mut last_out: Option<Output> = None;

    for attempt in 0..=retries {
        let out = run_openclaw(args)?;
        if out.status.success() {
            return Ok(out);
        }
        last_out = Some(out);
        if attempt < retries {
            let delay_ms = 250 * (attempt + 1) as u64;
            thread::sleep(Duration::from_millis(delay_ms));
        }
    }

    let Some(out) = last_out else {
        anyhow::bail!(
            "command failed after retries without output: openclaw {}",
            args.join(" ")
        );
    };
    anyhow::bail!(
        "command failed after retries: openclaw {}\nstdout: {}\nstderr: {}",
        args.join(" "),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

pub fn try_plugins_install(path: &Path) -> Result<()> {
    let path_str = path.to_string_lossy().to_string();
    let out = run_openclaw(&["plugins", "install", &path_str]);

    match out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            let stderr = String::from_utf8_lossy(&o.stderr).to_string();
            if stderr.contains("plugin already exists") || stderr.contains("already exists") {
                return Ok(());
            }
            anyhow::bail!("openclaw plugins install failed: {}", stderr.trim())
        }
        Err(err) => Err(err),
    }
}

pub fn run_gateway_restart(retries: usize) -> Result<()> {
    run_openclaw_retry(&["gateway", "restart"], retries)?;
    Ok(())
}

pub fn run_gateway_stop_start() -> Result<()> {
    run_openclaw_retry(&["gateway", "stop"], 1)?;
    run_openclaw_retry(&["gateway", "start"], 1)?;
    Ok(())
}

pub fn run_doctor() -> Result<()> {
    let primary = run_openclaw_retry(&["doctor", "--non-interactive"], 2);
    if primary.is_ok() {
        return Ok(());
    }

    run_openclaw_retry(&["doctor"], 1)?;
    Ok(())
}

pub fn plugins_list_json() -> Result<String> {
    let out = run_openclaw_retry(&["plugins", "list", "--json"], 1)?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn run_system_event(text: &str, mode: &str) -> Result<()> {
    run_openclaw_retry(&["system", "event", "--text", text, "--mode", mode], 1)?;
    Ok(())
}

pub fn run_sessions_compact(key: &str) -> Result<String> {
    let normalized_key = key.trim();
    if normalized_key.is_empty() {
        anyhow::bail!("chat.send /compact requires a non-empty session key");
    }

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_millis();
    let idempotency_key = format!("moon-system-compact-{}-{now_ms}", std::process::id());
    let params = serde_json::json!({
        "sessionKey": normalized_key,
        "message": "/compact",
        "deliver": false,
        "idempotencyKey": idempotency_key,
    });
    let params_str = serde_json::to_string(&params)?;

    let out = run_openclaw_retry(
        &[
            "gateway",
            "call",
            "chat.send",
            "--json",
            "--params",
            &params_str,
        ],
        1,
    )?;

    let parsed: Value =
        serde_json::from_slice(&out.stdout).context("invalid JSON from chat.send")?;
    let status = parsed
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let run_id = parsed
        .get("runId")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if status == "started" && !run_id.is_empty() {
        return Ok(format!(
            "requested key={} mode=chat.send:/compact run_id={}",
            normalized_key, run_id
        ));
    }

    if let Some(ok) = parsed.get("ok").and_then(Value::as_bool)
        && ok
    {
        return Ok(format!(
            "requested key={} mode=chat.send:/compact status={}",
            normalized_key, status
        ));
    }

    anyhow::bail!(
        "chat.send /compact returned unexpected response for key {}: {}",
        normalized_key,
        String::from_utf8_lossy(&out.stdout)
    )
}

pub fn openclaw_available() -> bool {
    resolve_openclaw_bin().is_ok()
}
