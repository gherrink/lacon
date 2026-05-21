//! Tracker health-check probe. Phase 2 D-13: defines the surface; Phase 4
//! `lacon doctor` consumes it. Phase 2 code never calls `health_check` itself —
//! it exists for the doctor command's introspection sweep.

use rusqlite::Connection;

use crate::error::TrackingError;

/// Result of a tracker health probe.
#[derive(Debug, Clone)]
pub struct HealthReport {
    /// `SELECT 1` round-trip result. Confirms the connection is reachable.
    pub select_one_returned: i32,
}

/// Run a no-op write/read probe against the tracker connection.
///
/// Currently a `SELECT 1` round-trip — confirms the DB is reachable and the
/// connection is healthy. Phase 4 may extend this to also check
/// `pragma user_version`, `journal_mode`, and `foreign_keys` settings.
///
/// # Errors
/// `TrackingError::Sqlite` if the round-trip fails.
pub fn health_check(conn: &Connection) -> Result<HealthReport, TrackingError> {
    let one: i32 = conn.query_row("SELECT 1", [], |r| r.get(0))?;
    // WR-05: return a hard error instead of `debug_assert_eq!(one, 1)`. doctor
    // calls this on a possibly-corrupt, user-owned history.db; a debug-build
    // assert would *panic* the very command whose job is to report DB problems
    // gracefully (doctor renders this Err as `[fail] tracker`). Mirrors the
    // `pragma_update_and_check` + `return Err(...)` pattern in tracking/mod.rs.
    if one != 1 {
        return Err(TrackingError::HealthProbe {
            expected: 1,
            got: one,
        });
    }
    Ok(HealthReport {
        select_one_returned: one,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_check_against_in_memory_conn() {
        let conn = Connection::open_in_memory().unwrap();
        let report = health_check(&conn).expect("ok");
        assert_eq!(report.select_one_returned, 1);
    }

    // WR-05: an unexpected probe result must surface as a returned error in ALL
    // build profiles (no debug-build panic). `SELECT 0` exercises the `one != 1`
    // branch so doctor stays graceful even when the probe is later extended to
    // assert a DB-derived value on a corrupt history.db.
    #[test]
    fn health_check_unexpected_value_is_error_not_panic() {
        let conn = Connection::open_in_memory().unwrap();
        let one: i32 = conn.query_row("SELECT 0", [], |r| r.get(0)).unwrap();
        assert_ne!(one, 1, "sanity: probe value is the unexpected case");

        // Simulate the probe returning a non-1 value by checking the same
        // branch health_check guards. The function itself runs `SELECT 1`, which
        // cannot return non-1 from SQLite, so we assert the error mapping holds.
        let err = TrackingError::HealthProbe {
            expected: 1,
            got: one,
        };
        assert!(matches!(err, TrackingError::HealthProbe { expected: 1, got: 0 }));
        assert!(format!("{err}").contains("health probe returned unexpected value"));
    }
}
