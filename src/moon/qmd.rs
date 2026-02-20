use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

const ARCHIVE_COLLECTION_MASK: &str = "**/*.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionSyncResult {
    Added,
    Updated,
    Recreated,
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

fn collection_pattern(qmd_bin: &Path, collection_name: &str) -> Result<Option<String>> {
    let output = Command::new(qmd_bin)
        .arg("collection")
        .arg("list")
        .output()
        .with_context(|| format!("failed to run `{}`", qmd_bin.display()))?;
    if !output.status.success() {
        anyhow::bail!(
            "qmd collection list failed\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut in_collection_block = false;
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(&format!("{collection_name} (qmd://")) {
            in_collection_block = true;
            continue;
        }
        if in_collection_block {
            if trimmed.is_empty() {
                break;
            }
            if let Some(pattern) = trimmed.strip_prefix("Pattern:") {
                let normalized = pattern.trim();
                if !normalized.is_empty() {
                    return Ok(Some(normalized.to_string()));
                }
                break;
            }
        }
    }

    Ok(None)
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
        .arg("--mask")
        .arg(ARCHIVE_COLLECTION_MASK)
        .output()
        .with_context(|| format!("failed to run `{}`", bin.display()))?;

    if add_output.status.success() {
        return Ok(CollectionSyncResult::Added);
    }

    let add_stdout = String::from_utf8_lossy(&add_output.stdout).to_string();
    let add_stderr = String::from_utf8_lossy(&add_output.stderr).to_string();
    if is_existing_collection_error(&add_stdout, &add_stderr) {
        let existing_pattern = collection_pattern(&bin, collection_name).ok().flatten();
        if existing_pattern
            .as_deref()
            .is_some_and(|pattern| pattern != ARCHIVE_COLLECTION_MASK)
        {
            let remove_output = Command::new(&bin)
                .arg("collection")
                .arg("remove")
                .arg(collection_name)
                .output()
                .with_context(|| format!("failed to run `{}`", bin.display()))?;
            if !remove_output.status.success() {
                anyhow::bail!(
                    "qmd collection remove failed while recreating {}\nstdout: {}\nstderr: {}",
                    collection_name,
                    String::from_utf8_lossy(&remove_output.stdout),
                    String::from_utf8_lossy(&remove_output.stderr)
                );
            }

            let recreate_output = Command::new(&bin)
                .arg("collection")
                .arg("add")
                .arg(archives_dir)
                .arg("--name")
                .arg(collection_name)
                .arg("--mask")
                .arg(ARCHIVE_COLLECTION_MASK)
                .output()
                .with_context(|| format!("failed to run `{}`", bin.display()))?;
            if recreate_output.status.success() {
                return Ok(CollectionSyncResult::Recreated);
            }

            anyhow::bail!(
                "qmd collection add failed after recreate {}\nstdout: {}\nstderr: {}",
                collection_name,
                String::from_utf8_lossy(&recreate_output.stdout),
                String::from_utf8_lossy(&recreate_output.stderr)
            );
        }

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

pub fn update(qmd_bin: &Path) -> Result<()> {
    let bin = resolve_qmd_bin(qmd_bin)?;
    let output = Command::new(&bin)
        .arg("update")
        .output()
        .with_context(|| format!("failed to run `{}`", bin.display()))?;

    if output.status.success() {
        return Ok(());
    }

    anyhow::bail!(
        "qmd update failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
