use crate::moon::config::load_config;
use crate::moon::daemon_lock::{DaemonLockPayload, daemon_lock_path};
use crate::moon::distill::{
    DistillInput, DistillOutput, WisdomDistillInput, run_distillation, run_wisdom_distillation,
};
use crate::moon::paths::{MoonPaths, resolve_paths};
use crate::moon::state::{load, save, state_file_path};
use anyhow::{Context, Result};
use chrono::{LocalResult, TimeZone, Timelike};
use chrono_tz::Tz;
use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, UNIX_EPOCH};

const BUILD_UUID: &str = env!("BUILD_UUID");

#[derive(Debug, Clone, Copy, Default)]
pub struct WatchRunOptions {
    pub force_distill_now: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone)]
pub struct WatchCycleOutcome {
    pub state_file: String,
    pub heartbeat_epoch_secs: u64,
    pub poll_interval_secs: u64,
    pub distill_max_per_cycle: u64,
    pub pending_mds_docs: usize,
    pub distill_runs: usize,
    pub syns_due: bool,
    pub distill: Option<DistillOutput>,
    pub syns_result: Option<String>,
}

#[derive(Debug, Clone)]
struct MdsDoc {
    path: PathBuf,
    mtime_epoch_secs: u64,
}

fn now_epoch_secs() -> Result<u64> {
    if let Ok(raw) = std::env::var("MOON_WATCH_FAKE_NOW_EPOCH_SECS") {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return trimmed
                .parse::<u64>()
                .context("invalid MOON_WATCH_FAKE_NOW_EPOCH_SECS");
        }
    }
    crate::moon::util::now_epoch_secs()
}

fn mds_dir(paths: &MoonPaths) -> PathBuf {
    paths.moon_home.join("mds")
}

fn path_epoch_secs(path: &Path) -> u64 {
    let Ok(metadata) = fs::metadata(path) else {
        return 0;
    };
    let Ok(modified) = metadata.modified() else {
        return 0;
    };
    let Ok(duration) = modified.duration_since(UNIX_EPOCH) else {
        return 0;
    };
    duration.as_secs()
}

fn gather_mds_docs(root: &Path, out: &mut Vec<MdsDoc>) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }

    for entry in
        fs::read_dir(root).with_context(|| format!("failed to read mds dir {}", root.display()))?
    {
        let entry = entry.with_context(|| format!("failed to read entry in {}", root.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .with_context(|| format!("failed to stat {}", path.display()))?;
        if file_type.is_dir() {
            gather_mds_docs(&path, out)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }
        if path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_none_or(|ext| !ext.eq_ignore_ascii_case("md"))
        {
            continue;
        }
        out.push(MdsDoc {
            mtime_epoch_secs: path_epoch_secs(&path),
            path,
        });
    }

    Ok(())
}

fn list_mds_docs(paths: &MoonPaths) -> Result<Vec<MdsDoc>> {
    let mut docs = Vec::new();
    gather_mds_docs(&mds_dir(paths), &mut docs)?;
    docs.sort_by(|a, b| {
        a.mtime_epoch_secs
            .cmp(&b.mtime_epoch_secs)
            .then_with(|| a.path.cmp(&b.path))
    });
    Ok(docs)
}

fn pending_mds_docs(
    paths: &MoonPaths,
    state: &crate::moon::state::MoonState,
) -> Result<Vec<MdsDoc>> {
    Ok(list_mds_docs(paths)?
        .into_iter()
        .filter(|doc| {
            let key = doc.path.display().to_string();
            match state.distilled_archives.get(&key) {
                None => true,
                Some(last_distill) => doc.mtime_epoch_secs > *last_distill,
            }
        })
        .collect())
}

fn residential_tz_name(cfg: &crate::moon::config::MoonConfig) -> String {
    let name = cfg.distill.residential_timezone.trim();
    if name.is_empty() {
        "UTC".to_string()
    } else {
        name.to_string()
    }
}

fn parse_residential_tz(cfg: &crate::moon::config::MoonConfig) -> Tz {
    residential_tz_name(cfg)
        .parse::<Tz>()
        .unwrap_or(chrono_tz::UTC)
}

fn day_key_for_epoch_in_timezone(epoch_secs: u64, tz: Tz) -> String {
    let dt = match tz.timestamp_opt(epoch_secs as i64, 0) {
        LocalResult::Single(v) => v,
        _ => tz.from_utc_datetime(&chrono::Utc::now().naive_utc()),
    };
    dt.format("%Y-%m-%d").to_string()
}

fn previous_day_key_for_epoch_in_timezone(epoch_secs: u64, tz: Tz) -> String {
    let dt = match tz.timestamp_opt(epoch_secs as i64, 0) {
        LocalResult::Single(v) => v,
        _ => tz.from_utc_datetime(&chrono::Utc::now().naive_utc()),
    };
    let previous_day = dt.date_naive() - chrono::Duration::days(1);
    previous_day.format("%Y-%m-%d").to_string()
}

fn syns_due_now(state: &crate::moon::state::MoonState, now_epoch_secs: u64, tz: Tz) -> bool {
    let now_local = match tz.timestamp_opt(now_epoch_secs as i64, 0) {
        LocalResult::Single(v) => v,
        _ => return false,
    };
    if now_local.hour() != 0 {
        return false;
    }
    let today_key = now_local.format("%Y-%m-%d").to_string();
    let last_key = state
        .last_syns_trigger_epoch_secs
        .map(|epoch| day_key_for_epoch_in_timezone(epoch, tz));
    last_key.as_deref() != Some(today_key.as_str())
}

fn lock_payload(paths: &MoonPaths, now_epoch_secs: u64) -> DaemonLockPayload {
    DaemonLockPayload {
        pid: std::process::id(),
        started_at_epoch_secs: now_epoch_secs,
        build_uuid: BUILD_UUID.to_string(),
        moon_home: paths.moon_home.display().to_string(),
    }
}

fn write_daemon_lock(paths: &MoonPaths, now_epoch_secs: u64) -> Result<PathBuf> {
    fs::create_dir_all(&paths.logs_dir)
        .with_context(|| format!("failed to create {}", paths.logs_dir.display()))?;
    let lock_path = daemon_lock_path(paths);
    let payload = lock_payload(paths, now_epoch_secs);
    fs::write(
        &lock_path,
        format!("{}\n", serde_json::to_string(&payload)?),
    )
    .with_context(|| format!("failed to write {}", lock_path.display()))?;
    Ok(lock_path)
}

fn remove_daemon_lock(paths: &MoonPaths) {
    let lock_path = daemon_lock_path(paths);
    match fs::remove_file(&lock_path) {
        Ok(_) => {}
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(_) => {}
    }
}

pub fn run_once() -> Result<WatchCycleOutcome> {
    run_once_with_options(WatchRunOptions::default())
}

pub fn run_once_with_options(run_opts: WatchRunOptions) -> Result<WatchCycleOutcome> {
    let paths = resolve_paths()?;
    let cfg = load_config()?;
    let mut state = load(&paths)?;
    let now_epoch = now_epoch_secs()?;
    let tz = parse_residential_tz(&cfg);

    let pending_docs = pending_mds_docs(&paths, &state)?;
    let pending_count = pending_docs.len();
    let mut last_distill = None;
    let mut distill_runs = 0usize;

    if !run_opts.dry_run {
        for doc in pending_docs
            .into_iter()
            .take(cfg.distill.max_per_cycle as usize)
        {
            let session_id = doc
                .path
                .file_stem()
                .and_then(|v| v.to_str())
                .unwrap_or("session")
                .to_string();
            let out = run_distillation(
                &paths,
                &DistillInput {
                    session_id,
                    archive_path: doc.path.display().to_string(),
                    archive_text: String::new(),
                    archive_epoch_secs: Some(doc.mtime_epoch_secs),
                },
            )?;
            state
                .distilled_archives
                .insert(doc.path.display().to_string(), now_epoch);
            state.last_distill_trigger_epoch_secs = Some(now_epoch);
            distill_runs += 1;
            last_distill = Some(out);
            if !run_opts.force_distill_now && distill_runs >= cfg.distill.max_per_cycle as usize {
                break;
            }
        }
    }

    let syns_due = syns_due_now(&state, now_epoch, tz);
    let mut syns_result = None;
    if syns_due {
        let yesterday_key = previous_day_key_for_epoch_in_timezone(now_epoch, tz);
        let yesterday_source = paths.memory_dir.join(format!("{yesterday_key}.md"));
        let sources = if yesterday_source.exists() {
            vec![
                yesterday_source.display().to_string(),
                paths.memory_file.display().to_string(),
            ]
        } else {
            Vec::new()
        };

        if run_opts.dry_run {
            syns_result = Some(format!(
                "dry-run trigger=watch-midnight sources={}",
                sources.join(",")
            ));
        } else {
            let out = run_wisdom_distillation(
                &paths,
                &WisdomDistillInput {
                    trigger: "watch-midnight".to_string(),
                    day_epoch_secs: Some(now_epoch),
                    source_paths: sources,
                    dry_run: false,
                },
            )?;
            state.last_syns_trigger_epoch_secs = Some(now_epoch);
            syns_result = Some(format!(
                "provider={} summary_path={}",
                out.provider, out.summary_path
            ));
        }
    }

    state.last_heartbeat_epoch_secs = now_epoch;
    let state_file = if run_opts.dry_run {
        state_file_path(&paths)
    } else {
        save(&paths, &state)?
    };

    Ok(WatchCycleOutcome {
        state_file: state_file.display().to_string(),
        heartbeat_epoch_secs: now_epoch,
        poll_interval_secs: cfg.watcher.poll_interval_secs,
        distill_max_per_cycle: cfg.distill.max_per_cycle,
        pending_mds_docs: pending_count,
        distill_runs,
        syns_due,
        distill: last_distill,
        syns_result,
    })
}

pub fn run_daemon() -> Result<()> {
    let paths = resolve_paths()?;
    let cfg = load_config()?;
    let now_epoch = now_epoch_secs()?;
    let _lock_path = write_daemon_lock(&paths, now_epoch)?;

    let keep_running = Arc::new(AtomicBool::new(true));
    {
        let keep_running = Arc::clone(&keep_running);
        ctrlc::set_handler(move || {
            keep_running.store(false, Ordering::SeqCst);
        })
        .context("failed to install ctrl-c handler")?;
    }

    while keep_running.load(Ordering::SeqCst) {
        let _ = run_once();
        for _ in 0..cfg.watcher.poll_interval_secs {
            if !keep_running.load(Ordering::SeqCst) {
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }
    }

    remove_daemon_lock(&paths);
    Ok(())
}
