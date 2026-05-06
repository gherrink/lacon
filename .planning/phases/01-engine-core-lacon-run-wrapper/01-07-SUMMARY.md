---
phase: 01-engine-core-lacon-run-wrapper
plan: "07"
subsystem: integration-testing
tags: [rust, assert_cmd, test_emitter, cold-start, benchmarks, open-questions, docs]

requires:
  - phase: 01-engine-core-lacon-run-wrapper
    plan: "01"
    provides: workspace Cargo.toml, shared deps
  - phase: 01-engine-core-lacon-run-wrapper
    plan: "02"
    provides: Pipeline + native primitives (strip_ansi, drop_regex, keep_regex, max_bytes)
  - phase: 01-engine-core-lacon-run-wrapper
    plan: "03"
    provides: RuleLoader, ResolvedRule, validate_file
  - phase: 01-engine-core-lacon-run-wrapper
    plan: "04"
    provides: StarlarkScript, Pipeline::run_with_post_process
  - phase: 01-engine-core-lacon-run-wrapper
    plan: "05"
    provides: Runner::run, on_error swap, LACON_DISABLE bypass, signal forwarding
  - phase: 01-engine-core-lacon-run-wrapper
    plan: "06"
    provides: lacon binary with all 6 subcommands

provides:
  - bin/test_emitter binary: deterministic stdout+stderr emitter for integration tests
  - crates/lacon-cli/tests/end_to_end.rs: 5 workspace-level e2e tests via assert_cmd
  - benches/cold_start.rs: cold-start probe measuring lacon --version and lacon validate
  - docs/architecture.md: Cold-start measurements (Phase 1) section with actual µs values, four CONTEXT.md benchmark item decisions, D-11/D-12 documentation
  - docs/open-questions.md: Q-deferred-merge-ordering and Q-deferred-signal-forwarding moved to Resolved

affects: [02-sqlite-tracker, 03-adapter-claudecode, 05-bundled-rules, 06-acceptance]

tech-stack:
  added: []  # All deps already in workspace; benches crate has no deps
  patterns:
    - "assert_cmd::cargo::cargo_bin for PATH-independent binary lookup in integration tests"
    - "Workspace member bin/* glob covers all helper binaries used in tests"
    - "Cold-start probe: Instant::now + Command::output loop, percentile via sort"

key-files:
  created:
    - bin/test_emitter/Cargo.toml
    - bin/test_emitter/src/main.rs
    - crates/lacon-cli/tests/end_to_end.rs
    - benches/Cargo.toml
    - benches/cold_start.rs
  modified:
    - Cargo.toml (workspace members: added bin/*, benches)
    - crates/lacon-cli/Cargo.toml (added test_emitter dev-dep for cargo_bin lookup)
    - docs/architecture.md (Cold-start measurements (Phase 1) section appended)
    - docs/open-questions.md (two deferred items resolved)

key-decisions:
  - "Use assert_cmd::cargo::cargo_bin not env!(CARGO_BIN_EXE_test_emitter): CARGO_BIN_EXE_* is only set for [[bin]] targets in the same crate; assert_cmd uses legacy_cargo_bin fallback from target_dir() which finds workspace binaries correctly"
  - "lacon --version median 1154 µs, lacon validate median 1259 µs — both well under 10ms Phase 6 budget; clap derive plan-B not triggered"
  - "D-11 resolved: best-effort line atomicity, no cross-stream order guarantee (single os_pipe FIFO)"
  - "D-12 resolved: SIGTERM/SIGINT forwarded via nix::kill, no drain, exit 128+sig"

requirements-completed:
  - REQ-engine-streaming-primitives
  - REQ-engine-on-error
  - REQ-engine-bypass
  - REQ-cli-run
  - REQ-cli-validate

duration: 6min
completed: "2026-05-06"
---

# Phase 01 Plan 07: Cross-cutting verification Summary

**5 workspace-level e2e integration tests via assert_cmd + test_emitter binary + cold-start probe baseline (lacon --version 1154 µs median, lacon validate 1259 µs median — well under 10ms Phase 6 gate) + D-11/D-12 open questions resolved**

## Performance

- **Duration:** 6 min
- **Started:** 2026-05-06T09:21:42Z
- **Completed:** 2026-05-06T09:28:02Z
- **Tasks:** 2 (Task 1 TDD, Task 2 docs/probe)
- **Files modified:** 9 (5 created, 4 modified)

## Cold-start Measurements (Actual)

Measured 2026-05-06 on Linux 6.8.0-111-generic (AMD Ryzen 7 5800X 8-Core Processor). Release build with `opt-level = "z"` + `lto = "thin"` + `strip = "symbols"`. 50 samples per scenario, 3 warm-up runs discarded.

| Command | min | median | p95 | max |
|---------|-----|--------|-----|-----|
| `lacon --version` | 982 µs | 1154 µs | 1301 µs | 1323 µs |
| `lacon validate <rule>` | 1082 µs | 1259 µs | 1401 µs | 1635 µs |

Both scenarios are ~8-9x under the Phase 6 10ms budget. The dominant cost is process startup + dynamic linking; the clap parse and rule loader code paths add only ~100 µs incremental overhead on top of `--version`. **Plan-B (pico-args) is not warranted.**

## Accomplishments

- `bin/test_emitter` workspace binary: deterministic emitter with `--stdout-lines`, `--stderr-lines`, `--mix`, `--ansi`, `--errors`, `--exit`, `--bytes` flags — covers all Phase 1 test scenarios
- 5 end-to-end integration tests at `crates/lacon-cli/tests/end_to_end.rs` via `assert_cmd`:
  1. `end_to_end_strip_ansi_and_drop_stderr` — strip_ansi + drop_regex combo; ANSI codes absent from output
  2. `end_to_end_on_error_swap_with_failing_subprocess` — on_error pipeline activated on exit 1; keep_regex passes FAIL lines
  3. `end_to_end_max_bytes_truncation_marker_byte_exact` — 10KB emitter capped to 200 bytes; byte-exact truncation marker present
  4. `end_to_end_validate_then_run` — round-trip: validate exits 0, no stderr; subsequent run produces expected output
  5. `end_to_end_lacon_disable_propagates_subprocess_exit` — LACON_DISABLE=1 bypass; exit code 5 propagated
- `benches/cold_start.rs`: operator-level cold-start probe printing markdown table (50 samples + 3 warm-up; both scenarios measured)
- `docs/architecture.md`: added `## Cold-start measurements (Phase 1)` with concrete µs values, four CONTEXT.md benchmark item decisions, D-11 stream merge guarantee, D-12 signal forwarding documentation
- `docs/open-questions.md`: Q-deferred-merge-ordering and Q-deferred-signal-forwarding moved to Resolved section with canonical answers and cross-references

## Phase 1 Final Test Count

`cargo test --workspace`: **135 passed, 1 ignored** (21 test suites)

## Phase 1 Final Binary Size

`target/release/lacon`: 6,506,992 bytes (6.2 MiB) with `strip = "symbols"` + `opt-level = "z"` + `lto = "thin"` + `codegen-units = 1`.

## Resolution of Deferred Open Questions

### Q-deferred-merge-ordering → Resolved (D-11)

**Canonical answer:** Best-effort line atomicity, no cross-stream order guarantee. Each individual line from stderr or stdout is emitted whole; interleaving is wall-clock-arrival order at the os_pipe FIFO. Most filter rules are content-based and insensitive to cross-stream ordering. No pty or select/epoll needed.

**Implementation file:** `crates/lacon-core/src/runtime/mod.rs` (PLAN-05, single os_pipe write-end cloned for stdout+stderr, `read_until` on single reader thread).

### Q-deferred-signal-forwarding → Resolved (D-12)

**Canonical answer:** SIGTERM/SIGINT forwarded via `nix::sys::signal::kill(Pid::from_raw(child_pid), signal)`. No drain or flush of buffered output. Exits with `128 + sig`. Process-group kill not in v1. macOS verification deferred to Phase 6 acceptance gate.

**Implementation file:** `crates/lacon-core/src/runtime/mod.rs` (PLAN-05 Task 2, signal-hook pending() poll + AtomicBool stop_flag).

## Task Commits

1. **Task 1: bin/test_emitter binary + workspace integration tests** - `77026d6` (feat)
2. **Task 2: cold-start probe + benchmark findings + docs updates + open-questions resolved** - `4a27828` (feat)

## Files Created/Modified

- `bin/test_emitter/Cargo.toml` — workspace package config; publish = false
- `bin/test_emitter/src/main.rs` — deterministic emitter binary
- `crates/lacon-cli/tests/end_to_end.rs` — 5 e2e integration tests (Option A: co-located with cli tests)
- `benches/Cargo.toml` — lacon_benches package with cold_start_probe [[bin]]
- `benches/cold_start.rs` — cold-start measurement probe
- `Cargo.toml` — workspace members updated to `["crates/*", "bin/*", "benches"]`
- `crates/lacon-cli/Cargo.toml` — test_emitter dev-dep for cargo_bin lookup
- `docs/architecture.md` — Cold-start measurements (Phase 1) section added
- `docs/open-questions.md` — two deferred items resolved with full rationale

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] env!(CARGO_BIN_EXE_test_emitter) compile failure**
- **Found during:** Task 1 TDD RED phase
- **Issue:** `env!("CARGO_BIN_EXE_test_emitter")` fails to compile: "environment variable not defined at compile time". Cargo only sets `CARGO_BIN_EXE_*` for `[[bin]]` targets in the same package being tested, not for external workspace members. The plan's action block noted this as "env!(\"CARGO_BIN_EXE_test_emitter\") or path::PathBuf" — the PathBuf alternative is the correct approach.
- **Fix:** Changed `test_emitter_path()` to return `PathBuf` using `assert_cmd::cargo::cargo_bin("test_emitter")`, which uses a `legacy_cargo_bin` fallback resolving via `current_exe()` parent directory. All 5 tests pass without any env var. Maintains T-07-04 guarantee (no PATH lookup).
- **Files modified:** `crates/lacon-cli/tests/end_to_end.rs`
- **Committed in:** 77026d6

## Known Stubs

None. All test scenarios are fully wired with real binaries and real rule files.

## Threat Flags

No new security surface. All T-07-xx mitigations implemented as designed:
- T-07-01 (temp path disclosure): accepted; operator-only tool, no sensitive content
- T-07-02 (50×2 subprocesses): documented; not in CI by default
- T-07-03 (cold-start regression): baseline recorded; Phase 6 gate enforces 10ms ceiling
- T-07-04 (test_emitter spoofing): `assert_cmd::cargo::cargo_bin` resolves from cargo's target dir, not PATH

## Self-Check

PASSED
- `bin/test_emitter/Cargo.toml` — FOUND
- `bin/test_emitter/src/main.rs` — FOUND
- `crates/lacon-cli/tests/end_to_end.rs` — FOUND
- `benches/cold_start.rs` — FOUND
- `benches/Cargo.toml` — FOUND
- `docs/architecture.md` has "Cold-start measurements (Phase 1)" — CONFIRMED
- `docs/open-questions.md` has D-11/D-12 in Resolved section — CONFIRMED
- Commits 77026d6, 4a27828 — both in git log

---
*Phase: 01-engine-core-lacon-run-wrapper*
*Completed: 2026-05-06*
