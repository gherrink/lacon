//! Tracking READ path (Phase 4, Plan 04-01) — the data layer that `stats`,
//! `explain`, and `doctor` consume.
//!
//! Phase 2 shipped a write-only tracking layer (`record.rs`). This module is
//! its read sibling: typed result rows over the four reporting views plus the
//! `invocations`/`raw_outputs` lookups `explain` needs.
//!
//! # Design boundary (D-01)
//! ALL SQL lives behind the `lacon-core` boundary — `lacon-cli` keeps
//! `rusqlite` dev-only and never inlines a query. These are free functions
//! over a borrowed `&Connection` (reads need no `Tracker` state, D-01
//! discretion). The connection is expected to come from
//! [`crate::tracking::open_readonly`].
//!
//! # Two read shapes
//! 1. **Unfiltered view readers** — `unmatched_offenders`, `filtered_offenders`,
//!    `bypass_rate`, `project_savings` — read the four DB views directly.
//! 2. **Filtered re-queries (D-09)** — `filtered_*` variants re-implement each
//!    view's GROUP BY/ORDER BY body against the BASE `invocations` table with
//!    added `WHERE` clauses, because no view exposes `ts` and only
//!    `v_project_savings` exposes `project_path`. Filter values are ALWAYS
//!    bound via `params![]` placeholders — never string-interpolated into SQL
//!    (threat T-04-01 / SQL injection mitigation).
//!
//! # explain lookups (D-05)
//! `fetch_invocation` returns the row `explain` displays; `fetch_raw_output`
//! returns the stored stdout/stderr BLOBs by id (only present when the user
//! opted into `store_raw_outputs`).

use rusqlite::{params, Connection};

use crate::error::TrackingError;

// ---------------------------------------------------------------------------
// Typed result rows (one small struct per view, mirroring health.rs style)
// ---------------------------------------------------------------------------

/// Row of `v_unmatched_offenders`: commands that ran without a matching rule.
#[derive(Debug, Clone, PartialEq)]
pub struct UnmatchedOffender {
    pub command_normalized: String,
    pub runs: i64,
    pub total_raw_bytes: i64,
}

/// Row of `v_filtered_offenders`: matched commands and how much they still emit.
#[derive(Debug, Clone, PartialEq)]
pub struct FilteredOffender {
    pub command_normalized: String,
    pub rule_id: Option<String>,
    pub runs: i64,
    pub total_filtered_bytes: i64,
    /// AVG(filtered_bytes / raw_bytes); `None` when every raw size was 0.
    pub avg_keep_ratio: Option<f64>,
}

/// Row of `v_bypass_rate`: per-rule bypass smell (rules with > 5 runs only).
#[derive(Debug, Clone, PartialEq)]
pub struct BypassRate {
    pub rule_id: Option<String>,
    pub total: i64,
    pub bypassed: i64,
    pub bypass_rate: f64,
}

/// Row of `v_project_savings`: per-project raw-vs-filtered byte totals.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSaving {
    pub project_path: Option<String>,
    pub total_runs: i64,
    pub raw_total: i64,
    pub filtered_total: i64,
    pub bytes_saved: i64,
}

/// Scalar headline aggregate over `bypassed = 0` invocations (ADR 0014 §1 /
/// D-05). Backs the stats headline: total runs, distinct projects, `raw → kept`
/// bytes, and `bytes_saved`. Unlike the per-view row structs this is a single
/// rolled-up row, not a list — see [`overall_totals`].
///
/// `distinct_projects` is `COUNT(DISTINCT project_path)` computed
/// PRE-canonicalization: SQL has no filesystem access, so it cannot resolve
/// symlinks or `..` segments. The headline's displayed "after canonicalization"
/// project count is computed in `stats.rs` from the rolled-up project map, NOT
/// from this field — it is kept available regardless for callers that want the
/// raw stored-path distinct count.
#[derive(Debug, Clone, PartialEq)]
pub struct OverallTotals {
    pub total_runs: i64,
    pub distinct_projects: i64,
    pub raw_total: i64,
    pub kept_total: i64,
    pub bytes_saved: i64,
}

/// Stored raw output as `explain` consumes it: `(stdout, stderr)` BLOBs.
/// Aliased to keep [`fetch_raw_output`]'s return type readable (clippy
/// `type_complexity`).
pub type RawOutputBlobs = (Vec<u8>, Vec<u8>);

/// One `invocations` row as `explain` needs it (D-05).
#[derive(Debug, Clone, PartialEq)]
pub struct InvocationRow {
    pub rule_id: Option<String>,
    pub exit_code: i64,
    pub command_raw: String,
    pub duration_ms: i64,
    pub project_path: Option<String>,
    /// `None` when raw bytes were not stored for this invocation; `explain`
    /// branches on this (D-05 step 3).
    pub raw_output_id: Option<i64>,
}

// ---------------------------------------------------------------------------
// (a) Unfiltered view readers — read the four DB views directly
// ---------------------------------------------------------------------------

/// Read `v_unmatched_offenders` in full (ordered by total_raw_bytes DESC in the
/// view DDL).
pub fn unmatched_offenders(conn: &Connection) -> Result<Vec<UnmatchedOffender>, TrackingError> {
    let mut stmt = conn.prepare(
        "SELECT command_normalized, runs, total_raw_bytes FROM v_unmatched_offenders",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(UnmatchedOffender {
                command_normalized: r.get(0)?,
                runs: r.get(1)?,
                total_raw_bytes: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Read `v_filtered_offenders` in full.
pub fn filtered_offenders(conn: &Connection) -> Result<Vec<FilteredOffender>, TrackingError> {
    let mut stmt = conn.prepare(
        "SELECT command_normalized, rule_id, runs, total_filtered_bytes, avg_keep_ratio
         FROM v_filtered_offenders",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(FilteredOffender {
                command_normalized: r.get(0)?,
                rule_id: r.get(1)?,
                runs: r.get(2)?,
                total_filtered_bytes: r.get(3)?,
                avg_keep_ratio: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Read `v_bypass_rate` in full (view applies `HAVING COUNT(*) > 5`).
pub fn bypass_rate(conn: &Connection) -> Result<Vec<BypassRate>, TrackingError> {
    let mut stmt = conn.prepare(
        "SELECT rule_id, total, bypassed, bypass_rate FROM v_bypass_rate",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(BypassRate {
                rule_id: r.get(0)?,
                total: r.get(1)?,
                bypassed: r.get(2)?,
                bypass_rate: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Read `v_project_savings` in full.
pub fn project_savings(conn: &Connection) -> Result<Vec<ProjectSaving>, TrackingError> {
    let mut stmt = conn.prepare(
        "SELECT project_path, total_runs, raw_total, filtered_total, bytes_saved
         FROM v_project_savings",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(ProjectSaving {
                project_path: r.get(0)?,
                total_runs: r.get(1)?,
                raw_total: r.get(2)?,
                filtered_total: r.get(3)?,
                bytes_saved: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// (b) Filtered re-queries (D-09) — base `invocations` table + WHERE clauses
// ---------------------------------------------------------------------------
//
// The four views expose neither `ts` nor (mostly) `project_path`, so
// --since/--project/--rule filtering re-implements each view body against the
// base table. Filter values are bound via params![]; only the static SQL
// fragments are concatenated. Indexes idx_inv_ts/idx_inv_project/idx_inv_rule
// back the added predicates.

/// Filtered counterpart of `v_unmatched_offenders` (D-09). Filters:
/// `since_cutoff_ms` (keep `ts >= cutoff`) and `project` (`project_path = ?`).
/// `rule` does not apply — unmatched rows have `rule_id IS NULL` by definition.
pub fn filtered_unmatched_offenders(
    conn: &Connection,
    since_cutoff_ms: Option<i64>,
    project: Option<&str>,
) -> Result<Vec<UnmatchedOffender>, TrackingError> {
    let mut sql = String::from(
        "SELECT command_normalized,
                COUNT(*) AS runs,
                SUM(raw_stdout_bytes + raw_stderr_bytes) AS total_raw_bytes
         FROM invocations
         WHERE rule_id IS NULL AND bypassed = 0",
    );
    let mut binds: Vec<&dyn rusqlite::ToSql> = Vec::new();
    let mut n = 0;
    if let Some(cut) = since_cutoff_ms.as_ref() {
        n += 1;
        sql.push_str(&format!(" AND ts >= ?{n}"));
        binds.push(cut);
    }
    if let Some(p) = project.as_ref() {
        n += 1;
        sql.push_str(&format!(" AND project_path = ?{n}"));
        binds.push(p);
    }
    sql.push_str(" GROUP BY command_normalized ORDER BY total_raw_bytes DESC");

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(binds.as_slice(), |r| {
            Ok(UnmatchedOffender {
                command_normalized: r.get(0)?,
                runs: r.get(1)?,
                total_raw_bytes: r.get(2)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Filtered counterpart of `v_filtered_offenders` (D-09). Filters:
/// `since_cutoff_ms`, `project`, `rule`.
pub fn filtered_filtered_offenders(
    conn: &Connection,
    since_cutoff_ms: Option<i64>,
    project: Option<&str>,
    rule: Option<&str>,
) -> Result<Vec<FilteredOffender>, TrackingError> {
    let mut sql = String::from(
        "SELECT command_normalized, rule_id,
                COUNT(*) AS runs,
                SUM(filtered_bytes) AS total_filtered_bytes,
                AVG(CAST(filtered_bytes AS REAL) /
                    NULLIF(raw_stdout_bytes + raw_stderr_bytes, 0)) AS avg_keep_ratio
         FROM invocations
         WHERE rule_id IS NOT NULL AND bypassed = 0",
    );
    let mut binds: Vec<&dyn rusqlite::ToSql> = Vec::new();
    let mut n = 0;
    if let Some(cut) = since_cutoff_ms.as_ref() {
        n += 1;
        sql.push_str(&format!(" AND ts >= ?{n}"));
        binds.push(cut);
    }
    if let Some(p) = project.as_ref() {
        n += 1;
        sql.push_str(&format!(" AND project_path = ?{n}"));
        binds.push(p);
    }
    if let Some(rl) = rule.as_ref() {
        n += 1;
        sql.push_str(&format!(" AND rule_id = ?{n}"));
        binds.push(rl);
    }
    sql.push_str(
        " GROUP BY command_normalized, rule_id ORDER BY total_filtered_bytes DESC",
    );

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(binds.as_slice(), |r| {
            Ok(FilteredOffender {
                command_normalized: r.get(0)?,
                rule_id: r.get(1)?,
                runs: r.get(2)?,
                total_filtered_bytes: r.get(3)?,
                avg_keep_ratio: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Filtered counterpart of `v_bypass_rate` (D-09). Filters:
/// `since_cutoff_ms`, `rule`. Preserves the view's `HAVING COUNT(*) > 5` gate.
pub fn filtered_bypass_rate(
    conn: &Connection,
    since_cutoff_ms: Option<i64>,
    rule: Option<&str>,
) -> Result<Vec<BypassRate>, TrackingError> {
    let mut sql = String::from(
        "SELECT rule_id,
                COUNT(*) AS total,
                SUM(bypassed) AS bypassed,
                CAST(SUM(bypassed) AS REAL) / COUNT(*) AS bypass_rate
         FROM invocations
         WHERE rule_id IS NOT NULL",
    );
    let mut binds: Vec<&dyn rusqlite::ToSql> = Vec::new();
    let mut n = 0;
    if let Some(cut) = since_cutoff_ms.as_ref() {
        n += 1;
        sql.push_str(&format!(" AND ts >= ?{n}"));
        binds.push(cut);
    }
    if let Some(rl) = rule.as_ref() {
        n += 1;
        sql.push_str(&format!(" AND rule_id = ?{n}"));
        binds.push(rl);
    }
    sql.push_str(" GROUP BY rule_id HAVING COUNT(*) > 5 ORDER BY bypass_rate DESC");

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(binds.as_slice(), |r| {
            Ok(BypassRate {
                rule_id: r.get(0)?,
                total: r.get(1)?,
                bypassed: r.get(2)?,
                bypass_rate: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Filtered counterpart of `v_project_savings` (D-09). Filters:
/// `since_cutoff_ms`, `project`.
pub fn filtered_project_savings(
    conn: &Connection,
    since_cutoff_ms: Option<i64>,
    project: Option<&str>,
) -> Result<Vec<ProjectSaving>, TrackingError> {
    let mut sql = String::from(
        "SELECT project_path,
                COUNT(*) AS total_runs,
                SUM(raw_stdout_bytes + raw_stderr_bytes) AS raw_total,
                SUM(filtered_bytes) AS filtered_total,
                SUM(raw_stdout_bytes + raw_stderr_bytes - filtered_bytes) AS bytes_saved
         FROM invocations
         WHERE bypassed = 0",
    );
    let mut binds: Vec<&dyn rusqlite::ToSql> = Vec::new();
    let mut n = 0;
    if let Some(cut) = since_cutoff_ms.as_ref() {
        n += 1;
        sql.push_str(&format!(" AND ts >= ?{n}"));
        binds.push(cut);
    }
    if let Some(p) = project.as_ref() {
        n += 1;
        sql.push_str(&format!(" AND project_path = ?{n}"));
        binds.push(p);
    }
    sql.push_str(" GROUP BY project_path ORDER BY bytes_saved DESC");

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(binds.as_slice(), |r| {
            Ok(ProjectSaving {
                project_path: r.get(0)?,
                total_runs: r.get(1)?,
                raw_total: r.get(2)?,
                filtered_total: r.get(3)?,
                bytes_saved: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// (b2) Overall headline aggregate (ADR 0014 §1 / D-05) — scalar collapse over
//      the base `invocations` table, no GROUP BY. D-01 forbids a `v_overall`
//      view, so these read the base table directly. Every SUM is wrapped in
//      COALESCE(..., 0) so a SUM over zero rows yields 0 rather than NULL.
// ---------------------------------------------------------------------------

/// Unfiltered headline aggregate over all `bypassed = 0` invocations.
/// `kept_total == SUM(filtered_bytes)`; `bytes_saved == raw_total - kept_total`.
pub fn overall_totals(conn: &Connection) -> Result<OverallTotals, TrackingError> {
    let mut stmt = conn.prepare(
        "SELECT COUNT(*) AS total_runs,
                COUNT(DISTINCT project_path) AS distinct_projects,
                COALESCE(SUM(raw_stdout_bytes + raw_stderr_bytes), 0) AS raw_total,
                COALESCE(SUM(filtered_bytes), 0) AS kept_total,
                COALESCE(SUM(raw_stdout_bytes + raw_stderr_bytes - filtered_bytes), 0)
                    AS bytes_saved
         FROM invocations
         WHERE bypassed = 0",
    )?;
    // A scalar aggregate always returns EXACTLY one row (all-zeros on an empty
    // table thanks to the COALESCEs), so use query_row — not query_map().collect()
    // like every other reader in this module.
    let row = stmt.query_row([], |r| {
        Ok(OverallTotals {
            total_runs: r.get(0)?,
            distinct_projects: r.get(1)?,
            raw_total: r.get(2)?,
            kept_total: r.get(3)?,
            bytes_saved: r.get(4)?,
        })
    })?;
    Ok(row)
}

/// Filtered counterpart of [`overall_totals`] (D-09 re-query shape). Filters:
/// `since_cutoff_ms` (keep `ts >= cutoff`) and `project` (`project_path = ?`).
/// The headline spans matched AND unmatched runs (D-05), so there is no
/// `rule_id` predicate. A filter matching zero rows returns an all-zero
/// `OverallTotals` (COALESCE + query_row), never NULL/Err.
pub fn filtered_overall_totals(
    conn: &Connection,
    since_cutoff_ms: Option<i64>,
    project: Option<&str>,
) -> Result<OverallTotals, TrackingError> {
    let mut sql = String::from(
        "SELECT COUNT(*) AS total_runs,
                COUNT(DISTINCT project_path) AS distinct_projects,
                COALESCE(SUM(raw_stdout_bytes + raw_stderr_bytes), 0) AS raw_total,
                COALESCE(SUM(filtered_bytes), 0) AS kept_total,
                COALESCE(SUM(raw_stdout_bytes + raw_stderr_bytes - filtered_bytes), 0)
                    AS bytes_saved
         FROM invocations
         WHERE bypassed = 0",
    );
    let mut binds: Vec<&dyn rusqlite::ToSql> = Vec::new();
    let mut n = 0;
    if let Some(cut) = since_cutoff_ms.as_ref() {
        n += 1;
        sql.push_str(&format!(" AND ts >= ?{n}"));
        binds.push(cut);
    }
    if let Some(p) = project.as_ref() {
        n += 1;
        sql.push_str(&format!(" AND project_path = ?{n}"));
        binds.push(p);
    }

    let mut stmt = conn.prepare(&sql)?;
    // Scalar aggregate → exactly one row; query_row, not query_map().collect().
    let row = stmt.query_row(binds.as_slice(), |r| {
        Ok(OverallTotals {
            total_runs: r.get(0)?,
            distinct_projects: r.get(1)?,
            raw_total: r.get(2)?,
            kept_total: r.get(3)?,
            bytes_saved: r.get(4)?,
        })
    })?;
    Ok(row)
}

// ---------------------------------------------------------------------------
// (c) explain lookups (D-05)
// ---------------------------------------------------------------------------

/// Fetch a single `invocations` row by id for `explain`. Returns `Ok(None)`
/// when no row has that id (a normal "id not found" outcome, not an error).
pub fn fetch_invocation(
    conn: &Connection,
    id: i64,
) -> Result<Option<InvocationRow>, TrackingError> {
    let mut stmt = conn.prepare(
        "SELECT rule_id, exit_code, command_raw, duration_ms, project_path, raw_output_id
         FROM invocations WHERE id = ?1",
    )?;
    let mut rows = stmt.query(params![id])?;
    match rows.next()? {
        Some(r) => Ok(Some(InvocationRow {
            rule_id: r.get(0)?,
            exit_code: r.get(1)?,
            command_raw: r.get(2)?,
            duration_ms: r.get(3)?,
            project_path: r.get(4)?,
            raw_output_id: r.get(5)?,
        })),
        None => Ok(None),
    }
}

/// Fetch the stored stdout/stderr BLOBs for a `raw_outputs` row by id.
/// Returns `Ok(None)` when no row has that id (e.g. the BLOB aged out of the
/// 3-day raw-outputs retention while the `invocations` row survives).
pub fn fetch_raw_output(
    conn: &Connection,
    raw_output_id: i64,
) -> Result<Option<RawOutputBlobs>, TrackingError> {
    let mut stmt =
        conn.prepare("SELECT stdout, stderr FROM raw_outputs WHERE id = ?1")?;
    let mut rows = stmt.query(params![raw_output_id])?;
    match rows.next()? {
        Some(r) => {
            // stdout/stderr are nullable BLOB columns; coalesce NULL to empty.
            let stdout: Option<Vec<u8>> = r.get(0)?;
            let stderr: Option<Vec<u8>> = r.get(1)?;
            Ok(Some((stdout.unwrap_or_default(), stderr.unwrap_or_default())))
        }
        None => Ok(None),
    }
}
