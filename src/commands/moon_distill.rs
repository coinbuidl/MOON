use anyhow::{Context, Result};
use std::path::Path;
use std::time::UNIX_EPOCH;

use crate::commands::CommandReport;
use crate::moon::distill::{
    DistillInput, WisdomDistillInput, archive_file_size, run_distillation, run_wisdom_distillation,
};
use crate::moon::paths::resolve_paths;

#[derive(Debug, Clone)]
pub struct MoonDistillOptions {
    pub mode: String,
    pub archive_path: Option<String>,
    pub files: Vec<String>,
    pub session_id: Option<String>,
    pub dry_run: bool,
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
    let mut report = CommandReport::new("distill");

    let mode = opts.mode.trim().to_ascii_lowercase();
    let normalized_mode = match mode.as_str() {
        "norm" | "l1" | "layer1" | "l1-normalisation" | "l1-normalization" | "" => "norm",
        "syns" | "syn" | "wisdom" | "layer2" | "l2-synthesis" | "l2-distillation" => "syns",
        _ => {
            report.issue(format!(
                "invalid distill mode `{}`; use `norm` or `syns`",
                opts.mode
            ));
            return Ok(report);
        }
    };

    if normalized_mode == "syns" {
        if opts.dry_run {
            report.detail("distill.dry_run=true".to_string());
        }
        let out = match run_wisdom_distillation(
            &paths,
            &WisdomDistillInput {
                trigger: "manual-distill".to_string(),
                day_epoch_secs: None,
                source_paths: opts.files.clone(),
                dry_run: opts.dry_run,
            },
        ) {
            Ok(out) => out,
            Err(err) => {
                let err_text = format!("{err:#}");
                report.issue(format!("syns skipped: {err_text}"));
                let lower = err_text.to_ascii_lowercase();
                if lower.contains("moon_wisdom_provider")
                    || lower.contains("moon_wisdom_model")
                    || lower.contains("primary model")
                    || lower.contains("provider credentials")
                    || lower.contains("api key")
                {
                    report.issue(
                        "fix MOON_WISDOM_PROVIDER, MOON_WISDOM_MODEL, and provider API key"
                            .to_string(),
                    );
                }
                return Ok(report);
            }
        };
        report.detail("distill.mode=syns".to_string());
        report.detail(format!("provider={}", out.provider));
        report.detail(format!("summary_path={}", out.summary_path));
        report.detail(format!("audit_log_path={}", out.audit_log_path));
        return Ok(report);
    }

    let archive_path = match opts.archive_path.as_deref() {
        Some(path) if !path.trim().is_empty() => path,
        _ => {
            report.issue("archive path cannot be empty in norm mode");
            return Ok(report);
        }
    };

    let archive_file = Path::new(archive_path);
    let archive_size = archive_file_size(archive_path)
        .with_context(|| format!("failed to stat {}", archive_path))?;
    let session_id = opts
        .session_id
        .clone()
        .unwrap_or_else(|| "manual-distill".to_string());
    let archive_epoch_secs = infer_archive_epoch_secs(archive_file);

    if opts.dry_run {
        report.detail("distill.dry_run=true".to_string());
        report.detail(format!("archive_size_bytes={archive_size}"));
        report.detail("distill.mode=norm".to_string());
        return Ok(report);
    }

    let out = run_distillation(
        &paths,
        &DistillInput {
            session_id,
            archive_path: archive_path.to_string(),
            archive_text: String::new(),
            archive_epoch_secs,
        },
    )?;
    report.detail("distill.mode=norm".to_string());

    report.detail(format!("provider={}", out.provider));
    report.detail(format!("summary_path={}", out.summary_path));
    report.detail(format!("audit_log_path={}", out.audit_log_path));
    report.detail(format!("archive_size_bytes={archive_size}"));

    Ok(report)
}
