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
