use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;
use std::time::{Duration, Instant};

const ARCHIVE_COLLECTION_MASK: &str = "mlib/**/*.md";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionSyncResult {
    Added,
    Updated,
    Recreated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedCapability {
    Bounded,
    UnboundedOnly,
    Missing,
}

impl EmbedCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Bounded => "bounded",
            Self::UnboundedOnly => "unbounded-only",
            Self::Missing => "missing",
        }
    }
}

#[derive(Debug, Clone)]
pub struct EmbedCapabilityProbe {
    pub capability: EmbedCapability,
    pub note: String,
}

#[derive(Debug, Clone)]
pub struct EmbedExecResult {
    pub stdout: String,
    pub stderr: String,
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

pub fn probe_embed_capability(qmd_bin: &Path) -> EmbedCapabilityProbe {
    let bin = match resolve_qmd_bin(qmd_bin) {
        Ok(bin) => bin,
        Err(err) => {
            return EmbedCapabilityProbe {
                capability: EmbedCapability::Missing,
                note: format!("qmd-binary-missing error={err:#}"),
            };
        }
    };

    let output = match Command::new(&bin).arg("embed").arg("--help").output() {
        Ok(output) => output,
        Err(err) => {
            return EmbedCapabilityProbe {
                capability: EmbedCapability::Missing,
                note: format!("embed-help-exec-failed error={err:#}"),
            };
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let combined = format!("{stdout}\n{stderr}");
    let lower = combined.to_ascii_lowercase();

    if !output.status.success() {
        return EmbedCapabilityProbe {
            capability: EmbedCapability::Missing,
            note: format!(
                "embed-help-nonzero code={:?} stderr={}",
                output.status.code(),
                stderr.trim()
            ),
        };
    }

    if lower.contains("--max-docs") {
        return EmbedCapabilityProbe {
            capability: EmbedCapability::Bounded,
            note: "embed-help-detected-max-docs".to_string(),
        };
    }

    EmbedCapabilityProbe {
        capability: EmbedCapability::UnboundedOnly,
        note: "embed-help-no-max-docs".to_string(),
    }
}

pub fn embed_bounded(
    qmd_bin: &Path,
    collection_name: &str,
    max_docs: usize,
    timeout_secs: Option<u64>,
) -> Result<EmbedExecResult> {
    let bin = resolve_qmd_bin(qmd_bin)?;
    let mut cmd = Command::new(&bin);
    cmd.arg("embed")
        .arg(collection_name)
        .arg("--max-docs")
        .arg(max_docs.to_string());
    let output = run_command_with_optional_timeout(&mut cmd, timeout_secs)
        .with_context(|| format!("failed to run `{}`", bin.display()))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if output.status.success() {
        return Ok(EmbedExecResult { stdout, stderr });
    }

    anyhow::bail!(
        "qmd embed (bounded) failed\nstdout: {}\nstderr: {}",
        stdout,
        stderr
    );
}

pub fn output_indicates_embed_status_failed(stdout: &str, stderr: &str) -> bool {
    let combined = format!("{stdout}\n{stderr}");
    let lower = combined.to_ascii_lowercase();

    if lower.contains("\"status\":\"failed\"")
        || lower.contains("\"status\": \"failed\"")
        || lower.contains("\"ok\":false")
        || lower.contains("\"ok\": false")
    {
        return true;
    }

    let Ok(value) = serde_json::from_str::<Value>(stdout) else {
        return false;
    };

    if value
        .get("status")
        .and_then(Value::as_str)
        .is_some_and(|v| v.eq_ignore_ascii_case("failed"))
    {
        return true;
    }
    value
        .get("ok")
        .and_then(Value::as_bool)
        .is_some_and(|ok| !ok)
}

fn run_command_with_optional_timeout(
    cmd: &mut Command,
    timeout_secs: Option<u64>,
) -> Result<Output> {
    let Some(timeout_secs) = timeout_secs else {
        return Ok(cmd.output()?);
    };
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn()?;
    let started = Instant::now();
    loop {
        if child.try_wait()?.is_some() {
            return Ok(child.wait_with_output()?);
        }
        if started.elapsed() >= Duration::from_secs(timeout_secs) {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("command timed out after {}s", timeout_secs);
        }
        thread::sleep(Duration::from_millis(50));
    }
}
