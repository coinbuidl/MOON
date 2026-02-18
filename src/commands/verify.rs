use anyhow::Result;

use crate::commands::status;
use crate::commands::{CommandReport, ensure_openclaw_available};
use crate::openclaw::doctor;

#[derive(Debug, Clone, Default)]
pub struct VerifyOptions {
    pub strict: bool,
}

pub fn run(opts: &VerifyOptions) -> Result<CommandReport> {
    let mut report = status::run()?;
    report.command = "verify".to_string();

    if !ensure_openclaw_available(&mut report) {
        return Ok(report);
    }

    if let Err(err) = doctor::run_full_doctor() {
        report.issue(format!("doctor failed: {err}"));
    } else {
        report.detail("doctor: ok".to_string());
    }

    if opts.strict && !report.ok {
        report.issue("strict verify failed");
    }

    Ok(report)
}
