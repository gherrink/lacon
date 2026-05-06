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
    debug_assert_eq!(one, 1);
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
}
