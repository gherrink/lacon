//! End-to-end coverage for the scope-aware `lacon init` (REQ-cli-init).
//!
//! Each test runs the real `lacon` binary and is HERMETIC + NON-INTERACTIVE:
//! - scope is always driven by `--project` / `--user` flags, so no test ever
//!   blocks on the TTY scope prompt;
//! - user-scope tests redirect `HOME` and `XDG_CONFIG_HOME` to tempdirs (the
//!   pattern used by `tracking_coldstart.rs`) so the binary NEVER touches the
//!   developer's real `~/.claude` (T-tor-03).
//!
//! Contract locked here:
//! - **project scope** — `.lacon/.gitkeep`, the `lacon-claude-hook` settings
//!   entry under matcher=Bash, `./.claude/LACON.md` with the `!!` / `LACON_DISABLE`
//!   bypass keywords, and a resolvable `@.claude/LACON.md` import in `./CLAUDE.md`.
//! - **project CLAUDE.md-missing** — warn on stderr + create `./CLAUDE.md`.
//! - **user scope** — hook added to `~/.claude/settings.json` while unrelated
//!   config + user hooks are preserved; `~/.claude/LACON.md`; `@LACON.md` import
//!   in `~/.claude/CLAUDE.md`; `~/.config/lacon/rules/` skeleton.
//! - **both scopes** — one invocation installs project + user artifacts.
//! - **idempotency** — settings.json / CLAUDE.md / LACON.md byte-stable across
//!   runs; the import line appears exactly once.
//! - **permission preservation** — re-running never narrows settings.json's mode.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

/// The empirically verified resolvable import tokens (claude 2.1.148): see the
/// SUMMARY and `init.rs`. Extensionless `@LACON` does NOT resolve.
const PROJECT_IMPORT: &str = "@.claude/LACON.md";
const USER_IMPORT: &str = "@LACON.md";

/// Collect every `command` string under all matcher=Bash groups in PreToolUse.
fn bash_commands(settings: &serde_json::Value) -> Vec<String> {
    settings["hooks"]["PreToolUse"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|g| g["matcher"] == "Bash")
        .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
        .filter_map(|h| h["command"].as_str())
        .map(String::from)
        .collect()
}

fn read_json(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

// ---------------------------------------------------------------------------
// Project scope
// ---------------------------------------------------------------------------

#[test]
fn init_project_scope_creates_skeleton() {
    let dir = tempdir().unwrap();
    // Pre-seed a CLAUDE.md so this is the "existing setup" path.
    fs::write(dir.path().join("CLAUDE.md"), "# My Project\n").unwrap();

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .args(["init", "--project"])
        .assert()
        .success();

    // .lacon/ skeleton with .gitkeep so it survives `git clone`.
    assert!(dir.path().join(".lacon").is_dir());
    assert!(dir.path().join(".lacon/.gitkeep").is_file());

    // .claude/settings.json carries the lacon hook under matcher=Bash.
    let settings = read_json(&dir.path().join(".claude/settings.json"));
    assert!(settings["hooks"]["PreToolUse"].is_array());
    assert!(
        bash_commands(&settings)
            .iter()
            .any(|c| c == "lacon-claude-hook"),
        "lacon-claude-hook installed under matcher=Bash; got {settings}"
    );

    // Standalone LACON.md with the user-trust bypass keywords.
    let lacon_md = fs::read_to_string(dir.path().join(".claude/LACON.md")).unwrap();
    assert!(
        lacon_md.contains("!!"),
        "LACON.md must mention the !! bypass"
    );
    assert!(
        lacon_md.contains("LACON_DISABLE"),
        "LACON.md must mention LACON_DISABLE"
    );

    // CLAUDE.md carries the resolvable @import — NOT any old marker.
    let claude_md = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    assert!(
        claude_md.contains(PROJECT_IMPORT),
        "CLAUDE.md must contain the project import token; got {claude_md}"
    );
    assert!(
        claude_md.starts_with("# My Project"),
        "prior content preserved"
    );
    assert!(
        !claude_md.contains("<!-- lacon"),
        "no marker-block comment must remain"
    );
}

#[test]
fn init_project_scope_missing_claude_md_warns_and_creates() {
    let dir = tempdir().unwrap();
    // Empty cwd — no CLAUDE.md.
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .args(["init", "--project"])
        .assert()
        .success()
        .stderr(predicate::str::contains("may not be a Claude Code setup"));

    // CLAUDE.md is created carrying the import.
    let claude_md = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    assert!(
        claude_md.contains(PROJECT_IMPORT),
        "created CLAUDE.md has the import"
    );
}

#[test]
fn init_project_scope_is_idempotent() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("CLAUDE.md"), "# Project\n\nNotes.\n").unwrap();

    let run = || {
        Command::cargo_bin("lacon")
            .unwrap()
            .current_dir(dir.path())
            .args(["init", "--project"])
            .assert()
            .success();
    };
    run();
    let settings_v1 = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
    let claude_v1 = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    let lacon_v1 = fs::read_to_string(dir.path().join(".claude/LACON.md")).unwrap();

    run();
    let settings_v2 = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
    let claude_v2 = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    let lacon_v2 = fs::read_to_string(dir.path().join(".claude/LACON.md")).unwrap();

    assert_eq!(
        settings_v1, settings_v2,
        "settings.json byte-stable across runs"
    );
    assert_eq!(claude_v1, claude_v2, "CLAUDE.md byte-stable across runs");
    assert_eq!(lacon_v1, lacon_v2, "LACON.md byte-stable across runs");
    // The import line appears exactly once.
    assert_eq!(
        claude_v2.matches(PROJECT_IMPORT).count(),
        1,
        "import line appears exactly once; got {claude_v2}"
    );
}

// ---------------------------------------------------------------------------
// User scope (HOME + XDG_CONFIG_HOME redirected to tempdirs — never real ~/.claude)
// ---------------------------------------------------------------------------

#[test]
fn init_user_scope_preserves_config_and_installs_artifacts() {
    let home = tempdir().unwrap();
    let xdg = tempdir().unwrap();
    let claude_dir = home.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();

    // Pre-seed a REAL-looking ~/.claude/settings.json with unrelated config +
    // user hooks (a top-level model key, an Edit matcher, a user Bash hook).
    fs::write(
        claude_dir.join("settings.json"),
        r#"{
  "model": "claude-opus-4",
  "hooks": {
    "PreToolUse": [
      { "matcher": "Edit", "hooks": [{ "type": "command", "command": "my-edit-hook.sh" }] },
      { "matcher": "Bash", "hooks": [{ "type": "command", "command": "my-bash-formatter.sh" }] }
    ]
  }
}"#,
    )
    .unwrap();
    // Pre-seed ~/.claude/CLAUDE.md with prior user content.
    fs::write(
        claude_dir.join("CLAUDE.md"),
        "# My global memory\n\nremember this.\n",
    )
    .unwrap();

    Command::cargo_bin("lacon")
        .unwrap()
        // Run from an unrelated cwd to prove user scope does NOT touch cwd.
        .current_dir(xdg.path())
        .args(["init", "--user"])
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", xdg.path())
        .assert()
        .success();

    // settings.json: lacon hook added; unrelated config + user hooks preserved.
    let settings = read_json(&claude_dir.join("settings.json"));
    assert_eq!(
        settings["model"], "claude-opus-4",
        "top-level key preserved"
    );
    let pretool = settings["hooks"]["PreToolUse"].as_array().unwrap();
    let edit = pretool.iter().find(|g| g["matcher"] == "Edit").unwrap();
    assert_eq!(
        edit["hooks"][0]["command"], "my-edit-hook.sh",
        "Edit hook preserved"
    );
    let cmds = bash_commands(&settings);
    assert!(
        cmds.iter().any(|c| c == "my-bash-formatter.sh"),
        "user Bash hook preserved"
    );
    assert!(
        cmds.iter().any(|c| c == "lacon-claude-hook"),
        "lacon hook added"
    );

    // ~/.claude/LACON.md written with bypass keywords.
    let lacon_md = fs::read_to_string(claude_dir.join("LACON.md")).unwrap();
    assert!(lacon_md.contains("!!") && lacon_md.contains("LACON_DISABLE"));

    // ~/.claude/CLAUDE.md gains the user import once and keeps prior content.
    let claude_md = fs::read_to_string(claude_dir.join("CLAUDE.md")).unwrap();
    assert!(
        claude_md.contains("# My global memory"),
        "prior content preserved"
    );
    assert!(
        claude_md.contains("remember this."),
        "prior content preserved"
    );
    assert_eq!(
        claude_md.matches(USER_IMPORT).count(),
        1,
        "user import appears exactly once; got {claude_md}"
    );

    // Rules skeleton exists. On Linux the binary resolves the XDG config dir
    // (honouring XDG_CONFIG_HOME) → <xdg>/lacon/rules; on macOS etcetera uses the
    // Apple strategy under $HOME/Library/Application Support. Accept either so the
    // assertion is cross-platform and STILL proves no real-~ write (both candidate
    // roots are tempdirs).
    let xdg_rules = xdg.path().join("lacon/rules/.gitkeep");
    let apple_rules = home
        .path()
        .join("Library/Application Support/lacon/rules/.gitkeep");
    assert!(
        xdg_rules.is_file() || apple_rules.is_file(),
        "user rules skeleton must exist under a tempdir; checked {} and {}",
        xdg_rules.display(),
        apple_rules.display()
    );

    // CRITICAL: user scope must NOT have written project artifacts into cwd.
    assert!(
        !xdg.path().join(".lacon").exists(),
        "user scope must not create cwd .lacon/"
    );
    assert!(
        !xdg.path().join("CLAUDE.md").exists(),
        "user scope must not create a cwd CLAUDE.md"
    );
}

#[test]
fn init_user_scope_is_idempotent() {
    let home = tempdir().unwrap();
    let xdg = tempdir().unwrap();
    let claude_dir = home.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(claude_dir.join("CLAUDE.md"), "# Global\n").unwrap();

    let run = || {
        Command::cargo_bin("lacon")
            .unwrap()
            .current_dir(xdg.path())
            .args(["init", "--user"])
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", xdg.path())
            .assert()
            .success();
    };
    run();
    let settings_v1 = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
    let claude_v1 = fs::read_to_string(claude_dir.join("CLAUDE.md")).unwrap();
    let lacon_v1 = fs::read_to_string(claude_dir.join("LACON.md")).unwrap();

    run();
    let settings_v2 = fs::read_to_string(claude_dir.join("settings.json")).unwrap();
    let claude_v2 = fs::read_to_string(claude_dir.join("CLAUDE.md")).unwrap();
    let lacon_v2 = fs::read_to_string(claude_dir.join("LACON.md")).unwrap();

    assert_eq!(settings_v1, settings_v2, "settings.json byte-stable");
    assert_eq!(claude_v1, claude_v2, "CLAUDE.md byte-stable");
    assert_eq!(lacon_v1, lacon_v2, "LACON.md byte-stable");
    assert_eq!(claude_v2.matches(USER_IMPORT).count(), 1, "import once");
}

// ---------------------------------------------------------------------------
// Both scopes in one invocation
// ---------------------------------------------------------------------------

#[test]
fn init_both_scopes_install_project_and_user_artifacts() {
    let home = tempdir().unwrap();
    let cwd = tempdir().unwrap();
    let claude_dir = home.path().join(".claude");
    fs::create_dir_all(&claude_dir).unwrap();
    fs::write(claude_dir.join("CLAUDE.md"), "# Global\n").unwrap();
    fs::write(cwd.path().join("CLAUDE.md"), "# Project\n").unwrap();

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(cwd.path())
        .args(["init", "--user", "--project"])
        // XDG points under cwd so the user rules dir lands in a tempdir on Linux.
        .env("HOME", home.path())
        .env("XDG_CONFIG_HOME", cwd.path().join("xdg"))
        .assert()
        .success();

    // Project artifact (in cwd).
    assert!(
        cwd.path().join(".lacon/.gitkeep").is_file(),
        "project .lacon created"
    );
    let proj_md = fs::read_to_string(cwd.path().join("CLAUDE.md")).unwrap();
    assert!(
        proj_md.contains(PROJECT_IMPORT),
        "project import in cwd CLAUDE.md"
    );

    // User artifact (in tmp HOME).
    let user_md = fs::read_to_string(claude_dir.join("CLAUDE.md")).unwrap();
    assert!(
        user_md.contains(USER_IMPORT),
        "user import in ~/.claude/CLAUDE.md"
    );
    assert!(
        claude_dir.join("LACON.md").is_file(),
        "user LACON.md created"
    );
}

// ---------------------------------------------------------------------------
// Drift collapse + permission preservation (carried over, now flag-driven)
// ---------------------------------------------------------------------------

#[test]
fn init_re_runs_drop_old_lacon_entries() {
    // Simulate drift: two lacon-managed Bash entries pre-exist. After init,
    // exactly one canonical entry must remain (scrub-then-reinsert).
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".claude")).unwrap();
    fs::write(
        dir.path().join(".claude/settings.json"),
        r#"{
  "hooks": {
    "PreToolUse": [
      { "matcher": "Bash", "hooks": [
        { "type": "command", "command": "lacon-claude-hook" },
        { "type": "command", "command": "lacon-claude-hook --debug" }
      ]}
    ]
  }
}"#,
    )
    .unwrap();

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .args(["init", "--project"])
        .assert()
        .success();

    let settings = read_json(&dir.path().join(".claude/settings.json"));
    let lacon_entries: Vec<String> = bash_commands(&settings)
        .into_iter()
        .filter(|c| c.starts_with("lacon-claude-hook"))
        .collect();
    assert_eq!(
        lacon_entries,
        vec!["lacon-claude-hook".to_string()],
        "drifted lacon entries collapse to exactly one canonical form"
    );
}

/// Re-running `lacon init` must NOT silently narrow a pre-existing
/// `settings.json`'s file permissions. A user with a group-readable file
/// (`0644`) on a shared box should keep that mode after the atomic write.
#[cfg(unix)]
#[test]
fn init_preserves_existing_settings_file_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".claude")).unwrap();
    let settings_path = dir.path().join(".claude/settings.json");
    fs::write(&settings_path, "{}\n").unwrap();
    fs::set_permissions(&settings_path, fs::Permissions::from_mode(0o644)).unwrap();

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .args(["init", "--project"])
        .assert()
        .success();

    let mode_after = fs::metadata(&settings_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode_after, 0o644,
        "lacon init must preserve the original 0644 mode, got {mode_after:o}"
    );
}
