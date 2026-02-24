use crate::moon::paths::MoonPaths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MoonState {
    pub schema_version: u32,
    pub last_heartbeat_epoch_secs: u64,
    pub last_archive_trigger_epoch_secs: Option<u64>,
    #[serde(alias = "last_prune_trigger_epoch_secs")]
    pub last_compaction_trigger_epoch_secs: Option<u64>,
    pub last_distill_trigger_epoch_secs: Option<u64>,
    pub last_session_id: Option<String>,
    pub last_usage_ratio: Option<f64>,
    pub last_provider: Option<String>,
    pub distilled_archives: BTreeMap<String, u64>,
    pub inbound_seen_files: BTreeMap<String, u64>,
}

impl Default for MoonState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            last_heartbeat_epoch_secs: 0,
            last_archive_trigger_epoch_secs: None,
            last_compaction_trigger_epoch_secs: None,
            last_distill_trigger_epoch_secs: None,
            last_session_id: None,
            last_usage_ratio: None,
            last_provider: None,
            distilled_archives: BTreeMap::new(),
            inbound_seen_files: BTreeMap::new(),
        }
    }
}

pub fn state_file_path(paths: &MoonPaths) -> PathBuf {
    if let Ok(custom_file) = env::var("MOON_STATE_FILE") {
        let trimmed = custom_file.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    if let Ok(custom_dir) = env::var("MOON_STATE_DIR") {
        let trimmed = custom_dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed).join("moon_state.json");
        }
    }
    paths
        .moon_home
        .join("moon")
        .join("state")
        .join("moon_state.json")
}

pub fn load(paths: &MoonPaths) -> Result<MoonState> {
    let file = state_file_path(paths);
    if !file.exists() {
        return Ok(MoonState::default());
    }

    let raw =
        fs::read_to_string(&file).with_context(|| format!("failed to read {}", file.display()))?;
    let parsed: MoonState = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse {}", file.display()))?;
    Ok(parsed)
}

pub fn save(paths: &MoonPaths, state: &MoonState) -> Result<PathBuf> {
    let file = state_file_path(paths);
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let data = serde_json::to_string_pretty(state)?;
    fs::write(&file, format!("{data}\n"))
        .with_context(|| format!("failed to write {}", file.display()))?;
    Ok(file)
}

pub fn rewrite_distilled_archive_paths(
    paths: &MoonPaths,
    rewrites: &BTreeMap<String, String>,
) -> Result<usize> {
    if rewrites.is_empty() {
        return Ok(0);
    }

    let mut state = load(paths)?;
    if state.distilled_archives.is_empty() {
        return Ok(0);
    }

    let mut rewritten = 0usize;
    let mut normalized = BTreeMap::new();
    for (archive_path, epoch_secs) in &state.distilled_archives {
        let next = rewrites
            .get(archive_path)
            .cloned()
            .unwrap_or_else(|| archive_path.clone());
        if next != *archive_path {
            rewritten += 1;
        }
        normalized
            .entry(next)
            .and_modify(|existing| {
                if *existing < *epoch_secs {
                    *existing = *epoch_secs;
                }
            })
            .or_insert(*epoch_secs);
    }

    if rewritten > 0 {
        state.distilled_archives = normalized;
        save(paths, &state)?;
    }

    Ok(rewritten)
}
