use std::fs;
use tempfile::tempdir;

#[test]
fn moon_snapshot_copies_latest_session_file_to_archives() {
    let tmp = tempdir().expect("tempdir");
    let sessions_dir = tmp.path().join("sessions");
    let archives_dir = tmp.path().join("archives");
    fs::create_dir_all(&sessions_dir).expect("mkdir sessions");

    let source = sessions_dir.join("main-session.json");
    fs::write(&source, "{\"hello\":\"moon\"}\n").expect("write source");

    assert_cmd::cargo::cargo_bin_cmd!("moon")
        .current_dir(tmp.path())
        .env("OPENCLAW_SESSIONS_DIR", &sessions_dir)
        .env("MOON_ARCHIVES_DIR", &archives_dir)
        .arg("moon-snapshot")
        .assert()
        .success();

    let raw_archives_dir = archives_dir.join("raw");
    let entries = fs::read_dir(&raw_archives_dir).expect("read raw archives");
    let mut count = 0usize;
    for entry in entries {
        let path = entry.expect("entry").path();
        if path.is_file() {
            count += 1;
        }
    }
    assert_eq!(count, 1);
}
