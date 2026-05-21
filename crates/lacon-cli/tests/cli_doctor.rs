//! Black-box coverage for `lacon doctor` (REQ-cli-doctor).
//!
//! Each test runs the real `lacon` binary in an isolated tempdir cwd with
//! `XDG_DATA_HOME` / `XDG_CONFIG_HOME` redirected to tempdirs (the
//! `tracking_e2e.rs` isolation shape), so doctor's DB/perms/health checks read
//! a controlled `history.db` location and never touch the developer's real one.
//!
//! Three cases lock the user-visible contract:
//! - `doctor_all_green_*` — hook installed (`lacon init`) + valid config/rules +
//!   a real seeded WAL `history.db` (via `lacon run`) → all pass, exit 0.
//! - `doctor_reports_invalid_config_*` — one broken `.lacon/config.yaml` → the
//!   offending path appears in output and exit is non-zero.
//! - `doctor_fresh_machine_*` — empty tempdir, no settings.json, no DB → the
//!   hook/DB/health checks render as informational (D-03), no panic, exit 0.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

fn test_emitter_path() -> PathBuf {
    assert_cmd::cargo::cargo_bin("test_emitter")
}

/// Write a minimal valid project rule matching `command_basename`.
fn write_rule(proj: &Path, rule_id: &str, command_basename: &str) {
    let rules_dir = proj.join(".lacon").join("rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(
        rules_dir.join(format!("{rule_id}.yaml")),
        format!(
            "id: {rule_id}\nmatch: {{ command: {command_basename} }}\npipeline:\n  - strip_ansi\n"
        ),
    )
    .unwrap();
}

/// Run `lacon doctor` in `proj` with XDG redirected to `xdg`. Returns the
/// completed assert handle for stdout/stderr/exit assertions.
fn run_doctor(proj: &Path, xdg: &Path) -> assert_cmd::assert::Assert {
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj)
        .env("XDG_DATA_HOME", xdg)
        .env("XDG_CONFIG_HOME", xdg.join("config"))
        .arg("doctor")
        .assert()
}

/// Seed a real WAL `history.db` under `xdg` by running `lacon run` once. This
/// also creates the data dir at 0700 via `ensure_data_dir`, so the perms check
/// has a real, correctly-permissioned directory to validate.
fn seed_history_db(proj: &Path, xdg: &Path) {
    let emitter = test_emitter_path();
    let emitter_name = emitter.file_name().unwrap().to_str().unwrap();
    write_rule(proj, "doctor-seed", emitter_name);
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj)
        .env("XDG_DATA_HOME", xdg)
        .env("XDG_CONFIG_HOME", xdg.join("config"))
        .args([
            "run",
            "--rule",
            "doctor-seed",
            "--",
            emitter.to_str().unwrap(),
            "--stdout-lines",
            "1",
        ])
        .assert()
        .success();
}

#[test]
fn doctor_all_green_passes_and_exits_zero() {
    let proj = tempdir().unwrap();
    let xdg = tempdir().unwrap();

    // Install the hook (.claude/settings.json with lacon-claude-hook).
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj.path())
        .arg("init")
        .assert()
        .success();

    // Seed a real WAL history.db (also creates the 0700 data dir).
    seed_history_db(proj.path(), xdg.path());

    run_doctor(proj.path(), xdg.path())
        .success()
        .stdout(predicate::str::contains("[ ok ] hook"))
        .stdout(predicate::str::contains("[ ok ] rules"))
        .stdout(predicate::str::contains("[ ok ] db-perms"))
        .stdout(predicate::str::contains("[ ok ] tracker"))
        .stdout(predicate::str::contains("all checks passed"));
}

#[test]
fn doctor_reports_invalid_config_and_exits_nonzero() {
    let proj = tempdir().unwrap();
    let xdg = tempdir().unwrap();

    // Install the hook so the failure is unambiguously the config, not the hook.
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj.path())
        .arg("init")
        .assert()
        .success();

    // Write a project config.yaml with an unknown top-level key → validate_file
    // returns a non-empty error list (UnknownKey), which doctor must surface.
    let lacon_dir = proj.path().join(".lacon");
    std::fs::create_dir_all(&lacon_dir).unwrap();
    std::fs::write(
        lacon_dir.join("config.yaml"),
        "totally_not_a_real_key: 42\n",
    )
    .unwrap();

    run_doctor(proj.path(), xdg.path())
        .failure()
        // The offending path appears in the output (T-04-13: path, not contents).
        .stdout(predicate::str::contains("config.yaml"))
        .stdout(predicate::str::contains("[fail] config"))
        .stdout(predicate::str::contains("one or more checks failed"));
}

#[test]
fn doctor_reports_invalid_rule_and_exits_nonzero() {
    let proj = tempdir().unwrap();
    let xdg = tempdir().unwrap();

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj.path())
        .arg("init")
        .assert()
        .success();

    // A rule with an invalid regex → load_all() returns Err, doctor must fail
    // the rules check and name the offending path.
    let rules_dir = proj.path().join(".lacon").join("rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(
        rules_dir.join("broken.yaml"),
        "id: broken\nmatch: { command: echo }\npipeline:\n  - drop_regex: \"(unterminated\"\n",
    )
    .unwrap();

    run_doctor(proj.path(), xdg.path())
        .failure()
        .stdout(predicate::str::contains("broken.yaml"))
        .stdout(predicate::str::contains("[fail] rules"))
        .stdout(predicate::str::contains("one or more checks failed"));
}

/// Fresh-machine case (D-03): an empty tempdir with no `.claude/settings.json`
/// and an empty XDG data dir (no `history.db`). The hook/DB-perms/tracker checks
/// must render as informational `[warn]` lines — NOT a hard red failure or a
/// panic. Chosen exit semantics (documented for the SUMMARY): a fresh-but-no-hook
/// project is NOT an error — every DB-dependent check is informational and the
/// missing hook is informational too (a brand-new clone has not run `lacon init`
/// yet), so doctor exits 0. A POSITIVELY broken state (settings.json present but
/// hook missing, an invalid config/rule, wrong perms) is what flips it red.
#[test]
fn doctor_fresh_machine_is_informational_not_red() {
    let proj = tempdir().unwrap();
    let xdg = tempdir().unwrap();

    run_doctor(proj.path(), xdg.path())
        .success()
        // No panic, coherent informational output for the DB-dependent checks.
        .stdout(predicate::str::contains("[warn] hook"))
        .stdout(predicate::str::contains("[warn] db-perms"))
        .stdout(predicate::str::contains("[warn] tracker"))
        .stdout(predicate::str::contains("all checks passed"));
}

/// A settings.json that PARSES but lacks the lacon hook is a positively broken
/// state (the user has Claude Code config but never ran `lacon init`), so the
/// hook check is a hard `[fail]`, distinct from the fresh-machine `[warn]`.
#[test]
fn doctor_settings_present_without_hook_is_a_hard_fail() {
    let proj = tempdir().unwrap();
    let xdg = tempdir().unwrap();

    let claude_dir = proj.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    // Valid JSON, valid Claude settings shape, but no lacon hook entry.
    std::fs::write(
        claude_dir.join("settings.json"),
        r#"{ "model": "claude-opus-4", "hooks": { "PreToolUse": [] } }"#,
    )
    .unwrap();

    run_doctor(proj.path(), xdg.path())
        .failure()
        .stdout(predicate::str::contains("[fail] hook"))
        .stdout(predicate::str::contains("run `lacon init`"));
}
