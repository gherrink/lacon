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
    assert_eq!(command_normalized, emitter_name, "command_normalized = basename");
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
    std::fs::write(
        proj_lacon.join("config.yaml"),
        "store_raw_outputs: true\n",
    )
    .unwrap();

    let marker = proj_lacon.join(".store_raw_outputs_acked");
    assert!(!marker.exists(), "marker absent before first run");

    // First run: warning expected on stderr.
    let first = Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(proj.path())
        .env("XDG_DATA_HOME", xdg.path())
        .env("XDG_CONFIG_HOME", xdg.path().join("config"))
        .args([
            "run", "--rule", "e2e-priv", "--",
            emitter.to_str().unwrap(), "--stdout-lines", "1",
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
            "run", "--rule", "e2e-priv", "--",
            emitter.to_str().unwrap(), "--stdout-lines", "1",
        ])
        .assert()
        .success();
    let second_stderr = String::from_utf8_lossy(&second.get_output().stderr).to_string();
    assert!(
        !second_stderr.contains("lacon: store_raw_outputs is enabled."),
        "second run: warning must NOT repeat; got: {second_stderr}"
    );
}
