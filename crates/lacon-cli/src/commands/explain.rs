//! lacon explain subcommand: re-derive filtered output from a tracked
//! invocation's STORED raw bytes and render a raw-vs-filtered side-by-side.
//!
//! # Flow (D-05, 6 steps)
//! 1. Parse the id (`i64`) — never `unwrap()` on user input (T-04-07).
//! 2. Resolve DB path; absent file or missing row → "no tracked invocations
//!    found" (D-03).
//! 3. Open read-only via `tracking::open_readonly` (D-02); fetch the row.
//! 4. If `raw_output_id` is NULL → clear error pointing at `store_raw_outputs`
//!    (SC2 — retention was disabled at the time of this invocation).
//! 5. Load the stored stdout/stderr BLOBs and merge them (stdout then stderr,
//!    matching v1's single merged-stream model).
//! 6. Resolve the rule and replay the bytes through `Runner::filter_bytes`
//!    (exit-code branch chosen from the stored exit code — ADR-0010), then
//!    render a hand-rolled two-column raw|filtered view (D-06, no diff crate).
//!
//! # Design boundary (D-01)
//! All SQL lives in `lacon_core::tracking::query`; this command never inlines a
//! query and keeps `rusqlite` dev-only.

use lacon_core::rules::loader::RuleLoader;
use lacon_core::runtime::{RunOptions, Runner};
use lacon_core::tracking::{self, query};

/// Exit codes (documented for the SUMMARY): 0 success, 2 bad CLI input
/// (non-numeric id), 1 operational failure (no DB / row / raw output / rule).
pub fn execute(id: String) -> anyhow::Result<i32> {
    // ─── Step 1: parse the id safely (T-04-07; never unwrap user input) ─────
    let id_i64: i64 = match id.parse::<i64>() {
        Ok(n) => n,
        Err(_) => {
            eprintln!("lacon explain: invalid invocation id `{id}` (expected a number)");
            return Ok(2);
        }
    };

    // ─── Step 2: resolve DB path; absent file → not found (D-03) ────────────
    let db_path = match tracking::Tracker::xdg_db_path() {
        Some(p) => p,
        None => {
            eprintln!("lacon explain: could not resolve the XDG data directory");
            return Ok(1);
        }
    };
    if !db_path.exists() {
        eprintln!("lacon explain: no tracked invocations found");
        return Ok(1);
    }

    // ─── Step 3: open read-only and fetch the invocation row ────────────────
    let conn = match tracking::open_readonly(&db_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("lacon explain: could not open history.db: {e}");
            return Ok(1);
        }
    };
    let row = match query::fetch_invocation(&conn, id_i64)? {
        Some(r) => r,
        None => {
            eprintln!("lacon explain: no tracked invocations found");
            return Ok(1);
        }
    };

    // ─── Step 4: raw retention check (SC2 — D-05 step 3) ────────────────────
    let raw_output_id = match row.raw_output_id {
        Some(rid) => rid,
        None => {
            eprintln!(
                "lacon explain: invocation {id_i64} has no stored raw output \
                 (store_raw_outputs was disabled at the time of this run). Enable \
                 store_raw_outputs in .lacon/config.yaml to capture raw bytes for \
                 future invocations; past runs cannot be replayed."
            );
            return Ok(1);
        }
    };

    // ─── Step 5: load the stored BLOBs and merge (stdout then stderr) ───────
    let (stdout, stderr) = match query::fetch_raw_output(&conn, raw_output_id)? {
        Some(blobs) => blobs,
        None => {
            eprintln!(
                "lacon explain: stored raw output for invocation {id_i64} is no longer \
                 available (it likely aged out of the raw-outputs retention window)."
            );
            return Ok(1);
        }
    };
    let mut merged: Vec<u8> = stdout;
    merged.extend_from_slice(&stderr);

    // ─── Step 6a: resolve the rule from the stored project context ──────────
    // The replay needs the rule that originally matched. Unmatched invocations
    // (rule_id NULL) have no pipeline to replay — explain still shows the raw
    // bytes as the "filtered" output (passthrough), which is what actually
    // reached the model.
    let project_path_buf = row.project_path.as_ref().map(std::path::PathBuf::from);

    let filtered: Vec<String> = match row.rule_id.as_deref() {
        Some(rule_id) => {
            let mut loader = RuleLoader::new(project_path_buf.clone());
            let resolved = match loader.resolve(rule_id) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("lacon explain: could not resolve rule `{rule_id}`: {e}");
                    return Ok(1);
                }
            };
            let options = RunOptions {
                project_path: project_path_buf,
                extra_env: Default::default(),
            };
            let mut runner = Runner::new(resolved, options);
            match runner.filter_bytes(
                &merged,
                row.exit_code as i32,
                row.duration_ms as u64,
                &row.command_raw,
                row.project_path.clone(),
            ) {
                Ok(lines) => lines,
                Err(e) => {
                    eprintln!("lacon explain: replay failed: {e}");
                    return Ok(1);
                }
            }
        }
        None => {
            // No rule matched at run time → raw bytes passed through unfiltered.
            split_lines(&merged)
        }
    };

    // ─── Step 6b: hand-rolled two-column raw|filtered render (D-06) ──────────
    let raw_lines = split_lines(&merged);
    render_side_by_side(&row.command_raw, row.exit_code, &raw_lines, &filtered);

    Ok(0)
}

/// Split merged bytes into lines (lossy UTF-8), mirroring `Runner::filter_bytes`.
fn split_lines(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|&b| b == b'\n')
        .map(|l| String::from_utf8_lossy(l).into_owned())
        .collect()
}

/// Hand-rolled two-column side-by-side (D-06): no LCS/Myers, no diff crate.
/// The left column is padded to a fixed width; rows are zipped and the shorter
/// side is padded with blanks. The raw column reproduces stored bytes verbatim
/// (byte-fidelity is the contract per Phase 6 SC3 / T-04-09); the filtered
/// column is the safe-to-read view.
fn render_side_by_side(command: &str, exit_code: i64, raw: &[String], filtered: &[String]) {
    const LEFT_WIDTH: usize = 60;

    println!("command: {command}");
    println!("exit_code: {exit_code}");
    println!();
    println!("{:<width$} | filtered", "raw", width = LEFT_WIDTH);
    println!("{} | {}", "-".repeat(LEFT_WIDTH), "-".repeat(8));

    let rows = raw.len().max(filtered.len());
    for i in 0..rows {
        let left = raw.get(i).map(String::as_str).unwrap_or("");
        let right = filtered.get(i).map(String::as_str).unwrap_or("");
        // Truncate/pad the left column to a fixed width for alignment. Use char
        // count for the pad budget so multibyte chars do not under-pad.
        let left_display = pad_or_truncate(left, LEFT_WIDTH);
        println!("{left_display} | {right}");
    }
}

/// Pad a string with spaces to `width` chars, or truncate (with a trailing `…`)
/// if it is longer. Operates on chars so the visual width is stable for ASCII.
fn pad_or_truncate(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len <= width {
        let mut out = String::with_capacity(width);
        out.push_str(s);
        for _ in len..width {
            out.push(' ');
        }
        out
    } else if width == 0 {
        String::new()
    } else {
        let mut out: String = s.chars().take(width - 1).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::{pad_or_truncate, split_lines};

    #[test]
    fn pad_short_string_to_width() {
        let p = pad_or_truncate("hi", 5);
        assert_eq!(p, "hi   ");
        assert_eq!(p.chars().count(), 5);
    }

    #[test]
    fn truncate_long_string_with_ellipsis() {
        let p = pad_or_truncate("hello world", 5);
        assert_eq!(p.chars().count(), 5);
        assert!(p.ends_with('…'));
    }

    #[test]
    fn split_lines_lossy_on_newlines() {
        let lines = split_lines(b"a\nb\nc");
        assert_eq!(lines, vec!["a", "b", "c"]);
    }
}
