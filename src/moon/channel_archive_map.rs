use crate::moon::paths::MoonPaths;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelArchiveRecord {
    pub channel_key: String,
    pub source_path: String,
    pub archive_path: String,
    pub updated_at_epoch_secs: u64,
}

fn now_secs() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_secs())
}

pub fn map_path(paths: &MoonPaths) -> PathBuf {
    paths
        .moon_home
        .join("continuity")
        .join("channel_archive_map.json")
}

pub fn load(paths: &MoonPaths) -> Result<BTreeMap<String, ChannelArchiveRecord>> {
    let path = map_path(paths);
    if !path.exists() {
        return Ok(BTreeMap::new());
    }

    let raw =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let parsed = serde_json::from_str(&raw)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(parsed)
}

fn save(paths: &MoonPaths, map: &BTreeMap<String, ChannelArchiveRecord>) -> Result<()> {
    let path = map_path(paths);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let data = serde_json::to_string_pretty(map)?;
    fs::write(&path, format!("{data}\n"))
        .with_context(|| format!("failed to write {}", path.display()))?;
    Ok(())
}

pub fn get(paths: &MoonPaths, channel_key: &str) -> Result<Option<ChannelArchiveRecord>> {
    if channel_key.trim().is_empty() {
        return Ok(None);
    }
    let map = load(paths)?;
    Ok(map.get(channel_key).cloned())
}

pub fn upsert(
    paths: &MoonPaths,
    channel_key: &str,
    source_path: &str,
    archive_path: &str,
) -> Result<ChannelArchiveRecord> {
    if channel_key.trim().is_empty() {
        anyhow::bail!("channel key cannot be empty");
    }
    if source_path.trim().is_empty() {
        anyhow::bail!("source path cannot be empty");
    }
    if archive_path.trim().is_empty() {
        anyhow::bail!("archive path cannot be empty");
    }

    let mut map = load(paths)?;
    let record = ChannelArchiveRecord {
        channel_key: channel_key.to_string(),
        source_path: source_path.to_string(),
        archive_path: archive_path.to_string(),
        updated_at_epoch_secs: now_secs()?,
    };
    map.insert(channel_key.to_string(), record.clone());

    save(paths, &map)?;

    Ok(record)
}

pub fn remove_by_archive_paths(
    paths: &MoonPaths,
    archive_paths: &BTreeSet<String>,
) -> Result<usize> {
    if archive_paths.is_empty() {
        return Ok(0);
    }

    let mut map = load(paths)?;
    let before = map.len();
    map.retain(|_, record| !archive_paths.contains(&record.archive_path));
    let removed = before.saturating_sub(map.len());
    if removed > 0 {
        save(paths, &map)?;
    }

    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::moon::paths::MoonPaths;
    use tempfile::tempdir;

    fn test_paths(root: &std::path::Path) -> MoonPaths {
        MoonPaths {
            moon_home: root.join("moon"),
            archives_dir: root.join("moon/archives"),
            memory_dir: root.join("moon/memory"),
            memory_file: root.join("moon/MEMORY.md"),
            logs_dir: root.join("moon/logs"),
            openclaw_sessions_dir: root.join("sessions"),
            qmd_bin: root.join("qmd"),
            qmd_db: root.join("qmd.sqlite"),
        }
    }

    #[test]
    fn upsert_and_get_roundtrip() {
        let tmp = tempdir().expect("tempdir");
        let paths = test_paths(tmp.path());
        fs::create_dir_all(&paths.moon_home).expect("mkdir");

        upsert(
            &paths,
            "agent:main:discord:channel:123",
            "/tmp/source.jsonl",
            "/tmp/archive.jsonl",
        )
        .expect("upsert");

        let got = get(&paths, "agent:main:discord:channel:123")
            .expect("get")
            .expect("some");
        assert_eq!(got.archive_path, "/tmp/archive.jsonl");
        assert_eq!(got.source_path, "/tmp/source.jsonl");
    }

    #[test]
    fn remove_by_archive_paths_removes_matching_entries() {
        let tmp = tempdir().expect("tempdir");
        let paths = test_paths(tmp.path());
        fs::create_dir_all(&paths.moon_home).expect("mkdir");

        upsert(
            &paths,
            "agent:main:discord:channel:1",
            "/tmp/s1.jsonl",
            "/tmp/a1.jsonl",
        )
        .expect("upsert1");
        upsert(
            &paths,
            "agent:main:discord:channel:2",
            "/tmp/s2.jsonl",
            "/tmp/a2.jsonl",
        )
        .expect("upsert2");

        let mut purge = BTreeSet::new();
        purge.insert("/tmp/a1.jsonl".to_string());
        let removed = remove_by_archive_paths(&paths, &purge).expect("remove");
        assert_eq!(removed, 1);
        assert!(
            get(&paths, "agent:main:discord:channel:1")
                .expect("get1")
                .is_none()
        );
        assert!(
            get(&paths, "agent:main:discord:channel:2")
                .expect("get2")
                .is_some()
        );
    }
}
