//! Phase 4 Plan 03 — black-box CLI tests for `lacon explain <id>`.
//!
//! Isolation: each test points the binary at a tempdir via `XDG_DATA_HOME`
//! (so it reads `<tempdir>/lacon/history.db`) and runs the binary with
//! `current_dir(proj)` so the project `.lacon/rules/` layer resolves the rule.
//! DBs are seeded directly with the dev-only `rusqlite` dependency — no runtime
//! rusqlite dep is introduced.
//!
//! Coverage:
//!   (a) seeded invocation WITH stored raw_output → side-by-side rendered, exit 0
//!   (b) invocation with `raw_output_id` NULL → error mentions `store_raw_outputs`,
//!       exit failure (SC2 required failure path, D-05 step 3)
//!   (c) non-numeric id (`lacon explain abc`) → error, exit failure, no panic
//!   (d) unknown id → "no tracked invocations found", exit failure

use std::path::Path;

use assert_cmd::Command;
use rusqlite::Connection;
use tempfile::tempdir;

const SCHEMA_DDL: &str = "
CREATE TABLE raw_outputs (
  id INTEGER PRIMARY KEY,
  invocation_id INTEGER NOT NULL,
  stdout BLOB,
  stderr BLOB,
  created_ts INTEGER NOT NULL
);
CREATE TABLE invocations (
  id INTEGER PRIMARY KEY,
  ts INTEGER NOT NULL,
  assistant TEXT NOT NULL,
  session_id TEXT,
  project_path TEXT,
  command_raw TEXT NOT NULL,
  command_normalized TEXT NOT NULL,
  rule_id TEXT,
  rule_source TEXT,
  exit_code INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL,
  raw_stdout_bytes INTEGER NOT NULL,
  raw_stderr_bytes INTEGER NOT NULL,
  filtered_bytes INTEGER NOT NULL,
  bypassed INTEGER NOT NULL DEFAULT 0,
  rewritten INTEGER NOT NULL DEFAULT 0,
  truncated_by_max_bytes INTEGER NOT NULL DEFAULT 0,
  raw_output_id INTEGER REFERENCES raw_outputs(id) ON DELETE SET NULL
);
";

fn db_path_under(xdg_data_home: &Path) -> std::path::PathBuf {
    xdg_data_home.join("lacon").join("history.db")
}

fn init_db(xdg_data_home: &Path) -> Connection {
    let db = db_path_under(xdg_data_home);
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(SCHEMA_DDL).unwrap();
    conn
}

/// Write a project rule that drops lines containing "noise" (so the filtered
/// column visibly differs from the raw column).
fn write_drop_noise_rule(proj: &Path, rule_id: &str) {
    let rules_dir = proj.join(".lacon").join("rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(
        rules_dir.join(format!("{rule_id}.yaml")),
        format!(
            "id: {rule_id}\nmatch: {{ command: cargo }}\npipeline:\n  - drop_regex: noise\n",
        ),
    )
    .unwrap();
}

fn lacon(xdg: &Path, proj: &Path) -> Command {
    let mut cmd = Command::cargo_bin("lacon").unwrap();
    cmd.current_dir(proj)
        .env("XDG_DATA_HOME", xdg)
        .env("XDG_CONFIG_HOME", xdg.join("config"));
    cmd
}

#[test]
fn explain_with_stored_raw_renders_side_by_side() {
    let xdg = tempdir().unwrap();
    let proj = tempdir().unwrap();
    write_drop_noise_rule(proj.path(), "cargo-rule");
    let conn = init_db(xdg.path());

    let raw = b"kept line one\nnoise dropped line\nkept line two\n";
    conn.execute(
        "INSERT INTO raw_outputs (id, invocation_id, stdout, stderr, created_ts)
         VALUES (1, 1, ?1, X'', 1700000000000)",
        rusqlite::params![raw.to_vec()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO invocations
          (id, ts, assistant, project_path, command_raw, command_normalized, rule_id,
           rule_source, exit_code, duration_ms, raw_stdout_bytes, raw_stderr_bytes,
           filtered_bytes, bypassed, rewritten, truncated_by_max_bytes, raw_output_id)
         VALUES (1, 1700000000000, 'claude-code', ?1, 'cargo build', 'cargo',
                 'cargo-rule', 'project', 0, 5, ?2, 0, 20, 0, 0, 0, 1)",
        rusqlite::params![proj.path().to_string_lossy(), raw.len() as i64],
    )
    .unwrap();

    let assert = lacon(xdg.path(), proj.path())
        .args(["explain", "1"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    // Raw column shows all lines; filtered column drops the noise line.
    assert!(stdout.contains("kept line one"), "raw lines present; got:\n{stdout}");
    assert!(stdout.contains("noise dropped line"), "raw column keeps the dropped line; got:\n{stdout}");
}

#[test]
fn explain_raw_output_id_null_errors_with_store_raw_outputs_hint() {
    let xdg = tempdir().unwrap();
    let proj = tempdir().unwrap();
    write_drop_noise_rule(proj.path(), "cargo-rule");
    let conn = init_db(xdg.path());

    conn.execute(
        "INSERT INTO invocations
          (id, ts, assistant, project_path, command_raw, command_normalized, rule_id,
           rule_source, exit_code, duration_ms, raw_stdout_bytes, raw_stderr_bytes,
           filtered_bytes, bypassed, rewritten, truncated_by_max_bytes, raw_output_id)
         VALUES (7, 1700000000000, 'claude-code', ?1, 'cargo build', 'cargo',
                 'cargo-rule', 'project', 0, 5, 100, 0, 20, 0, 0, 0, NULL)",
        rusqlite::params![proj.path().to_string_lossy()],
    )
    .unwrap();

    let assert = lacon(xdg.path(), proj.path())
        .args(["explain", "7"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains("store_raw_outputs"),
        "SC2: NULL raw_output_id must point at store_raw_outputs; got:\n{stderr}"
    );
}

#[test]
fn explain_non_numeric_id_errors_no_panic() {
    let xdg = tempdir().unwrap();
    let proj = tempdir().unwrap();
    init_db(xdg.path());

    let assert = lacon(xdg.path(), proj.path())
        .args(["explain", "abc"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.to_lowercase().contains("invalid invocation id"),
        "non-numeric id must error clearly; got:\n{stderr}"
    );
}

#[test]
fn explain_unknown_id_reports_not_found() {
    let xdg = tempdir().unwrap();
    let proj = tempdir().unwrap();
    init_db(xdg.path());

    let assert = lacon(xdg.path(), proj.path())
        .args(["explain", "999"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.to_lowercase().contains("no tracked invocations found"),
        "unknown id must report not-found; got:\n{stderr}"
    );
}

#[test]
fn explain_on_fresh_machine_reports_not_found() {
    let xdg = tempdir().unwrap();
    let proj = tempdir().unwrap();
    // No history.db at all (D-03).
    let assert = lacon(xdg.path(), proj.path())
        .args(["explain", "1"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.to_lowercase().contains("no tracked invocations found"),
        "fresh machine explain must report not-found; got:\n{stderr}"
    );
}
