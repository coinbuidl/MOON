use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionSyncResult {
    Added,
    Updated,
}

fn resolve_qmd_bin(bin: &Path) -> Result<PathBuf> {
    if bin.exists() {
        return Ok(bin.to_path_buf());
    }
    let found = which::which("qmd").context("qmd binary not found in QMD_BIN or PATH")?;
    Ok(found)
}

fn is_existing_collection_error(stdout: &str, stderr: &str) -> bool {
    let combined = format!("{stdout}\n{stderr}").to_ascii_lowercase();
    combined.contains("collection") && combined.contains("already exists")
}

pub fn collection_add_or_update(
    qmd_bin: &Path,
    archives_dir: &Path,
    collection_name: &str,
) -> Result<CollectionSyncResult> {
    let bin = resolve_qmd_bin(qmd_bin)?;
    let add_output = Command::new(&bin)
        .arg("collection")
        .arg("add")
        .arg(archives_dir)
        .arg("--name")
        .arg(collection_name)
        .output()
        .with_context(|| format!("failed to run `{}`", bin.display()))?;

    if add_output.status.success() {
        return Ok(CollectionSyncResult::Added);
    }

    let add_stdout = String::from_utf8_lossy(&add_output.stdout).to_string();
    let add_stderr = String::from_utf8_lossy(&add_output.stderr).to_string();
    if is_existing_collection_error(&add_stdout, &add_stderr) {
        let update_output = Command::new(&bin)
            .arg("update")
            .output()
            .with_context(|| format!("failed to run `{}`", bin.display()))?;

        if update_output.status.success() {
            return Ok(CollectionSyncResult::Updated);
        }

        anyhow::bail!(
            "qmd update failed after collection add conflict\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&update_output.stdout),
            String::from_utf8_lossy(&update_output.stderr)
        );
    }

    anyhow::bail!(
        "qmd collection add failed\nstdout: {}\nstderr: {}",
        add_stdout,
        add_stderr
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
