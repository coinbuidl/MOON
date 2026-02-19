use anyhow::{Context, Result};
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::commands::CommandReport;
use crate::moon::distill::{DistillInput, load_archive_excerpt, run_distillation};
use crate::moon::paths::resolve_paths;

#[derive(Debug, Clone)]
pub struct MoonDistillOptions {
    pub archive_path: String,
    pub session_id: Option<String>,
}

fn infer_archive_epoch_secs(path: &Path) -> Option<u64> {
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        && let Some((_, suffix)) = stem.rsplit_once('-')
        && suffix.chars().all(|ch| ch.is_ascii_digit())
        && let Ok(parsed) = suffix.parse::<u64>()
    {
        return Some(parsed);
    }

    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?;
    modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

pub fn run(opts: &MoonDistillOptions) -> Result<CommandReport> {
    let paths = resolve_paths()?;
    let mut report = CommandReport::new("moon-distill");

    if opts.archive_path.trim().is_empty() {
        report.issue("archive path cannot be empty");
        return Ok(report);
    }

    let archive_file = Path::new(&opts.archive_path);
    let text = load_archive_excerpt(&opts.archive_path)
        .with_context(|| format!("failed to stream {}", opts.archive_path))?;
    let session_id = opts
        .session_id
        .clone()
        .unwrap_or_else(|| "manual-distill".to_string());

    let out = run_distillation(
        &paths,
        &DistillInput {
            session_id,
            archive_path: opts.archive_path.clone(),
            archive_text: text,
            archive_epoch_secs: infer_archive_epoch_secs(archive_file),
        },
    )?;

    report.detail(format!("provider={}", out.provider));
    report.detail(format!("summary_path={}", out.summary_path));
    report.detail(format!("audit_log_path={}", out.audit_log_path));

    Ok(report)
}
