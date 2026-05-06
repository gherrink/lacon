//! Tracking subsystem (Phase 2): SQLite-backed history of every `lacon run`.
//!
//! Lives at `~/.local/share/lacon/history.db` (XDG `data_dir/lacon/history.db`)
//! per REQ-tracking-sqlite-location. Tracker writes are best-effort (D-12) — the
//! CLI logs failures to stderr and never alters exit codes.
//!
//! Module layout:
//! - `migrations` — single inline `M0001_INITIAL` migration via `user_version`
//! - `normalize` — pure `fn normalize(argv) -> String` for command grouping
//! - `privacy` — first-time `store_raw_outputs` opt-in marker + warning text
//! - `health` — `Tracker::health_check` no-op probe (Phase 4 surface)
//! - `prune` — throttled retention pruning (24h gate via `lacon_meta`)
//!
//! Cold-start posture (D-04): Tracker::open is reachable ONLY from
//! `lacon-cli::commands::run` after `Runner::run` returns. `lacon --version`,
//! `lacon validate`, and `lacon doctor` MUST NOT call into this module.

pub mod normalize;
pub mod migrations;
pub mod privacy;
pub mod health;
pub mod prune;
pub mod record;

pub use normalize::normalize;
pub use migrations::migrate;

use std::path::{Path, PathBuf};
use std::time::Duration;

use rusqlite::config::DbConfig;
use rusqlite::{Connection, OpenFlags};

use crate::config::Retention;
use crate::error::TrackingError;

/// Raw subprocess output captured for `raw_outputs` storage (D-01).
/// Populated by `lacon-cli::commands::run` only when `cfg.store_raw_outputs == true`.
#[derive(Debug, Clone, Default)]
pub struct RawOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Tracker handle (one per `lacon run` invocation; dropped at function exit).
/// Holds an open SQLite connection with the v1 PRAGMA contract applied.
///
/// `conn` is `pub` (NOT `pub(crate)`) because integration tests under
/// `crates/lacon-core/tests/` are external to the crate boundary; they need
/// to read `tracker.conn` directly to verify pragma state and inspect
/// inserted rows. (Revision iteration 1, Issue #1: pub(crate) caused compile
/// errors in tracking_tracker.rs and tracking_record.rs.)
pub struct Tracker {
    pub conn: Connection,
    #[allow(dead_code)]
    pub(crate) cfg_store_raw_outputs: bool,
}

impl Tracker {
    /// Open (or create) the tracker database at `db_path`, apply per-connection
    /// PRAGMAs (busy_timeout=200ms, foreign_keys=ON, journal_mode=WAL), run any
    /// pending migrations, and run the throttled prune.
    ///
    /// # Cold-start posture (CONTEXT D-04)
    /// `Tracker::open` is reachable ONLY from `lacon-cli::commands::run` after
    /// `Runner::run` returns. `lacon --version`, `lacon validate`, and
    /// `lacon doctor` (Phase 4) MUST NOT call this constructor.
    ///
    /// # Pragmas (RESEARCH §"Connection open + PRAGMA sequence")
    /// 1. `busy_timeout=200ms` — D-11; explicit override of rusqlite's 5000ms default.
    /// 2. `foreign_keys=ON` via `set_db_config(SQLITE_DBCONFIG_ENABLE_FKEY)` —
    ///    RESEARCH Pitfall #1; without it ON DELETE CASCADE / SET NULL are silent no-ops.
    /// 3. `journal_mode=WAL` via `pragma_update_and_check` — persistent on the DB header
    ///    but cheap to re-set on every connection.
    ///
    /// # Errors
    /// - `TrackingError::CreateDir` if the parent directory cannot be created.
    /// - `TrackingError::Chmod` if the parent directory permissions cannot be set.
    /// - `TrackingError::Sqlite` for any rusqlite failure (connection open, pragma,
    ///   migration, prune).
    pub fn open(
        db_path: &Path,
        retention: &Retention,
        cfg_store_raw_outputs: bool,
        now_ms: u64,
    ) -> Result<Self, TrackingError> {
        // 1. Ensure parent dir exists with 0700 perms.
        if let Some(parent) = db_path.parent() {
            ensure_data_dir(parent)?;
        }

        // 2. Open connection with NO_MUTEX (single-threaded per process).
        let mut conn = Connection::open_with_flags(
            db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;

        // 3. Per-connection PRAGMAs (RESEARCH §"Connection open + PRAGMA sequence").
        apply_connection_pragmas(&conn)?;

        // 4. Run pending migrations.
        crate::tracking::migrations::migrate(&mut conn)?;

        // 5. Throttled prune (24h gate via lacon_meta.last_pruned_ts).
        crate::tracking::prune::prune_if_due(&conn, retention, now_ms)?;

        Ok(Tracker {
            conn,
            cfg_store_raw_outputs,
        })
    }

    /// Resolve the production DB path: `<XDG_DATA_HOME>/lacon/history.db`.
    /// Uses `etcetera::choose_base_strategy()` which returns `Xdg` on Linux
    /// AND macOS — so the same code resolves to `~/.local/share/lacon/history.db`
    /// on both platforms (REQ-tracking-sqlite-location). [VERIFIED:
    /// docs.rs/etcetera/0.11.0]
    pub fn xdg_db_path() -> Option<PathBuf> {
        use etcetera::BaseStrategy;
        etcetera::choose_base_strategy()
            .ok()
            .map(|s| s.data_dir().join("lacon").join("history.db"))
    }
}

/// Apply the 3 per-connection pragmas in the documented order.
/// Public-in-crate so prune.rs and Plan 04's tests can sanity-check
/// the connection state without re-deriving the contract.
pub(crate) fn apply_connection_pragmas(conn: &Connection) -> Result<(), TrackingError> {
    // (1) busy_timeout — D-11; explicit 200ms (NOT rusqlite's 5000ms default).
    //     Per-connection. Reduces contention masking in tests.
    conn.busy_timeout(Duration::from_millis(200))?;

    // (2) foreign_keys=ON — RESEARCH Pitfall #1; per-connection, defaults OFF.
    //     Without this, ON DELETE CASCADE / SET NULL silently no-op.
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_FKEY, true)?;

    // (3) journal_mode=WAL — persistent on the DB file, but harmless to
    //     re-set on every connection. pragma_update_and_check verifies SQLite
    //     accepted the value rather than silently retaining the previous mode.
    let mode: String = conn
        .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
    debug_assert_eq!(mode.to_ascii_lowercase(), "wal");

    Ok(())
}

/// Ensure the data directory exists with `0700` permissions (idempotent).
/// On non-Unix platforms (none supported in v1, but compile keeps `cargo check`
/// happy on Windows local dev), this is a `create_dir_all`-only no-op.
#[cfg(unix)]
fn ensure_data_dir(dir: &Path) -> Result<(), TrackingError> {
    use std::os::unix::fs::PermissionsExt;

    // create_dir_all is race-free against itself [doc.rust-lang.org].
    std::fs::create_dir_all(dir).map_err(|e| TrackingError::CreateDir {
        path: dir.to_owned(),
        source: e,
    })?;

    // Idempotent perm fix — runs even when dir already existed, defending
    // against a previous lacon version (or human) that may have left it 0755.
    let metadata = std::fs::metadata(dir).map_err(|e| TrackingError::Chmod {
        path: dir.to_owned(),
        source: e,
    })?;
    let mut perms = metadata.permissions();
    if perms.mode() & 0o777 != 0o700 {
        perms.set_mode(0o700);
        std::fs::set_permissions(dir, perms).map_err(|e| TrackingError::Chmod {
            path: dir.to_owned(),
            source: e,
        })?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn ensure_data_dir(dir: &Path) -> Result<(), TrackingError> {
    // v1 explicitly excludes Windows, but keep cargo check on Win clean.
    std::fs::create_dir_all(dir).map_err(|e| TrackingError::CreateDir {
        path: dir.to_owned(),
        source: e,
    })?;
    Ok(())
}

/// Map a `RuleSource` to the spec-mandated TEXT value for `invocations.rule_source`.
/// Per `docs/specs/tracking-data-model.md:25`: `'project' | 'user' | 'bundled' | NULL`.
/// Pitfall 12 from RESEARCH.md.
pub fn rule_source_str(s: &crate::rules::RuleSource) -> &'static str {
    match s {
        crate::rules::RuleSource::Project => "project",
        crate::rules::RuleSource::User => "user",
        crate::rules::RuleSource::Bundled => "bundled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::RuleSource;

    #[test]
    fn rule_source_str_maps_all_three_variants() {
        assert_eq!(rule_source_str(&RuleSource::Project), "project");
        assert_eq!(rule_source_str(&RuleSource::User), "user");
        assert_eq!(rule_source_str(&RuleSource::Bundled), "bundled");
    }
}
