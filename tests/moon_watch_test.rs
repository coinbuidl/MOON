use predicates::str::contains;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::tempdir;

fn write_fake_qmd(bin_path: &Path) {
    let script = r#"#!/usr/bin/env bash
set -euo pipefail

if [[ -n "${MOON_TEST_QMD_LOG:-}" ]]; then
  printf "%s\n" "$*" >> "${MOON_TEST_QMD_LOG}"
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

fn write_fake_openclaw(bin_path: &Path) {
    let script = r#"#!/usr/bin/env bash
set -euo pipefail

if [[ "${1:-}" == "sessions" && "${2:-}" == "--json" ]]; then
  if [[ -n "${MOON_TEST_SESSIONS_JSON:-}" ]]; then
    echo "${MOON_TEST_SESSIONS_JSON}"
  else
    echo '{"path":"x","count":1,"sessions":[{"key":"agent:main:discord:channel:default","totalTokens":100,"contextTokens":10000}]}'
  fi
  exit 0
fi

if [[ "${1:-}" == "sessions" && "${2:-}" == "current" && "${3:-}" == "--json" ]]; then
  if [[ -n "${MOON_TEST_CURRENT_JSON:-}" ]]; then
    echo "${MOON_TEST_CURRENT_JSON}"
  else
    echo '{"sessionId":"current","usage":{"totalTokens":120},"limits":{"maxTokens":10000}}'
  fi
  exit 0
fi

if [[ "${1:-}" == "gateway" && "${2:-}" == "call" && "${3:-}" == "chat.send" ]]; then
  if [[ -n "${MOON_TEST_COMPACT_LOG:-}" ]]; then
    printf "%s\n" "$*" >> "${MOON_TEST_COMPACT_LOG}"
  fi
  echo '{"status":"started","runId":"test-run"}'
  exit 0
fi

if [[ "${1:-}" == "system" && "${2:-}" == "event" ]]; then
  if [[ -n "${MOON_TEST_EVENT_LOG:-}" ]]; then
    printf "%s\n" "$*" >> "${MOON_TEST_EVENT_LOG}"
  fi
  exit 0
fi

exit 0
"#;
    fs::write(bin_path, script).expect("write fake openclaw");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(bin_path).expect("metadata").permissions();
        perms.set_mode(0o755);
        fs::set_permissions(bin_path, perms).expect("chmod");
    }
}

fn read_distilled_archive_paths(state_file: &Path) -> Vec<String> {
    let raw = fs::read_to_string(state_file).expect("read state");
    let parsed: Value = serde_json::from_str(&raw).expect("parse state");
    let map = parsed
        .get("distilled_archives")
        .and_then(Value::as_object)
        .expect("distilled_archives map");
    map.keys().cloned().collect()
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
    let openclaw = tmp.path().join("openclaw");
    write_fake_openclaw(&openclaw);

    assert_cmd::cargo::cargo_bin_cmd!("oc-token-optim")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("OPENCLAW_SESSIONS_DIR", &sessions_dir)
        .env("QMD_BIN", &qmd)
        .env("OPENCLAW_BIN", &openclaw)
        .env("MOON_THRESHOLD_ARCHIVE_RATIO", "0.00001")
        .env("MOON_THRESHOLD_COMPACTION_RATIO", "0.00002")
        .arg("moon-watch")
        .arg("--once")
        .assert()
        .success();

    let state_file = moon_home.join("state/moon_state.json");
    assert!(state_file.exists());
    let ledger = moon_home.join("archives/ledger.jsonl");
    assert!(ledger.exists());
}

#[test]
fn moon_watch_once_triggers_inbound_system_event_for_new_file() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let sessions_dir = tmp.path().join("sessions");
    let inbound_dir = tmp.path().join("inbound");
    let event_log = tmp.path().join("events.log");
    fs::create_dir_all(moon_home.join("archives")).expect("mkdir archives");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("skills/moon-system/logs")).expect("mkdir logs");
    fs::create_dir_all(&sessions_dir).expect("mkdir sessions");
    fs::create_dir_all(&inbound_dir).expect("mkdir inbound");
    fs::write(
        sessions_dir.join("s1.json"),
        "{\"decision\":\"watch inbound\"}\n",
    )
    .expect("write session");
    fs::write(inbound_dir.join("task.md"), "run this file\n").expect("write inbound file");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(&qmd);
    let openclaw = tmp.path().join("openclaw");
    write_fake_openclaw(&openclaw);

    assert_cmd::cargo::cargo_bin_cmd!("oc-token-optim")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("OPENCLAW_SESSIONS_DIR", &sessions_dir)
        .env("QMD_BIN", &qmd)
        .env("OPENCLAW_BIN", &openclaw)
        .env("MOON_TEST_EVENT_LOG", &event_log)
        .env("MOON_THRESHOLD_ARCHIVE_RATIO", "0.00001")
        .env("MOON_THRESHOLD_COMPACTION_RATIO", "0.00002")
        .env("MOON_INBOUND_WATCH_ENABLED", "true")
        .env(
            "MOON_INBOUND_WATCH_PATHS",
            inbound_dir.to_string_lossy().to_string(),
        )
        .arg("moon-watch")
        .arg("--once")
        .assert()
        .success();

    let events = fs::read_to_string(&event_log).expect("read event log");
    assert!(events.contains("system event --text"));
    assert!(events.contains("Moon System inbound file detected"));
    assert!(events.contains("task.md"));

    let state_file = moon_home.join("state/moon_state.json");
    let state_raw = fs::read_to_string(state_file).expect("read state");
    assert!(state_raw.contains("inbound_seen_files"));
}

#[test]
fn moon_watch_once_compacts_all_oversized_discord_and_whatsapp_sessions() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let sessions_dir = tmp.path().join("sessions");
    let compact_log = tmp.path().join("compact.log");
    fs::create_dir_all(moon_home.join("archives")).expect("mkdir archives");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("skills/moon-system/logs")).expect("mkdir logs");
    fs::create_dir_all(&sessions_dir).expect("mkdir sessions");
    fs::write(
        sessions_dir.join("s1.json"),
        "{\"decision\":\"compact channels\"}\n",
    )
    .expect("write session");
    fs::write(
        sessions_dir.join("sess-over.jsonl"),
        "{\"messages\":[\"discord oversized\"]}\n",
    )
    .expect("write over session");
    fs::write(
        sessions_dir.join("sess-wa.jsonl"),
        "{\"messages\":[\"whatsapp oversized\"]}\n",
    )
    .expect("write wa session");
    fs::write(
        sessions_dir.join("sessions.json"),
        r#"{
            "agent:main:discord:channel:over": {"sessionId":"sess-over"},
            "agent:main:whatsapp:+61400000000": {"sessionId":"sess-wa"},
            "agent:main:discord:channel:small": {"sessionId":"sess-small"},
            "agent:main:main": {"sessionId":"sess-main"}
        }"#,
    )
    .expect("write sessions map");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(&qmd);
    let openclaw = tmp.path().join("openclaw");
    write_fake_openclaw(&openclaw);

    let sessions_json = r#"{"path":"x","count":4,"sessions":[
        {"key":"agent:main:discord:channel:over","totalTokens":29000,"contextTokens":32000},
        {"key":"agent:main:whatsapp:+61400000000","totalTokens":70000,"contextTokens":80000},
        {"key":"agent:main:discord:channel:small","totalTokens":1000,"contextTokens":32000},
        {"key":"agent:main:main","totalTokens":90000,"contextTokens":100000}
    ]}"#;

    assert_cmd::cargo::cargo_bin_cmd!("oc-token-optim")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("OPENCLAW_SESSIONS_DIR", &sessions_dir)
        .env("QMD_BIN", &qmd)
        .env("OPENCLAW_BIN", &openclaw)
        .env("MOON_TEST_SESSIONS_JSON", sessions_json)
        .env(
            "MOON_TEST_CURRENT_JSON",
            r#"{"sessionId":"agent:main:main","usage":{"totalTokens":120},"limits":{"maxTokens":10000}}"#,
        )
        .env("MOON_TEST_COMPACT_LOG", &compact_log)
        .env("MOON_THRESHOLD_ARCHIVE_RATIO", "0.80")
        .env("MOON_THRESHOLD_COMPACTION_RATIO", "0.85")
        .env("MOON_COOLDOWN_SECS", "0")
        .arg("moon-watch")
        .arg("--once")
        .assert()
        .success();

    let compact_calls = fs::read_to_string(&compact_log).expect("read compact log");
    assert!(compact_calls.contains("agent:main:discord:channel:over"));
    assert!(compact_calls.contains("agent:main:whatsapp:+61400000000"));
    assert!(compact_calls.contains("MOON_ARCHIVE_INDEX"));
    assert!(compact_calls.contains("moon-system-index-note"));
    assert!(!compact_calls.contains("agent:main:discord:channel:small"));
    assert!(!compact_calls.contains("agent:main:main"));

    let ledger = fs::read_to_string(moon_home.join("archives/ledger.jsonl")).expect("read ledger");
    assert!(ledger.contains("sess-over.jsonl"));
    assert!(ledger.contains("sess-wa.jsonl"));
    assert!(ledger.contains("\"projection_path\":"));
    assert!(ledger.contains(".md"));

    let raw_archives_dir = moon_home.join("archives/raw");
    let projection_count = fs::read_dir(&raw_archives_dir)
        .expect("read raw archives dir")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("md"))
        .count();
    assert!(projection_count >= 2);

    let channel_map = fs::read_to_string(moon_home.join("continuity/channel_archive_map.json"))
        .expect("read channel archive map");
    assert!(channel_map.contains("agent:main:discord:channel:over"));
    assert!(channel_map.contains("agent:main:whatsapp:+61400000000"));
}

#[test]
fn moon_watch_once_distills_oldest_pending_archive_day_first() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let sessions_dir = tmp.path().join("sessions");
    fs::create_dir_all(moon_home.join("archives")).expect("mkdir archives");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("skills/moon-system/logs")).expect("mkdir logs");
    fs::create_dir_all(&sessions_dir).expect("mkdir sessions");
    fs::write(
        sessions_dir.join("s1.json"),
        "{\"decision\":\"distill ordering\"}\n",
    )
    .expect("write session");

    let old_archive = moon_home.join("archives/old.jsonl");
    let new_archive = moon_home.join("archives/new.jsonl");
    fs::write(&old_archive, "{\"session\":\"old\"}\n").expect("write old archive");
    fs::write(&new_archive, "{\"session\":\"new\"}\n").expect("write new archive");
    fs::write(
        old_archive.with_extension("md"),
        "- [user] old projection\n",
    )
    .expect("write old projection");
    fs::write(
        new_archive.with_extension("md"),
        "- [user] new projection\n",
    )
    .expect("write new projection");

    let ledger = format!(
        concat!(
            "{{\"session_id\":\"old\",\"source_path\":\"/tmp/old.jsonl\",\"archive_path\":\"{}\",\"content_hash\":\"aaa\",\"created_at_epoch_secs\":86400,\"indexed_collection\":\"history\",\"indexed\":true}}\n",
            "{{\"session_id\":\"new\",\"source_path\":\"/tmp/new.jsonl\",\"archive_path\":\"{}\",\"content_hash\":\"bbb\",\"created_at_epoch_secs\":172800,\"indexed_collection\":\"history\",\"indexed\":true}}\n"
        ),
        old_archive.display(),
        new_archive.display()
    );
    fs::write(moon_home.join("archives/ledger.jsonl"), ledger).expect("write ledger");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(&qmd);
    let openclaw = tmp.path().join("openclaw");
    write_fake_openclaw(&openclaw);

    assert_cmd::cargo::cargo_bin_cmd!("oc-token-optim")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("OPENCLAW_SESSIONS_DIR", &sessions_dir)
        .env("QMD_BIN", &qmd)
        .env("OPENCLAW_BIN", &openclaw)
        .env("MOON_DISTILL_MODE", "idle")
        .env("MOON_DISTILL_IDLE_SECS", "1")
        .env("MOON_DISTILL_PROVIDER", "local")
        .env("MOON_DISTILL_MAX_PER_CYCLE", "1")
        .env("MOON_COOLDOWN_SECS", "0")
        .env("MOON_RETENTION_COLD_DAYS", "99999")
        .arg("moon-watch")
        .arg("--once")
        .assert()
        .success();

    let distilled = read_distilled_archive_paths(&moon_home.join("state/moon_state.json"));
    assert_eq!(distilled.len(), 1);
    assert!(distilled.contains(&old_archive.to_string_lossy().to_string()));
    assert!(!distilled.contains(&new_archive.to_string_lossy().to_string()));
}

#[test]
fn moon_watch_once_distill_selection_skips_unindexed_missing_and_already_distilled() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let sessions_dir = tmp.path().join("sessions");
    fs::create_dir_all(moon_home.join("archives")).expect("mkdir archives");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("skills/moon-system/logs")).expect("mkdir logs");
    fs::create_dir_all(moon_home.join("state")).expect("mkdir state");
    fs::create_dir_all(&sessions_dir).expect("mkdir sessions");
    fs::write(
        sessions_dir.join("s1.json"),
        "{\"decision\":\"distill filtering\"}\n",
    )
    .expect("write session");

    let eligible = moon_home.join("archives/eligible.jsonl");
    let unindexed = moon_home.join("archives/unindexed.jsonl");
    let already = moon_home.join("archives/already.jsonl");
    let missing = moon_home.join("archives/missing.jsonl");
    fs::write(&eligible, "{\"session\":\"eligible\"}\n").expect("write eligible");
    fs::write(&unindexed, "{\"session\":\"unindexed\"}\n").expect("write unindexed");
    fs::write(&already, "{\"session\":\"already\"}\n").expect("write already");
    fs::write(
        eligible.with_extension("md"),
        "- [user] eligible projection\n",
    )
    .expect("write eligible projection");
    fs::write(
        unindexed.with_extension("md"),
        "- [user] unindexed projection\n",
    )
    .expect("write unindexed projection");
    fs::write(
        already.with_extension("md"),
        "- [user] already projection\n",
    )
    .expect("write already projection");

    let ledger = format!(
        concat!(
            "{{\"session_id\":\"eligible\",\"source_path\":\"/tmp/e.jsonl\",\"archive_path\":\"{}\",\"content_hash\":\"a\",\"created_at_epoch_secs\":86400,\"indexed_collection\":\"history\",\"indexed\":true}}\n",
            "{{\"session_id\":\"unindexed\",\"source_path\":\"/tmp/u.jsonl\",\"archive_path\":\"{}\",\"content_hash\":\"b\",\"created_at_epoch_secs\":86401,\"indexed_collection\":\"history\",\"indexed\":false}}\n",
            "{{\"session_id\":\"already\",\"source_path\":\"/tmp/a.jsonl\",\"archive_path\":\"{}\",\"content_hash\":\"c\",\"created_at_epoch_secs\":86402,\"indexed_collection\":\"history\",\"indexed\":true}}\n",
            "{{\"session_id\":\"missing\",\"source_path\":\"/tmp/m.jsonl\",\"archive_path\":\"{}\",\"content_hash\":\"d\",\"created_at_epoch_secs\":86403,\"indexed_collection\":\"history\",\"indexed\":true}}\n"
        ),
        eligible.display(),
        unindexed.display(),
        already.display(),
        missing.display()
    );
    fs::write(moon_home.join("archives/ledger.jsonl"), ledger).expect("write ledger");

    let state = format!(
        "{{\n  \"schema_version\": 1,\n  \"last_heartbeat_epoch_secs\": 0,\n  \"last_archive_trigger_epoch_secs\": null,\n  \"last_compaction_trigger_epoch_secs\": null,\n  \"last_distill_trigger_epoch_secs\": null,\n  \"last_session_id\": null,\n  \"last_usage_ratio\": null,\n  \"last_provider\": null,\n  \"distilled_archives\": {{\n    \"{}\": 1\n  }},\n  \"inbound_seen_files\": {{}}\n}}\n",
        already.display()
    );
    fs::write(moon_home.join("state/moon_state.json"), state).expect("write state");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(&qmd);
    let openclaw = tmp.path().join("openclaw");
    write_fake_openclaw(&openclaw);

    assert_cmd::cargo::cargo_bin_cmd!("oc-token-optim")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("OPENCLAW_SESSIONS_DIR", &sessions_dir)
        .env("QMD_BIN", &qmd)
        .env("OPENCLAW_BIN", &openclaw)
        .env("MOON_DISTILL_MODE", "idle")
        .env("MOON_DISTILL_IDLE_SECS", "1")
        .env("MOON_DISTILL_PROVIDER", "local")
        .env("MOON_DISTILL_MAX_PER_CYCLE", "5")
        .env("MOON_COOLDOWN_SECS", "0")
        .env("MOON_RETENTION_COLD_DAYS", "99999")
        .arg("moon-watch")
        .arg("--once")
        .assert()
        .success();

    let distilled = read_distilled_archive_paths(&moon_home.join("state/moon_state.json"));
    assert_eq!(distilled.len(), 2);
    assert!(distilled.contains(&eligible.to_string_lossy().to_string()));
    assert!(distilled.contains(&already.to_string_lossy().to_string()));
    assert!(!distilled.contains(&unindexed.to_string_lossy().to_string()));
    assert!(!distilled.contains(&missing.to_string_lossy().to_string()));
}

#[test]
fn moon_watch_once_emits_ai_warning_when_ledger_is_invalid() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let sessions_dir = tmp.path().join("sessions");
    fs::create_dir_all(moon_home.join("archives")).expect("mkdir archives");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("skills/moon-system/logs")).expect("mkdir logs");
    fs::create_dir_all(&sessions_dir).expect("mkdir sessions");
    fs::write(
        sessions_dir.join("s1.json"),
        "{\"decision\":\"bad ledger\"}\n",
    )
    .expect("session");
    fs::write(moon_home.join("archives/ledger.jsonl"), "not-jsonl\n").expect("ledger");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(&qmd);
    let openclaw = tmp.path().join("openclaw");
    write_fake_openclaw(&openclaw);

    assert_cmd::cargo::cargo_bin_cmd!("oc-token-optim")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("OPENCLAW_SESSIONS_DIR", &sessions_dir)
        .env("QMD_BIN", &qmd)
        .env("OPENCLAW_BIN", &openclaw)
        .env("MOON_DISTILL_MODE", "idle")
        .env("MOON_DISTILL_IDLE_SECS", "1")
        .env("MOON_COOLDOWN_SECS", "0")
        .arg("moon-watch")
        .arg("--once")
        .assert()
        .success()
        .stderr(contains("MOON_WARN code=LEDGER_READ_FAILED"))
        .stderr(contains("stage=distill-selection"))
        .stderr(contains("action=read-ledger"));
}

#[test]
fn moon_watch_once_cleans_up_expired_distilled_archives_after_grace_period() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let sessions_dir = tmp.path().join("sessions");
    let qmd_log = tmp.path().join("qmd.log");
    fs::create_dir_all(moon_home.join("archives")).expect("mkdir archives");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("skills/moon-system/logs")).expect("mkdir logs");
    fs::create_dir_all(&sessions_dir).expect("mkdir sessions");
    fs::create_dir_all(moon_home.join("continuity")).expect("mkdir continuity");
    fs::create_dir_all(moon_home.join("state")).expect("mkdir state");
    fs::write(
        sessions_dir.join("s1.json"),
        "{\"decision\":\"cleanup retention\"}\n",
    )
    .expect("write session");

    let archive_path = moon_home.join("archives/expired.json");
    fs::write(&archive_path, "{\"session\":\"old\"}\n").expect("write archive");
    let archive_path_str = archive_path.to_string_lossy().to_string();

    let ledger_record = format!(
        "{{\"session_id\":\"agent:main:discord:channel:retained\",\"source_path\":\"/tmp/source.jsonl\",\"archive_path\":\"{}\",\"content_hash\":\"deadbeef\",\"created_at_epoch_secs\":1,\"indexed_collection\":\"history\",\"indexed\":true}}\n",
        archive_path_str
    );
    fs::write(moon_home.join("archives/ledger.jsonl"), ledger_record).expect("write ledger");

    let channel_map = format!(
        "{{\n  \"agent:main:discord:channel:retained\": {{\n    \"channel_key\": \"agent:main:discord:channel:retained\",\n    \"source_path\": \"/tmp/source.jsonl\",\n    \"archive_path\": \"{}\",\n    \"updated_at_epoch_secs\": 1\n  }}\n}}\n",
        archive_path_str
    );
    fs::write(
        moon_home.join("continuity/channel_archive_map.json"),
        channel_map,
    )
    .expect("write channel map");

    let state = format!(
        "{{\n  \"schema_version\": 1,\n  \"last_heartbeat_epoch_secs\": 0,\n  \"last_archive_trigger_epoch_secs\": null,\n  \"last_compaction_trigger_epoch_secs\": null,\n  \"last_distill_trigger_epoch_secs\": null,\n  \"last_session_id\": null,\n  \"last_usage_ratio\": null,\n  \"last_provider\": null,\n  \"distilled_archives\": {{\n    \"{}\": 1\n  }},\n  \"inbound_seen_files\": {{}}\n}}\n",
        archive_path_str
    );
    fs::write(moon_home.join("state/moon_state.json"), state).expect("write state");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(&qmd);
    let openclaw = tmp.path().join("openclaw");
    write_fake_openclaw(&openclaw);

    assert_cmd::cargo::cargo_bin_cmd!("oc-token-optim")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("OPENCLAW_SESSIONS_DIR", &sessions_dir)
        .env("QMD_BIN", &qmd)
        .env("OPENCLAW_BIN", &openclaw)
        .env("MOON_TEST_QMD_LOG", &qmd_log)
        .env(
            "MOON_TEST_CURRENT_JSON",
            r#"{"sessionId":"agent:main:main","usage":{"totalTokens":120},"limits":{"maxTokens":100000}}"#,
        )
        .env("MOON_DISTILL_MODE", "manual")
        .env("MOON_DISTILL_ARCHIVE_GRACE_HOURS", "60")
        .arg("moon-watch")
        .arg("--once")
        .assert()
        .success();

    assert!(!archive_path.exists());

    let ledger = fs::read_to_string(moon_home.join("archives/ledger.jsonl")).expect("read ledger");
    assert!(!ledger.contains(&archive_path_str));

    let map = fs::read_to_string(moon_home.join("continuity/channel_archive_map.json"))
        .expect("read map");
    assert!(!map.contains(&archive_path_str));

    let state_raw = fs::read_to_string(moon_home.join("state/moon_state.json")).expect("state");
    assert!(!state_raw.contains(&archive_path_str));

    let qmd_calls = fs::read_to_string(&qmd_log).expect("qmd calls");
    assert!(qmd_calls.lines().any(|line| line.trim() == "update"));
}

#[test]
fn moon_watch_once_retention_keeps_recent_cold_window_archives() {
    let tmp = tempdir().expect("tempdir");
    let moon_home = tmp.path().join("moon");
    let sessions_dir = tmp.path().join("sessions");
    fs::create_dir_all(moon_home.join("archives")).expect("mkdir archives");
    fs::create_dir_all(moon_home.join("memory")).expect("mkdir memory");
    fs::create_dir_all(moon_home.join("skills/moon-system/logs")).expect("mkdir logs");
    fs::create_dir_all(moon_home.join("state")).expect("mkdir state");
    fs::create_dir_all(&sessions_dir).expect("mkdir sessions");
    fs::write(
        sessions_dir.join("s1.json"),
        "{\"decision\":\"retention boundary\"}\n",
    )
    .expect("write session");

    let archive_path = moon_home.join("archives/recent.json");
    fs::write(&archive_path, "{\"session\":\"recent\"}\n").expect("write archive");
    let archive_path_str = archive_path.to_string_lossy().to_string();
    let now_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_secs();
    let created_at = now_epoch.saturating_sub(10 * 86_400);
    let ledger_record = format!(
        "{{\"session_id\":\"agent:main:discord:channel:recent\",\"source_path\":\"/tmp/source.jsonl\",\"archive_path\":\"{}\",\"content_hash\":\"deadbeef\",\"created_at_epoch_secs\":{},\"indexed_collection\":\"history\",\"indexed\":true}}\n",
        archive_path_str, created_at
    );
    fs::write(moon_home.join("archives/ledger.jsonl"), ledger_record).expect("write ledger");
    let state = format!(
        "{{\n  \"schema_version\": 1,\n  \"last_heartbeat_epoch_secs\": 0,\n  \"last_archive_trigger_epoch_secs\": null,\n  \"last_compaction_trigger_epoch_secs\": null,\n  \"last_distill_trigger_epoch_secs\": null,\n  \"last_session_id\": null,\n  \"last_usage_ratio\": null,\n  \"last_provider\": null,\n  \"distilled_archives\": {{\n    \"{}\": 1\n  }},\n  \"inbound_seen_files\": {{}}\n}}\n",
        archive_path_str
    );
    fs::write(moon_home.join("state/moon_state.json"), state).expect("write state");

    let qmd = tmp.path().join("qmd");
    write_fake_qmd(&qmd);
    let openclaw = tmp.path().join("openclaw");
    write_fake_openclaw(&openclaw);

    assert_cmd::cargo::cargo_bin_cmd!("oc-token-optim")
        .current_dir(tmp.path())
        .env("MOON_HOME", &moon_home)
        .env("OPENCLAW_SESSIONS_DIR", &sessions_dir)
        .env("QMD_BIN", &qmd)
        .env("OPENCLAW_BIN", &openclaw)
        .env("MOON_DISTILL_MODE", "manual")
        .env("MOON_RETENTION_ACTIVE_DAYS", "7")
        .env("MOON_RETENTION_WARM_DAYS", "30")
        .env("MOON_RETENTION_COLD_DAYS", "31")
        .arg("moon-watch")
        .arg("--once")
        .assert()
        .success();

    assert!(archive_path.exists());
    let state_raw = fs::read_to_string(moon_home.join("state/moon_state.json")).expect("state");
    assert!(state_raw.contains(&archive_path_str));
}
