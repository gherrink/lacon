//! Integration tests for the Phase 4 tracking READ path (Plan 04-01):
//! - `tracking::open_readonly` non-mutating open invariant (D-02, T-04-02)
//! - `tracking::query` view readers, D-09 filtered re-queries, explain lookups
//!
//! Seeds a realistic `history.db` via the WRITE path (`Tracker::open` +
//! `Tracker::record`), drops the writer, then exercises the read API through a
//! read-only connection. Mirrors the seeding harness in tracking_record.rs.

use std::path::{Path, PathBuf};

use lacon_core::config::Retention;
use lacon_core::runtime::{ByteCounts, InvocationMeta};
use lacon_core::rules::loader::RuleSource;
use lacon_core::tracking::{self, RawOutput, Tracker};

const FIXED_NOW_MS: u64 = 1_700_000_000_000;
const ONE_DAY_MS: u64 = 86_400_000;

fn setup_db_path() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("data").join("lacon").join("history.db");
    (tmp, db_path)
}

fn default_retention() -> Retention {
    Retention {
        invocations_days: 30,
        raw_outputs_days: 3,
    }
}

/// Build an InvocationMeta with the knobs the read tests vary.
#[allow(clippy::too_many_arguments)]
fn meta(
    ts_unix_ms: u64,
    command_normalized: &str,
    rule: Option<RuleSource>,
    rule_id: Option<&str>,
    project_path: Option<&str>,
    exit_code: i32,
    bypassed: bool,
    raw_stdout_bytes: usize,
) -> InvocationMeta {
    InvocationMeta {
        ts_unix_ms,
        rule_id: rule_id.map(|s| s.to_string()),
        rule_source: rule,
        command_raw: command_normalized.to_string(),
        argv: command_normalized.split(' ').map(|s| s.to_string()).collect(),
        exit_code,
        duration_ms: 42,
        byte_counts: ByteCounts {
            raw_stdout_bytes,
            raw_stderr_bytes: 0,
            filtered_bytes: raw_stdout_bytes / 2,
        },
        bypassed,
        rewritten: false,
        truncated_by_max_bytes: false,
        assistant: "claude-code".to_string(),
        session_id: Some("sess-xyz".to_string()),
        project_path: project_path.map(PathBuf::from),
        command_normalized: command_normalized.to_string(),
        raw_output_id: None,
    }
}

fn inv_count(conn: &rusqlite::Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM invocations", [], |r| r.get(0))
        .unwrap()
}

// ---------------------------------------------------------------------------
// Task 2: open_readonly invariant (D-02, T-04-02)
// ---------------------------------------------------------------------------

#[test]
fn open_readonly_does_not_mutate_invocations_count() {
    let (_tmp, db_path) = setup_db_path();

    // Seed two rows via the write path, then drop the writer.
    {
        let tracker =
            Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS).expect("open ok");
        tracker
            .record(
                &meta(
                    FIXED_NOW_MS,
                    "pnpm install",
                    None,
                    None,
                    Some("/proj-a"),
                    0,
                    false,
                    1000,
                ),
                None,
                None,
                None,
                false,
                false,
            )
            .expect("record 1");
        tracker
            .record(
                &meta(
                    FIXED_NOW_MS,
                    "cargo build",
                    Some(RuleSource::Project),
                    Some("rule-cargo"),
                    Some("/proj-a"),
                    0,
                    false,
                    2000,
                ),
                None,
                None,
                None,
                false,
                false,
            )
            .expect("record 2");
    }

    // Read-only open must not migrate/prune/INSERT. Record count before/after.
    let conn = tracking::open_readonly(&db_path).expect("open_readonly ok");
    let before = inv_count(&conn);
    // Run a SELECT to make sure the handle is usable.
    let _ = tracking::query::unmatched_offenders(&conn).expect("read view");
    let after = inv_count(&conn);

    assert_eq!(before, 2, "two seeded rows visible");
    assert_eq!(after, before, "open_readonly + SELECT did not INSERT");
}

#[test]
fn open_readonly_errors_on_missing_db() {
    let (_tmp, db_path) = setup_db_path();
    // db_path was never created (no Tracker::open). open_readonly must NOT
    // create the file (D-03: caller checks existence) and must return Err.
    assert!(!db_path.exists(), "precondition: db absent");
    let res = tracking::open_readonly(&db_path);
    assert!(res.is_err(), "open_readonly on absent db returns Err");
    assert!(!db_path.exists(), "open_readonly never creates the file");
}

// ---------------------------------------------------------------------------
// Task 3/4: read API — seed a realistic DB and assert
// ---------------------------------------------------------------------------

/// Seed a DB spanning matched/unmatched, multiple projects, old/new ts,
/// exit 0/non-zero, bypassed 0/1, and one row with a stored raw_output BLOB.
/// Returns (tempdir, db_path, raw_inv_id) where raw_inv_id is the invocations
/// row whose raw_output_id is non-NULL.
fn seed_realistic_db() -> (tempfile::TempDir, PathBuf, i64) {
    let (tmp, db_path) = setup_db_path();

    let tracker =
        Tracker::open(&db_path, &default_retention(), true, FIXED_NOW_MS).expect("open ok");

    // Unmatched (rule_id IS NULL), project-a, current ts, big output.
    tracker
        .record(
            &meta(
                FIXED_NOW_MS,
                "pnpm install",
                None,
                None,
                Some("/proj-a"),
                0,
                false,
                5000,
            ),
            None,
            None,
            None,
            false,
            false,
        )
        .expect("rec unmatched-a");

    // Another unmatched run of the same command (groups together, bigger sum).
    tracker
        .record(
            &meta(
                FIXED_NOW_MS,
                "pnpm install",
                None,
                None,
                Some("/proj-a"),
                0,
                false,
                3000,
            ),
            None,
            None,
            None,
            false,
            false,
        )
        .expect("rec unmatched-a2");

    // Unmatched but OLD (older than a 1-day cutoff) — excluded by --since.
    tracker
        .record(
            &meta(
                FIXED_NOW_MS - 5 * ONE_DAY_MS,
                "tsc --noEmit",
                None,
                None,
                Some("/proj-b"),
                0,
                false,
                1500,
            ),
            None,
            None,
            None,
            false,
            false,
        )
        .expect("rec unmatched-old");

    // Matched rule, project-b, current ts, non-zero exit.
    tracker
        .record(
            &meta(
                FIXED_NOW_MS,
                "cargo build",
                Some(RuleSource::Project),
                Some("rule-cargo"),
                Some("/proj-b"),
                1,
                false,
                4000,
            ),
            None,
            None,
            None,
            false,
            false,
        )
        .expect("rec matched-b");

    // Matched rule, bypassed=1 (excluded from offender/savings views).
    tracker
        .record(
            &meta(
                FIXED_NOW_MS,
                "cargo test",
                Some(RuleSource::User),
                Some("rule-cargo-test"),
                Some("/proj-a"),
                0,
                true,
                9999,
            ),
            None,
            None,
            None,
            false,
            false,
        )
        .expect("rec bypassed");

    // One row WITH a stored raw_output BLOB (raw_output_id non-NULL).
    let proj_tmp = tempfile::TempDir::new().unwrap();
    std::fs::create_dir_all(proj_tmp.path().join(".lacon")).unwrap();
    let raw = RawOutput {
        stdout: b"compiling...\nwarning: unused\n".to_vec(),
        stderr: b"error[E0382]\n".to_vec(),
    };
    let raw_inv_id = tracker
        .record(
            &meta(
                FIXED_NOW_MS,
                "cargo check",
                Some(RuleSource::Project),
                Some("rule-cargo-check"),
                Some("/proj-b"),
                101,
                false,
                256,
            ),
            Some(&raw),
            Some(proj_tmp.path()),
            None,
            true, // project_store_raw
            false,
        )
        .expect("rec raw-stored");

    drop(tracker);
    (tmp, db_path, raw_inv_id)
}

#[test]
fn unmatched_offenders_ordered_by_total_raw_bytes_desc() {
    let (_tmp, db_path, _raw_id) = seed_realistic_db();
    let conn = tracking::open_readonly(&db_path).expect("open_readonly");

    let rows = tracking::query::unmatched_offenders(&conn).expect("unmatched view");
    // Unmatched commands: "pnpm install" (2 runs, 8000) and "tsc --noEmit"
    // (1 run, 1500). Both rule_id IS NULL, bypassed=0.
    assert_eq!(rows.len(), 2, "two unmatched command groups");
    assert_eq!(rows[0].command_normalized, "pnpm install");
    assert_eq!(rows[0].runs, 2);
    assert_eq!(rows[0].total_raw_bytes, 8000);
    // ORDER BY total_raw_bytes DESC → pnpm (8000) before tsc (1500).
    assert!(rows[0].total_raw_bytes >= rows[1].total_raw_bytes);
    assert_eq!(rows[1].command_normalized, "tsc --noEmit");
}

#[test]
fn filtered_offenders_view_reads_matched_rows() {
    let (_tmp, db_path, _raw_id) = seed_realistic_db();
    let conn = tracking::open_readonly(&db_path).expect("open_readonly");

    let rows = tracking::query::filtered_offenders(&conn).expect("filtered view");
    // Matched, bypassed=0: cargo build, cargo check. (cargo test is bypassed.)
    let ids: Vec<&str> = rows.iter().filter_map(|r| r.rule_id.as_deref()).collect();
    assert!(ids.contains(&"rule-cargo"), "cargo build rule present");
    assert!(ids.contains(&"rule-cargo-check"), "cargo check rule present");
    assert!(
        !ids.contains(&"rule-cargo-test"),
        "bypassed rule excluded from filtered_offenders"
    );
}

#[test]
fn project_savings_excludes_bypassed_and_groups_by_project() {
    let (_tmp, db_path, _raw_id) = seed_realistic_db();
    let conn = tracking::open_readonly(&db_path).expect("open_readonly");

    let rows = tracking::query::project_savings(&conn).expect("savings view");
    // /proj-a and /proj-b both present; bypassed rows excluded from totals.
    let proj_a = rows
        .iter()
        .find(|r| r.project_path.as_deref() == Some("/proj-a"))
        .expect("proj-a present");
    // proj-a non-bypassed runs: 2x pnpm install (5000 + 3000). The bypassed
    // cargo test (9999) must NOT contribute.
    assert_eq!(proj_a.raw_total, 8000, "bypassed run excluded from raw_total");
}

#[test]
fn filtered_unmatched_offenders_since_cutoff_excludes_old_rows() {
    let (_tmp, db_path, _raw_id) = seed_realistic_db();
    let conn = tracking::open_readonly(&db_path).expect("open_readonly");

    // Cutoff = now - 1 day. "tsc --noEmit" was 5 days old → excluded.
    let cutoff = (FIXED_NOW_MS - ONE_DAY_MS) as i64;
    let rows = tracking::query::filtered_unmatched_offenders(&conn, Some(cutoff), None)
        .expect("filtered unmatched");
    let cmds: Vec<&str> = rows.iter().map(|r| r.command_normalized.as_str()).collect();
    assert!(cmds.contains(&"pnpm install"), "recent row kept");
    assert!(
        !cmds.contains(&"tsc --noEmit"),
        "old row excluded by --since cutoff"
    );
}

#[test]
fn filtered_project_savings_project_filter_narrows() {
    let (_tmp, db_path, _raw_id) = seed_realistic_db();
    let conn = tracking::open_readonly(&db_path).expect("open_readonly");

    let rows = tracking::query::filtered_project_savings(&conn, None, Some("/proj-a"))
        .expect("filtered savings");
    assert_eq!(rows.len(), 1, "project filter narrows to one project");
    assert_eq!(rows[0].project_path.as_deref(), Some("/proj-a"));
}

#[test]
fn filtered_offenders_rule_filter_narrows() {
    let (_tmp, db_path, _raw_id) = seed_realistic_db();
    let conn = tracking::open_readonly(&db_path).expect("open_readonly");

    let rows =
        tracking::query::filtered_filtered_offenders(&conn, None, None, Some("rule-cargo"))
            .expect("filtered offenders by rule");
    assert!(!rows.is_empty(), "rule filter returns cargo rows");
    assert!(
        rows.iter().all(|r| r.rule_id.as_deref() == Some("rule-cargo")),
        "rule filter narrows to exactly rule-cargo"
    );
}

#[test]
fn fetch_invocation_hit_returns_expected_fields() {
    let (_tmp, db_path, raw_id) = seed_realistic_db();
    let conn = tracking::open_readonly(&db_path).expect("open_readonly");

    let row = tracking::query::fetch_invocation(&conn, raw_id)
        .expect("fetch ok")
        .expect("row exists");
    assert_eq!(row.rule_id.as_deref(), Some("rule-cargo-check"));
    assert_eq!(row.exit_code, 101);
    assert!(row.raw_output_id.is_some(), "raw_output_id non-NULL");
    assert_eq!(row.project_path.as_deref(), Some("/proj-b"));
    assert_eq!(row.command_raw, "cargo check");
}

#[test]
fn fetch_invocation_miss_returns_none() {
    let (_tmp, db_path, _raw_id) = seed_realistic_db();
    let conn = tracking::open_readonly(&db_path).expect("open_readonly");

    let row = tracking::query::fetch_invocation(&conn, 999_999).expect("fetch ok");
    assert!(row.is_none(), "non-existent id → Ok(None)");
}

#[test]
fn fetch_raw_output_round_trips_blobs() {
    let (_tmp, db_path, raw_id) = seed_realistic_db();
    let conn = tracking::open_readonly(&db_path).expect("open_readonly");

    let inv = tracking::query::fetch_invocation(&conn, raw_id)
        .expect("fetch ok")
        .expect("row exists");
    let raw_output_id = inv.raw_output_id.expect("non-NULL raw_output_id");

    let (stdout, stderr) = tracking::query::fetch_raw_output(&conn, raw_output_id)
        .expect("fetch raw ok")
        .expect("raw row exists");
    assert_eq!(stdout, b"compiling...\nwarning: unused\n");
    assert_eq!(stderr, b"error[E0382]\n");
}

#[test]
fn fetch_raw_output_miss_returns_none() {
    let (_tmp, db_path, _raw_id) = seed_realistic_db();
    let conn = tracking::open_readonly(&db_path).expect("open_readonly");
    let res = tracking::query::fetch_raw_output(&conn, 999_999).expect("fetch raw ok");
    assert!(res.is_none(), "non-existent raw id → Ok(None)");
}

#[test]
fn read_path_helpers_compile_against_path_ref() {
    // Compile-time guard: open_readonly accepts &Path (not just &PathBuf).
    fn _takes_path(p: &Path) -> Result<rusqlite::Connection, lacon_core::error::TrackingError> {
        tracking::open_readonly(p)
    }
}

// ---------------------------------------------------------------------------
// Plan 08-01: overall_totals headline aggregate (ADR 0014 §1 / D-05)
// ---------------------------------------------------------------------------

#[test]
fn overall_totals_excludes_bypassed_rows() {
    let (_tmp, db_path, _raw_id) = seed_realistic_db();
    let conn = tracking::open_readonly(&db_path).expect("open_readonly");

    let totals = tracking::query::overall_totals(&conn).expect("overall_totals");

    // seed_realistic_db() writes 6 rows; exactly one (cargo test, 9999 bytes,
    // /proj-a) is bypassed=1. The headline floor is bypassed=0, so the bypassed
    // row's bytes (raw 9999) must be absent from every total. The five
    // bypassed=0 rows (filtered_bytes = raw_stdout_bytes/2, raw_stderr=0):
    //   pnpm install   raw 5000  kept 2500
    //   pnpm install   raw 3000  kept 1500
    //   tsc --noEmit   raw 1500  kept  750
    //   cargo build    raw 4000  kept 2000
    //   cargo check    raw  256  kept  128
    // raw_total  = 5000 + 3000 + 1500 + 4000 + 256 = 13756
    // kept_total = 2500 + 1500 +  750 + 2000 + 128 =  6878
    let expected_raw = 5000 + 3000 + 1500 + 4000 + 256;
    let expected_kept = 2500 + 1500 + 750 + 2000 + 128;
    assert_eq!(totals.total_runs, 5, "only the 5 bypassed=0 rows counted");
    assert_eq!(
        totals.raw_total, expected_raw,
        "bypassed row's raw bytes (9999) absent from raw_total"
    );
    assert_eq!(totals.kept_total, expected_kept, "kept_total == SUM(filtered_bytes)");
    assert_eq!(
        totals.bytes_saved,
        expected_raw - expected_kept,
        "bytes_saved == raw_total - kept_total"
    );
    // /proj-a and /proj-b appear among bypassed=0 rows → 2 distinct projects.
    assert_eq!(totals.distinct_projects, 2, "two distinct stored project paths");
}

#[test]
fn filtered_overall_totals_empty_filter_returns_zeroed_row() {
    // Regression guard for the scalar-SUM-over-zero-rows NULL pitfall: a filter
    // matching nothing on a POPULATED db must yield an all-zero OverallTotals
    // (proves COALESCE(SUM,0) + query_row), not a NULL-conversion Err or panic.
    let (_tmp, db_path, _raw_id) = seed_realistic_db();
    let conn = tracking::open_readonly(&db_path).expect("open_readonly");

    let totals =
        tracking::query::filtered_overall_totals(&conn, None, Some("/nope-no-such-project"))
            .expect("filtered_overall_totals must not Err on a zero-match filter");

    assert_eq!(
        totals,
        tracking::query::OverallTotals {
            total_runs: 0,
            distinct_projects: 0,
            raw_total: 0,
            kept_total: 0,
            bytes_saved: 0,
        },
        "zero-match filter returns an all-zero headline, not NULL/Err"
    );
}
