---
phase: 02-local-tracking
plan: 01
subsystem: database
tags: [rusqlite, sqlite, tracking, normalize, thiserror, scaffold]

# Dependency graph
requires:
  - phase: 01-engine-core-lacon-run-wrapper
    provides: InvocationMeta struct (extended additively per D-03), thiserror precedent, etcetera workspace dep
provides:
  - "rusqlite 0.39 with bundled feature wired into the workspace and inherited by lacon-core"
  - "TrackingError enum (5 variants: CreateDir, Chmod, Sqlite[#[from] rusqlite::Error], Marker, Clock) in lacon-core::error"
  - "InvocationMeta extended with 5 Phase 2 fields (assistant, session_id, project_path, command_normalized, raw_output_id) — additive, no Phase 1 callers existed"
  - "lacon-core::tracking module declared with public Tracker struct skeleton, RawOutput type, normalize() pure fn, rule_source_str helper"
  - "command_normalized derivation (D-18 conservative algorithm) tested with 7 unit + 3 integration fixtures"
affects: [02-02, 02-03, 02-04, 02-05, 02-06, phase-03-adapter, phase-04-cli-stats-explain-doctor]

# Tech tracking
tech-stack:
  added:
    - "rusqlite 0.39 + bundled feature (libsqlite3-sys ships SQLite 3.51.3 amalgamation)"
  patterns:
    - "Module facade: tracking/mod.rs re-exports normalize from submodule (mirrors rules/mod.rs)"
    - "Pure helper fn lives in its own file when test fixture coverage is non-trivial (normalize.rs, 7 unit cases)"
    - "Public API integration tests (tests/tracking_normalize.rs) guard against future pub(crate) regressions"
    - "Additive struct extension via D-03 — never redefine InvocationMeta; downstream waves gain new fields, never replace existing"

key-files:
  created:
    - "crates/lacon-core/src/tracking/mod.rs"
    - "crates/lacon-core/src/tracking/normalize.rs"
    - "crates/lacon-core/tests/tracking_normalize.rs"
    - ".planning/phases/02-local-tracking/deferred-items.md"
  modified:
    - "Cargo.toml (workspace.dependencies: +rusqlite 0.39 bundled)"
    - "Cargo.lock (rusqlite + libsqlite3-sys + transitive deps pinned)"
    - "crates/lacon-core/Cargo.toml (+rusqlite workspace inheritance)"
    - "crates/lacon-core/src/lib.rs (+pub mod tracking + module-map doc)"
    - "crates/lacon-core/src/error.rs (+TrackingError enum + 3 tests)"
    - "crates/lacon-core/src/runtime/mod.rs (InvocationMeta +5 Phase 2 fields)"

key-decisions:
  - "rusqlite[bundled] picked over [system] (D-07): hermetic — no libsqlite3-dev / version skew; ~1 MiB binary cost accepted for v1 since cold start is the load-bearing budget, not size."
  - "TrackingError::Sqlite uses #[from] rusqlite::Error so internal tracking code uses ? cleanly; CLI boundary will use eprintln! + swallow per D-12 (Plan 02-05)."
  - "InvocationMeta extension is strictly additive — no Phase 1 callers (only the def site touched), confirmed via grep -rn 'InvocationMeta' returning 1 hit."
  - "Tracker struct is a deliberate skeleton with one private bool field — public API surface (struct name) is stable from day one so downstream plans (02-02..02-04) layer methods without forward-reference dance."
  - "normalize() is a free pub fn (not a method) — pure, no error type, mirrors rules/loader::strip_layer_prefix shape; fixture-tested at unit and integration level."

patterns-established:
  - "Phase 2 module placement: tracking/ as a sibling of rules/, runtime/, etc. (D-01)."
  - "include_str! pattern reserved for migrations/ subdirectory (Plan 02-02 will add 0001_initial.sql)."
  - "Best-effort tracker contract (D-12) telegraphed via the TrackingError doc comment — no Phase 2 caller propagates via ?."

requirements-completed:
  - REQ-tracking-schema  # foundational scaffolding only; full schema lands in 02-02

# Metrics
duration: ~10min
completed: 2026-05-06
---

# Phase 2 Plan 1: Tracking Foundation Summary

**rusqlite 0.39 wired into the workspace, lacon-core::tracking module scaffolded with a pure D-18 normalize() helper, and InvocationMeta extended additively with 5 Phase 2 fields — buildable and 73-test green with zero Phase 1 regression.**

## Performance

- **Duration:** ~10 min (569 s wall)
- **Started:** 2026-05-06T14:03:39Z
- **Completed:** 2026-05-06T14:13:08Z
- **Tasks:** 2 (both `type="auto" tdd="true"` — combined RED/GREEN per task since the test gates are compile + same-file unit tests)
- **Files modified:** 6 (5 source + Cargo.lock)
- **Files created:** 4 (3 source + 1 deferred-items log)

## Accomplishments

- **rusqlite reachable from lacon-core** with `bundled` feature — workspace declared `rusqlite = { version = "0.39", features = ["bundled"] }`, lacon-core inherits via `{ workspace = true }`. `cargo check -p lacon-core` succeeds; first cold-cache compile of `libsqlite3-sys` took roughly the time of `cargo check`'s 13.37s wall (subsequent incremental builds are cache hits).
- **TrackingError enum** added after RuntimeError in `error.rs` with 5 variants — `CreateDir`/`Chmod`/`Sqlite { #[from] }`/`Marker`/`Clock`. Three new tests cover: byte-exact Display for `Clock`, prefix-and-message for `CreateDir`, and `?`-friendly `From<rusqlite::Error>` round-trip via `matches!`.
- **InvocationMeta extended additively (D-03)** with `assistant: String`, `session_id: Option<String>`, `project_path: Option<PathBuf>`, `command_normalized: String`, `raw_output_id: Option<i64>`. Phase 1 has no `InvocationMeta` constructor sites (verified by grep), so the extension breaks nothing.
- **lacon-core::tracking module declared** in `lib.rs` with module-map doc comment updated. `tracking::Tracker` struct skeleton, `RawOutput` type, and `rule_source_str()` helper exposed publicly. Plans 02-02 through 02-04 will add methods without breaking the public API surface.
- **normalize() (D-18 conservative algorithm)** lives at `tracking/normalize.rs` as a pure free function. Seven unit-test fixtures cover: pnpm-install-with-flag, abs-path basename, cargo -V flag-only, single-arg, cargo test --release, rel-path basename, empty argv. Three integration tests in `tests/tracking_normalize.rs` re-assert via the public crate boundary.

## Task Commits

1. **Task 1: rusqlite + TrackingError + InvocationMeta extension** — `bd8ca13` (feat)
2. **Task 2: tracking module scaffold + normalize** — `c120c41` (feat)

**Plan metadata commit:** to be created after this SUMMARY.

_Note: Each task was a TDD task; the "test gate" for Task 1 is the compile gate plus the in-file `#[cfg(test)] mod tests` block (unit + Display + #[from] coverage), which fails-then-passes against the same edit cycle. Task 2's test gate is the integration test file plus inline unit tests. Following the project's existing convention of co-located error-tests, no separate test-only commit was warranted — both tasks ship as `feat` commits with tests included._

## Files Created/Modified

### Created
- `crates/lacon-core/src/tracking/mod.rs` — module facade: Tracker skeleton (one private bool field), RawOutput, rule_source_str, re-exports normalize.
- `crates/lacon-core/src/tracking/normalize.rs` — pure D-18 algorithm + 7 unit tests + 3 doctest examples.
- `crates/lacon-core/tests/tracking_normalize.rs` — 3 public-API integration tests.
- `.planning/phases/02-local-tracking/deferred-items.md` — pre-existing rustdoc warning (Phase 1 leftover) logged for future cleanup.

### Modified
- `Cargo.toml` — workspace.dependencies: `rusqlite = { version = "0.39", features = ["bundled"] }` inserted alphabetically between `etcetera` and `rust-embed`.
- `Cargo.lock` — rusqlite, libsqlite3-sys, bindgen, and transitive C-build deps pinned by `cargo check`.
- `crates/lacon-core/Cargo.toml` — `rusqlite = { workspace = true }` appended after thiserror with a Phase 2 explanatory comment.
- `crates/lacon-core/src/lib.rs` — `pub mod tracking;` declared (last in module list); module-map doc updated to add `tracking` and `+ TrackingError` on the `error` line.
- `crates/lacon-core/src/error.rs` — TrackingError enum inserted after RuntimeError, before `impl ValidationError`. Three new `tracking_error_*` tests appended in the existing `mod tests` block.
- `crates/lacon-core/src/runtime/mod.rs` — InvocationMeta extended additively with 5 Phase 2 fields and a Phase-2-aware doc comment.

## Decisions Made

None — plan was specified to literal text level (action steps + verify-all). All four edits land verbatim from the plan's `<action>` blocks.

Notable design points carried forward from CONTEXT/RESEARCH (no deviation):
- `Tracker` struct is **public from day one** with a stable name; only fields are private and internal layout will change in 02-04. This avoids a `pub(crate)` → `pub` migration later.
- Normalize is a **free fn** (not a method on a NormalizeStrategy or similar) — matches the spec wording "implementation-defined" and the Phase 1 precedent (`strip_layer_prefix`).
- TrackingError variant ordering matches RESEARCH §"Error mapping" verbatim — verifier-friendly grep targets.

## Deviations from Plan

None — plan executed exactly as written.

The `Cargo.lock` update was an expected side-effect of adding the rusqlite workspace dep; staging the lockfile alongside the manifest changes is standard project practice (verified against the existing pattern: `Cargo.lock` was updated in earlier Phase 1 commits e.g. when nix/signal-hook landed).

The `#[allow(dead_code)]` on `Tracker.cfg_store_raw_outputs` is **expected** — Plan 02-04 wires the read path. Documented inline in the struct.

---

**Total deviations:** 0
**Impact on plan:** Plan executed verbatim. Acceptance criteria met for both tasks. No scope creep.

## Issues Encountered

None blocking.

**One pre-existing rustdoc warning** discovered during the Plan 02-01 acceptance check (`cargo doc -p lacon-core --no-deps --document-private-items`):

- `crates/lacon-core/src/rules/schema.rs:72` — doc comment `/// Exact match against argv[0] basename.` triggers `unresolved link to '0'` because rustdoc parses `[0]` as an intra-doc link.
- This warning is **not from Phase 2** — the file is not in this plan's `files_modified` and was not touched. Phase 2 doesn't widen the warning surface.
- Acceptance criterion (`cargo doc ... | grep -i error | wc -l returns 0`) is satisfied (0 errors).
- Logged in `.planning/phases/02-local-tracking/deferred-items.md` for future Phase 1 cleanup. Suggested fix: backtick or escape `argv[0]` in the doc comment.

## Threat Model Compliance

- **T-02-01 (rusqlite supply chain) — accept:** rusqlite pinned to `0.39` exactly via workspace dep; `Cargo.lock` updated and committed alongside the manifest change. Per-monitoring deferred to Phase 6 acceptance.
- **T-02-02 (InvocationMeta extra fields) — accept:** All new fields are scalar/Option; no PII surface added beyond what Phase 1 already accepted in `command_raw`.
- **T-02-03 (TrackingError #[from] rusqlite::Error) — mitigate:** `?`-conversion is intentional inside `tracking/`. CLI boundary (Plan 02-05) will surface failures via `eprintln!` + swallow per D-12; verified by the doc comment on `pub enum TrackingError` ("Best-effort consumers (CLI commands/run.rs) log via `eprintln!` and never propagate via `?`").

No new threat surface beyond the threat model.

## TDD Gate Compliance

Both tasks are `tdd="true"`. Per the plan's `<behavior>` blocks the test gates are:

- **Task 1:** compile gate (RED would be "rusqlite missing → cannot use rusqlite::Error in #[from]"; GREEN is the working compile + 3 new error tests passing). Bundled into a single `feat` commit per project convention; the same-file `mod tests` block is the test artifact.
- **Task 2:** unit + integration tests (RED would be "normalize undefined / pub(crate)"; GREEN is the working tracking module + 7 unit + 3 integration tests passing). Bundled into a single `feat` commit.

This is the project's established pattern for additive type-level work where tests live in the same edit as the implementation. The plan's `<action>` blocks specify the exact text in both production and test, so test-and-implementation are co-authored and committing them apart would create non-compiling intermediate states.

If a strict three-commit RED/GREEN/REFACTOR audit trail is desired in future phases, mark plans `type: tdd` (plan-level) rather than per-task — the gate enforcement is plan-level in execute-plan.md.

## User Setup Required

None — no external service configuration. `cargo build` is hermetic via `bundled`.

## Next Phase Readiness

- **Plan 02-02** (schema migration) can now `use lacon_core::error::TrackingError;` and `use rusqlite::{...}` directly. The `Tracker` struct exists for `impl Tracker` blocks to attach `pub fn open(...)`, `pub fn record(...)`, `pub fn prune(...)`, `pub fn health_check(...)`. The `migrations/` subdirectory under `crates/lacon-core/src/tracking/` is open territory for `0001_initial.sql` + `migrations.rs`.
- **Plan 02-05** (CLI wire-up) can populate the 5 new `InvocationMeta` fields from env vars (`LACON_ASSISTANT`, `LACON_SESSION_ID`) and `std::env::current_dir()` per D-17. `tracking::normalize::normalize(&argv)` is callable from the CLI assembly site without further work.
- **Cold-start invariant (D-04) preserved.** No `Tracker::open` exists yet, so `lacon --version` / `lacon validate` cannot accidentally touch the DB. Plan 02-05 must place `Tracker::open` exclusively in `lacon-cli::commands::run` and provide a `tracking_coldstart.rs` negative test.
- **No blockers.** Plan 02-02 is unblocked.

## Self-Check: PASSED

All claimed artifacts verified to exist:

- `crates/lacon-core/src/tracking/mod.rs` — FOUND
- `crates/lacon-core/src/tracking/normalize.rs` — FOUND
- `crates/lacon-core/tests/tracking_normalize.rs` — FOUND
- `.planning/phases/02-local-tracking/deferred-items.md` — FOUND

All claimed commits verified in git log:

- `bd8ca13` (Task 1) — FOUND on main
- `c120c41` (Task 2) — FOUND on main

Verification commands re-run during self-check:

- `cargo check -p lacon-core` — 0 (clean)
- `cargo test -p lacon-core --lib` — 73 passed (was 65 pre-Phase-2)
- `cargo test -p lacon-core --test tracking_normalize` — 3 passed
- `cargo test --workspace` — 162 passed, 1 ignored (no Phase 1 regression)
- `cargo doc -p lacon-core --no-deps --document-private-items 2>&1 | grep -i error | wc -l` — 0

---
*Phase: 02-local-tracking*
*Plan: 01*
*Completed: 2026-05-06*
