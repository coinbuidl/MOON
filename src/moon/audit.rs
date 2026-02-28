use crate::moon::paths::MoonPaths;
use crate::moon::util::now_epoch_secs;
use anyhow::{Context, Result};
use serde::Serialize;
use std::fs;
use std::path::Path;

const MAX_AUDIT_LOG_SIZE: u64 = 10 * 1024 * 1024; // 10MB

#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    pub at_epoch_secs: u64,
    pub phase: String,
    pub status: String,
    pub message: String,
}

pub fn append_event(paths: &MoonPaths, phase: &str, status: &str, message: &str) -> Result<()> {
    fs::create_dir_all(&paths.logs_dir)
        .with_context(|| format!("failed to create {}", paths.logs_dir.display()))?;
    let event = AuditEvent {
        at_epoch_secs: now_epoch_secs()?,
        phase: phase.to_string(),
        status: status.to_string(),
        message: message.to_string(),
    };

    let line = format!("{}\n", serde_json::to_string(&event)?);
    let path = paths.logs_dir.join("audit.log");
    let _ = maybe_rotate_log(&path);

    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(line.as_bytes())?;
    Ok(())
}

fn maybe_rotate_log(path: &Path) -> Result<()> {
    if let Ok(meta) = fs::metadata(path) {
        if meta.len() >= MAX_AUDIT_LOG_SIZE {
            let backup = format!("{}.1", path.display());
            let _ = fs::rename(path, backup);
        }
    }
    Ok(())
}
