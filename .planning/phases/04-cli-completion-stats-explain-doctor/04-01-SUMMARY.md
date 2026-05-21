---
phase: 04-cli-completion-stats-explain-doctor
plan: 01
subsystem: database
tags: [sqlite, rusqlite, tracking, read-api, wal, stats, explain]

# Dependency graph
requires:
  - phase: 02-local-tracking
    provides: "Tracker write path (Tracker::open WAL+migrate+prune, Tracker::record), the four reporting views, apply_connection_pragmas, TrackingError"
provides:
  - "tracking::open_readonly(&Path) -> Result<Connection, TrackingError> — non-mutating read-only DB open (D-02)"
  - "tracking::query module — typed view readers, D-09 base-table filtered re-queries, explain invocation+BLOB lookups (all SQL behind core boundary, D-01)"
  - "Wave-0 finding: strict SQLITE_OPEN_READ_ONLY succeeds on a WAL history.db (no D-02 fallback needed on this build)"
affects: [04-02, 04-03, stats, explain, doctor]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Read-only DB open: free fn (no Tracker state), READ_ONLY flags, safe pragmas only, never journal_mode=WAL"
    - "D-09 filtered re-query: re-implement view body over base invocations table; bind values via params!, interpolate only ?N placeholder indices"

key-files:
  created:
    - crates/lacon-core/src/tracking/query.rs
    - crates/lacon-core/tests/tracking_query.rs
  modified:
    - crates/lacon-core/src/tracking/mod.rs
    - crates/lacon-core/tests/wave0_smoke.rs

key-decisions:
  - "Wave-0 spike: strict SQLITE_OPEN_READ_ONLY works on WAL history.db (rusqlite 0.39 / libsqlite3-sys 0.37, Linux/ext4) — Task 2 uses READ_ONLY, the D-02 fallback was NOT needed"
  - "open_readonly is a free fn in the tracking module (not impl Tracker) — reads need no Tracker state (D-01)"
  - "D-09 filtered readers re-query the base invocations table because no view exposes ts and only v_project_savings exposes project_path"
  - "Filter values bound via params!/?N; only the placeholder index ?{n} is string-built — no user value ever interpolated (T-04-01)"

patterns-established:
  - "Pattern: typed result struct per view (UnmatchedOffender/FilteredOffender/BypassRate/ProjectSaving) mirroring health.rs HealthReport style"
  - "Pattern: explain lookups return Ok(None) on missing id rather than an error (normal not-found outcome)"

requirements-completed: [REQ-cli-stats, REQ-cli-explain]

# Metrics
duration: 9min
completed: 2026-05-22
---

# Phase 4 Plan 01: Tracking Read Surface Summary

**Read-only SQLite query layer for lacon-core: a non-mutating `open_readonly` helper plus a `tracking::query` module exposing typed view readers, D-09 base-table filtered re-queries, and explain's invocation+BLOB lookups — all SQL behind the core boundary.**

## Performance

- **Duration:** ~9 min
- **Started:** 2026-05-22T~00:01Z
- **Completed:** 2026-05-22T~00:10Z
- **Tasks:** 4
- **Files modified:** 4 (2 created, 2 modified)

## Accomplishments

- **Wave-0 read-only WAL spike (gating finding):** Strict `SQLITE_OPEN_READ_ONLY` succeeds against a WAL `history.db` on this build — so the read helper uses the simple path, not the documented D-02 fallback.
- **`open_readonly` helper (D-02):** Free fn opening an existing DB read-only; no CREATE (absent file → Err per D-03), safe pragmas only (`busy_timeout` + `foreign_keys`), and deliberately no `journal_mode=WAL` write (Pitfall 1). Proven non-mutating (invocations COUNT unchanged before/after open — T-04-02).
- **`tracking::query` module (D-01, D-05, D-09):** 4 unfiltered view readers + 4 base-table filtered re-queries + 2 explain lookups (`fetch_invocation`, `fetch_raw_output`). All parameterized; all SQL stays in lacon-core (lacon-cli keeps `rusqlite` dev-only).
- **13 integration tests** seed a realistic DB (matched/unmatched, multi-project, old/new ts, exit 0/non-zero, bypassed 0/1, NULL/non-NULL raw_output_id) and assert every read path including the BLOB round-trip and the no-write invariant.

## Read API exposed to Wave 2 (exact names)

`open_readonly` signature:
```
pub fn open_readonly(db_path: &Path) -> Result<Connection, TrackingError>
```

`tracking::query` result structs: `UnmatchedOffender`, `FilteredOffender`, `BypassRate`, `ProjectSaving`, `InvocationRow`; type alias `RawOutputBlobs = (Vec<u8>, Vec<u8>)`.

`tracking::query` functions:
- Unfiltered view readers: `unmatched_offenders`, `filtered_offenders`, `bypass_rate`, `project_savings` (each `&Connection -> Result<Vec<Row>, TrackingError>`)
- D-09 filtered re-queries:
  - `filtered_unmatched_offenders(&Connection, since_cutoff_ms: Option<i64>, project: Option<&str>)`
  - `filtered_filtered_offenders(&Connection, since_cutoff_ms, project, rule: Option<&str>)`
  - `filtered_bypass_rate(&Connection, since_cutoff_ms, rule)`
  - `filtered_project_savings(&Connection, since_cutoff_ms, project)`
- explain lookups:
  - `fetch_invocation(&Connection, id: i64) -> Result<Option<InvocationRow>, TrackingError>`
  - `fetch_raw_output(&Connection, raw_output_id: i64) -> Result<Option<RawOutputBlobs>, TrackingError>`

## Task Commits

1. **Task 1: Wave-0 read-only WAL spike** - `dc2fa35` (test)
2. **Task 2: open_readonly helper (D-02)** - `41707f2` (feat)
3. **Tasks 3+4: tracking::query read API + integration tests** - `15ff6a8` (feat)
4. **Deferred-items log (pre-existing clippy lints)** - `16c62b9` (chore)

_Tasks 3 (module) and 4 (its integration test) were committed together: the test is the integration proof for the module and the two share no isolation boundary._

## Files Created/Modified

- `crates/lacon-core/src/tracking/query.rs` (created) — the read API: typed view readers, D-09 filtered re-queries, explain lookups
- `crates/lacon-core/tests/tracking_query.rs` (created) — 13 seed-and-assert integration tests
- `crates/lacon-core/src/tracking/mod.rs` (modified) — added `open_readonly` free fn + `pub mod query;`
- `crates/lacon-core/tests/wave0_smoke.rs` (modified) — added `smoke_readonly_open_of_wal_db`

## Decisions Made

- **Strict READ_ONLY over D-02 fallback** — Wave-0 spike empirically settled Open Question 1; strict `SQLITE_OPEN_READ_ONLY` reads the WAL db fine, so `open_readonly` uses it directly. The fallback (read-write without CREATE) remains documented in the helper's doc comment for any platform that later fails the strict open.
- **`fetch_raw_output` coalesces NULL BLOBs to empty** — `raw_outputs.stdout/stderr` are nullable columns; the helper returns empty `Vec` rather than propagating a type error, so explain renders a present-but-empty stream correctly.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] clippy `type_complexity` on `fetch_raw_output` return type**
- **Found during:** Task 3 (tracking::query module), surfaced by `cargo clippy -- -D warnings`
- **Issue:** `Result<Option<(Vec<u8>, Vec<u8>)>, TrackingError>` tripped clippy's complex-type lint, blocking the plan's clippy verification gate on a file this plan owns.
- **Fix:** Introduced `pub type RawOutputBlobs = (Vec<u8>, Vec<u8>)` and used it in the signature.
- **Files modified:** crates/lacon-core/src/tracking/query.rs
- **Verification:** `cargo clippy -p lacon-core` no longer flags query.rs; all new 04-01 files are clippy-clean.
- **Committed in:** `15ff6a8` (Tasks 3+4 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking).
**Impact on plan:** Cosmetic readability fix to satisfy the clippy gate on a new file. No behavior change, no scope creep.

## Issues Encountered

- **`ByteCounts` fields are `usize`, not `u64`** — the test helper initially typed `raw_stdout_bytes: u64`, which mismatched `ByteCounts { raw_stdout_bytes: usize, ... }`. Fixed the helper signature to `usize`. (Caught at RED→GREEN compile; part of the Tasks 3+4 commit.)
- **Pre-existing clippy 1.95.0 lints (out of scope, deferred):** The toolchain's clippy now reports 4 errors in Phase 1/2 code — `stages.rs:438/451` (`collapsible_if`), `record.rs:8` (`doc_overindented_list_items`), `mod.rs:201` (`manual_ignore_case_cmp`). Confirmed pre-existing via git blame (commits `8924ff0`, `192e2c2`, `9798e78`); NOT fixed here per the scope boundary — fixing other phases' code from this read-layer plan would be scope creep and could mask a regression. Logged in `04-cli-completion-stats-explain-doctor/deferred-items.md` (commit `16c62b9`). The plan's `cargo clippy -p lacon-core -- -D warnings` gate therefore reports these 4 pre-existing failures, while every file this plan created/modified is clippy-clean. Recommend a Phase 6 hardening sweep (each is a one-line mechanical fix).

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- Wave 2 (`stats` in Plan 04-02, `explain` in 04-03) can plug straight into `tracking::open_readonly` + the `tracking::query` functions listed above. No further core read-layer work is needed for those commands' data access.
- `doctor`'s tracker probe consumes the same `open_readonly` connection plus the existing `tracking::health::health_check`.
- Concern: the 4 pre-existing clippy lints will keep the workspace `-D warnings` clippy gate red until a later plan/phase clears them (tracked in deferred-items.md).

## Self-Check: PASSED

- Files verified present: query.rs, tracking_query.rs, 04-01-SUMMARY.md, deferred-items.md
- Commits verified present: dc2fa35, 41707f2, 15ff6a8, 16c62b9

---
*Phase: 04-cli-completion-stats-explain-doctor*
*Completed: 2026-05-22*
