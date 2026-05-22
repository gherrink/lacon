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
    // WR-02: map SELECT failures to this command's own error channel + a
    // deliberate exit code, instead of letting a TrackingError::Sqlite escape via
    // `?` -> anyhow (which would print the internal "tracking: sqlite ..." text
    // and never reach the chosen exit code). Matches the open-failure handling
    // above and doctor's blanket-mapped posture (T-04-10).
    let row = match query::fetch_invocation(&conn, id_i64) {
        Ok(Some(r)) => r,
        Ok(None) => {
            eprintln!("lacon explain: no tracked invocations found");
            return Ok(1);
        }
        Err(e) => {
            eprintln!("lacon explain: query failed: {e}");
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
    let (stdout, stderr) = match query::fetch_raw_output(&conn, raw_output_id) {
        // WR-02: same posture as fetch_invocation above — a SELECT error maps to
        // the command's own channel + exit 1, not a raw anyhow propagation.
        Ok(Some(blobs)) => blobs,
        Ok(None) => {
            eprintln!(
                "lacon explain: stored raw output for invocation {id_i64} is no longer \
                 available (it likely aged out of the raw-outputs retention window)."
            );
            return Ok(1);
        }
        Err(e) => {
            eprintln!("lacon explain: query failed: {e}");
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
                // Replaying STORED bytes never re-captures (capture_raw stays
                // false): explain feeds previously-saved bytes through the
                // pipeline, it does not spawn a fresh subprocess.
                ..Default::default()
            };
            let mut runner = Runner::new(resolved, options);
            // WR-04: the stored columns are i64 (`INTEGER`); guard the casts so a
            // tampered/corrupt row cannot silently flip the replayed branch. An
            // out-of-i32-range exit_code is treated as a FAILURE (nonzero) rather
            // than truncated — truncation could turn a real nonzero exit into 0
            // and replay the success pipeline instead of `on_error` (ADR-0010 /
            // Phase 6 SC3 branch fidelity). Note: this only protects against a
            // value outside i32; a stored nonzero that still fits i32 already
            // selects the on_error branch correctly.
            let exit_code = exit_code_from_stored(row.exit_code);
            let duration_ms = u64::try_from(row.duration_ms).unwrap_or(0);
            match runner.filter_bytes(
                &merged,
                exit_code,
                duration_ms,
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

/// WR-04: convert a DB-stored `i64` exit code to the `i32` the runner expects,
/// guarding against a tampered/corrupt row. An out-of-`i32`-range value is
/// treated as a FAILURE (`1`), never truncated — truncation could turn a real
/// nonzero exit into `0` and replay the success pipeline instead of `on_error`
/// (ADR-0010 / Phase 6 SC3 branch fidelity). Values that fit `i32` pass through
/// unchanged, so the zero-vs-nonzero branch selection is preserved exactly.
fn exit_code_from_stored(stored: i64) -> i32 {
    i32::try_from(stored).unwrap_or_else(|_| {
        eprintln!("lacon explain: stored exit_code {stored} is out of range; treating as failure");
        1
    })
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
/// side is padded with blanks.
///
/// Terminal-safety contract (T-04-09 / Phase 6 SC3):
/// - The LEFT (raw) column reproduces stored bytes verbatim — byte-fidelity is
///   the contract, so escaping is intentionally NOT applied there.
/// - The RIGHT (filtered) column is the documented "safe-to-read view". WR-01:
///   the filtered output is NOT guaranteed to be sanitized (unmatched runs pass
///   raw bytes straight through, and rules without `strip_ansi` keep control
///   bytes), so a hostile stored build log could otherwise inject terminal
///   sequences via this column too. We therefore neutralize C0/C1 control and
///   ESC bytes on the right column unconditionally, making the code honour the
///   "safe view" claim without touching the raw column's byte-fidelity.
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
        // WR-01: sanitize the RIGHT column so the "safe view" claim actually
        // holds even for unmatched runs / rules without strip_ansi.
        let right_display = sanitize_for_display(right);
        println!("{left_display} | {right_display}");
    }
}

/// WR-01: neutralize terminal-control bytes for the filtered ("safe view")
/// column. Escapes C0 control chars (except `\t`, which is benign for display)
/// and C1 / DEL so embedded ESC/CSI/OSC sequences from a hostile stored build
/// log cannot drive the user's terminal (cursor moves, title rewrites, OSC 52
/// clipboard writes). Printable chars — including non-ASCII text — pass through
/// unchanged so legitimate filtered output stays readable. Applied ONLY to the
/// right column; the raw column keeps its byte-fidelity contract (Phase 6 SC3).
fn sanitize_for_display(s: &str) -> String {
    s.chars()
        .map(|c| {
            // Keep tab (column-friendly) and any printable/non-control char.
            // `is_control()` covers C0 (incl. ESC 0x1B), DEL (0x7F), and C1
            // (0x80..=0x9F) — exactly the terminal-driving range we must escape.
            if c == '\t' || !c.is_control() {
                c.to_string()
            } else {
                c.escape_default().to_string()
            }
        })
        .collect()
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
    use super::{exit_code_from_stored, pad_or_truncate, sanitize_for_display, split_lines};

    // WR-01: the filtered ("safe view") column must neutralize terminal-control
    // sequences. ESC (0x1B) starts every CSI/OSC escape; a raw build log could
    // embed cursor moves / title rewrites / OSC 52 clipboard writes that the
    // "safe view" claim says are not present.
    #[test]
    fn sanitize_escapes_ansi_and_control_bytes() {
        // CSI red color + reset around text.
        let injected = "\x1b[31mERR\x1b[0m";
        let safe = sanitize_for_display(injected);
        assert!(!safe.contains('\x1b'), "ESC must not survive: {safe:?}");
        assert!(safe.contains("ERR"));
        // OSC 52 clipboard write (ESC ] 52 ; ... BEL).
        let osc = "\x1b]52;c;ZXZpbA==\x07";
        let safe_osc = sanitize_for_display(osc);
        assert!(!safe_osc.contains('\x1b'));
        assert!(!safe_osc.contains('\x07'));
    }

    // WR-01: legitimate text — including tabs and non-ASCII — must pass through
    // unchanged so the safe view stays readable.
    #[test]
    fn sanitize_preserves_printable_and_tab() {
        assert_eq!(sanitize_for_display("hello\tworld"), "hello\tworld");
        assert_eq!(sanitize_for_display("café — 日本語"), "café — 日本語");
        assert_eq!(sanitize_for_display(""), "");
    }

    // WR-04: in-range values pass through unchanged so the zero-vs-nonzero
    // branch selection (success vs on_error) is preserved exactly.
    #[test]
    fn exit_code_in_range_passes_through() {
        assert_eq!(exit_code_from_stored(0), 0);
        assert_eq!(exit_code_from_stored(1), 1);
        assert_eq!(exit_code_from_stored(127), 127);
        assert_eq!(exit_code_from_stored(i32::MAX as i64), i32::MAX);
        assert_eq!(exit_code_from_stored(i32::MIN as i64), i32::MIN);
    }

    // WR-04: an out-of-i32-range stored value must be treated as a FAILURE,
    // never silently truncated. A naive `as i32` cast of (i32::MAX + 1) wraps to
    // i32::MIN; truncation of 2^32 (= 0x1_0000_0000) would wrap to 0 and flip a
    // failed run onto the success pipeline. We map it to 1 (nonzero) instead so
    // the on_error branch is still selected.
    #[test]
    fn exit_code_out_of_range_becomes_failure() {
        assert_eq!(exit_code_from_stored(i32::MAX as i64 + 1), 1);
        assert_eq!(exit_code_from_stored(i32::MIN as i64 - 1), 1);
        // 2^32: a naive `as i32` truncation would yield 0 (success); guard -> 1.
        assert_eq!(exit_code_from_stored(0x1_0000_0000), 1);
        assert_eq!(exit_code_from_stored(i64::MAX), 1);
    }

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
