use crate::moon::paths::MoonPaths;
use crate::openclaw::config::{read_config_value, write_config_atomic};
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

pub fn apply_aggressive_profile(_paths: &MoonPaths, plugin_id: &str) -> Result<String> {
    let enabled = std::env::var("MOON_ENABLE_PRUNE_WRITE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    if !enabled {
        return Ok("skipped (set MOON_ENABLE_PRUNE_WRITE=true to enable writes)".to_string());
    }

    let oc_paths = resolve_paths()?;
    let mut cfg = read_config_value(&oc_paths)?;

    set_path(
        &mut cfg,
        &["plugins", "entries", plugin_id, "config", "maxTokens"],
        Value::from(8000),
    );
    set_path(
        &mut cfg,
        &["plugins", "entries", plugin_id, "config", "maxChars"],
        Value::from(40000),
    );
    set_path(
        &mut cfg,
        &[
            "plugins",
            "entries",
            plugin_id,
            "config",
            "maxRetainedBytes",
        ],
        Value::from(100000),
    );

    write_config_atomic(&oc_paths, &cfg)
}
