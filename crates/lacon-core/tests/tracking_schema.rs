//! Integration tests for tracking::migrations: schema introspection, FK semantics,
//! idempotency, and last_pruned_ts seed.
//!
//! These tests open an in-memory SQLite connection (no filesystem) for speed.
//! Plan 04 owns the WAL + 0700 + filesystem dance; this file isolates DDL.
//!
//! RESEARCH Pitfall #1: PRAGMA foreign_keys=ON is per-connection. Tests that
//! verify CASCADE / SET NULL behaviour MUST set the pragma. Plan 04 will set
//! it inside Tracker::open; until then, tests set it explicitly.

use rusqlite::{params, Connection};

use lacon_core::tracking::migrate;

fn fresh_conn() -> Connection {
    Connection::open_in_memory().expect("open in-memory db")
}

fn fresh_conn_with_fks_on() -> Connection {
    let conn = fresh_conn();
    // RESEARCH Pitfall #1 mitigation in tests. Plan 04 will do this inside Tracker::open.
    conn.set_db_config(rusqlite::config::DbConfig::SQLITE_DBCONFIG_ENABLE_FKEY, true)
        .expect("enable fk pragma");
    conn
}

fn names_of_kind(conn: &Connection, kind: &str) -> Vec<String> {
    let mut stmt = conn
        .prepare(&format!(
            "SELECT name FROM sqlite_master WHERE type='{}' ORDER BY name",
            kind
        ))
        .unwrap();
    let rows: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        // Skip auto-created sqlite_autoindex_* entries when listing indexes.
        .filter(|n: &String| !n.starts_with("sqlite_"))
        .collect();
    rows
}

#[test]
fn migration_creates_all_objects() {
    let mut conn = fresh_conn();
    migrate(&mut conn).expect("migrate ok");

    let tables = names_of_kind(&conn, "table");
    assert!(tables.contains(&"invocations".to_string()), "tables: {:?}", tables);
    assert!(tables.contains(&"raw_outputs".to_string()), "tables: {:?}", tables);
    assert!(tables.contains(&"suspected_regressions".to_string()), "tables: {:?}", tables);
    assert!(tables.contains(&"lacon_meta".to_string()), "tables: {:?}", tables);

    let views = names_of_kind(&conn, "view");
    assert_eq!(
        views,
        vec![
            "v_bypass_rate".to_string(),
            "v_filtered_offenders".to_string(),
            "v_project_savings".to_string(),
            "v_unmatched_offenders".to_string(),
        ],
        "views (sorted)"
    );

    let indexes = names_of_kind(&conn, "index");
    for expected in [
        "idx_inv_cmd",
        "idx_inv_project",
        "idx_inv_rule",
        "idx_inv_ts",
        "idx_raw_created",
        "idx_reg_inv",
    ] {
        assert!(indexes.contains(&expected.to_string()), "missing index {expected}; saw {:?}", indexes);
    }

    let user_version: i32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(user_version, 1, "user_version stamps to 1");
}

#[test]
fn migration_is_idempotent() {
    let mut conn = fresh_conn();
    migrate(&mut conn).expect("first migrate ok");
    migrate(&mut conn).expect("second migrate is a no-op");

    let user_version: i32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(user_version, 1);
}

#[test]
fn last_pruned_ts_seed_present() {
    let mut conn = fresh_conn();
    migrate(&mut conn).expect("migrate ok");

    let v: String = conn
        .query_row(
            "SELECT value FROM lacon_meta WHERE key = 'last_pruned_ts'",
            [],
            |r| r.get(0),
        )
        .expect("seed row exists");
    assert_eq!(v, "0", "first-run-friendly seed");
}

fn insert_invocation(conn: &Connection, id: i64, rule_id: Option<&str>, bypassed: i64) {
    conn.execute(
        "INSERT INTO invocations (
            id, ts, assistant, command_raw, command_normalized,
            rule_id, exit_code, duration_ms,
            raw_stdout_bytes, raw_stderr_bytes, filtered_bytes,
            bypassed
         ) VALUES (?1, ?2, 'claude-code', 'cmd raw', 'cmd', ?3, 0, 1, 100, 0, 50, ?4)",
        params![id, 1_700_000_000_000_i64, rule_id, bypassed],
    )
    .unwrap();
}

#[test]
fn fk_cascade_on_invocation_delete() {
    let mut conn = fresh_conn_with_fks_on();
    migrate(&mut conn).expect("migrate ok");

    insert_invocation(&conn, 1, Some("rule-a"), 0);
    conn.execute(
        "INSERT INTO suspected_regressions (id, invocation_id, reason, detected_ts)
         VALUES (1, 1, 'rerun_with_verbose', ?1)",
        params![1_700_000_000_000_i64],
    )
    .unwrap();

    let before: i64 = conn
        .query_row("SELECT COUNT(*) FROM suspected_regressions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(before, 1);

    conn.execute("DELETE FROM invocations WHERE id = 1", []).unwrap();

    let after: i64 = conn
        .query_row("SELECT COUNT(*) FROM suspected_regressions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        after, 0,
        "CASCADE should have removed the regression row when its parent invocation was deleted"
    );
}

#[test]
fn fk_set_null_on_raw_output_delete() {
    let mut conn = fresh_conn_with_fks_on();
    migrate(&mut conn).expect("migrate ok");

    conn.execute(
        "INSERT INTO raw_outputs (id, invocation_id, stdout, stderr, created_ts)
         VALUES (10, 1, X'', X'', ?1)",
        params![1_700_000_000_000_i64],
    )
    .unwrap();

    conn.execute(
        "INSERT INTO invocations (
            id, ts, assistant, command_raw, command_normalized,
            rule_id, exit_code, duration_ms,
            raw_stdout_bytes, raw_stderr_bytes, filtered_bytes,
            bypassed, raw_output_id
         ) VALUES (1, ?1, 'claude-code', 'cmd raw', 'cmd', 'rule-a', 0, 1, 0, 0, 0, 0, 10)",
        params![1_700_000_000_000_i64],
    )
    .unwrap();

    conn.execute("DELETE FROM raw_outputs WHERE id = 10", []).unwrap();

    let r: Option<i64> = conn
        .query_row("SELECT raw_output_id FROM invocations WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(r, None, "SET NULL should have nulled raw_output_id");
}

#[test]
fn fk_silent_no_op_without_pragma() {
    // Negative test: documents WHY Plan 04 must set foreign_keys=ON per connection
    // (RESEARCH Pitfall #1).
    //
    // [Rule 1 deviation] libsqlite3-sys 0.37 (the build under rusqlite 0.39 +
    // bundled feature) compiles SQLite with `-DSQLITE_DEFAULT_FOREIGN_KEYS=1`
    // (build.rs:126), so a freshly-opened bundled connection has foreign_keys=ON
    // by default — the opposite of stock SQLite (`PRAGMA foreign_keys` defaults
    // to OFF per sqlite.org/foreignkeys.html). To prove the underlying SQL
    // contract still depends on the pragma, this test EXPLICITLY DISABLES
    // foreign_keys, then verifies the CASCADE silently no-ops.
    //
    // Plan 04's contract therefore stands: set `foreign_keys=ON` per connection
    // defensively, in case the build flips to non-bundled SQLite or
    // libsqlite3-sys drops the default in a future release.
    let mut conn = fresh_conn();
    conn.pragma_update(None, "foreign_keys", false)
        .expect("explicitly disable fk pragma");
    migrate(&mut conn).expect("migrate ok");

    insert_invocation(&conn, 1, Some("rule-a"), 0);
    conn.execute(
        "INSERT INTO suspected_regressions (id, invocation_id, reason, detected_ts)
         VALUES (1, 1, 'rerun_with_verbose', ?1)",
        params![1_700_000_000_000_i64],
    )
    .unwrap();
    conn.execute("DELETE FROM invocations WHERE id = 1", []).unwrap();

    let after: i64 = conn
        .query_row("SELECT COUNT(*) FROM suspected_regressions", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        after, 1,
        "With foreign_keys explicitly OFF the CASCADE is a silent no-op — Plan 04 MUST set the pragma defensively"
    );
}
