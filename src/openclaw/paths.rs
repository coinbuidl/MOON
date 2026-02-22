use anyhow::{Context, Result};
use std::env;
use std::path::Path;
use std::path::PathBuf;

pub const PLUGIN_ID: &str = "MOON";

#[derive(Debug, Clone)]
pub struct OpenClawPaths {
    pub state_dir: PathBuf,
    pub config_path: PathBuf,
    pub extensions_dir: PathBuf,
    pub plugin_dir: PathBuf,
    pub plugin_id: String,
}

fn required_home_dir() -> Result<PathBuf> {
    if let Ok(val) = env::var("OPENCLAW_HOME") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            return Ok(PathBuf::from(trimmed));
        }
    }

    if let Some(home) = dirs::home_dir() {
        return Ok(home);
    }

    Err(anyhow::anyhow!("HOME directory could not be resolved"))
}

pub fn resolve_paths() -> Result<OpenClawPaths> {
    let home = required_home_dir()?;

    let state_dir = match env::var("OPENCLAW_STATE_DIR") {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v.trim()),
        _ => home.join(".openclaw"),
    };

    let config_path = match env::var("OPENCLAW_CONFIG_PATH") {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v.trim()),
        _ => state_dir.join("openclaw.json"),
    };

    let extensions_dir = state_dir.join("extensions");
    let plugin_dir = extensions_dir.join(PLUGIN_ID);

    Ok(OpenClawPaths {
        state_dir,
        config_path,
        extensions_dir,
        plugin_dir,
        plugin_id: PLUGIN_ID.to_string(),
    })
}

pub fn ensure_parent_dir(path: &Path) -> Result<()> {
    let parent = path
        .parent()
        .context("path has no parent directory")?
        .to_path_buf();
    std::fs::create_dir_all(parent)?;
    Ok(())
}
