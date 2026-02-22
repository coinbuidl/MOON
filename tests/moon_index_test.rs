#![cfg(not(windows))]
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn write_fake_qmd(bin_path: &Path, log_path: &Path) {
    let script = format!(
        "#!/usr/bin/env bash\necho \"$@\" >> \"{}\"\nexit 0\n",
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

fn write_fake_qmd_add_conflict_then_update(bin_path: &Path, log_path: &Path) {
    let script = format!(
        "#!/usr/bin/env bash\n\
echo \"$@\" >> \"{}\"\n\
if [[ \"$1\" == \"collection\" && \"$2\" == \"add\" ]]; then\n\
  echo \"Collection 'history' already exists.\" >&2\n\
  exit 1\n\
fi\n\
if [[ \"$1\" == \"update\" ]]; then\n\
  exit 0\n\
fi\n\
exit 1\n",
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

fn write_fake_qmd_add_conflict_then_recreate(bin_path: &Path, log_path: &Path, marker: &Path) {
    let script = format!(
        "#!/usr/bin/env bash\n\
echo \"$@\" >> \"{}\"\n\
if [[ \"$1\" == \"collection\" && \"$2\" == \"add\" ]]; then\n\
  if [[ ! -f \"{}\" ]]; then\n\
    touch \"{}\"\n\
    echo \"Collection 'history' already exists.\" >&2\n\
    exit 1\n\
  fi\n\
  exit 0\n\
fi\n\
if [[ \"$1\" == \"collection\" && \"$2\" == \"list\" ]]; then\n\
  cat <<'EOF'\n\
Collections (1):\n\
\n\
history (qmd://history/)\n\
  Pattern:  **/*.jsonl\n\
  Files:    1\n\
  Updated:  0s ago\n\
EOF\n\
  exit 0\n\
fi\n\
if [[ \"$1\" == \"collection\" && \"$2\" == \"remove\" ]]; then\n\
  exit 0\n\
fi\n\
if [[ \"$1\" == \"update\" ]]; then\n\
  exit 0\n\
fi\n\
exit 1\n",
        log_path.display(),
        marker.display(),
        marker.display()
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
fn moon_index_registers_history_collection() {
    let tmp = tempdir().expect("tempdir");
    let archives_dir = tmp.path().join("archives");
    fs::create_dir_all(&archives_dir).expect("mkdir archives");

    let fake_qmd = tmp.path().join("qmd");
    let log_path = tmp.path().join("qmd.log");
    write_fake_qmd(&fake_qmd, &log_path);

    assert_cmd::cargo::cargo_bin_cmd!("MOON")
        .current_dir(tmp.path())
        .env("MOON_ARCHIVES_DIR", &archives_dir)
        .env("QMD_BIN", &fake_qmd)
        .arg("moon-index")
        .arg("--name")
        .arg("history")
        .assert()
        .success();

    let log = fs::read_to_string(&log_path).expect("read log");
    assert!(log.contains("collection add"));
    assert!(log.contains("--name history"));
    assert!(log.contains("--mask mlib/**/*.md"));
}

#[test]
#[cfg(not(windows))]
fn moon_index_updates_when_collection_already_exists() {
    let tmp = tempdir().expect("tempdir");
    let archives_dir = tmp.path().join("archives");
    fs::create_dir_all(&archives_dir).expect("mkdir archives");

    let fake_qmd = tmp.path().join("qmd");
    let log_path = tmp.path().join("qmd.log");
    write_fake_qmd_add_conflict_then_update(&fake_qmd, &log_path);

    assert_cmd::cargo::cargo_bin_cmd!("MOON")
        .current_dir(tmp.path())
        .env("MOON_ARCHIVES_DIR", &archives_dir)
        .env("QMD_BIN", &fake_qmd)
        .arg("moon-index")
        .arg("--name")
        .arg("history")
        .assert()
        .success();

    let log = fs::read_to_string(&log_path).expect("read log");
    assert!(log.contains("collection add"));
    assert!(log.contains("--mask mlib/**/*.md"));
    assert!(log.contains("update"));
}

#[test]
#[cfg(not(windows))]
fn moon_index_recreates_collection_when_mask_mismatches() {
    let tmp = tempdir().expect("tempdir");
    let archives_dir = tmp.path().join("archives");
    fs::create_dir_all(&archives_dir).expect("mkdir archives");

    let fake_qmd = tmp.path().join("qmd");
    let log_path = tmp.path().join("qmd.log");
    let marker = tmp.path().join("first_add.marker");
    write_fake_qmd_add_conflict_then_recreate(&fake_qmd, &log_path, &marker);

    assert_cmd::cargo::cargo_bin_cmd!("MOON")
        .current_dir(tmp.path())
        .env("MOON_ARCHIVES_DIR", &archives_dir)
        .env("QMD_BIN", &fake_qmd)
        .arg("moon-index")
        .arg("--name")
        .arg("history")
        .assert()
        .success();

    let log = fs::read_to_string(&log_path).expect("read log");
    assert!(log.contains("collection add"));
    assert!(log.contains("collection list"));
    assert!(log.contains("collection remove history"));
    assert!(log.contains("--mask mlib/**/*.md"));
    assert!(!log.contains("update"));
}
