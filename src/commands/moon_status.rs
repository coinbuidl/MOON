use anyhow::Result;

use crate::commands::CommandReport;
use crate::moon::paths::resolve_paths;

pub fn run() -> Result<CommandReport> {
    let paths = resolve_paths()?;
    let mut report = CommandReport::new("moon-status");

    report.detail(format!("moon_home={}", paths.moon_home.display()));
    report.detail(format!("archives_dir={}", paths.archives_dir.display()));
    report.detail(format!("memory_dir={}", paths.memory_dir.display()));
    report.detail(format!("memory_file={}", paths.memory_file.display()));
    report.detail(format!("logs_dir={}", paths.logs_dir.display()));
    report.detail(format!(
        "openclaw_sessions_dir={}",
        paths.openclaw_sessions_dir.display()
    ));
    report.detail(format!("qmd_bin={}", paths.qmd_bin.display()));
    report.detail(format!("qmd_db={}", paths.qmd_db.display()));

    if !paths.archives_dir.exists() {
        report.issue("missing archives dir (~/.lilac_metaflora/archives)");
    }
    if !paths.memory_dir.exists() {
        report.issue("missing daily memory dir (~/.lilac_metaflora/memory)");
    }
    if !paths.logs_dir.exists() {
        report.issue("missing moon log dir (~/.lilac_metaflora/skills/moon-system/logs)");
    }
    if !paths.memory_file.exists() {
        report.issue("missing long-term memory file (~/.lilac_metaflora/MEMORY.md)");
    }
    if !paths.openclaw_sessions_dir.exists() {
        report.issue("missing OpenClaw sessions dir (~/.openclaw/agents/main/sessions)");
    }
    if !paths.qmd_bin.exists() {
        report.issue("missing qmd binary (~/.bun/bin/qmd or QMD_BIN)");
    }

    Ok(report)
}
