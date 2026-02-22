use predicates::str::contains;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn write_fake_openclaw(bin_path: &Path, log_path: &Path) {
    let script = format!(
        "#!/usr/bin/env bash\necho \"$@\" >> \"{}\"\nif [ \"$1\" = \"plugins\" ] && [ \"$2\" = \"list\" ]; then\n  echo '{{\"plugins\":[{{\"id\":\"MOON\"}}]}}'\nfi\nexit 0\n",
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
fn second_install_is_noop_for_plugin_sync() {
    let tmp = tempdir().expect("tempdir");
    let state_dir = tmp.path().join("state");
    fs::create_dir_all(&state_dir).expect("mkdir");
    let config_path = state_dir.join("openclaw.json");
    fs::write(&config_path, "{}\n").expect("write config");

    let fake_openclaw = tmp.path().join("openclaw");
    let log_path = tmp.path().join("openclaw.log");
    write_fake_openclaw(&fake_openclaw, &log_path);

    assert_cmd::cargo::cargo_bin_cmd!("MOON")
        .current_dir(tmp.path())
        .env("OPENCLAW_STATE_DIR", &state_dir)
        .env("OPENCLAW_CONFIG_PATH", &config_path)
        .env("OPENCLAW_BIN", &fake_openclaw)
        .arg("install")
        .assert()
        .success();

    assert_cmd::cargo::cargo_bin_cmd!("MOON")
        .current_dir(tmp.path())
        .env("OPENCLAW_STATE_DIR", &state_dir)
        .env("OPENCLAW_CONFIG_PATH", &config_path)
        .env("OPENCLAW_BIN", &fake_openclaw)
        .arg("install")
        .assert()
        .success()
        .stdout(contains("plugin_changed=false"));
}
