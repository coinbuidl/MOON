use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn write_fake_qmd(bin_path: &Path) {
    let script = "#!/usr/bin/env bash\nexit 0\n";
    fs::write(bin_path, script).expect("write fake qmd");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(bin_path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(bin_path, perms).expect("chmod");
    }
}

fn write_fake_openclaw(bin_path: &Path) {
    let script = r#"#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "sessions" && "${2:-}" == "current" && "${3:-}" == "--json" ]]; then
  echo '{"sessionId":"current","usage":{"totalTokens":120},"limits":{"maxTokens":10000}}'
  exit 0
fi

if [[ "${1:-}" == "system" && "${2:-}" == "event" ]]; then
  if [[ -n "${MOON_TEST_EVENT_LOG:-}" ]]; then
    printf "%s\n" "$*" >> "${MOON_TEST_EVENT_LOG}"
  fi
  exit 0
fi

exit 0
"#;
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
fn moon_watch_once_triggers_pipeline_with_low_thresholds() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let sessions_dir = tmp.path().join("sessions");
    fs::create_dir_all(moon_home.join("archives")).expect("mkdir archives");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("skills/moon-system/logs")).expect("mkdir logs");
    fs::create_dir_all(&sessions_dir).expect("mkdir sessions");
    fs::write(
        sessions_dir.join("s1.json"),
        "{\"decision\":\"use moon\"}\n",
    )
    .expect("write session");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(&qmd);

    assert_cmd::cargo::cargo_bin_cmd!("oc-token-optim")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("OPENCLAW_SESSIONS_DIR", &sessions_dir)
        .env("QMD_BIN", &qmd)
        .env("MOON_THRESHOLD_ARCHIVE_RATIO", "0.00001")
        .env("MOON_THRESHOLD_PRUNE_RATIO", "0.00002")
        .env("MOON_THRESHOLD_DISTILL_RATIO", "0.00003")
        .arg("moon-watch")
        .arg("--once")
        .assert()
        .success();

    let state_file = moon_home.join("state/moon_state.json");
    assert!(state_file.exists());
    let ledger = moon_home.join("archives/ledger.jsonl");
    assert!(ledger.exists());
    let continuity_dir = moon_home.join("continuity");
    assert!(continuity_dir.exists());
}

#[test]
fn moon_watch_once_triggers_inbound_system_event_for_new_file() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let sessions_dir = tmp.path().join("sessions");
    let inbound_dir = tmp.path().join("inbound");
    let event_log = tmp.path().join("events.log");
    fs::create_dir_all(moon_home.join("archives")).expect("mkdir archives");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("skills/moon-system/logs")).expect("mkdir logs");
    fs::create_dir_all(&sessions_dir).expect("mkdir sessions");
    fs::create_dir_all(&inbound_dir).expect("mkdir inbound");
    fs::write(
        sessions_dir.join("s1.json"),
        "{\"decision\":\"watch inbound\"}\n",
    )
    .expect("write session");
    fs::write(inbound_dir.join("task.md"), "run this file\n").expect("write inbound file");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(&qmd);
    let openclaw = tmp.path().join("openclaw");
    write_fake_openclaw(&openclaw);

    assert_cmd::cargo::cargo_bin_cmd!("oc-token-optim")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("OPENCLAW_SESSIONS_DIR", &sessions_dir)
        .env("QMD_BIN", &qmd)
        .env("OPENCLAW_BIN", &openclaw)
        .env("MOON_TEST_EVENT_LOG", &event_log)
        .env("MOON_THRESHOLD_ARCHIVE_RATIO", "0.00001")
        .env("MOON_THRESHOLD_PRUNE_RATIO", "0.00002")
        .env("MOON_THRESHOLD_DISTILL_RATIO", "0.00003")
        .env("MOON_INBOUND_WATCH_ENABLED", "true")
        .env(
            "MOON_INBOUND_WATCH_PATHS",
            inbound_dir.to_string_lossy().to_string(),
        )
        .arg("moon-watch")
        .arg("--once")
        .assert()
        .success();

    let events = fs::read_to_string(&event_log).expect("read event log");
    assert!(events.contains("system event --text"));
    assert!(events.contains("Moon System inbound file detected"));
    assert!(events.contains("task.md"));

    let state_file = moon_home.join("state/moon_state.json");
    let state_raw = fs::read_to_string(state_file).expect("read state");
    assert!(state_raw.contains("inbound_seen_files"));
}
