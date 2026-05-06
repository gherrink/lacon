---
phase: 02-local-tracking
plan: 05
subsystem: tracking
tags: [rusqlite, sqlite, etcetera, xdg, privacy, env-vars, capture-before-move]

requires:
  - phase: 02-local-tracking
    provides: Tracker::open + 3-pragma contract + xdg_db_path + 24h-throttled prune (Plan 04); RawOutput type + rule_source_str helper + privacy::warn_once_if_needed + privacy::resolve_marker_path (Plans 02-03); InvocationMeta extended struct + tracking::normalize (Plan 01)
  - phase: 01-engine-core-lacon-run-wrapper
    provides: Runner::run + RunOutcome + ByteCounts; EngineConfig::load_layered; ResolvedRule with id + source: RuleSource (Clone, NOT Copy)
provides:
  - Tracker::record(meta, raw_opt, project_root, user_config_dir, project_store_raw, user_store_raw) -> Result<i64, TrackingError>
  - Conditional raw_outputs INSERT gated by cfg_store_raw_outputs && raw.is_some() (REQ-tracking-raw-outputs-default-off)
  - Privacy warning trigger via warn_once_if_needed BEFORE first raw_outputs INSERT (REQ-tracking-privacy-warning, D-15)
  - 17-column invocations INSERT with positional ?N binding
  - record_invocation helper in lacon-cli/src/commands/run.rs assembling InvocationMeta from RunOutcome + argv + env vars + tracking::normalize + load_layered
  - Capture-before-move pattern locking rule_id + rule_source via .clone() ahead of Runner::new (Issue #2 fix)
  - End-to-end CLI write path: lacon run -- echo hi → row in ~/.local/share/lacon/history.db
affects: [phase-02-plan-06-e2e-bench, phase-03-claude-code-adapter, phase-04-stats-explain-doctor]

tech-stack:
  added: [etcetera workspace dep on lacon-cli for XDG config dir resolution]
  patterns:
    - "Capture-before-move: clone Clone-but-not-Copy fields before fn moves the owner (Issue #2)"
    - "Best-effort tracker writes: match e { ... eprintln! ... return; } never alters wrapper exit code (D-12)"
    - "load_layered at every CLI run path so SC2 reachable via the wire (Issue #9)"
    - "Conditional FK INSERT pattern: raw INSERT first → captured rowid → invocations INSERT with FK"

key-files:
  created:
    - crates/lacon-core/tests/tracking_record.rs
  modified:
    - crates/lacon-core/src/tracking/record.rs
    - crates/lacon-cli/src/commands/run.rs
    - crates/lacon-cli/Cargo.toml

key-decisions:
  - "RuleSource import path: lacon_core::rules::loader::RuleSource (verified — no top-level re-export needed)"
  - "Per-layer split heuristic: when cfg.store_raw_outputs && project_config_path.is_some() → project layer; else user layer (D-14 project-wins, accepted trade-off vs. parse_partial pub(crate) leak)"
  - "raw=None always for v1 wire-up — capturing actual stdout bytes for INSERT lands in Phase 4's lacon explain work; v1 contract is invocation metadata only"
  - "Distinguish Marker errors from other tracker errors with separate stderr message for stderr clarity (still best-effort, never alters exit)"

patterns-established:
  - "Pattern: capture-before-move — clone Clone-but-not-Copy fields BEFORE the function call that moves the owner (RuleSource is Clone, NOT Copy)"
  - "Pattern: best-effort tracker boundary — eprintln!('lacon: tracker ...: {e}'); return; pattern repeats at each fallible call site (open, record); never propagate via ? to caller"
  - "Pattern: load_layered + filter(|p| p.exists()) — pass None when config files absent so load_layered short-circuits to defaults; fall back to Config::default() on validation errors silently (best-effort posture)"

requirements-completed: [REQ-tracking-raw-outputs-default-off, REQ-tracking-privacy-warning]

duration: 12min
completed: 2026-05-06
---

# Phase 02 Plan 05: Tracker Write Path + CLI Wire-up Summary

**Tracker::record dual-INSERT (raw_outputs gated by cfg + privacy warning + 17-col invocations INSERT) + lacon-cli wire-up assembling InvocationMeta from RunOutcome and persisting it best-effort after Runner::run returns.**

## Performance

- **Duration:** ~12 min
- **Started:** 2026-05-06T16:00:00Z
- **Completed:** 2026-05-06T16:12:00Z
- **Tasks:** 3 (record impl, CLI wire-up, integration tests)
- **Files modified:** 4 (record.rs overwritten, run.rs rewired, Cargo.toml dep added, tracking_record.rs created)

## Accomplishments

- **Tracker::record fully implemented.** Conditional raw_outputs INSERT gated by `cfg_store_raw_outputs && raw.is_some()`; privacy warning triggered via `warn_once_if_needed` BEFORE the first raw INSERT; 17-column invocations INSERT with `rule_source_str` enum mapping and `session_id.as_deref()` for the `Option<&str>` → `SQL NULL` binding (Pitfall #13). Plan 02 stub overwritten; `tracking/mod.rs` untouched (Plan 02 owns it, Issue #5 invariant held).
- **Capture-before-move pattern compiles cleanly.** `let rule_id = resolved.id.clone(); let rule_source = Some(resolved.source.clone());` lands in `run_with_rule` BEFORE `Runner::new(resolved, options)`. Confirmed `RuleSource` derives `Clone` but NOT `Copy` at `crates/lacon-core/src/rules/loader.rs:50` (Issue #2 fix verified).
- **load_layered invoked on every record path.** `record_invocation` calls `lacon_core::config::load_layered(project_config_path, user_config_path)` and reads `cfg.store_raw_outputs` + `cfg.retention` so SC2 ("flip project config to `store_raw_outputs: true` → marker + warning") is reachable end-to-end via the CLI (Issue #9 fix). Plan 06's e2e tests now have a CLI surface to exercise.
- **Both run paths persist invocations.** `run_with_rule` (rule-matched path) AND `run_unmatched` (no-rule path) call `record_invocation` after their respective subprocess returns. Synthetic `RunOutcome` with zero `byte_counts` for the unmatched path; no scope leakage.
- **Wall-time observation:** `lacon run -- echo hi` against an empty XDG-overridden tempdir DB measured at ~40ms wall in debug build (includes process start, DB creation, schema migration, prune, INSERT). Production release-build timing + cumulative cold start is Plan 06's bench target. DB row written successfully (id=1, assistant='claude-code', exit_code=0, raw_output_id=NULL — defaults correct).
- **`cargo test --workspace` is green.** 196 tests passed, 1 ignored, 27 suites (293s wall). No Phase 1 regression. The 7 new `tracking_record` integration tests run in 0.07s (sub-second; uses the bundled SQLite in-process).

## Task Commits

Each task was committed atomically:

1. **Task 1: Tracker::record dual-INSERT** — `7bac2f4` (feat)
2. **Task 2: lacon-cli wire-up + capture-before-move** — `dcaf01d` (feat)
3. **Task 3: 7 record-path integration tests** — `736cd76` (test)

**Plan metadata commit:** (this SUMMARY + STATE/ROADMAP) — final commit below.

## Files Created/Modified

- `crates/lacon-core/src/tracking/record.rs` — overwrote Plan 02 stub with `Tracker::record` (138 lines): conditional raw_outputs INSERT, privacy warning trigger, 17-column invocations INSERT, helper `insert_raw_output` + `insert_invocation` for clarity.
- `crates/lacon-cli/src/commands/run.rs` — added imports for config/error/runtime/tracking/RuleSource + UNIX_EPOCH; rewrote `run_with_rule` with capture-before-move; rewrote `run_unmatched` to also call `record_invocation` with synthetic `RunOutcome`; added `record_invocation` helper invoking `load_layered` + `Tracker::open` + `tracker.record` with best-effort error logging.
- `crates/lacon-cli/Cargo.toml` — added `etcetera = { workspace = true }` so the CLI can resolve XDG config dir for the privacy marker location.
- `crates/lacon-core/tests/tracking_record.rs` — 7 integration tests (298 lines): default-off short-circuit, FK linkage round-trip, raw=None skip, marker creation on first record, marker idempotent on second record, rule_source enum mapping for all 4 cases, full meta field round-trip across all 17 columns. 8 `tracker.conn` call sites (relies on Plan 04 Issue #1 `pub conn`).

## Decisions Made

- **`RuleSource` import path is `lacon_core::rules::loader::RuleSource`.** Verified during revision iteration 1 — no top-level re-export from `lacon_core::rules`. Plan 06's e2e tests should mirror this import.
- **Per-layer privacy gate split:** Phase 1's `load_layered` collapses project + user into a single `bool`. The CLI heuristic when `cfg.store_raw_outputs == true`:
  - `project_store_raw = project_config_path.is_some()` — project file exists → project layer wins (D-14).
  - `user_store_raw = !project_store_raw` — otherwise user layer.
  - `privacy::resolve_marker_path` then routes to the right layer's marker path. Trade-off accepted for v1; the Phase 1 `parse_partial` is `pub(crate)` so a precise per-layer split would require widening the API surface.
- **`raw=None` always for v1 CLI wire-up.** The wire of capturing actual stdout bytes for `raw_outputs` INSERT lands in Phase 4's `lacon explain` work. v1 contract is invocation metadata only; raw_outputs stays empty even with `store_raw_outputs: true` until Phase 4. The privacy warning trigger semantics still hold — they fire on the gate, not on the bytes.
- **Marker error vs. generic write error stderr split.** `tracker.record` may fail with `TrackingError::Marker` (privacy marker file write failed) or other errors (SQLite, etc.). Separate `eprintln!` messages for stderr clarity, but both swallow the error — exit code untouched (D-12).

## Deviations from Plan

None — plan executed exactly as written. The plan's literal-text actions for all three tasks compiled clean on the first attempt with no Rule 1/2/3 fixes required.

## Issues Encountered

None.

## Verification

- `cargo check --workspace` exits 0
- `cargo build --workspace` exits 0
- `cargo build -p lacon-cli` exits 0
- `cargo test -p lacon-core --test tracking_record` exits 0 (7/7 tests pass)
- `cargo test --workspace` exits 0 (196 passed, 1 ignored, no regressions)
- All grep acceptance criteria pass for run.rs (`fn record_invocation`, `tracking::Tracker::open`, `tracker.record`, `LACON_ASSISTANT`, `LACON_SESSION_ID`, `"claude-code"` literal, `tracking::normalize(&argv)`, `xdg_db_path`, `lacon: tracker` × 6, `let rule_id = resolved.id.clone();`, `let rule_source = Some(resolved.source.clone());`, `config::load_layered`, `cfg.store_raw_outputs`, `cfg.retention`)
- All grep acceptance criteria pass for record.rs (`pub fn record`, `INSERT INTO raw_outputs`, `INSERT INTO invocations`, `warn_once_if_needed`, `resolve_marker_path`, `rule_source_str`, `session_id.as_deref()`)
- `tracker.conn` call site count in `tracking_record.rs`: 8 (≥6 required)
- End-to-end smoke: `lacon run -- echo hi` writes a row to `~/.local/share/lacon/history.db`; `assistant='claude-code'`, `exit_code=0`, `raw_output_id=NULL` (defaults correct)

## User Setup Required

None — `lacon-cli` resolves XDG paths automatically via `etcetera::choose_base_strategy`. The DB is created on first `lacon run` invocation; the parent dir is chmodded to `0700` by Plan 04's `Tracker::open`.

## Next Phase Readiness

Ready for Plan 06 (Phase 2 e2e + bench). Plan 06's e2e SC2 ("flipping project config to `store_raw_outputs: true` for the first time prints stderr privacy notice + writes marker") now has a working CLI surface — the `record_invocation` → `load_layered` → `cfg.store_raw_outputs` → `tracker.record` → `privacy::warn_once_if_needed` chain is fully wired.

**No blockers for Plan 06.** Cold-start budget for Plan 07 bench is healthy: debug-build wall time on the happy path (`lacon run -- echo hi` end-to-end with DB creation) is ~40ms; release build with persistent DB will be substantially faster.

## Self-Check: PASSED

- `crates/lacon-core/src/tracking/record.rs` — FOUND (138 lines, `pub fn record` present, both INSERT statements present, privacy hooks present)
- `crates/lacon-cli/src/commands/run.rs` — FOUND (capture-before-move pattern present, `record_invocation` helper present, `load_layered` invoked, all 6 `lacon: tracker` stderr prefixes present)
- `crates/lacon-cli/Cargo.toml` — FOUND (`etcetera = { workspace = true }` present)
- `crates/lacon-core/tests/tracking_record.rs` — FOUND (7 test fns, 8 `tracker.conn` call sites)
- Commit `7bac2f4` (record.rs feat) — FOUND
- Commit `dcaf01d` (CLI wire-up feat) — FOUND
- Commit `736cd76` (integration tests) — FOUND

---
*Phase: 02-local-tracking*
*Completed: 2026-05-06*
