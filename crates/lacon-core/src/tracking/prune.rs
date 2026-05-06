//! 24h-throttled retention prune (CONTEXT D-06).
//!
//! On `Tracker::open`, this function reads `lacon_meta.last_pruned_ts` (text
//! column → parsed as i64; 0 if absent). If `(now_ms - last) >= 86_400_000` ms
//! (24h), it runs the three retention DELETEs in a single transaction and
//! updates `last_pruned_ts` to `now_ms`. Otherwise it short-circuits to Ok.
//!
//! # DELETE order matters (RESEARCH §"Pruning Throttle Pattern" line 488)
//! Delete `raw_outputs` first to avoid the `ON DELETE SET NULL` trigger firing
//! on every row about to be deleted by the cascading `invocations` DELETE.
//! Then `suspected_regressions` (independent in v1; rows are mostly removed by
//! the FK CASCADE when their parent is deleted, but the explicit DELETE catches
//! any orphans). Then `invocations` last.
//!
//! # Index coverage
//! - `DELETE FROM invocations WHERE ts < ?`             → `idx_inv_ts` ✓
//! - `DELETE FROM raw_outputs WHERE created_ts < ?`     → `idx_raw_created` ✓
//! - `DELETE FROM suspected_regressions WHERE detected_ts < ?` → no index, but
//!   v1 row volume is small (mostly cascaded from invocations).
//!
//! # Spec interpretation
//! `docs/specs/config-schema.md:36`: `invocations_days` also governs
//! `suspected_regressions`. Same cutoff applies to both.

use rusqlite::{params, Connection};

use crate::config::Retention;
use crate::error::TrackingError;

/// 24h in milliseconds — the prune throttle window per CONTEXT D-06.
pub(crate) const PRUNE_THROTTLE_MS: i64 = 86_400_000;

/// One day in milliseconds — used to convert retention.*_days into a cutoff.
pub(crate) const ONE_DAY_MS: i64 = 86_400_000;

/// Run the 3 retention DELETEs only if the 24h throttle window has elapsed
/// since the last prune (per `lacon_meta.last_pruned_ts`).
///
/// `now_ms` is injected for testability. Production callers pass
/// `SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64`.
///
/// # Errors
/// `TrackingError::Sqlite` on any rusqlite failure.
pub fn prune_if_due(
    conn: &Connection,
    retention: &Retention,
    now_ms: u64,
) -> Result<(), TrackingError> {
    let now_i64 = now_ms as i64;

    // Read last_pruned_ts (TEXT column → parse to i64; 0 if missing/garbage).
    let last: i64 = conn
        .query_row(
            "SELECT value FROM lacon_meta WHERE key = 'last_pruned_ts'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

    // Throttle gate: if less than 24h since last prune, skip.
    if now_i64 - last < PRUNE_THROTTLE_MS {
        return Ok(());
    }

    let inv_cutoff = now_i64 - (retention.invocations_days as i64) * ONE_DAY_MS;
    let raw_cutoff = now_i64 - (retention.raw_outputs_days as i64) * ONE_DAY_MS;

    // unchecked_transaction lets us hold &Connection (not &mut). Safe under
    // single-threaded-per-process invariant. [RESEARCH Crate API Notes line 481]
    let tx = conn.unchecked_transaction()?;

    // Order matters: raw_outputs first (avoids ON DELETE SET NULL firing on
    // every row in the same prune wave). Then suspected_regressions
    // (independent in v1). Then invocations last (cascades any orphans).
    tx.execute(
        "DELETE FROM raw_outputs WHERE created_ts < ?1",
        params![raw_cutoff],
    )?;
    tx.execute(
        "DELETE FROM suspected_regressions WHERE detected_ts < ?1",
        params![inv_cutoff],
    )?;
    tx.execute(
        "DELETE FROM invocations WHERE ts < ?1",
        params![inv_cutoff],
    )?;
    tx.execute(
        "UPDATE lacon_meta SET value = ?1 WHERE key = 'last_pruned_ts'",
        params![now_ms.to_string()],
    )?;
    tx.commit()?;
    Ok(())
}
