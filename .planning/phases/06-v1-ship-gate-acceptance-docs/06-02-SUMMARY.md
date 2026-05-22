---
phase: 06-v1-ship-gate-acceptance-docs
plan: 02
subsystem: infra
tags: [benchmark, criterion, cold-start, github-actions, ci, sqlite, hermetic]

# Dependency graph
requires:
  - phase: 02-local-tracking
    provides: "tracker_open criterion bench + Tracker::open + migrate() early-return on existing DB"
  - phase: 03-claude-code-adapter
    provides: "lacon-claude-hook binary + cold_start_probe hook scenarios"
provides:
  - "Steady-state tracker_open bench variant with the cold-start budget gate re-targeted onto it (D-05)"
  - "Committed, reproducible cold-start benchmark entry point (scripts/bench-cold-start.sh) exercising the lacon run hook hot path (D-04)"
  - "Hermetic GitHub Actions CI on ubuntu-latest + macos-latest lanes (D-08/D-09)"
  - "Cold-start measurement protocol + Linux numbers in docs/architecture.md"
affects: [phase 06 verification, phase 06 code review, ship gate SC1/SC4]

# Tech tracking
tech-stack:
  added: [GitHub Actions (actions/checkout@v4)]
  patterns:
    - "Steady-state vs first-ever benchmark split: create the DB once outside the timed loop; gate on the re-open number"
    - "Soft-reported wall-clock cold-start (min-of-N) vs deterministic in-process criterion hard gate"
    - "Hermetic-by-construction CI: pre-installed Rust only, default (non-ignored) test set, no package-manager fetch"

key-files:
  created:
    - "scripts/bench-cold-start.sh"
    - ".github/workflows/ci.yml"
    - ".planning/phases/06-v1-ship-gate-acceptance-docs/deferred-items.md"
  modified:
    - "crates/lacon-core/benches/tracker_open.rs"
    - "docs/architecture.md"

key-decisions:
  - "D-05 is a measurement-protocol change, not a source edit: migrate() early-returns on an existing DB, so steady-state Tracker::open has no migration-COMMIT fsync; the budget gate moves onto the steady-state variant and first-ever DB creation becomes a reported-only diagnostic."
  - "The hook wall-clock cold-start figure (~12ms min in the probe) is spawn-dominated measurement overhead, not hook execution (strace: ~0.3ms of actual hook syscall work). It is soft-reported; the deterministic hard gate is the in-process tracker_open_steady_state criterion bench."
  - "CI runs the DEFAULT (non-ignored) test set and adds no package-manager fetch step; rusqlite[bundled] vendors SQLite, and the interactive/real-pnpm tests stay #[ignore]d so the npm registry is never reached."

patterns-established:
  - "Steady-state benchmark: pay one-time setup outside the timed loop, then time the hot-path repeat; gate on the hot-path number."
  - "Cold-start reporting split: in-process criterion = deterministic hard gate; subprocess wall-clock = soft min-of-N report (per-OS labeled)."

requirements-completed: [REQ-acceptance-cold-start-budget, REQ-acceptance-test-coverage]

# Metrics
duration: ~10min
completed: 2026-05-22
---

# Phase 6 Plan 02: v1 ship-gate infrastructure (cold-start gate + hermetic CI) Summary

**Resolved the deferred Phase 2 tracker_open regression by gating the cold-start budget on steady-state Tracker::open (~208µs vs 3700µs budget), wrapped the existing cold_start_probe in a committed reproducible entry point, and stood up hermetic ubuntu + macos GitHub Actions CI — surfacing a pre-existing test-infra breakage that blocks the workspace test suite from going green.**

## Performance

- **Duration:** ~10 min (active execution; excludes release-build/bench compile waits)
- **Started:** 2026-05-22T09:40Z (approx)
- **Completed:** 2026-05-22T09:50Z
- **Tasks:** 3 completed
- **Files modified/created:** 5 (2 modified, 3 created)

## Accomplishments
- **D-05 resolved:** `tracker_open` now has a `tracker_open_steady_state` variant (DB created once outside the timed loop, re-open timed inside) carrying the hard `assert!(mean < BUDGET_MICROS)` gate. Measured steady-state mean ~208µs (criterion median 208.47µs), far under the 3700µs budget. The original first-run bench is demoted to a reported-only diagnostic — first-ever DB creation no longer breaks the build. No edit to the `Tracker::open` source path (confirmed by empty `git diff --stat crates/lacon-core/src/`).
- **D-04 delivered:** `scripts/bench-cold-start.sh` is a committed, executable thin wrapper that builds both release binaries and runs `cold_start_probe`, exercising the `lacon run` hook hot path. `docs/architecture.md` gained a "Cold-start measurements (Phase 6 ship gate)" section with the first-ever-vs-steady-state protocol and the Linux numbers.
- **D-08/D-09 delivered:** `.github/workflows/ci.yml` runs a matrix over `ubuntu-latest` + `macos-latest`, each lane doing `cargo build --release` + `cargo test --workspace` (default set) + the `tracker_open` hard gate + `scripts/bench-cold-start.sh`. Hermetic by construction (no package-manager fetch, no `--ignored`), least-privilege `permissions: contents: read`, no secrets, `actions/checkout@v4` pinned.

## Task Commits

Each task was committed atomically:

1. **Task 1: Split tracker_open into steady-state vs first-ever, re-target gate (D-05)** - `2f12388` (perf)
2. **Task 2: Committed cold-start benchmark entry point + measurement protocol (D-04)** - `b010b1b` (feat)
3. **Task 3: Hermetic ubuntu + macos CI lanes (D-08/D-09)** - `c32f479` (ci)

_Note: STATE.md / ROADMAP.md are NOT updated here — the orchestrator owns those after the wave completes._

## Files Created/Modified
- `crates/lacon-core/benches/tracker_open.rs` (modified) - added `bench_tracker_open_steady_state` (the new hard gate), demoted `bench_tracker_open` to reported-only, registered both in `criterion_group!`.
- `scripts/bench-cold-start.sh` (created, executable) - reproducible cold-start entry point; builds both release bins then runs `cold_start_probe`, echoing the per-OS-labeled markdown table.
- `docs/architecture.md` (modified) - added the Phase 6 cold-start measurements section: first-ever vs steady-state protocol, in-process hard gate vs soft wall-clock report, Linux numbers, macOS row to be filled from CI.
- `.github/workflows/ci.yml` (created) - hermetic two-OS-lane CI with build/test/bench/cold-start steps and an inline hermeticity contract.
- `.planning/phases/06-v1-ship-gate-acceptance-docs/deferred-items.md` (created) - logs the pre-existing `CARGO_BIN_EXE_test_emitter` test-infra breakage.

## Decisions Made
- **Steady-state split is measurement-only, not a source change.** `migrate()` (`migrations.rs:41-43`) early-returns when `PRAGMA user_version >= TARGET_VERSION`, so the second-and-later `Tracker::open` does no `BEGIN IMMEDIATE`/`COMMIT` and pays no migration fsync. The D-05 fix is therefore a new bench variant plus a gate re-target. Verified: no diff under `crates/lacon-core/src/`.
- **Hook wall-clock cold start is a soft report, not a hard gate.** The probe's hook scenarios show ~12ms min, but an `strace -c` of one hook run shows ~0.3ms of actual hook syscall work — the rest is `Command::spawn` + piped-stdio + scheduler latency under the probe's tight loop on a loaded 16-core box. The deterministic gate is the in-process `tracker_open_steady_state` criterion bench (208µs). This matches RESEARCH Pitfall 1 (no `<10ms` wall-clock build-breaker on shared VMs). The adapter hook does not itself open the tracker; `Tracker::open` lives in `lacon run`.

## Deviations from Plan

None — plan executed exactly as written. (No Rule 1/2/3 auto-fixes were needed within the plan's own files; the one notable discovery is a pre-existing out-of-scope failure documented under "Issues Encountered" and `deferred-items.md`.)

## Issues Encountered

**Pre-existing test-suite breakage: `CARGO_BIN_EXE_test_emitter is unset` (OUT OF SCOPE — logged, not fixed).**

While validating that the new CI workflow's `cargo test --workspace` step would pass locally, the `lacon-cli` integration tests (`cli_doctor.rs`, `end_to_end.rs`, `tracking_e2e.rs`) failed with:

```
`CARGO_BIN_EXE_test_emitter` is unset
help: available binary names are "lacon"
```

- **Root cause (pre-existing):** `assert_cmd` is pinned to `2.2.1`; its `cargo_bin("test_emitter")` reads `CARGO_BIN_EXE_test_emitter` and panics if unset. Cargo only sets that env var for **artifact dependencies** (`artifact = "bin"`), not the plain `[dev-dependencies]` path entry currently used (`crates/lacon-cli/Cargo.toml:27`). On rustc 1.95.0 + assert_cmd 2.2.1 the var is never populated.
- **Why not fixed:** Out of scope per the SCOPE BOUNDARY rule. This plan's files are the bench, the cold-start script, `docs/architecture.md`, and the CI workflow — none touch `lacon-cli` test wiring, `Cargo.toml` dev-deps, or the `assert_cmd` pin. Fixing it is a test-infrastructure change (artifact-dependency conversion or `assert_cmd` bump) for a follow-up plan.
- **Impact:** The CI workflow is correct per the plan spec, but its `cargo test --workspace` step will go RED on the first CI run until this pre-existing bug is fixed. **SC4's "CI runs the hermetic test suite green" clause is blocked by this pre-existing breakage, not by anything Plan 06-02 changed.** Everything else passes: lacon-core unit tests, the adapter/hook/chain/tui integration tests, the `tracker_open` hard gate, `cargo build --release`, and the cold-start probe step. Logged in `deferred-items.md` with a recommended follow-up.

## Verification Results

- **Task 1 (D-05):** `cargo bench -p lacon-core --bench tracker_open` exits 0; prints `tracker_open_steady_state` (mean ~208µs < 3700µs budget); `git diff --stat crates/lacon-core/src/` empty.
- **Task 2 (D-04):** `scripts/bench-cold-start.sh` is executable, passes `bash -n`, contains `cargo build --release`, runs `cargo run --release --bin cold_start_probe`, emits a per-OS-labeled markdown table, exits 0 on Linux. `docs/architecture.md` contains the literal `steady-state`, a Linux/macOS table, and the first-ever-vs-steady-state protocol paragraph.
- **Task 3 (D-08/D-09):** `.github/workflows/ci.yml` parses as valid YAML; has `ubuntu-latest` + `macos-latest` lanes; runs build/test(no `--ignored`)/`tracker_open` bench/`bench-cold-start.sh`; the plan's `! grep -nE 'brew install|npm i|npm install|pip install|apt-get install|--ignored'` gate passes; `permissions: contents: read`; no `secrets.*`; `actions/checkout@v4` pinned.
- **Overall:** all `must_haves` artifact `contains` checks and `key_links` confirmed present.

## Known Stubs

None. (No hardcoded empty values or placeholder data wired to UI/output were introduced; the macOS docs table cells are explicitly-labeled `_(CI macos-latest)_` placeholders to be populated from the macOS CI lane, which is the documented protocol, not a stub.)

## Threat Flags

None. The CI workflow stays inside the plan's `<threat_model>`: only `actions/checkout@v4` (pinned), no third-party actions, no `secrets.*`, least-privilege `permissions`, no package-manager fetch steps (T-06-CI-01/02/SC mitigations satisfied). The cold-start macOS number is soft-reported, not hard-asserted (T-06-CI-03 accepted-by-design).
