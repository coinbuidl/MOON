use crate::moon::archive::projection_path_for_archive;
use crate::moon::channel_archive_map;
use crate::moon::paths::MoonPaths;
use crate::moon::qmd;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallMatch {
    pub archive_path: String,
    pub snippet: String,
    pub score: f64,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallResult {
    pub query: String,
    pub matches: Vec<RecallMatch>,
    pub generated_at_epoch_secs: u64,
}

fn now_secs() -> Result<u64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs())
}

fn parse_matches(raw: &str) -> Vec<RecallMatch> {
    let mut out = Vec::new();
    let parsed = serde_json::from_str::<Value>(raw);
    let Ok(v) = parsed else {
        return out;
    };

    let items = v
        .as_array()
        .cloned()
        .or_else(|| v.get("results").and_then(Value::as_array).cloned())
        .unwrap_or_default();

    for item in items {
        let snippet = item
            .get("snippet")
            .and_then(Value::as_str)
            .or_else(|| item.get("text").and_then(Value::as_str))
            .unwrap_or("")
            .to_string();
        let archive_path = item
            .get("path")
            .and_then(Value::as_str)
            .or_else(|| item.get("source").and_then(Value::as_str))
            .unwrap_or("")
            .to_string();
        let score = item
            .get("score")
            .and_then(Value::as_f64)
            .unwrap_or_else(|| (snippet.len() as f64) / 1000.0);

        out.push(RecallMatch {
            archive_path,
            snippet,
            score,
            metadata: item,
        });
    }

    out.sort_by(|a, b| b.score.total_cmp(&a.score));
    out
}

fn snippet_from_archive(path: &str) -> String {
    let projection_path = projection_path_for_archive(path);
    let projection_path_str = projection_path.to_string_lossy().to_string();
    let projection = fs::read_to_string(&projection_path_str).ok();
    if let Some(raw) = projection {
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed.starts_with("---")
                || trimmed.starts_with('#')
                || trimmed.starts_with("moon_archive_projection:")
                || trimmed.starts_with("session_id:")
                || trimmed.starts_with("source_path:")
                || trimmed.starts_with("archive_jsonl_path:")
                || trimmed.starts_with("content_hash:")
                || trimmed.starts_with("created_at_epoch_secs:")
                || trimmed.eq_ignore_ascii_case("this file stores non-noise text signals extracted from the raw session archive for retrieval.")
            {
                continue;
            }

            let normalized = trimmed.trim_start_matches("- ").trim();
            if normalized.is_empty() {
                continue;
            }

            return normalized.chars().take(280).collect();
        }
    }

    let Ok(raw) = fs::read_to_string(path) else {
        return String::new();
    };

    raw.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .chars()
        .take(280)
        .collect()
}

pub fn recall(
    paths: &MoonPaths,
    query: &str,
    collection_name: &str,
    channel_key: Option<&str>,
) -> Result<RecallResult> {
    let mut matches = Vec::new();

    let key_hint = channel_key.or_else(|| {
        let trimmed = query.trim();
        if trimmed.starts_with("agent:") {
            Some(trimmed)
        } else {
            None
        }
    });

    if let Some(key) = key_hint
        && let Some(record) = channel_archive_map::get(paths, key)?
    {
        matches.push(RecallMatch {
            archive_path: record.archive_path.clone(),
            snippet: snippet_from_archive(&record.archive_path),
            score: 1_000_000.0,
            metadata: json!({
                "deterministic": true,
                "channelKey": record.channel_key,
                "sourcePath": record.source_path,
                "projectionPath": projection_path_for_archive(&record.archive_path).display().to_string(),
                "updatedAtEpochSecs": record.updated_at_epoch_secs,
            }),
        });
    }

    let raw = qmd::search(&paths.qmd_bin, collection_name, query)?;
    matches.extend(parse_matches(&raw));

    let mut deduped = Vec::with_capacity(matches.len());
    let mut seen_paths = BTreeSet::new();
    for item in matches {
        if item.archive_path.trim().is_empty() {
            deduped.push(item);
            continue;
        }
        if seen_paths.insert(item.archive_path.clone()) {
            deduped.push(item);
        }
    }

    deduped.sort_by(|a, b| b.score.total_cmp(&a.score));

    Ok(RecallResult {
        query: query.to_string(),
        matches: deduped,
        generated_at_epoch_secs: now_secs()?,
    })
}
