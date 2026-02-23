use anyhow::Result;
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct MoonPaths {
    pub moon_home: PathBuf,
    pub archives_dir: PathBuf,
    pub memory_dir: PathBuf,
    pub memory_file: PathBuf,
    pub logs_dir: PathBuf,
    pub openclaw_sessions_dir: PathBuf,
    pub qmd_bin: PathBuf,
    pub qmd_db: PathBuf,
}

fn required_home_dir() -> Result<PathBuf> {
    if let Some(home) = dirs::home_dir() {
        return Ok(home);
    }
    Err(anyhow::anyhow!("HOME directory could not be resolved"))
}

fn env_or_default_path(var: &str, fallback: PathBuf) -> PathBuf {
    match env::var(var) {
        Ok(v) if !v.trim().is_empty() => PathBuf::from(v.trim()),
        _ => fallback,
    }
}

pub fn resolve_paths() -> Result<MoonPaths> {
    let home = required_home_dir()?;
    let moon_home = env_or_default_path("MOON_HOME", home.join("MOON"));

    let archives_dir = env_or_default_path("MOON_ARCHIVES_DIR", moon_home.join("archives"));
    let memory_dir = env_or_default_path("MOON_MEMORY_DIR", moon_home.join("memory"));
    let memory_file = env_or_default_path("MOON_MEMORY_FILE", moon_home.join("MEMORY.md"));
    let logs_dir = env_or_default_path("MOON_LOGS_DIR", moon_home.join("MOON/logs"));
    let openclaw_sessions_dir = env_or_default_path(
        "OPENCLAW_SESSIONS_DIR",
        home.join(".openclaw/agents/main/sessions"),
    );
    let qmd_bin = env_or_default_path("QMD_BIN", home.join(".bun/bin/qmd"));
    let qmd_db = env_or_default_path("QMD_DB", home.join(".cache/qmd/index.sqlite"));

    Ok(MoonPaths {
        moon_home,
        archives_dir,
        memory_dir,
        memory_file,
        logs_dir,
        openclaw_sessions_dir,
        qmd_bin,
        qmd_db,
    })
}
