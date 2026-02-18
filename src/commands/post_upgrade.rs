use anyhow::Result;

use crate::commands::install::{self, InstallOptions};
use crate::commands::repair::{self, RepairOptions};
use crate::commands::verify::{self, VerifyOptions};
use crate::commands::{CommandReport, ensure_openclaw_available, restart_gateway_with_fallback};

pub fn run() -> Result<CommandReport> {
    let mut report = CommandReport::new("post-upgrade");

    if !ensure_openclaw_available(&mut report) {
        return Ok(report);
    }

    report.merge(install::run(&InstallOptions {
        force: false,
        dry_run: false,
        apply: true,
    })?);
    restart_gateway_with_fallback(&mut report);

    let verify_report = verify::run(&VerifyOptions { strict: true })?;
    let verify_ok = verify_report.ok;
    report.merge(verify_report);

    if !verify_ok {
        report.detail("post-upgrade verify failed; running automatic repair fallback");
        let repair_report = repair::run(&RepairOptions { force: true })?;
        let repair_ok = repair_report.ok;
        report.merge(repair_report);
        if repair_ok {
            report.ok = true;
            report.detail("automatic repair fallback succeeded");
        } else {
            report.ok = false;
        }
    }

    Ok(report)
}
