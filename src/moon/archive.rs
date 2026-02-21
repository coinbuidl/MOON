use crate::moon::distill::{extract_projection_data, ProjectionData};
use crate::moon::paths::MoonPaths;
use crate::moon::qmd;
use crate::moon::snapshot::write_snapshot;
use crate::moon::warn::{self, WarnEvent};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveRecord {
    pub session_id: String,
    pub source_path: String,
    pub archive_path: String,
    pub projection_path: Option<String>,
    pub content_hash: String,
    pub created_at_epoch_secs: u64,
    pub indexed_collection: String,
    pub indexed: bool,
}

#[derive(Debug, Clone)]
pub struct ArchivePipelineOutcome {
    pub record: ArchiveRecord,
    pub deduped: bool,
    pub ledger_path: PathBuf,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ProjectionBackfillOutcome {
    pub scanned: usize,
    pub created: usize,
    pub failed: usize,
    pub ledger_updated: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ArchiveLayoutMigrationOutcome {
    pub scanned: usize,
    pub moved: usize,
    pub missing: usize,
    pub failed: usize,
    pub ledger_updated: bool,
    pub path_rewrites: BTreeMap<String, String>,
}

fn epoch_now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_secs())
}

fn ledger_path(paths: &MoonPaths) -> PathBuf {
    paths.archives_dir.join("ledger.jsonl")
}

pub fn projection_path_for_archive_path(archive_path: &Path) -> PathBuf {
    archive_path.with_extension("md")
}

pub fn projection_path_for_archive(archive_path: &str) -> PathBuf {
    projection_path_for_archive_path(Path::new(archive_path))
}

fn raw_archives_dir(paths: &MoonPaths) -> PathBuf {
    paths.archives_dir.join("raw")
}

fn move_file(from: &Path, to: &Path) -> Result<()> {
    if from == to {
        return Ok(());
    }
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    match fs::rename(from, to) {
        Ok(_) => Ok(()),
        Err(rename_err) => {
            if matches!(
                rename_err.kind(),
                ErrorKind::CrossesDevices | ErrorKind::PermissionDenied
            ) {
                fs::copy(from, to).with_context(|| {
                    format!("failed to copy {} to {}", from.display(), to.display())
                })?;
                fs::remove_file(from)
                    .with_context(|| format!("failed to remove {}", from.display()))?;
                Ok(())
            } else {
                Err(rename_err).with_context(|| {
                    format!("failed to move {} to {}", from.display(), to.display())
                })
            }
        }
    }
}

fn file_hash(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn read_ledger(path: &Path) -> Result<Vec<ArchiveRecord>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let entry: ArchiveRecord = serde_json::from_str(trimmed)
            .with_context(|| format!("failed to parse ledger line in {}", path.display()))?;
        out.push(entry);
    }
    Ok(out)
}

fn yaml_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn truncate_preview(text: &str, max: usize) -> String {
    let clean: String = text.chars().filter(|c| !c.is_control()).collect();
    if clean.chars().count() > max {
        let mut s: String = clean.chars().take(max).collect();
        s.push_str("...");
        s
    } else {
        clean
    }
}

fn render_projection_markdown_v2(
    session_id: &str,
    source_path: &Path,
    archive_path: &Path,
    content_hash: &str,
    created_at_epoch_secs: u64,
    data: &ProjectionData,
) -> String {
    use chrono::{DateTime, Utc, TimeZone, Local};

    let mut out = String::new();
    out.push_str("---\n");
    out.push_str("moon_archive_projection: 2\n");
    out.push_str(&format!("session_id: {}\n", yaml_quote(session_id)));
    out.push_str(&format!(
        "source_path: {}\n",
        yaml_quote(&source_path.display().to_string())
    ));
    out.push_str(&format!(
        "archive_jsonl_path: {}\n",
        yaml_quote(&archive_path.display().to_string())
    ));
    out.push_str(&format!("content_hash: {}\n", yaml_quote(content_hash)));
    out.push_str(&format!("created_at_epoch_secs: {created_at_epoch_secs}\n"));

    let start_utc = data.time_start_epoch.map(|t| Utc.timestamp_opt(t as i64, 0).unwrap()).unwrap_or_else(Utc::now);
    let end_utc = data.time_end_epoch.map(|t| Utc.timestamp_opt(t as i64, 0).unwrap()).unwrap_or_else(Utc::now);
    
    let local_offset = std::env::var("MOON_LOCAL_TIMEZONE").unwrap_or_else(|_| Local::now().offset().to_string());
    
    let start_local: DateTime<Local> = start_utc.with_timezone(&Local);
    let end_local: DateTime<Local> = end_utc.with_timezone(&Local);

    out.push_str(&format!("time_range_utc: \"{} — {}\"\n", start_utc.format("%Y-%m-%dT%H:%M:%SZ"), end_utc.format("%Y-%m-%dT%H:%M:%SZ")));
    out.push_str(&format!("time_range_local: \"{} — {}\"\n", start_local.format("%Y-%m-%dT%H:%M:%S%:z"), end_local.format("%Y-%m-%dT%H:%M:%S%:z")));
    out.push_str(&format!("local_timezone: {}\n", yaml_quote(&local_offset)));
    out.push_str(&format!("message_count: {}\n", data.entries.len()));
    
    let tools_str = serde_json::to_string(&data.tool_calls).unwrap_or_else(|_| "[]".to_string());
    out.push_str(&format!("tool_calls: {}\n", tools_str));
    
    let keywords_str = serde_json::to_string(&data.keywords).unwrap_or_else(|_| "[]".to_string());
    out.push_str(&format!("keywords: {}\n", keywords_str));
    
    let topics_str = serde_json::to_string(&data.topics).unwrap_or_else(|_| "[]".to_string());
    out.push_str(&format!("topics: {}\n", topics_str));
    
    out.push_str("---\n\n");
    
    out.push_str(&format!("# Archive Projection — {}\n\n", session_id));
    out.push_str(&format!("> Session: {}–{} {} ({}–{} UTC)\n", start_local.format("%Y-%m-%d %H:%M"), end_local.format("%H:%M"), local_offset, start_utc.format("%Y-%m-%d %H:%M"), end_utc.format("%H:%M")));
    out.push_str(&format!("> Messages: {} | Tools used: {}\n\n", data.entries.len(), data.tool_calls.join(", ")));
    
    out.push_str("## Timeline\n\n");
    out.push_str("| # | Time (UTC) | Time (Local) | Role | Summary |\n");
    out.push_str("|---|---|---|---|---|\n");
    
    let mut convs_user = String::new();
    let mut convs_asst = String::new();
    let mut tool_sections: std::collections::BTreeMap<String, Vec<String>> = std::collections::BTreeMap::new();
    
    for (i, entry) in data.entries.iter().enumerate() {
        let ts_utc = entry.timestamp_epoch.map(|t| Utc.timestamp_opt(t as i64, 0).unwrap()).unwrap_or_else(Utc::now);
        let ts_local: DateTime<Local> = ts_utc.with_timezone(&Local);
        let time_str_utc = ts_utc.format("%H:%M:%SZ").to_string();
        let time_str_local = ts_local.format("%H:%M:%S").to_string();
        
        let preview = truncate_preview(&entry.content, 60);
        
        // NL injection every 15 entries for Lilac §4
        if i > 0 && i % 15 == 0 {
            let nl_time = ts_local.format("%A %p").to_string();
            out.push_str(&format!("| - | **[{}]** | - | - | - |\n", nl_time));
        }

        let role_display = if let Some(ref tool) = entry.tool_name {
            format!("tool:{}", tool)
        } else {
            entry.role.clone()
        };
        out.push_str(&format!("| {} | {} | {} | {} | {} |\n", i + 1, time_str_utc, time_str_local, role_display, preview));
        
        let conv_line = format!("- [{}] {}\n", time_str_utc, preview);
        if entry.role == "user" {
            convs_user.push_str(&conv_line);
        } else if entry.role == "assistant" {
            convs_asst.push_str(&format!("- [{}] {}\n", time_str_utc, truncate_preview(&entry.content, 120)));
        }
        
        if let Some(ref tool) = entry.tool_name {
            let list = tool_sections.entry(tool.clone()).or_default();
            let target = entry.tool_target.as_deref().unwrap_or("");
            let result_preview = entry.coupled_result.as_deref().map(|r| truncate_preview(r, 60)).unwrap_or_default();
            // Contextual stitching (Lilac §3)
            list.push(format!("- [{}] `{}` → {}\n", time_str_utc, target, result_preview));
        } else if entry.role == "toolResult" && entry.coupled_result.is_none() {
            let list = tool_sections.entry("unknown_tool".to_string()).or_default();
            list.push(format!("- [{}] {}\n", time_str_utc, preview));
        }
    }
    
    out.push_str("\n## Conversations\n\n### User Queries\n");
    if convs_user.is_empty() { out.push_str("- None\n"); } else { out.push_str(&convs_user); }
    out.push_str("\n### Assistant Responses\n");
    if convs_asst.is_empty() { out.push_str("- None\n"); } else { out.push_str(&convs_asst); }
    
    out.push_str("\n## Tool Activity\n\n");
    if tool_sections.is_empty() {
        out.push_str("- None\n");
    } else {
        for (tool, acts) in tool_sections {
            out.push_str(&format!("### {}\n", tool));
            for act in acts {
                out.push_str(&act);
            }
            out.push('\n');
        }
    }
    
    out.push_str("## Decisions & Outcomes\n- (Extracted via periodic compaction)\n\n");
    
    out.push_str("## Keywords & Topics\n");
    out.push_str(&format!("- **Keywords**: {}\n", data.keywords.join(", ")));
    out.push_str(&format!("- **Topics**: {}\n\n", data.topics.join(", ")));
    
    out.push_str("## Compaction Notes\n");
    if data.compaction_anchors.is_empty() {
        out.push_str("- No compactions recorded in this session.\n");
    } else {
        for anchor in &data.compaction_anchors {
            let origin_ref = anchor.origin_message_id.as_deref().unwrap_or("unknown");
            out.push_str(&format!("- {} (Origin: `{}`)\n", anchor.note, origin_ref));
        }
    }
    
    out
}

fn write_archive_projection(
    session_id: &str,
    source_path: &Path,
    archive_path: &Path,
    content_hash: &str,
    created_at_epoch_secs: u64,
) -> Result<PathBuf> {
    let projection_path = projection_path_for_archive_path(archive_path);
    let archive_path_str = archive_path.display().to_string();
    let proj_data = extract_projection_data(&archive_path_str)
        .with_context(|| format!("failed to extract projection data from {}", archive_path.display()))?;
    
    let markdown = render_projection_markdown_v2(
        session_id,
        source_path,
        archive_path,
        content_hash,
        created_at_epoch_secs,
        &proj_data,
    );
    
    fs::write(&projection_path, markdown)
        .with_context(|| format!("failed to write {}", projection_path.display()))?;
    Ok(projection_path)
}

pub fn read_ledger_records(paths: &MoonPaths) -> Result<Vec<ArchiveRecord>> {
    read_ledger(&ledger_path(paths))
}

fn append_ledger(path: &Path, record: &ArchiveRecord) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let line = format!("{}\n", serde_json::to_string(record)?);
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(line.as_bytes())?;
    Ok(())
}

fn write_ledger(path: &Path, records: &[ArchiveRecord]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let mut out = String::new();
    for record in records {
        out.push_str(&serde_json::to_string(record)?);
        out.push('\n');
    }
    fs::write(path, out).with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn normalize_archive_layout(paths: &MoonPaths) -> Result<ArchiveLayoutMigrationOutcome> {
    let ledger = ledger_path(paths);
    if !ledger.exists() {
        return Ok(ArchiveLayoutMigrationOutcome::default());
    }

    let mut records = read_ledger(&ledger)?;
    if records.is_empty() {
        return Ok(ArchiveLayoutMigrationOutcome::default());
    }

    let raw_dir = raw_archives_dir(paths);
    fs::create_dir_all(&raw_dir)
        .with_context(|| format!("failed to create {}", raw_dir.display()))?;

    let mut out = ArchiveLayoutMigrationOutcome::default();
    let mut changed = false;

    for record in &mut records {
        out.scanned += 1;

        let old_archive = PathBuf::from(&record.archive_path);
        if old_archive
            .parent()
            .is_some_and(|parent| parent == raw_dir.as_path())
        {
            continue;
        }

        let Some(file_name) = old_archive.file_name().map(|v| v.to_owned()) else {
            out.failed += 1;
            continue;
        };

        if !old_archive.exists() {
            out.missing += 1;
            continue;
        }

        let target_archive = raw_dir.join(file_name);
        if target_archive != old_archive {
            if target_archive.exists() {
                let from_hash = file_hash(&old_archive)?;
                let to_hash = file_hash(&target_archive)?;
                if from_hash == to_hash {
                    fs::remove_file(&old_archive)
                        .with_context(|| format!("failed to remove {}", old_archive.display()))?;
                } else {
                    out.failed += 1;
                    continue;
                }
            } else {
                move_file(&old_archive, &target_archive)?;
            }

            let old_archive_str = record.archive_path.clone();
            let new_archive_str = target_archive.display().to_string();
            if old_archive_str != new_archive_str {
                record.archive_path = new_archive_str.clone();
                out.path_rewrites.insert(old_archive_str, new_archive_str);
                out.moved += 1;
                changed = true;
            }
        }

        let old_projection = record
            .projection_path
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| projection_path_for_archive_path(&old_archive));
        let new_projection = projection_path_for_archive_path(Path::new(&record.archive_path));

        if old_projection.exists() {
            if old_projection != new_projection {
                if new_projection.exists() {
                    let from_hash = file_hash(&old_projection)?;
                    let to_hash = file_hash(&new_projection)?;
                    if from_hash == to_hash {
                        fs::remove_file(&old_projection).with_context(|| {
                            format!("failed to remove {}", old_projection.display())
                        })?;
                    } else {
                        out.failed += 1;
                        continue;
                    }
                } else {
                    move_file(&old_projection, &new_projection)?;
                }
            }

            let projection_str = new_projection.display().to_string();
            if record.projection_path.as_deref() != Some(projection_str.as_str()) {
                record.projection_path = Some(projection_str);
                changed = true;
            }
        } else if record.projection_path.is_some() {
            record.projection_path = None;
            changed = true;
        }
    }

    if changed {
        write_ledger(&ledger, &records)?;
        out.ledger_updated = true;
    }

    Ok(out)
}

pub fn backfill_archive_projections(paths: &MoonPaths, reproject: bool) -> Result<ProjectionBackfillOutcome> {
    let ledger = ledger_path(paths);
    if !ledger.exists() {
        return Ok(ProjectionBackfillOutcome::default());
    }

    let mut records = read_ledger(&ledger)?;
    if records.is_empty() {
        return Ok(ProjectionBackfillOutcome::default());
    }

    let mut out = ProjectionBackfillOutcome::default();
    let mut changed = false;

    let mut tracked_archives = BTreeSet::new();

    for record in &mut records {
        out.scanned += 1;
        tracked_archives.insert(record.archive_path.clone());

        let archive_path = Path::new(&record.archive_path);
        if !archive_path.exists() {
            continue;
        }

        if !reproject
            && let Some(existing_projection) = record
                .projection_path
                .as_deref()
                .map(PathBuf::from)
                .filter(|path| path.exists())
            {
                let normalized = existing_projection.display().to_string();
                if record.projection_path.as_deref() != Some(normalized.as_str()) {
                    record.projection_path = Some(normalized);
                    changed = true;
                }
                continue;
            }


        match write_archive_projection(
            &record.session_id,
            Path::new(&record.source_path),
            archive_path,
            &record.content_hash,
            record.created_at_epoch_secs,
        ) {
            Ok(path) => {
                out.created += 1;
                record.projection_path = Some(path.display().to_string());
                changed = true;
            }
            Err(_) => {
                out.failed += 1;
            }
        }
    }

    let raw_dir = raw_archives_dir(paths);
    if raw_dir.exists() {
        for entry in fs::read_dir(&raw_dir)? {
            let path = entry?.path();
            if !path.is_file() {
                continue;
            }

            let Some(ext) = path.extension().and_then(|v| v.to_str()) else {
                continue;
            };
            if ext != "json" && ext != "jsonl" {
                continue;
            }

            let archive_path = path.display().to_string();
            if tracked_archives.contains(&archive_path) {
                continue;
            }

            out.scanned += 1;
            let projection_path = projection_path_for_archive_path(&path);
            if projection_path.exists() {
                continue;
            }

            let session_id = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("session")
                .to_string();

            let content_hash = match file_hash(&path) {
                Ok(hash) => hash,
                Err(_) => {
                    out.failed += 1;
                    continue;
                }
            };
            let created_at_epoch_secs = epoch_now().unwrap_or(0);
            match write_archive_projection(
                &session_id,
                Path::new(&archive_path),
                &path,
                &content_hash,
                created_at_epoch_secs,
            ) {
                Ok(_) => {
                    out.created += 1;
                }
                Err(_) => {
                    out.failed += 1;
                }
            }
        }
    }

    if changed {
        write_ledger(&ledger, &records)?;
        out.ledger_updated = true;
    }

    Ok(out)
}

pub fn remove_ledger_records(paths: &MoonPaths, archive_paths: &BTreeSet<String>) -> Result<usize> {
    if archive_paths.is_empty() {
        return Ok(0);
    }

    let ledger = ledger_path(paths);
    if !ledger.exists() {
        return Ok(0);
    }

    let existing = read_ledger(&ledger)?;
    let existing_len = existing.len();
    let kept = existing
        .into_iter()
        .filter(|r| !archive_paths.contains(&r.archive_path))
        .collect::<Vec<_>>();
    let removed = existing_len.saturating_sub(kept.len());
    if removed == 0 {
        return Ok(0);
    }

    write_ledger(&ledger, &kept)?;
    Ok(removed)
}

pub fn archive_and_index(
    paths: &MoonPaths,
    source: &Path,
    collection_name: &str,
) -> Result<ArchivePipelineOutcome> {
    fs::create_dir_all(&paths.archives_dir)
        .with_context(|| format!("failed to create {}", paths.archives_dir.display()))?;

    let ledger = ledger_path(paths);
    let source_hash = file_hash(source)?;
    let existing = read_ledger(&ledger)?;

    if let Some(record) = existing
        .iter()
        .find(|r| r.content_hash == source_hash && r.source_path == source.display().to_string())
    {
        return Ok(ArchivePipelineOutcome {
            record: record.clone(),
            deduped: true,
            ledger_path: ledger,
        });
    }

    let write = write_snapshot(&paths.archives_dir, source)?;
    let archive_hash = file_hash(&write.archive_path)?;
    let session_id = source
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("session")
        .to_string();
    let created_at_epoch_secs = epoch_now()?;
    let projection_path = match write_archive_projection(
        &session_id,
        &write.source_path,
        &write.archive_path,
        &archive_hash,
        created_at_epoch_secs,
    ) {
        Ok(path) => Some(path),
        Err(err) => {
            warn::emit(WarnEvent {
                code: "PROJECTION_WRITE_FAILED",
                stage: "archive",
                action: "write-projection-md",
                session: &session_id,
                archive: &write.archive_path.display().to_string(),
                source: &write.source_path.display().to_string(),
                retry: "retry-next-cycle",
                reason: "projection-write-failed",
                err: &format!("{err:#}"),
            });
            None
        }
    };

    let mut indexed = projection_path.is_some();
    if let Err(err) =
        qmd::collection_add_or_update(&paths.qmd_bin, &paths.archives_dir, collection_name)
    {
        indexed = false;
        warn::emit(WarnEvent {
            code: "INDEX_FAILED",
            stage: "qmd-index",
            action: "archive-index",
            session: source
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("session"),
            archive: &write.archive_path.display().to_string(),
            source: &write.source_path.display().to_string(),
            retry: "retry-next-cycle",
            reason: "qmd-collection-add-or-update-failed",
            err: &format!("{err:#}"),
        });
        eprintln!("moon archive index warning: {err}");
    }

    let record = ArchiveRecord {
        session_id,
        source_path: write.source_path.display().to_string(),
        archive_path: write.archive_path.display().to_string(),
        projection_path: projection_path.map(|p| p.display().to_string()),
        content_hash: archive_hash,
        created_at_epoch_secs,
        indexed_collection: collection_name.to_string(),
        indexed,
    };

    append_ledger(&ledger, &record)?;

    Ok(ArchivePipelineOutcome {
        record,
        deduped: false,
        ledger_path: ledger,
    })
}
