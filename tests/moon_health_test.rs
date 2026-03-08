#![cfg(not(windows))]

use predicates::str::contains;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;

#[test]
fn moon_health_treats_fresh_heartbeat_as_activity_when_lock_is_missing() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("workspace");
    let archives_dir = moon_home.join("archives");
    let logs_dir = moon_home.join("moon").join("logs");
    let state_dir = moon_home.join("moon").join("state");
    let state_file = state_dir.join("moon_state.json");

    fs::create_dir_all(&archives_dir).expect("mkdir archives");
    fs::create_dir_all(&logs_dir).expect("mkdir logs");
    fs::create_dir_all(&state_dir).expect("mkdir state");

    let now_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_secs();

    fs::write(
        &state_file,
        format!(
            "{{\n  \"schema_version\": 3,\n  \"last_heartbeat_epoch_secs\": {now_epoch},\n  \"last_archive_trigger_epoch_secs\": null,\n  \"last_compaction_trigger_epoch_secs\": null,\n  \"last_distill_trigger_epoch_secs\": null,\n  \"last_syns_trigger_epoch_secs\": null,\n  \"last_embed_trigger_epoch_secs\": null,\n  \"last_session_id\": null,\n  \"last_usage_ratio\": null,\n  \"last_provider\": null,\n  \"distilled_archives\": {{}},\n  \"embedded_projections\": {{}},\n  \"compaction_hysteresis_active\": {{}},\n  \"inbound_seen_files\": {{}}\n}}\n"
        ),
    )
    .expect("write state");

    assert_cmd::cargo::cargo_bin_cmd!("moon")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .arg("health")
        .assert()
        .success()
        .stdout(contains(
            "daemon.lock=not_found (recent heartbeat age_secs=",
        ))
        .stdout(contains(
            "daemon may still be running without a linked lockfile",
        ));
}
