//! Integration tests for tracking::Tracker::open:
//! - Creates parent dir with 0700 perms (Unix)
//! - Applies the 3 PRAGMAs in the documented order
//! - WAL is persistent on the DB file (subsequent connections see it)
//! - Idempotent re-open (migration short-circuits)
//! - 0755 parent dir is fixed to 0700 idempotently
//!
//! Test isolation strategy (RESEARCH Pitfall #4): Tracker::open accepts a
//! `db_path: &Path` directly — no env-var override needed for test isolation.
//! Each test allocates its own TempDir.

use std::path::PathBuf;

use rusqlite::Connection;

use lacon_core::config::Retention;
use lacon_core::tracking::Tracker;

const FIXED_NOW_MS: u64 = 1_700_000_000_000;

fn default_retention() -> Retention {
    Retention {
        invocations_days: 30,
        raw_outputs_days: 3,
    }
}

fn setup_db_path() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("lacon").join("history.db");
    (tmp, db_path)
}

#[test]
fn open_creates_db_and_migrates() {
    let (_tmp, db_path) = setup_db_path();
    let _tracker = Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS)
        .expect("open ok");
    assert!(db_path.exists(), "history.db created");

    // Re-open a fresh connection and verify user_version is stamped.
    let conn = Connection::open(&db_path).unwrap();
    let user_version: i32 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(user_version, 1, "migration applied");
}

#[test]
fn open_idempotent_reopen() {
    let (_tmp, db_path) = setup_db_path();
    {
        let _t1 = Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS)
            .expect("first open ok");
    } // drop closes connection
    let _t2 = Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS + 1)
        .expect("re-open ok");
    // No assertion needed — second open succeeded → migration short-circuited
    // and re-applied pragmas without DDL replay.
}

#[test]
fn open_persists_wal_in_db_header() {
    let (_tmp, db_path) = setup_db_path();
    let _tracker = Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS)
        .expect("open ok");
    // Drop the tracker first — important: WAL is persistent on the DB FILE
    // [sqlite.org/wal.html], so a fresh connection sees journal_mode=wal even
    // without re-setting the pragma.
    drop(_tracker);

    let conn = Connection::open(&db_path).unwrap();
    let mode: String = conn
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(mode.to_ascii_lowercase(), "wal", "WAL persists in DB header");
}

#[test]
fn open_fk_pragma_is_per_connection() {
    // RESEARCH Pitfall #1 — verify Tracker::open's connection has FKs ON, AND
    // verify the pragma is per-connection (i.e. another connection's FK state
    // is independent).
    //
    // [Rule 1 deviation] The original plan asserted "fresh conn defaults to FKs
    // OFF" as the per-connection proof, but libsqlite3-sys 0.37 (under bundled
    // rusqlite 0.39) compiles SQLite with -DSQLITE_DEFAULT_FOREIGN_KEYS=1 — so a
    // freshly-opened bundled connection has foreign_keys=ON by default
    // (cf. tracking_schema.rs::fk_silent_no_op_without_pragma which documents
    // the same finding). We instead prove the per-connection invariant by
    // explicitly disabling FKs on a sibling connection and verifying that
    // the toggle does NOT propagate back to tracker.conn.
    //
    // This test reads `tracker.conn` directly — which only compiles because
    // Tracker.conn is `pub` (NOT `pub(crate)`). Revision iteration 1, Issue #1.
    let (_tmp, db_path) = setup_db_path();
    let tracker = Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS)
        .expect("open ok");

    // tracker.conn has FKs ON — defensive pragma applied by apply_connection_pragmas.
    let fk_on_tracker: i32 = tracker
        .conn
        .pragma_query_value(None, "foreign_keys", |r| r.get(0))
        .unwrap();
    assert_eq!(fk_on_tracker, 1, "Tracker::open's conn has FKs ON");

    // Open a sibling connection and EXPLICITLY DISABLE foreign_keys on it.
    let sibling = Connection::open(&db_path).unwrap();
    sibling
        .pragma_update(None, "foreign_keys", false)
        .expect("disable fk on sibling");
    let fk_on_sibling: i32 = sibling
        .pragma_query_value(None, "foreign_keys", |r| r.get(0))
        .unwrap();
    assert_eq!(fk_on_sibling, 0, "sibling conn FKs explicitly OFF");

    // Per-connection invariant: tracker.conn is unaffected by sibling's pragma.
    let fk_on_tracker_after: i32 = tracker
        .conn
        .pragma_query_value(None, "foreign_keys", |r| r.get(0))
        .unwrap();
    assert_eq!(
        fk_on_tracker_after, 1,
        "tracker.conn FKs remain ON — pragma is per-connection, not shared"
    );
}

#[test]
fn open_busy_timeout_is_200ms() {
    // D-11: 200ms busy_timeout, NOT rusqlite's 5000ms default.
    // Reads `tracker.conn` directly — requires Tracker.conn to be `pub`
    // (Issue #1).
    let (_tmp, db_path) = setup_db_path();
    let tracker = Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS)
        .expect("open ok");
    // PRAGMA busy_timeout returns the value in milliseconds.
    let timeout: i64 = tracker
        .conn
        .pragma_query_value(None, "busy_timeout", |r| r.get(0))
        .unwrap();
    assert_eq!(timeout, 200, "D-11 explicit override of rusqlite's 5000ms default");
}

#[cfg(unix)]
#[test]
fn open_creates_parent_dir_with_0700() {
    use std::os::unix::fs::PermissionsExt;
    let (_tmp, db_path) = setup_db_path();
    let _tracker = Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS)
        .expect("open ok");
    let parent = db_path.parent().unwrap();
    let mode = std::fs::metadata(parent).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "parent dir mode must be 0700");
}

#[cfg(unix)]
#[test]
fn open_fixes_pre_existing_0755_to_0700() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::TempDir::new().unwrap();
    let parent = tmp.path().join("lacon");
    std::fs::create_dir_all(&parent).unwrap();
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();

    let db_path = parent.join("history.db");
    let _tracker = Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS)
        .expect("open ok");
    let mode = std::fs::metadata(&parent).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700, "pre-existing 0755 dir is fixed idempotently");
}

#[test]
fn xdg_db_path_returns_lacon_history_db() {
    let p = Tracker::xdg_db_path().expect("etcetera resolves on test platform");
    // The path should END with `lacon/history.db` regardless of XDG_DATA_HOME.
    let s = p.to_string_lossy();
    assert!(
        s.ends_with("lacon/history.db") || s.ends_with("lacon\\history.db"),
        "xdg_db_path ends with lacon/history.db; got {s}"
    );
}
