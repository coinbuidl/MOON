use crate::moon::paths::MoonPaths;
use crate::moon::qmd;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

pub fn recall(paths: &MoonPaths, query: &str, collection_name: &str) -> Result<RecallResult> {
    let raw = qmd::search(&paths.qmd_bin, collection_name, query)?;
    let matches = parse_matches(&raw);
    Ok(RecallResult {
        query: query.to_string(),
        matches,
        generated_at_epoch_secs: now_secs()?,
    })
}
