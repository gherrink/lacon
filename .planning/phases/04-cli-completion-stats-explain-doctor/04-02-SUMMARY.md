---
phase: 04-cli-completion-stats-explain-doctor
plan: 02
subsystem: lacon-core runtime
tags: [explain, byte-replay, on_error, reproducibility, ADR-0010, D-04]
requires:
  - "Runner / ResolvedRule / Pipeline::run_with_post_process (Phase 1 runtime + pipeline)"
  - "ScriptCtx (Phase 1 starlark_host)"
provides:
  - "Runner::filter_bytes — subprocess-free byte-replay entry point (consumed by Wave 2 explain)"
affects:
  - "Wave 2 explain command (04-03): re-derives filtered output from stored raw_outputs"
  - "Phase 6 SC3 reproducibility: depends on filter_bytes branch matching the live runner"
tech-stack:
  added: []
  patterns:
    - "Mirror the live runner's exit-code branch (runtime/mod.rs:342-359) in a colocated method so it cannot drift (D-04)"
    - "Lossy-UTF8 byte->line split (split on b'\\n' + String::from_utf8_lossy), mirroring the live reader at runtime/mod.rs:265-270"
    - "ScriptCtx reconstructed from STORED command_raw (whitespace split), not a live process"
key-files:
  created:
    - "crates/lacon-core/tests/runtime_filter_bytes.rs — 4 branch-fidelity tests"
  modified:
    - "crates/lacon-core/src/runtime/mod.rs — added Runner::filter_bytes to impl Runner"
decisions:
  - "filter_bytes lives in lacon-core (D-04), NOT duplicated into lacon-cli, so the exit-code branch cannot drift from Runner::run"
  - "No REFACTOR commit: GREEN implementation is already minimal and clippy-clean"
metrics:
  duration: 7min
  completed: 2026-05-22
---

# Phase 4 Plan 02: Runner::filter_bytes Byte-Replay Summary

Subprocess-free byte-replay (`Runner::filter_bytes`, D-04) that re-derives filtered output from STORED stdout/stderr bytes without ever spawning the original command — the entry point Wave 2's `explain` consumes to reproduce what the live runner emitted.

## What Was Built

`Runner::filter_bytes` added to `impl Runner` in `crates/lacon-core/src/runtime/mod.rs`. It mirrors the live runner's exit-code branch (runtime/mod.rs:342-359, ADR-0010) exactly, sourced from STORED values instead of a subprocess:

- `exit_code == 0` → `success_pipeline.run_with_post_process(...)` (+ `post_process`)
- `exit_code != 0` AND `on_error_pipeline` present → `on_error_pipeline.run_with_post_process(...)` (+ `on_error_post_process`)
- `exit_code != 0` AND no `on_error_pipeline` → raw lines returned unchanged (ADR-0010 passthrough)

It never calls `Runner::run` and never spawns (verified: 0 `.run(`/`.spawn(`/`Command::new` calls in the method body).

### Exact signature (Wave 2 `explain` consumes this)

```rust
pub fn filter_bytes(
    &mut self,
    merged_bytes: &[u8],
    exit_code: i32,
    duration_ms: u64,
    command_raw: &str,
    project_path: Option<String>,
) -> Result<Vec<String>, RuntimeError>
```

`&mut self` because the on_error arm borrows `&mut on_err` (same constraint as `Runner::run`). Returns the filtered lines as `Vec<String>` — rendering/escaping is the explain command's concern (T-04-05 accepted; core only returns Strings). `command`/`args` for `ScriptCtx` are reconstructed from `command_raw` via whitespace split (empty `command_raw` → empty command + empty args); `merged_bytes` is split on `b'\n'` and lossily decoded (mirrors the live reader at runtime/mod.rs:265-270; v1 is a single merged stdout+stderr stream).

## Tasks Completed

| Task | Name | Commit | Files |
| ---- | ---- | ------ | ----- |
| RED | failing branch-fidelity tests | c109b17 | crates/lacon-core/tests/runtime_filter_bytes.rs |
| 1 (GREEN) | Runner::filter_bytes byte-replay | 4c4fe75 | crates/lacon-core/src/runtime/mod.rs |
| 2 | branch-fidelity unit tests (RED tests now pass) | c109b17 | crates/lacon-core/tests/runtime_filter_bytes.rs |

Task 2's deliverable (the three branch-case tests) was authored as the TDD RED gate (c109b17) and required no modification once `filter_bytes` landed — all three cases plus a fidelity assertion pass against the GREEN implementation.

## Three Branch Cases Confirmed (all pass)

1. **success** — `filter_bytes_success_path_runs_success_pipeline`: `exit_code = 0` runs through `success_pipeline`; dropped line absent, kept lines present.
2. **on_error** — `filter_bytes_on_error_path_runs_on_error_pipeline`: `exit_code = 1` with an `on_error_pipeline` applies the on_error transform (NOT the success transform, which would have dropped everything).
3. **no-on_error passthrough** — `filter_bytes_no_on_error_passes_raw_unchanged`: `exit_code = 2` with no `on_error_pipeline` returns the raw input lines byte-identical (ADR-0010).
4. **fidelity** (optional) — `filter_bytes_success_matches_direct_pipeline_application`: `filter_bytes` output equals applying the success pipeline directly to the same lines.

`cargo test -p lacon-core --test runtime_filter_bytes` → 4 passed, 0 failed.
`cargo test -p lacon-core` (full suite) → all binaries `ok`, 0 failed (no regression).

## Threat Model

- **T-04-04 (Tampering — branch drift) mitigated:** `filter_bytes` mirrors runtime/mod.rs:342-359 exactly; the three branch-fidelity tests lock all ADR-0010 cases, so a future edit desyncing one branch from the other fails CI. The doc comment names runtime/mod.rs:342-359 as the source of truth and notes Phase 6 SC3 depends on it.
- **T-04-05 (Information disclosure) accepted:** `filter_bytes` only transforms bytes already stored under the user's opt-in `store_raw_outputs`; it returns `Vec<String>` only — rendering/escaping is the Wave-2 explain command's concern.

## Deviations from Plan

None — plan executed exactly as written. No Rule 1-4 deviations.

## Deferred Issues

`cargo clippy -p lacon-core -- -D warnings` reports 4 pre-existing lints in Phase 1/2 files (`pipeline/stages.rs:438`, `:451`; `tracking/record.rs:8`; `tracking/mod.rs:201`) — none in this plan's files. They were already logged in Plan 04-01's deferred-items.md; Plan 04-02 re-confirmed them and noted that `runtime/mod.rs` (this plan's only source change) is clippy-clean. Out of scope per the SCOPE BOUNDARY rule (fixing other phases' code from this read-layer plan would be scope creep). See `.planning/phases/04-cli-completion-stats-explain-doctor/deferred-items.md`.

## Self-Check: PASSED

- FOUND: crates/lacon-core/src/runtime/mod.rs (Runner::filter_bytes at line 423)
- FOUND: crates/lacon-core/tests/runtime_filter_bytes.rs (4 tests)
- FOUND commit c109b17 (test/RED)
- FOUND commit 4c4fe75 (feat/GREEN)
