---
phase: 04-cli-completion-stats-explain-doctor
reviewed: 2026-05-22T00:35:00Z
depth: standard
files_reviewed: 15
files_reviewed_list:
  - crates/lacon-cli/src/commands/doctor.rs
  - crates/lacon-cli/src/commands/explain.rs
  - crates/lacon-cli/src/commands/stats.rs
  - crates/lacon-cli/src/main.rs
  - crates/lacon-cli/tests/cli_doctor.rs
  - crates/lacon-cli/tests/cli_explain.rs
  - crates/lacon-cli/tests/cli_stats.rs
  - crates/lacon-cli/tests/cli_surface.rs
  - crates/lacon-cli/tests/tracking_coldstart.rs
  - crates/lacon-core/src/runtime/mod.rs
  - crates/lacon-core/src/tracking/mod.rs
  - crates/lacon-core/src/tracking/query.rs
  - crates/lacon-core/tests/runtime_filter_bytes.rs
  - crates/lacon-core/tests/tracking_query.rs
  - crates/lacon-core/tests/wave0_smoke.rs
findings:
  critical: 0
  warning: 5
  info: 5
  total: 10
status: issues_found
---

# Phase 4: Code Review Report

**Reviewed:** 2026-05-22T00:35:00Z
**Depth:** standard
**Files Reviewed:** 15
**Status:** issues_found

## Summary

Phase 4 adds three read-only CLI commands (`stats`, `explain`, `doctor`), a tracking READ API (`query.rs` + `open_readonly`), and a subprocess-free byte-replay path (`Runner::filter_bytes`). The implementation is careful and the highest-risk areas hold up under adversarial inspection:

- **SQL injection (T-04-01): clean.** Every dynamic query in `query.rs` concatenates only static SQL fragments plus `?N` positional placeholders; all filter values (`--since` cutoff, `--project`, `--rule`) flow through `params!`/`binds`. Placeholder numbering (`n` incremented before each push, binds pushed in matching order) is consistent and 1-indexed correctly. No value is ever string-interpolated into SQL.
- **Read-only / non-mutating DB access (D-02, T-04-02): clean.** `open_readonly` uses `SQLITE_OPEN_READ_ONLY` without `CREATE`, omits the WAL write, and the three commands check `db_path.exists()` before opening. The cold-start lazy-open invariant for `doctor` is enforced by `tracking_coldstart.rs` (no `Tracker::open`, no DB file created).
- **Six-command surface cap: clean.** `cli.rs` declares exactly six subcommands; `cli_surface.rs` locks the count and adds forbidden-subcommand assertions.
- **Argument parsing: no panics.** `explain` parses the id with `match … parse::<i64>()` (no `unwrap`); `stats` `parse_since` rejects bad units/empty/overflow with `checked_mul`.

No BLOCKER-tier defects were found. The findings below are robustness, error-posture, and correctness-of-claim issues. The most material is the gap between the documented `explain` terminal-safety mitigation and what the code actually guarantees (WR-01).

No structural-findings block was provided with this review.

## Warnings

### WR-01: `explain` "filtered column is the safe-to-read view" guarantee does not hold for unmatched runs or rules without `strip_ansi`

**File:** `crates/lacon-cli/src/commands/explain.rs:101-134, 156-174`
**Issue:** The accepted design (04-03-PLAN.md T-04-09, 04-03-SUMMARY.md) renders the raw column verbatim *because* the filtered column is supposed to be the safe-to-read alternative. But the filtered column is only safe when the matched rule actually strips control sequences. Two paths render attacker-influenced control/ANSI bytes into the filtered column too:

1. **Unmatched invocation (`rule_id` NULL):** the `None` branch sets `filtered = split_lines(&merged)` — i.e. the filtered column *is* the raw bytes, verbatim. Both columns then carry any stored ESC/CSI/OSC sequences straight to the terminal.
2. **Matched rule without `strip_ansi`:** rules are not required to include `strip_ansi` (it is one optional primitive in `filter-rule-schema.md`). A rule that only does `drop_regex`/`keep_tail` passes control bytes through unchanged.

The stored bytes originate from arbitrary subprocess output captured on a prior run, so a hostile build log can embed terminal-manipulation sequences (cursor moves, title rewrites, clipboard OSC 52 on some emulators). The doc claims the filtered side is the mitigation; the code does not enforce that claim.

**Fix:** Either (a) sanitize the *filtered/right* column unconditionally before printing (the right column is explicitly the "safe view", so escaping it does not break the byte-fidelity contract that only applies to the left/raw column), e.g. escape C0/C1 control bytes:
```rust
fn sanitize_for_display(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() && c != '\t' { c.escape_default().to_string() } else { c.to_string() })
        .collect()
}
// in render_side_by_side, apply to the RIGHT column only:
let right = sanitize_for_display(filtered.get(i).map(String::as_str).unwrap_or(""));
```
or (b) downgrade the SUMMARY/PLAN wording so it no longer claims the filtered column is universally safe, and document the unmatched/no-strip_ansi caveat explicitly.

### WR-02: Read-path SQL errors propagate as raw `anyhow` errors, contradicting the documented "never surfaces a raw error" posture

**File:** `crates/lacon-cli/src/commands/explain.rs:58,81`; `crates/lacon-cli/src/commands/stats.rs:82-84,101-103,128-131,150-153`; `crates/lacon-cli/src/main.rs:15-19`
**Issue:** `explain` and `stats` use `?` on the `query::*` calls (e.g. `query::fetch_invocation(&conn, id_i64)?`, `query::unmatched_offenders(&conn)?`). A `TrackingError::Sqlite` from any of these bubbles up through `execute() -> anyhow::Result<i32>` into `main`, which returns `anyhow::Result<()>`. The default anyhow reporter then prints the error and the `std::process::exit(exit_code)` line is never reached. This diverges from the documented posture for these commands: `doctor` explicitly maps every IO/parse error to a printed line + exit code (T-04-10), and `explain`/`stats` map *open* failures to clean `eprintln!` + `Ok(1)` — but a SELECT failure on an already-opened handle escapes that discipline and surfaces the internal `tracking: sqlite … failed: …` text via anyhow instead of the command's own `lacon explain:` / `lacon stats:` prefix. The exit code in that case is anyhow's default (1), not a deliberately chosen code.
**Fix:** Map the query errors to the command's own error channel for consistency with the rest of the file, e.g.:
```rust
let row = match query::fetch_invocation(&conn, id_i64) {
    Ok(r) => r,
    Err(e) => { eprintln!("lacon explain: query failed: {e}"); return Ok(1); }
};
```
Apply the same pattern to each `query::*?` in `stats.rs`.

### WR-03: `stats --project` requires a byte-exact path string match against the stored cwd, silently yielding empty results on any path-form mismatch

**File:** `crates/lacon-cli/src/commands/stats.rs:50` and `crates/lacon-core/src/tracking/query.rs:208,252,347`
**Issue:** `stats` converts `--project` via `p.to_string_lossy()` and the query binds it into `project_path = ?N`. The stored `project_path` comes from `std::env::current_dir().ok()` at run time (`run.rs:25`), which is the absolute, non-canonicalized cwd. A user who runs `lacon stats --project .`, `--project ./` , `--project /home/me/proj/` (trailing slash), a relative path, or a symlinked path will get an exact-string mismatch and a misleading "no data yet" for every section, with no error. There is no normalization on either the write or the read side. This is a quiet correctness/usability trap: the filter appears to work (exit 0) but silently drops everything.
**Fix:** Canonicalize the `--project` argument before binding (and ideally canonicalize at write time in `run.rs` too, so both sides agree):
```rust
let project_str = project.as_ref().map(|p| {
    std::fs::canonicalize(p)
        .unwrap_or_else(|_| p.clone())
        .to_string_lossy()
        .into_owned()
});
```
At minimum, document that `--project` must match the stored absolute path verbatim, and consider printing a hint when a `--project` filter returns zero rows but the unfiltered query would not.

### WR-04: `explain` casts DB-stored `i64` exit/duration to `i32`/`u64` without range checks, allowing silent value corruption from a tampered DB

**File:** `crates/lacon-cli/src/commands/explain.rs:118-121`
**Issue:** `row.exit_code as i32` and `row.duration_ms as u64` are unchecked casts from the `i64` columns. The DB is local and user-owned, so this is not a remote attack surface, but the column is `INTEGER` (i64) and nothing constrains its range. A row with `exit_code` outside `i32` range truncates to a wrong code (e.g. could turn a non-zero exit into `0`, flipping `filter_bytes` onto the *success* pipeline instead of `on_error` — a fidelity break, the very thing Phase 6 SC3 / the branch-fidelity tests exist to protect). A negative `duration_ms` wraps to a huge `u64`. No panic, but the replayed branch can silently diverge from the original run.
**Fix:** Use checked/saturating conversions and treat any out-of-range stored value as a corrupt-row error rather than silently coercing:
```rust
let exit_code = i32::try_from(row.exit_code).unwrap_or_else(|_| {
    eprintln!("lacon explain: stored exit_code {} out of range; treating as failure", row.exit_code);
    1
});
let duration_ms = u64::try_from(row.duration_ms).unwrap_or(0);
```

### WR-05: `health::health_check` panics in debug builds on an unexpected probe result, reachable from `doctor`

**File:** `crates/lacon-core/src/tracking/health.rs:27` (called from `doctor.rs:354`)
**Issue:** `health_check` contains `debug_assert_eq!(one, 1)`. `doctor` calls this on a user-supplied/possibly-corrupt `history.db`. While `SELECT 1` returning anything other than `1` is not normally possible, the function is also `pub` and documented as a future extension point ("Phase 4 may extend this to also check `pragma user_version`, `journal_mode`…"). A `debug_assert` inside a health-check whose entire purpose is to *report* DB problems gracefully (not abort) is a latent contradiction: the moment the probe is extended to assert any DB-derived value, a debug-build `doctor` on a broken DB will panic instead of printing `[fail] tracker`. The same `debug_assert_eq!`-panic-on-untrusted-state anti-pattern was already burned once in this codebase (see the WR-02 fix note in `tracking/mod.rs:190-203`, which removed exactly such an assert in favor of a hard error).
**Fix:** Replace the assert with a returned error so the doctor path stays graceful in all build profiles:
```rust
let one: i32 = conn.query_row("SELECT 1", [], |r| r.get(0))?;
if one != 1 {
    return Err(TrackingError::Sqlite { /* or a dedicated HealthProbe variant */ });
}
```

## Info

### IN-01: `explain` raw column always renders a trailing empty line from the final newline

**File:** `crates/lacon-cli/src/commands/explain.rs:137,144-149`
**Issue:** `split_lines` splits on `b'\n'`, so stored output ending in `\n` (the common case) yields a trailing empty `String`. The raw column therefore shows one extra blank row versus the actual content. The branch-fidelity test `filter_bytes_no_on_error_passes_raw_unchanged` already notes this trailing-empty-segment quirk and asserts only `&out[..3]`. Cosmetic, but it makes the side-by-side row counts look off by one.
**Fix:** Drop a single trailing empty element before rendering if the input ended in a newline, or document the off-by-one as expected.

### IN-02: `pad_or_truncate` aligns by `char` count, not display width; embedded control/wide chars break column alignment

**File:** `crates/lacon-cli/src/commands/explain.rs:178-194`
**Issue:** The doc comment concedes "stable for ASCII". CJK/wide glyphs (2 cells), zero-width joiners, and embedded control bytes in the verbatim raw column will misalign the `|` separator. This is purely visual and is consistent with the deliberately simple D-06 "no diff crate" choice, but worth recording.
**Fix:** None required for v1; a `unicode-width`-based pad would fix it if alignment ever becomes a contract.

### IN-03: `explain` right (filtered) column is neither padded nor width-bounded

**File:** `crates/lacon-cli/src/commands/explain.rs:171-172`
**Issue:** Only the left column goes through `pad_or_truncate`; the right column prints unbounded. Long filtered lines wrap in the terminal and visually merge the two columns. Minor; the left-pad already provides the primary alignment. (Note this is also the vector for the WR-01 control-byte concern on the filtered side.)
**Fix:** Optionally truncate the right column to a max width as well.

### IN-04: `read_path_helpers_compile_against_path_ref` defines an unused inner fn and asserts nothing at runtime

**File:** `crates/lacon-core/tests/tracking_query.rs:438-444`
**Issue:** The test is a compile-time guard only — `_takes_path` is never called and there is no assertion. It is a legitimate "does this still accept `&Path`" check, but it reads as a no-op test and will not fail if the surface regresses at runtime (only if it stops compiling). Acceptable, but flag for clarity. The `clippy::too_many_arguments` allow on the seeding helpers is also fine but worth noting as accumulated test-helper complexity.
**Fix:** Add a brief comment that this is intentionally compile-only (it has one, but the empty body invites "dead test" suspicion), or invoke it behind `let _ = _takes_path;` to make the intent explicit.

### IN-05: `explain` ignores stdout/stderr ordering fidelity (merge is always stdout-then-stderr)

**File:** `crates/lacon-cli/src/commands/explain.rs:91-92`
**Issue:** The replay merges `stdout` then `stderr` as separate concatenated blobs, but the live runner interleaves both streams through a single pipe in arrival order (`runtime/mod.rs:184-192`). For any command whose real output interleaved stdout and stderr, the `explain` raw column will not byte-match the original interleaving — so the "byte-fidelity" claim for `explain` is approximate, not exact, whenever both streams were non-empty. This is an inherent v1 storage-model limitation (the two blobs are stored separately), not introduced by this code, but the `explain` doc comment (step 5, "matching v1's single merged-stream model") slightly overstates fidelity.
**Fix:** Document the limitation in the `explain` header (interleaving is not preserved across the stdout/stderr blob boundary), or in v2 store a single merged blob to match the live model.

---

_Reviewed: 2026-05-22T00:35:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
