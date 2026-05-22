---
phase: 07-close-gap-capture-raw-output-on-opt-in-so-lacon-explain-work
plan: 01
subsystem: tracking
tags: [rust, sqlite, raw-outputs, lacon-explain, capture, opt-in, byte-exact]

# Dependency graph
requires:
  - phase: 02-local-tracking
    provides: "RawOutput carrier, Tracker::record(meta, raw_opt, ...) write API + insert_raw_output, privacy double-gate, raw_outputs schema"
  - phase: 04-cli-completion-stats-explain-doctor
    provides: "Runner::filter_bytes byte-replay, query::fetch_raw_output, explain.rs side-by-side render"
provides:
  - "RunOptions.capture_raw flag (default false) gating raw-byte capture in the core runner"
  - "RunOutcome.raw_captured: Option<Vec<u8>> carrying captured pre-filter bytes (canonical form raw_buffer.join(\"\\n\"), no trailing newline)"
  - "run.rs wiring: capture flag set from resolved store_raw_outputs; RawOutput constructed; Some(&raw) passed to tracker.record (run.rs:275 None gap closed)"
  - "True E2E test: real lacon run -> lacon explain byte-for-byte reproduction"
affects: [lacon-explain, raw-outputs-retention, future-redaction, lacon-purge]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Gated serialization on the cold-start hot path: cost paid only inside the capture-on arm (ADR-0013 / ADR-0005 preserved)"
    - "Shared config resolution helpers (load_cfg/config_paths/user_config_dir) so the capture flag and the persist gate read identical store_raw_outputs"
    - "Capture form is the exact inverse of the per-line reader build, so filter_bytes' re-split round-trips byte-identically"

key-files:
  created:
    - .planning/phases/07-close-gap-capture-raw-output-on-opt-in-so-lacon-explain-work/07-01-SUMMARY.md
  modified:
    - crates/lacon-core/src/runtime/mod.rs
    - crates/lacon-cli/src/commands/run.rs
    - crates/lacon-cli/src/commands/explain.rs
    - crates/lacon-cli/tests/tracking_e2e.rs

key-decisions:
  - "RunOutcome field name: raw_captured: Option<Vec<u8>> (CONTEXT D-01 discretion)"
  - "RunOptions flag name: capture_raw: bool (CONTEXT D-02 discretion)"
  - "Capture form: raw_buffer.join(\"\\n\").into_bytes() with NO trailing newline re-added (D-05, load-bearing)"
  - "explain.rs RunOptions uses ..Default::default() so replay never re-captures"
  - "Config resolution refactored into shared helpers so run_with_rule (flag) and record_invocation (gate) never diverge"

patterns-established:
  - "Hot-path-safe optional capture: compute join-to-bytes strictly in the flag==true arm; default path moves raw_buffer into the pipeline exactly as before (zero extra cost)"
  - "Double-gate-trusting Some: pass Some(&raw) unconditionally on the capture path; Tracker::record's (cfg, Some) re-gate is the sole persist authority"

requirements-completed: [REQ-acceptance-explain-reproducibility, REQ-cli-explain, REQ-tracking-raw-outputs-default-off]

# Metrics
duration: 12min
completed: 2026-05-22
---

# Phase 7 Plan 01: Capture raw output on opt-in so `lacon explain` works end-to-end Summary

**Closes the v1.0 audit gap: `lacon run` with `store_raw_outputs:true` now persists `raw_buffer.join("\n")` to `raw_outputs`, so `lacon explain` reproduces a REAL invocation byte-for-byte instead of only hand-seeded SQL rows.**

## Performance

- **Duration:** ~12 min
- **Started:** 2026-05-22T21:12:00Z
- **Completed:** 2026-05-22T21:21:00Z
- **Tasks:** 3
- **Files modified:** 4

## Accomplishments
- Added a default-false `RunOptions.capture_raw` flag and a `RunOutcome.raw_captured: Option<Vec<u8>>` field, set at all three RunOutcome construction sites (Runner::run = computed, run_bypassed = None, run_unmatched = None) — the compiler enforces all sites since RunOutcome has no Default derive.
- `Runner::run` computes `Some(raw_buffer.join("\n").into_bytes())` strictly in the `capture_raw == true` arm, immediately before `raw_buffer` is moved into the exit-code branch — the default-off hot path is byte-for-byte unchanged with zero extra serialization cost (ADR-0013 cold start + ADR-0005 memory bound preserved).
- Wired `run.rs` to set the capture flag from the project's resolved `store_raw_outputs`, construct `RawOutput { stdout: captured, stderr: Vec::new() }` (D-04 merged-stream/empty-stderr), and pass `Some(&raw)` to `tracker.record` — closing the hard-coded `None` at `run.rs:275`.
- Refactored config resolution into shared helpers (`user_config_dir`, `config_paths`, `load_cfg`, `resolve_store_raw_outputs`) so `run_with_rule` (which sets the flag before the run) and `record_invocation` (which gates the persist after the run) read the SAME `store_raw_outputs` value and can never diverge.
- Added a true E2E test driving real `lacon run` + `lacon explain` over a shared XDG DB, asserting the explain filtered column equals the run's filtered stdout byte-for-byte (REQ-acceptance-explain-reproducibility) plus a one-raw_outputs-row + non-NULL raw_output_id check that proves capture actually fired.

## Task Commits

Each task was committed atomically:

1. **Task 1: Gated raw capture on RunOutcome/RunOptions + D-10 unit test** - `6fc683b` (feat)
2. **Task 2: Wire capture into run.rs — flag, RawOutput, Some(&raw)** - `284f06f` (feat)
3. **Task 3: True E2E lacon run -> lacon explain byte-exact test** - `5980e68` (test)

**Plan metadata:** committed separately with SUMMARY/STATE/ROADMAP/REQUIREMENTS.

## Files Created/Modified
- `crates/lacon-core/src/runtime/mod.rs` - Added `capture_raw` to RunOptions and `raw_captured` to RunOutcome; gated `raw_buffer.join("\n")` capture in Runner::run before the exit-code branch; set the field at run_bypassed (None); added inline `#[cfg(test)] mod tests` with the D-10 Some/None shape assertions (driven against `printf`).
- `crates/lacon-cli/src/commands/run.rs` - Set `RunOptions.capture_raw` from `resolve_store_raw_outputs(project_path)`; added shared config helpers; moved `outcome.raw_captured` out and constructed `RawOutput { stdout, stderr: Vec::new() }`; passed `Some(&raw)` (via `raw_output.as_ref()`) to `tracker.record`, replacing the `None` at the old `:275`. `project_store_raw`/`user_store_raw` layer split and privacy marker path unchanged.
- `crates/lacon-cli/src/commands/explain.rs` - RunOptions literal switched to `..Default::default()` so replaying stored bytes keeps `capture_raw` false (compile-fix once RunOptions gained the field; replay never re-captures).
- `crates/lacon-cli/tests/tracking_e2e.rs` - Added `explain_reproduces_real_run_byte_for_byte` (real run + real explain, byte-exact `assert_eq!`, raw_outputs row + non-NULL raw_output_id checks), plus `write_drop_line_rule` and `trim_one_trailing_blank` helpers. Off-path guard `raw_outputs_empty_by_default` left unchanged.

## Decisions Made
- **RunOutcome field name:** `raw_captured: Option<Vec<u8>>` (CONTEXT D-01 granted discretion; one name used consistently across declaration + 3 construction sites + the gated compute).
- **RunOptions flag name:** `capture_raw: bool` (CONTEXT D-02 discretion).
- **Capture form (load-bearing, D-05):** `raw_buffer.join("\n").into_bytes()` with NO trailing newline re-added — the exact inverse of the per-line reader build (lossy decode + strip one trailing `\n`), so `Runner::filter_bytes`' split-on-`\n` re-split regenerates the identical `Vec<String>` the live pipeline consumed. Confirmed via the Task 3 byte-for-byte E2E assertion.
- **Wrapper type:** chose `Option<Vec<u8>>` (not `Option<RawOutput>`) on RunOutcome — the runner stays storage-unaware; run.rs builds the `RawOutput` carrier (D-06, keeps config/storage awareness out of the core runner).
- **Some passed via `raw_output.as_ref()`:** semantically `Some(&raw)`; relies on the existing double-gate in `Tracker::record` (the raw_outputs INSERT fires only on `(cfg_store_raw_outputs, Some)`), so passing `Some` is always safe.

## Deviations from Plan

None - plan executed exactly as written.

The plan's Task 1 explicitly anticipated that adding the RunOutcome field would require touching the run_unmatched site (done) and that the workspace must build at the Task 1 boundary; the two RunOptions struct literals in `run.rs:70` and `explain.rs:126` needed the new field to compile. These were handled within Task 1's stated scope (run.rs run_unmatched None initializer; explain.rs `..Default::default()`), not as deviations.

## Issues Encountered
- **`cargo fmt` reformatted unrelated files.** The repo had pre-existing whitespace drift across many files; a blanket `cargo fmt` / `cargo fmt -p <crate>` restaged churn in files outside this plan's scope. Resolved by reverting the unrelated reformatting (`git checkout -- <files>`) and formatting only the four plan files via `rustfmt --edition 2021 <file>` before each commit. Each task commit contains only its own files; no out-of-scope formatting churn was committed.

## Verification

- `cargo build --workspace` exits 0.
- `cargo test --workspace` exits 0 — full default suite green (no regressions). The new E2E test `explain_reproduces_real_run_byte_for_byte` passes.
- `raw_outputs_empty_by_default` (D-09 off-path negative guard) still passes — default-off path writes zero raw_outputs rows + NULL raw_output_id, byte-for-byte unchanged (D-03).
- The masked seeded test `explain_filtered_column_byte_equals_run_output` (D-10) still passes unchanged.
- `cargo clippy -p lacon-core --all-targets` = 14 warnings (baseline 14, no new); `cargo clippy -p lacon-cli --all-targets` = 8 warnings (baseline 8, no new).
- `rustfmt --edition 2021 --check` passes on all four modified files.
- Decision coverage: D-01..D-05 (Task 1), D-06/D-07 (Task 2), D-08/D-09 (Task 3), D-10 (Task 1) — all ten locked decisions implemented; none of the deferred ideas (redaction, lacon purge, encryption-at-rest, separate stderr capture, new schema, 7th command) introduced.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness
- `lacon explain` is now functional on REAL captured invocations end-to-end; the single root-cause gap from the v1.0 milestone audit is closed.
- The opt-in privacy double-gate, 0700 dir perms, and 3-day raw_outputs retention remain in force and unmodified.
- Deferred-to-backlog items that presuppose working capture (redaction, `lacon purge`, encryption-at-rest, separate real stderr capture) are now unblocked but remain out of v1 scope.

## Self-Check: PASSED

- All four modified files exist on disk.
- All three task commits present in git history (`6fc683b`, `284f06f`, `5980e68`).
- `cargo build --workspace && cargo test --workspace` both exit 0.

---
*Phase: 07-close-gap-capture-raw-output-on-opt-in-so-lacon-explain-work*
*Completed: 2026-05-22*
