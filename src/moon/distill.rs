use crate::moon::audit;
use crate::moon::paths::MoonPaths;
use anyhow::{Context, Result};
use chrono::{Datelike, Local};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct DistillInput {
    pub session_id: String,
    pub archive_path: String,
    pub archive_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistillOutput {
    pub provider: String,
    pub summary: String,
    pub summary_path: String,
    pub audit_log_path: String,
    pub created_at_epoch_secs: u64,
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
const MAX_CANDIDATE_CHARS: usize = 280;
const MAX_SUMMARY_CHARS: usize = 12_000;
const MAX_PROMPT_LINES: usize = 80;
const MAX_MODEL_LINES: usize = 80;
const MIN_MODEL_BULLETS: usize = 3;
const REQUEST_TIMEOUT_SECS: u64 = 45;

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
    if let Some((prefix, model)) = trimmed.split_once(':') {
        if let Some(provider) = parse_provider_alias(prefix) {
            return (Some(provider), model.trim().to_string());
        }
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

fn now_secs() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_secs())
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

fn normalize_text(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clean_candidate_text(input: &str) -> Option<String> {
    let normalized = normalize_text(input);
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

fn extract_candidate_lines(raw: &str) -> Vec<String> {
    let mut out = Vec::new();

    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Ok(entry) = serde_json::from_str::<Value>(trimmed) {
            push_message_candidates(&entry, &mut out);
        } else if !looks_like_json_blob(trimmed) {
            if let Some(cleaned) = clean_candidate_text(trimmed) {
                out.push(cleaned);
            }
        }

        if out.len() >= 200 {
            break;
        }
    }

    out
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
        "Summarize this session into concise bullets under headings for Decisions, Rules, Milestones, and Open Tasks. Return markdown only.\nSession id: {}\nArchive path: {}\n\nContext lines:\n{}",
        input.session_id, input.archive_path, context
    )
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
        if looks_like_json_blob(trimmed) || trimmed.contains("<<<EXTERNAL_UNTRUSTED_CONTENT>>>") {
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

fn daily_memory_path(paths: &MoonPaths) -> String {
    let now = Local::now();
    let date = format!("{:04}-{:02}-{:02}", now.year(), now.month(), now.day());
    paths
        .memory_dir
        .join(format!("{}.md", date))
        .display()
        .to_string()
}

pub fn run_distillation(paths: &MoonPaths, input: &DistillInput) -> Result<DistillOutput> {
    fs::create_dir_all(&paths.memory_dir)
        .with_context(|| format!("failed to create {}", paths.memory_dir.display()))?;

    let local = LocalDistiller;
    let local_summary = local.distill(input)?;
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
                None => ("local".to_string(), local_summary.clone()),
            },
            Err(_) => ("local".to_string(), local_summary.clone()),
        }
    } else {
        ("local".to_string(), local_summary)
    };
    let summary = clamp_summary(&generated_summary);

    let summary_path = daily_memory_path(paths);
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
            "distilled session {} into {}",
            input.session_id, summary_path
        ),
    )?;

    Ok(DistillOutput {
        provider: provider_used,
        summary,
        summary_path: summary_path.clone(),
        audit_log_path: paths.logs_dir.join("audit.log").display().to_string(),
        created_at_epoch_secs: now_secs()?,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        DistillInput, Distiller, LocalDistiller, MAX_SUMMARY_CHARS, RemoteProvider, clamp_summary,
        extract_anthropic_text, extract_openai_compatible_text, extract_openai_text,
        infer_provider_from_model, parse_prefixed_model, sanitize_model_summary,
    };
    use serde_json::json;

    #[test]
    fn local_distiller_avoids_raw_jsonl_payloads() {
        let input = DistillInput {
            session_id: "s".to_string(),
            archive_path: "/tmp/s.jsonl".to_string(),
            archive_text: format!(
                "{{\"type\":\"message\",\"message\":{{\"role\":\"toolResult\",\"content\":[{{\"type\":\"text\",\"text\":\"{{\\\"payload\\\":\\\"{}\\\"}}\"}}]}}}}\n{{\"type\":\"message\",\"message\":{{\"role\":\"user\",\"content\":[{{\"type\":\"text\",\"text\":\"Decision: set qmd mask to jsonl for archive indexing.\"}}]}}}}\n",
                "X".repeat(4096)
            ),
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
}
