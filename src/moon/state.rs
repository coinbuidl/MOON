use crate::moon::paths::MoonPaths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MoonState {
    pub schema_version: u32,
    pub last_heartbeat_epoch_secs: u64,
    pub last_archive_trigger_epoch_secs: Option<u64>,
    pub last_prune_trigger_epoch_secs: Option<u64>,
    pub last_distill_trigger_epoch_secs: Option<u64>,
    pub last_session_id: Option<String>,
    pub last_usage_ratio: Option<f64>,
    pub last_provider: Option<String>,
    pub inbound_seen_files: BTreeMap<String, u64>,
}

impl Default for MoonState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            last_heartbeat_epoch_secs: 0,
            last_archive_trigger_epoch_secs: None,
            last_prune_trigger_epoch_secs: None,
            last_distill_trigger_epoch_secs: None,
            last_session_id: None,
            last_usage_ratio: None,
            last_provider: None,
            inbound_seen_files: BTreeMap::new(),
        }
    }
}

pub fn state_file_path(paths: &MoonPaths) -> PathBuf {
    paths.moon_home.join("state").join("moon_state.json")
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
