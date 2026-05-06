//! Integration tests for tracking::Tracker::record:
//! - store_raw_outputs gate (REQ-tracking-raw-outputs-default-off)
//! - FK linkage between raw_outputs and invocations
//! - privacy warning trigger creates marker exactly once (REQ-tracking-privacy-warning)
//! - rule_source enum → TEXT mapping
//! - meta field round-trip

use std::path::PathBuf;

use rusqlite::Connection;

use lacon_core::config::Retention;
use lacon_core::runtime::{ByteCounts, InvocationMeta};
use lacon_core::rules::loader::RuleSource;
use lacon_core::tracking::{privacy::MARKER_FILENAME, RawOutput, Tracker};

const FIXED_NOW_MS: u64 = 1_700_000_000_000;

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

fn sample_meta(rule: Option<RuleSource>) -> InvocationMeta {
    InvocationMeta {
        ts_unix_ms: FIXED_NOW_MS,
        rule_id: rule.as_ref().map(|_| "rule-a".to_string()),
        rule_source: rule,
        command_raw: "pnpm install".to_string(),
        argv: vec!["pnpm".into(), "install".into()],
        exit_code: 0,
        duration_ms: 42,
        byte_counts: ByteCounts {
            raw_stdout_bytes: 1234,
            raw_stderr_bytes: 0,
            filtered_bytes: 567,
        },
        bypassed: false,
        rewritten: false,
        truncated_by_max_bytes: false,
        assistant: "claude-code".to_string(),
        session_id: Some("sess-xyz".to_string()),
        project_path: Some(PathBuf::from("/proj")),
        command_normalized: "pnpm install".to_string(),
        raw_output_id: None,
    }
}

fn count(conn: &Connection, table: &str) -> i64 {
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn raw_outputs_off_no_insert() {
    let (_tmp, db_path) = setup_db_path();
    let tracker = Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS)
        .expect("open ok");
    let raw = RawOutput { stdout: b"hello".to_vec(), stderr: b"err".to_vec() };

    let inv_id = tracker
        .record(
            &sample_meta(Some(RuleSource::Project)),
            Some(&raw),
            Some(std::path::Path::new("/proj")),
            None,
            false, // project_store_raw
            false, // user_store_raw
        )
        .expect("record ok");

    // Even with raw=Some(...) and (irrelevantly) the gate flags, cfg_store_raw_outputs=false
    // means no raw_outputs INSERT.
    assert_eq!(count(&tracker.conn, "raw_outputs"), 0, "default off: no raw_outputs");
    assert_eq!(count(&tracker.conn, "invocations"), 1);

    let raw_id: Option<i64> = tracker
        .conn
        .query_row("SELECT raw_output_id FROM invocations WHERE id = ?1", [inv_id], |r| r.get(0))
        .unwrap();
    assert!(raw_id.is_none(), "raw_output_id is NULL when raw retention off");
}

#[test]
fn raw_outputs_on_inserts_both_with_fk_link() {
    let (_tmp, db_path) = setup_db_path();
    let tracker = Tracker::open(&db_path, &default_retention(), true, FIXED_NOW_MS)
        .expect("open ok");
    let raw = RawOutput {
        stdout: b"line1\nline2\n".to_vec(),
        stderr: b"err1\n".to_vec(),
    };

    // Use a tempdir as project_root so the marker doesn't pollute the real fs.
    let proj_tmp = tempfile::TempDir::new().unwrap();
    let proj = proj_tmp.path();
    std::fs::create_dir_all(proj.join(".lacon")).unwrap();

    let _inv_id = tracker
        .record(
            &sample_meta(Some(RuleSource::User)),
            Some(&raw),
            Some(proj),
            None,
            true,  // project_store_raw — turn on the gate
            false,
        )
        .expect("record ok");

    assert_eq!(count(&tracker.conn, "raw_outputs"), 1);
    assert_eq!(count(&tracker.conn, "invocations"), 1);

    // FK linkage: invocations.raw_output_id == raw_outputs.id
    let (inv_raw_id, raw_id): (Option<i64>, i64) = tracker
        .conn
        .query_row(
            "SELECT i.raw_output_id, r.id
             FROM invocations i, raw_outputs r",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(inv_raw_id, Some(raw_id), "FK linkage correct");

    // Round-trip the BLOB.
    let stored_stdout: Vec<u8> = tracker
        .conn
        .query_row("SELECT stdout FROM raw_outputs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(stored_stdout, b"line1\nline2\n");
}

#[test]
fn raw_outputs_on_with_none_raw_skips_raw_insert() {
    let (_tmp, db_path) = setup_db_path();
    let tracker = Tracker::open(&db_path, &default_retention(), true, FIXED_NOW_MS)
        .expect("open ok");

    let _ = tracker
        .record(
            &sample_meta(None),
            None, // raw_opt = None
            None,
            None,
            false,
            false,
        )
        .expect("record ok");

    assert_eq!(count(&tracker.conn, "raw_outputs"), 0, "raw=None → no raw INSERT");
    assert_eq!(count(&tracker.conn, "invocations"), 1);
}

#[test]
fn privacy_marker_created_on_first_raw_record() {
    let (_tmp, db_path) = setup_db_path();
    let tracker = Tracker::open(&db_path, &default_retention(), true, FIXED_NOW_MS)
        .expect("open ok");
    let raw = RawOutput { stdout: vec![], stderr: vec![] };

    let proj_tmp = tempfile::TempDir::new().unwrap();
    let proj = proj_tmp.path();
    std::fs::create_dir_all(proj.join(".lacon")).unwrap();
    let marker = proj.join(".lacon").join(MARKER_FILENAME);
    assert!(!marker.exists(), "marker absent before first record");

    tracker
        .record(
            &sample_meta(Some(RuleSource::Project)),
            Some(&raw),
            Some(proj),
            None,
            true,
            false,
        )
        .expect("record ok");

    assert!(marker.exists(), "marker created on first raw record");
}

#[test]
fn second_record_does_not_re_warn() {
    let (_tmp, db_path) = setup_db_path();
    let tracker = Tracker::open(&db_path, &default_retention(), true, FIXED_NOW_MS)
        .expect("open ok");
    let raw = RawOutput { stdout: vec![], stderr: vec![] };

    let proj_tmp = tempfile::TempDir::new().unwrap();
    let proj = proj_tmp.path();
    std::fs::create_dir_all(proj.join(".lacon")).unwrap();
    let marker = proj.join(".lacon").join(MARKER_FILENAME);

    tracker.record(&sample_meta(Some(RuleSource::Project)), Some(&raw),
        Some(proj), None, true, false).expect("first record ok");
    tracker.record(&sample_meta(Some(RuleSource::Project)), Some(&raw),
        Some(proj), None, true, false).expect("second record ok (silent)");

    assert!(marker.exists(), "marker still exists");
    assert_eq!(count(&tracker.conn, "raw_outputs"), 2, "2 raw rows");
    assert_eq!(count(&tracker.conn, "invocations"), 2, "2 invocation rows");
}

#[test]
fn rule_source_text_maps_correctly() {
    let (_tmp, db_path) = setup_db_path();
    let tracker = Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS)
        .expect("open ok");

    let cases: &[(Option<RuleSource>, Option<&str>)] = &[
        (Some(RuleSource::Project), Some("project")),
        (Some(RuleSource::User), Some("user")),
        (Some(RuleSource::Bundled), Some("bundled")),
        (None, None),
    ];

    for (src, expected) in cases {
        let inv_id = tracker
            .record(&sample_meta(src.clone()), None, None, None, false, false)
            .expect("record ok");

        let stored: Option<String> = tracker
            .conn
            .query_row(
                "SELECT rule_source FROM invocations WHERE id = ?1",
                [inv_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stored.as_deref(), *expected, "rule_source for {src:?}");
    }
}

#[test]
fn meta_fields_round_trip() {
    let (_tmp, db_path) = setup_db_path();
    let tracker = Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS)
        .expect("open ok");
    let mut meta = sample_meta(Some(RuleSource::Project));
    meta.exit_code = 42;
    meta.duration_ms = 7777;
    meta.byte_counts.raw_stdout_bytes = 1024;
    meta.byte_counts.filtered_bytes = 256;
    meta.bypassed = true;
    meta.rewritten = true;

    let inv_id = tracker
        .record(&meta, None, None, None, false, false)
        .expect("record ok");

    let row: (
        i64, i64, String, Option<String>, Option<String>,
        String, String, Option<String>, Option<String>,
        i64, i64, i64, i64, i64, i64, i64, i64, Option<i64>,
    ) = tracker
        .conn
        .query_row(
            "SELECT id, ts, assistant, session_id, project_path,
                    command_raw, command_normalized, rule_id, rule_source,
                    exit_code, duration_ms,
                    raw_stdout_bytes, raw_stderr_bytes, filtered_bytes,
                    bypassed, rewritten, truncated_by_max_bytes, raw_output_id
             FROM invocations WHERE id = ?1",
            [inv_id],
            |r| {
                Ok((
                    r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?,
                    r.get(5)?, r.get(6)?, r.get(7)?, r.get(8)?,
                    r.get(9)?, r.get(10)?, r.get(11)?, r.get(12)?, r.get(13)?,
                    r.get(14)?, r.get(15)?, r.get(16)?, r.get(17)?,
                ))
            },
        )
        .unwrap();

    assert_eq!(row.1, FIXED_NOW_MS as i64);
    assert_eq!(row.2, "claude-code");
    assert_eq!(row.3.as_deref(), Some("sess-xyz"));
    assert_eq!(row.4.as_deref(), Some("/proj"));
    assert_eq!(row.5, "pnpm install");
    assert_eq!(row.6, "pnpm install");
    assert_eq!(row.7.as_deref(), Some("rule-a"));
    assert_eq!(row.8.as_deref(), Some("project"));
    assert_eq!(row.9, 42);
    assert_eq!(row.10, 7777);
    assert_eq!(row.11, 1024);
    assert_eq!(row.13, 256);
    assert_eq!(row.14, 1);
    assert_eq!(row.15, 1);
    assert_eq!(row.17, None);
}
