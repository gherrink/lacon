//! Black-box coverage for the scope-aware `lacon doctor` (REQ-cli-doctor).
//!
//! Each test runs the real `lacon` binary in an isolated tempdir cwd with
//! `HOME` + `XDG_DATA_HOME` + `XDG_CONFIG_HOME` redirected to tempdirs. The HOME
//! override is CRITICAL: doctor's user-scope checks read `~/.claude/*` via
//! `etcetera::home_dir()` (which reads `$HOME`), so without overriding `HOME`
//! every doctor test would read the developer's REAL `~/.claude`. `run_doctor`
//! now takes an explicit `home: &Path`, mirroring the user-scope tests in
//! `cli_init.rs`; every test passes a fresh tempdir so no test ever touches the
//! real home.
//!
//! The contract this locks (the LOCKED opt-in posture, see the SUMMARY):
//! - A scope is **configured** iff its `settings.json` carries the lacon hook.
//! - `doctor_all_green_*` — project hook installed (`lacon init`) + valid
//!   config/rules + a real seeded WAL `history.db` (via `lacon run`) + an empty
//!   HOME → project scope all `[ ok ]`, user scope shown NEUTRALLY, exit 0.
//! - `doctor_reports_invalid_config_*` / `..._rule_*` — a broken global check →
//!   the offending path appears and exit is non-zero.
//! - `doctor_neither_scope_configured_warns` — a project `settings.json` that
//!   parses but lacks the hook + an empty HOME (NEITHER scope configured) is now
//!   a `[warn]` ("run `lacon init`"), exit 0 — NOT a hard fail (the INTENTIONAL
//!   behavior change from `doctor_settings_present_without_hook_is_a_hard_fail`;
//!   rationale in the SUMMARY).
//! - `doctor_user_scope_complete_*` — user scope fully installed under tempdir
//!   HOME → user scope all `[ ok ]`, exit 0.
//! - `doctor_configured_but_broken_*` — a configured scope missing a sub-check
//!   (LACON.md or the `@import` reference) → `[fail]`, exit 1.
//! - `doctor_one_scope_only_*` — exactly one scope configured → the other scope
//!   rendered NEUTRALLY (never `[warn]`/`[fail]` for that scope), exit 0.
//! - `doctor_fresh_machine_*` — empty cwd + empty HOME + empty XDG → neither
//!   scope configured (`[warn] hook`) + informational DB lines, exit 0.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

/// The empirically verified resolvable import tokens (claude 2.1.148), mirrored
/// from `init.rs` / `cli_init.rs`. Extensionless `@LACON` does NOT resolve.
const PROJECT_IMPORT: &str = "@.claude/LACON.md";
const USER_IMPORT: &str = "@LACON.md";

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

/// Run `lacon doctor` in `proj` with `HOME` + XDG redirected to tempdirs.
///
/// `home` MUST be a tempdir so the user-scope checks never read the developer's
/// real `~/.claude`. Returns the completed assert handle.
fn run_doctor(proj: &Path, home: &Path, xdg: &Path) -> assert_cmd::assert::Assert {
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj)
        .env("HOME", home)
        .env("XDG_DATA_HOME", xdg)
        .env("XDG_CONFIG_HOME", xdg.join("config"))
        .arg("doctor")
        .assert()
}

/// Seed a real WAL `history.db` under `xdg` by running `lacon run` once. This
/// also creates the data dir at 0700 via `ensure_data_dir`, so the perms check
/// has a real, correctly-permissioned directory to validate.
fn seed_history_db(proj: &Path, home: &Path, xdg: &Path) {
    let emitter = test_emitter_path();
    let emitter_name = emitter.file_name().unwrap().to_str().unwrap();
    write_rule(proj, "doctor-seed", emitter_name);
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj)
        .env("HOME", home)
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

/// Install a complete user scope under tempdir `home` + `xdg` via `lacon init
/// --user` (HOME + XDG overridden), mirroring `cli_init.rs`. Pre-seeds
/// `~/.claude/CLAUDE.md` so the user reference line has a file to land in.
///
/// IMPORTANT: `XDG_CONFIG_HOME` is set to `<xdg>/config` here (matching
/// `run_doctor`'s split) so the user RULES skeleton (`<xdg>/config/lacon/rules`)
/// does NOT collide with doctor's DB DATA dir (`XDG_DATA_HOME=<xdg>` →
/// `<xdg>/lacon`). Otherwise init would create `<xdg>/lacon` at a non-0700 mode
/// and doctor's db-perms check would spuriously `[fail]`.
fn init_user_scope(home: &Path, xdg: &Path) {
    let claude_dir = home.join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(claude_dir.join("CLAUDE.md"), "# global memory\n").unwrap();
    Command::cargo_bin("lacon")
        .unwrap()
        // Run from an unrelated cwd to prove user scope does not touch cwd.
        .current_dir(xdg)
        .args(["init", "--user"])
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", xdg.join("config"))
        .assert()
        .success();
}

// ---------------------------------------------------------------------------
// Project scope green path (user scope empty → shown neutrally)
// ---------------------------------------------------------------------------

#[test]
fn doctor_all_green_passes_and_exits_zero() {
    let proj = tempdir().unwrap();
    let home = tempdir().unwrap(); // empty HOME → user scope not configured.
    let xdg = tempdir().unwrap();

    // Install the project hook (.claude/settings.json + LACON.md + @import).
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj.path())
        .args(["init", "--project"])
        .assert()
        .success();

    // Seed a real WAL history.db (also creates the 0700 data dir).
    seed_history_db(proj.path(), home.path(), xdg.path());

    run_doctor(proj.path(), home.path(), xdg.path())
        .success()
        // Project scope fully green (hook + instructions + reference).
        .stdout(predicate::str::contains("[ ok ] hook: project"))
        .stdout(predicate::str::contains("[ ok ] instructions: project"))
        .stdout(predicate::str::contains("[ ok ] reference: project"))
        // User scope (empty HOME) is shown NEUTRALLY, not warned/failed.
        .stdout(predicate::str::contains("[ -- ] hook: user"))
        // Global checks still green.
        .stdout(predicate::str::contains("[ ok ] rules"))
        .stdout(predicate::str::contains("[ ok ] db-perms"))
        .stdout(predicate::str::contains("[ ok ] tracker"))
        .stdout(predicate::str::contains("all checks passed"));
}

#[test]
fn doctor_reports_invalid_config_and_exits_nonzero() {
    let proj = tempdir().unwrap();
    let home = tempdir().unwrap();
    let xdg = tempdir().unwrap();

    // Install the project hook so the failure is unambiguously the config.
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj.path())
        .args(["init", "--project"])
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

    run_doctor(proj.path(), home.path(), xdg.path())
        .failure()
        // The offending path appears in the output (T-04-13: path, not contents).
        .stdout(predicate::str::contains("config.yaml"))
        .stdout(predicate::str::contains("[fail] config"))
        .stdout(predicate::str::contains("one or more checks failed"));
}

#[test]
fn doctor_reports_invalid_rule_and_exits_nonzero() {
    let proj = tempdir().unwrap();
    let home = tempdir().unwrap();
    let xdg = tempdir().unwrap();

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj.path())
        .args(["init", "--project"])
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

    run_doctor(proj.path(), home.path(), xdg.path())
        .failure()
        .stdout(predicate::str::contains("broken.yaml"))
        .stdout(predicate::str::contains("[fail] rules"))
        .stdout(predicate::str::contains("one or more checks failed"));
}

// ---------------------------------------------------------------------------
// Fresh-machine / neither-configured posture (D-03)
// ---------------------------------------------------------------------------

/// Fresh-machine case (D-03): an empty cwd, empty HOME, and empty XDG data dir.
/// NEITHER scope is configured, so each scope's hook line is a `[warn]` posture
/// line; the DB-perms/tracker checks render informational `[warn]` lines. No
/// panic, no hard red — doctor exits 0. A POSITIVELY broken state (a configured
/// scope missing a sub-check, an invalid config/rule, wrong perms) is what flips
/// it red.
#[test]
fn doctor_fresh_machine_is_informational_not_red() {
    let proj = tempdir().unwrap();
    let home = tempdir().unwrap();
    let xdg = tempdir().unwrap();

    run_doctor(proj.path(), home.path(), xdg.path())
        .success()
        // Neither scope configured → warn hook posture line with the init hint.
        .stdout(predicate::str::contains("[warn] hook"))
        .stdout(predicate::str::contains("run `lacon init`"))
        // DB-dependent checks informational.
        .stdout(predicate::str::contains("[warn] db-perms"))
        .stdout(predicate::str::contains("[warn] tracker"))
        .stdout(predicate::str::contains("all checks passed"));
}

/// INTENTIONAL BEHAVIOR CHANGE (see the SUMMARY): the old
/// `doctor_settings_present_without_hook_is_a_hard_fail` is REVISED. With two
/// opt-in scopes, a project `.claude/settings.json` that parses but lacks the
/// lacon hook is no longer "positively broken" — it just means that scope is
/// not configured. With an empty HOME (neither scope configured) this is now a
/// `[warn]` ("run `lacon init`"), exit 0 — NOT a hard fail.
#[test]
fn doctor_neither_scope_configured_warns_not_fails() {
    let proj = tempdir().unwrap();
    let home = tempdir().unwrap(); // empty HOME → user not configured.
    let xdg = tempdir().unwrap();

    let claude_dir = proj.path().join(".claude");
    std::fs::create_dir_all(&claude_dir).unwrap();
    // Valid JSON, valid Claude settings shape, but no lacon hook entry.
    std::fs::write(
        claude_dir.join("settings.json"),
        r#"{ "model": "claude-opus-4", "hooks": { "PreToolUse": [] } }"#,
    )
    .unwrap();

    run_doctor(proj.path(), home.path(), xdg.path())
        .success()
        .stdout(predicate::str::contains("[warn] hook"))
        .stdout(predicate::str::contains("run `lacon init`"))
        // Crucially NOT a hard fail anymore.
        .stdout(predicate::str::contains("[fail] hook").not())
        .stdout(predicate::str::contains("all checks passed"));
}

// ---------------------------------------------------------------------------
// User scope (HOME + XDG redirected — never real ~/.claude)
// ---------------------------------------------------------------------------

/// A fully-installed user scope (hook + LACON.md + `@LACON.md` reference under
/// tempdir HOME) with an empty project cwd: the user scope is all `[ ok ]`, the
/// project scope is shown NEUTRALLY, exit 0.
#[test]
fn doctor_user_scope_complete_passes() {
    let proj = tempdir().unwrap();
    let home = tempdir().unwrap();
    let xdg = tempdir().unwrap();

    init_user_scope(home.path(), xdg.path());

    run_doctor(proj.path(), home.path(), xdg.path())
        .success()
        .stdout(predicate::str::contains("[ ok ] hook: user"))
        .stdout(predicate::str::contains("[ ok ] instructions: user"))
        .stdout(predicate::str::contains("[ ok ] reference: user"))
        // The `@LACON.md` user import is the verified resolvable token.
        .stdout(predicate::str::contains(USER_IMPORT))
        // Project scope (empty cwd) shown neutrally, never warned/failed.
        .stdout(predicate::str::contains("[ -- ] hook: project"))
        .stdout(predicate::str::contains("[warn] hook: project").not())
        .stdout(predicate::str::contains("[fail] hook: project").not())
        .stdout(predicate::str::contains("all checks passed"));
}

/// A configured scope missing its `LACON.md` instructions sub-check is positively
/// broken → `[fail]` on instructions, exit 1. Here we install the user scope then
/// delete `~/.claude/LACON.md`, leaving the hook + reference intact.
#[test]
fn doctor_configured_but_broken_instructions_fails() {
    let proj = tempdir().unwrap();
    let home = tempdir().unwrap();
    let xdg = tempdir().unwrap();

    init_user_scope(home.path(), xdg.path());
    // Break the scope: remove the instructions file but keep the hook + import.
    std::fs::remove_file(home.path().join(".claude").join("LACON.md")).unwrap();

    run_doctor(proj.path(), home.path(), xdg.path())
        .failure()
        .stdout(predicate::str::contains("[fail] instructions: user"))
        .stdout(predicate::str::contains("one or more checks failed"));
}

/// A configured scope whose CLAUDE.md is missing the `@import` reference is
/// positively broken → `[fail]` on reference, exit 1. Here we install the user
/// scope then overwrite `~/.claude/CLAUDE.md` to drop the import line.
#[test]
fn doctor_configured_but_broken_reference_fails() {
    let proj = tempdir().unwrap();
    let home = tempdir().unwrap();
    let xdg = tempdir().unwrap();

    init_user_scope(home.path(), xdg.path());
    // Break the scope: strip the @import reference from CLAUDE.md (hook intact).
    std::fs::write(
        home.path().join(".claude").join("CLAUDE.md"),
        "# global memory\n\nno import here.\n",
    )
    .unwrap();

    run_doctor(proj.path(), home.path(), xdg.path())
        .failure()
        .stdout(predicate::str::contains("[fail] reference: user"))
        .stdout(predicate::str::contains("one or more checks failed"));
}

// ---------------------------------------------------------------------------
// One-scope-only neutrality (the user's explicit requirement)
// ---------------------------------------------------------------------------

/// Install ONLY the user scope; leave the project cwd empty/unconfigured. The
/// project scope must be rendered NEUTRALLY — never `[warn]`/`[fail]` for the
/// project group — and exit stays 0.
#[test]
fn doctor_user_only_renders_project_neutrally() {
    let proj = tempdir().unwrap();
    let home = tempdir().unwrap();
    let xdg = tempdir().unwrap();

    init_user_scope(home.path(), xdg.path());

    run_doctor(proj.path(), home.path(), xdg.path())
        .success()
        // User configured + complete.
        .stdout(predicate::str::contains("[ ok ] hook: user"))
        // Project NOT flagged — neutral marker present, no warn/fail for project.
        .stdout(predicate::str::contains("[ -- ] hook: project"))
        .stdout(predicate::str::contains("[warn] hook: project").not())
        .stdout(predicate::str::contains("[fail] hook: project").not())
        .stdout(predicate::str::contains("all checks passed"));
}

/// The inverse: install ONLY the project scope; leave HOME empty. The user scope
/// must be rendered NEUTRALLY, exit 0.
#[test]
fn doctor_project_only_renders_user_neutrally() {
    let proj = tempdir().unwrap();
    let home = tempdir().unwrap();
    let xdg = tempdir().unwrap();

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj.path())
        .args(["init", "--project"])
        .assert()
        .success();

    run_doctor(proj.path(), home.path(), xdg.path())
        .success()
        // Project configured + complete (includes the verified project import).
        .stdout(predicate::str::contains("[ ok ] hook: project"))
        .stdout(predicate::str::contains(PROJECT_IMPORT))
        // User NOT flagged — neutral, no warn/fail for user.
        .stdout(predicate::str::contains("[ -- ] hook: user"))
        .stdout(predicate::str::contains("[warn] hook: user").not())
        .stdout(predicate::str::contains("[fail] hook: user").not())
        .stdout(predicate::str::contains("all checks passed"));
}
