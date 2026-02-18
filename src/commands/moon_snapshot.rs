use anyhow::Result;
use std::path::PathBuf;

use crate::commands::CommandReport;
use crate::moon::paths::resolve_paths;
use crate::moon::snapshot::{latest_session_file, write_snapshot};

#[derive(Debug, Clone, Default)]
pub struct MoonSnapshotOptions {
    pub source: Option<PathBuf>,
    pub dry_run: bool,
}

pub fn run(opts: &MoonSnapshotOptions) -> Result<CommandReport> {
    let paths = resolve_paths()?;
    let mut report = CommandReport::new("moon-snapshot");

    let source = match &opts.source {
        Some(path) => path.clone(),
        None => {
            let Some(path) = latest_session_file(&paths.openclaw_sessions_dir)? else {
                report.issue("no source session file found in openclaw sessions dir");
                return Ok(report);
            };
            path
        }
    };

    report.detail(format!("source={}", source.display()));
    report.detail(format!("archives_dir={}", paths.archives_dir.display()));

    if opts.dry_run {
        report.detail("dry-run: snapshot planned but not written".to_string());
        return Ok(report);
    }

    let outcome = write_snapshot(&paths.archives_dir, &source)?;
    report.detail(format!(
        "source_confirmed={}",
        outcome.source_path.display()
    ));
    report.detail(format!("archive={}", outcome.archive_path.display()));
    report.detail(format!("bytes={}", outcome.bytes));

    Ok(report)
}
