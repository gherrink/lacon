//! `lacon init` subcommand: opt into lacon at a chosen scope (REQ-cli-init).
//!
//! lacon can be installed at two **scopes**, selected via `--project` / `--user`
//! (both may be passed → install both). Each scope performs the same three
//! idempotent install steps against scope-specific paths:
//!
//! 1. **Rules skeleton** — an empty rules directory with a `.gitkeep` so the
//!    directory survives `git clone` (project: `.lacon/`; user:
//!    `~/.config/lacon/rules/`, the XDG path the loader already reads).
//! 2. **`settings.json` PreToolUse(Bash) hook** — installs (or refreshes) the
//!    lacon-managed `lacon-claude-hook` entry inside `hooks.PreToolUse[]`. The
//!    file is parsed as a `serde_json::Value`, so unrelated user config
//!    (top-level keys, non-Bash matcher groups, user-authored Bash hooks) is
//!    preserved untouched (T-tor-01). Scrub-then-reinsert guarantees a
//!    byte-stable result across runs (idempotency). The write is atomic via
//!    `tempfile::NamedTempFile::persist` (POSIX rename(2)) and preserves the
//!    destination's Unix permissions — important for a real `~/.claude/settings.json`.
//!    Paths: project = `./.claude/settings.json`; user = `~/.claude/settings.json`.
//! 3. **`LACON.md` + `@import` reference** — the instructions live in a
//!    standalone `LACON.md` file (mentioning the `!!` bypass prefix and
//!    `LACON_DISABLE=1`). `CLAUDE.md` only carries a single, idempotent Claude
//!    Code `@import` reference line pointing at that `LACON.md` (T-tor-02).
//!    Block-level HTML comments in CLAUDE.md are stripped by Claude Code before
//!    injection, so the previous embedded HTML-comment instruction block never
//!    reliably reached the model — a real `@import` does.
//!
//!    The exact import token differs per scope because Claude Code resolves a
//!    relative `@path` relative to the file containing the import (verified
//!    empirically against `claude` 2.1.148, 2026-05-22 — see SUMMARY):
//!      - **user**: `~/.claude/CLAUDE.md` imports `@LACON.md` → `~/.claude/LACON.md`.
//!      - **project**: `./CLAUDE.md` (repo root) imports `@.claude/LACON.md` →
//!        `./.claude/LACON.md`.
//!
//!    The extensionless `@LACON` does NOT resolve and is therefore not used.
//!
//! For **project** scope only: if `./CLAUDE.md` does not exist, a warning is
//! printed (this may not be a Claude Code setup) and the file is created
//! carrying the reference. For **user** scope, `~/.claude/CLAUDE.md` is expected
//! to already exist; the reference is appended/refreshed idempotently.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// `LACON.md` body. MUST mention `!!` and `LACON_DISABLE=1` so the user-trust
/// property holds. Exact phrasing is the author's discretion.
const LACON_MD_BODY: &str = "# lacon\n\nBash command output in this environment is filtered by \
[lacon](https://github.com/) to reduce token usage — it trims noisy build/test output before it \
reaches the model, without dropping signal.\n\n## Bypassing the filter\n\n- Prefix a single command \
with `!!` to run it unfiltered (e.g. `!! pnpm test`).\n- Set `LACON_DISABLE=1` to disable filtering \
entirely for a command or session.\n";

/// The scope-specific Claude Code `@import` reference written into CLAUDE.md.
///
/// These are the EMPIRICALLY VERIFIED resolvable forms (claude 2.1.148): a
/// relative `@path` resolves relative to the importing file's directory. The
/// extensionless `@LACON` does NOT resolve, so it is deliberately not used.
const PROJECT_IMPORT_LINE: &str = "@.claude/LACON.md";
const USER_IMPORT_LINE: &str = "@LACON.md";

/// Which scope an install targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    Project,
    User,
}

impl Scope {
    fn label(self) -> &'static str {
        match self {
            Scope::Project => "project",
            Scope::User => "user",
        }
    }
}

/// Fully-resolved install paths + the import token for one scope.
struct ScopePaths {
    scope: Scope,
    /// Rules skeleton directory (`.gitkeep` is created inside it).
    rules_dir: PathBuf,
    /// `settings.json` to install the PreToolUse(Bash) hook into.
    settings_path: PathBuf,
    /// Standalone instructions file.
    lacon_md_path: PathBuf,
    /// CLAUDE.md that should carry the `@import` reference.
    claude_md_path: PathBuf,
    /// The exact, scope-correct `@import` line to install into `claude_md_path`.
    import_line: &'static str,
    /// Whether a missing `claude_md_path` is a warn-and-create case (project) or
    /// expected-to-exist-but-still-create case (user). Both create the file; only
    /// project warns. Captured as a flag so the message is scope-specific.
    warn_if_claude_md_missing: bool,
}

/// Entry point dispatched from `cli.rs`'s `Init` variant.
///
/// `user` / `project` come from the matching clap flags. Selection:
/// - either/both flag set → install that/those scope(s);
/// - neither flag set + a TTY on stdin → interactively prompt for the scope;
/// - neither flag set + NOT a TTY (CI / scripted / hermetic tests) → do not
///   block; default to **project** scope.
///
/// Returns `Ok(0)` on success, `Ok(1)` on a recoverable user/IO error (with a
/// `lacon init:`-prefixed stderr message), mirroring the `validate` convention.
pub fn execute(user: bool, project: bool) -> anyhow::Result<i32> {
    let scopes = match select_scopes(user, project) {
        Ok(s) => s,
        Err(code) => return Ok(code),
    };

    for scope in scopes {
        let paths = match resolve_scope_paths(scope) {
            Ok(p) => p,
            Err(e) => {
                eprintln!(
                    "lacon init: failed to resolve {} scope paths: {e}",
                    scope.label()
                );
                return Ok(1);
            }
        };
        println!("lacon init: installing {} scope", scope.label());
        match install_scope(&paths)? {
            0 => {}
            code => return Ok(code),
        }
    }

    Ok(0)
}

/// Resolve the set of scopes to install from the two flags + TTY heuristic.
///
/// Returns `Err(exit_code)` only when the interactive prompt yields no usable
/// answer (so `execute` can surface a clean `Ok(code)`).
fn select_scopes(user: bool, project: bool) -> Result<Vec<Scope>, i32> {
    if user || project {
        let mut scopes = Vec::with_capacity(2);
        if project {
            scopes.push(Scope::Project);
        }
        if user {
            scopes.push(Scope::User);
        }
        return Ok(scopes);
    }

    // Neither flag passed.
    use std::io::IsTerminal;
    if std::io::stdin().is_terminal() {
        prompt_for_scope()
    } else {
        // Non-interactive (CI / scripted / hermetic tests): never block on a
        // prompt — use the documented deterministic default.
        eprintln!(
            "lacon init: no scope flag and non-interactive stdin; defaulting to project scope \
             (pass --user and/or --project to choose explicitly)"
        );
        Ok(vec![Scope::Project])
    }
}

/// Interactive one-of-three scope prompt (TTY only). Implemented with a plain
/// `stdin().read_line` (no TUI crate) — the empirical D-ux measurement found a
/// TUI select crate (dialoguer) adds transitive deps and ~27 KB to the single
/// `lacon` binary for a binary-ish choice that a one-line read covers; see
/// SUMMARY. Never reached in hermetic tests (those always pass a flag).
fn prompt_for_scope() -> Result<Vec<Scope>, i32> {
    use std::io::Write;
    print!(
        "Install lacon for which scope?\n  [p] project (this directory)\n  [u] user (~/.claude, global)\n  [b] both\nChoice [p/u/b] (default p): "
    );
    let _ = std::io::stdout().flush();

    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        eprintln!("lacon init: failed to read scope selection");
        return Err(1);
    }
    match line.trim().to_ascii_lowercase().as_str() {
        "" | "p" | "project" => Ok(vec![Scope::Project]),
        "u" | "user" => Ok(vec![Scope::User]),
        "b" | "both" => Ok(vec![Scope::Project, Scope::User]),
        other => {
            eprintln!("lacon init: unrecognized scope '{other}'; expected p, u, or b");
            Err(1)
        }
    }
}

/// Compute the three install paths + import token for a scope.
fn resolve_scope_paths(scope: Scope) -> anyhow::Result<ScopePaths> {
    match scope {
        Scope::Project => {
            let cwd = std::env::current_dir()?;
            Ok(ScopePaths {
                scope,
                rules_dir: cwd.join(".lacon"),
                settings_path: cwd.join(".claude").join("settings.json"),
                lacon_md_path: cwd.join(".claude").join("LACON.md"),
                claude_md_path: cwd.join("CLAUDE.md"),
                import_line: PROJECT_IMPORT_LINE,
                warn_if_claude_md_missing: true,
            })
        }
        Scope::User => {
            // `~/.claude/*` triple via etcetera::home_dir() (reads $HOME, which
            // tests override). Do NOT use config_dir() here — that is the XDG
            // config dir, not `$HOME/.claude`.
            let home = etcetera::home_dir()?;
            let claude_dir = home.join(".claude");
            // User rules skeleton lives at the XDG config dir the loader reads
            // (mirrors loader.rs / doctor.rs), which honours XDG_CONFIG_HOME.
            use etcetera::BaseStrategy;
            let config_dir = etcetera::choose_base_strategy()?.config_dir();
            Ok(ScopePaths {
                scope,
                rules_dir: config_dir.join("lacon").join("rules"),
                settings_path: claude_dir.join("settings.json"),
                lacon_md_path: claude_dir.join("LACON.md"),
                claude_md_path: claude_dir.join("CLAUDE.md"),
                import_line: USER_IMPORT_LINE,
                warn_if_claude_md_missing: false,
            })
        }
    }
}

/// Run the three install steps against one scope's resolved paths.
///
/// Returns `Ok(0)` on success or `Ok(1)` on a recoverable error (after printing
/// a `lacon init:` stderr message), matching `execute`'s contract.
fn install_scope(paths: &ScopePaths) -> anyhow::Result<i32> {
    // Step 1: rules skeleton dir + .gitkeep.
    if let Err(e) = std::fs::create_dir_all(&paths.rules_dir) {
        eprintln!(
            "lacon init: failed to create rules dir {}: {e}",
            paths.rules_dir.display()
        );
        return Ok(1);
    }
    if let Err(e) = std::fs::write(paths.rules_dir.join(".gitkeep"), b"") {
        eprintln!("lacon init: failed to write .gitkeep: {e}");
        return Ok(1);
    }

    // Step 2: settings.json hook (scrub-then-reinsert + atomic, perm-preserving).
    let mut settings = match std::fs::read_to_string(&paths.settings_path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(value) => value,
            Err(e) => {
                eprintln!(
                    "lacon init: failed to parse {}: {e}",
                    paths.settings_path.display()
                );
                return Ok(1);
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(e) => {
            eprintln!(
                "lacon init: failed to read {}: {e}",
                paths.settings_path.display()
            );
            return Ok(1);
        }
    };
    // Defensive: a non-object settings file is not valid Claude Code config.
    if !settings.is_object() {
        eprintln!(
            "lacon init: {} is not a JSON object; refusing to overwrite",
            paths.settings_path.display()
        );
        return Ok(1);
    }
    install_lacon_hook(&mut settings);
    if let Err(e) = atomic_write_json(&paths.settings_path, &settings) {
        eprintln!(
            "lacon init: failed to write {}: {e}",
            paths.settings_path.display()
        );
        return Ok(1);
    }

    // Step 3a: standalone LACON.md (overwritten with canonical body each run).
    if let Some(parent) = paths.lacon_md_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            eprintln!("lacon init: failed to create {}: {e}", parent.display());
            return Ok(1);
        }
    }
    if let Err(e) = std::fs::write(&paths.lacon_md_path, LACON_MD_BODY) {
        eprintln!(
            "lacon init: failed to write {}: {e}",
            paths.lacon_md_path.display()
        );
        return Ok(1);
    }

    // Step 3b: idempotent @import reference in CLAUDE.md.
    let existing_md = match std::fs::read_to_string(&paths.claude_md_path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if paths.warn_if_claude_md_missing {
                eprintln!(
                    "lacon init: warning — {} does not exist; this may not be a Claude Code setup. \
                     Creating it with the lacon import reference.",
                    paths.claude_md_path.display()
                );
            }
            String::new()
        }
        Err(e) => {
            eprintln!(
                "lacon init: failed to read {}: {e}",
                paths.claude_md_path.display()
            );
            return Ok(1);
        }
    };
    let new_md = install_reference_line(&existing_md, paths.import_line);
    if let Err(e) = std::fs::write(&paths.claude_md_path, new_md) {
        eprintln!(
            "lacon init: failed to write {}: {e}",
            paths.claude_md_path.display()
        );
        return Ok(1);
    }

    println!(
        "lacon init: {} scope installed ({}, {} hook, {} + {} import)",
        paths.scope.label(),
        paths.rules_dir.display(),
        paths.settings_path.display(),
        paths.lacon_md_path.display(),
        paths.import_line,
    );
    Ok(0)
}

/// Install (or refresh) the lacon-managed `PreToolUse(Bash)` hook entry inside
/// `settings.hooks.PreToolUse[]`.
///
/// Scrub-then-reinsert: existing lacon entries (fingerprinted by command-string
/// prefix) are stripped, then exactly one canonical entry is appended. This is
/// what makes re-running `lacon init` byte-stable (idempotency) while leaving
/// every user-authored hook and every non-Bash matcher group untouched.
fn install_lacon_hook(settings: &mut Value) {
    // Ensure path: settings.hooks.PreToolUse exists and is an array.
    let hooks = settings
        .as_object_mut()
        .expect("settings is object")
        .entry("hooks")
        .or_insert_with(|| json!({}));
    // If `hooks` exists but is not an object (malformed user file), replace it
    // with a fresh object so the walk can proceed without panicking.
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let pretool = hooks
        .as_object_mut()
        .expect("hooks is object")
        .entry("PreToolUse")
        .or_insert_with(|| json!([]));
    if !pretool.is_array() {
        *pretool = json!([]);
    }
    let pretool_arr = pretool.as_array_mut().expect("PreToolUse is array");

    // Phase 1 (scrub): for each Bash matcher-group, drop inner hooks whose
    // command starts with the lacon fingerprint.
    for group in pretool_arr.iter_mut() {
        if group.get("matcher").and_then(Value::as_str) != Some("Bash") {
            continue;
        }
        let Some(inner) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
            continue;
        };
        inner.retain(|h| {
            let cmd = h.get("command").and_then(Value::as_str).unwrap_or("");
            !cmd.starts_with("lacon-claude-hook")
        });
    }

    // Phase 2 (cleanup): remove now-empty Bash matcher groups so the file stays
    // tidy. Non-Bash groups and non-empty Bash groups are kept.
    pretool_arr.retain(|group| {
        let is_bash = group.get("matcher").and_then(Value::as_str) == Some("Bash");
        if !is_bash {
            return true;
        }
        group
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty())
    });

    // Phase 3 (insert): append the canonical lacon entry.
    pretool_arr.push(json!({
        "matcher": "Bash",
        "hooks": [
            { "type": "command", "command": "lacon-claude-hook" }
        ]
    }));
}

/// Install the lacon `@import` reference line into a CLAUDE.md body idempotently.
///
/// - If `import_line` is already present (matched as a whole line, tolerant of
///   trailing whitespace), the body is returned unchanged → re-runs are
///   byte-stable and the line is never appended twice (T-tor-02).
/// - Otherwise the line is appended at EOF with a clean newline boundary, plus a
///   blank line of separation when the file is non-empty.
///
/// A second pass over the output is byte-identical (idempotency).
fn install_reference_line(existing: &str, import_line: &str) -> String {
    let already_present = existing.lines().any(|l| l.trim_end() == import_line);
    if already_present {
        return existing.to_string();
    }

    let mut out = String::with_capacity(existing.len() + import_line.len() + 2);
    out.push_str(existing);
    if !existing.is_empty() && !existing.ends_with('\n') {
        out.push('\n');
    }
    if !existing.is_empty() {
        out.push('\n'); // visual separation from prior content
    }
    out.push_str(import_line);
    out.push('\n');
    out
}

/// Write `value` to `path` atomically.
///
/// Creates the parent directory if missing, serializes with 2-space pretty
/// indent + trailing newline (Claude Code's conventional style), then `persist`es
/// a same-directory tempfile via POSIX rename(2) — atomic on macOS + Linux.
///
/// `persist` keeps the *tempfile's* mode (`0600`), not the destination's. To
/// avoid silently narrowing a pre-existing `settings.json`'s permissions (e.g. a
/// group-readable real `~/.claude/settings.json`), the original mode is read and
/// re-applied to the tempfile before `persist` when the destination exists.
fn atomic_write_json(path: &Path, value: &Value) -> anyhow::Result<()> {
    use std::io::Write;

    let parent = path.parent().expect("settings.json has a parent directory");
    std::fs::create_dir_all(parent)?;

    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    let bytes = serde_json::to_vec_pretty(value)?;
    tmp.write_all(&bytes)?;
    tmp.write_all(b"\n")?;
    tmp.flush()?;

    // Preserve the destination's existing permissions across the atomic replace.
    // Unix-only: v1 targets macOS + Linux. Best-effort — a metadata read failure
    // must not abort the (more important) atomic write.
    #[cfg(unix)]
    {
        if let Ok(meta) = std::fs::metadata(path) {
            let perms = meta.permissions();
            let _ = std::fs::set_permissions(tmp.path(), perms);
        }
    }

    tmp.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bash_lacon_commands(settings: &Value) -> Vec<String> {
        settings["hooks"]["PreToolUse"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|g| g["matcher"] == "Bash")
            .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
            .filter_map(|h| h["command"].as_str())
            .filter(|c| c.starts_with("lacon-claude-hook"))
            .map(String::from)
            .collect()
    }

    #[test]
    fn install_hook_into_empty_object_adds_one_entry() {
        let mut settings = json!({});
        install_lacon_hook(&mut settings);
        assert_eq!(bash_lacon_commands(&settings), vec!["lacon-claude-hook"]);
    }

    #[test]
    fn install_hook_is_idempotent() {
        let mut settings = json!({});
        install_lacon_hook(&mut settings);
        let after_one = settings.clone();
        install_lacon_hook(&mut settings);
        assert_eq!(after_one, settings, "second install is a structural no-op");
        assert_eq!(bash_lacon_commands(&settings).len(), 1);
    }

    #[test]
    fn install_hook_preserves_non_bash_and_user_bash_hooks() {
        let mut settings = json!({
            "model": "claude-opus-4",
            "hooks": {
                "PreToolUse": [
                    { "matcher": "Edit", "hooks": [{ "type": "command", "command": "my-edit-hook.sh" }] },
                    { "matcher": "Bash", "hooks": [{ "type": "command", "command": "my-bash-formatter.sh" }] }
                ]
            }
        });
        install_lacon_hook(&mut settings);

        // Top-level key untouched.
        assert_eq!(settings["model"], "claude-opus-4");
        // Edit matcher untouched.
        let pretool = settings["hooks"]["PreToolUse"].as_array().unwrap();
        let edit = pretool.iter().find(|g| g["matcher"] == "Edit").unwrap();
        assert_eq!(edit["hooks"][0]["command"], "my-edit-hook.sh");
        // User's Bash formatter preserved AND lacon hook added.
        let all_bash: Vec<&str> = pretool
            .iter()
            .filter(|g| g["matcher"] == "Bash")
            .flat_map(|g| g["hooks"].as_array().unwrap().iter())
            .filter_map(|h| h["command"].as_str())
            .collect();
        assert!(all_bash.contains(&"my-bash-formatter.sh"));
        assert!(all_bash.contains(&"lacon-claude-hook"));
    }

    #[test]
    fn install_hook_collapses_drifted_lacon_entries_to_one() {
        let mut settings = json!({
            "hooks": {
                "PreToolUse": [
                    { "matcher": "Bash", "hooks": [
                        { "type": "command", "command": "lacon-claude-hook" },
                        { "type": "command", "command": "lacon-claude-hook --debug" }
                    ]}
                ]
            }
        });
        install_lacon_hook(&mut settings);
        assert_eq!(bash_lacon_commands(&settings), vec!["lacon-claude-hook"]);
    }

    #[test]
    fn reference_line_appends_to_empty() {
        let out = install_reference_line("", PROJECT_IMPORT_LINE);
        assert_eq!(out, format!("{PROJECT_IMPORT_LINE}\n"));
        assert!(out.contains(PROJECT_IMPORT_LINE));
    }

    #[test]
    fn reference_line_appends_with_separation_to_nonempty() {
        let out = install_reference_line("# My Project\n\nNotes.\n", USER_IMPORT_LINE);
        assert!(out.starts_with("# My Project"));
        assert!(out.contains("Notes."));
        // Exactly one import line, separated by a blank line.
        assert!(out.ends_with(&format!("\n\n{USER_IMPORT_LINE}\n")));
    }

    #[test]
    fn reference_line_adds_trailing_newline_when_missing() {
        // File with no trailing newline must still get a clean boundary.
        let out = install_reference_line("no newline", PROJECT_IMPORT_LINE);
        assert!(out.contains("no newline\n"));
        assert!(out.trim_end().ends_with(PROJECT_IMPORT_LINE));
    }

    #[test]
    fn reference_line_is_idempotent() {
        let first = install_reference_line("# Project\n\nSome notes.\n", PROJECT_IMPORT_LINE);
        let second = install_reference_line(&first, PROJECT_IMPORT_LINE);
        assert_eq!(first, second, "second pass is byte-identical");
        assert_eq!(
            first.matches(PROJECT_IMPORT_LINE).count(),
            1,
            "import line appears exactly once"
        );
    }

    #[test]
    fn reference_line_detects_existing_with_trailing_whitespace() {
        // A pre-existing import line with trailing spaces must still be detected
        // so re-runs do not append a duplicate.
        let existing = format!("# Project\n\n{PROJECT_IMPORT_LINE}   \n");
        let out = install_reference_line(&existing, PROJECT_IMPORT_LINE);
        assert_eq!(
            out, existing,
            "trailing-whitespace match leaves file unchanged"
        );
        assert_eq!(out.matches(PROJECT_IMPORT_LINE).count(), 1);
    }

    #[test]
    fn project_and_user_import_tokens_are_the_verified_resolvable_forms() {
        // Guard against regressing to the non-resolving extensionless `@LACON`.
        assert_eq!(PROJECT_IMPORT_LINE, "@.claude/LACON.md");
        assert_eq!(USER_IMPORT_LINE, "@LACON.md");
        assert_ne!(PROJECT_IMPORT_LINE, "@LACON");
        assert_ne!(USER_IMPORT_LINE, "@LACON");
    }

    #[test]
    fn lacon_md_body_mentions_bypass_mechanics() {
        assert!(LACON_MD_BODY.contains("!!"));
        assert!(LACON_MD_BODY.contains("LACON_DISABLE"));
    }
}
