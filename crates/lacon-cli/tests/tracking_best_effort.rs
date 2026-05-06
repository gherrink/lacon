//! Phase 2 best-effort posture tests (CONTEXT D-12).
//!
//! When the tracker cannot open or write — for example the data dir parent
//! is unwritable — the wrapper MUST:
//!   1. Log a `lacon: tracker open|write failed: ...` line to stderr
//!   2. Preserve the subprocess's exit code
//!   3. Continue to emit filtered stdout

#![cfg(unix)]

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

/// `/dev/null/sub` is unwritable on Unix — `/dev/null` is a character device,
/// not a directory, so `create_dir_all("/dev/null/sub")` fails fast.
const UNWRITABLE_XDG: &str = "/dev/null/lacon-test-unwritable";

#[test]
fn best_effort_unwritable_data_dir_preserves_exit_zero() {
    let proj = tempdir().unwrap();

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj.path())
        .env("XDG_DATA_HOME", UNWRITABLE_XDG)
        .env("XDG_CONFIG_HOME", proj.path().join("config"))
        .args(["run", "--", "echo", "hi"])
        .assert()
        .success() // exit code 0 — subprocess succeeded; tracker failed silently
        .stderr(predicate::str::contains("lacon: tracker"));
}

#[test]
fn best_effort_subprocess_exit_code_propagates() {
    let proj = tempdir().unwrap();

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj.path())
        .env("XDG_DATA_HOME", UNWRITABLE_XDG)
        .env("XDG_CONFIG_HOME", proj.path().join("config"))
        .args(["run", "--", "sh", "-c", "exit 42"])
        .assert()
        .code(42)
        .stderr(predicate::str::contains("lacon: tracker"));
}
