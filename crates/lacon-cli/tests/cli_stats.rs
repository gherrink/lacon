//! Phase 4 Plan 03 — black-box CLI tests for `lacon stats`.
//!
//! Isolation: each test points the binary at a tempdir via `XDG_DATA_HOME`
//! (so it reads `<tempdir>/lacon/history.db`) and seeds that DB directly with
//! the dev-only `rusqlite` dependency (Cargo.toml `[dev-dependencies]`). No
//! runtime `rusqlite` dep is introduced — all production SQL lives in
//! `lacon-core::tracking::query`.
//!
//! Coverage:
//!   (a) seeded DB → the four section headers + at least one offender row
//!   (b) `--since`/`--project`/`--rule` narrow the output (a row visible without
//!       the filter is absent with a narrowing filter)
//!   (c) empty DB (no history.db) → "no data yet" present, exit success (D-03)

use std::path::Path;

use assert_cmd::Command;
use rusqlite::Connection;
use tempfile::tempdir;

/// The byte-exact schema DDL (subset sufficient for the read path: the two
/// base tables + the four views). Mirrors
/// `crates/lacon-core/src/tracking/migrations/0001_initial.sql`. Seeding tests
/// via the dev-only rusqlite is the established lacon-cli pattern (tracking_e2e.rs).
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
DROP VIEW IF EXISTS v_unmatched_offenders;
CREATE VIEW v_unmatched_offenders AS
SELECT command_normalized,
       COUNT(*) AS runs,
       SUM(raw_stdout_bytes + raw_stderr_bytes) AS total_raw_bytes
FROM invocations
WHERE rule_id IS NULL AND bypassed = 0
GROUP BY command_normalized
ORDER BY total_raw_bytes DESC;
DROP VIEW IF EXISTS v_filtered_offenders;
CREATE VIEW v_filtered_offenders AS
SELECT command_normalized, rule_id,
       COUNT(*) AS runs,
       SUM(filtered_bytes) AS total_filtered_bytes,
       AVG(CAST(filtered_bytes AS REAL) /
           NULLIF(raw_stdout_bytes + raw_stderr_bytes, 0)) AS avg_keep_ratio
FROM invocations
WHERE rule_id IS NOT NULL AND bypassed = 0
GROUP BY command_normalized, rule_id
ORDER BY total_filtered_bytes DESC;
DROP VIEW IF EXISTS v_bypass_rate;
CREATE VIEW v_bypass_rate AS
SELECT rule_id,
       COUNT(*) AS total,
       SUM(bypassed) AS bypassed,
       CAST(SUM(bypassed) AS REAL) / COUNT(*) AS bypass_rate
FROM invocations
WHERE rule_id IS NOT NULL
GROUP BY rule_id
HAVING COUNT(*) > 5
ORDER BY bypass_rate DESC;
DROP VIEW IF EXISTS v_project_savings;
CREATE VIEW v_project_savings AS
SELECT project_path,
       COUNT(*) AS total_runs,
       SUM(raw_stdout_bytes + raw_stderr_bytes) AS raw_total,
       SUM(filtered_bytes) AS filtered_total,
       SUM(raw_stdout_bytes + raw_stderr_bytes - filtered_bytes) AS bytes_saved
FROM invocations
WHERE bypassed = 0
GROUP BY project_path
ORDER BY bytes_saved DESC;
";

fn db_path_under(xdg_data_home: &Path) -> std::path::PathBuf {
    xdg_data_home.join("lacon").join("history.db")
}

/// Create the schema in a fresh history.db under `<xdg>/lacon/`.
fn init_db(xdg_data_home: &Path) -> Connection {
    let db = db_path_under(xdg_data_home);
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(SCHEMA_DDL).unwrap();
    conn
}

/// Insert one invocation row. `rule_id = None` → unmatched offender.
#[allow(clippy::too_many_arguments)]
fn insert_invocation(
    conn: &Connection,
    ts: i64,
    project_path: &str,
    command_normalized: &str,
    rule_id: Option<&str>,
    exit_code: i64,
    raw_bytes: i64,
    filtered_bytes: i64,
    bypassed: i64,
) {
    conn.execute(
        "INSERT INTO invocations
          (ts, assistant, project_path, command_raw, command_normalized, rule_id,
           rule_source, exit_code, duration_ms, raw_stdout_bytes, raw_stderr_bytes,
           filtered_bytes, bypassed, rewritten, truncated_by_max_bytes, raw_output_id)
         VALUES (?1,'claude-code',?2,?3,?4,?5,?6,?7,5,?8,0,?9,?10,0,0,NULL)",
        rusqlite::params![
            ts,
            project_path,
            command_normalized,
            command_normalized,
            rule_id,
            rule_id.map(|_| "project"),
            exit_code,
            raw_bytes,
            filtered_bytes,
            bypassed,
        ],
    )
    .unwrap();
}

fn lacon(xdg: &Path) -> Command {
    let mut cmd = Command::cargo_bin("lacon").unwrap();
    cmd.env("XDG_DATA_HOME", xdg)
        .env("XDG_CONFIG_HOME", xdg.join("config"));
    cmd
}

#[test]
fn stats_seeded_db_shows_four_sections_and_offender_rows() {
    let xdg = tempdir().unwrap();
    let now_ms = 1_700_000_000_000_i64;
    let conn = init_db(xdg.path());
    // Unmatched offender (no rule).
    insert_invocation(&conn, now_ms, "/p/a", "make", None, 0, 5000, 5000, 0);
    // Filtered offender (matched rule).
    insert_invocation(&conn, now_ms, "/p/a", "cargo", Some("cargo-rule"), 0, 8000, 1200, 0);

    let assert = lacon(xdg.path()).arg("stats").assert().success();
    let out = assert.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(stdout.contains("Unmatched offenders"), "missing section header; got:\n{stdout}");
    assert!(stdout.contains("Filtered offenders"), "missing section header; got:\n{stdout}");
    assert!(stdout.contains("Bypass rates"), "missing section header; got:\n{stdout}");
    assert!(stdout.contains("Per-project savings"), "missing section header; got:\n{stdout}");
    assert!(stdout.contains("make"), "expected unmatched offender row; got:\n{stdout}");
    assert!(stdout.contains("cargo"), "expected filtered offender row; got:\n{stdout}");
}

#[test]
fn stats_empty_db_prints_no_data_yet_and_succeeds() {
    let xdg = tempdir().unwrap();
    // No history.db created at all → fresh-machine state (D-03).
    let assert = lacon(xdg.path()).arg("stats").assert().success();
    let out = assert.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.to_lowercase().contains("no data yet"),
        "expected 'no data yet' on a fresh machine; got:\n{stdout}"
    );
}

#[test]
fn stats_project_filter_narrows_output() {
    let xdg = tempdir().unwrap();
    let now_ms = 1_700_000_000_000_i64;
    let conn = init_db(xdg.path());
    // Two unmatched offenders in different projects.
    insert_invocation(&conn, now_ms, "/p/keep", "ninja", None, 0, 4000, 4000, 0);
    insert_invocation(&conn, now_ms, "/p/drop", "scons", None, 0, 4000, 4000, 0);

    // Without filter: both visible.
    let all = lacon(xdg.path()).arg("stats").assert().success();
    let all_out = String::from_utf8_lossy(&all.get_output().stdout).to_string();
    assert!(all_out.contains("ninja") && all_out.contains("scons"), "both present unfiltered; got:\n{all_out}");

    // With --project /p/keep: only ninja's project remains.
    let filtered = lacon(xdg.path())
        .args(["stats", "--project", "/p/keep"])
        .assert()
        .success();
    let f_out = String::from_utf8_lossy(&filtered.get_output().stdout).to_string();
    assert!(f_out.contains("ninja"), "kept project's command present; got:\n{f_out}");
    assert!(!f_out.contains("scons"), "other project's command must be filtered out; got:\n{f_out}");
}

#[test]
fn stats_since_filter_narrows_output() {
    let xdg = tempdir().unwrap();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let conn = init_db(xdg.path());
    // One recent, one ancient (older than 7 days).
    insert_invocation(&conn, now_ms, "/p/x", "recentcmd", None, 0, 3000, 3000, 0);
    let ten_days_ago = now_ms - 10 * 86_400_000;
    insert_invocation(&conn, ten_days_ago, "/p/x", "ancientcmd", None, 0, 3000, 3000, 0);

    let filtered = lacon(xdg.path())
        .args(["stats", "--since", "7d"])
        .assert()
        .success();
    let f_out = String::from_utf8_lossy(&filtered.get_output().stdout).to_string();
    assert!(f_out.contains("recentcmd"), "recent command within window; got:\n{f_out}");
    assert!(!f_out.contains("ancientcmd"), "command older than 7d must be excluded; got:\n{f_out}");
}

#[test]
fn stats_invalid_since_errors_nonzero_no_panic() {
    let xdg = tempdir().unwrap();
    init_db(xdg.path());
    let assert = lacon(xdg.path())
        .args(["stats", "--since", "7x"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.to_lowercase().contains("since"),
        "expected a clear --since error; got:\n{stderr}"
    );
}
