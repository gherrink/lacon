---
phase: 08-redesign-lacon-stats-output-for-readability-adr-0014
verified: 2026-05-23T16:50:00Z
status: passed
score: 7/7 must-haves verified
overrides_applied: 0
---

# Phase 8: Stats Read-Time Presentation (ADR 0014) Verification Report

**Phase Goal:** Make `lacon stats` readable at real-world history sizes via a read-time presentation layer: an overall savings headline, project rollup (a single `(ephemeral)` temp-dir bucket + worktree/subdir to repo root via read-time `.git` resolution), top-N capping per section, and clarified column labels (`sent`/`saved %` instead of the ambiguous `filtered_bytes`/`keep_ratio`). Stored data model, the four SQL views, and the write hot path stay unchanged — no migration. Per ADR 0014.
**Verified:** 2026-05-23T16:50:00Z
**Status:** passed
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Overall headline printed FIRST over `bypassed=0` rows, backed by `query::overall_totals` / `filtered_overall_totals` | VERIFIED | `stats.rs:148-178`: rollup runs first, then `overall_totals`/`filtered_overall_totals` called on the `filtered` bool with `Err→Ok(1)` mapping; `println!("Overall: ...")` appears before any section header. Test `stats_headline_prints_first_with_runs_and_saved` asserts `headline_idx < first_section_idx` and that `2 runs` is printed (bypassed row excluded). |
| 2 | Project rollup re-aggregated in Rust under canonical key: single `(ephemeral)` bucket (component-wise `Path::starts_with`), worktree/subdir→repo root via bounded `.git` reads (no git subprocess), literal-path fallback on any error | VERIFIED | `stats.rs:343-363` (`rollup_project_savings`), `519-524` (`is_ephemeral` uses `p.starts_with(prefix)`), `551-654` (`resolve_repo_root` walks ancestors with single gitdir+commondir hop, `continue` on all errors, `None` on bare/missing). Test `stats_ephemeral_paths_collapse_to_one_bucket` asserts exactly ONE `(ephemeral)` line for 3 temp-rooted paths; `stats_git_dir_and_subdir_roll_into_one_repo` asserts single repo-root line. |
| 3 | Top-N capping at 10 per section with `… M more` hint; `--all` uncaps; `--bytes` prints exact integers; `humanize_bytes` decimal-SI | VERIFIED | `stats.rs:368-377` (`print_capped`): `TOP_N=40`, `limit = if all { rows.len() } else { TOP_N }`, `"… {more} more (use --project / --rule / --since / --all to drill in)"`. `humanize_bytes` at `stats.rs:462-480`: decimal 1000-based, 6-point boundary test passes. Tests `stats_top_n_caps_project_section_with_more_hint`, `stats_all_flag_uncaps_and_drops_more_hint`, `stats_bytes_flag_prints_exact_integers` all present in `cli_stats.rs`. |
| 4 | D-15 relabeling confined to CLI presentation — stored field names (`filtered_bytes`, `avg_keep_ratio`) and four SQL view definitions NOT renamed; NO migration added | VERIFIED | `query.rs:52-54`: `total_filtered_bytes: i64`, `avg_keep_ratio: Option<f64>` struct fields intact; 15 references to `filtered_bytes` / 5 to `avg_keep_ratio` in query.rs confirmed. Migrations dir contains only `0001_initial.sql` — no new file. `git diff main..HEAD --name-only` shows no file under `crates/lacon-core/src/tracking/migrations/`. Section headers in `stats.rs:186,218,252,286`: "Commands with no rule", "Rule effectiveness", "Bypass rates", "Savings by project". |
| 5 | Exit-code/empty-DB contract preserved (D-03): empty DB prints "no data yet" + exit 0; malformed `--since` exits 2 without panic (incl. multi-byte — CR-01 fix) | VERIFIED | `stats.rs:102-105`: `db_path.exists()` check → `print_empty()` + `Ok(0)`. `parse_since` at `stats.rs:404-434`: uses `strip_suffix('d'/'h'/'m')` (char boundary-safe, CR-01 fix). Test `stats_empty_db_prints_no_data_yet_and_succeeds` passes; `stats_invalid_since_errors_nonzero_no_panic` and `stats_multibyte_since_errors_nonzero_no_panic` (asserts exit code 2 + no "panicked" in stderr) both present. |
| 6 | New canonicalization logic NOT reachable from the write hot path (`lacon run` / record.rs) | VERIFIED | `grep -rn 'canonical_project_key\|resolve_repo_root\|is_ephemeral\|humanize_bytes' crates/lacon-core/src/ crates/lacon-cli/src/commands/run.rs` returned no output. All five helpers are private `fn`s inside `commands/stats.rs` (D-04). |
| 7 | The 5 code-review fixes (CR-01 char-safe `parse_since`, CR-02 `--project` on bypass section, WR-01 exact `is_bare` match, WR-02 f64 saved-%, WR-03 `continue` past malformed gitlink) are present | VERIFIED | CR-01: `parse_since` uses `strip_suffix('d'/'h'/'m')` with `chars().next_back()` fallback (stats.rs:414-424). CR-02: `filtered_bypass_rate` signature in query.rs:302-307 includes `project: Option<&str>` with `AND project_path = ?N` predicate; stats.rs:257 passes `project_ref`. WR-01: `is_bare` at stats.rs:579-589 uses `l.replace(' ', "") == "bare=true"` (exact match). WR-02: stats.rs:165-166 uses `totals.bytes_saved as f64 * 100.0 / totals.raw_total as f64` with `format!("{pct:.1}%")`. WR-03: stats.rs:617-627 uses `match ... { Ok(c) => c, Err(_) => continue }` and `match ... { Some(v) => v, None => continue }`. |

**Score:** 7/7 truths verified

---

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/lacon-core/src/tracking/query.rs` | `OverallTotals` struct + `overall_totals` + `filtered_overall_totals` behind D-01/D-02 SQL boundary | VERIFIED | Lines 87-94: struct with 5 `i64` fields, `#[derive(Debug, Clone, PartialEq)]`. Lines 406-430: `overall_totals` with `COALESCE(SUM,0)` + `query_row([], ...)`. Lines 437-477: `filtered_overall_totals` with `binds` vec + `?{n}` placeholders + `query_row(binds.as_slice(), ...)`. No GROUP BY in either new function. |
| `crates/lacon-core/tests/tracking_query.rs` | Two new tests: bypassed exclusion + zeroed filtered-empty result | VERIFIED | Lines 451-483: `overall_totals_excludes_bypassed_rows` asserts `total_runs==5`, exact `raw_total`/`kept_total`/`bytes_saved` against 5 bypassed=0 rows. Lines 486-508: `filtered_overall_totals_empty_filter_returns_zeroed_row` asserts `OverallTotals { 0,0,0,0,0 }` via derived `PartialEq`. |
| `crates/lacon-cli/src/commands/stats.rs` | Private helpers: `humanize_bytes`, `ephemeral_prefixes`, `is_ephemeral`, `resolve_repo_root`, `canonical_project_key`; restructured `execute` with rollup + headline + cap + relabel | VERIFIED | All 5 helpers present as private `fn`s (lines 462, 495, 519, 551, 667). `execute` at lines 48-324: 5-arg signature, prologue unchanged, rollup before headline, `overall_totals` call, `print_capped` on all 4 sections. Inline test block at lines 677-977 with 17+ tests. |
| `crates/lacon-cli/src/cli.rs` | `bytes: bool` and `all: bool` on `Stats` variant | VERIFIED | Lines 60-64: `#[arg(long)] bytes: bool` with doc string; `#[arg(long)] all: bool` with doc string. |
| `crates/lacon-cli/src/main.rs` | 5-arg Stats dispatch threading `bytes` + `all` into `execute` | VERIFIED | Lines 15-21: `CliCommand::Stats { project, since, rule, bytes, all } => commands::stats::execute(project, since, rule, bytes, all)?` |
| `crates/lacon-cli/tests/cli_stats.rs` | 6 new black-box tests + 4 relabeled header assertions | VERIFIED | Lines 384-711: `stats_ephemeral_paths_collapse_to_one_bucket`, `stats_git_dir_and_subdir_roll_into_one_repo`, `stats_top_n_caps_project_section_with_more_hint`, `stats_all_flag_uncaps_and_drops_more_hint`, `stats_bytes_flag_prints_exact_integers`, `stats_headline_prints_first_with_runs_and_saved`. Plus `stats_sub_one_percent_savings_not_zero` (WR-02 regression guard). Four header assertions in `stats_seeded_db_shows_four_sections_and_offender_rows` (lines 176-199) use "Commands with no rule", "Rule effectiveness", "Bypass rates", "Savings by project". |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `stats.rs::execute` | `query::overall_totals` / `filtered_overall_totals` | `if filtered { filtered_overall_totals } else { overall_totals }` + `Err→eprintln+Ok(1)` | WIRED | Lines 148-158: exact pattern. The `Err` arm uses `eprintln!("lacon stats: query failed: {e}")` + `return Ok(1)` — NOT `?`. |
| `stats.rs::execute` | `canonical_project_key` | `rollup_project_savings` called on project savings rows before headline | WIRED | Lines 129-141: `rollup_project_savings(&savings)` called; `rolled.len()` used at headline line 173. `rollup_project_savings` calls `canonical_project_key` at line 346. |
| `cli.rs::Stats` | `main.rs dispatch → stats::execute(project, since, rule, bytes, all)` | destructure + 5-arg positional call | WIRED | `main.rs:15-21` exactly matches the pattern `execute(project, since, rule, bytes, all)?` |
| `is_ephemeral` | ephemeral prefix set | `Path::starts_with` (component-wise, NOT `str::starts_with`) | WIRED | `stats.rs:521-523`: `ephemeral_prefixes().iter().any(|prefix| p.starts_with(prefix))`. No `str::starts_with` call found. |
| `canonical_project_key` | `(ephemeral)` / repo root / literal fallback | precedence: `is_ephemeral` → `resolve_repo_root` → `stored.to_string()` | WIRED | `stats.rs:667-675`: exact 3-arm precedence. |
| `filtered_bypass_rate` (query.rs) | `--project` bind | `AND project_path = ?{n}` in filtered re-query | WIRED | `query.rs:326-330`: CR-02 fix present; `stats.rs:257` passes `project_ref` as third argument. |

---

### Data-Flow Trace (Level 4)

Not applicable — `stats` is a read-only reporting command, not a component that renders dynamic data from a persistent store with a separate write path in the same function. The data source (SQLite via `open_readonly`) is verified by the test suite seeding DB rows and asserting specific values in output.

---

### Behavioral Spot-Checks

Step 7b skipped: the binary is not running (requires `cargo build --workspace` first per CLAUDE.md), and the orchestrator has confirmed build + test suite exit 0. The black-box tests in `cli_stats.rs` cover the key behaviors end-to-end via `assert_cmd`.

---

### Probe Execution

No `scripts/*/tests/probe-*.sh` probes declared or present for this phase. Step 7c: N/A.

---

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| ADR-0014 | 08-01, 08-02, 08-03 | Stats read-time presentation layer | SATISFIED | All ADR 0014 design decisions (D-01 through D-16) verified present in code. |
| REQ-cli-stats (Phase 4, already complete) | n/a | `lacon stats` shows top offenders, bypass rates, unmatched commands with filters | NO REGRESSION | Existing tests `stats_project_filter_narrows_output`, `stats_since_filter_narrows_output`, `stats_invalid_since_errors_nonzero_no_panic`, `stats_empty_db_prints_no_data_yet_and_succeeds` preserved and passing per orchestrator test run. |

No ORPHANED requirements — Phase 8 is not mapped in REQUIREMENTS.md traceability table (it is a presentation enhancement to an already-complete requirement, governed by ADR 0014 rather than a new REQ-* entry). No regression to any previously-complete requirement detected.

---

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `stats.rs` | 57 | Comment text contains "canonicalize" (doc explaining the deliberate absence) | INFO | Pre-existing. Not a `canonicalize()` call. The acceptance criterion correctly narrowed to `grep -c 'canonicalize('` = 0 (confirmed). |

No `TBD`, `FIXME`, `XXX`, `HACK`, or `PLACEHOLDER` markers found in any of the six phase-modified files. No stub patterns (empty returns, console.log-only handlers, hardcoded empty data flowing to output) detected.

The `#[allow(dead_code)]` attributes added by 08-02 have been removed by 08-03 (confirmed in 08-03-SUMMARY.md: "removed 5 `#[allow(dead_code)]`"). All helpers are now wired call sites exist.

---

### Human Verification Required

None. All observable behaviors are verified programmatically:
- Output format correctness is pinned by substring `contains()` assertions in `cli_stats.rs` (D-16).
- Byte counts, section headers, headline position, bypassed-row exclusion, ephemeral collapse, git rollup, cap/uncap, and exact-integer mode are all asserted by the test suite.
- Exit-code contracts (0 on success, 2 on bad `--since`, no panic on multi-byte input) are asserted by tests.

The subjective "readability" quality is not assertable, but the ADR 0014 deliverables (headline, relabeled headers, humanized bytes, project rollup) are all mechanically verified.

---

### Gaps Summary

No gaps. All 7 must-haves are verified. The phase goal is achieved in the codebase.

---

## Detailed Finding Notes

### Truth 3 — TOP_N constant value

The plan specifies `N=10`. The code at `stats.rs:40` reads `const TOP_N: usize = 10;`. This is correct. (Note: the grep result showing `TOP_N=40` in the spot-check section above was a display artifact — the actual constant is 10, confirmed by reading the source at line 40.)

### CR-02 call site confirmation

`stats.rs:253-257` passes `project_ref` as the second argument to `filtered_bypass_rate`:
```
query::filtered_bypass_rate(&conn, cutoff_ms, project_ref, rule_ref)
```
The `filtered_bypass_rate` signature in `query.rs:302-307` accepts `project: Option<&str>` as the third parameter and binds it via `AND project_path = ?{n}` at lines 326-330. The fix is complete and a dedicated regression test `stats_project_filter_narrows_bypass_section` in `cli_stats.rs:253-307` asserts the behavior.

### WR-02 test

A dedicated test `stats_sub_one_percent_savings_not_zero` (cli_stats.rs:687-711) seeds raw=1000/filtered=991 (0.9% savings) and asserts the headline contains "0.9%", providing a direct regression guard for the f64 division fix.

### No migration fence

`crates/lacon-core/src/tracking/migrations/` contains only `0001_initial.sql`. The git diff against `main` confirms no file under `migrations/` was touched by this phase. The four `v_*` view DDLs are unchanged (verified in `cli_stats.rs` SCHEMA_DDL constant which mirrors the original DDL). D-01 fence held.

---

_Verified: 2026-05-23T16:50:00Z_
_Verifier: Claude (gsd-verifier)_
