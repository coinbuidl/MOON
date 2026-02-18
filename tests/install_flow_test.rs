use serde_json::Value;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn write_fake_openclaw(bin_path: &Path, log_path: &Path) {
    let script = format!(
        "#!/usr/bin/env bash\necho \"$@\" >> \"{}\"\nif [ \"$1\" = \"plugins\" ] && [ \"$2\" = \"list\" ]; then\n  echo '[{{\"id\":\"oc-token-optim\"}}]'\nfi\nexit 0\n",
        log_path.display()
    );
    fs::write(bin_path, script).expect("write fake openclaw");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(bin_path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(bin_path, perms).expect("chmod");
    }
}

#[test]
fn install_creates_plugin_and_stage2_config_entries() {
    let tmp = tempdir().expect("tempdir");
    let state_dir = tmp.path().join("state");
    fs::create_dir_all(&state_dir).expect("mkdir");
    let config_path = state_dir.join("openclaw.json");
    fs::write(&config_path, "{}\n").expect("write config");

    let fake_openclaw = tmp.path().join("openclaw");
    let log_path = tmp.path().join("openclaw.log");
    write_fake_openclaw(&fake_openclaw, &log_path);

    assert_cmd::cargo::cargo_bin_cmd!("oc-token-optim")
        .current_dir(tmp.path())
        .env("OPENCLAW_STATE_DIR", &state_dir)
        .env("OPENCLAW_CONFIG_PATH", &config_path)
        .env("OPENCLAW_BIN", &fake_openclaw)
        .arg("install")
        .assert()
        .success();

    let plugin_dir = state_dir.join("extensions").join("oc-token-optim");
    assert!(plugin_dir.join("index.js").exists());
    assert!(plugin_dir.join("openclaw.plugin.json").exists());
    assert!(plugin_dir.join("package.json").exists());

    let cfg: Value = serde_json::from_str(&fs::read_to_string(&config_path).expect("read config"))
        .expect("parse cfg");
    assert_eq!(
        cfg.get("plugins")
            .and_then(|v| v.get("entries"))
            .and_then(|v| v.get("oc-token-optim"))
            .and_then(|v| v.get("enabled"))
            .and_then(Value::as_bool),
        Some(true)
    );

    assert_eq!(
        cfg.get("plugins")
            .and_then(|v| v.get("entries"))
            .and_then(|v| v.get("oc-token-optim"))
            .and_then(|v| v.get("config"))
            .and_then(|v| v.get("maxTokens"))
            .and_then(Value::as_i64),
        Some(12_000)
    );

    assert_eq!(
        cfg.get("plugins")
            .and_then(|v| v.get("entries"))
            .and_then(|v| v.get("oc-token-optim"))
            .and_then(|v| v.get("config"))
            .and_then(|v| v.get("tools"))
            .and_then(|v| v.get("read"))
            .and_then(|v| v.get("maxTokens"))
            .and_then(Value::as_i64),
        Some(6_000)
    );

    assert!(
        cfg.get("agents")
            .and_then(|v| v.get("defaults"))
            .and_then(|v| v.get("contextPruning"))
            .and_then(|v| v.get("softTrim"))
            .and_then(|v| v.get("maxChars"))
            .is_some()
    );
    assert_eq!(
        cfg.get("agents")
            .and_then(|v| v.get("defaults"))
            .and_then(|v| v.get("contextTokens"))
            .and_then(Value::as_i64),
        Some(16_000)
    );
}
