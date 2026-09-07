use assert_cmd::Command;
use predicates::prelude::*;

fn moon() -> Command {
    let mut command = Command::new(assert_cmd::cargo::cargo_bin!("moon"));
    command
        .env_remove("MOON_DATABASE")
        .env_remove("MOON_HOME")
        .env_remove("MOON_EMBEDDING_DIMENSIONS");
    command
}

#[test]
fn init_only_writes_to_explicit_test_home() {
    let temp = tempfile::tempdir().expect("tempdir");
    let test_home = temp.path().join("moon");
    moon()
        .args([
            "--home",
            test_home.to_str().expect("utf8"),
            "--dimensions",
            "64",
            "--json",
            "init",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("moon.sqlite"));
    assert!(test_home.join("state/moon.sqlite").is_file());
}

#[test]
fn explicit_database_keeps_isolated_commands_out_of_ambient_storage() {
    let temp = tempfile::tempdir().expect("tempdir");
    let test_home = temp.path().join("moon");
    let database = test_home.join("state/moon.sqlite");
    let ambient_database = temp.path().join("must-not-be-created.sqlite");
    moon()
        .env("MOON_DATABASE", &ambient_database)
        .arg("--home")
        .arg(&test_home)
        .arg("--database")
        .arg(&database)
        .args(["--dimensions", "64", "--json", "init"])
        .assert()
        .success();
    assert!(database.is_file());
    assert!(!ambient_database.exists());
}

#[test]
fn commands_never_offer_cutover_or_delete_operations() {
    moon()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("import-legacy"))
        .stdout(predicate::str::contains("record"))
        .stdout(predicate::str::contains("distill"))
        .stdout(predicate::str::contains("context"))
        .stdout(predicate::str::contains("\n  auth").not())
        .stdout(predicate::str::contains("cutover").not())
        .stdout(predicate::str::contains("uninstall").not());
}

#[test]
fn health_never_creates_a_missing_runtime() {
    let temp = tempfile::tempdir().expect("tempdir");
    let missing_home = temp.path().join("misspelled-runtime");
    moon()
        .args([
            "--home",
            missing_home.to_str().expect("utf8"),
            "--dimensions",
            "64",
            "--json",
            "health",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""code":"operation_failed""#))
        .stderr(predicate::str::contains("does not exist"));
    assert!(!missing_home.exists());
}

#[test]
fn json_mode_returns_structured_argument_and_operation_errors() {
    moon()
        .args(["--json", "unknown-command"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""code":"invalid_arguments""#));

    let temp = tempfile::tempdir().expect("tempdir");
    moon()
        .args([
            "--home",
            temp.path().join("test").to_str().expect("utf8"),
            "--dimensions",
            "64",
            "--json",
            "remember",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(r#""code":"operation_failed""#));
}

#[test]
fn evidence_to_context_cli_workflow_is_self_contained() {
    let temp = tempfile::tempdir().expect("tempdir");
    let test_home = temp.path().join("moon");
    let transcript = temp.path().join("session.txt");
    std::fs::write(
        &transcript,
        "Decision: Moon context packets cite immutable session evidence.",
    )
    .expect("write transcript");

    moon()
        .args([
            "--home",
            test_home.to_str().expect("utf8"),
            "--dimensions",
            "64",
            "--json",
            "record",
            "--session-id",
            "cli-session",
            "--scope",
            "moon",
            "--file",
            transcript.to_str().expect("utf8"),
            "--completed-at-ms",
            "100",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""changed":true"#));

    moon()
        .args([
            "--home",
            test_home.to_str().expect("utf8"),
            "--dimensions",
            "64",
            "--json",
            "distill",
            "--key",
            "moon:context-packets",
            "--session-id",
            "cli-session",
            "--scope",
            "moon",
            "--content",
            "Moon context packets cite immutable session evidence.",
            "--evidence-quote",
            "Moon context packets cite immutable session evidence.",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""action":"created"#));

    moon()
        .args([
            "--home",
            test_home.to_str().expect("utf8"),
            "--dimensions",
            "64",
            "context",
            "--query",
            "immutable session evidence",
            "--scope",
            "moon",
            "--mode",
            "lexical",
            "--max-chars",
            "2000",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("# Moon Context"))
        .stdout(predicate::str::contains("cli-session"))
        .stdout(predicate::str::contains("immutable session evidence"));
}

#[test]
fn evidence_and_distillation_accept_private_payloads_on_stdin() {
    let temp = tempfile::tempdir().expect("tempdir");
    let test_home = temp.path().join("moon");
    let binary = moon;

    binary()
        .args([
            "--home",
            test_home.to_str().expect("utf8"),
            "--dimensions",
            "64",
            "--json",
            "record",
            "--session-id",
            "stdin-session",
            "--completed-at-ms",
            "100",
        ])
        .write_stdin("User prefers concise answers.")
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""changed":true"#));

    binary()
        .args([
            "--home",
            test_home.to_str().expect("utf8"),
            "--dimensions",
            "64",
            "--json",
            "distill",
            "--key",
            "user:preference:concise",
            "--session-id",
            "stdin-session",
            "--kind",
            "preference",
            "--proposal-json",
        ])
        .write_stdin(
            r#"{"content":"User prefers concise answers.","evidence_quote":"User prefers concise answers.","title":"Response style"}"#,
        )
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""action":"created"#));
}

#[test]
fn failed_distillation_batch_commits_nothing_and_can_be_retried() {
    let temp = tempfile::tempdir().expect("tempdir");
    let test_home = temp.path().join("moon");
    let command = || {
        let mut command = moon();
        command
            .arg("--home")
            .arg(&test_home)
            .args(["--dimensions", "64", "--json"]);
        command
    };
    command()
        .args([
            "record",
            "--session-id",
            "batch-session",
            "--completed-at-ms",
            "100",
        ])
        .write_stdin("User prefers mint tea. User prefers quiet rooms.")
        .assert()
        .success();
    let mut proposals = serde_json::json!([
        {
            "canonical_key": "user:tea",
            "content": "User prefers mint tea.",
            "evidence_quote": "User prefers mint tea."
        },
        {
            "canonical_key": "user:room",
            "content": "User prefers quiet rooms.",
            "evidence_quote": "User prefers loud rooms."
        }
    ]);
    command()
        .args(["distill-batch", "--session-id", "batch-session"])
        .write_stdin(serde_json::to_vec(&proposals).unwrap())
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "evidence_quote was not found exactly",
        ));
    command()
        .arg("health")
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""active_memories":0"#))
        .stdout(predicate::str::contains(r#""citations":0"#));

    proposals[1]["evidence_quote"] = serde_json::json!("User prefers quiet rooms.");
    command()
        .args(["distill-batch", "--session-id", "batch-session"])
        .write_stdin(serde_json::to_vec(&proposals).unwrap())
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""distilled":2"#));
    command()
        .arg("health")
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""active_memories":2"#))
        .stdout(predicate::str::contains(r#""citations":2"#));
}

#[test]
fn text_context_output_is_empty_when_nothing_is_relevant() {
    let temp = tempfile::tempdir().expect("tempdir");
    let test_home = temp.path().join("moon");
    moon()
        .args([
            "--home",
            test_home.to_str().expect("utf8"),
            "--dimensions",
            "64",
            "context",
            "--query",
            "Hi lilac",
            "--mode",
            "lexical",
        ])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}
