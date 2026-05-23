//! Workspace-level end-to-end integration tests for Phase 1.
//!
//! Layered above the per-crate tests in crates/lacon-{core,cli}/tests/.
//! These tests exercise the full pipeline: lacon binary -> RuleLoader ->
//! Runner -> test_emitter subprocess -> filtered output -> assert_cmd.
//!
//! The test_emitter binary is resolved via assert_cmd::cargo_bin, which uses
//! cargo metadata to locate the compiled artifact. This ensures we always use
//! the cargo-built artifact, NOT a PATH lookup (T-07-04 mitigation).
//!
//! Discoverable via:  cargo test -p lacon-cli --test end_to_end

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn write_rule(dir: &std::path::Path, content: &str) {
    let rules_dir = dir.join(".lacon").join("rules");
    fs::create_dir_all(&rules_dir).unwrap();
    fs::write(rules_dir.join("test.yaml"), content).unwrap();
}

/// Returns the absolute path to the compiled `test_emitter` binary.
///
/// Uses assert_cmd's cargo_bin lookup which resolves to the workspace artifact
/// in `target/{debug|release}/test_emitter`, not to whatever `test_emitter`
/// might be on the user's PATH. This satisfies T-07-04 (anti-spoofing).
fn test_emitter_path() -> PathBuf {
    assert_cmd::cargo::cargo_bin("test_emitter")
}

#[test]
fn end_to_end_strip_ansi_and_drop_stderr() {
    let dir = tempdir().unwrap();
    let emitter_path = test_emitter_path();
    let emitter_name = emitter_path.file_name().unwrap().to_str().unwrap();

    write_rule(
        dir.path(),
        &format!(
            r#"
id: e2e-strip
match: {{ command: {} }}
pipeline:
  - strip_ansi
  - drop_regex: '^err '
"#,
            emitter_name
        ),
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
            "e2e-strip",
            "--",
            emitter_path.to_str().unwrap(),
            "--stdout-lines",
            "3",
            "--stderr-lines",
            "2",
            "--ansi",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("line 1"))
        .stdout(predicate::str::contains("line 2"))
        .stdout(predicate::str::contains("line 3"))
        // stderr lines merged in and dropped by drop_regex:
        .stdout(predicate::str::contains("err 1").not())
        // ANSI codes stripped — the ESC character should not appear:
        .stdout(predicate::str::contains("\x1b[31m").not());
}

#[test]
fn end_to_end_on_error_swap_with_failing_subprocess() {
    let dir = tempdir().unwrap();
    let emitter_path = test_emitter_path();
    let emitter_name = emitter_path.file_name().unwrap().to_str().unwrap();

    write_rule(
        dir.path(),
        &format!(
            r#"
id: e2e-on-err
match: {{ command: {} }}
pipeline:
  - drop_regex: '.*'
on_error:
  pipeline:
    - keep_regex: '^FAIL'
    - max_bytes: 1024
"#,
            emitter_name
        ),
    );

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .env("XDG_DATA_HOME", dir.path())
        .args([
            "run",
            "--rule",
            "e2e-on-err",
            "--",
            emitter_path.to_str().unwrap(),
            "--stdout-lines",
            "5",
            "--errors",
            "3",
            "--exit",
            "1",
        ])
        .assert()
        .code(1)
        // on_error pipeline: keep_regex '^FAIL' passes FAIL lines through
        .stdout(predicate::str::contains("FAIL error 1"))
        .stdout(predicate::str::contains("FAIL error 2"))
        .stdout(predicate::str::contains("FAIL error 3"))
        // success-pipeline output ("line 1"..."line 5") dropped by drop_regex .*:
        .stdout(predicate::str::contains("line 1").not());
}

#[test]
fn end_to_end_max_bytes_truncation_marker_byte_exact() {
    let dir = tempdir().unwrap();
    let emitter_path = test_emitter_path();
    let emitter_name = emitter_path.file_name().unwrap().to_str().unwrap();

    write_rule(
        dir.path(),
        &format!(
            r#"
id: e2e-max-bytes
match: {{ command: {} }}
pipeline:
  - max_bytes: 200
"#,
            emitter_name
        ),
    );

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .env("XDG_DATA_HOME", dir.path())
        .args([
            "run",
            "--rule",
            "e2e-max-bytes",
            "--",
            emitter_path.to_str().unwrap(),
            "--bytes",
            "10000",
        ])
        .assert()
        .success()
        // Truncation marker must be present — byte-exact marker from Stage::MaxBytes:
        .stdout(predicate::str::contains("[lacon: truncated, "))
        .stdout(predicate::str::contains(" more bytes dropped]"));
}

#[test]
fn end_to_end_validate_then_run() {
    // Round-trip: validate a rule, then use it. Catches integration bugs
    // where validation accepts something the runtime rejects (or vice versa).
    let dir = tempdir().unwrap();
    let emitter_path = test_emitter_path();
    let emitter_name = emitter_path.file_name().unwrap().to_str().unwrap().to_owned();

    let rule_yaml = format!(
        r#"
id: e2e-roundtrip
match: {{ command: {} }}
pipeline:
  - strip_ansi
  - max_bytes: 4096
"#,
        emitter_name
    );

    let rule_path = dir.path().join("rule.yaml");
    fs::write(&rule_path, &rule_yaml).unwrap();

    // 1. Validate the rule file — must exit 0 with no stderr.
    Command::cargo_bin("lacon")
        .unwrap()
        .args(["validate", rule_path.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());

    // 2. Install the rule and run it — must produce the expected output.
    write_rule(dir.path(), &rule_yaml);
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .env("XDG_DATA_HOME", dir.path())
        .args([
            "run",
            "--rule",
            "e2e-roundtrip",
            "--",
            emitter_path.to_str().unwrap(),
            "--stdout-lines",
            "2",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("line 1"))
        .stdout(predicate::str::contains("line 2"));
}

#[test]
fn end_to_end_lacon_disable_propagates_subprocess_exit() {
    // With LACON_DISABLE=1, no filtering; exit code must still propagate.
    let dir = tempdir().unwrap();
    let emitter_path = test_emitter_path();
    let emitter_name = emitter_path.file_name().unwrap().to_str().unwrap();

    write_rule(
        dir.path(),
        &format!(
            r#"
id: e2e-bypass
match: {{ command: {} }}
pipeline:
  - drop_regex: '.*'
"#,
            emitter_name
        ),
    );

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .env("XDG_DATA_HOME", dir.path())
        .env("LACON_DISABLE", "1")
        .args([
            "run",
            "--rule",
            "e2e-bypass",
            "--",
            emitter_path.to_str().unwrap(),
            "--exit",
            "5",
        ])
        .assert()
        .code(5);
}
