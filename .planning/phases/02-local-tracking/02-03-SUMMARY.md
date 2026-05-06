---
phase: 02-local-tracking
plan: 03
subsystem: database
tags: [privacy, marker, sqlite, health-check, atomic-create, byte-stable, integration-tests]

# Dependency graph
requires:
  - phase: 02-local-tracking
    plan: 01
    provides: TrackingError enum (Marker + Sqlite variants), lacon-core::tracking module + Tracker skeleton
  - phase: 02-local-tracking
    plan: 02
    provides: tracking/mod.rs with `pub mod privacy;` + `pub mod health;` decls; empty stub files at privacy.rs and health.rs
provides:
  - "tracking::privacy::warn_once_if_needed(config_path, marker_path) -> Result<(), TrackingError> — atomic marker creation via OpenOptions::create_new(true) + byte-stable D-16 stderr warning on first creation"
  - "tracking::privacy::resolve_marker_path(project_root, user_config_dir, project_store_raw, user_store_raw) -> Option<(PathBuf, PathBuf)> — D-14 layer-priority resolver returning (config_path, marker_path) or None when both layers off"
  - "tracking::privacy::format_warning(config_path, marker_path) -> String (pub(crate)) — single source of byte-stable D-16 template; tests assert byte-exact"
  - "tracking::privacy::MARKER_FILENAME = \".store_raw_outputs_acked\" — public constant, reusable by Plan 04 callers"
  - "tracking::health::health_check(&Connection) -> Result<HealthReport, TrackingError> — SELECT 1 round-trip probe defining the surface Phase 4 `lacon doctor` calls"
  - "tracking::health::HealthReport { select_one_returned: i32 } — extension-friendly struct (Phase 4 may add user_version, journal_mode, foreign_keys fields)"
  - "5 unit tests (4 privacy + 1 health) + 5 integration tests (tracking_privacy.rs) covering byte-exact warning text, layer priority, idempotent re-call, missing-parent-dir error path, and concurrent-thread race posture"
affects: [02-04, 02-05, phase-04-cli-doctor]

# Tech tracking
tech-stack:
  added:
    - "(none new — rusqlite 0.39 + bundled was wired by Plan 01; std::fs::OpenOptions::create_new is std-only)"
  patterns:
    - "Atomic marker file primitive: OpenOptions::new().write(true).create_new(true).open(path) returns Ok exactly once across racing processes; AlreadyExists on subsequent attempts."
    - "Byte-stable warning text via String concatenation in tests — catches both edits AND line reordering"
    - "Cross-platform marker mode: #[cfg(unix)] sets 0o600 belt-and-suspenders; #[cfg(not(unix))] omits PermissionsExt path so cargo check stays green on Windows even though v1 excludes it."
    - "pub(crate) format_warning helper isolates byte-stability so the public warn_once_if_needed surface stays minimal."

key-files:
  created:
    - "crates/lacon-core/tests/tracking_privacy.rs (93 lines, 5 integration tests)"
  modified:
    - "crates/lacon-core/src/tracking/privacy.rs (overwrites Plan 02 stub: 168 lines → real implementation + 4 unit tests)"
    - "crates/lacon-core/src/tracking/health.rs (overwrites Plan 02 stub: 39 lines → real implementation + 1 unit test)"

key-decisions:
  - "Plan 03 OVERWRITES Plan 02's stub files for privacy.rs and health.rs without touching tracking/mod.rs. Wave-2 mod.rs ownership rule (Plan 02 owns the module-decl block) holds verbatim — `git diff 5f9b3b1..HEAD --name-only` confirms 3 files changed, none of them mod.rs (Issue #5 elimination verified)."
  - "Marker mode bits 0o600 are belt-and-suspenders (parent dir is 0700 regardless), wrapped in #[cfg(unix)] so Windows compilation stays green per RESEARCH §Filesystem & Permissions."
  - "format_warning is pub(crate), not pub — consumers go through warn_once_if_needed which combines marker check + format. Tests reach format_warning directly for byte-exact assertions."
  - "concurrent_calls_at_most_one_creates is a smoke test, not a fairness test — it asserts both threads return Ok and the marker exists; it does NOT assert which thread won the race or that exactly one warning was printed (the latter would require capturing stderr from threads). Race claim from RESEARCH §Privacy Marker File Semantics is upheld at the API level."

patterns-established:
  - "Future tracking helpers that need atomic create-or-skip semantics should use OpenOptions::create_new(true) directly — no Path::exists() pre-check (TOCTOU)."
  - "Test fixtures using tempfile::TempDir + MARKER_FILENAME constant are reusable in Plan 04 (Tracker::record callsite) and Phase 3 (lacon init)."

requirements-completed:
  - REQ-tracking-privacy-warning  # one-time warning + marker semantics implemented and byte-exact tested
  # REQ-tracking-raw-outputs-default-off — listed in plan frontmatter as a covered requirement
  # via the resolve_marker_path D-14 contract (returns None when both flags false), but the
  # off-by-default INSERT gating lives in Plan 04's Tracker::record. Plan 04 will close this.

# Metrics
duration: ~6min
completed: 2026-05-06
---

# Phase 2 Plan 3: Privacy Marker + Health Probe Summary

**Atomic privacy marker (D-14/D-15) with byte-stable D-16 stderr warning, plus the no-op SELECT 1 health probe (D-13) Phase 4's `lacon doctor` will consume — both modules overwrite Plan 02 stubs without touching `tracking/mod.rs`, eliminating wave-2 merge contention. 10 new tests pass (5 unit + 5 integration); workspace at 183 passed (was 173).**

## Performance

- **Duration:** ~6 min
- **Started:** 2026-05-06 (post-Plan-02-02 commit cycle)
- **Completed:** 2026-05-06
- **Tasks:** 2 (`type="auto" tdd="true"` each — production + tests)
- **Files created:** 1 (integration test)
- **Files modified:** 2 (overwrite stubs)

### Confirmation: warning text byte-exact test passes

`format_warning_byte_exact_template` (privacy.rs unit test) asserts the literal 4-line template:

```
lacon: store_raw_outputs is enabled.
lacon: raw stdout/stderr will be retained at ~/.local/share/lacon/history.db
lacon: for up to 3 days. Disable in /proj/.lacon/config.yaml or run `rm` on the DB.
lacon: this notice is shown once per project (marker: /proj/.lacon/.store_raw_outputs_acked).
```

The `~/.local/share/lacon/history.db` substring stays literal even when the actual XDG_DATA_HOME is overridden — per CONTEXT D-16 + RESEARCH "Note on D-16" (line 558). Only `<config-path>` and `<marker-path>` are interpolated.

### concurrent_calls_at_most_one_creates: not flaky

The integration test was run multiple times (`cargo test -p lacon-core --test tracking_privacy` and `cargo test --workspace`); both passed cleanly. `OpenOptions::create_new(true)` is OS-atomic [VERIFIED: doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.create_new] — exactly one thread wins, the other gets `AlreadyExists` which the API catches and returns Ok. No timing-dependent assertions in the test (it does NOT check which thread won or that exactly one warning was printed — only that both threads return Ok and the marker file exists at the end).

If a future stricter test wants to assert "exactly one warning written to stderr," it would need to redirect stderr to a buffer and count the `lacon: store_raw_outputs is enabled.` substrings. Current threading model passes Reqs without that complexity.

### rusqlite API surface notes for `health_check`

The probe uses the **simplest possible API surface**:

```rust
let one: i32 = conn.query_row("SELECT 1", [], |r| r.get(0))?;
```

- `query_row` is the "exactly one row expected" idiom — returns `Err(QueryReturnedNoRows)` if zero, `Err(SqliteFailure)` if the SQL itself fails. For `SELECT 1` neither error path is reachable on a healthy connection.
- `[]` is the empty params array — `params!` macro is unnecessary for parameter-less queries.
- The closure `|r| r.get(0)` reads column 0 as `i32` (rusqlite's `FromSql` trait handles INTEGER → i32 for any value within range).
- `?` operator works because Plan 01 wired `TrackingError::Sqlite { #[from] source: rusqlite::Error }`.

Phase 4 may extend `HealthReport` with `pragma_query_value(None, "user_version", |r| r.get(0))?`, `pragma_query_value(None, "journal_mode", |r| r.get::<_, String>(0))?`, and `pragma_query_value(None, "foreign_keys", |r| r.get::<_, i32>(0))?` calls — all using the same `rusqlite 0.39` surface verified in RESEARCH (Context7 `/websites/rs_rusqlite_0_39_0_rusqlite`).

### Confirmation: tracking/mod.rs NOT touched by Plan 03

```
$ git diff 5f9b3b1..HEAD --name-only
crates/lacon-core/src/tracking/health.rs
crates/lacon-core/src/tracking/privacy.rs
crates/lacon-core/tests/tracking_privacy.rs
```

Three files changed; `tracking/mod.rs` not in the list. Wave-2 ownership rule (Plan 02 owns the module-decl block; Plan 03 only overwrites stubs) is upheld. Issue #5 wave-2 merge contention is verifiably eliminated.

### Workspace test count

- Pre-Plan-03: **173 passed, 1 ignored** (per Plan 02-02 SUMMARY)
- Post-Plan-03: **183 passed, 1 ignored** (`cargo test --workspace`, 25 suites, 2.84s)
- Delta: +10 (4 privacy unit + 1 health unit + 5 privacy integration)

## Accomplishments

- **`warn_once_if_needed` ships with race-free atomic marker creation** via `OpenOptions::create_new(true)` — no `Path::exists()` pre-check, so two concurrent `lacon run` invocations cannot both warn the user. `AlreadyExists` is the silent-success path; any other I/O error becomes `TrackingError::Marker { path, source }` carrying the marker path for diagnostics.
- **`format_warning` produces byte-stable D-16 text.** The 4-line template is asserted character-by-character via `format_warning_byte_exact_template`; only `<config-path>` and `<marker-path>` are interpolated. The literal `~/.local/share/lacon/history.db` stays in the warning text per CONTEXT D-16 even though it's the documented default and not the runtime-resolved XDG path.
- **`resolve_marker_path` implements D-14 layer priority** — project-layer wins over user-layer when both have `store_raw_outputs: true`; user-layer is the fallback; `None` when both are false (bundled default cannot opt in). Three unit tests cover all three branches.
- **`health_check(&Connection) -> Result<HealthReport, TrackingError>`** runs `SELECT 1` and returns a report struct. Phase 2 defines the surface; Phase 4's `lacon doctor` will consume it. The struct shape (`HealthReport { select_one_returned: i32 }`) is extension-friendly — Phase 4 can add `user_version`, `journal_mode`, `foreign_keys` fields without breaking callers.
- **5 integration tests via the public crate boundary** (`lacon_core::tracking::privacy::*`) cover marker creation, idempotent re-call, pre-existing marker, missing parent dir, and a concurrent-thread race smoke test. Tests live at `crates/lacon-core/tests/tracking_privacy.rs` and use `tempfile::TempDir` for isolation.

## Task Commits

1. **Task 1: privacy.rs + health.rs (overwrite Plan 02 stubs)** — `ebc544f` (feat)
   - Files: `crates/lacon-core/src/tracking/privacy.rs`, `crates/lacon-core/src/tracking/health.rs`
2. **Task 2: tracking_privacy.rs integration tests** — `8068fa6` (test)
   - Files: `crates/lacon-core/tests/tracking_privacy.rs`

**Plan metadata commit:** to be created after this SUMMARY.

## Files Created/Modified

### Created

- `crates/lacon-core/tests/tracking_privacy.rs` (93 lines) — 5 integration tests: marker creation, idempotent re-call, pre-existing marker silent-ok, missing-parent-dir TrackingError::Marker, concurrent-thread race smoke.

### Modified (overwrite of Plan 02 stubs)

- `crates/lacon-core/src/tracking/privacy.rs` (1-line stub → 168 lines):
  - `pub const MARKER_FILENAME = ".store_raw_outputs_acked"`
  - `pub fn resolve_marker_path` (D-14 layer priority)
  - `pub fn warn_once_if_needed` (atomic create + byte-stable warning)
  - `pub(crate) fn format_warning` (D-16 template, single source of truth)
  - `#[cfg(unix)] / #[cfg(not(unix))]` `marker_open_create_new` helpers
  - 4 unit tests: byte-exact warning template, project-wins-over-user, fall-back-to-user, both-off-returns-none

- `crates/lacon-core/src/tracking/health.rs` (1-line stub → 39 lines):
  - `pub struct HealthReport { pub select_one_returned: i32 }` (extension-friendly)
  - `pub fn health_check(&Connection) -> Result<HealthReport, TrackingError>`
  - 1 unit test: `health_check_against_in_memory_conn`

## Decisions Made

- **`format_warning` is `pub(crate)`** — not `pub`. Consumers go through `warn_once_if_needed`, which couples marker creation with warning emission as the spec intends. Tests reach `format_warning` directly via `super::*` for byte-exact assertions.
- **Marker mode bits `0o600` wrapped in `#[cfg(unix)]`** — provides defense-in-depth on Unix while keeping cross-platform compilation green (per RESEARCH §Filesystem & Permissions). v1 explicitly excludes Windows but a `cargo check` on Windows shouldn't fail to compile.
- **The concurrent-thread race test does NOT assert exactly-one-warning-printed** — it asserts the API contract (both threads return Ok, marker exists at end). Adding stderr-capture across threads would complicate the test without strengthening the privacy contract. The byte-exact warning template test (`format_warning_byte_exact_template`) is the strong assertion; the race test is the smoke check.

## Deviations from Plan

None — plan executed exactly as written.

The plan literal text used `MARKER_FILENAME` as `pub const` (not `pub(crate)`), enabling integration tests in `tracking_privacy.rs` to import it via `use lacon_core::tracking::privacy::{warn_once_if_needed, MARKER_FILENAME};` cleanly. This was explicit in the plan's `<action>` block.

---

**Total deviations:** 0
**Impact on plan:** Plan executed verbatim. Both tasks landed in 2 commits. All 14 acceptance-criteria grep targets pass; all 10 new tests pass; `cargo test --workspace` clean (173 → 183 tests, no regression).

## Issues Encountered

None.

Pre-existing rustdoc warning at `crates/lacon-core/src/rules/schema.rs:72` (logged by Plan 01) is **still out of scope** — Plan 03 did not touch `rules/`. Tracked in `.planning/phases/02-local-tracking/deferred-items.md`.

## Threat Model Compliance

- **T-02-08 (marker file leakage) — accept (no mitigation needed):** Marker is zero-byte (no content); parent dir is 0700 (Plan 04 enforces). The marker's existence reveals only that store_raw_outputs has been enabled — already user-visible from the config file.
- **T-02-09 (warning text drift) — mitigate:** `format_warning_byte_exact_template` test asserts the exact 4-line template character-by-character via String concatenation (catches both edits AND reordering). Any future edit that changes the template will fail this test.
- **T-02-10 (suppressed warning when stderr write fails) — accept:** Best-effort by design (D-12 posture). Marker is created BEFORE the stderr write; if the TTY is unreachable, the marker still exists so the notice will not repeat. The `let _ = std::io::stderr().write_all(...)` pattern is intentional — failure to write is silently swallowed.
- **T-02-11 (TOCTOU race on marker creation) — mitigate:** `OpenOptions::create_new(true)` is the atomic primitive — exactly one process creates the file, all others get AlreadyExists. `concurrent_calls_at_most_one_creates` test verifies the API-level contract; the OS-level atomicity is documented in RESEARCH §Privacy Marker File Semantics (line 491-558) and the Rust std docs.

No new threat surface introduced beyond the threat model.

## TDD Gate Compliance

Both tasks are `tdd="true"`. Following the project's established convention (Plan 01 + Plan 02 SUMMARY §"TDD Gate Compliance"): Task 1 is the production code (with co-located unit tests in `#[cfg(test)] mod tests`); Task 2 is the integration test file. RED would be "no privacy.rs / health.rs implementations exist" (cannot compile `tracking_privacy.rs` test that imports them); GREEN is the working pair. Committing production-then-test yields a clean two-step audit trail without breaking-build intermediates.

If a stricter three-commit RED/GREEN/REFACTOR audit trail is desired, mark the plan `type: tdd` (plan-level). The current commits are:

1. `ebc544f` (feat) — production: privacy.rs + health.rs (with co-located unit tests).
2. `8068fa6` (test) — verification: 5 integration tests covering marker semantics, idempotency, error paths, concurrency.

## User Setup Required

None — work is hermetic (no filesystem dependencies beyond `tempfile::TempDir`, no network, no external deps).

## Next Phase Readiness

- **Plan 02-04 (Tracker::open + prune + WAL)** is unblocked: `tracking::privacy::warn_once_if_needed` and `tracking::privacy::resolve_marker_path` are reachable from `Tracker::record` once Plan 04 wires the marker check inside the conditional `INSERT INTO raw_outputs` path. Plan 04 must call `warn_once_if_needed` BEFORE the first would-be raw_outputs INSERT (D-15: "Warning is checked exactly once per invocation").
- **Plan 02-05 (Tracker::record + CLI wire-up)** is unblocked: the privacy module is complete; Plan 05 just calls `warn_once_if_needed(&cfg, &marker)?` (with `?` on the structural error variant) inside the conditional raw-output path.
- **Phase 4 (lacon doctor)** is unblocked on health: `lacon_core::tracking::health::{health_check, HealthReport}` is reachable as a public API. Phase 4 can extend `HealthReport` additively without breaking Phase 2 callers (there are none).
- **Plan 02-06 (benchmarks + verification)** is unblocked.
- **No blockers.**

## Self-Check

All claimed artifacts verified to exist:

- `crates/lacon-core/src/tracking/privacy.rs` — FOUND (168 lines)
- `crates/lacon-core/src/tracking/health.rs` — FOUND (39 lines)
- `crates/lacon-core/tests/tracking_privacy.rs` — FOUND (93 lines)

All claimed commits verified in git log:

- `ebc544f` (Task 1) — FOUND on main
- `8068fa6` (Task 2) — FOUND on main

Verification commands re-run during self-check:

- `cargo check -p lacon-core` — 0 (clean)
- `cargo test -p lacon-core --lib tracking::privacy::tests` — 4 passed
- `cargo test -p lacon-core --lib tracking::health::tests` — 1 passed
- `cargo test -p lacon-core --test tracking_privacy` — 5 passed
- `cargo test --workspace` — 183 passed, 1 ignored (was 173 pre-Plan-03; +10 new tests, no Phase 1/2 regression)
- `grep -E 'create_new\s*\(\s*true\s*\)' crates/lacon-core/src/tracking/privacy.rs` — 2 matches (impl + cfg-not-unix branch)
- `grep -F 'lacon: store_raw_outputs is enabled' crates/lacon-core/src/tracking/privacy.rs` — multiple matches (template + test)
- `grep -F '~/.local/share/lacon/history.db' crates/lacon-core/src/tracking/privacy.rs` — 3 matches (doc + template + test)
- `git diff 5f9b3b1..HEAD --name-only` — confirms 3 files (privacy.rs, health.rs, tracking_privacy.rs); `tracking/mod.rs` NOT in the list (Issue #5 verified eliminated)

## Self-Check: PASSED

---
*Phase: 02-local-tracking*
*Plan: 03*
*Completed: 2026-05-06*
