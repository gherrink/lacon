//! End-to-end coverage for `lacon init` (REQ-cli-init).
//!
//! Each test runs the real `lacon` binary in an isolated tempdir (no global
//! config writes — `lacon init` only touches cwd-relative paths: `.lacon/`,
//! `.claude/settings.json`, `CLAUDE.md`). The four tests lock the phase's
//! user-visible contract:
//!
//! - `init_in_empty_dir_creates_skeleton` — create path (D-11, D-14).
//! - `init_is_idempotent` — content-stable re-run (D-12, D-28, T-init-idempotency).
//! - `init_preserves_user_hooks_and_settings` — clobber-safety (D-28, T-settings-clobber).
//! - `init_re_runs_drop_old_lacon_entries` — drift collapse (D-12 scrub-then-reinsert).

use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

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

#[test]
fn init_in_empty_dir_creates_skeleton() {
    let dir = tempdir().unwrap();
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .success();

    // .lacon/ skeleton with .gitkeep so it survives `git clone`.
    assert!(dir.path().join(".lacon").is_dir());
    assert!(dir.path().join(".lacon/.gitkeep").is_file());

    // .claude/settings.json carries the lacon hook under matcher=Bash.
    let settings_text =
        fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
    let settings: serde_json::Value = serde_json::from_str(&settings_text).unwrap();
    assert!(settings["hooks"]["PreToolUse"].is_array());
    assert!(
        bash_commands(&settings).iter().any(|c| c == "lacon-claude-hook"),
        "lacon-claude-hook installed under matcher=Bash; got {settings_text}"
    );

    // CLAUDE.md block with markers + the user-trust keywords.
    let claude_md = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    assert!(claude_md.contains("<!-- lacon:start -->"));
    assert!(claude_md.contains("<!-- lacon:end -->"));
    assert!(claude_md.contains("!!"));
    assert!(claude_md.contains("LACON_DISABLE"));
}

#[test]
fn init_is_idempotent() {
    let dir = tempdir().unwrap();

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .success();
    let settings_v1 =
        fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
    let claude_md_v1 = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .success();
    let settings_v2 =
        fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
    let claude_md_v2 = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();

    assert_eq!(settings_v1, settings_v2, "settings.json byte-stable across runs");
    assert_eq!(claude_md_v1, claude_md_v2, "CLAUDE.md byte-stable across runs");
}

#[test]
fn init_preserves_user_hooks_and_settings() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".claude")).unwrap();
    fs::write(
        dir.path().join(".claude/settings.json"),
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

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .success();

    let settings: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();

    // Top-level key untouched.
    assert_eq!(settings["model"], "claude-opus-4");

    // Edit matcher group preserved entirely.
    let pretool = settings["hooks"]["PreToolUse"].as_array().unwrap();
    let edit_grp = pretool
        .iter()
        .find(|g| g["matcher"] == "Edit")
        .expect("Edit matcher preserved");
    assert_eq!(edit_grp["hooks"][0]["command"], "my-edit-hook.sh");

    // Bash matcher: user's formatter survives AND lacon hook is added.
    let cmds = bash_commands(&settings);
    assert!(
        cmds.iter().any(|c| c == "my-bash-formatter.sh"),
        "user's Bash hook preserved; got {cmds:?}"
    );
    assert!(
        cmds.iter().any(|c| c == "lacon-claude-hook"),
        "lacon hook added; got {cmds:?}"
    );
}

#[test]
fn init_re_runs_drop_old_lacon_entries() {
    // Simulate drift: two lacon-managed Bash entries pre-exist. After init,
    // exactly one canonical entry must remain (D-12 scrub-then-reinsert).
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
        .arg("init")
        .assert()
        .success();

    let settings: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap(),
    )
    .unwrap();

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
