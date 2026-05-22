//! Phase 2 end-to-end CLI integration tests for the tracking subsystem.
//!
//! Each test redirects `XDG_DATA_HOME` AND `XDG_CONFIG_HOME` to tempdirs so
//! the lacon binary writes its history.db AND reads its config from
//! controlled locations. Spawn pattern follows
//! `crates/lacon-cli/tests/end_to_end.rs` (Phase 1).
//!
//! Covers Phase 2 success criteria:
//!   SC1 — DB exists at XDG path; parent dir 0700; WAL on; row in invocations
//!   SC2 — store_raw_outputs:false default → no raw_outputs rows
//!         + flipping project config:true triggers warning + marker (Issue #9)
//!   SC3 — all 4 views queryable

use std::path::PathBuf;

use assert_cmd::Command;
use rusqlite::Connection;
use tempfile::tempdir;

fn test_emitter_path() -> PathBuf {
    assert_cmd::cargo::cargo_bin("test_emitter")
}

fn write_rule(dir: &std::path::Path, rule_id: &str, command_basename: &str) {
    let rules_dir = dir.join(".lacon").join("rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(
        rules_dir.join(format!("{rule_id}.yaml")),
        format!(
            "id: {rule_id}\nmatch: {{ command: {command_basename} }}\npipeline:\n  - strip_ansi\n",
        ),
    )
    .unwrap();
}

fn db_path_under(xdg_data_home: &std::path::Path) -> PathBuf {
    xdg_data_home.join("lacon").join("history.db")
}

fn run_lacon_with_xdg(
    xdg_data_home: &std::path::Path,
    proj_dir: &std::path::Path,
    emitter: &PathBuf,
    rule_id: &str,
    extra_envs: &[(&str, &str)],
) {
    let mut cmd = Command::cargo_bin("lacon").unwrap();
    cmd.current_dir(proj_dir)
        .env("XDG_DATA_HOME", xdg_data_home)
        .env("XDG_CONFIG_HOME", xdg_data_home.join("config"))
        .args([
            "run",
            "--rule",
            rule_id,
            "--",
            emitter.to_str().unwrap(),
            "--stdout-lines",
            "1",
        ]);
    for (k, v) in extra_envs {
        cmd.env(k, v);
    }
    cmd.assert().success();
}

#[test]
fn db_created_at_xdg_path() {
    let proj = tempdir().unwrap();
    let xdg = tempdir().unwrap();
    let emitter = test_emitter_path();
    let emitter_name = emitter.file_name().unwrap().to_str().unwrap();
    write_rule(proj.path(), "e2e-ok", emitter_name);

    run_lacon_with_xdg(xdg.path(), proj.path(), &emitter, "e2e-ok", &[]);

    let db = db_path_under(xdg.path());
    assert!(db.exists(), "history.db created at {}", db.display());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(db.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o700, "parent dir 0700");
    }
}

#[test]
fn single_row_after_run() {
    let proj = tempdir().unwrap();
    let xdg = tempdir().unwrap();
    let emitter = test_emitter_path();
    let emitter_name = emitter.file_name().unwrap().to_str().unwrap();
    write_rule(proj.path(), "e2e-row", emitter_name);

    run_lacon_with_xdg(xdg.path(), proj.path(), &emitter, "e2e-row", &[]);

    let conn = Connection::open(db_path_under(xdg.path())).unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM invocations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "exactly one row");

    let (assistant, exit_code, rule_id, rule_source, command_normalized): (
        String, i64, Option<String>, Option<String>, String,
    ) = conn
        .query_row(
            "SELECT assistant, exit_code, rule_id, rule_source, command_normalized FROM invocations",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(assistant, "claude-code", "default LACON_ASSISTANT");
    assert_eq!(exit_code, 0);
    assert_eq!(rule_id.as_deref(), Some("e2e-row"));
    assert_eq!(rule_source.as_deref(), Some("project"));
    assert_eq!(
        command_normalized, emitter_name,
        "command_normalized = basename"
    );
}

#[test]
fn raw_outputs_empty_by_default() {
    let proj = tempdir().unwrap();
    let xdg = tempdir().unwrap();
    let emitter = test_emitter_path();
    let emitter_name = emitter.file_name().unwrap().to_str().unwrap();
    write_rule(proj.path(), "e2e-raw-off", emitter_name);

    run_lacon_with_xdg(xdg.path(), proj.path(), &emitter, "e2e-raw-off", &[]);

    let conn = Connection::open(db_path_under(xdg.path())).unwrap();
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM raw_outputs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0, "raw_outputs is OFF by default");

    let raw_id: Option<i64> = conn
        .query_row("SELECT raw_output_id FROM invocations", [], |r| r.get(0))
        .unwrap();
    assert!(raw_id.is_none(), "raw_output_id NULL on default config");
}

#[test]
fn all_4_views_queryable_after_run() {
    let proj = tempdir().unwrap();
    let xdg = tempdir().unwrap();
    let emitter = test_emitter_path();
    let emitter_name = emitter.file_name().unwrap().to_str().unwrap();
    write_rule(proj.path(), "e2e-views", emitter_name);

    run_lacon_with_xdg(xdg.path(), proj.path(), &emitter, "e2e-views", &[]);

    let conn = Connection::open(db_path_under(xdg.path())).unwrap();
    for view in [
        "v_unmatched_offenders",
        "v_filtered_offenders",
        "v_bypass_rate",
        "v_project_savings",
    ] {
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM {view}"))
            .unwrap_or_else(|e| panic!("prepare {view} failed: {e}"));
        let _: usize = stmt
            .query([])
            .unwrap_or_else(|e| panic!("query {view} failed: {e}"))
            .mapped(|_| Ok(()))
            .count();
    }
}

#[test]
fn lacon_assistant_env_override() {
    let proj = tempdir().unwrap();
    let xdg = tempdir().unwrap();
    let emitter = test_emitter_path();
    let emitter_name = emitter.file_name().unwrap().to_str().unwrap();
    write_rule(proj.path(), "e2e-asst", emitter_name);

    run_lacon_with_xdg(
        xdg.path(),
        proj.path(),
        &emitter,
        "e2e-asst",
        &[("LACON_ASSISTANT", "test-assistant")],
    );

    let conn = Connection::open(db_path_under(xdg.path())).unwrap();
    let assistant: String = conn
        .query_row("SELECT assistant FROM invocations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(assistant, "test-assistant");
}

#[test]
fn lacon_session_id_env_propagation() {
    let proj = tempdir().unwrap();
    let xdg = tempdir().unwrap();
    let emitter = test_emitter_path();
    let emitter_name = emitter.file_name().unwrap().to_str().unwrap();
    write_rule(proj.path(), "e2e-session", emitter_name);

    run_lacon_with_xdg(
        xdg.path(),
        proj.path(),
        &emitter,
        "e2e-session",
        &[("LACON_SESSION_ID", "sess-abc")],
    );

    let conn = Connection::open(db_path_under(xdg.path())).unwrap();
    let sess: Option<String> = conn
        .query_row("SELECT session_id FROM invocations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(sess.as_deref(), Some("sess-abc"));
}

#[test]
fn journal_mode_wal_persists_after_lacon_run() {
    let proj = tempdir().unwrap();
    let xdg = tempdir().unwrap();
    let emitter = test_emitter_path();
    let emitter_name = emitter.file_name().unwrap().to_str().unwrap();
    write_rule(proj.path(), "e2e-wal", emitter_name);

    run_lacon_with_xdg(xdg.path(), proj.path(), &emitter, "e2e-wal", &[]);

    let conn = Connection::open(db_path_under(xdg.path())).unwrap();
    let mode: String = conn
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_ascii_lowercase(), "wal");
}

/// Issue #9 (revision iteration 1): SC2 reachable via CLI.
/// Writing project `.lacon/config.yaml` with `store_raw_outputs: true` and
/// running `lacon run` twice MUST emit the privacy warning EXACTLY once and
/// create the marker file. Closes the SC2 verification loop end-to-end.
#[test]
fn sc2_privacy_warning_via_cli() {
    let proj = tempdir().unwrap();
    let xdg = tempdir().unwrap();
    let emitter = test_emitter_path();
    let emitter_name = emitter.file_name().unwrap().to_str().unwrap();
    write_rule(proj.path(), "e2e-priv", emitter_name);

    // Write project config flipping store_raw_outputs to true.
    // (Must NOT also include retention.* — that's user-only per Phase 1 contract.)
    let proj_lacon = proj.path().join(".lacon");
    std::fs::create_dir_all(&proj_lacon).unwrap();
    std::fs::write(proj_lacon.join("config.yaml"), "store_raw_outputs: true\n").unwrap();

    let marker = proj_lacon.join(".store_raw_outputs_acked");
    assert!(!marker.exists(), "marker absent before first run");

    // First run: warning expected on stderr.
    let first = Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj.path())
        .env("XDG_DATA_HOME", xdg.path())
        .env("XDG_CONFIG_HOME", xdg.path().join("config"))
        .args([
            "run",
            "--rule",
            "e2e-priv",
            "--",
            emitter.to_str().unwrap(),
            "--stdout-lines",
            "1",
        ])
        .assert()
        .success();
    let first_stderr = String::from_utf8_lossy(&first.get_output().stderr).to_string();
    assert!(
        first_stderr.contains("lacon: store_raw_outputs is enabled."),
        "first run: warning expected on stderr; got: {first_stderr}"
    );
    assert!(marker.exists(), "marker created on first run");

    // Second run: warning must NOT re-appear (marker shortcuts the warn path).
    let second = Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj.path())
        .env("XDG_DATA_HOME", xdg.path())
        .env("XDG_CONFIG_HOME", xdg.path().join("config"))
        .args([
            "run",
            "--rule",
            "e2e-priv",
            "--",
            emitter.to_str().unwrap(),
            "--stdout-lines",
            "1",
        ])
        .assert()
        .success();
    let second_stderr = String::from_utf8_lossy(&second.get_output().stderr).to_string();
    assert!(
        !second_stderr.contains("lacon: store_raw_outputs is enabled."),
        "second run: warning must NOT repeat; got: {second_stderr}"
    );
}

/// Write a project rule that DROPS lines beginning with `line ` (so the filtered
/// output visibly differs from the raw output and is non-empty for the
/// test_emitter payload below). The pipeline is `drop_regex` over plain ASCII
/// only — no `strip_ansi`, no control bytes — so `lacon explain`'s safe-view
/// sanitization (WR-01 C0/C1/ESC neutralization) is the identity on this
/// payload, keeping the comparison a true byte-equality of the re-derived
/// filter result.
fn write_drop_line_rule(dir: &std::path::Path, rule_id: &str, command_basename: &str) {
    let rules_dir = dir.join(".lacon").join("rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(
        rules_dir.join(format!("{rule_id}.yaml")),
        format!("id: {rule_id}\nmatch: {{ command: {command_basename} }}\npipeline:\n  - drop_regex: '^line '\n"),
    )
    .unwrap();
}

/// Drop exactly one trailing blank element (the single trailing newline the
/// runtime emits on non-empty output yields one trailing empty segment on the
/// explain side). Mirrors `cli_explain.rs::explain_filtered_column_byte_equals_run_output`
/// — trims only that ONE known blank, staying sensitive to interior-blank drift.
fn trim_one_trailing_blank(v: &[String]) -> &[String] {
    match v.last() {
        Some(last) if last.is_empty() => &v[..v.len() - 1],
        _ => v,
    }
}

/// Phase 7 (D-08, REQ-acceptance-explain-reproducibility): TRUE end-to-end proof
/// that `lacon explain` reproduces a REAL `lacon run` byte-for-byte.
///
/// Unlike `cli_explain.rs::explain_filtered_column_byte_equals_run_output` (which
/// seeds the raw bytes via a hand-written `INSERT INTO raw_outputs`), this test
/// drives an actual `lacon run` with `store_raw_outputs: true`, lets the capture
/// path (Task 1 + Task 2) persist the raw bytes itself, then drives
/// `lacon explain <id>` on the SAME DB and asserts the explain filtered column
/// equals the filtered stdout `lacon run` originally emitted to the model.
///
/// This closes the gap from the v1.0 milestone audit: before this phase,
/// `run.rs` hard-coded `None`, so opting in produced an empty `raw_outputs`
/// table and every real `lacon explain` hit the "no stored raw output" branch.
#[test]
fn explain_reproduces_real_run_byte_for_byte() {
    let proj = tempdir().unwrap();
    let xdg = tempdir().unwrap();
    let emitter = test_emitter_path();
    let emitter_name = emitter.file_name().unwrap().to_str().unwrap();

    // Rule drops `line N` lines; the kept output is the `FAIL error N` lines.
    write_drop_line_rule(proj.path(), "e2e-explain", emitter_name);

    // Opt in: project config enables raw capture (mirrors sc2_privacy_warning_via_cli).
    let proj_lacon = proj.path().join(".lacon");
    std::fs::create_dir_all(&proj_lacon).unwrap();
    std::fs::write(proj_lacon.join("config.yaml"), "store_raw_outputs: true\n").unwrap();

    // ── 1. Run `lacon run` and capture the filtered stdout that reached the model ──
    // Emit 3 `line N` lines (dropped) + 2 `FAIL error N` lines (kept). Exit 0 so
    // the success pipeline (the drop_regex rule) runs. The privacy warning fires
    // on stderr the first time — expected; we assert success, not stderr silence.
    let run = Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj.path())
        .env("XDG_DATA_HOME", xdg.path())
        .env("XDG_CONFIG_HOME", xdg.path().join("config"))
        .args([
            "run",
            "--rule",
            "e2e-explain",
            "--",
            emitter.to_str().unwrap(),
            "--stdout-lines",
            "3",
            "--errors",
            "2",
        ])
        .assert()
        .success();
    let run_stdout = String::from_utf8_lossy(&run.get_output().stdout).to_string();

    // The filtered stdout that reached the model: the runtime joins kept lines
    // with `\n` and adds one trailing `\n` on non-empty output, so `.lines()`
    // yields the kept lines without the trailing blank.
    let run_filtered_lines: Vec<String> = run_stdout.lines().map(str::to_owned).collect();
    // Sanity: the drop actually ran (no `line N`, the `FAIL` lines survived).
    assert!(
        run_filtered_lines
            .iter()
            .any(|l| l.contains("FAIL error 1")),
        "lacon run kept FAIL lines; got: {run_filtered_lines:?}"
    );
    assert!(
        !run_filtered_lines.iter().any(|l| l.starts_with("line ")),
        "lacon run dropped `line N` lines; got: {run_filtered_lines:?}"
    );

    // ── 2. Confirm capture fired: one raw_outputs row + non-NULL raw_output_id ──
    let conn = Connection::open(db_path_under(xdg.path())).unwrap();
    let raw_rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM raw_outputs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(raw_rows, 1, "capture fired: exactly one raw_outputs row");
    let (inv_id, raw_output_id): (i64, Option<i64>) = conn
        .query_row("SELECT id, raw_output_id FROM invocations", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert!(
        raw_output_id.is_some(),
        "invocations.raw_output_id is non-NULL after a captured run"
    );

    // ── 3. Run `lacon explain <id>` on the same DB and extract the filtered column ──
    let explain = Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj.path())
        .env("XDG_DATA_HOME", xdg.path())
        .env("XDG_CONFIG_HOME", xdg.path().join("config"))
        .args(["explain", &inv_id.to_string()])
        .assert()
        .success();
    let explain_stdout = String::from_utf8_lossy(&explain.get_output().stdout).to_string();

    // Render is `{left:<60} | {right}`; the filtered column is the text after the
    // FIRST " | " on each row. Drop the "filtered" header + the dash separator.
    let filtered_column: Vec<String> = explain_stdout
        .lines()
        .filter_map(|line| line.split_once(" | "))
        .map(|(_, right)| right.trim_end().to_owned())
        .filter(|right| right != "filtered" && !right.chars().all(|c| c == '-'))
        .collect();

    // ── 4. Byte-for-byte: explain filtered column == lacon run filtered stdout ──
    let rendered = trim_one_trailing_blank(&filtered_column);
    let expected = trim_one_trailing_blank(&run_filtered_lines);
    assert_eq!(
        rendered, expected,
        "D-08 / REQ-acceptance-explain-reproducibility: explain filtered column must \
         byte-equal the filtered stdout the REAL lacon run emitted\n\
         rendered: {rendered:?}\nexpected: {expected:?}\n\
         explain stdout:\n{explain_stdout}"
    );
}
