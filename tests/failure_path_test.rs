use predicates::str::contains;
use std::fs;
use tempfile::tempdir;

#[test]
fn verify_fails_when_openclaw_binary_missing() {
    let tmp = tempdir().expect("tempdir");
    let state_dir = tmp.path().join("state");
    fs::create_dir_all(&state_dir).expect("mkdir");
    let config_path = state_dir.join("openclaw.json");
    fs::write(&config_path, "{}\n").expect("write config");

    assert_cmd::cargo::cargo_bin_cmd!("moon")
        .current_dir(tmp.path())
        .env("OPENCLAW_STATE_DIR", &state_dir)
        .env("OPENCLAW_CONFIG_PATH", &config_path)
        .env("OPENCLAW_BIN", "/definitely/not/a/real/openclaw")
        .arg("verify")
        .assert()
        .failure()
        .stdout(contains("OPENCLAW_BIN is required"));
}

#[test]
fn install_fails_when_config_invalid() {
    let tmp = tempdir().expect("tempdir");
    let state_dir = tmp.path().join("state");
    fs::create_dir_all(&state_dir).expect("mkdir");
    let config_path = state_dir.join("openclaw.json");
    fs::write(&config_path, "{not valid json5 :::").expect("write config");

    assert_cmd::cargo::cargo_bin_cmd!("moon")
        .current_dir(tmp.path())
        .env("OPENCLAW_STATE_DIR", &state_dir)
        .env("OPENCLAW_CONFIG_PATH", &config_path)
        .arg("install")
        .assert()
        .failure()
        .stderr(contains("failed to parse config as JSON/JSON5"));
}
