use anyhow::Result;

use crate::commands::CommandReport;
use crate::moon::archive::{backfill_archive_projections, normalize_archive_layout};
use crate::moon::channel_archive_map;
use crate::moon::paths::resolve_paths;
use crate::moon::qmd;
use crate::moon::qmd::CollectionSyncResult;
use crate::moon::state;

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
        report.detail(
            "dry-run: qmd collection add planned (with update fallback on existing collection)"
                .to_string(),
        );
        return Ok(report);
    }

    let layout = normalize_archive_layout(&paths)?;
    report.detail(format!("layout_migration.scanned={}", layout.scanned));
    report.detail(format!("layout_migration.moved={}", layout.moved));
    report.detail(format!("layout_migration.missing={}", layout.missing));
    report.detail(format!("layout_migration.failed={}", layout.failed));
    report.detail(format!(
        "layout_migration.ledger_updated={}",
        layout.ledger_updated
    ));
    report.detail(format!(
        "layout_migration.path_rewrites={}",
        layout.path_rewrites.len()
    ));

    if !layout.path_rewrites.is_empty() {
        let channel_map_updates =
            channel_archive_map::rewrite_archive_paths(&paths, &layout.path_rewrites)?;
        report.detail(format!(
            "layout_migration.channel_map_updates={}",
            channel_map_updates
        ));

        let state_updates = state::rewrite_distilled_archive_paths(&paths, &layout.path_rewrites)?;
        report.detail(format!("layout_migration.state_updates={}", state_updates));
    }

    let backfill = backfill_archive_projections(&paths)?;
    report.detail(format!("projection_backfill.scanned={}", backfill.scanned));
    report.detail(format!("projection_backfill.created={}", backfill.created));
    report.detail(format!("projection_backfill.failed={}", backfill.failed));
    report.detail(format!(
        "projection_backfill.ledger_updated={}",
        backfill.ledger_updated
    ));
    if backfill.failed > 0 {
        report.issue("some archive projections failed to build; check archive readability");
    }

    match qmd::collection_add_or_update(&paths.qmd_bin, &paths.archives_dir, &opts.collection_name)?
    {
        CollectionSyncResult::Added => report.detail("qmd collection add completed".to_string()),
        CollectionSyncResult::Updated => {
            report.detail("qmd update completed (collection already existed)".to_string())
        }
        CollectionSyncResult::Recreated => report
            .detail("qmd collection recreated with latest archive projection mask".to_string()),
    }

    Ok(report)
}
