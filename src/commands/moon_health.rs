use anyhow::Result;
use crate::commands::CommandReport;
use crate::moon::paths::resolve_paths;
use std::fs;

pub fn run() -> Result<CommandReport> {
    let mut report = CommandReport::new("moon-health");
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

    // Check daemon lock
    let lock_path = paths.logs_dir.join("moon.lock");
    if lock_path.exists() {
        match fs::read_to_string(&lock_path) {
            Ok(content) => {
                match serde_json::from_str::<serde_json::Value>(&content) {
                    Ok(payload) => {
                        let pid = payload.get("pid").and_then(|v| v.as_u64());
                        let build_uuid = payload.get("build_uuid").and_then(|v| v.as_str());
                        let start_time = payload.get("start_time").and_then(|v| v.as_str());
                        
                        report.detail(format!("daemon.lock=found"));
                        report.detail(format!("daemon.pid={:?}", pid));
                        report.detail(format!("daemon.build_uuid={:?}", build_uuid));
                        report.detail(format!("daemon.start_time={:?}", start_time));

                        if let Some(pid) = pid {
                            if crate::moon::util::pid_alive(pid as u32) {
                                report.detail("daemon.process=alive".to_string());
                            } else {
                                report.issue("daemon.process=dead (stale lock)".to_string());
                            }
                        }

                        if let Some(uuid) = build_uuid {
                            let current_uuid = env!("BUILD_UUID");
                            if uuid == current_uuid {
                                report.detail("daemon.build_match=ok".to_string());
                            } else {
                                report.issue(format!("daemon.build_mismatch=found (lock={} current={})", uuid, current_uuid));
                            }
                        }
                    }
                    Err(err) => {
                        report.issue(format!("daemon.lock=corrupt ({err})"));
                    }
                }
            }
            Err(err) => {
                report.issue(format!("daemon.lock=unreadable ({err})"));
            }
        }
    } else {
        report.detail("daemon.lock=not_found (daemon likely not running)".to_string());
    }

    Ok(report)
}
