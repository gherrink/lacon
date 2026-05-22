---
phase: 07-close-gap-capture-raw-output-on-opt-in-so-lacon-explain-work
verified: 2026-05-22T21:40:00Z
status: passed
score: 5/5 must-haves verified
overrides_applied: 0
re_verification: false
---

# Phase 7: Close Gap — Capture Raw Output on Opt-in So `lacon explain` Works End-to-End — Verification Report

**Phase Goal:** Capture the pre-filter (raw) bytes of a `lacon run` invocation when `store_raw_outputs` is enabled and persist them to the existing `raw_outputs` table, so `lacon explain <id>` reproduces a real invocation end-to-end (byte-for-byte) instead of only hand-seeded SQL rows. Closes the single root-cause gap from the v1.0 milestone audit — the capture path missing at run.rs:275 (raw=None).
**Verified:** 2026-05-22T21:40:00Z
**Status:** PASSED
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Running `lacon run` with `store_raw_outputs: true` persists merged raw bytes to the raw_outputs table (gap at run.rs:275 closed) [D-06] | VERIFIED | `run.rs:351` constructs `RawOutput { stdout: raw_captured, stderr: Vec::new() }` and passes `raw_output.as_ref()` to `tracker.record()` — the hard-coded `None` is gone. `explain_reproduces_real_run_byte_for_byte` drives a real run and asserts `raw_rows == 1`. Test passes. |
| 2 | Running `lacon run` with default config writes ZERO raw_outputs rows; raw_output_id stays NULL [D-03, D-09] | VERIFIED | `raw_outputs_empty_by_default` test (tracking_e2e.rs:127) asserts `COUNT(*) FROM raw_outputs == 0` and `raw_output_id IS NULL`. Passes in full suite (461 tests, 0 failed). |
| 3 | `lacon explain <id>` on an invocation captured by a REAL `lacon run` re-derives a filtered column byte-for-byte equal to the stdout `lacon run` originally emitted [D-05, D-08, REQ-acceptance-explain-reproducibility] | VERIFIED | `explain_reproduces_real_run_byte_for_byte` (tracking_e2e.rs:350) drives real `lacon run`, checks `raw_output_id` is non-NULL, then drives `lacon explain <id>` and `assert_eq!(rendered, expected, "D-08 / REQ-acceptance-explain-reproducibility...")`. Test passes. The empty-output edge case is covered separately by `explain_empty_output_replays_as_zero_rows`. |
| 4 | RunOptions capture flag defaults to false via `#[derive(Default)]`; every existing RunOptions construction site keeps compiling with capture OFF [D-02] | VERIFIED | `pub capture_raw: bool` declared at runtime/mod.rs:61 inside the `#[derive(Debug, Clone, Default)]` struct. `explain.rs:153` uses `..Default::default()` so replay never re-captures. Full workspace builds clean. |
| 5 | Capture serialization (`raw_buffer.join("\n")`) executes ONLY when the capture flag is true; when false, raw_buffer is moved into the pipeline exactly as today [D-03, D-05] | VERIFIED | runtime/mod.rs:370-374: `let raw_captured: Option<Vec<u8>> = if self.options.capture_raw { Some(raw_buffer.join("\n").into_bytes()) } else { None };` — join-to-bytes cost is ONLY in the `true` arm, executed before the raw_buffer move at the exit-code branch (:383-400). |

**Score:** 5/5 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/lacon-core/src/runtime/mod.rs` | RunOptions capture flag (default false), RunOutcome raw-bytes field set at all 3 construction sites, gated capture, D-10 unit tests | VERIFIED | `capture_raw: bool` at :61; `raw_captured: Option<Vec<u8>>` at :100; gated at :370-374; `run_bypassed` sets `raw_captured: None` at :565; `run_unmatched` in run.rs sets `raw_captured: None` at :145; `#[cfg(test)] mod tests` at :654 contains `capture_raw_true_yields_some`, `capture_raw_false_yields_none`, and `raw_buffer_join_split_round_trips`. 14 occurrences of `raw_captured` (>= 4 required). |
| `crates/lacon-cli/src/commands/run.rs` | Sets capture flag from resolved store_raw_outputs; constructs RawOutput{stdout, stderr: Vec::new()}; passes Some(&raw) to tracker.record; replaces None at :275 | VERIFIED | `capture_raw = resolved_cfg.cfg.store_raw_outputs` at :81; `RawOutput { stdout, stderr: Vec::new() }` at :351-354; `raw_output.as_ref()` at :358; old `None` at :275 replaced — grep for `None.*v1 default.*raw output` returns nothing. |
| `crates/lacon-cli/tests/tracking_e2e.rs` | True E2E lacon run -> lacon explain byte-exact test; `raw_outputs_empty_by_default` still present and green | VERIFIED | `explain_reproduces_real_run_byte_for_byte` at :350 (real run, real explain, `assert_eq!` on byte equality, raw_outputs row + non-NULL raw_output_id checks); `explain_empty_output_replays_as_zero_rows` at :469 (WR-01 empty-output edge case, additional fix from code review); `raw_outputs_empty_by_default` at :127 (unchanged). All pass. No `#[ignore]` on any of these tests. |
| `crates/lacon-cli/src/commands/explain.rs` | RunOptions uses `..Default::default()` so replay never re-captures | VERIFIED | Line :153: `..Default::default()`. Also implements `split_lines` with the WR-01 empty-bytes guard (returns `Vec::new()` for empty input), and adds `debug_assert!(stderr.is_empty(), ...)` at :122 to enforce the D-04 invariant (addresses WR-03). |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| runtime/mod.rs (Runner::run, before raw_buffer move at :383) | RunOutcome.raw_captured | `raw_buffer.join("\n").into_bytes()` when `capture_raw == true`, else `None` | WIRED | Lines 370-374; capture computed BEFORE `raw_buffer.into_iter()` at :385. |
| run.rs (record_invocation, :351-358) | `tracker.record(&meta, raw_output.as_ref(), ...)` | `RawOutput { stdout: raw_captured, stderr: Vec::new() }` constructed from `outcome.raw_captured.take()` | WIRED | Lines 275 (take), 351-354 (RawOutput build), 356-363 (tracker.record call). |
| tracking_e2e.rs (explain_reproduces_real_run_byte_for_byte, :443) | `lacon explain` filtered column == `lacon run` filtered stdout | `assert_eq!(rendered, expected, "D-08 / REQ-acceptance-explain-reproducibility...")` over same XDG-tempdir DB | WIRED | Test at :350-450; asserts 1 raw_outputs row, non-NULL raw_output_id, byte-exact column equality. |

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|--------------------|--------|
| `tracking_e2e.rs:explain_reproduces_real_run_byte_for_byte` | `raw_rows`, `raw_output_id`, `run_stdout` | Real `lacon run` invocation with `store_raw_outputs: true`; `Connection::open` DB query | Yes — asserts `raw_rows == 1`, `raw_output_id.is_some()`, and byte equality of explain output | FLOWING |
| `run.rs:record_invocation` | `raw_captured` | `outcome.raw_captured.take()` from `Runner::run` output where `capture_raw == true` | Yes — sourced from `raw_buffer.join("\n").into_bytes()` in Runner::run | FLOWING |

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Full test suite (461 tests) | `cargo test --workspace` | 461 passed, 0 failed | PASS |
| tracking_e2e all 10 tests | `cargo test -p lacon-cli --test tracking_e2e` | 10 passed, 0 failed | PASS |
| D-10 inline unit tests (capture shape) | `cargo test -p lacon-core --lib runtime` | 4 passed: `capture_raw_true_yields_some`, `capture_raw_false_yields_none`, `raw_buffer_join_split_round_trips`, Starlark test | PASS |
| Masked seeded test (legacy proof, unchanged) | `cargo test -p lacon-cli --test cli_explain explain_filtered_column_byte_equals_run_output` | 1 passed | PASS |

### Probe Execution

No phase-specific probe scripts declared in PLAN or present under `scripts/*/tests/`. Step 7c: SKIPPED (no probe scripts for this phase; behavioral verification covered by the E2E test suite above).

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| REQ-acceptance-explain-reproducibility | 07-01-PLAN.md | `lacon explain` correctly reproduces the filtering decision for any tracked invocation that has stored raw output | SATISFIED | `explain_reproduces_real_run_byte_for_byte` asserts byte-exact reproduction via a REAL `lacon run` — not a hand-seeded row. Test passes. |
| REQ-cli-explain | 07-01-PLAN.md | `lacon explain <id>` re-runs filtering against stored raw output; requires raw retention enabled at invocation time | SATISFIED | The capture path now writes real data; `explain.rs` read path unchanged and functional. `explain_reproduces_real_run_byte_for_byte` proves the full explain round-trip on real data. |
| REQ-tracking-raw-outputs-default-off | 07-01-PLAN.md | `raw_outputs` storage is OFF by default; opt-in per project via `store_raw_outputs: true` | SATISFIED | `raw_outputs_empty_by_default` proves default-off writes zero rows + NULL FK. `explain_reproduces_real_run_byte_for_byte` and `sc2_privacy_warning_via_cli` prove opt-in path. |

Note: These three requirements were previously marked Complete in REQUIREMENTS.md (mapped to Phases 2, 4, and 6). Phase 7 re-validates the functional half that was non-functional (the capture path). All three are now observably satisfied end-to-end.

### Anti-Patterns Found

No TBD, FIXME, or XXX markers found in any of the four modified files. No TODO or PLACEHOLDER markers found. No stub return patterns (empty arrays, `return null`, unimplemented handlers). No hardcoded empty data on the capture path.

The code review (07-REVIEW.md) raised 3 warnings (WR-01 empty-output round-trip, WR-02 redundant config resolution, WR-03 stderr invariant drift) and 3 info items. All three warnings were addressed in the implementation:

- WR-01: `split_merged_bytes` function (runtime/mod.rs:584-593) and `explain::split_lines` (:215-224) both guard `if merged_bytes.is_empty() { Vec::new() }`. Locked by `explain_empty_output_replays_as_zero_rows` test.
- WR-02: Config resolved ONCE via `resolve_config()` helper (run.rs:219-233); `ResolvedConfig` struct threaded through both `run_with_rule` (for capture flag) and `record_invocation` (for persist gate). The 3x redundant resolution is gone.
- WR-03: `debug_assert!(stderr.is_empty(), ...)` added at explain.rs:122-127 with cross-reference comment to run.rs.

Info items (IN-01 `..Default::default()` consistency, IN-02 dead binding, IN-03 round-trip unit test) also addressed: IN-01 via `..Default::default()` in run.rs:87; IN-02 removed by WR-02 fix; IN-03 via `raw_buffer_join_split_round_trips` test.

### Human Verification Required

None. All must-haves are mechanically verifiable and verified. The phase goal is a backend wiring change (capture path + persistence + E2E test) with no interactive UI, no visual output to assess, and no external services. The E2E test suite constitutes the user-facing acceptance bar (byte-exact reproduction proven programmatically).

### Gaps Summary

No gaps. All five must-have truths verified against actual codebase evidence. All required artifacts exist, are substantive, are wired, and have confirmed data flow. The test suite (461 tests, 0 failed) including the new E2E tests confirms the implementation is correct.

---

_Verified: 2026-05-22T21:40:00Z_
_Verifier: Claude (gsd-verifier)_
