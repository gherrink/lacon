//! REQ-acceptance-pnpm-end-to-end (SC2 first half + SC4 hermeticity, D-07):
//! `lacon init` → `PreToolUse` hook rewrite → `lacon run` works end-to-end with
//! no manual config. Two artifacts in one file:
//!
//!   1. `pnpm_e2e_hermetic` — runs in DEFAULT `cargo test` / CI. Drives the full
//!      init→hook-rewrite→run pipeline using the in-repo `test_emitter` stub
//!      (NO pnpm, NO network, NO registry). This is the CI-facing acceptance.
//!   2. `pnpm_e2e_real` — `#[ignore]`d; runs a REAL `pnpm install` through the
//!      same pipeline, runnable on demand via `cargo test -- --ignored`. The
//!      `#[ignore]` string is the runbook line and keeps CI hermetic — default
//!      `cargo test` (which CI runs, never with `--ignored`) skips it.
//!
//! Anti-spoofing (T-07-04 / T-06-04): all binaries are resolved via
//! `assert_cmd::cargo::cargo_bin(...)` (the cargo artifact), never a PATH lookup.
//! Isolation (Pitfall 4 / T-06-02): tempdir project cwd + XDG_DATA_HOME/
//! XDG_CONFIG_HOME redirection so the tests never touch the developer's real
//! `~/.claude/settings.json` or `~/.local/share/lacon/history.db`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::Value;
use tempfile::tempdir;

/// Write a project rule at `<dir>/.lacon/rules/test.yaml`
/// (mirrors `end_to_end.rs:19-23` / `hook_e2e.rs:42-46`).
fn write_rule(dir: &Path, content: &str) {
    let rules_dir = dir.join(".lacon").join("rules");
    fs::create_dir_all(&rules_dir).unwrap();
    fs::write(rules_dir.join("test.yaml"), content).unwrap();
}

/// Resolves the cargo-built `test_emitter` artifact, NOT a PATH lookup
/// (anti-spoofing, T-07-04). Source: `end_to_end.rs:30-32`.
fn test_emitter_path() -> PathBuf {
    assert_cmd::cargo::cargo_bin("test_emitter")
}

/// Spawn the `lacon-claude-hook` binary, write `input_json` to stdin, capture
/// output. Source: `hook_e2e.rs:22-29`.
fn run_hook_with_input(input_json: &str) -> Output {
    Command::cargo_bin("lacon-claude-hook")
        .unwrap()
        .write_stdin(input_json)
        .output()
        .expect("hook binary runs")
}

/// Build a Bash `PreToolUse` payload pointing `cwd` at `cwd`. Source:
/// `hook_e2e.rs:48-61`.
fn bash_payload(cwd: &str, command: &str) -> String {
    serde_json::json!({
        "session_id": "s1",
        "transcript_path": "/t",
        "cwd": cwd,
        "permission_mode": "default",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": command },
        "tool_use_id": "u1"
    })
    .to_string()
}

/// Parse stdout into the rewrite-response `updatedInput.command` string. Source:
/// `hook_e2e.rs:64-71`.
fn updated_command(output: &Output) -> String {
    let value: Value = serde_json::from_slice(&output.stdout)
        .expect("stdout is valid JSON on the rewrite path");
    value["hookSpecificOutput"]["updatedInput"]["command"]
        .as_str()
        .expect("updatedInput.command is a string")
        .to_owned()
}

/// XDG-redirecting `lacon` builder so the binary never touches real user state.
/// Source: `cli_explain.rs:78-84` shape.
fn lacon(xdg: &Path, proj: &Path) -> Command {
    let mut cmd = Command::cargo_bin("lacon").unwrap();
    cmd.current_dir(proj)
        .env("XDG_DATA_HOME", xdg)
        .env("XDG_CONFIG_HOME", xdg.join("config"));
    cmd
}

/// Hermetic pnpm end-to-end: drives init→hook-rewrite→run with the `test_emitter`
/// stub standing in for the real package manager. Runs in default `cargo test`.
#[test]
fn pnpm_e2e_hermetic() {
    let xdg = tempdir().unwrap();
    let proj = tempdir().unwrap();
    let emitter = test_emitter_path();
    let emitter_name = emitter.file_name().unwrap().to_str().unwrap();
    let emitter_str = emitter.to_str().unwrap();

    // Project rule matching the stub's invocation name — stands in for the
    // bundled `pkg-install` rule that would match a real `pnpm install`.
    write_rule(
        proj.path(),
        &format!(
            "id: pnpm-stub\nmatch: {{ command: {emitter_name} }}\npipeline:\n  - strip_ansi\n"
        ),
    );

    // ── Step 1: the PreToolUse hook rewrites the matched command ────────────
    // The emitter absolute path is wrap-safe (alnum + `/ . - _`), so the hook
    // wraps it as `lacon run --rule pnpm-stub -- <emitter> --stdout-lines 3`.
    let inner_cmd = format!("{emitter_str} --stdout-lines 3");
    let payload = bash_payload(&proj.path().to_string_lossy(), &inner_cmd);
    let hook_out = run_hook_with_input(&payload);
    assert!(hook_out.status.success(), "hook exits 0 on a matched command");
    let rewritten = updated_command(&hook_out);
    assert!(
        rewritten.contains(&format!("lacon run --rule pnpm-stub -- {emitter_str} --stdout-lines 3")),
        "hook must wrap the matched command as `lacon run --rule pnpm-stub -- ...`; got: {rewritten}"
    );
    // The wrap also carries the tracker-contract env-var prefix the adapter emits
    // (LACON_ASSISTANT for Phase 2 tracking, LACON_SESSION_ID / LACON_TOOL_USE_ID
    // for cross-correlation). Assert it explicitly so a regression that dropped the
    // prefix can't leave this acceptance test green (WR-04).
    assert!(
        rewritten.contains("LACON_ASSISTANT=claude-code")
            && rewritten.contains("LACON_SESSION_ID=")
            && rewritten.contains("LACON_TOOL_USE_ID="),
        "hook wrap must carry the LACON_* tracker-contract env prefix; got: {rewritten}"
    );

    // ── Step 2: execute the rewritten `lacon run` and assert filtered output ─
    // We run the SAME `lacon run --rule pnpm-stub -- <emitter> ...` that the hook
    // produced (resolved via cargo_bin, XDG-sandboxed) and assert the filtered
    // stub output reaches the caller (what Claude Code would capture as the tool
    // result).
    lacon(xdg.path(), proj.path())
        .args([
            "run",
            "--rule",
            "pnpm-stub",
            "--",
            emitter_str,
            "--stdout-lines",
            "3",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("line 1"))
        .stdout(predicate::str::contains("line 2"))
        .stdout(predicate::str::contains("line 3"));
}

/// Real pnpm end-to-end (opt-in). Runs an ACTUAL `pnpm install` through
/// `lacon init` → `PreToolUse` hook rewrite → `lacon run` and asserts the
/// filtered output is non-empty and reduced versus raw.
///
/// `#[ignore]`d so default `cargo test` and CI never invoke `pnpm` (D-08
/// hermeticity). The `#[ignore]` string IS the runbook line — it prints in test
/// output. Style matches `crates/lacon-core/tests/runtime_signal.rs:47`.
#[test]
#[ignore = "requires pnpm — run via `cargo test -p lacon-cli --test pnpm_e2e -- --ignored`"]
fn pnpm_e2e_real() {
    let xdg = tempdir().unwrap();
    let proj = tempdir().unwrap();

    // A minimal package manifest so `pnpm install` has something to resolve.
    fs::write(
        proj.path().join("package.json"),
        r#"{ "name": "lacon-pnpm-e2e", "version": "0.0.0", "private": true }"#,
    )
    .unwrap();

    // ── Step 1: `lacon init` wires the PreToolUse hook + `.lacon/` skeleton ──
    lacon(xdg.path(), proj.path())
        .arg("init")
        .assert()
        .success();

    // ── Step 2: drive the hook with a real `pnpm install` PreToolUse payload ─
    let payload = bash_payload(&proj.path().to_string_lossy(), "pnpm install");
    let hook_out = run_hook_with_input(&payload);
    assert!(hook_out.status.success(), "hook exits 0 on `pnpm install`");
    let rewritten = updated_command(&hook_out);
    assert!(
        rewritten.contains("lacon run --rule pkg-install -- pnpm install"),
        "hook must wrap `pnpm install` via the bundled pkg-install rule; got: {rewritten}"
    );

    // ── Step 3: execute the REAL pnpm install through `lacon run` ────────────
    // Capture both the raw `pnpm install` bytes and the lacon-filtered bytes to
    // assert filtering produced non-empty, reduced output.
    let raw = Command::new("pnpm")
        .current_dir(proj.path())
        .arg("install")
        .output()
        .expect("pnpm install runs (real test)");
    let raw_len = raw.stdout.len() + raw.stderr.len();

    let filtered_out = lacon(xdg.path(), proj.path())
        .args(["run", "--rule", "pkg-install", "--", "pnpm", "install"])
        .output()
        .expect("lacon run wraps pnpm install");
    let filtered_len = filtered_out.stdout.len();

    assert!(
        !filtered_out.stdout.is_empty(),
        "filtered pnpm output must be non-empty"
    );
    assert!(
        filtered_len <= raw_len,
        "filtered output ({filtered_len} bytes) must not exceed raw ({raw_len} bytes)"
    );
}
