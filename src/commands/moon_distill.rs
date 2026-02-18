use anyhow::{Context, Result};

use crate::commands::CommandReport;
use crate::moon::distill::{DistillInput, run_distillation};
use crate::moon::paths::resolve_paths;

#[derive(Debug, Clone)]
pub struct MoonDistillOptions {
    pub archive_path: String,
    pub session_id: Option<String>,
}

pub fn run(opts: &MoonDistillOptions) -> Result<CommandReport> {
    let paths = resolve_paths()?;
    let mut report = CommandReport::new("moon-distill");

    if opts.archive_path.trim().is_empty() {
        report.issue("archive path cannot be empty");
        return Ok(report);
    }

    let text = std::fs::read_to_string(&opts.archive_path)
        .with_context(|| format!("failed to read {}", opts.archive_path))?;
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
        },
    )?;

    report.detail(format!("provider={}", out.provider));
    report.detail(format!("summary_path={}", out.summary_path));
    report.detail(format!("audit_log_path={}", out.audit_log_path));

    Ok(report)
}
