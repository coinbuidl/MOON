#![cfg(not(windows))]
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn write_fake_qmd(bin_path: &Path, payload: &str) {
    let script = format!(
        "#!/usr/bin/env bash\necho '{}'\n",
        payload.replace('\'', "'\"'\"'")
    );
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
#[cfg(not(windows))]
fn moon_recall_returns_matches() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    fs::create_dir_all(moon_home.join("archives")).expect("mkdir archives");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("moon/logs")).expect("mkdir logs");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(
        &qmd,
        r#"[{"path":"/tmp/a.json","snippet":"rule captured","score":0.8}]"#,
    );

    assert_cmd::cargo::cargo_bin_cmd!("moon")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("QMD_BIN", &qmd)
        .arg("moon-recall")
        .args(["--query", "rule"])
        .assert()
        .success();
}

#[test]
#[cfg(not(windows))]
fn moon_recall_maps_qmd_file_uri_to_archive_path() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let archives = moon_home.join("archives");
    fs::create_dir_all(archives.join("raw")).expect("mkdir archives/raw");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("moon/logs")).expect("mkdir logs");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(
        &qmd,
        r#"[{"file":"qmd://history/mlib/test-session-1771470000.md","snippet":"timeline hit","score":0.8}]"#,
    );

    let expected = archives.join("raw/test-session-1771470000.jsonl");
    let assert = assert_cmd::cargo::cargo_bin_cmd!("moon")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("QMD_BIN", &qmd)
        .arg("moon-recall")
        .args(["--query", "timeline"])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(stdout.contains(&format!("match[0].archive={}", expected.display())));
}

#[test]
#[cfg(not(windows))]
fn moon_recall_prefers_exact_channel_archive_match() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let archives = moon_home.join("archives");
    let continuity = moon_home.join("continuity");
    fs::create_dir_all(&archives).expect("mkdir archives");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("moon/logs")).expect("mkdir logs");
    fs::create_dir_all(&continuity).expect("mkdir continuity");

    let deterministic_archive = archives.join("exact-session.jsonl");
    fs::write(&deterministic_archive, "{\"decision\":\"exact\"}\n").expect("write archive");

    let channel_key = "agent:main:discord:channel:1468330747428474974";
    fs::write(
        continuity.join("channel_archive_map.json"),
        format!(
            "{{\n  \"{channel_key}\": {{\n    \"channel_key\": \"{channel_key}\",\n    \"source_path\": \"/tmp/source.jsonl\",\n    \"archive_path\": \"{}\",\n    \"updated_at_epoch_secs\": 1771400000\n  }}\n}}\n",
            deterministic_archive.display()
        ),
    )
    .expect("write channel archive map");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(
        &qmd,
        r#"[{"path":"/tmp/semantic.json","snippet":"semantic recall","score":0.8}]"#,
    );

    let assert = assert_cmd::cargo::cargo_bin_cmd!("moon")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("QMD_BIN", &qmd)
        .arg("--json")
        .arg("moon-recall")
        .args(["--query", "where is old info"])
        .args(["--channel-key", channel_key])
        .assert()
        .success();

    let stdout = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(stdout.contains(&format!(
        "match[0].archive={}",
        deterministic_archive.display()
    )));
}
