---
phase: 02-local-tracking
plan: 04
subsystem: database
tags: [sqlite, rusqlite, wal, foreign-keys, busy-timeout, etcetera, xdg, retention, prune, throttle, integration-tests]

# Dependency graph
requires:
  - phase: 02-local-tracking
    plan: 01
    provides: TrackingError enum (CreateDir + Chmod + Sqlite + Marker + Clock variants), tracking module + Tracker skeleton (single bool field)
  - phase: 02-local-tracking
    plan: 02
    provides: tracking/mod.rs `pub mod prune;` declaration + empty stub at prune.rs; M0001_INITIAL DDL with lacon_meta(last_pruned_ts) seeded to '0'; idx_inv_ts and idx_raw_created indexes used by prune DELETEs
  - phase: 02-local-tracking
    plan: 03
    provides: tracking::privacy and tracking::health modules implemented (consumed by Plan 05's record path, not Plan 04 itself)
provides:
  - "tracking::Tracker::open(db_path, retention, cfg_store_raw_outputs, now_ms) -> Result<Tracker, TrackingError> — lazy on-write-path constructor: ensure 0700 parent dir, open SQLite (READ_WRITE | CREATE | NO_MUTEX), apply 3 PRAGMAs (busy_timeout=200ms, foreign_keys=ON via DBCONFIG_ENABLE_FKEY, journal_mode=WAL via pragma_update_and_check), run migrate, run prune_if_due"
  - "tracking::Tracker::xdg_db_path() -> Option<PathBuf> — etcetera::choose_base_strategy() resolution to <data_dir>/lacon/history.db on both Linux and macOS"
  - "tracking::apply_connection_pragmas(&Connection) -> Result<(), TrackingError> — pub(crate) helper applying the 3-pragma contract; reused by tests"
  - "tracking::prune::prune_if_due(&Connection, &Retention, now_ms) -> Result<(), TrackingError> — 24h-throttled retention DELETE; reads lacon_meta.last_pruned_ts, writes back on success"
  - "tracking::prune::PRUNE_THROTTLE_MS = 86_400_000 (pub(crate)); ONE_DAY_MS = 86_400_000 (pub(crate))"
  - "Tracker.conn: pub Connection — required by integration tests in this plan AND Plan 05's tracking_record.rs (revision Issue #1)"
  - "8 integration tests (tracking_tracker.rs) covering DB creation, migration applied, WAL persistence, FK pragma per-connection invariant, busy_timeout=200 exact, 0700 parent dir, idempotent re-open, idempotent 0755→0700 perm fix, xdg_db_path"
  - "5 integration tests (tracking_prune.rs) covering first-run prune, retention windows applied per-table (30d invocations / 3d raw_outputs / 30d suspected_regressions), within-24h throttle, exactly-24h boundary, corrupted last_pruned_ts graceful fallback to zero"
affects: [02-05, 02-06, phase-04-cli-doctor, phase-06-cold-start-bench]

# Tech tracking
tech-stack:
  added:
    - "(none new — rusqlite 0.39 + bundled, etcetera, tempfile, std::os::unix::fs::PermissionsExt all wired by Plans 01-03)"
  patterns:
    - "3-pragma contract: busy_timeout(Duration::from_millis(200)) → set_db_config(SQLITE_DBCONFIG_ENABLE_FKEY, true) → pragma_update_and_check(None, journal_mode, WAL, |row| row.get(0)). Order locked by docstring; debug_assert verifies WAL was actually accepted."
    - "Connection flags: SQLITE_OPEN_READ_WRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_NO_MUTEX. NO_MUTEX is the per-process single-threaded posture per RESEARCH §Crate API Notes — we never share Connection across threads."
    - "Idempotent perm fix: read metadata().permissions(), branch on (mode & 0o777) != 0o700, set_mode(0o700) only when needed. Defends against pre-existing 0755 or 0775 directories."
    - "unchecked_transaction() to obtain a transaction handle from &Connection (not &mut). Safe under our single-threaded-per-process invariant; documented per RESEARCH line 481."
    - "Clock injection in prune: now_ms accepted as u64 parameter, cast to i64 internally for SQLite TEXT-as-i64 arithmetic. Tests use FIXED_NOW_MS = 1_700_000_000_000 (~2023-11-14)."
    - "DELETE order in prune transaction: raw_outputs FIRST (avoids ON DELETE SET NULL trigger spam from the cascading invocations DELETE), then suspected_regressions, then invocations LAST."
    - "Tempdir-based test isolation: Tracker::open accepts &Path directly — no env-var override needed. Each test allocates its own TempDir under tempfile crate (RESEARCH Pitfall #4)."
    - "etcetera resolution returns Xdg on macOS too — single-path implementation for both Linux and macOS production targets (RESEARCH lines 562-575). Test asserts path ends_with(\"lacon/history.db\")."

key-files:
  created:
    - "crates/lacon-core/tests/tracking_tracker.rs (181 lines, 8 integration tests)"
    - "crates/lacon-core/tests/tracking_prune.rs (187 lines, 5 integration tests)"
  modified:
    - "crates/lacon-core/src/tracking/mod.rs (Tracker struct gains pub conn: Connection; impl Tracker block adds open + xdg_db_path; helpers apply_connection_pragmas + ensure_data_dir; total ~125 new lines)"
    - "crates/lacon-core/src/tracking/prune.rs (overwrites Plan 02's 1-line stub with 95-line real implementation: PRUNE_THROTTLE_MS, ONE_DAY_MS, prune_if_due)"

key-decisions:
  - "Tracker.conn ships as `pub` (NOT `pub(crate)`) per revision iteration 1 Issue #1. Integration tests under crates/lacon-core/tests/ are external to the crate boundary and must read tracker.conn directly (verified by grep regression-guard `! grep 'pub(crate) conn: Connection'`). The visibility cost is documented as accepted trust-boundary trade-off — Tracker is the documented entry point; conn is the test escape hatch."
  - "FK per-connection invariant test reworked from negative-side proof (\"fresh conn defaults to OFF\") to sibling-toggle proof. Reason: bundled libsqlite3-sys 0.37 ships -DSQLITE_DEFAULT_FOREIGN_KEYS=1 (build.rs:126), so a freshly-opened connection has FKs ON by default — opposite of stock SQLite. The original assertion was a build-config artefact, not a SQLite-spec invariant. The reworked test toggles FKs OFF on a sibling Connection::open and verifies tracker.conn's FK state remains ON, proving per-connection independence without depending on the default. Same Rule 1 deviation already documented in tracking_schema.rs::fk_silent_no_op_without_pragma."
  - "Plan 02 owns ALL `pub mod X;` declarations in tracking/mod.rs (Issue #5 wave-2 merge contention rule). Plan 04 confirms `grep -c '^pub mod prune;' = 1` — no duplicate declaration, no Plan 02 ownership encroachment."
  - "DELETE order in prune transaction: raw_outputs first → suspected_regressions → invocations last. Rationale per RESEARCH §Pruning Throttle Pattern line 488: deleting raw_outputs first avoids the ON DELETE SET NULL trigger firing once per row when the cascading invocations DELETE would otherwise sweep them. Order is asserted by inspection (grep against the DELETE statements) rather than runtime test, since the trigger-fire count isn't observable from SQL."
  - "Corrupted lacon_meta.last_pruned_ts (e.g. \"not-a-number\") is silently treated as 0 → prune fires immediately and rewrites the row. Locked by prune_with_corrupted_last_pruned_ts_treats_as_zero. This is the Tampering mitigation T-02-15 in the threat register."
  - "PRAGMA busy_timeout=200ms is an explicit downward override of rusqlite's 5000ms default (D-11). The 200ms ceiling exposes contention bugs in tests; CC-session concurrency in production should still fit comfortably."

patterns-established:
  - "Pattern: Apply per-connection PRAGMAs in a single helper (apply_connection_pragmas) reused by Tracker::open. Future tracker entry points (e.g. Phase 4 lacon doctor read-only opens) should reuse this helper rather than re-deriving the contract."
  - "Pattern: Clock-injected free functions (prune_if_due) over Tracker methods. Tests inject FIXED_NOW_MS without holding a Tracker; production callers compute now_ms from SystemTime once at the call site. Same shape will work for any future time-gated tracker operation."
  - "Pattern: Idempotent perm-enforcement on a directory we own. Read mode, branch on != desired, write only on mismatch. Avoids touching mtime when the directory is already correct."

requirements-completed:
  - REQ-tracking-sqlite-location  # Tracker::open creates ~/.local/share/lacon/history.db with 0700 parent dir; xdg_db_path resolves the path via etcetera::choose_base_strategy

# Metrics
duration: ~12min
completed: 2026-05-06
---

# Phase 02 Plan 04: Tracker::open + 24h-Throttled Prune Summary

**Tracker::open lazy on-write-path constructor with 3-pragma contract (busy_timeout=200ms, foreign_keys=ON via DBCONFIG_ENABLE_FKEY, journal_mode=WAL via pragma_update_and_check), idempotent 0700 parent dir creation, and 24h-throttled prune_if_due that runs three retention DELETEs in a single transaction.**

## Performance

- **Duration:** ~12 min
- **Started:** 2026-05-06 (resuming after Task 1 had landed pre-session)
- **Completed:** 2026-05-06
- **Tasks:** 3 (all committed atomically)
- **Files modified:** 2 (tracking/mod.rs, tracking/prune.rs)
- **Files created:** 2 (tests/tracking_tracker.rs, tests/tracking_prune.rs)

## Accomplishments

- `Tracker::open(db_path, retention, cfg_store_raw_outputs, now_ms)` opens or creates the SQLite DB with the v1 PRAGMA contract verified end-to-end (8 integration tests).
- `prune_if_due` reads `lacon_meta.last_pruned_ts`, applies the 24h throttle, and runs three retention DELETEs in transactional order (raw_outputs → suspected_regressions → invocations).
- 13 new integration tests across 2 suites, all green; full workspace `cargo test` (196 tests, 27 suites, 1 ignored) regression-clean.
- `Tracker.conn` ships as `pub` per revision Issue #1, with a regression-guard grep enforcing `pub(crate)` is absent.

## Task Commits

Each task was committed atomically:

1. **Task 1: Implement Tracker::open + 3 PRAGMAs + xdg_db_path** — `c1e4bf4` (feat) — landed pre-session, sequenced before Task 2 because the impl block forward-references `crate::tracking::prune::prune_if_due` (Plan 02's empty stub had to be overwritten before `cargo check` would pass).
2. **Task 2: Implement 24h-throttled prune_if_due + 5 integration tests** — `58c2277` (feat) — overwrites Plan 02's 1-line prune.rs stub; 5 tests cover first-run, retention windows, within-24h throttle, exactly-24h boundary, corrupted-ts graceful fallback.
3. **Task 3: Tracker::open integration tests** — `2af9107` (test) — 8 tests cover the full `Tracker::open` contract; FK test reworked under Rule 1 (see Deviations).

**Plan metadata:** (this commit) (docs: complete plan)

_Note: All three tasks were marked `tdd="true"` in the plan, but the plan's deliberate sequencing meant the test files for Tasks 2 and 3 land in the same commit as their implementation rather than as separate RED/GREEN commits._

## Files Created/Modified

- `crates/lacon-core/src/tracking/mod.rs` (modified) — Tracker struct gains `pub conn: Connection`; `impl Tracker` block adds `open` + `xdg_db_path`; helpers `apply_connection_pragmas` (the 3-pragma contract) + `ensure_data_dir` (0700 idempotent on Unix; create_dir_all-only fallback on non-Unix).
- `crates/lacon-core/src/tracking/prune.rs` (overwritten) — `pub fn prune_if_due(&Connection, &Retention, u64) -> Result<(), TrackingError>` + `PRUNE_THROTTLE_MS` / `ONE_DAY_MS` constants; transactional 4-DELETE/UPDATE block via `unchecked_transaction()`.
- `crates/lacon-core/tests/tracking_tracker.rs` (new, 181 lines, 8 tests) — full contract verification including WAL persistence, FK per-connection invariant via sibling-toggle, busy_timeout=200 exact, 0700 parent dir, idempotent perm fix, xdg_db_path.
- `crates/lacon-core/tests/tracking_prune.rs` (new, 187 lines, 5 tests) — clock-injected prune verification covering first-run, retention windows, throttle behaviour, exactly-24h boundary, corrupted-ts fallback.

## Decisions Made

- **Tracker.conn is `pub`, not `pub(crate)`** (Issue #1). Documented in the Tracker doc-comment as a deliberate test-affordance escape hatch; integration tests under `crates/lacon-core/tests/` are external to the crate and must read `tracker.conn` directly to verify pragma state. Regression guard: `! grep -F 'pub(crate) conn: Connection' crates/lacon-core/src/tracking/mod.rs`.
- **Plan 02 owns `pub mod prune;`** (Issue #5 wave-2 ownership rule). Plan 04 only modifies the Tracker struct + impl block + helpers, and overwrites Plan 02's stub at `prune.rs`. Verified by `grep -c '^pub mod prune;' = 1`.
- **DELETE order: raw_outputs → suspected_regressions → invocations**. Asserted by source inspection rather than runtime test, since trigger-fire counts are not observable from SQL. Documented in the prune.rs module-level docstring with the rationale from RESEARCH line 488.
- **`unchecked_transaction()` over `transaction()`**. The latter requires `&mut Connection`, but `prune_if_due` accepts `&Connection` to keep the call site in `Tracker::open` clean (the `&mut conn` is held by `migrations::migrate` which runs first; by the time prune runs we only have `&conn` left in scope). Safe under our single-threaded-per-process invariant.
- **Bundled SQLite has FKs ON by default** (libsqlite3-sys 0.37 build flag). Documented as a Rule 1 deviation in Task 3's commit message; the FK-per-connection test was reworked to a sibling-toggle proof instead of relying on stock SQLite's "FKs default to OFF" behaviour. Same finding already documented in `tracking_schema.rs::fk_silent_no_op_without_pragma` (Plan 02). The defensive `set_db_config(SQLITE_DBCONFIG_ENABLE_FKEY, true)` in `apply_connection_pragmas` still stands — locks behaviour against any future bundled flip.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] FK-per-connection test asserted incorrect default**

- **Found during:** Task 3 (Tracker::open integration tests)
- **Issue:** The plan's `open_fk_pragma_is_per_connection` test asserted that a fresh `Connection::open(&db_path)` returns `foreign_keys=0`, on the assumption that PRAGMA `foreign_keys` defaults to OFF per the SQLite spec. But our bundled rusqlite (libsqlite3-sys 0.37) compiles SQLite with `-DSQLITE_DEFAULT_FOREIGN_KEYS=1` (`build.rs:126`), so a freshly-opened bundled connection has FKs ON by default. The test failed with `assertion left == right failed: left: 1, right: 0`.
- **Fix:** Reworked the per-connection invariant proof. Instead of asserting a default, the test now (a) verifies `tracker.conn` has FKs ON (positive side, required by the plan), (b) opens a sibling `Connection::open(&db_path)` and explicitly disables FKs on it via `pragma_update`, (c) re-reads `tracker.conn`'s `foreign_keys` and asserts it remains ON — proving the pragma is independent across connections without relying on a particular default. Same approach already documented in Plan 02's `tracking_schema.rs::fk_silent_no_op_without_pragma`. The defensive `set_db_config(SQLITE_DBCONFIG_ENABLE_FKEY, true)` in `apply_connection_pragmas` stands unchanged — it's a future-proof guard against the bundled flag being flipped.
- **Files modified:** `crates/lacon-core/tests/tracking_tracker.rs` (single test body + comment block)
- **Verification:** `cargo test -p lacon-core --test tracking_tracker` → 8 passed, 0 failed.
- **Committed in:** `2af9107` (Task 3 commit)

---

**Total deviations:** 1 auto-fixed (1 bug — test premise corrected for bundled-build reality)
**Impact on plan:** No scope change. The deviation is the same Rule 1 finding Plan 02 already logged in STATE.md ("libsqlite3-sys 0.37 ships -DSQLITE_DEFAULT_FOREIGN_KEYS=1 — bundled rusqlite 0.39 has fks=ON by default; Plan 04 must still set pragma defensively"). The defensive `set_db_config` call still ships per the plan's contract; only the test's negative-side assertion was reframed.

## Issues Encountered

- None blocking. Task 1 was already committed (`c1e4bf4`) before this execution session — the plan's `mod.rs` block was applied in an earlier turn and the commit metadata confirms the contract. This session executed Tasks 2 and 3 (commits `58c2277` and `2af9107`).

## Plan-Specific Output Confirmations (per `<output>` requirements)

- **3-pragma order verified:** `apply_connection_pragmas` runs `busy_timeout(Duration::from_millis(200))` → `set_db_config(SQLITE_DBCONFIG_ENABLE_FKEY, true)` → `pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))` in that exact order, with a `debug_assert_eq!(mode.to_ascii_lowercase(), "wal")` post-condition. `grep -E 'busy_timeout\\s*\\(\\s*Duration::from_millis\\(200\\)\\s*\\)'`, `grep -F 'DbConfig::SQLITE_DBCONFIG_ENABLE_FKEY'`, and `grep -F '"journal_mode", "WAL"'` all return matches.
- **Wall-time observation:** `cargo test -p lacon-core --test tracking_tracker -- --exact open_creates_db_and_migrates` reports `finished in 0.05s` for the test body (which runs the full `Tracker::open` path including parent-dir creation, Connection::open, 3 PRAGMAs, migrate, and prune_if_due against an empty tempdir DB). The 0.05–0.11s range across 3 runs is dominated by test runner startup; the actual `Tracker::open` cost will be measured under cold-start microbenchmarks in Plan 06 (RESEARCH §Open Risks item 1).
- **WAL persistence test tear-down sequencing:** `open_persists_wal_in_db_header` requires explicit `drop(_tracker)` before opening a fresh `Connection::open(&db_path)` for the verification query, because the original tracker holds an open WAL writer and the verification query needs an independent connection to confirm the DB-header `journal_mode` byte (not just the in-memory pragma state). This is documented inline in the test comment ("WAL is persistent on the DB FILE [sqlite.org/wal.html]").
- **No additional tests beyond the enumerated 8 + 5.** The plan's 13-test slate is sufficient for the contract; no panic-on-bad-path negative tests were added.
- **Plan 01–03 tests remain green:** `cargo test --workspace` → 196 passed, 1 ignored, 0 failed across 27 suites. The phase-2 integration suite (`tracking_normalize` + `tracking_schema` + `tracking_views` + `tracking_privacy` + `tracking_tracker` + `tracking_prune`) reports 32 passed across 6 suites, no regression in the prior 19 plan-01–03 tests.
- **`Tracker.conn` is `pub conn: Connection` (Issue #1):** verified by `grep -c 'pub conn: Connection' = 1` AND `! grep -F 'pub(crate) conn: Connection'`.
- **Plan 02's `pub mod prune;` declaration is untouched (Issue #5):** verified by `grep -c '^pub mod prune;' = 1` (single match at `mod.rs:22`).

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- **Plan 05 (Tracker::record + CLI wire-up) is unblocked.** All `Tracker::open` deliverables ship: pub Connection, applied pragmas, migrate-on-first-open, throttled prune. Plan 05 only needs to add `Tracker::record(&self, meta, raw)` and call `Tracker::open` from `lacon-cli/src/commands/run.rs`.
- **Plan 06 (cold-start bench + lazy-open negative tests) inherits the lazy-on-write-path posture.** D-04 is preserved: `Tracker::open` is reachable only from inside `Tracker::open`'s call chain, never from `--version` / `validate` / `doctor` paths. Plan 06 will add negative tests that assert the DB file does NOT exist after `lacon --version` or `lacon validate`.
- **No new blockers.** Three deferred-to-prototyping open questions remain at their phase-assigned locations.

## Self-Check: PASSED

- `crates/lacon-core/src/tracking/mod.rs` — FOUND
- `crates/lacon-core/src/tracking/prune.rs` — FOUND
- `crates/lacon-core/tests/tracking_tracker.rs` — FOUND
- `crates/lacon-core/tests/tracking_prune.rs` — FOUND
- Commit `c1e4bf4` (Task 1) — FOUND
- Commit `58c2277` (Task 2) — FOUND
- Commit `2af9107` (Task 3) — FOUND
- `cargo check -p lacon-core` exits 0 — VERIFIED
- `cargo test --workspace` exits 0 (196 passed, 1 ignored, 0 failed) — VERIFIED
- `grep -E 'busy_timeout\(Duration::from_millis\(200\)\)' mod.rs` → match — VERIFIED
- `grep -F 'DbConfig::SQLITE_DBCONFIG_ENABLE_FKEY' mod.rs` → match — VERIFIED
- `grep -F '"journal_mode", "WAL"' mod.rs` → match — VERIFIED
- `grep -F 'PRUNE_THROTTLE_MS: i64 = 86_400_000' prune.rs` → match — VERIFIED
- `grep -F 'pub conn: Connection' mod.rs` → 1 match — VERIFIED
- `! grep -F 'pub(crate) conn: Connection' mod.rs` → no match (regression guard pass) — VERIFIED

---
*Phase: 02-local-tracking*
*Completed: 2026-05-06*
