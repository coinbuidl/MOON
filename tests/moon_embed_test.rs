#![cfg(not(windows))]
use predicates::str::contains;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn write_fake_qmd_bounded(bin_path: &Path, log_path: &Path) {
    let script = format!(
        r#"#!/usr/bin/env bash
set -euo pipefail
echo "$*" >> "{}"

if [[ "${{1:-}}" == "embed" && "${{2:-}}" == "--help" ]]; then
  echo "Usage: qmd embed <collection> --max-docs <n>"
  exit 0
fi

if [[ "${{1:-}}" == "embed" ]]; then
  exit 0
fi

exit 0
"#,
        log_path.display()
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

fn write_fake_qmd_missing_capability(bin_path: &Path) {
    let script = r#"#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "embed" && "${2:-}" == "--help" ]]; then
  echo "unknown command: embed" >&2
  exit 1
fi

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

#[test]
fn moon_embed_runs_bounded_and_updates_state() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let mlib_dir = moon_home.join("archives/mlib");
    fs::create_dir_all(&mlib_dir).expect("mkdir mlib");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("moon/logs")).expect("mkdir logs");

    fs::write(mlib_dir.join("a.md"), "a").expect("write a");
    fs::write(mlib_dir.join("b.md"), "b").expect("write b");

    let qmd = tmp.path().join("qmd");
    let qmd_log = tmp.path().join("qmd.log");
    write_fake_qmd_bounded(&qmd, &qmd_log);

    assert_cmd::cargo::cargo_bin_cmd!("moon")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("QMD_BIN", &qmd)
        .arg("--json")
        .arg("moon-embed")
        .args(["--name", "history"])
        .args(["--max-docs", "1"])
        .assert()
        .success()
        .stdout(contains("embed.selected_docs=1"))
        .stdout(contains("embed.pending_before=2"))
        .stdout(contains("embed.pending_after=1"));

    let log = fs::read_to_string(&qmd_log).expect("read qmd log");
    assert!(log.contains("embed --help"));
    assert!(log.contains("embed history --max-docs 1"));
}

#[test]
fn moon_embed_manual_fails_when_capability_missing() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let mlib_dir = moon_home.join("archives/mlib");
    fs::create_dir_all(&mlib_dir).expect("mkdir mlib");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("moon/logs")).expect("mkdir logs");
    fs::write(mlib_dir.join("x.md"), "x").expect("write x");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd_missing_capability(&qmd);

    assert_cmd::cargo::cargo_bin_cmd!("moon")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("QMD_BIN", &qmd)
        .arg("moon-embed")
        .assert()
        .failure()
        .stdout(contains("embed capability missing"));
}

#[test]
fn moon_embed_watcher_trigger_degrades_on_missing_capability() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let mlib_dir = moon_home.join("archives/mlib");
    fs::create_dir_all(&mlib_dir).expect("mkdir mlib");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("moon/logs")).expect("mkdir logs");
    fs::write(mlib_dir.join("x.md"), "x").expect("write x");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd_missing_capability(&qmd);

    assert_cmd::cargo::cargo_bin_cmd!("moon")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("QMD_BIN", &qmd)
        .arg("moon-embed")
        .arg("--watcher-trigger")
        .assert()
        .success()
        .stdout(contains("embed.skip_reason=capability-missing"));
}
