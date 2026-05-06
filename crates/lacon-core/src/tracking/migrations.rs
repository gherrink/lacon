//! Schema migration runner via SQLite `PRAGMA user_version`.
//!
//! Per CONTEXT D-08: a single inline `M0001_INITIAL` migration covers all v1
//! schema. Future versions append to this dispatch — never edit M0001.
//!
//! Per CONTEXT D-09: applied inside a single `BEGIN IMMEDIATE` / `COMMIT`
//! transaction; views use `DROP VIEW IF EXISTS` for forward-compat.
//!
//! Per RESEARCH §"`migrate()` pattern": `BEGIN IMMEDIATE` acquires the write
//! lock up front, avoiding the upgrade-from-read SQLITE_BUSY race documented
//! at sqlite.org/forum/info/843e9b7f8f8f3398.
//!
//! `include_str!` resolves relative to THIS source file —
//! `crates/lacon-core/src/tracking/migrations.rs` — so the path
//! `migrations/0001_initial.sql` lands at
//! `crates/lacon-core/src/tracking/migrations/0001_initial.sql`.

use rusqlite::{Connection, TransactionBehavior};

use crate::error::TrackingError;

/// Inline DDL for the v1 schema. Embedded at compile time.
pub(crate) const M0001_INITIAL: &str = include_str!("migrations/0001_initial.sql");

/// Current schema version. Increment when appending a new migration.
pub(crate) const TARGET_VERSION: i32 = 1;

/// Apply all unapplied migrations in a single transaction.
///
/// Reads `PRAGMA user_version`; if it's already `>= TARGET_VERSION`, returns
/// without doing any work. Otherwise opens a `BEGIN IMMEDIATE` transaction,
/// runs each unapplied migration's `execute_batch`, stamps `user_version`,
/// and commits.
///
/// # Errors
/// Returns `TrackingError::Sqlite` on any rusqlite failure (connection bad,
/// SQL syntax error, transaction commit failed, etc.).
pub fn migrate(conn: &mut Connection) -> Result<(), TrackingError> {
    let current: i32 =
        conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if current >= TARGET_VERSION {
        return Ok(());
    }

    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if current < 1 {
        tx.execute_batch(M0001_INITIAL)?;
    }
    // Future: if current < 2 { tx.execute_batch(M0002_FOO)?; }
    tx.pragma_update(None, "user_version", TARGET_VERSION)?;
    tx.commit()?;
    Ok(())
}
