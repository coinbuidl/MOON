pub mod install;
pub mod moon_distill;
pub mod moon_embed;
pub mod moon_index;
pub mod moon_recall;
pub mod moon_snapshot;
pub mod moon_status;
pub mod moon_stop;
pub mod moon_watch;
pub mod post_upgrade;
pub mod repair;
pub mod status;
pub mod verify;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CommandReport {
    pub command: String,
    pub ok: bool,
    pub details: Vec<String>,
    pub issues: Vec<String>,
}

impl CommandReport {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            ok: true,
            details: Vec::new(),
            issues: Vec::new(),
        }
    }

    pub fn detail(&mut self, text: impl Into<String>) {
        self.details.push(text.into());
    }

    pub fn issue(&mut self, text: impl Into<String>) {
        self.ok = false;
        self.issues.push(text.into());
    }

    pub fn merge(&mut self, mut other: CommandReport) {
        self.ok &= other.ok;
        self.details.append(&mut other.details);
        self.issues.append(&mut other.issues);
    }
}

pub fn ensure_openclaw_available(report: &mut CommandReport) -> bool {
    if crate::openclaw::gateway::openclaw_available() {
        return true;
    }

    report.issue("openclaw binary unavailable; set OPENCLAW_BIN or ensure openclaw is on PATH");
    false
}

pub fn restart_gateway_with_fallback(report: &mut CommandReport) {
    if let Err(err) = crate::openclaw::gateway::run_gateway_restart(2) {
        report.issue(format!("gateway restart failed: {err}"));
        if let Err(fallback_err) = crate::openclaw::gateway::run_gateway_stop_start() {
            report.issue(format!(
                "gateway stop/start fallback failed: {fallback_err}"
            ));
        } else {
            report.detail("gateway stop/start fallback succeeded");
        }
    } else {
        report.detail("gateway restart succeeded");
    }
}
