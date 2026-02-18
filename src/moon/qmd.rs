use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

fn resolve_qmd_bin(bin: &Path) -> Result<PathBuf> {
    if bin.exists() {
        return Ok(bin.to_path_buf());
    }
    let found = which::which("qmd").context("qmd binary not found in QMD_BIN or PATH")?;
    Ok(found)
}

pub fn collection_add(qmd_bin: &Path, archives_dir: &Path, collection_name: &str) -> Result<()> {
    let bin = resolve_qmd_bin(qmd_bin)?;
    let output = Command::new(&bin)
        .arg("collection")
        .arg("add")
        .arg(archives_dir)
        .arg("--name")
        .arg(collection_name)
        .output()
        .with_context(|| format!("failed to run `{}`", bin.display()))?;

    if output.status.success() {
        return Ok(());
    }

    anyhow::bail!(
        "qmd collection add failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

pub fn search(qmd_bin: &Path, collection_name: &str, query: &str) -> Result<String> {
    let bin = resolve_qmd_bin(qmd_bin)?;
    let output = Command::new(&bin)
        .arg("search")
        .arg(collection_name)
        .arg(query)
        .arg("--json")
        .output()
        .with_context(|| format!("failed to run `{}`", bin.display()))?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }

    anyhow::bail!(
        "qmd search failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
