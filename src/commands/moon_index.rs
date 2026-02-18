use anyhow::Result;

use crate::commands::CommandReport;
use crate::moon::paths::resolve_paths;
use crate::moon::qmd;

#[derive(Debug, Clone)]
pub struct MoonIndexOptions {
    pub collection_name: String,
    pub dry_run: bool,
}

pub fn run(opts: &MoonIndexOptions) -> Result<CommandReport> {
    let paths = resolve_paths()?;
    let mut report = CommandReport::new("moon-index");

    report.detail(format!("archives_dir={}", paths.archives_dir.display()));
    report.detail(format!("qmd_bin={}", paths.qmd_bin.display()));
    report.detail(format!("collection_name={}", opts.collection_name));

    if !paths.archives_dir.exists() {
        report.issue("archives dir does not exist");
        return Ok(report);
    }

    if opts.dry_run {
        report.detail("dry-run: qmd collection add planned but not executed".to_string());
        return Ok(report);
    }

    qmd::collection_add(&paths.qmd_bin, &paths.archives_dir, &opts.collection_name)?;
    report.detail("qmd collection add completed".to_string());

    Ok(report)
}
