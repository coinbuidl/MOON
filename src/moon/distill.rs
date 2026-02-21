use crate::moon::audit;
use crate::moon::paths::MoonPaths;
use crate::moon::util::now_epoch_secs;
use anyhow::{Context, Result};
use chrono::{Datelike, Local, TimeZone};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader};
use std::sync::OnceLock;


#[derive(Debug, Clone)]
pub struct DistillInput {
    pub session_id: String,
    pub archive_path: String,
    pub archive_text: String,
    pub archive_epoch_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillOutput {
    pub provider: String,
    pub summary: String,
    pub summary_path: String,
    pub audit_log_path: String,
    pub created_at_epoch_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkedDistillOutput {
    pub provider: String,
    pub summary: String,
    pub summary_path: String,
    pub audit_log_path: String,
    pub created_at_epoch_secs: u64,
    pub chunk_count: usize,
    pub chunk_target_bytes: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionData {
    pub entries: Vec<ProjectionEntry>,
    pub tool_calls: Vec<String>,
    pub keywords: Vec<String>,
    pub topics: Vec<String>,
    pub time_start_epoch: Option<u64>,
    pub time_end_epoch: Option<u64>,
    pub message_count: usize,
    pub truncated: bool,
    pub compaction_anchors: Vec<CompactionAnchor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompactionAnchor {
    pub note: String,
    pub origin_message_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ToolPriority {
    High,
    Normal,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionEntry {
    pub timestamp_epoch: Option<u64>,
    pub role: String,
    pub content: String,
    pub tool_name: Option<String>,
    pub tool_target: Option<String>,
    pub priority: Option<ToolPriority>,
    pub coupled_result: Option<String>,
}

pub trait Distiller {
    fn distill(&self, input: &DistillInput) -> Result<String>;
}

pub struct LocalDistiller;
pub struct GeminiDistiller {
    pub api_key: String,
    pub model: String,
}
pub struct OpenAiDistiller {
    pub api_key: String,
    pub model: String,
}
pub struct AnthropicDistiller {
    pub api_key: String,
    pub model: String,
}
pub struct OpenAiCompatDistiller {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RemoteProvider {
    OpenAi,
    Anthropic,
    Gemini,
    OpenAiCompatible,
}

impl RemoteProvider {
    fn label(self) -> &'static str {
        match self {
            RemoteProvider::OpenAi => "openai",
            RemoteProvider::Anthropic => "anthropic",
            RemoteProvider::Gemini => "gemini",
            RemoteProvider::OpenAiCompatible => "openai-compatible",
        }
    }
}

#[derive(Debug, Clone)]
struct RemoteModelConfig {
    provider: RemoteProvider,
    model: String,
    api_key: String,
    base_url: Option<String>,
}

const SIGNAL_KEYWORDS: [&str; 5] = ["decision", "rule", "todo", "next", "milestone"];
const MAX_SIGNAL_LINES: usize = 20;
const MAX_FALLBACK_LINES: usize = 12;
const MAX_CANDIDATE_CHARS: usize = 512;
const MAX_SUMMARY_CHARS: usize = 12_000;
const MAX_PROMPT_LINES: usize = 80;
const MAX_MODEL_LINES: usize = 80;
const MIN_MODEL_BULLETS: usize = 3;
const REQUEST_TIMEOUT_SECS: u64 = 45;
const DEFAULT_DISTILL_CHUNK_BYTES: usize = 512 * 1024;
const DEFAULT_DISTILL_MAX_CHUNKS: usize = 128;
const DEFAULT_AUTO_CONTEXT_TOKENS: u64 = 250_000;
const MIN_DISTILL_CHUNK_BYTES: usize = 64 * 1024;
const MAX_AUTO_CHUNK_BYTES: usize = 2 * 1024 * 1024;
const AUTO_CHUNK_BYTES_PER_TOKEN: f64 = 3.0;
const AUTO_CHUNK_SAFETY_RATIO: f64 = 0.60;
const MAX_ROLLUP_LINES_PER_SECTION: usize = 30;
const MAX_ROLLUP_TOTAL_LINES: usize = 120;
const MAX_ARCHIVE_SCAN_BYTES: usize = 4 * 1024 * 1024;
const MAX_ARCHIVE_SCAN_LINES: usize = 50_000;
const MAX_ARCHIVE_CANDIDATES: usize = 400;

static AUTO_CHUNK_BYTES_CACHE: OnceLock<usize> = OnceLock::new();

fn env_non_empty(var: &str) -> Option<String> {
    match env::var(var) {
        Ok(v) if !v.trim().is_empty() => Some(v.trim().to_string()),
        _ => None,
    }
}

fn parse_provider_alias(raw: &str) -> Option<RemoteProvider> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "openai" => Some(RemoteProvider::OpenAi),
        "anthropic" | "claude" => Some(RemoteProvider::Anthropic),
        "gemini" | "google" => Some(RemoteProvider::Gemini),
        "openai-compatible" | "compatible" | "deepseek" => Some(RemoteProvider::OpenAiCompatible),
        _ => None,
    }
}

fn parse_prefixed_model(raw: &str) -> (Option<RemoteProvider>, String) {
    let trimmed = raw.trim();
    if let Some((prefix, model)) = trimmed.split_once(':')
        && let Some(provider) = parse_provider_alias(prefix)
    {
        return (Some(provider), model.trim().to_string());
    }
    (None, trimmed.to_string())
}

fn infer_provider_from_model(model: &str) -> Option<RemoteProvider> {
    let lower = model.trim().to_ascii_lowercase();
    if lower.starts_with("deepseek-") {
        return Some(RemoteProvider::OpenAiCompatible);
    }
    if lower.starts_with("claude-") {
        return Some(RemoteProvider::Anthropic);
    }
    if lower.starts_with("gemini-") {
        return Some(RemoteProvider::Gemini);
    }
    if lower.starts_with("gpt-")
        || lower.starts_with("o1")
        || lower.starts_with("o3")
        || lower.starts_with("o4")
    {
        return Some(RemoteProvider::OpenAi);
    }
    None
}

fn first_available_provider() -> Option<RemoteProvider> {
    if env_non_empty("AI_BASE_URL").is_some() && env_non_empty("AI_API_KEY").is_some() {
        return Some(RemoteProvider::OpenAiCompatible);
    }
    if env_non_empty("AI_API_KEY").is_some() {
        return Some(RemoteProvider::OpenAiCompatible);
    }
    if env_non_empty("OPENAI_API_KEY").is_some() {
        return Some(RemoteProvider::OpenAi);
    }
    if env_non_empty("ANTHROPIC_API_KEY").is_some() {
        return Some(RemoteProvider::Anthropic);
    }
    if env_non_empty("GEMINI_API_KEY").is_some() {
        return Some(RemoteProvider::Gemini);
    }
    None
}

fn default_model_for_provider(provider: RemoteProvider) -> &'static str {
    match provider {
        RemoteProvider::OpenAi => "gpt-4.1-mini",
        RemoteProvider::Anthropic => "claude-3-5-haiku-latest",
        RemoteProvider::Gemini => "gemini-2.5-flash-lite",
        RemoteProvider::OpenAiCompatible => "deepseek-chat",
    }
}

fn resolve_api_key(provider: RemoteProvider) -> Option<String> {
    match provider {
        RemoteProvider::OpenAi => {
            env_non_empty("OPENAI_API_KEY").or_else(|| env_non_empty("AI_API_KEY"))
        }
        RemoteProvider::Anthropic => {
            env_non_empty("ANTHROPIC_API_KEY").or_else(|| env_non_empty("AI_API_KEY"))
        }
        RemoteProvider::Gemini => {
            env_non_empty("GEMINI_API_KEY").or_else(|| env_non_empty("AI_API_KEY"))
        }
        RemoteProvider::OpenAiCompatible => env_non_empty("AI_API_KEY")
            .or_else(|| env_non_empty("DEEPSEEK_API_KEY"))
            .or_else(|| env_non_empty("OPENAI_API_KEY")),
    }
}

fn resolve_compatible_base_url(model: &str) -> Option<String> {
    if let Some(base) = env_non_empty("AI_BASE_URL") {
        return Some(base);
    }
    if model.trim().to_ascii_lowercase().starts_with("deepseek-") {
        return Some("https://api.deepseek.com".to_string());
    }
    None
}

fn resolve_remote_config() -> Option<RemoteModelConfig> {
    if env_non_empty("MOON_DISTILL_PROVIDER")
        .as_deref()
        .is_some_and(|v| v.eq_ignore_ascii_case("local"))
    {
        return None;
    }

    let configured_model = env_non_empty("MOON_DISTILL_MODEL")
        .or_else(|| env_non_empty("AI_MODEL"))
        .or_else(|| env_non_empty("MOON_GEMINI_MODEL"))
        .or_else(|| first_available_provider().map(|p| default_model_for_provider(p).to_string()));

    let mut chosen_provider = env_non_empty("MOON_DISTILL_PROVIDER")
        .as_deref()
        .and_then(parse_provider_alias)
        .or_else(|| {
            env_non_empty("AI_PROVIDER")
                .as_deref()
                .and_then(parse_provider_alias)
        });
    let (prefixed_provider, mut model) = configured_model
        .as_deref()
        .map(parse_prefixed_model)
        .unwrap_or((None, String::new()));
    if chosen_provider.is_none() {
        chosen_provider = prefixed_provider
            .or_else(|| infer_provider_from_model(&model))
            .or_else(first_available_provider);
    }

    let provider = chosen_provider?;
    if model.trim().is_empty() {
        model = default_model_for_provider(provider).to_string();
    }
    let base_url = match provider {
        RemoteProvider::OpenAiCompatible => resolve_compatible_base_url(&model),
        _ => None,
    };
    let api_key = resolve_api_key(provider)?;
    Some(RemoteModelConfig {
        provider,
        model,
        api_key,
        base_url,
    })
}



fn token_limit_to_chunk_bytes(tokens: u64) -> usize {
    let estimated = (tokens as f64) * AUTO_CHUNK_BYTES_PER_TOKEN * AUTO_CHUNK_SAFETY_RATIO;
    (estimated as usize).clamp(MIN_DISTILL_CHUNK_BYTES, MAX_AUTO_CHUNK_BYTES)
}

fn parse_env_u64(var: &str) -> Option<u64> {
    env_non_empty(var).and_then(|raw| raw.trim().parse::<u64>().ok())
}

fn find_u64_paths(root: &Value, paths: &[&[&str]]) -> Option<u64> {
    for path in paths {
        let mut cursor = root;
        let mut found = true;
        for part in *path {
            let Some(next) = cursor.get(*part) else {
                found = false;
                break;
            };
            cursor = next;
        }
        if found && let Some(value) = cursor.as_u64() {
            return Some(value);
        }
    }
    None
}

fn detect_gemini_input_token_limit(api_key: &str, model: &str) -> Option<u64> {
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}?key={}",
        model, api_key
    );
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .ok()?;
    let response = client.get(&url).send().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let json: Value = response.json().ok()?;
    json.get("inputTokenLimit").and_then(Value::as_u64)
}

fn detect_openai_compatible_input_token_limit(
    api_key: &str,
    base_url: Option<&str>,
    model: &str,
) -> Option<u64> {
    let base = base_url
        .map(str::to_string)
        .or_else(|| resolve_compatible_base_url(model))
        .unwrap_or_else(|| "https://api.openai.com".to_string());
    let url = format!("{}/v1/models", base.trim_end_matches('/'));
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .ok()?;
    let response = client.get(&url).bearer_auth(api_key).send().ok()?;
    if !response.status().is_success() {
        return None;
    }
    let json: Value = response.json().ok()?;
    let data = json.get("data").and_then(Value::as_array)?;
    let entry = data
        .iter()
        .find(|item| item.get("id").and_then(Value::as_str) == Some(model))?;

    find_u64_paths(
        entry,
        &[
            &["context_window"],
            &["max_context_length"],
            &["max_input_tokens"],
            &["input_token_limit"],
            &["inputTokenLimit"],
            &["context_length"],
            &["n_ctx"],
            &["capabilities", "context_window"],
            &["capabilities", "max_context_length"],
            &["capabilities", "max_input_tokens"],
            &["capabilities", "input_token_limit"],
        ],
    )
}

fn infer_context_tokens_from_model(provider: RemoteProvider, model: &str) -> u64 {
    let lower = model.to_ascii_lowercase();
    match provider {
        RemoteProvider::Gemini => {
            if lower.starts_with("gemini-2.5") {
                1_000_000
            } else {
                250_000
            }
        }
        RemoteProvider::OpenAi => {
            if lower.starts_with("gpt-4.1") {
                1_000_000
            } else if lower.starts_with("gpt-4o") {
                128_000
            } else {
                200_000
            }
        }
        RemoteProvider::Anthropic => 200_000,
        RemoteProvider::OpenAiCompatible => {
            if lower.starts_with("deepseek-") {
                128_000
            } else {
                200_000
            }
        }
    }
}

fn detect_context_tokens_from_remote(remote: &RemoteModelConfig) -> Option<u64> {
    match remote.provider {
        RemoteProvider::Gemini => detect_gemini_input_token_limit(&remote.api_key, &remote.model),
        RemoteProvider::OpenAiCompatible => detect_openai_compatible_input_token_limit(
            &remote.api_key,
            remote.base_url.as_deref(),
            &remote.model,
        ),
        RemoteProvider::OpenAi | RemoteProvider::Anthropic => None,
    }
}

fn detect_auto_chunk_bytes() -> usize {
    if let Some(tokens) = parse_env_u64("MOON_DISTILL_MODEL_CONTEXT_TOKENS") {
        return token_limit_to_chunk_bytes(tokens);
    }

    if let Some(remote) = resolve_remote_config() {
        if let Some(tokens) = detect_context_tokens_from_remote(&remote) {
            return token_limit_to_chunk_bytes(tokens);
        }
        return token_limit_to_chunk_bytes(infer_context_tokens_from_model(
            remote.provider,
            &remote.model,
        ));
    }

    token_limit_to_chunk_bytes(DEFAULT_AUTO_CONTEXT_TOKENS)
}

pub fn distill_chunk_bytes() -> usize {
    let auto = || *AUTO_CHUNK_BYTES_CACHE.get_or_init(detect_auto_chunk_bytes);
    match env::var("MOON_DISTILL_CHUNK_BYTES") {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return auto();
            }
            if trimmed.eq_ignore_ascii_case("auto") {
                return auto();
            }
            trimmed
                .parse::<usize>()
                .ok()
                .filter(|v| *v > 0)
                .unwrap_or(DEFAULT_DISTILL_CHUNK_BYTES)
                .max(MIN_DISTILL_CHUNK_BYTES)
        }
        Err(_) => auto(),
    }
}

fn distill_max_chunks() -> usize {
    match env::var("MOON_DISTILL_MAX_CHUNKS") {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return DEFAULT_DISTILL_MAX_CHUNKS;
            }
            trimmed
                .parse::<usize>()
                .ok()
                .filter(|v| *v > 0)
                .unwrap_or(DEFAULT_DISTILL_MAX_CHUNKS)
        }
        Err(_) => DEFAULT_DISTILL_MAX_CHUNKS,
    }
}

pub fn archive_file_size(path: &str) -> Result<u64> {
    Ok(fs::metadata(path)
        .with_context(|| format!("failed to stat {path}"))?
        .len())
}

fn truncate_with_ellipsis(input: &str, max_chars: usize) -> String {
    if input.chars().count() <= max_chars {
        return input.to_string();
    }
    if max_chars <= 3 {
        return "...".chars().take(max_chars).collect();
    }
    let mut out = String::new();
    for (idx, ch) in input.chars().enumerate() {
        if idx >= max_chars - 3 {
            break;
        }
        out.push(ch);
    }
    out.push_str("...");
    out
}

fn unescape_json_noise(input: &str) -> String {
    input
        .replace("\\\\\"", "\"")
        .replace("\\\\n", "\n")
        .replace("\\\\t", "\t")
        .replace("\\\\\\\\", "\\")
}

fn normalize_text(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clean_candidate_text(input: &str) -> Option<String> {
    let unescaped = unescape_json_noise(input);
    let normalized = normalize_text(&unescaped);
    if normalized.is_empty() {
        return None;
    }
    Some(truncate_with_ellipsis(&normalized, MAX_CANDIDATE_CHARS))
}

fn looks_like_json_blob(input: &str) -> bool {
    let trimmed = input.trim_start();
    trimmed.starts_with('{')
        || trimmed.starts_with('[')
        || trimmed.contains("\"type\":\"message\"")
        || trimmed.contains("\"message\":{\"role\"")
}

fn push_message_candidates(entry: &Value, out: &mut Vec<String>) {
    let Some(message) = entry.get("message") else {
        return;
    };
    let role = message.get("role").and_then(Value::as_str).unwrap_or("");
    let Some(content) = message.get("content").and_then(Value::as_array) else {
        return;
    };

    for part in content {
        if part.get("type").and_then(Value::as_str) != Some("text") {
            continue;
        }
        let Some(text) = part.get("text").and_then(Value::as_str) else {
            continue;
        };
        let Some(cleaned) = clean_candidate_text(text) else {
            continue;
        };

        let candidate = match role {
            "toolResult" => {
                // Tool payloads can be huge JSON blobs; only keep concise plain-text outputs.
                if cleaned.len() > 220
                    || looks_like_json_blob(&cleaned)
                    || cleaned.contains("<<<EXTERNAL_UNTRUSTED_CONTENT>>>")
                {
                    continue;
                }
                format!("[tool] {cleaned}")
            }
            "user" => format!("[user] {cleaned}"),
            "assistant" => format!("[assistant] {cleaned}"),
            _ => cleaned,
        };
        out.push(candidate);
        if out.len() >= 200 {
            return;
        }
    }
}

fn push_candidate_from_line(trimmed: &str, out: &mut Vec<String>) {
    if trimmed.is_empty() {
        return;
    }

    if let Ok(entry) = serde_json::from_str::<Value>(trimmed) {
        push_message_candidates(&entry, out);
        return;
    }

    if !looks_like_json_blob(trimmed)
        && let Some(cleaned) = clean_candidate_text(trimmed)
    {
        out.push(cleaned);
    }
}

fn extract_candidate_lines(raw: &str) -> Vec<String> {
    let mut out = Vec::new();

    for line in raw.lines() {
        push_candidate_from_line(line.trim(), &mut out);

        if out.len() >= 200 {
            break;
        }
    }

    out
}

fn extract_message_entry(entry: &Value) -> Option<ProjectionEntry> {
    let message = entry.get("message")?;
    let role = message.get("role").and_then(Value::as_str).unwrap_or("").to_string();
    
    let mut timestamp_epoch = None;
    if let Some(ts_str) = message.get("createdAt").and_then(Value::as_str) {
        if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(ts_str) {
            timestamp_epoch = Some(ts.timestamp() as u64);
        }
    } else if let Some(ts_num) = entry.get("timestamp_epoch").and_then(Value::as_u64) {
        timestamp_epoch = Some(ts_num);
    }
    
    let content_arr = message.get("content").and_then(Value::as_array)?;
    let mut text_parts = Vec::new();
    let mut tool_name = None;
    let mut tool_target = None;
    let mut priority = None;

    if role == "toolResult" {
        for part in content_arr {
            if part.get("type").and_then(Value::as_str) == Some("text")
                && let Some(text) = part.get("text").and_then(Value::as_str)
                    && let Some(cleaned) = clean_candidate_text(text)
                        && cleaned.len() <= 1024 && !looks_like_json_blob(&cleaned) && !cleaned.contains("<<<EXTERNAL_UNTRUSTED_CONTENT>>>") {
                            text_parts.push(cleaned);
                        }
        }
    } else {
        for part in content_arr {
            let part_type = part.get("type").and_then(Value::as_str).unwrap_or("");
            if part_type == "text" {
                if let Some(text) = part.get("text").and_then(Value::as_str)
                    && let Some(cleaned) = clean_candidate_text(text) {
                        text_parts.push(cleaned);
                    }
            } else if part_type == "toolUse"
                && let Some(name) = part.get("name").and_then(Value::as_str) {
                    tool_name = Some(name.to_string());
                    priority = Some(match name {
                        "write_to_file" | "exec" | "edit" | "gateway" => ToolPriority::High,
                        _ => ToolPriority::Normal,
                    });
                    
                    if let Some(input) = part.get("input").and_then(Value::as_object) {
                        if let Some(cmd) = input.get("command").and_then(Value::as_str) {
                            tool_target = Some(cmd.to_string());
                        } else if let Some(path) = input.get("path").or_else(|| input.get("file")).and_then(Value::as_str) {
                            tool_target = Some(path.to_string());
                        } else if let Ok(dump) = serde_json::to_string(input) {
                            tool_target = Some(truncate_with_ellipsis(&dump, 64));
                        }
                    }
                }
        }
    }

    if text_parts.is_empty() && tool_name.is_none() {
        return None;
    }

    Some(ProjectionEntry {
        timestamp_epoch,
        role,
        content: text_parts.join("\n"),
        tool_name,
        tool_target,
        priority,
        coupled_result: None,
    })
}

fn extract_keywords(entries: &[ProjectionEntry]) -> Vec<String> {
    let mut keywords = BTreeSet::new();
    for entry in entries {
        if entry.role != "user" && entry.role != "assistant" {
            continue;
        }
        for word in entry.content.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != '.') {
            if word.len() > 4 && word.len() < 24 && !word.chars().all(|c| c.is_numeric()) {
                keywords.insert(word.to_lowercase());
            }
        }
        if keywords.len() > 100 {
            break;
        }
    }
    keywords.into_iter().take(30).collect()
}

fn infer_topics(_entries: &[ProjectionEntry], keywords: &[String]) -> Vec<String> {
    if keywords.is_empty() {
        vec![]
    } else {
        vec!["Session activity".to_string()]
    }
}

pub fn extract_projection_data(path: &str) -> Result<ProjectionData> {
    let file = fs::File::open(path).with_context(|| format!("failed to open {path}"))?;
    let reader = BufReader::new(file);

    let mut scanned_bytes = 0usize;
    let mut scanned_lines = 0usize;
    let mut entries: Vec<ProjectionEntry> = Vec::new();
    let mut tool_calls_set = BTreeSet::new();
    let mut compaction_anchors = Vec::new();
    let mut truncated = false;

    let mut pending_tool_uses: Vec<usize> = Vec::new();

    for line in reader.split(b'\n') {
        let raw = line.with_context(|| format!("failed to read line from {path}"))?;
        scanned_lines = scanned_lines.saturating_add(1);
        scanned_bytes = scanned_bytes.saturating_add(raw.len().saturating_add(1));

        let decoded = String::from_utf8_lossy(&raw);
        let trimmed = decoded.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Ok(json_entry) = serde_json::from_str::<Value>(trimmed) {
            if let Some(note) = json_entry.get("compaction_summary").and_then(Value::as_str) {
                compaction_anchors.push(CompactionAnchor {
                    note: note.to_string(),
                    origin_message_id: json_entry.get("message_id").and_then(Value::as_str).map(|s| s.to_string()),
                });
            }

            if let Some(entry) = extract_message_entry(&json_entry) {
                let idx = entries.len();
                
                if entry.role == "assistant" && entry.tool_name.is_some() {
                    tool_calls_set.insert(entry.tool_name.clone().unwrap());
                    pending_tool_uses.push(idx);
                } else if entry.role == "toolResult"
                    && let Some(use_idx) = pending_tool_uses.pop() {
                        entries[use_idx].coupled_result = Some(entry.content.clone());
                    }
                
                entries.push(entry);
            }
        } else if !looks_like_json_blob(trimmed) && let Some(cleaned) = clean_candidate_text(trimmed) {
            entries.push(ProjectionEntry {
                timestamp_epoch: None,
                role: "system".to_string(),
                content: cleaned,
                tool_name: None,
                tool_target: None,
                priority: None,
                coupled_result: None,
            });
        }

        if entries.len() >= MAX_ARCHIVE_CANDIDATES
            || scanned_lines >= MAX_ARCHIVE_SCAN_LINES
            || scanned_bytes >= MAX_ARCHIVE_SCAN_BYTES
        {
            truncated = true;
            break;
        }
    }

    let message_count = entries.len();
    let time_start_epoch = entries.first().and_then(|e| e.timestamp_epoch);
    let time_end_epoch = entries.last().and_then(|e| e.timestamp_epoch);
    let keywords = extract_keywords(&entries);
    let topics = infer_topics(&entries, &keywords);

    Ok(ProjectionData {
        entries,
        tool_calls: tool_calls_set.into_iter().collect(),
        keywords,
        topics,
        time_start_epoch,
        time_end_epoch,
        message_count,
        truncated,
        compaction_anchors,
    })
}

impl ProjectionData {
    pub fn to_excerpt(&self) -> String {
        let mut out = Vec::new();
        for entry in &self.entries {
            let candidate = match entry.role.as_str() {
                "toolResult" => {
                    if entry.coupled_result.is_none() {
                        format!("[tool] {}", entry.content)
                    } else {
                        continue;
                    }
                }
                "user" => format!("[user] {}", entry.content),
                "assistant" => {
                    let mut s = format!("[assistant] {}", entry.content);
                    if let Some(ref t) = entry.tool_name {
                        s.push_str(&format!(" [toolUse {}]", t));
                    }
                    if let Some(ref r) = entry.coupled_result {
                        s.push_str(&format!("\n[toolResult] {}", r));
                    }
                    s
                },
                _ => entry.content.clone(),
            };
            if !candidate.trim().is_empty() {
                out.push(candidate);
            }
        }
        let mut excerpt = out.join("\n");
        if self.truncated {
            excerpt.push_str("\n[archive excerpt truncated]");
        }
        excerpt
    }
}

pub fn load_archive_excerpt(path: &str) -> Result<String> {
    let data = extract_projection_data(path)?;
    Ok(data.to_excerpt())
}

fn is_signal_line(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    SIGNAL_KEYWORDS
        .iter()
        .any(|keyword| lower.contains(keyword))
}

fn extract_signal_lines(raw: &str) -> Vec<String> {
    let candidates = extract_candidate_lines(raw);
    let mut out = Vec::new();

    for line in &candidates {
        if is_signal_line(line) {
            out.push(line.clone());
        }
        if out.len() >= MAX_SIGNAL_LINES {
            return out;
        }
    }

    if out.is_empty() {
        candidates.into_iter().take(MAX_FALLBACK_LINES).collect()
    } else {
        out
    }
}

fn build_prompt_context(raw: &str) -> String {
    let candidates = extract_candidate_lines(raw);
    let mut out = String::new();
    for line in candidates.into_iter().take(MAX_PROMPT_LINES) {
        out.push_str("- ");
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn build_llm_prompt(input: &DistillInput) -> String {
    let context = build_prompt_context(&input.archive_text);
    format!(
        "Summarize this session into concise bullets under headings for Decisions, Rules, Milestones, and Open Tasks. Return markdown only. Never output raw JSON, JSONL, code fences, XML, YAML, tool payload dumps, or verbatim logs.\nSession id: {}\nArchive path: {}\n\nContext lines:\n{}",
        input.session_id, input.archive_path, context
    )
}

fn looks_like_structured_fragment(input: &str) -> bool {
    let trimmed = input.trim();
    trimmed.starts_with("```")
        || trimmed == "{"
        || trimmed == "}"
        || trimmed == "["
        || trimmed == "]"
        || (trimmed.starts_with('"') && trimmed.contains("\":"))
}

fn extract_openai_text(json: &Value) -> Option<String> {
    if let Some(text) = json.get("output_text").and_then(Value::as_str) {
        return Some(text.to_string());
    }

    let mut chunks = Vec::new();
    let output = json.get("output").and_then(Value::as_array)?;
    for item in output {
        let Some(content) = item.get("content").and_then(Value::as_array) else {
            continue;
        };
        for part in content {
            if let Some(text) = part.get("text").and_then(Value::as_str) {
                chunks.push(text.to_string());
            }
        }
    }

    if chunks.is_empty() {
        None
    } else {
        Some(chunks.join("\n"))
    }
}

fn extract_anthropic_text(json: &Value) -> Option<String> {
    let mut chunks = Vec::new();
    let content = json.get("content").and_then(Value::as_array)?;
    for part in content {
        if let Some(text) = part.get("text").and_then(Value::as_str) {
            chunks.push(text.to_string());
        }
    }
    if chunks.is_empty() {
        None
    } else {
        Some(chunks.join("\n"))
    }
}

fn extract_openai_compatible_text(json: &Value) -> Option<String> {
    let choices = json.get("choices").and_then(Value::as_array)?;
    let first = choices.first()?;
    let content = first.get("message")?.get("content")?;
    match content {
        Value::String(s) => Some(s.to_string()),
        Value::Array(parts) => {
            let mut chunks = Vec::new();
            for part in parts {
                if let Some(text) = part.get("text").and_then(Value::as_str) {
                    chunks.push(text.to_string());
                }
            }
            if chunks.is_empty() {
                None
            } else {
                Some(chunks.join("\n"))
            }
        }
        _ => None,
    }
}

fn sanitize_model_summary(summary: &str) -> Option<String> {
    let mut lines = Vec::new();
    let mut bullet_count = 0usize;

    for raw_line in summary.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if looks_like_json_blob(trimmed)
            || looks_like_structured_fragment(trimmed)
            || trimmed.contains("<<<EXTERNAL_UNTRUSTED_CONTENT>>>")
        {
            continue;
        }

        let cleaned = clean_candidate_text(trimmed)?;
        let normalized = if cleaned.starts_with('#') {
            cleaned
        } else if cleaned.starts_with("- ") {
            bullet_count += 1;
            cleaned
        } else if cleaned.starts_with("* ") {
            bullet_count += 1;
            cleaned.replacen("* ", "- ", 1)
        } else {
            bullet_count += 1;
            format!("- {cleaned}")
        };
        lines.push(normalized);
        if lines.len() >= MAX_MODEL_LINES {
            break;
        }
    }

    if bullet_count < MIN_MODEL_BULLETS {
        return None;
    }
    Some(lines.join("\n"))
}

fn clamp_summary(summary: &str) -> String {
    let normalized = summary.trim_end();
    if normalized.chars().count() <= MAX_SUMMARY_CHARS {
        return normalized.to_string();
    }
    let truncated = truncate_with_ellipsis(normalized, MAX_SUMMARY_CHARS);
    format!("{truncated}\n\n[summary truncated]")
}

impl Distiller for LocalDistiller {
    fn distill(&self, input: &DistillInput) -> Result<String> {
        let mut lines = extract_signal_lines(&input.archive_text);
        if lines.is_empty() {
            lines = input
                .archive_text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .take(MAX_FALLBACK_LINES)
                .filter_map(clean_candidate_text)
                .collect();
        }

        let mut summary = String::new();
        summary.push_str("## Distilled Session Summary\n");
        summary.push_str(&format!("- session_id: {}\n", input.session_id));
        summary.push_str(&format!("- archive_path: {}\n", input.archive_path));
        summary.push_str("- extracted_signals:\n");
        for line in lines {
            summary.push_str(&format!("  - {}\n", line));
        }
        Ok(summary)
    }
}

impl Distiller for GeminiDistiller {
    fn distill(&self, input: &DistillInput) -> Result<String> {
        let prompt = build_llm_prompt(input);

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            self.model, self.api_key
        );

        let payload = serde_json::json!({
            "contents": [
                {
                    "parts": [
                        {"text": prompt}
                    ]
                }
            ]
        });

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()?;
        let response = client.post(&url).json(&payload).send()?;
        if !response.status().is_success() {
            anyhow::bail!("gemini call failed with status {}", response.status());
        }
        let json: Value = response.json()?;
        let text = json
            .get("candidates")
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(|v| v.get("content"))
            .and_then(|v| v.get("parts"))
            .and_then(Value::as_array)
            .and_then(|parts| parts.first())
            .and_then(|v| v.get("text"))
            .and_then(Value::as_str)
            .context("gemini response missing text content")?;

        Ok(text.to_string())
    }
}

impl Distiller for OpenAiDistiller {
    fn distill(&self, input: &DistillInput) -> Result<String> {
        let prompt = build_llm_prompt(input);
        let payload = serde_json::json!({
            "model": self.model,
            "input": prompt,
            "temperature": 0.2
        });

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()?;
        let response = client
            .post("https://api.openai.com/v1/responses")
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()?;
        if !response.status().is_success() {
            anyhow::bail!("openai call failed with status {}", response.status());
        }

        let json: Value = response.json()?;
        let text = extract_openai_text(&json).context("openai response missing text content")?;
        Ok(text)
    }
}

impl Distiller for OpenAiCompatDistiller {
    fn distill(&self, input: &DistillInput) -> Result<String> {
        let prompt = build_llm_prompt(input);
        let base = self.base_url.trim_end_matches('/');
        let url = format!("{base}/v1/chat/completions");
        let payload = serde_json::json!({
            "model": self.model,
            "messages": [
                {"role": "user", "content": prompt}
            ],
            "temperature": 0.2
        });

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()?;
        let response = client
            .post(&url)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()?;
        if !response.status().is_success() {
            anyhow::bail!(
                "openai-compatible call failed with status {}",
                response.status()
            );
        }

        let json: Value = response.json()?;
        let text = extract_openai_compatible_text(&json)
            .context("openai-compatible response missing text content")?;
        Ok(text)
    }
}

impl Distiller for AnthropicDistiller {
    fn distill(&self, input: &DistillInput) -> Result<String> {
        let prompt = build_llm_prompt(input);
        let payload = serde_json::json!({
            "model": self.model,
            "max_tokens": 1200,
            "temperature": 0.2,
            "messages": [
                {
                    "role": "user",
                    "content": prompt
                }
            ]
        });

        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()?;
        let response = client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .json(&payload)
            .send()?;
        if !response.status().is_success() {
            anyhow::bail!("anthropic call failed with status {}", response.status());
        }

        let json: Value = response.json()?;
        let text =
            extract_anthropic_text(&json).context("anthropic response missing text content")?;
        Ok(text)
    }
}

fn daily_memory_path(paths: &MoonPaths, archive_epoch_secs: Option<u64>) -> String {
    let timestamp = archive_epoch_secs
        .and_then(|secs| Local.timestamp_opt(secs as i64, 0).single())
        .unwrap_or_else(Local::now);
    let date = format!(
        "{:04}-{:02}-{:02}",
        timestamp.year(),
        timestamp.month(),
        timestamp.day()
    );
    paths
        .memory_dir
        .join(format!("{}.md", date))
        .display()
        .to_string()
}

fn distill_summary(input: &DistillInput) -> Result<(String, String)> {
    let mut local_summary_cache: Option<String> = None;
    let mut local_summary = || -> Result<String> {
        if let Some(existing) = &local_summary_cache {
            return Ok(existing.clone());
        }
        let summary = LocalDistiller.distill(input)?;
        local_summary_cache = Some(summary.clone());
        Ok(summary)
    };

    let (provider_used, generated_summary) = if let Some(remote) = resolve_remote_config() {
        let remote_result = match remote.provider {
            RemoteProvider::OpenAi => OpenAiDistiller {
                api_key: remote.api_key.clone(),
                model: remote.model.clone(),
            }
            .distill(input),
            RemoteProvider::Anthropic => AnthropicDistiller {
                api_key: remote.api_key.clone(),
                model: remote.model.clone(),
            }
            .distill(input),
            RemoteProvider::Gemini => GeminiDistiller {
                api_key: remote.api_key.clone(),
                model: remote.model.clone(),
            }
            .distill(input),
            RemoteProvider::OpenAiCompatible => OpenAiCompatDistiller {
                api_key: remote.api_key.clone(),
                model: remote.model.clone(),
                base_url: remote
                    .base_url
                    .clone()
                    .unwrap_or_else(|| "https://api.openai.com".to_string()),
            }
            .distill(input),
        };

        match remote_result {
            Ok(out) => match sanitize_model_summary(&out) {
                Some(cleaned) => (remote.provider.label().to_string(), cleaned),
                None => ("local".to_string(), local_summary()?),
            },
            Err(_) => ("local".to_string(), local_summary()?),
        }
    } else {
        ("local".to_string(), local_summary()?)
    };
    Ok((provider_used, clamp_summary(&generated_summary)))
}

fn append_distilled_summary(
    paths: &MoonPaths,
    input: &DistillInput,
    provider_used: String,
    summary: String,
) -> Result<DistillOutput> {
    let summary_path = daily_memory_path(paths, input.archive_epoch_secs);
    let mut text = String::new();
    text.push_str(&format!("\n\n### {}\n", input.session_id));
    text.push_str(&summary);
    text.push('\n');

    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&summary_path)
        .with_context(|| format!("failed to open {}", summary_path))?;
    file.write_all(text.as_bytes())?;

    audit::append_event(
        paths,
        "distill",
        "ok",
        &format!(
            "distilled session {} into {} provider={}",
            input.session_id, summary_path, provider_used
        ),
    )?;

    Ok(DistillOutput {
        provider: provider_used,
        summary,
        summary_path: summary_path.clone(),
        audit_log_path: paths.logs_dir.join("audit.log").display().to_string(),
        created_at_epoch_secs: now_epoch_secs()?,
    })
}

#[derive(Default)]
struct ChunkSummaryRollup {
    seen: BTreeSet<String>,
    decisions: Vec<String>,
    rules: Vec<String>,
    milestones: Vec<String>,
    tasks: Vec<String>,
    other: Vec<String>,
}

impl ChunkSummaryRollup {
    fn total_lines(&self) -> usize {
        self.decisions.len()
            + self.rules.len()
            + self.milestones.len()
            + self.tasks.len()
            + self.other.len()
    }

    fn push_line(&mut self, raw_line: &str) {
        if self.total_lines() >= MAX_ROLLUP_TOTAL_LINES {
            return;
        }

        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            return;
        }

        let normalized = trimmed
            .trim_start_matches("- ")
            .trim_start_matches("* ")
            .trim();
        if normalized.is_empty() || normalized.starts_with('#') {
            return;
        }
        if looks_like_json_blob(normalized) || looks_like_structured_fragment(normalized) {
            return;
        }

        let Some(cleaned) = clean_candidate_text(normalized) else {
            return;
        };
        let key = cleaned.to_ascii_lowercase();
        if !self.seen.insert(key) {
            return;
        }

        let lower = cleaned.to_ascii_lowercase();
        let target = if lower.contains("decision") {
            &mut self.decisions
        } else if lower.contains("rule") {
            &mut self.rules
        } else if lower.contains("milestone") {
            &mut self.milestones
        } else if lower.contains("todo")
            || lower.contains("open task")
            || lower.contains("next")
            || lower.contains("follow up")
            || lower.contains("follow-up")
            || lower.contains("action item")
        {
            &mut self.tasks
        } else {
            &mut self.other
        };

        if target.len() < MAX_ROLLUP_LINES_PER_SECTION {
            target.push(cleaned);
        }
    }

    fn ingest_summary(&mut self, summary: &str) {
        for line in summary.lines() {
            self.push_line(line);
            if self.total_lines() >= MAX_ROLLUP_TOTAL_LINES {
                break;
            }
        }
    }

    fn render(
        &self,
        session_id: &str,
        archive_path: &str,
        chunk_count: usize,
        chunk_target_bytes: usize,
        max_chunks: usize,
        truncated: bool,
    ) -> String {
        fn append_section(out: &mut String, title: &str, lines: &[String]) {
            if lines.is_empty() {
                return;
            }
            out.push_str(&format!("### {title}\n"));
            for line in lines {
                out.push_str("- ");
                out.push_str(line);
                out.push('\n');
            }
            out.push('\n');
        }

        let mut out = String::new();
        out.push_str("## Distilled Session Summary\n");
        out.push_str(&format!("- session_id: {session_id}\n"));
        out.push_str(&format!("- archive_path: {archive_path}\n"));
        out.push_str(&format!("- chunk_count: {chunk_count}\n"));
        out.push_str(&format!("- chunk_target_bytes: {chunk_target_bytes}\n"));
        if truncated {
            out.push_str(&format!(
                "- chunking_truncated: true (max_chunks={max_chunks})\n"
            ));
        }
        out.push('\n');

        append_section(&mut out, "Decisions", &self.decisions);
        append_section(&mut out, "Rules", &self.rules);
        append_section(&mut out, "Milestones", &self.milestones);
        append_section(&mut out, "Open Tasks", &self.tasks);
        append_section(&mut out, "Other Signals", &self.other);

        if self.total_lines() == 0 {
            out.push_str("### Notes\n- no high-signal lines extracted from chunk summaries\n");
        }

        out
    }
}

fn summarize_provider_mix(provider_counts: &BTreeMap<String, usize>) -> String {
    if provider_counts.is_empty() {
        return "local".to_string();
    }
    if provider_counts.len() == 1 {
        return provider_counts.keys().next().cloned().unwrap_or_default();
    }
    let parts = provider_counts
        .iter()
        .map(|(provider, count)| format!("{provider}:{count}"))
        .collect::<Vec<_>>()
        .join(",");
    format!("mixed({parts})")
}

fn stream_archive_chunks<F>(
    path: &str,
    chunk_target_bytes: usize,
    max_chunks: usize,
    mut on_chunk: F,
) -> Result<(usize, bool)>
where
    F: FnMut(usize, String) -> Result<()>,
{
    let file = fs::File::open(path).with_context(|| format!("failed to open {path}"))?;
    let reader = BufReader::new(file);

    let mut current_chunk = String::new();
    let mut current_bytes = 0usize;
    let mut chunk_count = 0usize;
    let mut truncated = false;

    for line in reader.split(b'\n') {
        let raw = line.with_context(|| format!("failed to read line from {path}"))?;
        let line_bytes = raw.len().saturating_add(1);

        if !current_chunk.is_empty()
            && current_bytes.saturating_add(line_bytes) > chunk_target_bytes
        {
            chunk_count = chunk_count.saturating_add(1);
            on_chunk(chunk_count, std::mem::take(&mut current_chunk))?;
            current_bytes = 0;
            if chunk_count >= max_chunks {
                truncated = true;
                break;
            }
        }

        current_chunk.push_str(&String::from_utf8_lossy(&raw));
        current_chunk.push('\n');
        current_bytes = current_bytes.saturating_add(line_bytes);
    }

    if !truncated {
        if current_chunk.is_empty() {
            if chunk_count == 0 {
                chunk_count = 1;
                on_chunk(chunk_count, String::new())?;
            }
        } else {
            chunk_count = chunk_count.saturating_add(1);
            on_chunk(chunk_count, current_chunk)?;
        }
    }

    Ok((chunk_count, truncated))
}

pub fn run_chunked_archive_distillation(
    paths: &MoonPaths,
    input: &DistillInput,
) -> Result<ChunkedDistillOutput> {
    fs::create_dir_all(&paths.memory_dir)
        .with_context(|| format!("failed to create {}", paths.memory_dir.display()))?;

    let chunk_target_bytes = distill_chunk_bytes();
    let max_chunks = distill_max_chunks();

    let mut rollup = ChunkSummaryRollup::default();
    let mut provider_counts = BTreeMap::<String, usize>::new();

    let (chunk_count, truncated) = stream_archive_chunks(
        &input.archive_path,
        chunk_target_bytes,
        max_chunks,
        |chunk_index, chunk_text| {
            let chunk_input = DistillInput {
                session_id: format!("{} [chunk {}]", input.session_id, chunk_index),
                archive_path: format!("{}#chunk={}", input.archive_path, chunk_index),
                archive_text: chunk_text,
                archive_epoch_secs: input.archive_epoch_secs,
            };
            let (provider, summary) = distill_summary(&chunk_input)?;
            *provider_counts.entry(provider).or_insert(0) += 1;
            rollup.ingest_summary(&summary);
            Ok(())
        },
    )?;

    let provider = summarize_provider_mix(&provider_counts);
    let summary = clamp_summary(&rollup.render(
        &input.session_id,
        &input.archive_path,
        chunk_count,
        chunk_target_bytes,
        max_chunks,
        truncated,
    ));
    let out = append_distilled_summary(paths, input, provider.clone(), summary.clone())?;

    Ok(ChunkedDistillOutput {
        provider,
        summary,
        summary_path: out.summary_path,
        audit_log_path: out.audit_log_path,
        created_at_epoch_secs: out.created_at_epoch_secs,
        chunk_count,
        chunk_target_bytes,
        truncated,
    })
}

pub fn run_distillation(paths: &MoonPaths, input: &DistillInput) -> Result<DistillOutput> {
    fs::create_dir_all(&paths.memory_dir)
        .with_context(|| format!("failed to create {}", paths.memory_dir.display()))?;

    let (provider_used, summary) = distill_summary(input)?;
    append_distilled_summary(paths, input, provider_used, summary)
}

#[cfg(test)]
mod tests {
    use super::{
        ChunkSummaryRollup, DistillInput, Distiller, LocalDistiller, MAX_SUMMARY_CHARS,
        RemoteProvider, clamp_summary, extract_anthropic_text, extract_openai_compatible_text,
        extract_openai_text, infer_provider_from_model, parse_prefixed_model,
        sanitize_model_summary, stream_archive_chunks, summarize_provider_mix,
    };
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn local_distiller_avoids_raw_jsonl_payloads() {
        let input = DistillInput {
            session_id: "s".to_string(),
            archive_path: "/tmp/s.jsonl".to_string(),
            archive_text: format!(
                "{{\"type\":\"message\",\"message\":{{\"role\":\"toolResult\",\"content\":[{{\"type\":\"text\",\"text\":\"{{\\\"payload\\\":\\\"{}\\\"}}\"}}]}}}}\n{{\"type\":\"message\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"text\",\"text\":\"Decision: set qmd mask to jsonl for archive indexing.\"}}]}}}}\n",
                "X".repeat(4096)
            ),
            archive_epoch_secs: None,
        };

        let summary = LocalDistiller
            .distill(&input)
            .expect("distill should succeed");
        assert!(summary.contains("Decision: set qmd mask to jsonl"));
        assert!(!summary.contains("\"payload\""));
        assert!(!summary.contains("\"type\":\"message\""));
    }

    #[test]
    fn clamp_summary_limits_large_output() {
        let giant = "A".repeat(MAX_SUMMARY_CHARS + 5000);
        let clamped = clamp_summary(&giant);
        assert!(clamped.chars().count() <= MAX_SUMMARY_CHARS + 32);
        assert!(clamped.contains("[summary truncated]"));
    }

    #[test]
    fn sanitize_model_summary_rejects_json_blob_output() {
        let raw = "{ \"type\": \"message\" }\n{ \"payload\": \"x\" }\n";
        assert!(sanitize_model_summary(raw).is_none());
    }

    #[test]
    fn sanitize_model_summary_normalizes_plain_lines_to_bullets() {
        let raw =
            "Decision: use jsonl mask\nRule: prefer concise bullets\nMilestone: qmd indexing fixed";
        let got = sanitize_model_summary(raw).expect("should produce summary");
        assert!(got.contains("- Decision: use jsonl mask"));
        assert!(got.contains("- Rule: prefer concise bullets"));
        assert!(got.contains("- Milestone: qmd indexing fixed"));
    }

    #[test]
    fn parse_prefixed_model_resolves_provider_hint() {
        let (provider, model) = parse_prefixed_model("openai:gpt-4.1-mini");
        assert_eq!(provider, Some(RemoteProvider::OpenAi));
        assert_eq!(model, "gpt-4.1-mini");

        let (provider, model) = parse_prefixed_model("claude:claude-3-5-haiku-latest");
        assert_eq!(provider, Some(RemoteProvider::Anthropic));
        assert_eq!(model, "claude-3-5-haiku-latest");

        let (provider, model) = parse_prefixed_model("deepseek:deepseek-chat");
        assert_eq!(provider, Some(RemoteProvider::OpenAiCompatible));
        assert_eq!(model, "deepseek-chat");
    }

    #[test]
    fn infer_provider_from_model_supports_openai_anthropic_and_gemini() {
        assert_eq!(
            infer_provider_from_model("gpt-4.1-mini"),
            Some(RemoteProvider::OpenAi)
        );
        assert_eq!(
            infer_provider_from_model("claude-3-5-haiku-latest"),
            Some(RemoteProvider::Anthropic)
        );
        assert_eq!(
            infer_provider_from_model("gemini-2.5-flash-lite"),
            Some(RemoteProvider::Gemini)
        );
        assert_eq!(
            infer_provider_from_model("deepseek-chat"),
            Some(RemoteProvider::OpenAiCompatible)
        );
    }

    #[test]
    fn extract_openai_text_prefers_output_text_field() {
        let payload = json!({
            "output_text": "hello from openai"
        });
        assert_eq!(
            extract_openai_text(&payload).as_deref(),
            Some("hello from openai")
        );
    }

    #[test]
    fn extract_anthropic_text_reads_content_blocks() {
        let payload = json!({
            "content": [
                {"type": "text", "text": "line one"},
                {"type": "text", "text": "line two"}
            ]
        });
        assert_eq!(
            extract_anthropic_text(&payload).as_deref(),
            Some("line one\nline two")
        );
    }

    #[test]
    fn extract_openai_compatible_text_reads_chat_completions_shape() {
        let payload = json!({
            "choices": [
                {
                    "message": {
                        "content": "hello from compatible provider"
                    }
                }
            ]
        });
        assert_eq!(
            extract_openai_compatible_text(&payload).as_deref(),
            Some("hello from compatible provider")
        );
    }

    #[test]
    fn chunk_rollup_groups_keyword_sections() {
        let mut rollup = ChunkSummaryRollup::default();
        rollup.ingest_summary(
            "- Decision: enable chunk distill\n- Rule: keep archive gate at 2MB\n- Milestone: watcher can process 10MB archives\n- Open task: tune chunk size by workload",
        );

        let rendered = rollup.render("session-1", "/tmp/a.jsonl", 4, 524_288, 128, false);
        assert!(rendered.contains("### Decisions"));
        assert!(rendered.contains("### Rules"));
        assert!(rendered.contains("### Milestones"));
        assert!(rendered.contains("### Open Tasks"));
    }

    #[test]
    fn stream_archive_chunks_splits_input_by_target_size() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("moon-chunk-test-{stamp}.jsonl"));
        fs::write(&path, "line-one\nline-two\nline-three\n").expect("write test file");

        let mut chunks = Vec::new();
        let path_str = path.to_string_lossy().to_string();
        let (count, truncated) = stream_archive_chunks(&path_str, 10, 16, |idx, text| {
            chunks.push((idx, text));
            Ok(())
        })
        .expect("chunking should succeed");

        let _ = fs::remove_file(&path);

        assert_eq!(count, 3);
        assert!(!truncated);
        assert_eq!(chunks.len(), 3);
        assert!(chunks[0].1.contains("line-one"));
        assert!(chunks[1].1.contains("line-two"));
        assert!(chunks[2].1.contains("line-three"));
    }

    #[test]
    fn summarize_provider_mix_reports_mixed_counts() {
        let mut counts = BTreeMap::new();
        counts.insert("local".to_string(), 2usize);
        counts.insert("gemini".to_string(), 3usize);
        let label = summarize_provider_mix(&counts);
        assert!(label.starts_with("mixed("));
        assert!(label.contains("local:2"));
        assert!(label.contains("gemini:3"));
    }



    #[test]
    fn test_extract_keywords() {
        let text = "We need to fix the WebGL rendering bug on Safari. Also investigate the auth-token expiration issue.";
        let entry = super::ProjectionEntry {
            timestamp_epoch: None,
            role: "user".to_string(),
            content: text.to_string(),
            tool_name: None,
            tool_target: None,
            priority: None,
            coupled_result: None,
        };
        let keywords = super::extract_keywords(&[entry]);
        assert!(keywords.contains(&"webgl".to_string()) || keywords.contains(&"safari".to_string()) || keywords.contains(&"auth-token".to_string()));
    }
}
