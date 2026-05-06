//! Integration tests for the 4 views — proves SC3 (views queryable, non-error
//! result sets after a session of `lacon run` invocations).
//!
//! RESEARCH Pitfall #11: v_bypass_rate has `HAVING COUNT(*) > 5` so an empty
//! result set IS the expected behaviour until ≥6 rows for a rule_id exist.
//! Tests assert "queryable, no error" rather than "row count" for the empty case.

use rusqlite::{params, Connection};

use lacon_core::tracking::migrate;

fn migrated_conn() -> Connection {
    let mut conn = Connection::open_in_memory().expect("open in-memory db");
    migrate(&mut conn).expect("migrate ok");
    conn
}

fn insert_invocation_full(
    conn: &Connection,
    id: i64,
    rule_id: Option<&str>,
    bypassed: i64,
    raw_stdout_bytes: i64,
    raw_stderr_bytes: i64,
    filtered_bytes: i64,
    cmd_normalized: &str,
    project_path: Option<&str>,
) {
    conn.execute(
        "INSERT INTO invocations (
            id, ts, assistant, command_raw, command_normalized,
            rule_id, exit_code, duration_ms,
            raw_stdout_bytes, raw_stderr_bytes, filtered_bytes,
            bypassed, project_path
         ) VALUES (?1, ?2, 'claude-code', 'cmd raw', ?3,
                   ?4, 0, 1, ?5, ?6, ?7, ?8, ?9)",
        params![
            id,
            1_700_000_000_000_i64,
            cmd_normalized,
            rule_id,
            raw_stdout_bytes,
            raw_stderr_bytes,
            filtered_bytes,
            bypassed,
            project_path,
        ],
    )
    .unwrap();
}

#[test]
fn views_return_rows_empty_db() {
    let conn = migrated_conn();
    // SC3 wording: "non-error result sets when queried via sqlite3 after a session
    // of lacon run invocations". Empty IS non-error. We assert SELECT succeeds.
    for view in [
        "v_unmatched_offenders",
        "v_filtered_offenders",
        "v_bypass_rate",
        "v_project_savings",
    ] {
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM {view}"))
            .unwrap_or_else(|e| panic!("prepare {view} failed: {e}"));
        let count: usize = stmt
            .query([])
            .unwrap_or_else(|e| panic!("query {view} failed: {e}"))
            .mapped(|_r| Ok(()))
            .count();
        // Empty result set is valid for an empty DB.
        assert_eq!(count, 0, "empty DB → 0 rows in {view}");
    }
}

#[test]
fn v_bypass_rate_below_threshold_returns_empty() {
    let conn = migrated_conn();
    for i in 1..=5 {
        insert_invocation_full(&conn, i, Some("rule-a"), 0, 100, 0, 50, "cmd-a", Some("/proj"));
    }
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM v_bypass_rate", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0, "5 rows < HAVING COUNT(*) > 5 threshold → empty");
}

#[test]
fn v_bypass_rate_above_threshold_returns_one_row() {
    let conn = migrated_conn();
    // 5 non-bypassed + 1 bypassed = 6 total → COUNT(*) > 5 → row appears.
    for i in 1..=5 {
        insert_invocation_full(&conn, i, Some("rule-a"), 0, 100, 0, 50, "cmd-a", Some("/proj"));
    }
    insert_invocation_full(&conn, 6, Some("rule-a"), 1, 100, 0, 100, "cmd-a", Some("/proj"));

    let mut stmt = conn
        .prepare("SELECT rule_id, total, bypassed, bypass_rate FROM v_bypass_rate")
        .unwrap();
    let mut rows = stmt.query([]).unwrap();
    let row = rows.next().unwrap().expect("at least one row");
    let rule_id: String = row.get(0).unwrap();
    let total: i64 = row.get(1).unwrap();
    let bypassed: i64 = row.get(2).unwrap();
    let rate: f64 = row.get(3).unwrap();
    assert_eq!(rule_id, "rule-a");
    assert_eq!(total, 6);
    assert_eq!(bypassed, 1);
    assert!((rate - (1.0 / 6.0)).abs() < 1e-9, "bypass_rate ≈ 1/6 = {rate}");
    assert!(rows.next().unwrap().is_none(), "exactly one row");
}

#[test]
fn v_unmatched_offenders_groups_by_command_and_orders_desc() {
    let conn = migrated_conn();
    // 3 rows for cmd-a (NULL rule), 1 row for cmd-b (NULL rule)
    for i in 1..=3 {
        insert_invocation_full(&conn, i, None, 0, 1000, 0, 500, "cmd-a", Some("/proj"));
    }
    insert_invocation_full(&conn, 4, None, 0, 100, 0, 100, "cmd-b", Some("/proj"));
    // 1 row with a matched rule — must NOT appear in the unmatched view.
    insert_invocation_full(&conn, 5, Some("rule-x"), 0, 9999, 0, 0, "cmd-c", Some("/proj"));

    let mut stmt = conn
        .prepare("SELECT command_normalized, runs, total_raw_bytes FROM v_unmatched_offenders")
        .unwrap();
    let rows: Vec<(String, i64, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    assert_eq!(rows.len(), 2, "two distinct unmatched commands");
    assert_eq!(rows[0].0, "cmd-a", "ORDER BY total_raw_bytes DESC");
    assert_eq!(rows[0].1, 3);
    assert_eq!(rows[0].2, 3000);
    assert_eq!(rows[1].0, "cmd-b");
}

#[test]
fn v_project_savings_excludes_bypassed_rows() {
    let conn = migrated_conn();
    insert_invocation_full(&conn, 1, Some("rule-a"), 0, 1000, 0, 100, "cmd-a", Some("/projA"));
    insert_invocation_full(&conn, 2, Some("rule-a"), 1, 1000, 0, 1000, "cmd-a", Some("/projA"));
    insert_invocation_full(&conn, 3, Some("rule-a"), 0, 500, 0, 100, "cmd-a", Some("/projB"));

    let mut stmt = conn
        .prepare("SELECT project_path, total_runs, raw_total, filtered_total, bytes_saved
                  FROM v_project_savings")
        .unwrap();
    let rows: Vec<(String, i64, i64, i64, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))
        .unwrap()
        .map(|r| r.unwrap())
        .collect();
    // /projA: 1 non-bypassed row (raw=1000, filtered=100, saved=900)
    // /projB: 1 non-bypassed row (raw=500,  filtered=100, saved=400)
    // Bypassed row in /projA is excluded.
    assert_eq!(rows.len(), 2, "two projects after excluding bypassed");
    let proj_a = rows.iter().find(|r| r.0 == "/projA").expect("/projA present");
    assert_eq!(proj_a.1, 1, "bypassed row excluded → 1 run for /projA");
    assert_eq!(proj_a.4, 900);
}
