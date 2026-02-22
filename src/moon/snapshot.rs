use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone)]
pub struct SnapshotOutcome {
    pub source_path: PathBuf,
    pub archive_path: PathBuf,
    pub bytes: usize,
}

fn is_session_snapshot_candidate(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
        return false;
    };

    let lower_name = file_name.to_ascii_lowercase();
    if lower_name.ends_with(".lock")
        || lower_name.ends_with(".tmp")
        || lower_name.ends_with(".swp")
        || lower_name.ends_with(".part")
    {
        return false;
    }

    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase());

    match ext.as_deref() {
        Some("jsonl") => true,
        Some("json") => lower_name != "sessions.json",
        _ => false,
    }
}

fn sanitize_slug(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut prev_dash = false;
    for ch in input.chars() {
        let keep = ch.is_ascii_alphanumeric();
        if keep {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn epoch_seconds_string() -> Result<String> {
    let secs = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_secs();
    Ok(secs.to_string())
}

pub fn latest_session_file(dir: &Path) -> Result<Option<PathBuf>> {
    let mut latest: Option<(std::time::SystemTime, PathBuf)> = None;
    let read_dir =
        fs::read_dir(dir).with_context(|| format!("failed to read {}", dir.display()))?;

    for entry in read_dir {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if !is_session_snapshot_candidate(&path) {
            continue;
        }
        let meta = entry.metadata()?;
        let modified = meta.modified().unwrap_or(UNIX_EPOCH);
        match &latest {
            Some((best, _)) if modified <= *best => {}
            _ => latest = Some((modified, path)),
        }
    }

    Ok(latest.map(|(_, p)| p))
}

pub fn write_snapshot(archives_dir: &Path, source_path: &Path) -> Result<SnapshotOutcome> {
    fs::create_dir_all(archives_dir)
        .with_context(|| format!("failed to create {}", archives_dir.display()))?;
    let raw_archives_dir = archives_dir.join("raw");
    fs::create_dir_all(&raw_archives_dir)
        .with_context(|| format!("failed to create {}", raw_archives_dir.display()))?;

    let raw = fs::read(source_path)
        .with_context(|| format!("failed to read source session {}", source_path.display()))?;

    let ext = source_path
        .extension()
        .and_then(|s| s.to_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("json");

    let source_stem = source_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("session");
    let slug = sanitize_slug(source_stem);
    let stamp = epoch_seconds_string()?;

    let filename = if slug.is_empty() {
        format!("snapshot-{stamp}.{ext}")
    } else {
        format!("{slug}-{stamp}.{ext}")
    };
    let archive_path = raw_archives_dir.join(filename);

    fs::write(&archive_path, &raw)
        .with_context(|| format!("failed to write {}", archive_path.display()))?;

    Ok(SnapshotOutcome {
        source_path: source_path.to_path_buf(),
        archive_path,
        bytes: raw.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::{is_session_snapshot_candidate, sanitize_slug};
    use std::path::Path;

    #[test]
    fn slug_sanitization_is_stable() {
        assert_eq!(sanitize_slug("Main Session #1"), "main-session-1");
        assert_eq!(sanitize_slug("---"), "");
        assert_eq!(sanitize_slug("abc___def"), "abc-def");
    }

    #[test]
    fn snapshot_candidate_filter_excludes_non_session_files() {
        assert!(is_session_snapshot_candidate(Path::new(
            "/tmp/abc-123.jsonl"
        )));
        assert!(is_session_snapshot_candidate(Path::new(
            "/tmp/abc-123.json"
        )));
        assert!(!is_session_snapshot_candidate(Path::new(
            "/tmp/sessions.json"
        )));
        assert!(!is_session_snapshot_candidate(Path::new(
            "/tmp/abc-123.jsonl.lock"
        )));
        assert!(!is_session_snapshot_candidate(Path::new(
            "/tmp/abc-123.lock"
        )));
        assert!(!is_session_snapshot_candidate(Path::new("/tmp/abc-123.md")));
    }
}
