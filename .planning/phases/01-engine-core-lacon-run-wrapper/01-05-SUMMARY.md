---
phase: 01-engine-core-lacon-run-wrapper
plan: "05"
subsystem: runtime
tags: [rust, subprocess, os_pipe, crossbeam-channel, nix, signal-hook, streaming, on_error]

requires:
  - phase: 01-engine-core-lacon-run-wrapper
    plan: "01"
    provides: workspace + Cargo.toml with os_pipe, crossbeam-channel, nix, signal-hook deps
  - phase: 01-engine-core-lacon-run-wrapper
    plan: "02"
    provides: Pipeline + Stage enum with run_with_post_process
  - phase: 01-engine-core-lacon-run-wrapper
    plan: "03"
    provides: ResolvedRule, RuleLoader, RuleSource, implicit MaxBytes injection
  - phase: 01-engine-core-lacon-run-wrapper
    plan: "04"
    provides: StarlarkScript, ScriptCtx, RuntimeError Starlark variants

provides:
  - Runner::new(resolved, options) + Runner::run(&argv, sink) -> Result<RunOutcome>
  - RunOutcome { exit_code, byte_counts, signaled, bypassed, truncated, duration_ms }
  - ByteCounts { raw_stdout_bytes, raw_stderr_bytes, filtered_bytes }
  - RunOptions { project_path, extra_env }
  - InvocationMeta struct for Phase 2 SQLite tracker (no Phase 2 refactor needed)
  - RuntimeError variants: SpawnFailed, IoError, SubprocessKilled, EmptyArgv
  - Signal forwarder thread (unix): SIGTERM/SIGINT forwarded to child PID via nix::kill
  - LACON_DISABLE=1 bypass: subprocess inherits stdout/stderr, bypassed=true
  - on_error pipeline swap: success buffer discarded on non-zero exit (ADR-0010)
  - 4 integration test suites: runtime_subprocess, runtime_on_error, runtime_bypass, runtime_signal

affects: [01-06, 02-sqlite-tracker, 03-adapter-claudecode]

tech-stack:
  added: []  # All deps declared by PLAN-01; this plan uses but does not add
  patterns:
    - "os_pipe merge: single write-end cloned for stdout+stderr, drop(cmd) before reading (Pitfall 1)"
    - "read_until not .lines(): non-UTF8 safety via from_utf8_lossy (Pitfall 2)"
    - "Dual-buffer model: raw_buffer held until exit code known; pipeline chosen by exit code (D-13)"
    - "AtomicBool stop_flag: signal watcher thread exits cleanly after child.wait()"
    - "NO runtime pre-cap: Stage::MaxBytes in pipeline is the sole total-output truncation point (D-08, W3 fix)"

key-files:
  created:
    - crates/lacon-core/tests/runtime_subprocess.rs
    - crates/lacon-core/tests/runtime_on_error.rs
    - crates/lacon-core/tests/runtime_bypass.rs
    - crates/lacon-core/tests/runtime_signal.rs
  modified:
    - crates/lacon-core/src/runtime/mod.rs
    - crates/lacon-core/src/error.rs

key-decisions:
  - "W3 fix applied: NO hardcoded raw_buffer pre-cap at the runtime level. Stage::MaxBytes (injected by PLAN-03 loader) is the sole total-output truncation enforcement point per D-08."
  - "Per-line MAX_LINE_BYTES = 1 MiB retained as T-05-04 DoS defense (separate from total-output cap)."
  - "Signal watcher uses pending() poll loop (50ms interval) + AtomicBool stop_flag rather than Signals::forever() to avoid hanging after child is reaped."
  - "LACON_DISABLE bypass test uses static Mutex to serialize env-mutating tests within the binary."
  - "No-op signal forwarder stub on non-unix targets keeps code compiling on all platforms."

patterns-established:
  - "drop(cmd) immediately after child.spawn() — enforced by acceptance criterion grep"
  - "read_until(b'\\n', &mut buf) + from_utf8_lossy — never BufRead::lines() on subprocess streams"
  - "install_signal_forwarder(child_pid) returns (JoinHandle, Arc<AtomicBool>) for clean shutdown"

requirements-completed:
  - REQ-engine-on-error
  - REQ-engine-bypass
  - REQ-cli-run

duration: 3min
completed: "2026-05-06"
---

# Phase 01 Plan 05: lacon run Runtime Summary

**Runner::run with os_pipe subprocess merge, dual-buffer on_error swap, SIGTERM/SIGINT forwarding, and LACON_DISABLE bypass — the production hot path for every filtered Claude Code Bash invocation**

## Performance

- **Duration:** 3 min
- **Started:** 2026-05-06T09:02:02Z
- **Completed:** 2026-05-06T09:04:57Z
- **Tasks:** 2 (both feat, TDD)
- **Files modified:** 6 (2 modified, 4 created)

## Accomplishments

- `Runner::run` spawns subprocess via `std::process::Command`, merges stdout+stderr through a single `os_pipe` writer (D-09)
- Dual-buffer model (D-13): raw lines accumulated until exit code known; success path or on_error path chosen after `child.wait()`
- `on_error` pipeline replaces success pipeline on non-zero exit (ADR-0010, REQ-engine-on-error); raw output passed through when no `on_error` block
- Signal forwarder thread (unix): polls SIGTERM/SIGINT via `signal-hook` and forwards to child PID via `nix::sys::signal::kill` (D-12)
- `LACON_DISABLE=1` bypass: subprocess inherits stdout/stderr unchanged (REQ-engine-bypass)
- W3 fix: no hardcoded raw_buffer pre-cap; `Stage::MaxBytes` is the sole total-output truncation point (D-08); per-line 1 MiB cap (T-05-04) retained

## W3 Fix Verification

The `max_bytes_overflow_emits_byte_exact_truncation_marker` integration test passes:
- Subprocess emits 200 lines × ~21 bytes each = ~4.2 KiB
- Rule has `Stage::MaxBytes { cap: 200 }` (200 bytes)
- Output contains `[lacon: truncated, N more bytes dropped]` — emitted by `Stage::MaxBytes` in the pipeline, NOT by any runtime-level pre-cap
- `RunOutcome.truncated` is true

## Cross-Platform Signal Forwarding Status

Tested on **Linux only** (Ubuntu 22.04, Rust 1.94.1). The smoke test `signal_forwarder_does_not_hang_on_normal_exit` verifies the forwarder thread lifecycle under non-signal conditions. The interactive `sigterm_forwarded_to_child` test is gated `#[ignore]` per VALIDATION.md. Cross-platform (macOS) sign-off is deferred to PLAN-07.

## Task Commits

1. **Task 1: Runner with os_pipe merge, dual-buffer, on_error swap, bypass** - `46a6053` (feat)
2. **Task 2: Signal forwarding SIGTERM/SIGINT + runtime_signal tests** - `05585af` (feat)

## Files Created/Modified

- `crates/lacon-core/src/runtime/mod.rs` — Full Runner implementation (456 lines)
- `crates/lacon-core/src/error.rs` — Extended RuntimeError with SpawnFailed, IoError, SubprocessKilled, EmptyArgv
- `crates/lacon-core/tests/runtime_subprocess.rs` — End-to-end subprocess tests (7 tests)
- `crates/lacon-core/tests/runtime_on_error.rs` — on_error swap tests (3 tests)
- `crates/lacon-core/tests/runtime_bypass.rs` — LACON_DISABLE bypass tests (2 tests)
- `crates/lacon-core/tests/runtime_signal.rs` — Signal forwarder tests (1 auto + 1 ignored)

## Decisions Made

- **W3 fix applied (deviation from earlier sketch):** No hardcoded raw_buffer pre-cap at the runtime level. The plan sketch (revision 0) showed a 64 KiB silent drop pattern; revision 1 removed it. Only `Stage::MaxBytes` (loader-injected by PLAN-03) enforces total-output truncation. The runtime's job is to collect raw lines faithfully and hand them to the pipeline.
- **LACON_DISABLE test isolation:** Used `static Mutex<()>` to serialize env-mutating tests within the bypass test binary. Prevents race between `set_var`/`remove_var` when tests run concurrently (Rust 1.94+ parallel test default).
- **Signal watcher exit pattern:** `pending()` poll loop (50ms) + `AtomicBool` stop_flag (set after `child.wait()`) rather than `Signals::forever()`. The `forever()` iterator would block forever if no signal arrives, preventing the watcher from exiting after the child is reaped.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] .lines() comment caused negative gate failure**
- **Found during:** Task 1 acceptance criteria check
- **Issue:** The comment "NOT BufRead::lines() which panics" contained the literal `BufRead::lines()` string, causing the negative grep gate to count 1 instead of 0
- **Fix:** Rephrased comment to avoid the literal pattern: "NOT the `lines()` iterator method which panics"
- **Files modified:** `crates/lacon-core/src/runtime/mod.rs`
- **Committed in:** 46a6053 (part of Task 1)

**2. [Rule 1 - Bug] Clippy: needless .as_bytes() on String.len()**
- **Found during:** Task 1 clippy run
- **Issue:** `joined.as_bytes().len()` should be `joined.len()` for String
- **Fix:** Changed to `joined.len()`
- **Files modified:** `crates/lacon-core/src/runtime/mod.rs`
- **Committed in:** 46a6053 (part of Task 1)

**3. [Rule 1 - Bug] LACON_DISABLE bypass test racing with env cleanup**
- **Found during:** Task 1 test run
- **Issue:** Two tests in `runtime_bypass.rs` both mutated the process env concurrently; `remove_var` in `lacon_disable_not_set_does_filter` could clear the env var set by `lacon_disable_bypasses_filtering` before `Runner::run` checked it
- **Fix:** Added `static Mutex<()> ENV_LOCK` to serialize the two tests; both tests acquire the lock before mutating the env
- **Files modified:** `crates/lacon-core/tests/runtime_bypass.rs`
- **Committed in:** 46a6053 (part of Task 1)

**4. [Rule 1 - Bug] unnecessary `unsafe` block in signal test**
- **Found during:** Task 2 clippy run
- **Issue:** `nix::sys::signal::kill` is not unsafe in nix 0.31; the `unsafe {}` block around it was spurious
- **Fix:** Removed the `unsafe {}` wrapper from the `sigterm_forwarded_to_child` test
- **Files modified:** `crates/lacon-core/tests/runtime_signal.rs`
- **Committed in:** 05585af (part of Task 2)

---

**Total deviations:** 4 auto-fixed (all Rule 1 — bugs)
**Impact on plan:** All auto-fixes necessary for correctness and test reliability. No scope creep.

## Issues Encountered

None beyond what was documented as deviations above.

## Known Stubs

None. Runner is fully implemented. The `InvocationMeta` struct is defined and populated by callers (PLAN-06 does this); no stub data flows to any consumer.

## Threat Flags

No new security-relevant surface beyond what is documented in the plan's threat model. The implementation faithfully implements all T-05-xx mitigations:
- T-05-01 (argv injection): `Command::new(&argv[0]).args(&argv[1..])` — no shell concat
- T-05-02 (pipe deadlock): `drop(cmd)` before reading enforced by acceptance criterion
- T-05-03 (non-UTF8 panic): `read_until` + `from_utf8_lossy` enforced by negative gate
- T-05-04 (single-line DoS): `MAX_LINE_BYTES = 1 MiB` with suffix `[lacon: line truncated]`
- T-05-05 (unbounded output): `Stage::MaxBytes` (W3 fix — no runtime pre-cap)

## Next Phase Readiness

- PLAN-06 can now wire `lacon run` CLI subcommand: call `RuleLoader::resolve(rule_id)`, construct `Runner::new(resolved, options)`, call `runner.run(&argv, &mut stdout())`, and exit with `outcome.exit_code`
- Phase 2 (tracker) can hang `InvocationMeta` → SQLite write alongside `runner.run()` without refactoring Phase 1 code

## Self-Check

PASSED

---
*Phase: 01-engine-core-lacon-run-wrapper*
*Completed: 2026-05-06*
