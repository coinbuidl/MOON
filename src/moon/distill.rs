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

fn now_secs() -> Result<u64> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX_EPOCH")?
        .as_secs())
}

fn extract_signal_lines(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_ascii_lowercase();
        if lower.contains("decision")
            || lower.contains("rule")
            || lower.contains("todo")
            || lower.contains("next")
            || lower.contains("milestone")
        {
            out.push(trimmed.to_string());
        }
        if out.len() >= 20 {
            break;
        }
    }
    out
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
                .take(12)
                .map(ToOwned::to_owned)
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
        let prompt = format!(
            "Summarize this session into concise bullets: decisions, rules, milestones, and open tasks. Session id: {}. Archive path: {}. Return plain markdown bullets only.\n\n{}",
            input.session_id, input.archive_path, input.archive_text
        );

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
            .timeout(std::time::Duration::from_secs(45))
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
    let model =
        env::var("MOON_GEMINI_MODEL").unwrap_or_else(|_| "gemini-2.5-flash-lite".to_string());
    let summary = if let Ok(key) = env::var("GEMINI_API_KEY") {
        let gemini = GeminiDistiller {
            api_key: key,
            model,
        };
        match gemini.distill(input) {
            Ok(out) => out,
            Err(_) => local.distill(input)?,
        }
    } else {
        local.distill(input)?
    };

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
        provider: if env::var("GEMINI_API_KEY").is_ok() {
            "gemini-or-local-fallback".to_string()
        } else {
            "local".to_string()
        },
        summary,
        summary_path: summary_path.clone(),
        audit_log_path: paths.logs_dir.join("audit.log").display().to_string(),
        created_at_epoch_secs: now_secs()?,
    })
}
