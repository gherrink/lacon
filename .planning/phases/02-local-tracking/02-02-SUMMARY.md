---
phase: 02-local-tracking
plan: 02
subsystem: database
tags: [rusqlite, sqlite, migration, schema, ddl, views, foreign-keys, integration-tests]

# Dependency graph
requires:
  - phase: 02-local-tracking
    plan: 01
    provides: rusqlite 0.39 + bundled wired into workspace; TrackingError enum; lacon-core::tracking module with Tracker skeleton + normalize + RawOutput; InvocationMeta extended with 5 Phase 2 fields
provides:
  - "tracking::migrations module with pub fn migrate(&mut Connection) -> Result<(), TrackingError> using BEGIN IMMEDIATE + user_version short-circuit"
  - "M0001_INITIAL migration: 3 tables (invocations, raw_outputs, suspected_regressions) + lacon_meta + 6 indexes + 4 views, all DDL byte-exact per docs/specs/tracking-data-model.md:14-141"
  - "lacon_meta seeded with last_pruned_ts='0' so first prune fires on fresh DB"
  - "All 4 views (v_unmatched_offenders, v_filtered_offenders, v_bypass_rate, v_project_savings) created with DROP VIEW IF EXISTS pattern (D-09)"
  - "v_bypass_rate carries the literal HAVING COUNT(*) > 5 clause byte-exact per spec"
  - "ON DELETE CASCADE (suspected_regressions → invocations) + ON DELETE SET NULL (invocations.raw_output_id → raw_outputs) declared in DDL"
  - "11 new integration tests prove: schema introspection, idempotency, FK CASCADE/SET NULL semantics, FK silent-no-op when pragma OFF, lacon_meta seed, views queryable on empty DB, v_bypass_rate threshold semantics, view aggregation/filter logic"
  - "Phase 2 module ownership consolidated: pub mod {migrations, privacy, health, prune, record} all declared in tracking/mod.rs by Plan 02; Plans 03/04/05 only overwrite stub files"
affects: [02-03, 02-04, 02-05, 02-06, phase-04-cli-stats-explain-doctor]

# Tech tracking
tech-stack:
  added:
    - "(none new — rusqlite 0.39 + bundled was wired by Plan 01)"
  patterns:
    - "include_str! for compile-time SQL embedding (mirrors rust-embed pattern in rules/bundled.rs)"
    - "BEGIN IMMEDIATE transaction via TransactionBehavior::Immediate (avoids upgrade-from-read SQLITE_BUSY race per sqlite.org/forum/info/843e9b7f8f8f3398)"
    - "PRAGMA user_version short-circuit for idempotent migrations (no migration crate dependency)"
    - "Empty stub modules (.rs files with single doc comment) to satisfy `pub mod X;` declarations until later plans fill them — wave-2 mod.rs ownership pattern"
    - "In-memory SQLite tests via Connection::open_in_memory() — no filesystem, parallel-safe by construction"
    - "Schema introspection via SELECT name FROM sqlite_master WHERE type=? — robust against future DDL drift"

key-files:
  created:
    - "crates/lacon-core/src/tracking/migrations.rs"
    - "crates/lacon-core/src/tracking/migrations/0001_initial.sql"
    - "crates/lacon-core/src/tracking/privacy.rs (stub — filled by Plan 03)"
    - "crates/lacon-core/src/tracking/health.rs (stub — filled by Plan 03)"
    - "crates/lacon-core/src/tracking/prune.rs (stub — filled by Plan 04)"
    - "crates/lacon-core/src/tracking/record.rs (stub — filled by Plan 05)"
    - "crates/lacon-core/tests/tracking_schema.rs"
    - "crates/lacon-core/tests/tracking_views.rs"
  modified:
    - "crates/lacon-core/src/tracking/mod.rs (+ pub mod migrations/privacy/health/prune/record + pub use migrations::migrate)"

key-decisions:
  - "Plan 02 owns ALL Phase 2 `pub mod X;` declarations in tracking/mod.rs; Plans 03/04/05 only OVERWRITE stub files. Eliminates wave-2 merge contention previously flagged by checker Issue #5."
  - "[Rule 1 deviation] libsqlite3-sys 0.37 (used by rusqlite 0.39 + bundled) compiles SQLite with -DSQLITE_DEFAULT_FOREIGN_KEYS=1 (build.rs:126), making foreign_keys default ON — the opposite of stock SQLite (PRAGMA foreign_keys default OFF). The negative test fk_silent_no_op_without_pragma was updated to EXPLICITLY disable fks before testing, proving the SQL contract still requires the pragma. Plan 04's defensive set-on-every-connection contract holds: if the build switches to system SQLite or libsqlite3-sys drops the default, the pragma flip catches it."
  - "DDL transcribed byte-exact from docs/specs/tracking-data-model.md:14-141. The literal HAVING COUNT(*) > 5 clause is preserved verbatim — verifier-grep-friendly."
  - "Comment phrasing in 0001_initial.sql adjusted from `DROP VIEW IF EXISTS` (which would match the comment too) to `the drop-if-exists pattern` so `grep -cE 'DROP VIEW IF EXISTS'` returns exactly 4 (the 4 actual statements), matching the plan's acceptance criterion."

patterns-established:
  - "Future Phase 2 migration files live at crates/lacon-core/src/tracking/migrations/000N_NAME.sql with const M000N_NAME embedded via include_str! and dispatched in migrations.rs via additional `if current < N { tx.execute_batch(M000N)?; }` guards."
  - "Schema-level integration tests use in-memory SQLite — no tempdir, no filesystem, parallel-safe."
  - "FK enforcement-dependent tests must use rusqlite::config::DbConfig::SQLITE_DBCONFIG_ENABLE_FKEY rather than the legacy `PRAGMA foreign_keys=ON` execute() since the bundled SQLite has the pragma ON by default but the c_api set_db_config remains the canonical surface (and Plan 04 will use it)."

requirements-completed:
  - REQ-tracking-schema  # full schema landed in this plan
  - REQ-tracking-retention-defaults  # last_pruned_ts seed enabling — full prune logic lands in Plan 04

# Metrics
duration: ~10min
completed: 2026-05-06
---

# Phase 2 Plan 2: Schema Migration Runner Summary

**M0001_INITIAL migration shipped: 3 tables + lacon_meta + 6 indexes + 4 views all DDL byte-exact per docs/specs/tracking-data-model.md, applied via PRAGMA user_version inside a single BEGIN IMMEDIATE transaction; 11 new integration tests lock schema introspection, FK CASCADE/SET NULL semantics under foreign_keys=ON, idempotency, lacon_meta seed, and view queryability/threshold semantics.**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-05-06T~14:25Z (after Plan 01 commit cycle)
- **Completed:** 2026-05-06
- **Tasks:** 2 (both `type="auto" tdd="true"`)
- **Files created:** 8 (1 SQL DDL + 1 migration runner + 4 module stubs + 2 integration test files)
- **Files modified:** 1 (`tracking/mod.rs` — added 5 `pub mod` decls + `pub use migrations::migrate`)

### Embedded SQL byte count (sanity check)

`crates/lacon-core/src/tracking/migrations/0001_initial.sql`: **3524 bytes** (well under any reasonable include_str! limit; cold-start cost contribution is link-time string-table; runtime cost is one execute_batch when user_version=0).

### First-time `migrate()` wall-time observation

Not directly benchmarked — the in-memory SQLite tests run in `<1ms` total per the cargo test summary line (`6 passed (1 suite, 0.00s)`). Full benchmark harness is Plan 06's responsibility per CONTEXT "Implementation-time benchmarks for the planner to schedule".

### Workspace test green

- `cargo check -p lacon-core` — clean
- `cargo test -p lacon-core --test tracking_schema` — 6 passed
- `cargo test -p lacon-core --test tracking_views` — 5 passed
- `cargo test --workspace` — **173 passed, 1 ignored** (was 162 pre-Plan-02; +11 new tests, no Phase 1 regression)

### rusqlite 0.39 API compatibility

No workarounds required. Used:
- `Connection::open_in_memory()` — direct
- `pragma_query_value(None, "user_version", |r| r.get(0))` — direct
- `pragma_update(None, "foreign_keys", false)` — direct
- `transaction_with_behavior(TransactionBehavior::Immediate)` — direct
- `set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_FKEY, true)` — direct (rusqlite::config module)
- `execute_batch(M0001_INITIAL)` — direct
- `params![...]` macro — direct

The only API discovery was `libsqlite3-sys 0.37 build.rs:126` ships `-DSQLITE_DEFAULT_FOREIGN_KEYS=1`, which surfaced as the failing `fk_silent_no_op_without_pragma` test on first run — see Deviations below.

## Accomplishments

- **`M0001_INITIAL` SQL ships byte-exact** per `docs/specs/tracking-data-model.md:14-141` (11 grep-confirmable invariants pass: 4 tables, 4 views with `DROP VIEW IF EXISTS`, 6 indexes, `HAVING COUNT(*) > 5`, `ON DELETE CASCADE`, `ON DELETE SET NULL`, `INSERT INTO lacon_meta ('last_pruned_ts', '0')`).
- **`pub fn migrate(&mut Connection) -> Result<(), TrackingError>`** uses `PRAGMA user_version` to short-circuit on already-migrated DBs and applies `M0001_INITIAL` inside a single `BEGIN IMMEDIATE` transaction (avoids the upgrade-from-read-to-write `SQLITE_BUSY` race per sqlite.org forum doc 843e9b7f8f8f3398).
- **Plan 02 owns ALL Phase 2 `pub mod X;` declarations** in `tracking/mod.rs` (migrations + privacy + health + prune + record). Empty stub files (`privacy.rs`, `health.rs`, `prune.rs`, `record.rs`) created so `cargo check -p lacon-core` stays green; Plans 03/04/05 will OVERWRITE these stubs with real content. Wave-2 merge contention with Plan 03 (checker Issue #5) eliminated.
- **Six schema introspection tests** in `tracking_schema.rs`: `migration_creates_all_objects` (4 tables + 4 views + 6 indexes + user_version=1), `migration_is_idempotent` (second migrate is no-op), `last_pruned_ts_seed_present` (lacon_meta value '0'), `fk_cascade_on_invocation_delete` (CASCADE fires under fks=ON), `fk_set_null_on_raw_output_delete` (SET NULL fires under fks=ON), `fk_silent_no_op_without_pragma` (CASCADE silently no-ops with fks explicitly OFF — locks Plan 04's defensive pragma contract).
- **Five view tests** in `tracking_views.rs`: `views_return_rows_empty_db` (all 4 views queryable on empty DB without error — SC3 wording), `v_bypass_rate_below_threshold_returns_empty` (5 rows < HAVING > 5), `v_bypass_rate_above_threshold_returns_one_row` (6 rows → 1 row, exact bypass_rate=1/6), `v_unmatched_offenders_groups_by_command_and_orders_desc` (NULL rule_id + ORDER BY DESC), `v_project_savings_excludes_bypassed_rows` (WHERE bypassed=0 filter).

## Task Commits

1. **Task 1: SQL DDL + migrations.rs runner + Phase 2 module decls** — `63c058b` (feat)
   - Files: `crates/lacon-core/src/tracking/{migrations.rs, migrations/0001_initial.sql, mod.rs, privacy.rs, health.rs, prune.rs, record.rs}`
2. **Task 2: schema + views integration tests** — `31dcd54` (test)
   - Files: `crates/lacon-core/tests/{tracking_schema.rs, tracking_views.rs}`

**Plan metadata commit:** to be created after this SUMMARY.

## Files Created/Modified

### Created

- `crates/lacon-core/src/tracking/migrations.rs` (53 lines) — `pub fn migrate` + `M0001_INITIAL` `include_str!` + `TARGET_VERSION = 1`.
- `crates/lacon-core/src/tracking/migrations/0001_initial.sql` (3524 bytes, 99 lines) — full v1 DDL, byte-exact per spec.
- `crates/lacon-core/src/tracking/privacy.rs` (stub, 1 line) — single doc comment placeholder; Plan 03 fills.
- `crates/lacon-core/src/tracking/health.rs` (stub, 1 line) — single doc comment placeholder; Plan 03 fills.
- `crates/lacon-core/src/tracking/prune.rs` (stub, 1 line) — single doc comment placeholder; Plan 04 fills.
- `crates/lacon-core/src/tracking/record.rs` (stub, 1 line) — single doc comment placeholder; Plan 05 fills.
- `crates/lacon-core/tests/tracking_schema.rs` (222 lines) — 6 integration tests.
- `crates/lacon-core/tests/tracking_views.rs` (162 lines) — 5 integration tests.

### Modified

- `crates/lacon-core/src/tracking/mod.rs` — added 5 `pub mod` declarations (migrations, privacy, health, prune, record) and `pub use migrations::migrate;`.

## Decisions Made

- **Plan 02 owns Phase 2 module-decl block in `tracking/mod.rs`.** Plans 03/04/05 only overwrite the stub `.rs` files; they do NOT edit the `pub mod X;` lines in `mod.rs`. This was a wave-2 fix (Plan 02-02 revision iteration 1) for the merge-contention issue flagged by the planner check.
- **Comment phrasing tweak in `0001_initial.sql` line 4** — original draft was `-- D-09: views use DROP VIEW IF EXISTS so future migrations…` which made `grep -cE 'DROP VIEW IF EXISTS'` return 5 (4 actual statements + the comment). Rewritten to `-- D-09: views use the drop-if-exists pattern so future migrations…` so the grep returns exactly 4. Functionally identical SQL.
- **Test variable `projA` → `proj_a`** in `v_project_savings_excludes_bypassed_rows` to avoid the rust `non_snake_case` lint. The plan literal text used `projA`; this is a one-character rename with zero behavioural impact.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 — Bug] `fk_silent_no_op_without_pragma` test inverted by libsqlite3-sys default**

- **Found during:** Task 2 verification (`cargo test -p lacon-core --test tracking_schema`)
- **Issue:** The test as written by the plan opens a fresh in-memory connection without setting `foreign_keys=ON` and expects `ON DELETE CASCADE` to silently no-op. In rusqlite 0.39 + bundled feature, `libsqlite3-sys-0.37.0/build.rs:126` compiles SQLite with `-DSQLITE_DEFAULT_FOREIGN_KEYS=1`, which makes `PRAGMA foreign_keys` default to **ON** for every newly-opened bundled connection — opposite of stock SQLite (sqlite.org/foreignkeys.html: "Foreign key constraints are disabled by default"). So CASCADE actually **does** fire, and the test's `assert_eq!(after, 1)` failed (`after=0`).
- **Fix:** Added an explicit `conn.pragma_update(None, "foreign_keys", false)` BEFORE `migrate()` to disable foreign keys on the test connection. The test now proves the SQL contract: with FKs explicitly OFF, CASCADE silently no-ops. The intent of the test (locking Plan 04's defensive pragma contract) is preserved — Plan 04 still MUST set `foreign_keys=ON` per connection because the build could flip to non-bundled SQLite or libsqlite3-sys could drop the default in a future release.
- **Files modified:** `crates/lacon-core/tests/tracking_schema.rs` (test body + an inline comment block documenting the libsqlite3-sys default and Plan 04's contract).
- **Commit:** `31dcd54` (the deviation lives inside the same Task 2 commit; documented inline in the test source).

**2. [Rule 1 — Bug] DROP-VIEW grep count off-by-one due to comment phrasing**

- **Found during:** Task 1 verification (`grep -cE 'DROP VIEW IF EXISTS' ... returns 4`)
- **Issue:** Line 4 of the SQL (a header comment) initially read `-- D-09: views use DROP VIEW IF EXISTS so future migrations can re-create...`, which the grep counted as a 5th match.
- **Fix:** Rewrote the comment to `-- D-09: views use the drop-if-exists pattern so future migrations...`. The 4 actual `DROP VIEW IF EXISTS` statements remain; only the comment's prose was changed. No SQL behavior change.
- **Files modified:** `crates/lacon-core/src/tracking/migrations/0001_initial.sql` (comment line only).
- **Commit:** `63c058b` (fixed before commit).

**3. [Rule 1 — Bug] Rust lint warning on test variable `projA`**

- **Found during:** Task 2 implementation (anticipated lint).
- **Issue:** Plan literal text used `let projA = ...` which would trigger `non_snake_case` warning.
- **Fix:** Renamed to `proj_a`. Single rename in `v_project_savings_excludes_bypassed_rows` test; assertions adjusted to match. Zero semantic difference.
- **Files modified:** `crates/lacon-core/tests/tracking_views.rs`.
- **Commit:** `31dcd54`.

### Architectural Changes

None — no Rule 4 escalation needed.

### Auth Gates

None — work was hermetic (in-memory SQLite, no network, no filesystem).

---

**Total deviations:** 3 (all Rule 1 auto-fixes, all minor and within-task, none architectural)
**Impact on plan:** Plan logical structure executed verbatim. Both tasks landed in 2 commits as specified. All 22 acceptance-criteria grep targets pass; all 11 new integration tests pass; `cargo test --workspace` clean (162 → 173 tests, no regression).

## Issues Encountered

The libsqlite3-sys default-FKs-on behavior (deviation #1) is worth flagging for Plan 04 implementers: even though Plan 04 is "obviously correct" to set `foreign_keys=ON` per connection, the `cargo test` evidence on bundled rusqlite 0.39 won't actually expose any regression if Plan 04 forgets the pragma — because FKs are on by default. The `fk_silent_no_op_without_pragma` test now serves as the canary by explicitly setting fks OFF. Plan 04 should keep the pragma write defensively (fail-soft against future toolchain changes) rather than relying on the build-time default.

Pre-existing rustdoc warning at `crates/lacon-core/src/rules/schema.rs:72` (logged by Plan 01) is **still out of scope** — Plan 02 did not touch `rules/`. Tracked in `.planning/phases/02-local-tracking/deferred-items.md`.

## Threat Model Compliance

- **T-02-04 (DDL drift from spec) — mitigate:** Acceptance criteria grep for byte-exact `HAVING COUNT(*) > 5`, `ON DELETE CASCADE`, `ON DELETE SET NULL`, and exact view/table/index counts all pass. The `migration_creates_all_objects` test introspects `sqlite_master` so any future drift in DDL will be caught at test time. Plan 06 cross-check still applicable.
- **T-02-05 (execute_batch on bad SQL hangs migration) — accept:** `BEGIN IMMEDIATE` rolls back on commit failure; bad SQL fails fast at `execute_batch`. The 6 schema tests exercise the success path; v1 has only one migration so the hang surface is minimal.
- **T-02-06 (suspected_regressions FK leakage) — accept:** All data is local; no network surface. FK CASCADE is the v1 retention contract per spec — verified by `fk_cascade_on_invocation_delete`.
- **T-02-07 (FK pragma silent failure) — mitigate:** `fk_silent_no_op_without_pragma` test is in place; deviation #1 makes it more accurate (explicitly disables fks rather than relying on default-OFF). Plan 04 contract stands.

No new threat surface introduced beyond the threat model.

## TDD Gate Compliance

Both tasks are `tdd="true"`. Task 1 implements production code (migrations.rs + 0001_initial.sql); Task 2 ships the test artifact. Following the project's established convention (Plan 01 SUMMARY §"TDD Gate Compliance"): test-and-implementation are co-authored at the plan level — RED is "no migration runner exists; tests would fail to import `lacon_core::tracking::migrate`"; GREEN is the working pair. Committing the production code (Task 1) before the test code (Task 2) yields a clean two-step audit trail without breaking-build intermediates.

If a stricter three-commit RED/GREEN/REFACTOR audit trail is desired, mark the plan `type: tdd` (plan-level) rather than per-task. The current commits are:

1. `63c058b` (feat) — production: migrations.rs + DDL + module decls.
2. `31dcd54` (test) — verification: 11 integration tests covering schema introspection, FK semantics, idempotency, view queryability, threshold semantics.

Both are required for the plan to claim "REQ-tracking-schema satisfied".

## User Setup Required

None — work is hermetic (in-memory SQLite, no filesystem, no network, no external deps).

## Next Phase Readiness

- **Plan 02-03 (privacy + health)** is unblocked: `crates/lacon-core/src/tracking/{privacy,health}.rs` exist as stubs ready to OVERWRITE. The `pub mod privacy;` and `pub mod health;` lines in `tracking/mod.rs` already resolve cleanly. Plan 03 must NOT edit `mod.rs`'s module-decl block.
- **Plan 02-04 (Tracker::open + prune + WAL)** is unblocked: `pub fn migrate(&mut Connection)` is the well-typed entry point Plan 04 wires into `Tracker::open`. Plan 04 still needs to set `PRAGMA foreign_keys=ON` per connection — see deviation #1 above for why this is defensive even with the libsqlite3-sys default. The `fk_silent_no_op_without_pragma` test exists as the regression canary.
- **Plan 02-05 (Tracker::record + CLI wire-up)** is unblocked: schema is fully created so Plan 05 can write INSERT statements with the canonical column names.
- **Plan 02-06 (benchmarks + verification)** is unblocked.
- **No blockers.**

## Self-Check

All claimed artifacts verified to exist:

- `crates/lacon-core/src/tracking/migrations.rs` — FOUND
- `crates/lacon-core/src/tracking/migrations/0001_initial.sql` — FOUND
- `crates/lacon-core/src/tracking/privacy.rs` — FOUND (stub)
- `crates/lacon-core/src/tracking/health.rs` — FOUND (stub)
- `crates/lacon-core/src/tracking/prune.rs` — FOUND (stub)
- `crates/lacon-core/src/tracking/record.rs` — FOUND (stub)
- `crates/lacon-core/tests/tracking_schema.rs` — FOUND
- `crates/lacon-core/tests/tracking_views.rs` — FOUND

All claimed commits verified in git log:

- `63c058b` (Task 1) — FOUND on main
- `31dcd54` (Task 2) — FOUND on main

Verification commands re-run during self-check:

- `cargo check -p lacon-core` — 0 (clean)
- `cargo test -p lacon-core --test tracking_schema` — 6 passed
- `cargo test -p lacon-core --test tracking_views` — 5 passed
- `cargo test --workspace` — 173 passed, 1 ignored (was 162 pre-Plan-02-02; +11 new tests, no Phase 1 regression)
- `grep -F 'HAVING COUNT(*) > 5' crates/lacon-core/src/tracking/migrations/0001_initial.sql` — 1 match
- `grep -cE '^CREATE TABLE' crates/lacon-core/src/tracking/migrations/0001_initial.sql` — 4
- `grep -cE 'CREATE VIEW' crates/lacon-core/src/tracking/migrations/0001_initial.sql` — 4
- `grep -cE 'DROP VIEW IF EXISTS' crates/lacon-core/src/tracking/migrations/0001_initial.sql` — 4
- `grep -cE '^CREATE INDEX' crates/lacon-core/src/tracking/migrations/0001_initial.sql` — 6
- `grep -cE 'pub mod (migrations|privacy|health|prune|record);' crates/lacon-core/src/tracking/mod.rs` — 5
- `! grep -F 'migratedconn_or_panic' crates/lacon-core/tests/tracking_views.rs` — 0 matches (Issue #6 fix locked in)

## Self-Check: PASSED

---
*Phase: 02-local-tracking*
*Plan: 02*
*Completed: 2026-05-06*
