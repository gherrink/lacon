//! `lacon doctor` subcommand: a scope-aware setup + health sweep (D-07).
//!
//! Doctor is pure composition — every check reuses an already-verified core
//! surface; this file constructs nothing new. Output is grouped first by SCOPE
//! (the per-scope setup `lacon init` writes), then by the global checks below.
//!
//! # Per-scope setup checks (project + user)
//! `lacon init` installs at two **scopes** (project = cwd-relative, user =
//! `~/.claude` + `~/.config/lacon/rules`). For EACH scope, doctor verifies the
//! full setup `init` produces — three checks driven through one shared code path
//! ([`check_scope`]):
//!
//! 1. **hook** — the scope's `settings.json` carries the `lacon-claude-hook`
//!    `PreToolUse(Bash)` fingerprint that `lacon init` writes (A4 contract;
//!    mirrors `init.rs`'s walk). Presence of this fingerprint is what defines a
//!    scope as *configured*.
//! 2. **instructions** — the scope's `LACON.md` exists (project
//!    `<cwd>/.claude/LACON.md`; user `~/.claude/LACON.md`).
//! 3. **reference** — the scope's `CLAUDE.md` carries the scope-correct `@import`
//!    token (project `@.claude/LACON.md` in `<cwd>/CLAUDE.md`; user `@LACON.md`
//!    in `~/.claude/CLAUDE.md`). A whole-line substring scan is sufficient (the
//!    token form was empirically verified to resolve in 260522-tor); doctor does
//!    not shell out to `claude`.
//!
//! # Global checks (below the scope groups)
//! 4. **Config per layer** — every existing `config.yaml` (project
//!    `<cwd>/.lacon/config.yaml` + user `<config_dir>/lacon/config.yaml`)
//!    passes `lacon_core::validate::validate_file`.
//! 5. **Rule sweep** — `RuleLoader::load_all()` parses every reachable rule
//!    across all three layers without error.
//! 6. **DB dir perms** — the `history.db` parent directory is `0700`.
//! 7. **Tracker health** — the DB opens **read-only** (`open_readonly`, D-08 —
//!    never the write/migrate path; no migrate/prune/INSERT) and a `SELECT 1`
//!    probe (`health::health_check`) succeeds.
//!
//! # Opt-in posture (the key rule)
//! A scope is **configured** iff its `settings.json` carries the lacon hook. Let
//! `any_configured = project_configured || user_configured`.
//!
//! - **Configured + complete** → `[ ok ]` for hook + instructions + reference.
//! - **Configured + broken** (hook present but `LACON.md` missing OR the
//!   `@import` reference missing from CLAUDE.md) → `[fail]` for the missing
//!   sub-check → flips exit to 1. A half-installed scope is positively broken.
//! - **Not-configured scope WHILE the other scope IS configured** → rendered
//!   NEUTRALLY (`[ -- ]`, informational). Installing only one scope is a
//!   legitimate complete setup, so the other scope is NOT flagged as a warning
//!   or failure and does NOT affect the exit code.
//! - **Neither scope configured** (fresh machine / lacon not set up at all) →
//!   a single `[warn] hook` line with the `run \`lacon init\`` hint; exit 0
//!   (D-03 fresh-machine posture).
//! - **IO / parse errors** on a file that IS present (unreadable/unparseable
//!   settings.json or CLAUDE.md) → `[fail]` regardless (T-04-10 error posture).
//!
//! The DB-dependent global checks keep the D-03 fresh-machine posture: a missing
//! DB dir / `history.db` is informational (`[warn]`), never red.
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

/// The scope-correct `@import` tokens `init.rs` writes into CLAUDE.md. These MUST
/// stay byte-identical to `init`'s `PROJECT_IMPORT_LINE` / `USER_IMPORT_LINE`
/// (the EMPIRICALLY VERIFIED resolvable forms — claude 2.1.148). The
/// extensionless `@LACON` does NOT resolve and is deliberately not used.
const PROJECT_IMPORT_TOKEN: &str = "@.claude/LACON.md";
const USER_IMPORT_TOKEN: &str = "@LACON.md";

/// Outcome of a single check line.
enum Status {
    /// Check passed.
    Pass,
    /// Check positively failed — flips the overall result to red (exit 1).
    Fail,
    /// Informational (fresh-machine state, D-03) — printed as `[warn]` but never
    /// flips the overall result.
    Warn,
    /// Neutral / informational — printed as `[ -- ]` and never flips the overall
    /// result. Used for a not-configured scope when the OTHER scope IS configured
    /// (installing only one scope is a legitimate setup, so the un-configured one
    /// is shown neutrally, not as a warning or failure).
    Info,
}

/// Print one checklist line and return whether it should flip `all_ok`.
///
/// Only `Fail` flips `all_ok` (returns `false`); `Pass`, `Warn` (D-03
/// fresh-machine), and `Info` (neutral not-configured-while-other-configured) all
/// leave the overall result green.
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
        Status::Info => {
            println!("[ -- ] {label}: {detail}");
            true
        }
        Status::Fail => {
            println!("[fail] {label}: {detail}");
            false
        }
    }
}

/// Fully-resolved per-scope verification paths + import token. Mirrors
/// `init::ScopePaths` (the source of truth doctor verifies against).
struct ScopeTargets {
    /// Human label for the scope's group header / lines (`project` / `user`).
    label: &'static str,
    /// The scope's `settings.json` (carries the PreToolUse(Bash) hook).
    settings_path: PathBuf,
    /// The scope's standalone instructions file (`LACON.md`).
    lacon_md_path: PathBuf,
    /// The scope's `CLAUDE.md` that should carry the `@import` reference.
    claude_md_path: PathBuf,
    /// The exact, scope-correct `@import` token expected in `claude_md_path`.
    import_token: &'static str,
}

/// Entry point dispatched from `cli.rs`'s `Doctor` variant. Takes no args.
///
/// Drives the scope-aware opt-in posture: each configured scope's three setup
/// checks are emitted and folded into the result; a not-configured scope is shown
/// neutrally when the other scope IS configured, or a single `[warn]` when
/// NEITHER is configured (fresh machine). The four global checks follow.
///
/// Returns `Ok(0)` iff every check passes (warnings / neutral lines are not
/// failures, D-03), else `Ok(1)`. Mirrors the `validate`/`init` `Ok(0)`/`Ok(1)`
/// convention; this boundary never surfaces a raw error (T-04-10).
pub fn execute() -> anyhow::Result<i32> {
    let cwd = std::env::current_dir()?;
    let mut all_ok = true;

    // Resolve both scopes' verification targets (mirror init.rs path resolution).
    let project = ScopeTargets {
        label: "project",
        settings_path: cwd.join(".claude").join("settings.json"),
        lacon_md_path: cwd.join(".claude").join("LACON.md"),
        claude_md_path: cwd.join("CLAUDE.md"),
        import_token: PROJECT_IMPORT_TOKEN,
    };
    let user = user_claude_dir().map(|claude_dir| ScopeTargets {
        label: "user",
        settings_path: claude_dir.join("settings.json"),
        lacon_md_path: claude_dir.join("LACON.md"),
        claude_md_path: claude_dir.join("CLAUDE.md"),
        import_token: USER_IMPORT_TOKEN,
    });

    // Determine each scope's configured-state FIRST (cheap hook probe) so the
    // cross-scope posture is known before rendering the not-configured line for
    // an un-configured scope.
    let project_configured = scope_hook_present(&project.settings_path);
    let user_configured = user
        .as_ref()
        .map(|u| scope_hook_present(&u.settings_path))
        .unwrap_or(false);
    let any_configured = project_configured || user_configured;

    // Project setup group.
    println!("Project setup:");
    if project_configured {
        all_ok &= check_scope(&project);
    } else if any_configured {
        // The other (user) scope IS configured → show project neutrally.
        report(
            Status::Info,
            "hook",
            "project setup: not configured (optional)",
        );
    } else {
        // Neither scope configured (fresh machine) → single warn posture line.
        report(Status::Warn, "hook", "not installed (run `lacon init`)");
    }

    // User setup group.
    println!();
    println!("User setup:");
    match &user {
        Some(u) if user_configured => {
            all_ok &= check_scope(u);
        }
        _ if any_configured => {
            // The other (project) scope IS configured → show user neutrally. This
            // also covers the case where `user_claude_dir()` is None (treated as
            // not-configured) while project is configured.
            report(
                Status::Info,
                "hook",
                "user setup: not configured (optional)",
            );
        }
        _ => {
            // Neither scope configured (fresh machine) → single warn posture line.
            report(Status::Warn, "hook", "not installed (run `lacon init`)");
        }
    }

    // Global checks (unchanged) below the scope groups.
    println!();
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

/// Cheap, side-effect-free probe: does `settings_path` parse AND carry the
/// `lacon-claude-hook` `PreToolUse(Bash)` fingerprint? This is the single source
/// of the *configured* predicate (used by `execute` BEFORE rendering, so the
/// cross-scope posture is known up front). An absent OR unparseable settings.json
/// returns `false` here — `check_scope` is responsible for the present-but-broken
/// `[fail]` line; this probe only answers "is the hook installed?".
fn scope_hook_present(settings_path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(settings_path) else {
        return false;
    };
    let Ok(settings) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    settings_has_hook(&settings)
}

/// The shared `hooks.PreToolUse[]` walk: a Bash matcher whose inner command
/// starts with the lacon fingerprint (mirror init.rs:373-405 / init.rs:476-488).
/// This is the ONE walk reused by both scopes and by `scope_hook_present`.
fn settings_has_hook(settings: &serde_json::Value) -> bool {
    settings["hooks"]["PreToolUse"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|g| g["matcher"] == "Bash")
        .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
        .filter_map(|h| h["command"].as_str())
        .any(|c| c.starts_with(HOOK_FINGERPRINT))
}

/// Per-scope setup check (project + user share this ONE code path). Called by
/// `execute` only for a CONFIGURED scope (or a scope whose present settings.json
/// is broken). Emits a line for each of the three sub-checks and returns whether
/// the scope as a whole is OK (`true`) or has a positively broken sub-check
/// (`false` → flips the overall exit to 1).
///
/// Sub-checks (all greppable: each line's label contains `hook`):
/// - **hook** — `settings_path` parses and carries the fingerprint. A present
///   but unreadable/unparseable settings.json is `[fail]` (T-04-10) and returns
///   early. (An absent settings.json never reaches here — `execute` only calls
///   this for a configured scope, and configured implies the file parsed.)
/// - **instructions** — `lacon_md_path` is `Path::is_file()` → `[ ok ]`, else
///   `[fail]` (a half-installed scope is positively broken).
/// - **reference** — `claude_md_path` carries the scope's `@import` token as a
///   whole line (tolerant of trailing whitespace, mirroring init's
///   `install_reference_line`) → `[ ok ]`. Missing token → `[fail]`. A
///   NotFound CLAUDE.md while configured → `[fail]` (the reference is missing).
///   A present-but-unreadable CLAUDE.md → `[fail]` (T-04-10).
fn check_scope(targets: &ScopeTargets) -> bool {
    let label = targets.label;

    // hook sub-check (also re-confirms the file still parses; T-04-10).
    let text = match std::fs::read_to_string(&targets.settings_path) {
        Ok(t) => t,
        Err(e) => {
            return report(
                Status::Fail,
                "hook",
                &format!(
                    "{label}: could not read {}: {e}",
                    targets.settings_path.display()
                ),
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
                    "{label}: could not parse {}: {e}",
                    targets.settings_path.display()
                ),
            );
        }
    };
    let mut ok = if settings_has_hook(&settings) {
        report(
            Status::Pass,
            "hook",
            &format!("{label}: lacon-claude-hook installed"),
        )
    } else {
        // Defensive: configured was true at probe time but the hook is gone now.
        report(
            Status::Fail,
            "hook",
            &format!(
                "{label}: Bash PreToolUse hook missing in {} — run `lacon init`",
                targets.settings_path.display()
            ),
        )
    };

    // instructions sub-check: the scope's LACON.md must exist.
    ok &= if targets.lacon_md_path.is_file() {
        report(
            Status::Pass,
            "instructions",
            &format!("{label}: {} present", targets.lacon_md_path.display()),
        )
    } else {
        report(
            Status::Fail,
            "instructions",
            &format!(
                "{label}: {} missing — run `lacon init`",
                targets.lacon_md_path.display()
            ),
        )
    };

    // reference sub-check: the scope's CLAUDE.md must carry the @import token.
    ok &= check_reference(targets);

    ok
}

/// reference sub-check helper for one scope. Reads `claude_md_path` and verifies
/// the scope-correct `@import` token is present as a whole line (tolerant of
/// trailing whitespace). See [`check_scope`] for the posture this enforces.
fn check_reference(targets: &ScopeTargets) -> bool {
    let label = targets.label;
    let text = match std::fs::read_to_string(&targets.claude_md_path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Configured scope whose CLAUDE.md is missing → the reference cannot
            // be present → positively broken.
            return report(
                Status::Fail,
                "reference",
                &format!(
                    "{label}: {} missing — run `lacon init`",
                    targets.claude_md_path.display()
                ),
            );
        }
        Err(e) => {
            // Present but unreadable → fail (T-04-10).
            return report(
                Status::Fail,
                "reference",
                &format!(
                    "{label}: could not read {}: {e}",
                    targets.claude_md_path.display()
                ),
            );
        }
    };

    // Whole-line match tolerant of trailing whitespace (mirror init.rs's
    // install_reference_line `l.trim_end() == import_line`).
    let present = text.lines().any(|l| l.trim_end() == targets.import_token);
    if present {
        report(
            Status::Pass,
            "reference",
            &format!(
                "{label}: {} imports {}",
                targets.claude_md_path.display(),
                targets.import_token
            ),
        )
    } else {
        report(
            Status::Fail,
            "reference",
            &format!(
                "{label}: {} is missing the `{}` import — run `lacon init`",
                targets.claude_md_path.display(),
                targets.import_token
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
            ok &= report(Status::Pass, "config", &format!("{} valid", path.display()));
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
                    &format!("{} perms are {mode:o}, expected 0700", parent.display()),
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
///
/// NOTE: this is the XDG `~/.config/lacon` path (for the user `config.yaml`
/// check) — a DIFFERENT directory from [`user_claude_dir`]'s `~/.claude`. Do NOT
/// conflate the two.
fn user_config_dir() -> Option<PathBuf> {
    use etcetera::BaseStrategy;
    etcetera::choose_base_strategy()
        .ok()
        .map(|s| s.config_dir().join("lacon"))
}

/// Resolve the user `~/.claude` directory (where `init --user` writes the hook /
/// `LACON.md` / `CLAUDE.md`), mirroring `init.rs:211-212`. Reads `$HOME` via
/// `etcetera::home_dir()` so tests override it.
///
/// This is intentionally NOT [`user_config_dir`]: that resolves the XDG
/// `~/.config/lacon` path for `config.yaml`; this resolves `$HOME/.claude` for
/// the user-scope setup checks. Conflating the two is the bug this avoids.
fn user_claude_dir() -> Option<PathBuf> {
    etcetera::home_dir().ok().map(|home| home.join(".claude"))
}
