---
phase: 09-output-fidelity-safety-no-fabrication-on-dedupe-collapse-and
plan: 03
subsystem: bundled-rules
tags: [git-status, collapse_repeated, dedupe, fabrication-safety, fixtures, filter-rule-schema]

# Dependency graph
requires:
  - phase: 09-output-fidelity-safety-no-fabrication-on-dedupe-collapse-and
    plan: 02
    provides: "Standardized [lacon: collapsed N lines] elision marker in stages.rs (the marker the spec now documents)"
provides:
  - "git-status rule no longer collapses tabular per-file signal — every file line survives verbatim (D-08)"
  - "git-status/many-untracked regenerated as a verbatim-survival fixture (exempt_from_reduction_check, Open Q2)"
  - "git-status/tabular-signal new no-fabrication CLASS fixture (aligned columns + repeated-prefix rows, Open Q1)"
  - "filter-rule-schema.md documents the standardized [lacon: …] collapse marker + the dropped-summary contract change (D-12)"
  - "tsc dedupe fixture confirmed signal-preserving and unchanged (D-10)"
affects: [phase-09 verification, v1.0 milestone fidelity gate]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "exempt_from_reduction_check posture for rules whose output IS the signal (git-status joins tsc) — reduction comes only from ANSI strip + noise drops, never from dropping signal lines"

key-files:
  created:
    - tests/fixtures/git-status/tabular-signal/input.txt
    - tests/fixtures/git-status/tabular-signal/expected.txt
    - tests/fixtures/git-status/tabular-signal/meta.yaml
  modified:
    - bundled-rules/git-status.yaml
    - tests/fixtures/git-status/many-untracked/expected.txt
    - tests/fixtures/git-status/many-untracked/meta.yaml
    - docs/specs/filter-rule-schema.md

key-decisions:
  - "Open Q2 resolved (recommended option taken): removing collapse_repeated makes git-status output approx input, breaching the >=50% reduction floor — exempt git-status/many-untracked via exempt_from_reduction_check: true (tsc 'the output IS the signal' precedent) rather than retaining a narrowed collapse."
  - "D-12 user-facing contract change documented: the free-form collapse_repeated `summary` template is no longer emitted (replaced by the fixed [lacon: collapsed N lines] marker per Plan 02); the `summary` YAML key still parses for backward compatibility but is ignored at emission."
  - "Open Q1 honored: the tabular-signal fixture gates on the fabrication CLASS (aligned status columns + repeated-prefix row_NNN loop rows), NOT the literal `table table table` string (that came from a non-git-status loop and is explicitly not a gate)."

requirements-completed: [REQ-engine-streaming-primitives]

# Metrics
duration: 3min
completed: 2026-05-31
---

# Phase 9 Plan 03: git-status no-fabrication re-audit + spec marker contract Summary

**The git-status rule no longer collapses tabular per-file signal — every changed/untracked file line survives byte-identical; a no-fabrication CLASS fixture proves it, the tsc dedupe fixture is confirmed signal-preserving (D-10), and filter-rule-schema.md now documents the standardized `[lacon: collapsed N lines]` marker plus the dropped-summary contract change.**

## Performance

- **Duration:** ~3 min
- **Started:** 2026-05-31T06:18:56Z
- **Completed:** 2026-05-31T06:21:56Z
- **Tasks:** 3
- **Files modified:** 4 modified + 3 created

## Accomplishments
- **D-08:** Removed the signal-collapsing `collapse_repeated` stage (`pattern: '^\t'`, `max_kept: 5`, tab-indented summary) from `bundled-rules/git-status.yaml`. The success pipeline is now `strip_ansi` + `drop_regex: '^\s*\(use '` only; every tab-indented per-file line survives verbatim. The `on_error` block is untouched (ADR-0010).
- **D-11 / many-untracked regeneration:** `expected.txt` now carries all 123 untracked file lines verbatim (the only drop is the single `(use …)` hint line); no summary line, no substitution, no `[lacon:` marker. `meta.yaml` records `exempt_from_reduction_check: true` with `must_keep_lines` (representative file paths) and the D-08 / Open Q2 rationale.
- **D-11 / no-fabrication CLASS fixture:** Added `tests/fixtures/git-status/tabular-signal/` reproducing the fabrication class — aligned status columns (`new file:` / `modified:` / `renamed: a -> b`) and repeated-prefix loop rows (`src/gen/table/row_NNN.rs`) across staged/unstaged/untracked blocks. Every signal line survives verbatim; `grep -Fxf` survival check passes; no `[lacon:` marker.
- **D-10 / tsc confirmation:** Verified `tests/fixtures/tsc/type-errors/meta.yaml` is `exempt_from_reduction_check: true` with `must_keep_lines: ["error TS"]`, the input has no consecutive duplicates, and `input.txt == expected.txt` (so `dedupe: { max_kept: 1 }` drops nothing). No change made — confirm-only, as planned.
- **D-12 / spec:** Rewrote the `collapse_repeated` entry in `docs/specs/filter-rule-schema.md` to document the fixed `[lacon: collapsed N lines]` marker (mirroring the `max_bytes` `[lacon: truncated, …]` style), explicitly flagging the user-facing contract change that the free-form `summary` template is no longer emitted (the key still parses but is ignored).

## Task Commits

Each task was committed atomically:

1. **Task 1: Remove signal-collapsing collapse_repeated from git-status + regenerate fixture** — `89dada3` (fix)
2. **Task 2: Add no-fabrication class fixture + confirm tsc dedupe (D-10)** — `ea2b2af` (test)
3. **Task 3: Document standardized collapse_repeated [lacon:] marker (D-12)** — `d15e637` (docs)

## Files Created/Modified
- `bundled-rules/git-status.yaml` — Removed the `collapse_repeated` stage (D-08); added a comment block explaining why the file list is signal and the exempt-from-reduction posture. `on_error` untouched.
- `tests/fixtures/git-status/many-untracked/expected.txt` — Regenerated: all 123 file lines verbatim, only the `(use …)` hint dropped, no summary/marker.
- `tests/fixtures/git-status/many-untracked/meta.yaml` — `exempt_from_reduction_check: true`, `must_keep_lines`, D-08 / Open Q2 notes.
- `tests/fixtures/git-status/tabular-signal/{input,expected,meta}.yaml/txt` — New no-fabrication CLASS fixture (created).
- `docs/specs/filter-rule-schema.md` — `collapse_repeated` entry rewritten for the standardized marker + contract-change callout (D-12).

## Decisions Made
- **Open Q2 (reduction-floor exemption) — chose exempt over narrow:** Removing the collapse stage makes git-status output ≈ input, which would breach the `bundled_rules.rs` ≥50% reduction floor. Set `exempt_from_reduction_check: true` on the git-status success fixtures, mirroring tsc ("the output IS the signal"), rather than retaining a narrowed collapse. Rationale: tabular file lists ARE signal; fidelity outranks the reduction target on this one rule (T-09-08 accept). Recorded in `many-untracked/meta.yaml` notes.
- **Open Q1 (fixture gate) — CLASS not literal:** The `tabular-signal` fixture reproduces the fabrication *class* (aligned columns + repeated-prefix rows), NOT the literal `table table table` string, which RESEARCH established came from a non-git-status loop and is explicitly not a gate.
- **D-12 contract change documented as such:** The spec prose explicitly calls out that the free-form `summary` body is no longer emitted — a deliberate user-facing change — while noting the YAML key still parses for backward compatibility.

## Deviations from Plan

None — plan executed exactly as written. All three tasks took the recommended path (removal + exempt for git-status, confirm-only for tsc, contract-change-flag for the spec).

## Issues Encountered
- Pre-existing clippy warnings remain in `lacon-core` (two `collapsible_if` at the `CollapseRepeated`/`MaxBytes` flush arms, already documented in Plan 02's SUMMARY as pre-existing) and in untouched tracking tests. Per the scope boundary these are out of scope for this doc/fixture/rule plan; Task 3 is doc-only and introduces no new Rust warnings. CI gates on `cargo test` (green), not clippy.
- An accidental empty `crates/lacon-core/build.rs` was created mid-task by a `touch` used to probe rebuild behavior, then immediately removed (it was never tracked and never committed). No impact.

## User Setup Required
None — no external service configuration required.

## Next Phase Readiness
- All Phase 9 plans (09-01 inline LACON_DISABLE bypass, 09-02 collapse marker standardization, 09-03 git-status re-audit + spec) are now executed. The fabrication-safety requirement (REQ-engine-streaming-primitives, success-criterion #3) is satisfied across engine, rules, fixtures, and the user-facing spec.
- Ready for phase verification / code review.

## Verification
- `cargo build --workspace && cargo test --workspace` — full suite green (every `test result: ok`, 0 failures across all crates).
- `cargo test -p lacon-core --test bundled_rules` — git-status (many-untracked + tabular-signal) + tsc fixtures pass the walker contract.
- `grep -Fxf` survival checks — every expected line in both git-status fixtures is a verbatim input line; no `[lacon:` marker in either.
- `grep '\[lacon:' docs/specs/filter-rule-schema.md` — spec documents the marker.
- `cargo fmt --check` clean; clippy warnings are all pre-existing/out-of-scope.

## Self-Check: PASSED

- FOUND: bundled-rules/git-status.yaml
- FOUND: tests/fixtures/git-status/many-untracked/expected.txt
- FOUND: tests/fixtures/git-status/many-untracked/meta.yaml
- FOUND: tests/fixtures/git-status/tabular-signal/input.txt
- FOUND: tests/fixtures/git-status/tabular-signal/expected.txt
- FOUND: tests/fixtures/git-status/tabular-signal/meta.yaml
- FOUND: docs/specs/filter-rule-schema.md
- FOUND commit: 89dada3
- FOUND commit: ea2b2af
- FOUND commit: d15e637

---
*Phase: 09-output-fidelity-safety-no-fabrication-on-dedupe-collapse-and*
*Completed: 2026-05-31*
