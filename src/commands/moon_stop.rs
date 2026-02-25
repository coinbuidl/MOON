use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use crate::commands::CommandReport;
use crate::moon::paths::resolve_paths;

const DAEMON_LOCK_FILE: &str = "moon-watch.daemon.lock";
const STOP_TIMEOUT: Duration = Duration::from_secs(8);
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(100);

fn daemon_lock_path() -> Result<PathBuf> {
    let paths = resolve_paths()?;
    Ok(paths.logs_dir.join(DAEMON_LOCK_FILE))
}

fn read_lock_pid(path: &Path) -> Result<u32> {
    let raw =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let pid_str = raw
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .context("daemon lock file is empty")?;
    let pid = pid_str
        .parse::<u32>()
        .with_context(|| format!("invalid daemon pid in lock file: {pid_str}"))?;
    Ok(pid)
}

fn process_alive(pid: u32) -> Result<bool> {
    let status = Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .context("failed to probe process state with `kill -0`")?;
    if !status.success() {
        return Ok(false);
    }

    let ps_out = Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("stat=")
        .output()
        .context("failed to inspect process state with `ps`")?;

    if !ps_out.status.success() {
        return Ok(false);
    }

    let proc_state = String::from_utf8_lossy(&ps_out.stdout).trim().to_string();
    if proc_state.starts_with('Z') {
        return Ok(false);
    }

    Ok(true)
}

fn process_command_line(pid: u32) -> Result<String> {
    let output = Command::new("ps")
        .arg("-p")
        .arg(pid.to_string())
        .arg("-o")
        .arg("command=")
        .output()
        .context("failed to inspect process command line with `ps`")?;
    if !output.status.success() {
        return Ok(String::new());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn send_sigterm(pid: u32) -> Result<()> {
    let status = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .context("failed to send SIGTERM with `kill -TERM`")?;

    if status.success() {
        return Ok(());
    }

    if process_alive(pid)? {
        anyhow::bail!("`kill -TERM {pid}` failed and process is still alive");
    }

    Ok(())
}

fn cleanup_lock_file(lock_path: &Path, report: &mut CommandReport) {
    match fs::remove_file(lock_path) {
        Ok(()) => report.detail(format!("removed stale daemon lock {}", lock_path.display())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => report.detail(format!(
            "failed to remove daemon lock {}: {}",
            lock_path.display(),
            err
        )),
    }
}

pub fn run() -> Result<CommandReport> {
    let mut report = CommandReport::new("moon-stop");
    let lock_path = daemon_lock_path()?;
    report.detail(format!("daemon_lock={}", lock_path.display()));

    if !lock_path.exists() {
        report.detail("moon watcher daemon already stopped (lock file not found)".to_string());
        return Ok(report);
    }

    let pid = match read_lock_pid(&lock_path) {
        Ok(pid) => pid,
        Err(err) => {
            report.issue(format!(
                "failed to read daemon pid from lock {}: {err:#}",
                lock_path.display()
            ));
            return Ok(report);
        }
    };
    report.detail(format!("daemon_pid={pid}"));

    if !process_alive(pid)? {
        report.detail(format!("daemon pid {pid} is not running"));
        cleanup_lock_file(&lock_path, &mut report);
        return Ok(report);
    }

    let command_line = process_command_line(pid)?;
    if !command_line.contains("moon-watch --daemon") {
        report.issue(format!(
            "refusing to stop pid {pid}; command does not match moon watcher daemon: {}",
            if command_line.is_empty() {
                "<unknown>".to_string()
            } else {
                command_line
            }
        ));
        return Ok(report);
    }

    send_sigterm(pid)?;
    let deadline = Instant::now() + STOP_TIMEOUT;
    while Instant::now() < deadline {
        if !process_alive(pid)? {
            report.detail(format!("stopped moon watcher daemon pid={pid}"));
            cleanup_lock_file(&lock_path, &mut report);
            return Ok(report);
        }
        thread::sleep(STOP_POLL_INTERVAL);
    }

    report.issue(format!(
        "timed out waiting for daemon pid {pid} to stop after {}s",
        STOP_TIMEOUT.as_secs()
    ));
    Ok(report)
}
