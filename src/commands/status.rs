use anyhow::Result;
use serde_json::Value;

use crate::commands::CommandReport;
use crate::moon::config::{
    MoonContextCompactionAuthority, MoonContextPruneMode, MoonContextWindowMode,
    load_context_policy_if_explicit_env,
};
use crate::openclaw::config;
use crate::openclaw::gateway;
use crate::openclaw::paths::resolve_paths;
use crate::openclaw::plugin_verify;

#[derive(Debug, Clone, Default)]
pub struct StatusSnapshot {
    pub plugin_enabled: bool,
    pub context_pruning_mode: bool,
    pub context_pruning_soft_trim: bool,
    pub plugin_max_tokens: bool,
    pub plugin_max_chars: bool,
    pub plugin_max_retained_bytes: bool,
    pub plugin_read_profile_tokens: bool,
}

#[derive(Debug, Clone, Default)]
struct InstallRecordSnapshot {
    source: Option<String>,
    source_path: Option<String>,
    install_path: Option<String>,
}

fn path_exists(root: &Value, path: &[&str]) -> bool {
    let mut cursor = root;
    for part in path {
        let Some(next) = cursor.get(*part) else {
            return false;
        };
        cursor = next;
    }
    true
}

fn path_value<'a>(root: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cursor = root;
    for part in path {
        let next = cursor.get(*part)?;
        cursor = next;
    }
    Some(cursor)
}

fn path_string(root: &Value, path: &[&str]) -> Option<String> {
    path_value(root, path)
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn path_u64(root: &Value, path: &[&str]) -> Option<u64> {
    path_value(root, path).and_then(Value::as_u64)
}

fn install_record_snapshot(root: &Value, plugin_id: &str) -> InstallRecordSnapshot {
    InstallRecordSnapshot {
        source: path_string(root, &["plugins", "installs", plugin_id, "source"]),
        source_path: path_string(root, &["plugins", "installs", plugin_id, "sourcePath"]),
        install_path: path_string(root, &["plugins", "installs", plugin_id, "installPath"]),
    }
}

pub fn config_snapshot(root: &Value, plugin_id: &str) -> StatusSnapshot {
    StatusSnapshot {
        plugin_enabled: root
            .get("plugins")
            .and_then(|v| v.get("entries"))
            .and_then(|v| v.get(plugin_id))
            .and_then(|v| v.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        context_pruning_mode: path_exists(root, &["agents", "defaults", "contextPruning", "mode"]),
        context_pruning_soft_trim: path_exists(
            root,
            &[
                "agents",
                "defaults",
                "contextPruning",
                "softTrim",
                "maxChars",
            ],
        ),
        plugin_max_tokens: path_exists(
            root,
            &["plugins", "entries", plugin_id, "config", "maxTokens"],
        ),
        plugin_max_chars: path_exists(
            root,
            &["plugins", "entries", plugin_id, "config", "maxChars"],
        ),
        plugin_max_retained_bytes: path_exists(
            root,
            &[
                "plugins",
                "entries",
                plugin_id,
                "config",
                "maxRetainedBytes",
            ],
        ),
        plugin_read_profile_tokens: path_exists(
            root,
            &[
                "plugins",
                "entries",
                plugin_id,
                "config",
                "tools",
                "read",
                "maxTokens",
            ],
        ),
    }
}

pub fn run() -> Result<CommandReport> {
    eprintln!("DEBUG: status::run started");
    let paths = resolve_paths()?;
    eprintln!("DEBUG: paths resolved: {:?}", paths);
    let mut report = CommandReport::new("status");

    let cfg = config::read_config_value(&paths)?;
    let snapshot = config_snapshot(&cfg, &paths.plugin_id);
    let install_snapshot = install_record_snapshot(&cfg, &paths.plugin_id);
    let context_policy = load_context_policy_if_explicit_env()?;
    let verify = plugin_verify::verify_plugin(&paths)?;

    let state_dir_disp = paths.state_dir.display().to_string();
    let config_path_disp = paths.config_path.display().to_string();
    let plugin_dir_disp = paths.plugin_dir.display().to_string();

    report.detail(format!("state_dir={}", state_dir_disp.trim()));
    report.detail(format!("config_path={}", config_path_disp.trim()));
    report.detail(format!("plugin_dir={}", plugin_dir_disp.trim()));

    report.detail(format!("plugin_present_on_disk={}", verify.present_on_disk));
    report.detail(format!("plugin_listed_by_openclaw={}", verify.listed_by_openclaw));
    report.detail(format!("plugin_loaded_by_openclaw={}", verify.loaded_by_openclaw));
    report.detail(format!("plugin_assets_match_local={}", verify.assets_match_local));
    report.detail(format!("plugin_enabled={}", snapshot.plugin_enabled));

    if let Some(s) = &install_snapshot.source {
        report.detail(format!("install_record.source={}", s.trim()));
    }
    if let Some(s) = &install_snapshot.source_path {
        report.detail(format!("install_record.sourcePath={}", s.trim()));
    }
    if let Some(s) = &install_snapshot.install_path {
        report.detail(format!("install_record.installPath={}", s.trim()));
    }

    if let Some(v) = path_value(&cfg, &["plugins", "entries", &paths.plugin_id, "config", "maxTokens"]) {
        report.detail(format!("plugin_config.maxTokens={}", v.to_string().trim()));
    }
    if let Some(v) = path_value(&cfg, &["plugins", "entries", &paths.plugin_id, "config", "maxChars"]) {
        report.detail(format!("plugin_config.maxChars={}", v.to_string().trim()));
    }
    if let Some(v) = path_value(&cfg, &["plugins", "entries", &paths.plugin_id, "config", "maxRetainedBytes"]) {
        report.detail(format!("plugin_config.maxRetainedBytes={}", v.to_string().trim()));
    }
    if let Some(v) = path_value(&cfg, &["agents", "defaults", "contextTokens"]) {
        report.detail(format!("agents.defaults.contextTokens={}", v.to_string().trim()));
    }
    if let Some(v) = path_value(&cfg, &["agents", "defaults", "compaction", "mode"]) {
        report.detail(format!("agents.defaults.compaction.mode={}", v.to_string().trim()));
    }
    if let Some(policy) = &context_policy {
        report.detail(format!(
            "context.policy=window_mode={:?} prune_mode={:?} compaction_authority={:?} start_ratio={} emergency_ratio={} recover_ratio={}",
            policy.window_mode,
            policy.prune_mode,
            policy.compaction_authority,
            policy.compaction_start_ratio,
            policy.compaction_emergency_ratio,
            policy.compaction_recover_ratio
        ));
    } else {
        report.detail(
            "context.policy=legacy (no explicit MOON_CONFIG_PATH/MOON_HOME context section)"
                .to_string(),
        );
    }

    let context_tokens = path_u64(&cfg, &["agents", "defaults", "contextTokens"]);
    let compaction_mode = path_string(&cfg, &["agents", "defaults", "compaction", "mode"]);
    if let Some(policy) = &context_policy {
        match policy.prune_mode {
            MoonContextPruneMode::Disabled => {
                if snapshot.context_pruning_mode {
                    report.issue(
                        "context policy drift: agents.defaults.contextPruning must be disabled"
                            .to_string(),
                    );
                }
            }
            MoonContextPruneMode::Guarded => {
                if !snapshot.context_pruning_mode {
                    report.issue("missing agents.defaults.contextPruning.mode");
                }
                if !snapshot.context_pruning_soft_trim {
                    report.issue("missing agents.defaults.contextPruning.softTrim.maxChars");
                }
            }
        }

        match policy.window_mode {
            MoonContextWindowMode::Inherit => {
                if context_tokens.is_some() {
                    report.issue(
                        "context policy drift: agents.defaults.contextTokens must be unset when window_mode=inherit"
                            .to_string(),
                    );
                } else {
                    report.detail(
                        "agents.defaults.contextTokens unset by policy (window_mode=inherit)"
                            .to_string(),
                    );
                }
            }
            MoonContextWindowMode::Fixed => {
                let expected = policy
                    .window_tokens
                    .unwrap_or(config::MIN_AGENT_CONTEXT_TOKENS);
                if context_tokens != Some(expected) {
                    report.issue(format!(
                        "context policy drift: agents.defaults.contextTokens expected {expected}, found {}",
                        context_tokens
                            .map(|v| v.to_string())
                            .unwrap_or_else(|| "<missing>".to_string())
                    ));
                }
            }
        }

        let expected_compaction_mode = match policy.compaction_authority {
            MoonContextCompactionAuthority::Moon => config::MOON_AUTHORITY_COMPACTION_MODE,
            MoonContextCompactionAuthority::Openclaw => config::OPENCLAW_AUTHORITY_COMPACTION_MODE,
        };
        if compaction_mode.as_deref() != Some(expected_compaction_mode) {
            let auth = match policy.compaction_authority {
                MoonContextCompactionAuthority::Moon => "moon",
                MoonContextCompactionAuthority::Openclaw => "openclaw",
            };
            report.issue(format!(
                "context policy drift: agents.defaults.compaction.mode expected {expected_compaction_mode} when compaction_authority={auth}, found {}",
                compaction_mode.unwrap_or_else(|| "<missing>".to_string())
            ));
        }
    } else {
        if !snapshot.context_pruning_mode {
            report.issue("missing agents.defaults.contextPruning.mode");
        }
        if !snapshot.context_pruning_soft_trim {
            report.issue("missing agents.defaults.contextPruning.softTrim.maxChars");
        }
        if context_tokens.is_none() {
            report.detail(
                "agents.defaults.contextTokens not set (using OpenClaw/model default)".to_string(),
            );
        } else if let Some(v) = context_tokens
            && v < config::MIN_AGENT_CONTEXT_TOKENS
        {
            report.issue(format!(
                "agents.defaults.contextTokens too low ({v}); minimum is {}",
                config::MIN_AGENT_CONTEXT_TOKENS
            ));
        }
    }

    if !snapshot.plugin_max_tokens {
        report.issue("missing plugins.entries.moon.config.maxTokens");
    }
    if !snapshot.plugin_max_chars {
        report.issue("missing plugins.entries.moon.config.maxChars");
    }
    if !snapshot.plugin_max_retained_bytes {
        report.issue("missing plugins.entries.moon.config.maxRetainedBytes");
    }
    if !snapshot.plugin_read_profile_tokens {
        report.issue("missing plugins.entries.moon.config.tools.read.maxTokens");
    }
    if !verify.present_on_disk {
        report.issue("plugin files missing on disk");
    }
    if !verify.assets_match_local {
        report.issue("installed plugin assets drift from local package assets");
    }
    if gateway::openclaw_available() && !verify.listed_by_openclaw {
        report.issue("plugin not listed by `openclaw plugins list --json`");
    }
    if gateway::openclaw_available() && !verify.loaded_by_openclaw {
        report.issue("plugin is listed but not loaded");
    }
    if gateway::openclaw_available() && verify.provenance_warning_detected {
        report.issue(
            "plugin loaded without install/load-path provenance per `openclaw plugins list --json` diagnostics",
        );
    }

    let expected_plugin_dir = paths.plugin_dir.display().to_string();
    let mut install_record_reasons = Vec::new();
    if install_snapshot.source.as_deref() != Some("path") {
        install_record_reasons.push(format!(
            "plugins.installs.{}.source expected \"path\", found {}",
            paths.plugin_id,
            install_snapshot.source.as_deref().unwrap_or("<missing>")
        ));
    }
    if install_snapshot.source_path.as_deref() != Some(expected_plugin_dir.as_str()) {
        install_record_reasons.push(format!(
            "plugins.installs.{}.sourcePath expected {}, found {}",
            paths.plugin_id,
            expected_plugin_dir,
            install_snapshot
                .source_path
                .as_deref()
                .unwrap_or("<missing>")
        ));
    }
    if install_snapshot.install_path.as_deref() != Some(expected_plugin_dir.as_str()) {
        install_record_reasons.push(format!(
            "plugins.installs.{}.installPath expected {}, found {}",
            paths.plugin_id,
            expected_plugin_dir,
            install_snapshot
                .install_path
                .as_deref()
                .unwrap_or("<missing>")
        ));
    }
    if !install_record_reasons.is_empty() {
        if verify.provenance_warning_detected {
            report.issue(format!(
                "install record drift: {}",
                install_record_reasons.join("; ")
            ));
        } else {
            report.detail(format!(
                "provenance repair hint: {}",
                install_record_reasons.join("; ")
            ));
        }
    }
    if !snapshot.plugin_enabled {
        report.issue("plugin entry is not enabled in config");
    }

    Ok(report)
}
