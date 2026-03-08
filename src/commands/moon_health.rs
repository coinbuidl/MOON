use crate::commands::CommandReport;
use crate::moon::daemon_lock::{daemon_lock_path, read_daemon_lock_payload};
use crate::moon::paths::resolve_paths;
use crate::moon::state::{self, MoonState};
use crate::moon::util::now_epoch_secs;
use anyhow::Result;
use std::fs;
use std::io::Write;

const DEFAULT_MAX_CYCLE_AGE_SECS: u64 = 600;

#[derive(Debug, Clone, Copy, Default)]
struct HeartbeatStatus {
    age_secs: Option<u64>,
    max_age_secs: u64,
    is_fresh: bool,
}

fn max_cycle_age_secs() -> u64 {
    std::env::var("MOON_HEALTH_MAX_CYCLE_AGE_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_MAX_CYCLE_AGE_SECS)
}

fn check_state_file(
    paths: &crate::moon::paths::MoonPaths,
    report: &mut CommandReport,
) -> HeartbeatStatus {
    let mut heartbeat = HeartbeatStatus {
        max_age_secs: max_cycle_age_secs(),
        ..HeartbeatStatus::default()
    };
    let state_path = state::state_file_path(paths);
    report.detail(format!("state.file={}", state_path.display()));

    let state_exists = state_path.exists();

    if let Some(parent) = state_path.parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            report.issue(format!("state.dir=unwritable ({err})"));
            return heartbeat;
        }
        if state_exists {
            let writable = fs::OpenOptions::new()
                .append(true)
                .open(&state_path)
                .and_then(|mut f| f.write_all(b""));
            if let Err(err) = writable {
                report.issue(format!("state.file=unwritable ({err})"));
                return heartbeat;
            }
        } else {
            let probe = parent.join(".moon-health-write-probe");
            let writable = fs::write(&probe, b"probe").and_then(|_| fs::remove_file(&probe));
            if let Err(err) = writable {
                report.issue(format!("state.dir=unwritable ({err})"));
                return heartbeat;
            }
        }
    }
    report.detail("state.file=writable".to_string());

    if !state_exists {
        report.detail("state.file=not_found (will be created on first cycle)".to_string());
        return heartbeat;
    }

    let raw = match fs::read_to_string(&state_path) {
        Ok(raw) => raw,
        Err(err) => {
            report.issue(format!("state.file=unreadable ({err})"));
            return heartbeat;
        }
    };

    let parsed: MoonState = match serde_json::from_str(&raw) {
        Ok(state) => state,
        Err(err) => {
            report.issue(format!("state.file=corrupt ({err})"));
            return heartbeat;
        }
    };

    report.detail("state.file=parse_ok".to_string());
    if parsed.last_heartbeat_epoch_secs == 0 {
        report.issue("state.last_heartbeat=missing".to_string());
        return heartbeat;
    }

    let now = now_epoch_secs().unwrap_or(parsed.last_heartbeat_epoch_secs);
    let age = now.saturating_sub(parsed.last_heartbeat_epoch_secs);
    heartbeat.age_secs = Some(age);
    report.detail(format!("state.last_heartbeat_age_secs={age}"));

    if age > heartbeat.max_age_secs {
        report.issue(format!(
            "state.last_heartbeat=stale age_secs={age} max_allowed_secs={}",
            heartbeat.max_age_secs
        ));
    } else {
        heartbeat.is_fresh = true;
        report.detail(format!(
            "state.last_heartbeat=fresh max_allowed_secs={}",
            heartbeat.max_age_secs
        ));
    }

    heartbeat
}

pub fn run() -> Result<CommandReport> {
    let mut report = CommandReport::new("health");
    let paths = resolve_paths()?;

    report.detail(format!("moon_home={}", paths.moon_home.display()));

    // Check paths
    for (name, path) in [
        ("archives_dir", &paths.archives_dir),
        ("logs_dir", &paths.logs_dir),
    ] {
        if path.exists() {
            report.detail(format!("path.{name}=ok"));
        } else {
            report.issue(format!("path.{name}=missing ({})", path.display()));
        }
    }

    let heartbeat = check_state_file(&paths, &mut report);

    // Check daemon lock
    let lock_path = daemon_lock_path(&paths);
    if lock_path.exists() {
        match read_daemon_lock_payload(&paths) {
            Ok(Some(payload)) => {
                report.detail("daemon.lock=found".to_string());
                report.detail(format!("daemon.pid={}", payload.pid));
                if payload.started_at_epoch_secs > 0 {
                    report.detail(format!(
                        "daemon.started_at_epoch_secs={}",
                        payload.started_at_epoch_secs
                    ));
                }
                if !payload.moon_home.trim().is_empty() {
                    report.detail(format!("daemon.moon_home={}", payload.moon_home.trim()));
                }

                if crate::moon::util::pid_alive(payload.pid) {
                    report.detail("daemon.process=alive".to_string());
                } else {
                    report.issue("daemon.process=dead (stale lock)".to_string());
                }

                if !payload.build_uuid.trim().is_empty() {
                    let current_uuid = env!("BUILD_UUID");
                    if payload.build_uuid == current_uuid {
                        report.detail("daemon.build_match=ok".to_string());
                    } else {
                        report.issue(format!(
                            "daemon.build_mismatch=found (lock={} current={})",
                            payload.build_uuid, current_uuid
                        ));
                    }
                } else {
                    report.issue("daemon.build_uuid=missing".to_string());
                }
            }
            Ok(None) => {
                report.issue("daemon.lock=corrupt (empty payload)".to_string());
            }
            Err(err) => {
                report.issue(format!("daemon.lock=corrupt ({err})"));
            }
        }
    } else if heartbeat.is_fresh {
        let age_secs = heartbeat.age_secs.unwrap_or(0);
        report.detail(format!(
            "daemon.lock=not_found (recent heartbeat age_secs={age_secs} max_allowed_secs={}; daemon may still be running without a linked lockfile)",
            heartbeat.max_age_secs
        ));
    } else {
        report.detail("daemon.lock=not_found (daemon likely not running)".to_string());
    }

    Ok(report)
}
