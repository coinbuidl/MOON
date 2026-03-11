use anyhow::{Context, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

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

    let mut cmd = Command::new(&bin);
    cmd.arg("embed").arg("--help");
    let output = match crate::moon::util::run_command_with_optional_timeout(&mut cmd, Some(30)) {
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
    let output = crate::moon::util::run_command_with_optional_timeout(&mut cmd, timeout_secs)
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
