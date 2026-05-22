---
phase: 05-bundled-tier-1-rules
plan: 08
subsystem: bundled-rules
tags: [pytest, extends, test-runner-family, D-06, D-10, on_error]
requires:
  - "Phase 1 engine: RuleLoader, filter_bytes, primitives, extends/merge_rules"
  - "05-01 Wave-0 runner: crates/lacon-core/tests/bundled_rules.rs"
  - "05-02 D-06 verdict + test-base.yaml + loader.rs bundled->bundled extends fix"
provides:
  - "bundled-rules/pytest.yaml — pytest / python -m pytest success + on_error rule (extends test-base)"
  - "tests/fixtures/pytest/verbose-pass — exit 0 success fixture (ratio ~0.072)"
  - "tests/fixtures/pytest/assert-failure — exit 1 on_error fixture (must_keep_lines)"
affects:
  - "05-09 (vitest/jest): same test-runner family pattern (extends test-base + own on_error)"
tech-stack:
  added: []
  patterns:
    - "extends: bundled/test-base on pytest — parent prepends strip_ansi to the success head (ADR-0012)"
    - "child defines its own on_error keep whitelist (overrides the generic parent on_error)"
    - "match.any covers `pytest` and `python -m pytest` (D-10)"
key-files:
  created:
    - bundled-rules/pytest.yaml
    - tests/fixtures/pytest/verbose-pass/input.txt
    - tests/fixtures/pytest/verbose-pass/expected.txt
    - tests/fixtures/pytest/verbose-pass/meta.yaml
    - tests/fixtures/pytest/assert-failure/input.txt
    - tests/fixtures/pytest/assert-failure/expected.txt
    - tests/fixtures/pytest/assert-failure/meta.yaml
  modified: []
decisions:
  - "APPLIED 05-02 D-06 verdict: pytest uses extends: bundled/test-base (not copy-the-parent)"
  - "pytest defines its own precise on_error whitelist rather than inheriting the generic parent on_error"
  - "pytest test failures exit 1 (not 101 like cargo) — recorded as exit_code: 1 in the failure meta"
metrics:
  duration: ~20m
  completed: 2026-05-22
  tasks: 2
  files: 7
---

# Phase 5 Plan 08: pytest rule + fixtures Summary

The `pytest` bundled rule (second member of the test-runner family) — drops the
verbose per-test `PASSED [ NN%]` lines plus the session-start header block on
success while keeping the `=== N passed ===` summary, and preserves the full
traceback signal (`FAILURES` / `E ` / `>` / `file:LINE: ExceptionType` / `FAILED`
/ `=== N failed ===`) on failure via its own `on_error` pipeline. Authored with
`extends: bundled/test-base` per the 05-02 D-06 verdict, with two real
`pytest 9.0.2` fixtures.

## What was built

- **bundled-rules/pytest.yaml** — `id: pytest`, `extends: bundled/test-base`,
  `match.any` covering `pytest` and `python -m pytest` (D-10). Success drops
  `PASSED\s+\[`, `^platform `, `^cachedir:`, `^rootdir:`, `^plugins:`,
  `^collecting`, `^collected`, and the `=== test session starts ===` banner;
  keeps the final `=== N passed ===` banner (not matched by any drop). The
  parent prepends `strip_ansi` (ADR-0012). Own `on_error`: `strip_ansi` + a
  single OR-alternation `keep_regex`
  (`=+ FAILURES =+|^E |^>|:\d+: \w*Error$|^FAILED |=+ \d+ failed.* =+`) +
  `keep_tail: { lines: 60 }`.
- **tests/fixtures/pytest/verbose-pass** (exit 0) — real `pytest -v` capture of a
  throwaway 10-passing-test module. The 10 `PASSED` lines + header block drop;
  the `=== 10 passed ===` banner survives. Reduction ratio **0.072** (92.8%
  saved), well under the 0.5 floor. `must_keep_lines: ["10 passed"]`.
- **tests/fixtures/pytest/assert-failure** (exit 1 → on_error) — real default
  `pytest` capture: 2 failing tests (an `AssertionError` + a `KeyError`) + 1
  passing. Reduction-exempt; `must_keep_lines` asserts the assertion text
  (`assert 6 == 7`), the `KeyError`, the `file:LINE: AssertionError` marker, the
  `FAILED` line, and the `2 failed, 1 passed` banner all survive on_error
  (T-5-03 / V5).

## Applied 05-02 D-06 verdict (extends, not copy)

Per 05-02-SUMMARY: **cross-bundled `extends` WORKS — keep it.** pytest carries
`extends: bundled/test-base`, mirroring `cargo-test`'s structure: inherit the thin
shared `strip_ansi` success head, add the pytest-specific drops, and define an
own `on_error` whitelist tuned to pytest's failure markers (overriding the generic
parent on_error per ADR-0012 scalar inheritance). `lacon validate
bundled-rules/pytest.yaml` → exit 0 (extends resolves through the same-dir parent
lookup), and the fixture runner replays it through the embedded bundled layer
green. No fallback to copy-the-parent was needed.

## Deviations from Plan

None — plan executed exactly as written. Both tasks landed as specified: the
extends-vs-copy verdict was applied (extends), no `max_bytes` was hand-placed
(D-07), no look-around tokens were used (RE2-safe), and both fixtures are real
captured `pytest 9.0.2` output (not synthetic).

## Out-of-scope (NOT fixed, pre-existing)

- `cli_doctor::doctor_all_green_passes_and_exits_zero` fails with
  `CARGO_BIN_EXE_test_emitter is unset` when `cargo test -p lacon-cli --test
  cli_doctor` runs in isolation (the `test_emitter` helper binary isn't
  auto-built for a single `--test` target). This is the exact pre-existing issue
  documented in 05-02-SUMMARY: it is unrelated to this plan and passes once
  `cargo build -p test_emitter` runs (or in a full `cargo test` workspace run
  that builds all bin targets). Verified: with `test_emitter` built, the full
  workspace `cargo test` is all green. Not caused by this plan; not fixed.

## Verification

- `lacon validate bundled-rules/pytest.yaml` → exit 0 (extends resolves).
- `cargo build -p lacon-cli` → clean.
- `cargo test --test bundled_rules` → green; 16 fixtures asserted (incl. the 2
  new pytest scenarios).
- `cargo test` (full workspace, with `test_emitter` built) → ALL GREEN.
- verbose-pass reduction ratio 0.072 ≤ 0.5; assert-failure `must_keep_lines` all
  survive on_error.

## No max_bytes / no look-around / adjacency

- No hand-placed `max_bytes` in the YAML (auto-injected 32768, D-07).
- No look-around / backreferences in any regex (RE2-safe).
- on_error keep is a single alternation `keep_regex` (one stage) — no adjacency
  hazard; `keep_tail` placed after it. The success pipeline has only `drop_regex`
  stages (no keep adjacency to manage); parent `strip_ansi` runs first.

## Self-Check: PASSED

- All 7 created files exist on disk (pytest.yaml + 6 fixture files).
- Both task commits exist (706d4f5 feat, e555bdc test).
- STATE.md / ROADMAP.md untouched (orchestrator-owned).
