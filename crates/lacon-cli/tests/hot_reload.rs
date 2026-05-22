//! REQ-acceptance-hot-reload (SC2 second half, D-06): a rule edit takes effect
//! on the NEXT invocation, with NO daemon, NO file-watcher, and NO restart.
//!
//! Mechanism being proven (NOT built): the no-daemon process model (ADR-0013).
//! Every `lacon run` / `lacon-claude-hook` is a fresh OS process that re-reads
//! the rule files from disk. The in-process mtime cache
//! (`crates/lacon-core/src/rules/loader.rs:87-88, 262-274`) keys on
//! `(PathBuf, SystemTime)`, so an edit that changes the file's mtime is a cache
//! miss on the next process anyway — and since each invocation is a brand-new
//! process, the second `lacon run` always re-parses the edited rule. This test
//! ships a PROOF of that behavior; it adds no watcher, daemon, or new caching
//! mechanism (doing so would contradict the locked no-daemon ADR-0013).
//!
//! Isolation (Pitfall 4): tempdir project cwd + XDG_DATA_HOME/XDG_CONFIG_HOME
//! redirection so the test never touches the developer's real
//! `~/.claude/settings.json` or `~/.local/share/lacon/history.db`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

/// Write the project rule at `<dir>/.lacon/rules/test.yaml`
/// (mirrors `end_to_end.rs:19-23` / `hook_e2e.rs:42-46`).
fn write_rule(dir: &Path, content: &str) {
    let rules_dir = dir.join(".lacon").join("rules");
    fs::create_dir_all(&rules_dir).unwrap();
    fs::write(rules_dir.join("test.yaml"), content).unwrap();
}

/// Path to the rule file written by `write_rule`.
fn rule_path(dir: &Path) -> PathBuf {
    dir.join(".lacon").join("rules").join("test.yaml")
}

/// Resolves the cargo-built `test_emitter` artifact, NOT a PATH lookup
/// (anti-spoofing, T-07-04). Source: `end_to_end.rs:30-32`.
fn test_emitter_path() -> PathBuf {
    assert_cmd::cargo::cargo_bin("test_emitter")
}

/// XDG-redirecting `lacon` command builder so the binary never touches real
/// user state. Source: `cli_explain.rs:78-84` shape.
fn lacon(xdg: &Path, proj: &Path) -> Command {
    let mut cmd = Command::cargo_bin("lacon").unwrap();
    cmd.current_dir(proj)
        .env("XDG_DATA_HOME", xdg)
        .env("XDG_CONFIG_HOME", xdg.join("config"));
    cmd
}

/// REQ-acceptance-hot-reload: edit a rule file mid-session; the next `lacon run`
/// (a fresh process) reflects the edit. Two invocations across an mtime-changing
/// rewrite, no watcher, no daemon.
#[test]
fn rule_edit_takes_effect_on_next_invocation() {
    let xdg = tempdir().unwrap();
    let proj = tempdir().unwrap();
    let emitter = test_emitter_path();
    let emitter_name = emitter.file_name().unwrap().to_str().unwrap();
    let emitter_str = emitter.to_str().unwrap();

    // ── Rule v1: strip_ansi only → all emitter lines pass through ───────────
    // NOTE (WR-05): this test invokes `lacon run --rule hot-rule`, which resolves
    // the rule by id and never runs the matcher (run.rs bypasses
    // match_argv_via_load_all under `--rule`). The `match:` block below is therefore
    // INERT here — it is kept only so the fixtures mirror a real rule's shape. What
    // this test proves is hot reload (mtime-keyed re-read across invocations), not
    // matching.
    write_rule(
        proj.path(),
        &format!("id: hot-rule\nmatch: {{ command: {emitter_name} }}\npipeline:\n  - strip_ansi\n"),
    );

    // First invocation (fresh process) reflects v1: "line 1".."line 3" present.
    lacon(xdg.path(), proj.path())
        .args([
            "run",
            "--rule",
            "hot-rule",
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

    // ── Rule v2: drop every "line N" → emitter output filtered to nothing ───
    // Overwrite the SAME file with a DIFFERENT pipeline.
    write_rule(
        proj.path(),
        &format!(
            "id: hot-rule\nmatch: {{ command: {emitter_name} }}\npipeline:\n  - drop_regex: '^line '\n"
        ),
    );

    // Deterministically bump the rewritten file's mtime to an explicitly LATER
    // instant so the loader's (path, mtime) cache key changes even on
    // coarse-resolution filesystems. This is NOT a flaky sleep — we set an
    // absolute mtime 10s in the future via the std file API (Rust >= 1.75;
    // workspace MSRV 1.80). It guarantees the key differs without relying on
    // wall-clock progression between the two invocations.
    let later = SystemTime::now() + Duration::from_secs(10);
    let f = fs::File::options()
        .write(true)
        .open(rule_path(proj.path()))
        .unwrap();
    f.set_modified(later).unwrap();
    drop(f);

    // Second invocation (a brand-new process) MUST reflect v2: the "line N"
    // lines are now dropped. This proves the edit took effect on the next
    // invocation with no daemon/restart.
    lacon(xdg.path(), proj.path())
        .args([
            "run",
            "--rule",
            "hot-rule",
            "--",
            emitter_str,
            "--stdout-lines",
            "3",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("line 1").not())
        .stdout(predicate::str::contains("line 2").not())
        .stdout(predicate::str::contains("line 3").not());
}
