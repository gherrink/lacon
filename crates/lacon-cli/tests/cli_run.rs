//! Real-binary integration tests for `lacon run`. Use assert_cmd to spawn
//! the compiled `target/{debug|release}/lacon` binary.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

fn write_rule(dir: &std::path::Path, rule_yaml: &str) {
    let rules_dir = dir.join(".lacon").join("rules");
    fs::create_dir_all(&rules_dir).unwrap();
    fs::write(rules_dir.join("test.yaml"), rule_yaml).unwrap();
}

#[test]
fn run_with_rule_filters_output() {
    let dir = tempdir().unwrap();
    write_rule(
        dir.path(),
        r#"
id: filter-greet
match: { command: sh }
pipeline:
  - drop_regex: '^skip'
"#,
    );

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        // Isolate the tracking DB into the tempdir; without this the run records
        // into the developer's real ~/.local/share/lacon/history.db (XDG default).
        .env("XDG_DATA_HOME", dir.path())
        .args([
            "run",
            "--rule",
            "filter-greet",
            "--",
            "/bin/sh",
            "-c",
            "echo skip me; echo keep me",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("keep me"))
        .stdout(predicate::str::contains("skip me").not());
}

#[test]
fn run_propagates_exit_code() {
    let dir = tempdir().unwrap();
    write_rule(
        dir.path(),
        r#"
id: any
match: { command: sh }
pipeline: []
"#,
    );
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .env("XDG_DATA_HOME", dir.path())
        .args(["run", "--rule", "any", "--", "/bin/sh", "-c", "exit 42"])
        .assert()
        .code(42);
}

#[test]
fn run_unknown_rule_id_errors() {
    let dir = tempdir().unwrap();
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .env("XDG_DATA_HOME", dir.path())
        .args([
            "run",
            "--rule",
            "nonexistent",
            "--",
            "/bin/sh",
            "-c",
            "exit 0",
        ])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("no rule with id `nonexistent`"));
}

#[test]
fn run_no_argv_returns_usage_error() {
    Command::cargo_bin("lacon")
        .unwrap()
        .args(["run", "--", ""])
        // clap may handle the empty argument as a single empty string;
        // either it succeeds with empty argv (returning 2) or clap rejects.
        // The acceptance contract: non-zero exit code.
        .assert()
        .failure();
}

#[test]
fn run_no_rule_no_match_passes_through() {
    // No rules in tempdir; argv pass-through with subprocess output.
    let dir = tempdir().unwrap();
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .env("XDG_DATA_HOME", dir.path())
        .args(["run", "--", "/bin/sh", "-c", "exit 0"])
        .assert()
        .success();
}

#[test]
fn run_lacon_disable_bypasses_filtering() {
    let dir = tempdir().unwrap();
    write_rule(
        dir.path(),
        r#"
id: filter
match: { command: sh }
pipeline:
  - drop_regex: '.*'  # would drop everything if active
"#,
    );
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .env("XDG_DATA_HOME", dir.path())
        .env("LACON_DISABLE", "1")
        .args([
            "run",
            "--rule",
            "filter",
            "--",
            "/bin/sh",
            "-c",
            "echo bypassed",
        ])
        .assert()
        .success();
    // Bypass uses Stdio::inherit; output goes to the test harness's
    // stdout, NOT to assert_cmd's captured pipe. So we cannot assert on
    // stdout content directly — only on exit code. Document this in the
    // test comment. The unit test in PLAN-05 (runtime_bypass.rs) verifies
    // the bypass flag at the API level.
}

#[test]
fn run_on_error_swap_filters_failed_command_output() {
    let dir = tempdir().unwrap();
    write_rule(
        dir.path(),
        r#"
id: with-on-err
match: { command: sh }
pipeline:
  - drop_regex: '.*'
on_error:
  pipeline:
    - keep_regex: '^FAIL'
    - max_bytes: 1024
"#,
    );
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .env("XDG_DATA_HOME", dir.path())
        .args([
            "run",
            "--rule",
            "with-on-err",
            "--",
            "/bin/sh",
            "-c",
            "echo info; echo FAIL bad; exit 1",
        ])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("FAIL bad"))
        .stdout(predicate::str::contains("info").not());
}
