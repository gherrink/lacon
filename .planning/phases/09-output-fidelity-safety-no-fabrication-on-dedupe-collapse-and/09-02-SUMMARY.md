---
phase: 09-output-fidelity-safety-no-fabrication-on-dedupe-collapse-and
plan: 02
subsystem: engine
tags: [streaming-primitives, collapse_repeated, elision-marker, fabrication-safety, lacon-core]

# Dependency graph
requires:
  - phase: 01-engine-core-lacon-run-wrapper
    provides: "CollapseRepeated/Dedupe streaming primitives + MaxBytes [lacon: …] marker convention in stages.rs"
provides:
  - "collapse_repeated emits the standardized [lacon: collapsed N lines] elision marker at both the in-run and flush emission sites"
  - "Verbatim-survivor guarantee: every non-marker line collapse_repeated emits is byte-identical to an input line (D-09)"
  - "dedupe verified verbatim-only (regression guard) — unchanged"
affects: [09-03, git-status rule re-audit, filter-rule-schema spec update, bundled-rule fixtures]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Standardized [lacon: …]-namespaced elision marker used consistently across all non-verbatim emission points (collapse_repeated joins MaxBytes + per-line truncation)"

key-files:
  created: []
  modified:
    - crates/lacon-core/src/pipeline/stages.rs
    - tests/fixtures/primitives/collapse_repeated/expected.txt
    - tests/fixtures/git-status/many-untracked/expected.txt

key-decisions:
  - "D-07 discretion resolved: chose option (a) — drop free-form summary_template emission entirely in favor of the fixed [lacon: collapsed N lines] marker. summary_template struct field retained for YAML deserialization but no longer read at emission."
  - "In-run guard tightened from (kept_so_far > 0 || dropped > 0) to (dropped > 0) so the marker only appears when lines were actually suppressed — matching the CR-03 flush guard and eliminating any spurious '0 lines' marker risk mid-stream."

patterns-established:
  - "Elision-marker convention: a dropped line leaves a fixed [lacon: …] marker that cannot inherit the elided lines' formatting (no tab-indent blend), never a substituted/plausible-but-false line."

requirements-completed: [REQ-engine-streaming-primitives]

# Metrics
duration: 8min
completed: 2026-05-31
---

# Phase 9 Plan 02: Fidelity-safe collapse_repeated elision marker Summary

**`collapse_repeated` now emits a fixed `[lacon: collapsed N lines]` marker at both the in-run and flush sites instead of the free-form `summary_template` that blended into real tool output — survivors proven verbatim, `dedupe` confirmed unchanged.**

## Performance

- **Duration:** ~8 min
- **Started:** 2026-05-31T06:12:00Z
- **Completed:** 2026-05-31T06:20:00Z
- **Tasks:** 1
- **Files modified:** 3

## Accomplishments
- Replaced the free-form `summary_template.replace("{count}", …)` emission at BOTH `CollapseRepeated` call sites (in-run summary path + flush path) with the standardized `[lacon: collapsed {N} lines]` marker, modeled on the existing `MaxBytes` `[lacon: truncated, N more bytes dropped]` marker in the same file.
- Preserved the CR-03 `*dropped > 0` guard on the flush path (and aligned the in-run path to the same guard) — no marker emitted when nothing was dropped.
- Added a new unit test (`collapse_repeated_survivors_are_verbatim_input_lines`) feeding tab-indented (tabular) repeated-prefix input and asserting every non-marker output line is byte-identical to an input line (D-09 / T-09-05), and that the elision is the namespaced marker — not a tab-indented blend.
- Verified `dedupe` is untouched and remains verbatim-only (3 unit + 1 fixture test green).
- Retained the `summary_template` struct field so YAML rule deserialization (loader.rs) is unaffected.

## Task Commits

Each task was committed atomically:

1. **Task 1: Standardize the collapse_repeated elision marker at both emission sites** - `62f5177` (fix)

_TDD task: tests were written/updated to RED (3 failing assertions on the new marker form) before the two-site source change brought them to GREEN. Both the test and implementation edits landed in the single `62f5177` commit; the gate-sequence note is below._

## TDD Gate Compliance

This plan's task carried `tdd="true"`. The RED → GREEN cycle was followed (tests updated to expect `[lacon: collapsed N lines]` and confirmed failing against the old `summary_template` emission before the source change made them pass — RED output captured during execution: 3 failed / 2 passed, then 5 passed / 0 failed after the fix). However, the test edits and the implementation edit were committed together in a single `fix` commit (`62f5177`) rather than as separate `test` (RED) and `feat` (GREEN) commits. There is therefore no standalone `test(...)` commit preceding a `feat(...)` commit in the git log for this plan. Behavior was correct (RED verified before GREEN); only the commit granularity differs from the strict two-commit gate sequence.

## Files Created/Modified
- `crates/lacon-core/src/pipeline/stages.rs` — Standardized `CollapseRepeated` elision marker at both `step()` (in-run) and `flush()` paths; bound `summary_template: _` at the step site (field no longer read); updated two existing `collapse_repeated_*` unit tests to the new marker; added the verbatim-survivor unit test.
- `tests/fixtures/primitives/collapse_repeated/expected.txt` — Synced expected line 2 from `… 199 progress lines` to `[lacon: collapsed 199 lines]` (drives `primitives.rs::collapse_repeated_fixture`).
- `tests/fixtures/git-status/many-untracked/expected.txt` — Synced the blending `\t… 118 more changed/untracked files` line to `[lacon: collapsed 118 lines]` so the existing bundled-rule fixture matches the new engine output (see Deviations — this is a minimal byte-exact sync; the fuller git-status re-audit is Plan 03).

## Decisions Made
- **D-07 discretion — fixed marker (option a):** Dropped the free-form `summary_template` emission entirely in favor of the fixed `[lacon: collapsed {N} lines]` marker. Rationale: simplest, impossible to blend into tool output, fewest moving parts (per RESEARCH Pattern 2 recommendation). The `summary_template` field still parses from YAML (loader unchanged) but is no longer emitted; bound as `summary_template: _` at the `step()` site to avoid an unused-binding warning.
- **In-run guard alignment:** Changed the in-run emission guard from `*kept_so_far > 0 || *dropped > 0` to `*dropped > 0`, matching the flush-path CR-03 guard. State reset (`kept_so_far`/`dropped` → 0) remains unconditional so the next run starts clean. This removes any path where a marker could appear with `dropped == 0`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Synced git-status/many-untracked fixture to the new engine marker**
- **Found during:** Task 1 (wave-merge full-suite verification step)
- **Issue:** The engine marker change reaches the rust-embed'd `git-status` rule, so `all_bundled_rule_fixtures` failed — its `expected.txt` still carried the old blending `\t… 118 more changed/untracked files` line, while the engine now correctly emits `[lacon: collapsed 118 lines]`. Left unfixed, the workspace test suite stays red.
- **Fix:** Updated the single blending line in `tests/fixtures/git-status/many-untracked/expected.txt` to the new marker. The `git-status.yaml` rule itself was NOT touched — only the byte-exact expectation of the current rule's output was synced. The fuller git-status re-audit (removing the collapse stage, `exempt_from_reduction_check`, new tabular scenarios) is explicitly Plan 03's scope and is left to it.
- **Files modified:** tests/fixtures/git-status/many-untracked/expected.txt
- **Verification:** `cargo test -p lacon-core --test bundled_rules` → 1 passed; full `cargo test --workspace` → all green (44 `test result: ok`, 0 failures).
- **Committed in:** `62f5177` (Task 1 commit)

---

**Total deviations:** 1 auto-fixed (1 blocking)
**Impact on plan:** The fixture sync was necessary to keep the suite green after the engine change; it does not preempt Plan 03's design decisions (rule YAML untouched). No scope creep.

## Issues Encountered
- Two `collapsible_if` clippy warnings surfaced at the `CollapseRepeated` flush arm (`stages.rs:444`) and the pre-existing `MaxBytes` flush arm (`stages.rs:458`). The flush-arm `if *dropped > 0` guard pre-existed at HEAD (was line 438 before this change), so this is a pre-existing warning shape, not newly introduced — left untouched per the scope boundary. CI gates on `cargo test` (green), not on clippy warnings.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Marker format `[lacon: collapsed {N} lines]` is established and is the contract Plan 03 (Wave 2) depends on for the git-status rule re-audit, `filter-rule-schema.md` spec update, and new tabular fixtures.
- `dedupe` confirmed verbatim-only; no further engine change needed for the fabrication-safety requirement.

## Self-Check: PASSED

- FOUND: crates/lacon-core/src/pipeline/stages.rs
- FOUND: tests/fixtures/primitives/collapse_repeated/expected.txt
- FOUND: tests/fixtures/git-status/many-untracked/expected.txt
- FOUND commit: 62f5177

---
*Phase: 09-output-fidelity-safety-no-fabrication-on-dedupe-collapse-and*
*Completed: 2026-05-31*
