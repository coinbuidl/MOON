#![cfg(not(windows))]
use chrono::{Duration as ChronoDuration, TimeZone, Utc};
use predicates::str::contains;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;

fn write_fake_qmd(bin_path: &Path) {
    let script = r#"#!/usr/bin/env bash
set -euo pipefail
exit 0
"#;
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

fn read_distilled_paths(state_file: &Path) -> Vec<String> {
    let raw = fs::read_to_string(state_file).expect("read state");
    let parsed: Value = serde_json::from_str(&raw).expect("parse state");
    let map = parsed
        .get("distilled_archives")
        .and_then(Value::as_object)
        .expect("distilled_archives map");
    map.keys().cloned().collect()
}

#[test]
fn moon_watch_once_uses_moon_state_file_override() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let sessions_dir = tmp.path().join("sessions");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("moon/logs")).expect("mkdir logs");
    fs::create_dir_all(&sessions_dir).expect("mkdir sessions");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(&qmd);
    let openclaw = tmp.path().join("openclaw");
    write_fake_openclaw(&openclaw);

    let custom_state_file = tmp.path().join("custom-state").join("moon_state.json");

    assert_cmd::cargo::cargo_bin_cmd!("moon")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("MOON_STATE_FILE", &custom_state_file)
        .env("OPENCLAW_SESSIONS_DIR", &sessions_dir)
        .env("QMD_BIN", &qmd)
        .env("OPENCLAW_BIN", &openclaw)
        .arg("watch")
        .arg("--once")
        .assert()
        .success()
        .stdout(contains(format!(
            "state_file={}",
            custom_state_file.display()
        )));

    assert!(custom_state_file.exists());
    assert!(!moon_home.join("moon/state/moon_state.json").exists());
}

#[test]
fn moon_watch_once_dry_run_skips_state_write() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let sessions_dir = tmp.path().join("sessions");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("moon/logs")).expect("mkdir logs");
    fs::create_dir_all(&sessions_dir).expect("mkdir sessions");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(&qmd);
    let openclaw = tmp.path().join("openclaw");
    write_fake_openclaw(&openclaw);

    assert_cmd::cargo::cargo_bin_cmd!("moon")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("OPENCLAW_SESSIONS_DIR", &sessions_dir)
        .env("QMD_BIN", &qmd)
        .env("OPENCLAW_BIN", &openclaw)
        .arg("watch")
        .arg("--once")
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(contains("dry_run=true"));

    assert!(!moon_home.join("moon/state/moon_state.json").exists());
}

#[test]
fn moon_watch_once_distills_pending_mds_docs() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let sessions_dir = tmp.path().join("sessions");
    let mds_dir = moon_home.join("mds");
    fs::create_dir_all(&mds_dir).expect("mkdir mds");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("moon/logs")).expect("mkdir logs");
    fs::create_dir_all(&sessions_dir).expect("mkdir sessions");

    fs::write(
        mds_dir.join("fresh.md"),
        "# MOON Archive Markdown\n\nDecision: simplify the primary flow.\n",
    )
    .expect("write mds");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(&qmd);
    let openclaw = tmp.path().join("openclaw");
    write_fake_openclaw(&openclaw);

    assert_cmd::cargo::cargo_bin_cmd!("moon")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("OPENCLAW_SESSIONS_DIR", &sessions_dir)
        .env("QMD_BIN", &qmd)
        .env("OPENCLAW_BIN", &openclaw)
        .env("MOON_DISTILL_PROVIDER", "local")
        .env("MOON_DISTILL_MAX_PER_CYCLE", "1")
        .arg("watch")
        .arg("--once")
        .assert()
        .success()
        .stdout(contains("pending_mds_docs=1"))
        .stdout(contains("distill.runs=1"));

    let state_file = moon_home.join("moon/state/moon_state.json");
    let distilled = read_distilled_paths(&state_file);
    assert_eq!(distilled.len(), 1);
    assert!(distilled[0].contains("/mds/fresh.md"));
}

#[test]
fn moon_watch_once_runs_midnight_syns_from_yesterday_and_memory() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let sessions_dir = tmp.path().join("sessions");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("moon/logs")).expect("mkdir logs");
    fs::create_dir_all(moon_home.join("moon/state")).expect("mkdir state");
    fs::create_dir_all(&sessions_dir).expect("mkdir sessions");

    let now_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("epoch")
        .as_secs();
    let now_utc = Utc
        .timestamp_opt(now_epoch as i64, 0)
        .single()
        .expect("utc timestamp");
    let yesterday = (now_utc.date_naive() - ChronoDuration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let yesterday_file = moon_home.join("memory").join(format!("{yesterday}.md"));
    let memory_file = moon_home.join("MEMORY.md");
    fs::write(
        &yesterday_file,
        "# Daily Memory\n<!-- moon_memory_format: conversation_v1 -->\n\n## Session y1\n**User:** Keep workflow simple.\n**Assistant:** Use one path.\n",
    )
    .expect("write yesterday memory");
    fs::write(
        &memory_file,
        "# MEMORY\n\n## Durable\n- Keep summaries concise.\n",
    )
    .expect("write memory file");

    let midnight_state = format!(
        "{{\n  \"schema_version\": 3,\n  \"last_heartbeat_epoch_secs\": 0,\n  \"last_archive_trigger_epoch_secs\": null,\n  \"last_compaction_trigger_epoch_secs\": null,\n  \"last_distill_trigger_epoch_secs\": null,\n  \"last_syns_trigger_epoch_secs\": null,\n  \"last_embed_trigger_epoch_secs\": null,\n  \"last_session_id\": null,\n  \"last_usage_ratio\": null,\n  \"last_provider\": null,\n  \"distilled_archives\": {{}},\n  \"embedded_projections\": {{}},\n  \"inbound_seen_files\": {{}}\n}}\n"
    );
    fs::write(moon_home.join("moon/state/moon_state.json"), midnight_state).expect("write state");

    let fake_midnight_epoch = now_utc
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc()
        .timestamp() as u64;

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(&qmd);
    let openclaw = tmp.path().join("openclaw");
    write_fake_openclaw(&openclaw);

    assert_cmd::cargo::cargo_bin_cmd!("moon")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("OPENCLAW_SESSIONS_DIR", &sessions_dir)
        .env("QMD_BIN", &qmd)
        .env("OPENCLAW_BIN", &openclaw)
        .env("MOON_RESIDENTIAL_TIMEZONE", "UTC")
        .env("MOON_WISDOM_PROVIDER", "local")
        .env(
            "MOON_WATCH_FAKE_NOW_EPOCH_SECS",
            fake_midnight_epoch.to_string(),
        )
        .arg("watch")
        .arg("--once")
        .assert()
        .success();

    let state_raw =
        fs::read_to_string(moon_home.join("moon/state/moon_state.json")).expect("read state");
    assert!(state_raw.contains("\"last_syns_trigger_epoch_secs\":"));
}
