use anyhow::{Context, Result};
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::commands::CommandReport;
use crate::moon::distill::{
    DistillInput, archive_file_size, distill_chunk_bytes, load_archive_excerpt,
    run_chunked_archive_distillation, run_distillation,
};
use crate::moon::paths::resolve_paths;

#[derive(Debug, Clone)]
pub struct MoonDistillOptions {
    pub archive_path: String,
    pub session_id: Option<String>,
    pub allow_large_archive: bool,
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
    let archive_size = archive_file_size(&opts.archive_path)
        .with_context(|| format!("failed to stat {}", opts.archive_path))?;
    let chunk_threshold_bytes = distill_chunk_bytes() as u64;
    let session_id = opts
        .session_id
        .clone()
        .unwrap_or_else(|| "manual-distill".to_string());
    let archive_epoch_secs = infer_archive_epoch_secs(archive_file);

    let out = if archive_size > chunk_threshold_bytes && !opts.allow_large_archive {
        let chunked = run_chunked_archive_distillation(
            &paths,
            &DistillInput {
                session_id,
                archive_path: opts.archive_path.clone(),
                archive_text: String::new(),
                archive_epoch_secs,
            },
        )?;
        report.detail("distill.mode=chunked".to_string());
        report.detail(format!("distill.chunk_count={}", chunked.chunk_count));
        report.detail(format!(
            "distill.chunk_target_bytes={}",
            chunked.chunk_target_bytes
        ));
        report.detail(format!(
            "distill.chunk_trigger_bytes={chunk_threshold_bytes}"
        ));
        if chunked.truncated {
            report.issue(
                "chunked distill truncated by MOON_DISTILL_MAX_CHUNKS; increase the limit and rerun"
                    .to_string(),
            );
        }
        report.detail(format!("provider={}", chunked.provider));
        report.detail(format!("summary_path={}", chunked.summary_path));
        report.detail(format!("audit_log_path={}", chunked.audit_log_path));
        report.detail(format!("archive_size_bytes={archive_size}"));
        return Ok(report);
    } else {
        let text = load_archive_excerpt(&opts.archive_path)
            .with_context(|| format!("failed to stream {}", opts.archive_path))?;

        run_distillation(
            &paths,
            &DistillInput {
                session_id,
                archive_path: opts.archive_path.clone(),
                archive_text: text,
                archive_epoch_secs,
            },
        )?
    };

    report.detail("distill.mode=single-pass".to_string());

    report.detail(format!("provider={}", out.provider));
    report.detail(format!("summary_path={}", out.summary_path));
    report.detail(format!("audit_log_path={}", out.audit_log_path));
    report.detail(format!("archive_size_bytes={archive_size}"));

    Ok(report)
}
