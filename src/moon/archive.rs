use crate::moon::paths::MoonPaths;
use crate::moon::qmd;
use crate::moon::snapshot::write_snapshot;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveRecord {
    pub session_id: String,
    pub source_path: String,
    pub archive_path: String,
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

fn epoch_now() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_secs())
}

fn ledger_path(paths: &MoonPaths) -> PathBuf {
    paths.archives_dir.join("ledger.jsonl")
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

    let mut indexed = true;
    if let Err(err) =
        qmd::collection_add_or_update(&paths.qmd_bin, &paths.archives_dir, collection_name)
    {
        indexed = false;
        eprintln!("moon archive index warning: {err}");
    }

    let record = ArchiveRecord {
        session_id: source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("session")
            .to_string(),
        source_path: write.source_path.display().to_string(),
        archive_path: write.archive_path.display().to_string(),
        content_hash: archive_hash,
        created_at_epoch_secs: epoch_now()?,
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
