use anyhow::Result;

use crate::commands::status;
use crate::commands::{CommandReport, ensure_openclaw_available};
use crate::openclaw::doctor;

#[derive(Debug, Clone, Default)]
pub struct VerifyOptions {
    pub strict: bool,
}

pub fn run(opts: &VerifyOptions) -> Result<CommandReport> {
    let mut report = CommandReport::new("verify");

    let openclaw_ready = ensure_openclaw_available(&mut report);
    if openclaw_ready {
        if let Err(err) = doctor::run_full_doctor() {
            report.issue(format!("doctor failed: {err}"));
        } else {
            report.detail("doctor: ok".to_string());
        }
    }

    report.merge(status::run()?);

    if opts.strict && !report.ok {
        report.issue("strict verify failed");
    }

    Ok(report)
}
