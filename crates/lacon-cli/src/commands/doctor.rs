//! `lacon doctor` subcommand: a fixed five-check health sweep (D-07).
//!
//! Doctor is pure composition — every check reuses an already-verified core
//! surface; this file constructs nothing new. The five checks, in order:
//!
//! 1. **Hook install** — `<cwd>/.claude/settings.json` carries the
//!    `lacon-claude-hook` `PreToolUse(Bash)` fingerprint that `lacon init`
//!    writes (A4 contract; mirrors `init.rs`'s walk).
//! 2. **Config per layer** — every existing `config.yaml` (project
//!    `<cwd>/.lacon/config.yaml` + user `<config_dir>/lacon/config.yaml`)
//!    passes `lacon_core::validate::validate_file`.
//! 3. **Rule sweep** — `RuleLoader::load_all()` parses every reachable rule
//!    across all three layers without error.
//! 4. **DB dir perms** — the `history.db` parent directory is `0700`.
//! 5. **Tracker health** — the DB opens **read-only** (`open_readonly`, D-08 —
//!    never the write/migrate path; no migrate/prune/INSERT) and a `SELECT 1`
//!    probe (`health::health_check`) succeeds.
//!
//! # Fresh-machine posture (D-03)
//! A brand-new project has no `.claude/settings.json` and no `history.db`. Those
//! states are reported as **informational** (a `[warn]` line pointing at the
//! remediation command) and do NOT flip the overall result to red. The command
//! exits 0 when no check hard-fails. A check only hard-fails on a *positively
//! broken* state (settings.json present but missing the hook; a config/rule that
//! does not validate; a DB dir with the wrong perms; a present DB that fails the
//! health probe).
//!
//! # Error posture (T-04-10)
//! Every parse / IO error is mapped to a printed line + a non-zero exit. No raw
//! `?` ever propagates an internal error to the user as a panic.

use std::path::{Path, PathBuf};

use lacon_core::rules::loader::RuleLoader;
use lacon_core::tracking::{self, health};
use lacon_core::validate::validate_file;

/// The lacon-managed hook fingerprint that `init.rs` writes (D-12, A4).
/// MUST stay byte-identical to `init::install_lacon_hook`'s inserted command.
const HOOK_FINGERPRINT: &str = "lacon-claude-hook";

/// Outcome of a single check line.
enum Status {
    /// Check passed.
    Pass,
    /// Check positively failed — flips the overall result to red (exit 1).
    Fail,
    /// Informational (fresh-machine state, D-03) — printed but never red.
    Warn,
}

/// Print one checklist line and return whether it should flip `all_ok`.
///
/// Only `Fail` flips `all_ok`; `Warn` (D-03 fresh-machine) is printed as `[warn]`
/// and leaves the overall result green.
fn report(status: Status, label: &str, detail: &str) -> bool {
    match status {
        Status::Pass => {
            println!("[ ok ] {label}: {detail}");
            true
        }
        Status::Warn => {
            println!("[warn] {label}: {detail}");
            true
        }
        Status::Fail => {
            println!("[fail] {label}: {detail}");
            false
        }
    }
}

/// Entry point dispatched from `cli.rs`'s `Doctor` variant. Takes no args.
///
/// Returns `Ok(0)` iff every check passes (warnings are not failures, D-03),
/// else `Ok(1)`. Mirrors the `validate`/`init` `Ok(0)`/`Ok(1)` convention; this
/// boundary never surfaces a raw error (T-04-10).
pub fn execute() -> anyhow::Result<i32> {
    let cwd = std::env::current_dir()?;
    let mut all_ok = true;

    all_ok &= check_hook(&cwd);
    all_ok &= check_configs(&cwd);
    all_ok &= check_rules(&cwd);
    all_ok &= check_db_perms();
    all_ok &= check_tracker_health();

    println!();
    if all_ok {
        println!("doctor: all checks passed");
        Ok(0)
    } else {
        println!("doctor: one or more checks failed");
        Ok(1)
    }
}

/// CHECK 1 (hook install). Reads `<cwd>/.claude/settings.json` and looks for the
/// `lacon-claude-hook` `PreToolUse(Bash)` fingerprint (A4; mirrors init.rs).
///
/// - No settings.json (fresh project) → informational (D-03), not red.
/// - settings.json present + fingerprint found → pass.
/// - settings.json present, no fingerprint → fail (run `lacon init`).
/// - settings.json present but unreadable / unparseable → fail (T-04-10).
fn check_hook(cwd: &Path) -> bool {
    let settings_path = cwd.join(".claude").join("settings.json");
    let text = match std::fs::read_to_string(&settings_path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return report(
                Status::Warn,
                "hook",
                "not installed (run `lacon init`)",
            );
        }
        Err(e) => {
            return report(
                Status::Fail,
                "hook",
                &format!("could not read {}: {e}", settings_path.display()),
            );
        }
    };

    let settings: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            return report(
                Status::Fail,
                "hook",
                &format!(
                    "could not parse {}: {e}",
                    settings_path.display()
                ),
            );
        }
    };

    // Walk hooks.PreToolUse[] for a Bash matcher whose inner command starts with
    // the lacon fingerprint (mirror init.rs:318-329 / init.rs:136-147).
    let has_hook = settings["hooks"]["PreToolUse"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|g| g["matcher"] == "Bash")
        .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
        .filter_map(|h| h["command"].as_str())
        .any(|c| c.starts_with(HOOK_FINGERPRINT));

    if has_hook {
        report(Status::Pass, "hook", "lacon-claude-hook installed")
    } else {
        report(
            Status::Fail,
            "hook",
            &format!(
                "Bash PreToolUse hook missing in {} — run `lacon init`",
                settings_path.display()
            ),
        )
    }
}

/// CHECK 2 (config per layer). Validates every *existing* `config.yaml`:
/// project `<cwd>/.lacon/config.yaml` and user `<config_dir>/lacon/config.yaml`.
/// Absent files are skipped (all optional per the config schema). A non-empty
/// `ValidationError` list fails the check, printing each error (path + message;
/// never raw file contents — T-04-13).
fn check_configs(cwd: &Path) -> bool {
    let mut paths: Vec<PathBuf> = Vec::new();
    paths.push(cwd.join(".lacon").join("config.yaml"));
    if let Some(user_dir) = user_config_dir() {
        paths.push(user_dir.join("config.yaml"));
    }

    let mut ok = true;
    let mut checked_any = false;
    for path in paths {
        if !path.exists() {
            continue;
        }
        checked_any = true;
        let errors = validate_file(&path);
        if errors.is_empty() {
            ok &= report(
                Status::Pass,
                "config",
                &format!("{} valid", path.display()),
            );
        } else {
            // Print the header line as the fail, then each structured error.
            ok &= report(
                Status::Fail,
                "config",
                &format!("{} has {} error(s):", path.display(), errors.len()),
            );
            for err in &errors {
                println!("         {err}");
            }
        }
    }

    if !checked_any {
        ok &= report(
            Status::Pass,
            "config",
            "no config.yaml present (defaults in effect)",
        );
    }
    ok
}

/// CHECK 3 (rule sweep). Eager-loads every reachable rule across all three
/// layers via `RuleLoader::load_all()`. `Ok(_)` passes; `Err(vec)` fails,
/// printing each error with its offending path.
fn check_rules(cwd: &Path) -> bool {
    let mut loader = RuleLoader::new(Some(cwd.to_path_buf()));
    match loader.load_all() {
        Ok(rules) => report(
            Status::Pass,
            "rules",
            &format!("{} rule(s) parse cleanly", rules.len()),
        ),
        Err(errors) => {
            let ok = report(
                Status::Fail,
                "rules",
                &format!("{} rule error(s):", errors.len()),
            );
            for err in &errors {
                println!("         {err}");
            }
            ok
        }
    }
}

/// CHECK 4 (DB dir perms). Resolves the `history.db` parent directory and checks
/// it is `0700`.
///
/// - Parent missing (fresh machine, DB never created) → informational (D-03).
/// - Parent present + `0700` → pass.
/// - Parent present + wrong mode → fail (`{actual:o}, expected 0700`).
fn check_db_perms() -> bool {
    let db_path = match tracking::Tracker::xdg_db_path() {
        Some(p) => p,
        None => {
            return report(
                Status::Fail,
                "db-perms",
                "could not resolve the XDG data directory",
            );
        }
    };
    let Some(parent) = db_path.parent() else {
        return report(Status::Fail, "db-perms", "DB path has no parent directory");
    };

    if !parent.exists() {
        return report(
            Status::Warn,
            "db-perms",
            "DB not yet initialized (run a command first)",
        );
    }

    perms_of(parent)
}

/// Unix: read the directory mode and compare to `0700`. On non-Unix (v1 excludes
/// Windows, but keep `cargo check` green) the perms concept does not apply, so
/// this passes informationally.
#[cfg(unix)]
fn perms_of(parent: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(parent) {
        Ok(meta) => {
            let mode = meta.permissions().mode() & 0o777;
            if mode == 0o700 {
                report(
                    Status::Pass,
                    "db-perms",
                    &format!("{} is 0700", parent.display()),
                )
            } else {
                report(
                    Status::Fail,
                    "db-perms",
                    &format!(
                        "{} perms are {mode:o}, expected 0700",
                        parent.display()
                    ),
                )
            }
        }
        Err(e) => report(
            Status::Fail,
            "db-perms",
            &format!("could not stat {}: {e}", parent.display()),
        ),
    }
}

#[cfg(not(unix))]
fn perms_of(_parent: &Path) -> bool {
    report(
        Status::Warn,
        "db-perms",
        "directory permission check is Unix-only",
    )
}

/// CHECK 5 (tracker health). If `history.db` exists, opens it **read-only**
/// (`open_readonly`, D-08 — never the write/migrate path; this never migrates,
/// prunes, or INSERTs) and runs the `SELECT 1` probe (`health::health_check`).
///
/// - DB absent (fresh machine) → informational (D-03), not red.
/// - DB present + probe Ok → pass.
/// - DB present + open/probe Err → fail.
fn check_tracker_health() -> bool {
    let db_path = match tracking::Tracker::xdg_db_path() {
        Some(p) => p,
        None => {
            return report(
                Status::Fail,
                "tracker",
                "could not resolve the XDG data directory",
            );
        }
    };

    if !db_path.exists() {
        return report(
            Status::Warn,
            "tracker",
            "history.db not yet initialized (run a command first)",
        );
    }

    // D-08: read-only open ONLY (never the write/migrate constructor).
    // open_readonly applies safe pragmas and never writes WAL / migrates /
    // prunes / INSERTs.
    let conn = match tracking::open_readonly(&db_path) {
        Ok(c) => c,
        Err(e) => {
            return report(
                Status::Fail,
                "tracker",
                &format!("could not open {} read-only: {e}", db_path.display()),
            );
        }
    };

    match health::health_check(&conn) {
        Ok(_) => report(Status::Pass, "tracker", "health probe (SELECT 1) ok"),
        Err(e) => report(
            Status::Fail,
            "tracker",
            &format!("health probe failed: {e}"),
        ),
    }
}

/// Resolve the user lacon config directory (`<config_dir>/lacon`) via etcetera,
/// mirroring `run.rs:182-187`. Honours `XDG_CONFIG_HOME` so tests stay isolated.
fn user_config_dir() -> Option<PathBuf> {
    use etcetera::BaseStrategy;
    etcetera::choose_base_strategy()
        .ok()
        .map(|s| s.config_dir().join("lacon"))
}
