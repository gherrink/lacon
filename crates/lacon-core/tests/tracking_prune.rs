//! Integration tests for tracking::prune::prune_if_due:
//! - prune fires after 24h via clock injection
//! - throttled within 24h (no DELETE, no last_pruned_ts update)
//! - retention windows applied per-table
//! - first-run semantics (last_pruned_ts seeded to '0' → prune fires immediately)

use rusqlite::{params, Connection};

use lacon_core::config::Retention;
use lacon_core::tracking::migrate;
use lacon_core::tracking::prune::prune_if_due;

const ONE_DAY_MS: i64 = 86_400_000;
const FIXED_NOW_MS: i64 = 1_700_000_000_000; // ~2023-11-14 — stable test clock

fn migrated_conn() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    // No FK pragma here — prune doesn't depend on FK semantics, and the
    // negative test in tracking_schema.rs already covers the pragma case.
    let mut conn = conn;
    migrate(&mut conn).expect("migrate ok");
    conn
}

fn default_retention() -> Retention {
    Retention {
        invocations_days: 30,
        raw_outputs_days: 3,
    }
}

fn last_pruned_ts(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT value FROM lacon_meta WHERE key = 'last_pruned_ts'",
        [],
        |r| r.get::<_, String>(0),
    )
    .unwrap()
    .parse()
    .unwrap()
}

fn insert_inv(conn: &Connection, id: i64, ts: i64) {
    conn.execute(
        "INSERT INTO invocations (
            id, ts, assistant, command_raw, command_normalized,
            rule_id, exit_code, duration_ms,
            raw_stdout_bytes, raw_stderr_bytes, filtered_bytes
         ) VALUES (?1, ?2, 'claude-code', 'cmd raw', 'cmd', NULL, 0, 1, 0, 0, 0)",
        params![id, ts],
    )
    .unwrap();
}

fn insert_raw(conn: &Connection, id: i64, created_ts: i64) {
    conn.execute(
        "INSERT INTO raw_outputs (id, invocation_id, stdout, stderr, created_ts)
         VALUES (?1, 0, X'', X'', ?2)",
        params![id, created_ts],
    )
    .unwrap();
}

fn insert_reg(conn: &Connection, id: i64, invocation_id: i64, detected_ts: i64) {
    // suspected_regressions FK requires existing invocation row; we use a real id
    // in the small set of tests that touch this table.
    conn.execute(
        "INSERT INTO suspected_regressions (id, invocation_id, reason, detected_ts)
         VALUES (?1, ?2, 'rerun_with_verbose', ?3)",
        params![id, invocation_id, detected_ts],
    )
    .unwrap();
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn prune_fires_on_first_run_with_zero_seed() {
    let conn = migrated_conn();
    // Fresh DB: lacon_meta.last_pruned_ts seeded to '0' by migration.
    // (FIXED_NOW_MS - 0) > 24h → prune fires.
    prune_if_due(&conn, &default_retention(), FIXED_NOW_MS as u64).expect("ok");
    assert_eq!(last_pruned_ts(&conn), FIXED_NOW_MS);
}

#[test]
fn prune_deletes_old_rows_only() {
    let conn = migrated_conn();
    let now = FIXED_NOW_MS;
    let retention = default_retention();

    // invocations: row at -31d (delete) and -1d (keep)
    insert_inv(&conn, 1, now - 31 * ONE_DAY_MS);
    insert_inv(&conn, 2, now - 1 * ONE_DAY_MS);

    // raw_outputs: row at -4d (delete) and -1d (keep)
    insert_raw(&conn, 10, now - 4 * ONE_DAY_MS);
    insert_raw(&conn, 11, now - 1 * ONE_DAY_MS);

    // suspected_regressions: row at -31d (delete) and -1d (keep)
    insert_reg(&conn, 100, 2, now - 31 * ONE_DAY_MS);
    insert_reg(&conn, 101, 2, now - 1 * ONE_DAY_MS);

    prune_if_due(&conn, &retention, now as u64).expect("ok");

    // Old invocation gone (id=1); recent kept (id=2).
    assert_eq!(count(&conn, "invocations"), 1, "old invocation pruned");
    let surviving_inv: i64 = conn
        .query_row("SELECT id FROM invocations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(surviving_inv, 2);

    // Old raw_output gone (id=10); recent kept (id=11).
    assert_eq!(count(&conn, "raw_outputs"), 1, "old raw_output pruned");

    // Old regression gone (id=100); recent kept (id=101).
    assert_eq!(count(&conn, "suspected_regressions"), 1, "old regression pruned");
}

#[test]
fn prune_throttled_within_24h() {
    let conn = migrated_conn();
    let now = FIXED_NOW_MS;
    let retention = default_retention();

    // Set last_pruned_ts to 1h ago (within the 24h window).
    let one_hour_ago = (now - 3_600_000).to_string();
    conn.execute(
        "UPDATE lacon_meta SET value = ?1 WHERE key = 'last_pruned_ts'",
        params![one_hour_ago],
    )
    .unwrap();

    // Insert an OLD invocation that would be deleted if prune ran.
    insert_inv(&conn, 1, now - 31 * ONE_DAY_MS);

    prune_if_due(&conn, &retention, now as u64).expect("ok");

    // Throttle hit → prune skipped → old row still present.
    assert_eq!(count(&conn, "invocations"), 1, "throttled: no DELETE");

    // last_pruned_ts unchanged (still 1h ago, not bumped to now).
    assert_eq!(last_pruned_ts(&conn), now - 3_600_000, "throttled: ts not updated");
}

#[test]
fn prune_runs_after_exactly_24h() {
    let conn = migrated_conn();
    let now = FIXED_NOW_MS;
    let retention = default_retention();

    // Set last_pruned_ts to exactly 24h ago.
    let exactly_24h_ago = (now - 86_400_000).to_string();
    conn.execute(
        "UPDATE lacon_meta SET value = ?1 WHERE key = 'last_pruned_ts'",
        params![exactly_24h_ago],
    )
    .unwrap();

    insert_inv(&conn, 1, now - 31 * ONE_DAY_MS);

    prune_if_due(&conn, &retention, now as u64).expect("ok");

    // 24h is the boundary — the impl uses `< PRUNE_THROTTLE_MS` for the gate,
    // so exactly-24h-elapsed satisfies the gate (not less than). Old row pruned.
    assert_eq!(count(&conn, "invocations"), 0, "24h elapsed → prune fires");
    assert_eq!(last_pruned_ts(&conn), now);
}

#[test]
fn prune_with_corrupted_last_pruned_ts_treats_as_zero() {
    let conn = migrated_conn();
    // Corrupt the seed to a non-numeric string.
    conn.execute(
        "UPDATE lacon_meta SET value = 'not-a-number' WHERE key = 'last_pruned_ts'",
        [],
    )
    .unwrap();

    prune_if_due(&conn, &default_retention(), FIXED_NOW_MS as u64).expect("ok");
    // Garbage parsed as 0 → prune fires → ts updates.
    assert_eq!(last_pruned_ts(&conn), FIXED_NOW_MS);
}
