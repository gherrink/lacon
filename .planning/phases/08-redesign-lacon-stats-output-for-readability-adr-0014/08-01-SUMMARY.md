---
phase: 08-redesign-lacon-stats-output-for-readability-adr-0014
plan: 01
subsystem: tracking-read-path
tags: [sqlite, tracking, query, stats, adr-0014]
requires:
  - "lacon-core::tracking::query (existing read API, Phase 4 04-01)"
  - "lacon-core::tracking::open_readonly (read-only WAL open)"
  - "invocations base table (bypassed column, byte columns)"
provides:
  - "lacon-core::tracking::query::OverallTotals (row struct)"
  - "lacon-core::tracking::query::overall_totals(conn)"
  - "lacon-core::tracking::query::filtered_overall_totals(conn, since, project)"
affects:
  - "08-03 stats headline (consumes these readers)"
tech-stack:
  added: []
  patterns:
    - "Scalar aggregate via query_row (not query_map().collect()) — exactly one row, all-zeros on empty"
    - "COALESCE(SUM(...), 0) guards scalar-NULL-on-zero-rows"
    - "?N placeholder binds in Vec<&dyn rusqlite::ToSql> (copied from filtered_project_savings)"
key-files:
  created: []
  modified:
    - "crates/lacon-core/src/tracking/query.rs"
    - "crates/lacon-core/tests/tracking_query.rs"
decisions:
  - "D-01 fence held: no view/migration/write-path change; readers hit the base invocations table directly (no v_overall view added)"
  - "D-02: exactly one new aggregate concept (overall + filtered counterpart) behind the lacon-core query boundary; rusqlite stays lacon-cli dev-only"
  - "D-05: headline floor is bypassed=0, spans matched AND unmatched runs (no rule_id predicate)"
  - "distinct_projects is COUNT(DISTINCT project_path) PRE-canonicalization; the displayed 'after canonicalization' count is computed in stats.rs from the rolled-up map, not from this field"
metrics:
  duration: 6min
  completed: 2026-05-23
  tasks: 2
  files: 2
---

# Phase 8 Plan 01: OverallTotals Headline Aggregate Summary

Added one read-only scalar aggregate behind `lacon-core::tracking::query` — `overall_totals(conn)` plus its `--since`/`--project`-filtered counterpart `filtered_overall_totals(conn, since, project)`, returning a new `OverallTotals` row struct — as the typed data source for the ADR 0014 stats headline (08-03), over `bypassed = 0` rows, with no migration, view, field rename, or write-path change.

## What Was Built

**Task 1 — `OverallTotals` struct + two readers** (`query.rs`, commit `8de3bf6`):
- `pub struct OverallTotals { total_runs, distinct_projects, raw_total, kept_total, bytes_saved }` (all `i64`), mirroring `ProjectSaving`'s `#[derive(Debug, Clone, PartialEq)]` and doc-above-struct style. Doc-comment records that `distinct_projects` is `COUNT(DISTINCT project_path)` pre-canonicalization (SQL has no FS access) and that the headline's "after canonicalization" count is computed in `stats.rs`, not from this field.
- `overall_totals(conn)`: static SQL — `COUNT(*)`, `COUNT(DISTINCT project_path)`, three `COALESCE(SUM(...), 0)` byte aggregates — `FROM invocations WHERE bypassed = 0`, read via `query_row([], ...)` (a scalar aggregate always returns exactly one row, all-zeros on empty; one-line code comment notes this differs from every other reader). Reads the base table directly — no `v_overall` view (D-01 forbids one).
- `filtered_overall_totals(conn, since_cutoff_ms, project)`: copies `filtered_project_savings`' binds-vec + `?{n}` placeholder pattern verbatim (`Vec<&dyn rusqlite::ToSql>`, two `if let Some(...)` blocks with `n += 1`), same scalar SELECT, `WHERE bypassed = 0` floor, **no** `GROUP BY`/`ORDER BY`, **no** `rule_id` predicate (headline spans matched + unmatched per D-05), read via `query_row(binds.as_slice(), ...)`. Filter values flow only through `?N` placeholders (T-08-02). Bare `?` propagation via `TrackingError`'s `#[from] rusqlite::Error`.

**Task 2 — lacon-core integration tests** (`tracking_query.rs`, commit `b3cb45f`):
- `overall_totals_excludes_bypassed_rows`: reuses `seed_realistic_db()` (which already seeds 5 bypassed=0 rows + exactly one bypassed=1 row of 9999 bytes). Asserts `total_runs == 5`, `raw_total == 13756`, `kept_total == 6878`, `bytes_saved == raw_total - kept_total`, `distinct_projects == 2` — the bypassed row's 9999 bytes are absent from every total (T-08-01 boundary).
- `filtered_overall_totals_empty_filter_returns_zeroed_row`: a `--project` filter matching nothing on the populated DB returns `OverallTotals { 0, 0, 0, 0, 0 }` via the derived `PartialEq` — guards the scalar-SUM-NULL-on-zero-rows pitfall (T-08-03).

## Verification Results

| Gate | Result |
|------|--------|
| `cargo build -p lacon-core` | pass |
| `cargo test -p lacon-core --test tracking_query overall` | 2 passed |
| `cargo test -p lacon-core --test tracking_query` (full file) | 15 passed (13 prior + 2 new) |
| `cargo test -p lacon-core` (full crate, all bins) | all green, 0 failed |
| `grep -c 'COALESCE(SUM'` query.rs (need ≥6) | 6 |
| `grep -c 'pub struct OverallTotals'` query.rs (need ==1) | 1 |
| `query_row` in both new readers (not query_map) | confirmed (lines 410, 457) |
| No `GROUP BY` in the two new functions | confirmed (only pre-existing per-project/offender readers retain it) |
| No diff to `migrations/`, view DDL, or field names | confirmed (only query.rs + tracking_query.rs changed) |
| `filtered_bytes` / `avg_keep_ratio` unchanged | confirmed (15 / 5 references intact) |
| `rusqlite` in lacon-cli still `[dev-dependencies]` only | confirmed (line 42 under `[dev-dependencies]` line 23) |
| clippy on lacon-core lib | no new warnings touch query.rs (4 pre-existing unrelated warnings out of scope) |

## Threat Mitigations Applied

- **T-08-01 (Info Disclosure)** — `WHERE bypassed = 0` floor on both readers; bypassed (user-suppressed) rows never enter headline totals. Asserted by `overall_totals_excludes_bypassed_rows`.
- **T-08-02 (SQL injection)** — `--since`/`--project` bound only via `?{n}` placeholders in a `Vec<&dyn rusqlite::ToSql>`; only static SQL fragments concatenated. No string interpolation of user values.
- **T-08-03 (DoS / NULL panic)** — `COALESCE(SUM(...), 0)` + `query_row` returns an all-zero row instead of erroring on a NULL SUM over zero rows. Asserted by `filtered_overall_totals_empty_filter_returns_zeroed_row`.

## Deviations from Plan

None — plan executed exactly as written. The plan listed Task 1 (implementation) before Task 2 (tests); both are `tdd="true"`, but `tdd_mode` is off in config and the plan's explicit task order placed the implementation first (the interfaces section gave the verbatim target bodies), so execution followed the plan order. Both new tests pass against the implementation and the bypassed-exclusion boundary is non-vacuous (the seeded bypassed row carries distinct non-zero bytes).

## Known Stubs

None. Both readers are fully wired against the base `invocations` table and proven by tests. The 08-03 stats headline will consume `overall_totals` / `filtered_overall_totals` directly.

## Self-Check: PASSED

- `crates/lacon-core/src/tracking/query.rs` — FOUND (modified, contains `pub fn overall_totals`, `pub fn filtered_overall_totals`, `pub struct OverallTotals`)
- `crates/lacon-core/tests/tracking_query.rs` — FOUND (modified, contains both new tests)
- Commit `8de3bf6` (feat 08-01) — FOUND
- Commit `b3cb45f` (test 08-01) — FOUND
