use crate::moon::paths::MoonPaths;
use crate::openclaw::config::{MIN_AGENT_CONTEXT_TOKENS, read_config_value, write_config_atomic};
use crate::openclaw::paths::resolve_paths;
use anyhow::Result;
use serde_json::Value;

fn set_path(root: &mut Value, path: &[&str], value: Value) {
    if path.is_empty() {
        return;
    }

    let mut cursor = root;
    for key in &path[..path.len() - 1] {
        if !cursor.is_object() {
            *cursor = serde_json::json!({});
        }
        let obj = cursor.as_object_mut().expect("object");
        cursor = obj
            .entry((*key).to_string())
            .or_insert_with(|| serde_json::json!({}));
    }

    if !cursor.is_object() {
        *cursor = serde_json::json!({});
    }
    let obj = cursor.as_object_mut().expect("object");
    obj.insert(path[path.len() - 1].to_string(), value);
}

fn path_u64(root: &Value, path: &[&str]) -> Option<u64> {
    let mut cursor = root;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_u64()
}

fn set_path_u64_floor(root: &mut Value, path: &[&str], floor: u64) -> bool {
    let current = path_u64(root, path);
    if current.is_some_and(|v| v >= floor) {
        return false;
    }
    set_path(root, path, Value::from(floor));
    true
}

pub fn apply_aggressive_profile(_paths: &MoonPaths, plugin_id: &str) -> Result<String> {
    let enabled = std::env::var("MOON_ENABLE_PRUNE_WRITE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !enabled {
        return Ok("skipped (set MOON_ENABLE_PRUNE_WRITE=true to enable writes)".to_string());
    }

    let oc_paths = resolve_paths()?;
    let mut cfg = read_config_value(&oc_paths)?;
    let mut changed = false;

    let reserve_floor = path_u64(&cfg, &["agents", "defaults", "contextTokens"])
        .unwrap_or(MIN_AGENT_CONTEXT_TOKENS)
        .max(MIN_AGENT_CONTEXT_TOKENS);
    changed |= set_path_u64_floor(
        &mut cfg,
        &["agents", "defaults", "contextTokens"],
        reserve_floor,
    );

    changed |= set_path_u64_floor(
        &mut cfg,
        &["plugins", "entries", plugin_id, "config", "maxTokens"],
        12_000,
    );
    changed |= set_path_u64_floor(
        &mut cfg,
        &["plugins", "entries", plugin_id, "config", "maxChars"],
        60_000,
    );
    changed |= set_path_u64_floor(
        &mut cfg,
        &[
            "plugins",
            "entries",
            plugin_id,
            "config",
            "maxRetainedBytes",
        ],
        250_000,
    );

    if !changed {
        return Ok(format!(
            "unchanged (safety floors already satisfied): {}",
            oc_paths.config_path.display()
        ));
    }

    write_config_atomic(&oc_paths, &cfg)
}
