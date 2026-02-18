use crate::moon::config::MoonConfig;
use crate::moon::paths::MoonPaths;
use crate::moon::state::MoonState;
use crate::openclaw::gateway;
use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone)]
pub struct InboundWatchEvent {
    pub file_path: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct InboundWatchOutcome {
    pub enabled: bool,
    pub watched_paths: Vec<String>,
    pub detected_files: usize,
    pub triggered_events: usize,
    pub failed_events: usize,
    pub events: Vec<InboundWatchEvent>,
}

fn modified_epoch_secs(path: &Path) -> Result<u64> {
    let meta = fs::metadata(path).with_context(|| format!("failed to stat {}", path.display()))?;
    let modified = meta.modified().unwrap_or(UNIX_EPOCH);
    Ok(modified
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs())
}

fn collect_files(root: &Path, recursive: bool, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries =
        fs::read_dir(root).with_context(|| format!("failed to read {}", root.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            out.push(path);
            continue;
        }
        if recursive && path.is_dir() {
            collect_files(&path, recursive, out)?;
        }
    }
    Ok(())
}

fn trigger_event(file_path: &Path, mode: &str) -> Result<()> {
    let filename = file_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");
    let event_text = format!(
        "Moon System inbound file detected: {} ({})",
        filename,
        file_path.display()
    );

    gateway::run_system_event(&event_text, mode)
}

pub fn process(
    _paths: &MoonPaths,
    cfg: &MoonConfig,
    state: &mut MoonState,
) -> Result<InboundWatchOutcome> {
    let mut out = InboundWatchOutcome {
        enabled: cfg.inbound_watch.enabled,
        watched_paths: cfg.inbound_watch.watch_paths.clone(),
        ..InboundWatchOutcome::default()
    };

    if !cfg.inbound_watch.enabled || cfg.inbound_watch.watch_paths.is_empty() {
        return Ok(out);
    }

    let mut files = Vec::new();
    for watch_path in &cfg.inbound_watch.watch_paths {
        let dir = Path::new(watch_path);
        if !dir.exists() {
            fs::create_dir_all(dir)
                .with_context(|| format!("failed to create inbound watch dir {}", dir.display()))?;
        }
        collect_files(dir, cfg.inbound_watch.recursive, &mut files)?;
    }

    files.sort();
    let mut currently_seen = BTreeSet::new();

    for file in files {
        let key = file.display().to_string();
        currently_seen.insert(key.clone());

        let modified = modified_epoch_secs(&file)?;
        let previous = state.inbound_seen_files.get(&key).copied().unwrap_or(0);

        if modified <= previous {
            continue;
        }

        out.detected_files += 1;

        match trigger_event(&file, &cfg.inbound_watch.event_mode) {
            Ok(_) => {
                out.triggered_events += 1;
                out.events.push(InboundWatchEvent {
                    file_path: key.clone(),
                    status: "triggered".to_string(),
                    message: "openclaw system event sent".to_string(),
                });
                state.inbound_seen_files.insert(key, modified);
            }
            Err(err) => {
                out.failed_events += 1;
                out.events.push(InboundWatchEvent {
                    file_path: key,
                    status: "failed".to_string(),
                    message: err.to_string(),
                });
            }
        }
    }

    state
        .inbound_seen_files
        .retain(|k, _| currently_seen.contains(k));

    Ok(out)
}
