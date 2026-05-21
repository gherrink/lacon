//! Lazy-open invariant tests (CONTEXT D-04).
//!
//! `lacon --version`, `lacon validate <path>`, and `lacon doctor` MUST NOT
//! open the tracker DB. This is the core cold-start guarantee — the binary
//! is invoked thousands of times per session and any read-only path that
//! pays the SQLite open + migrate cost breaks the <10ms budget.
//!
//! Strategy: redirect XDG_DATA_HOME to a tempdir, run the read-only command,
//! then assert the DB file was NOT created.
//!
//! See: `crates/lacon-cli/src/commands/run.rs` is the SOLE reachable callsite
//! for `Tracker::open`. The grep-based source-invariant tests confirm that
//! contract via env!("CARGO_MANIFEST_DIR") (Issue #7 fix — no fragile
//! relative-path fallback).

use std::path::PathBuf;

use assert_cmd::Command;
use tempfile::tempdir;

fn db_path_under(xdg: &std::path::Path) -> PathBuf {
    xdg.join("lacon").join("history.db")
}

/// Resolve a source file under `crates/lacon-cli/src/...`. Tests run with
/// CARGO_MANIFEST_DIR pointing at the lacon-cli crate root, so the source
/// is at `${CARGO_MANIFEST_DIR}/src/...`. Issue #7 fix.
fn cli_src_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src").join(relative)
}

#[test]
fn version_does_not_open_db() {
    let xdg = tempdir().unwrap();
    Command::cargo_bin("lacon")
        .unwrap()
        .env("XDG_DATA_HOME", xdg.path())
        .env("XDG_CONFIG_HOME", xdg.path().join("config"))
        .args(["--version"])
        .assert()
        .success();

    let db = db_path_under(xdg.path());
    assert!(
        !db.exists(),
        "--version must NOT create history.db; saw {}",
        db.display()
    );
}

/// Issue #4 fix: split the assertion. The test's purpose is the lazy-open
/// invariant — NOT validate's correctness on a particular rule fixture.
/// Invoke `lacon validate <path>` without asserting exit code, then
/// independently assert `!db_path.exists()`.
#[test]
fn validate_does_not_open_db() {
    let xdg = tempdir().unwrap();
    let proj = tempdir().unwrap();
    let rule_path = proj.path().join("rule.yaml");
    std::fs::write(
        &rule_path,
        "id: cold\nmatch: { command: echo }\npipeline: []\n",
    )
    .unwrap();

    // Invoke validate. Do NOT assert exit code — the test's contract is
    // lazy-open, not validate's pass/fail behaviour.
    let _ = Command::cargo_bin("lacon")
        .unwrap()
        .env("XDG_DATA_HOME", xdg.path())
        .env("XDG_CONFIG_HOME", xdg.path().join("config"))
        .args(["validate", rule_path.to_str().unwrap()])
        .output();
    // ^ .output() runs the command; we intentionally ignore the ExitStatus.

    let db = db_path_under(xdg.path());
    assert!(
        !db.exists(),
        "validate must NOT create history.db (regardless of validate's own exit code); saw {}",
        db.display()
    );
}

#[test]
fn doctor_does_not_open_db() {
    // Phase 4 (Plan 04-04): doctor is now implemented. On a fresh machine (empty
    // XDG_DATA_HOME → no history.db) it reports the DB/perms/health checks as
    // informational (D-03) and exits 0. The load-bearing invariant this test
    // guards is unchanged: doctor's tracker check opens the DB *read-only*
    // (open_readonly, D-08) and checks `db_path.exists()` first, so a fresh run
    // NEVER creates history.db — preserving the D-04 cold-start guarantee.
    let xdg = tempdir().unwrap();
    let mut cmd = Command::cargo_bin("lacon").unwrap();
    cmd.env("XDG_DATA_HOME", xdg.path())
        .env("XDG_CONFIG_HOME", xdg.path().join("config"))
        .args(["doctor"]);
    // Fresh machine: every DB-dependent check is informational, so exit 0.
    cmd.assert().success();

    let db = db_path_under(xdg.path());
    assert!(
        !db.exists(),
        "doctor must NOT create history.db; saw {}",
        db.display()
    );
}

/// Issue #7 fix: source-grep test uses env!("CARGO_MANIFEST_DIR")
/// (per cli_validate.rs:7-10 reference pattern).
#[test]
fn validate_rs_does_not_reference_tracker() {
    let path = cli_src_path("commands/validate.rs");
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {} failed: {e}", path.display()));
    assert!(
        !src.contains("tracking::Tracker"),
        "validate.rs must not reference tracking::Tracker"
    );
    assert!(
        !src.contains("Tracker::open"),
        "validate.rs must not reference Tracker::open"
    );
}

#[test]
fn doctor_rs_does_not_reference_tracker() {
    let path = cli_src_path("commands/doctor.rs");
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {} failed: {e}", path.display()));
    // Phase 4 may add `tracking::health_check` (D-13) but MUST NOT call Tracker::open
    // unconditionally on the doctor path. Lock the contract.
    assert!(
        !src.contains("Tracker::open"),
        "doctor.rs must not reference Tracker::open (D-04 lazy-open invariant)"
    );
}
