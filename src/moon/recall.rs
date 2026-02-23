use crate::moon::archive::projection_path_for_archive;
use crate::moon::channel_archive_map;
use crate::moon::paths::MoonPaths;
use crate::moon::qmd;
use crate::moon::util::now_epoch_secs;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

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

fn boost_score_for_priority(snippet: &str, base_score: f64) -> f64 {
    let lower = snippet.to_ascii_lowercase();
    if lower.contains("write_to_file")
        || lower.contains("exec")
        || lower.contains("edit")
        || lower.contains("gateway")
    {
        // High priority side-effects
        base_score * 1.30
    } else if lower.contains("read_file") || lower.contains("web_search") || lower.contains("ls") {
        // Normal priority side-effects
        base_score * 1.05
    } else {
        base_score
    }
}

fn archive_path_from_projection_path(path: &Path) -> PathBuf {
    let Some(file_name) = path.file_name() else {
        return path.with_extension("jsonl");
    };
    if path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "mlib" || name == "lib")
        && let Some(archives_root) = path.parent().and_then(Path::parent)
    {
        let mut archive_name = PathBuf::from(file_name);
        archive_name.set_extension("jsonl");
        return archives_root.join("raw").join(archive_name);
    }
    path.with_extension("jsonl")
}

fn normalize_archive_path(candidate: &str) -> String {
    let trimmed = candidate.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if trimmed.starts_with("qmd://") {
        return trimmed
            .strip_suffix(".md")
            .map(|v| format!("{v}.jsonl"))
            .unwrap_or_else(|| trimmed.to_string());
    }
    if Path::new(trimmed)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
    {
        return archive_path_from_projection_path(Path::new(trimmed))
            .display()
            .to_string();
    }
    trimmed.to_string()
}

fn resolve_archive_path(paths: &MoonPaths, item: &Value) -> String {
    if let Some(path) = item.get("path").and_then(Value::as_str) {
        return normalize_archive_path(path);
    }
    if let Some(source) = item.get("source").and_then(Value::as_str) {
        return normalize_archive_path(source);
    }
    if let Some(file) = item.get("file").and_then(Value::as_str) {
        if let Some(uri_body) = file.strip_prefix("qmd://") {
            let mut parts = uri_body.splitn(2, '/');
            let _collection = parts.next();
            if let Some(relative_path) = parts.next() {
                let local_projection = paths.archives_dir.join(relative_path);
                return archive_path_from_projection_path(&local_projection)
                    .display()
                    .to_string();
            }
        }
        return normalize_archive_path(file);
    }
    String::new()
}

fn parse_matches(paths: &MoonPaths, raw: &str) -> Vec<RecallMatch> {
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
        let archive_path = resolve_archive_path(paths, &item);
        let base_score = item
            .get("score")
            .and_then(Value::as_f64)
            .unwrap_or_else(|| (snippet.len() as f64) / 1000.0);

        let score = boost_score_for_priority(&snippet, base_score);

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
        let mut in_v2_content = false;
        let mut fallback = String::new();
        for line in raw.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if trimmed == "## Conversations"
                || trimmed == "## Timeline"
                || trimmed == "## Tool Activity"
            {
                in_v2_content = true;
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
                || trimmed.starts_with("time_range_utc:")
                || trimmed.starts_with("time_range_local:")
                || trimmed.starts_with("local_timezone:")
                || trimmed.starts_with("message_count:")
                || trimmed.starts_with("tool_calls:")
                || trimmed.starts_with("keywords:")
                || trimmed.starts_with("topics:")
                || trimmed.starts_with('>')
                || trimmed.eq_ignore_ascii_case("this file stores non-noise text signals extracted from the raw session archive for retrieval.")
            {
                continue;
            }

            let normalized = trimmed.trim_start_matches("- ").trim();
            if normalized.is_empty() {
                continue;
            }

            if fallback.is_empty() {
                fallback = normalized.chars().take(280).collect();
            }

            if in_v2_content && !normalized.starts_with('|') {
                return normalized.chars().take(280).collect();
            }
        }
        if !fallback.is_empty() {
            return fallback;
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

    // Timezone-aware query pre-processing
    // Basic heuristic: append UTC version if query contains a time-like pattern
    let mut enhanced_query = query.to_string();
    if query.contains(':')
        || query.to_lowercase().contains("am")
        || query.to_lowercase().contains("pm")
    {
        use chrono::Local;
        let offset = Local::now().offset().to_string();
        enhanced_query.push_str(&format!(" UTC {}", offset));
    }

    let raw = qmd::search(&paths.qmd_bin, collection_name, &enhanced_query)?;
    matches.extend(parse_matches(paths, &raw));

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
        generated_at_epoch_secs: now_epoch_secs()?,
    })
}
