//! lacon stats subcommand: summarize tracking data from the four reporting
//! views, with `--project`/`--since`/`--rule` filters (D-09, D-10) and a
//! graceful empty-DB path (D-03).
//!
//! # Design boundary (D-01)
//! All SQL lives in `lacon_core::tracking::query`. This command opens the DB
//! read-only via `tracking::open_readonly` (D-02) and calls the typed view
//! readers / filtered re-queries — it never inlines a query and keeps
//! `rusqlite` a dev-only dependency.
//!
//! # Filters (D-09 / D-10)
//! When any of `--project`/`--since`/`--rule` is set, the affected sections
//! read the base-table filtered re-queries; otherwise they read the views
//! directly. `--since` accepts relative forms only (`Nd`/`Nh`/`Nm`); a
//! malformed value errors with exit code 2 and no panic.
//!
//! # Output
//! Plain text, snapshot-testable (D-11) — no color dependency.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use lacon_core::tracking::{self, query};

/// Exit codes (documented for the SUMMARY): 0 success, 2 bad CLI input
/// (malformed `--since`). The empty-DB path is a success (0), not an error.
pub fn execute(
    project: Option<PathBuf>,
    since: Option<String>,
    rule: Option<String>,
) -> anyhow::Result<i32> {
    // ─── Resolve --since to an absolute cutoff in unix MILLISECONDS (D-10) ───
    // ts is unix ms (tracking-data-model.md); cutoff = now_ms - n*unit_ms.
    let cutoff_ms: Option<i64> = match since.as_deref() {
        None => None,
        Some(s) => match parse_since(s) {
            Ok(window_ms) => {
                let now_ms = match SystemTime::now().duration_since(UNIX_EPOCH) {
                    Ok(d) => d.as_millis() as i64,
                    Err(_) => {
                        eprintln!("lacon stats: system time is before the unix epoch");
                        return Ok(2);
                    }
                };
                Some(now_ms - window_ms)
            }
            Err(msg) => {
                eprintln!("lacon stats: invalid --since `{s}`: {msg}");
                return Ok(2);
            }
        },
    };

    let project_str: Option<String> = project.as_ref().map(|p| p.to_string_lossy().into_owned());
    let project_ref: Option<&str> = project_str.as_deref();
    let rule_ref: Option<&str> = rule.as_deref();
    let filtered = cutoff_ms.is_some() || project_ref.is_some() || rule_ref.is_some();

    // ─── DB path resolve + graceful empty-DB skip (D-03, Pitfall 4) ─────────
    // Check existence BEFORE opening: open_readonly errors on an absent file
    // (it never CREATEs), so the fresh-machine state must be detected first.
    let db_path = match tracking::Tracker::xdg_db_path() {
        Some(p) => p,
        None => {
            eprintln!("lacon stats: could not resolve the XDG data directory");
            return Ok(2);
        }
    };

    if !db_path.exists() {
        print_empty();
        return Ok(0);
    }

    let conn = match tracking::open_readonly(&db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("lacon stats: could not open history.db: {e}");
            return Ok(1);
        }
    };

    // ─── Section 1: Unmatched offenders ─────────────────────────────────────
    println!("Unmatched offenders");
    let unmatched = if filtered {
        query::filtered_unmatched_offenders(&conn, cutoff_ms, project_ref)?
    } else {
        query::unmatched_offenders(&conn)?
    };
    if unmatched.is_empty() {
        println!("  no data yet");
    } else {
        for r in &unmatched {
            println!(
                "  {}  runs={}  raw_bytes={}",
                r.command_normalized, r.runs, r.total_raw_bytes
            );
        }
    }
    println!();

    // ─── Section 2: Filtered offenders ──────────────────────────────────────
    println!("Filtered offenders");
    let f_offenders = if filtered {
        query::filtered_filtered_offenders(&conn, cutoff_ms, project_ref, rule_ref)?
    } else {
        query::filtered_offenders(&conn)?
    };
    if f_offenders.is_empty() {
        println!("  no data yet");
    } else {
        for r in &f_offenders {
            let ratio = r
                .avg_keep_ratio
                .map(|v| format!("{v:.2}"))
                .unwrap_or_else(|| "-".to_string());
            println!(
                "  {}  rule={}  runs={}  filtered_bytes={}  keep_ratio={}",
                r.command_normalized,
                r.rule_id.as_deref().unwrap_or("-"),
                r.runs,
                r.total_filtered_bytes,
                ratio
            );
        }
    }
    println!();

    // ─── Section 3: Bypass rates ────────────────────────────────────────────
    println!("Bypass rates");
    let bypass = if filtered {
        query::filtered_bypass_rate(&conn, cutoff_ms, rule_ref)?
    } else {
        query::bypass_rate(&conn)?
    };
    if bypass.is_empty() {
        println!("  no data yet");
    } else {
        for r in &bypass {
            println!(
                "  rule={}  total={}  bypassed={}  rate={:.2}",
                r.rule_id.as_deref().unwrap_or("-"),
                r.total,
                r.bypassed,
                r.bypass_rate
            );
        }
    }
    println!();

    // ─── Section 4: Per-project savings ─────────────────────────────────────
    println!("Per-project savings");
    let savings = if filtered {
        query::filtered_project_savings(&conn, cutoff_ms, project_ref)?
    } else {
        query::project_savings(&conn)?
    };
    if savings.is_empty() {
        println!("  no data yet");
    } else {
        for r in &savings {
            println!(
                "  {}  runs={}  raw={}  filtered={}  saved={}",
                r.project_path.as_deref().unwrap_or("-"),
                r.total_runs,
                r.raw_total,
                r.filtered_total,
                r.bytes_saved
            );
        }
    }

    Ok(0)
}

/// Parse a relative `--since` value into a window in milliseconds.
///
/// Grammar (v1, D-10): an unsigned integer prefix followed by a single unit
/// suffix — `d` (days), `h` (hours), `m` (minutes). Combined forms like
/// `1d12h` are out of scope for v1 (left to discretion); reject them clearly.
fn parse_since(s: &str) -> Result<i64, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty value; use a form like 7d, 24h, or 30m".to_string());
    }
    let (num_part, unit) = s.split_at(s.len() - 1);
    let unit_ms: i64 = match unit {
        "d" => 86_400_000,
        "h" => 3_600_000,
        "m" => 60_000,
        other => {
            return Err(format!(
                "unknown unit `{other}`; use d (days), h (hours), or m (minutes)"
            ))
        }
    };
    let n: i64 = num_part
        .parse()
        .map_err(|_| format!("`{num_part}` is not a whole number"))?;
    if n < 0 {
        return Err("the count must be non-negative".to_string());
    }
    n.checked_mul(unit_ms)
        .ok_or_else(|| "the window is too large".to_string())
}

/// Fresh-machine output: a "no data yet" line per section, exit 0 (D-03).
fn print_empty() {
    for header in [
        "Unmatched offenders",
        "Filtered offenders",
        "Bypass rates",
        "Per-project savings",
    ] {
        println!("{header}");
        println!("  no data yet");
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::parse_since;

    #[test]
    fn parse_since_days_hours_minutes() {
        assert_eq!(parse_since("7d").unwrap(), 7 * 86_400_000);
        assert_eq!(parse_since("24h").unwrap(), 24 * 3_600_000);
        assert_eq!(parse_since("30m").unwrap(), 30 * 60_000);
    }

    #[test]
    fn parse_since_rejects_bad_unit() {
        assert!(parse_since("7x").is_err());
        assert!(parse_since("abc").is_err());
    }

    #[test]
    fn parse_since_rejects_empty() {
        assert!(parse_since("").is_err());
    }
}
