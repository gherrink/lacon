---
phase: 05-bundled-tier-1-rules
plan: 07
subsystem: testing
tags: [bundled-rules, tsc, eslint, typescript, fixtures, strip_ansi, dedupe, keep_around_match, keep_tail]

# Dependency graph
requires:
  - phase: 05-bundled-tier-1-rules (plan 01)
    provides: bundled_rules.rs fixture-walking runner + rust-embed bundled-rules/ layer + meta.yaml schema (exit_code, exempt_from_reduction_check, must_keep_lines)
provides:
  - tsc bundled rule (strip_ansi + dedupe + keep_tail; failure-primary, output IS the signal)
  - eslint bundled rule (strip_ansi + keep_around_match preserving the file-path header)
  - 4 fixtures (tsc/type-errors, tsc/clean, eslint/lint-errors, eslint/clean) from real npx-captured output
affects: [05-bundled-tier-1-rules phase tracking, future TypeScript-toolchain rule additions, fixture-regeneration workflow]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "tsc-style 'output IS the signal' pipeline (RESEARCH Pattern 3): ANSI strip + dedupe + tail cap, no line dropping"
    - "eslint stylish path-header preservation via keep_around_match before:1 on the ' (error|warning) ' detail line"
    - "Failure fixture as the PRIMARY scenario when a tool emits nothing on success; reduction-exempt with error survival proven by must_keep_lines"
    - "Real-output capture for non-installed tools via npx in a throwaway scratch project; deps never enter repo manifests"

key-files:
  created:
    - bundled-rules/tsc.yaml
    - bundled-rules/eslint.yaml
    - tests/fixtures/tsc/type-errors/{input,expected}.txt + meta.yaml
    - tests/fixtures/tsc/clean/{input,expected}.txt + meta.yaml
    - tests/fixtures/eslint/lint-errors/{input,expected}.txt + meta.yaml
    - tests/fixtures/eslint/clean/{input,expected}.txt + meta.yaml
  modified: []

key-decisions:
  - "tsc captured with --pretty false for a stable one-line `file(line,col): error TSxxxx:` form (no ANSI carets)"
  - "eslint captured with NO_COLOR=1 to avoid stylish ANSI; absolute scratch temp-dir path header anonymized to bare `bad.js`"
  - "on_error mirrors the success pipeline shape for both rules (failure output is all signal; dedupe+tail caps without dropping errors)"
  - "All four fixtures exempt_from_reduction_check: true — the ≥50% floor is structurally unachievable for these two 'output IS the signal' tools"

patterns-established:
  - "Pattern 3 (tsc): minimal-reduction pipeline where the output is entirely errors"
  - "keep_around_match before:1 preserves a grouping header line above each kept detail line"

requirements-completed: [REQ-bundled-rules-tier1, REQ-bundled-rules-format]

# Metrics
duration: ~10min
completed: 2026-05-22
---

# Phase 5 Plan 07: tsc and eslint Bundled Rules Summary

**tsc and eslint bundled rules with failure-primary fixtures captured from real npx output (typescript 6.0.3, eslint 10.4.0), where the error output IS the signal and survival is asserted via must_keep_lines.**

## Performance

- **Duration:** ~10 min
- **Started:** 2026-05-22T05:02Z (approx)
- **Completed:** 2026-05-22T05:12Z
- **Tasks:** 2
- **Files modified:** 14 (2 rules + 12 fixture files)

## Accomplishments
- `tsc` rule: `strip_ansi` → `dedupe: {max_kept: 1}` → `keep_tail: {lines: 100}` on both success and `on_error` (RESEARCH Pattern 3 — every `error TSxxxx` line is unique signal).
- `eslint` rule: `strip_ansi` → `keep_around_match: {pattern: ' (error|warning) ', before: 1, after: 0}` to preserve each error/warning detail plus the file-path header above it; `on_error` adds a `keep_tail: {lines: 60}` cap.
- 4 fixtures captured from genuine npx output (tsc/eslint are NOT installed locally): tsc/type-errors (7 `error TS` lines, exit 2), tsc/clean (empty, exit 0), eslint/lint-errors (3 errors + 1 warning, exit 1), eslint/clean (empty, exit 0).
- `cargo test --test bundled_rules` asserts all 4 fixtures green (byte-exact + must_keep_lines); `lacon doctor` reports both rules parse cleanly.

## Task Commits

Each task was committed atomically:

1. **Task 1: Author tsc.yaml and eslint.yaml** - `c63588b` (feat)
2. **Task 2: Capture tsc + eslint fixtures via npx (4 fixtures)** - `fc5927d` (test)

## Files Created/Modified
- `bundled-rules/tsc.yaml` - tsc / tsc --noEmit rule (ANSI strip + dedupe + tail; failure-primary)
- `bundled-rules/eslint.yaml` - eslint rule (keep error/warning detail + path header)
- `tests/fixtures/tsc/type-errors/{input,expected,meta}` - primary failure fixture (exit 2, must_keep: `error TS`)
- `tests/fixtures/tsc/clean/{input,expected,meta}` - empty success path (exit 0)
- `tests/fixtures/eslint/lint-errors/{input,expected,meta}` - primary failure fixture (exit 1, must_keep: `error`, `no-unused-vars`)
- `tests/fixtures/eslint/clean/{input,expected,meta}` - empty clean path (exit 0)

## Decisions Made
- **tsc `--pretty false`:** captured the stable one-line error form rather than pretty + ANSI carets, recorded in meta notes. `strip_ansi` remains in the pipeline as a no-op guard for pretty captures.
- **eslint `NO_COLOR=1`:** captured plain stylish output; `strip_ansi` still first in the pipeline (Pitfall 1).
- **Path anonymization:** the eslint absolute scratch temp-dir prefix (`/tmp/lacon-eslint.XXXX/`) was stripped to the bare `bad.js` so the fixture isn't tied to a throwaway dir (PATTERNS: "lightly trimmed/anonymized").
- **Reduction exemption:** all four fixtures are `exempt_from_reduction_check: true` per the plan — for these two tools the ≥50% floor is structurally unachievable (tsc errors are all unique; a single-file eslint capture has no per-file PASS lines to drop). Error survival is proven by must_keep_lines.
- **eslint `✖ N problems` summary dropped:** `keep_around_match` keeps the path header + detail lines but not the summary footer (it matches neither the pattern nor before:1). Acceptable — must_keep_lines targets the error detail, which survives.

## Deviations from Plan

None - plan executed exactly as written. Rule shapes, fixture scenarios, exit codes, and exemption flags all match the plan's `<interfaces>` and Task specs.

## Issues Encountered
- **Worktree cwd-drift (#3097) corrected:** initial `cd /home/maurice/Projects/gherrink-lacon` resolved to the MAIN repo (on `main`), not this agent's worktree (`.claude/worktrees/agent-ae5f270103e080ef1`). The first `tsc.yaml`/`eslint.yaml` writes landed in the main repo. Detected via `git worktree list`, moved both files into the worktree, and switched all subsequent git/build commands to `git -C "$WT"` + absolute worktree paths + `(cd "$WT" && ...)` subshells. Sibling agents' stray files in the main repo (`cargo-test.yaml`, `test-base.yaml`) were left untouched. No commits ever landed on `main`.
- **npx install-warning noise:** the first npx invocation for each tool emitted an `npm warn exec ... will be installed` line into the capture. Re-ran each capture after the package was cached so the fixtures contain only genuine tool output.
- **tsc 6.0.3 `moduleResolution=node` deprecation:** the initial scratch tsconfig used `moduleResolution: node`, which TS 6 reports as a deprecation error (TS5107) masking the intended type errors. Switched to `moduleResolution: bundler` and re-captured — clean 7-line type-error output.
- **`cli_doctor::doctor_all_green_passes_and_exits_zero` flaky under parallel full-suite run (out of scope):** the test failed once during a `cargo test` whole-workspace run but passes in isolation (`--test cli_doctor --test-threads=1`) and `lacon doctor` exits 0 directly. Root cause is 8 parallel worktree agents sharing one `$HOME` `~/.local/share/lacon/history.db` (WAL/db-perms contention) — a test-harness concurrency artifact, not a regression from these rules. Logged here rather than fixed (pre-existing, out of this plan's scope). The plan's own verification target `cargo test --test bundled_rules` is green.

## Threat Surface
No new threat surface beyond the plan's `<threat_model>`. tsc/eslint regexes are RE2 (linear-time, no ReDoS). npx-fetched typescript/eslint were used in a throwaway scratch project for capture only; their deps never entered any repo manifest (no Cargo.toml or node deps changed).

## Next Phase Readiness
- Two of the ten Tier 1 rules (tsc, eslint) complete and green; the shared `bundled_rules.rs` runner now asserts 4 more fixtures.
- Pattern 3 (output IS the signal) and the keep_around_match header-preservation pattern are now exercised end-to-end for future rule authors.
- No blockers introduced.

## Self-Check: PASSED

All 14 created files exist on disk; both task commits (`c63588b`, `fc5927d`) are reachable. `cargo test --test bundled_rules` asserts 4 fixtures green.

---
*Phase: 05-bundled-tier-1-rules*
*Completed: 2026-05-22*
