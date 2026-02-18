use anyhow::Result;

use crate::commands::CommandReport;
use crate::moon::paths::resolve_paths;
use crate::moon::recall;

#[derive(Debug, Clone)]
pub struct MoonRecallOptions {
    pub query: String,
    pub collection_name: String,
}

pub fn run(opts: &MoonRecallOptions) -> Result<CommandReport> {
    let paths = resolve_paths()?;
    let mut report = CommandReport::new("moon-recall");

    if opts.query.trim().is_empty() {
        report.issue("query cannot be empty");
        return Ok(report);
    }

    let result = recall::recall(&paths, &opts.query, &opts.collection_name)?;
    report.detail(format!("query={}", result.query));
    report.detail(format!("collection={}", opts.collection_name));
    report.detail(format!("match_count={}", result.matches.len()));
    for (idx, m) in result.matches.iter().take(5).enumerate() {
        report.detail(format!("match[{idx}].score={:.4}", m.score));
        report.detail(format!("match[{idx}].archive={}", m.archive_path));
        if !m.snippet.is_empty() {
            report.detail(format!(
                "match[{idx}].snippet={}",
                m.snippet.replace('\n', " ")
            ));
        }
    }

    Ok(report)
}
