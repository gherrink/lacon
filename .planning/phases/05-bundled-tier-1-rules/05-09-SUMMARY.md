---
phase: 05-bundled-tier-1-rules
plan: 09
subsystem: bundled-rules
tags: [vitest, jest, test-runner, extends, D-06, D-10, npx-capture, on_error]

# Dependency graph
requires:
  - phase: 05-01
    provides: "Wave-0 fixture runner crates/lacon-core/tests/bundled_rules.rs"
  - phase: 05-02
    provides: "test-base.yaml parent + D-06 verdict (bundled->bundled extends WORKS) + loader.rs fix"
provides:
  - "bundled-rules/vitest.yaml — vitest / vitest run rule (drop per-file PASS + timing, keep Test Files/Tests summary)"
  - "bundled-rules/jest.yaml — jest / npx jest rule (drop PASS/Snapshots/Time/Ran-all, keep Test Suites:/Tests: summary)"
  - "4 fixtures (vitest all-pass + mixed-run, jest all-pass + suite-failure) from real npx output"
  - "Completes the test-runner family (cargo-test, pytest, vitest, jest) on the shared test-base parent"
affects: [phase-05-verification, phase-05-evolve]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "vitest/jest extend bundled/test-base (D-06): inherit strip_ansi head, add own drops + override on_error"
    - "vitest per-file PASS drop matches ASCII tail '(N tests) Nms$' (multibyte glyph avoided); failing-file line carries '| failed)' so it is NOT dropped"
    - "jest output is on STDERR — captured via 2>&1 merge (filter_bytes takes merged bytes)"
    - "match.any with two alternatives for tool + npx-wrapped invocation (D-10)"

key-files:
  created:
    - bundled-rules/vitest.yaml
    - bundled-rules/jest.yaml
    - tests/fixtures/vitest/all-pass/{input,expected}.txt + meta.yaml
    - tests/fixtures/vitest/mixed-run/{input,expected}.txt + meta.yaml
    - tests/fixtures/jest/all-pass/{input,expected}.txt + meta.yaml
    - tests/fixtures/jest/suite-failure/{input,expected}.txt + meta.yaml
  modified: []

key-decisions:
  - "APPLIED 05-02 D-06 verdict: both rules use extends: bundled/test-base (not copy-the-parent)"
  - "Each child defines its own on_error (precise per-tool keep whitelist), overriding the inherited generic parent on_error per ADR-0012 scalar inheritance"
  - "vitest all-pass captured with --reporter=default (forces the per-file '(N tests) Nms' lines the rule targets); jest all-pass captured colorized via PTY so strip_ansi is genuinely exercised"

patterns-established:
  - "Test-runner family: strip_ansi (inherited) -> per-test/per-suite PASS drop -> keep summary; on_error keeps failure detail + summary + keep_tail"

requirements-completed: [REQ-bundled-rules-tier1, REQ-bundled-rules-format]

# Metrics
duration: ~35min
completed: 2026-05-22
---

# Phase 5 Plan 09: vitest + jest Bundled Rules Summary

**vitest and jest test-runner rules (both `extends: bundled/test-base` per the D-06 verdict) that drop per-file/per-suite PASS lines + timing scaffolding and keep the Test Files/Tests (vitest) and Test Suites:/Tests: (jest) summary, with per-tool on_error preserving the failure detail — all four fixtures captured from real npx runs (vitest 4.1.7, jest 30.4.1).**

## Performance

- **Duration:** ~35 min
- **Started:** 2026-05-22T04:52Z (approx)
- **Completed:** 2026-05-22T05:27Z
- **Tasks:** 2
- **Files modified:** 14 (2 rules + 12 fixture files)

## Accomplishments

- **vitest.yaml** — `extends: bundled/test-base`, `match.any` for `vitest` and `vitest run` (D-10). Success drops the per-file PASS line (ASCII `(N tests) Nms$` tail) + RUN/Duration/Start at/Transform/Setup/Collect/Environment/Prepare timing lines, keeps `Test Files`/`Tests` summary. Own `on_error` keeps failed/FAIL/`(N tests`/summary/expectation lines + keep_tail.
- **jest.yaml** — `extends: bundled/test-base`, `match.any` for `jest` and `npx jest` (D-10). Success drops `^PASS `/`^Snapshots:`/`^Time:`/`^Ran all test suites`, keeps `Test Suites:`/`Tests:`. Own `on_error` keeps `^FAIL `/`● `/`✕ `/`Expected:`/`Received:`/`at … (file:line:col)`/summary + keep_tail.
- **4 real-output fixtures** captured via npx in throwaway node projects (vitest/jest NOT installed in this repo), one-shot (no `--watch`):
  - `vitest/all-pass` (exit 0, primary): reduction **0.099** (90.1% saved).
  - `vitest/mixed-run` (exit 1 → on_error): FAIL header + `AssertionError: expected 4 to be 5` + summary survive; exempt.
  - `jest/all-pass` (exit 0, primary): colorized summary; reduction **0.298** (70.2% saved).
  - `jest/suite-failure` (exit 1 → on_error): FAIL/●/Expected/Received/at-frame + summary survive; exempt.
- Completes the test-runner family (cargo-test, pytest, vitest, jest) on the shared `test-base` parent.

## Task Commits

1. **Task 1: Author vitest.yaml and jest.yaml (apply 05-02 verdict)** — `feff3e8` (feat)
2. **Task 2: Capture vitest + jest fixtures via npx (4 fixtures, one-shot)** — `17586e7` (test)

## Files Created/Modified

- `bundled-rules/vitest.yaml` - vitest / vitest run rule (extends test-base; per-file PASS + timing drop; own on_error)
- `bundled-rules/jest.yaml` - jest / npx jest rule (extends test-base; PASS/Snapshots/Time/Ran-all drop; own on_error)
- `tests/fixtures/vitest/all-pass/{input,expected}.txt|meta.yaml` - vitest 4.1.7 success, 12 files / 40 tests
- `tests/fixtures/vitest/mixed-run/{input,expected}.txt|meta.yaml` - vitest 4.1.7 failure (exit 1), on_error path
- `tests/fixtures/jest/all-pass/{input,expected}.txt|meta.yaml` - jest 30.4.1 success, colorized summary (PTY)
- `tests/fixtures/jest/suite-failure/{input,expected}.txt|meta.yaml` - jest 30.4.1 failure (exit 1), on_error path

## Decisions Made

- **Applied the 05-02 D-06 verdict consistently:** both rules use `extends: bundled/test-base` (inherit the thin `strip_ansi` success head; add tool-specific drops; override `on_error`). Verified via `lacon run --rule <id>` (resolves through the embedded bundled layer + extends chain) and `cargo test --test bundled_rules`.
- **vitest per-file PASS drop matches the ASCII tail `\(\d+ tests?\) \d+\s*ms$`** rather than the multibyte `✓` glyph (RE2-safe, glyph-agnostic). The failing-file line ends `| 1 failed) 6ms` and so is intentionally not matched by the trailing `(N tests) Nms$`.
- **vitest all-pass captured with `--reporter=default`** — vitest's default reporter only renders the per-file `✓ … (N tests) Nms` lines (the shape the rule targets) when run interactively; the flag forces that genuine production shape so the rule has lines to drop (Pitfall 5).
- **jest all-pass captured colorized via a PTY** — jest 30 emits a colorized summary block (and, in this fast non-interactive run, no per-suite `PASS` lines at all — see Issues). The colorized capture genuinely exercises `strip_ansi`; reduction comes from stripping the SGR codes plus dropping `Snapshots:`/`Time:`/`Ran all test suites.`.

## Deviations from Plan

None - plan executed exactly as written. Both rules and all four fixtures match the plan's `<interfaces>` and `<action>` shapes; no auto-fixes (Rules 1-3) were required, and no architectural questions (Rule 4) arose.

## Issues Encountered

- **jest 30.4.1 does not emit per-suite `PASS <file>` lines** in a fast, non-interactive (piped) run — they are not present even in the raw PTY byte stream (jest's interactive reporter writes/clears them only for slower, terminal-attached runs). The RESEARCH `^PASS ` shape is from older jest/jestjs.io docs. **Resolution:** the `^PASS ` drop is still authored per D-10/RESEARCH (correct and harmless for versions/configs that emit them), and the jest all-pass primary fixture instead reduces via `strip_ansi` (genuine colorized summary) + the `Snapshots:`/`Time:`/`Ran all test suites.` drops, keeping `Test Suites:`/`Tests:`. Genuine reduction 0.298, under the 0.5 floor. Documented in the fixture's `meta.yaml` notes.
- **vitest/jest emit no ANSI when stdout is piped** (their reporters detect non-TTY). The vitest all-pass fixture is the clean piped form (the production case lacon sees, where `strip_ansi` is a correct no-op); the jest all-pass fixture was captured via a PTY specifically to carry real ANSI so `strip_ansi` is exercised end-to-end.
- **Pre-existing (out of scope, not fixed):** `cli_doctor::doctor_all_green_passes_and_exits_zero` fails with `CARGO_BIN_EXE_test_emitter is unset` when `cargo test` runs before the `test_emitter` helper bin is built. Confirmed pre-existing (same as 05-02 SUMMARY): builds and passes after `cargo build -p test_emitter`; full `cargo test` workspace run is green. Not caused by this plan.

## Verification

- `lacon validate bundled-rules/vitest.yaml` → exit 0; `lacon validate bundled-rules/jest.yaml` → exit 0 (extends resolves).
- `lacon doctor` → exit 0, "10 rule(s) parse cleanly" (9 real + inert test-base), none flagged broken.
- `cargo test --test bundled_rules` → green, 18 fixtures asserted (incl. the 4 new vitest/jest).
- `cargo test` (full workspace) → ALL GREEN (52/19/22/51/24/91/… all `ok`) after `test_emitter` is built.
- Reductions: vitest/all-pass 0.099, jest/all-pass 0.298 (both ≤ 0.5). Failure fixtures route through on_error and their must_keep_lines survive.

## No max_bytes / no look-around / adjacency

- No hand-placed `max_bytes` in either YAML (D-07; loader auto-injects the 32768 cap).
- No look-around / backreferences in any regex (RE2-safe).
- strip_ansi (inherited from the parent) runs at the head of each success pipeline before any keep/drop (Pitfall 1); each on_error re-runs strip_ansi first.

## Next Phase Readiness

- **Test-runner family complete on test-base** (cargo-test, pytest, vitest, jest). With pytest (parallel plan 05-08) merged, the full 10-rule Tier 1 set is present; this plan adds vitest + jest (bringing this worktree's real-rule count from 8 to 10 once 05-08 lands; 9 present here since pytest is the sibling agent's work that merges separately).
- No blockers. STATE.md / ROADMAP.md deliberately untouched (orchestrator-owned).

## Self-Check: PASSED

- All 14 created rule/fixture files + SUMMARY.md exist on disk.
- Both task commits exist (feff3e8, 17586e7).
- STATE.md / ROADMAP.md untouched (orchestrator-owned).

---
*Phase: 05-bundled-tier-1-rules*
*Completed: 2026-05-22*
