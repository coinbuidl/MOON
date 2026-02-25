use predicates::str::contains;
use std::fs;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use tempfile::tempdir;

#[test]
#[cfg(not(windows))]
fn moon_stop_terminates_watcher_daemon_from_lock_pid() {
    let tmp = tempdir().expect("tempdir");
    let logs_dir = tmp.path().join("moon").join("logs");
    fs::create_dir_all(&logs_dir).expect("mkdir logs");
    let lock_path = logs_dir.join("moon-watch.daemon.lock");

    // Keep the process command line containing `moon-watch --daemon` so
    // moon-stop can verify it is terminating the expected daemon.
    let mut child = Command::new("sh")
        .arg("-c")
        .arg("while :; do sleep 1; done")
        .arg("moon-watch")
        .arg("--daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn fake daemon");

    fs::write(&lock_path, format!("{}\n", child.id())).expect("write lock pid");

    assert_cmd::cargo::cargo_bin_cmd!("moon")
        .current_dir(tmp.path())
        .env("MOON_LOGS_DIR", &logs_dir)
        .arg("moon-stop")
        .assert()
        .success()
        .stdout(contains("stopped moon watcher daemon pid="));

    let mut exited = false;
    for _ in 0..40 {
        if child.try_wait().expect("try_wait").is_some() {
            exited = true;
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    if !exited {
        let _ = child.kill();
    }
    assert!(exited, "fake daemon process did not stop");
    assert!(!lock_path.exists(), "daemon lock should be removed");
}

#[test]
fn moon_stop_is_idempotent_when_lock_is_missing() {
    let tmp = tempdir().expect("tempdir");
    let logs_dir = tmp.path().join("moon").join("logs");
    fs::create_dir_all(&logs_dir).expect("mkdir logs");

    assert_cmd::cargo::cargo_bin_cmd!("moon")
        .current_dir(tmp.path())
        .env("MOON_LOGS_DIR", &logs_dir)
        .arg("moon-stop")
        .assert()
        .success()
        .stdout(contains("already stopped"));
}
