---
phase: 05-bundled-tier-1-rules
plan: 02
subsystem: bundled-rules
tags: [cargo-test, extends, D-06, test-runner-family, loader]
requires:
  - "Phase 1 engine: RuleLoader, filter_bytes, primitives, extends/merge_rules"
  - "05-01 Wave-0 runner: crates/lacon-core/tests/bundled_rules.rs"
provides:
  - "bundled-rules/cargo-test.yaml — cargo test success + on_error rule"
  - "bundled-rules/test-base.yaml — shared parent for the test-runner family (extends works)"
  - "D-06 VERDICT: cross-bundled extends RESOLVES — Wave 2 (pytest/vitest/jest) may use extends"
  - "loader.rs fix: load_all now resolves bundled->bundled extends (engine bug fixed)"
affects:
  - "05-08 (pytest), 05-09 (vitest/jest): consume the D-06 extends verdict"
  - "crates/lacon-core/src/rules/loader.rs (engine, all bundled rules with extends)"
tech-stack:
  added: []
  patterns:
    - "extends: bundled/<parent> on a bundled child resolves via find_in_bundled (both lazy + eager paths)"
    - "child on_error overrides inherited parent on_error (ADR-0012 scalar inheritance)"
key-files:
  created:
    - bundled-rules/cargo-test.yaml
    - bundled-rules/test-base.yaml
    - tests/fixtures/cargo-test/clean-run/{input,expected,meta}.txt|yaml
    - tests/fixtures/cargo-test/test-failure/{input,expected,meta}.txt|yaml
  modified:
    - crates/lacon-core/src/rules/loader.rs
    - crates/lacon-core/src/rules/bundled.rs
decisions:
  - "KEEP extends (not copy-the-parent): bundled->bundled resolution works through the runner"
  - "cargo-test defines its own on_error (precise keep whitelist) rather than inheriting the generic parent on_error"
metrics:
  duration: ~25m
  completed: 2026-05-22
  tasks: 2
  files: 10
---

# Phase 5 Plan 02: cargo-test rule + D-06 cross-bundled extends SPIKE Summary

cargo-test bundled rule (drop-PASS success pipeline + context-preserving on_error)
extending a shared `test-base` parent — and the D-06 spike that proves bundled→bundled
`extends` resolves cleanly through the runner, after fixing a latent engine bug it exposed.

## D-06 VERDICT (for Wave 2: pytest / vitest / jest)

**extends WORKS — keep it. Do NOT fall back to copy-the-parent.**

`cargo-test.yaml` carries `extends: bundled/test-base`. Both `test-base` and `cargo-test`
live in the embedded bundled layer, so this is the first real bundled→bundled extends in
the project. Proven two ways:

1. `lacon validate bundled-rules/cargo-test.yaml` → exit 0 (same-dir parent lookup).
2. `lacon run --rule cargo-test -- cat <success input>` → resolves through the **embedded**
   bundled layer (`try_resolve_from_bundled` → `find_in_bundled`) and produces correctly
   merged output (parent `strip_ansi` prepended, then child drops). The runner
   (`cargo test --test bundled_rules`) is byte-exact green against this.

ADR-0012 ordering holds: the parent's `strip_ansi` is **prepended** to the child success
pipeline. The child defines its own `on_error` (a precise cargo keep-whitelist), which
**overrides** the inherited generic parent `on_error` per scalar inheritance — this is the
recommended pattern for Wave 2: inherit the thin shared success head, override `on_error`
per tool where the failure signal differs.

**Wave 2 guidance:** pytest/vitest/jest may safely use `extends: bundled/test-base`. The
genuinely-shared part is just `strip_ansi` (RESEARCH confirmed the per-test-PASS drop regex
differs per tool), so the base stays thin and each child adds its own drops + `on_error`.

## What was built

- **bundled-rules/test-base.yaml** — `id: test-base`, sentinel `command_regex`
  (`^__lacon_test_base_never_matches__$`) so it is loadable but inert in command resolution
  (only ever reached via `extends`). Success pipeline: `strip_ansi`. on_error: `strip_ansi`
  + `keep_around_match` on `(FAILED|panicked|error|assertion)` + `keep_tail`.
- **bundled-rules/cargo-test.yaml** — `extends: bundled/test-base`, `match: { command: cargo,
  args_prefix: [test] }`. Success drops `^test .+ \.\.\. ok$`, Compiling/Finished/Running/
  Updating/Locking/Downloading/Downloaded, and `^running \d+ tests$`; keeps `test result:`.
  on_error (own, overrides parent): `strip_ansi` + a single OR-alternation `keep_regex` for
  `... FAILED`, panicked, assertion, left:/right:, `test result:`, `error:`, `---- ... ----`,
  `failures:`, and indented `tests::` names + `keep_tail: { lines: 80 }`.
- **2 fixtures** under `tests/fixtures/cargo-test/`:
  - `clean-run` (exit 0): real multi-dep cargo test success. Reduction **0.139** (86.1% saved),
    well under the 0.5 floor.
  - `test-failure` (exit 101 → on_error): real run with 2 failing tests; reduction-exempt;
    `must_keep_lines` asserts FAILED/panicked/assertion/failing-test-names/`test result: FAILED`/
    `error: test failed` all survive.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] load_all could not resolve bundled→bundled extends (D-06 spike exposed it)**
- **Found during:** Task 1 spike (running `lacon run --rule cargo-test`, then full workspace tests).
- **Issue:** `RuleLoader::load_all` (eager path, used by `match_argv_via_load_all` — the Claude
  Code hook's command-matching path) flattened bundled-rule `extends` with a **no-op** parent
  lookup `&|_, _| None`. Any bundled rule carrying `extends: bundled/<parent>` (i.e. cargo-test)
  failed at load with "could not find parent rule `test-base`", and that error **poisoned
  load_all for every command** — so the hook emitted empty stdout for ALL matched commands.
  This reddened all 8 `hook_e2e` rewrite tests (symptom: "EOF while parsing a value" — empty
  stdout where JSON was expected). The lazy `resolve()` path was already correct
  (`try_resolve_from_bundled` → `find_in_bundled`); only `load_all` had the gap. This is the
  exact latent risk D-06 / Pitfall 4 flagged ("untested at fixture level — a bug here breaks
  the test-runner rules at once").
- **Fix:** wire `find_in_bundled` as the parent-lookup closure in the `load_all` bundled
  branch, identical to the lazy path. One-site change, makes the two code paths consistent.
- **Files modified:** crates/lacon-core/src/rules/loader.rs
- **Commit:** d6b485b

**2. [Rule 1 - Bug] Stale Phase-1 assertion in `bundled_iter_does_not_panic_on_empty_dir`**
- **Found during:** Task 2 (full workspace test run).
- **Issue:** the test asserted `iter_bundled().count() == 0`, a Phase-1-era expectation that
  `bundled-rules/` holds only `.gitkeep`. Adding the first two real bundled rules made the
  count 2. The test's own comment anticipated this ("Phase 5 will add real rules; this test
  just ensures no panic").
- **Fix:** assert the real invariant — `iter_bundled` yields only `.yaml` entries (`.gitkeep`
  filtered) and does not panic — instead of a brittle fixed count.
- **Files modified:** crates/lacon-core/src/rules/bundled.rs
- **Commit:** d6b485b

Both fixes are Rule 1 (broken behavior directly caused by this plan's change — introducing
the first cross-bundled extends rule). No architectural change; both make existing code paths
consistent with their documented intent.

## Out-of-scope (NOT fixed, pre-existing)

- `cli_doctor::doctor_all_green_passes_and_exits_zero` fails with
  `CARGO_BIN_EXE_test_emitter is unset` when `cargo test -p lacon-cli --test cli_doctor` is run
  in isolation (the `test_emitter` helper binary isn't auto-built for a single `--test`
  target). Verified pre-existing: it fails identically at the base commit with all my files
  removed, and passes once `cargo build -p test_emitter` runs (or in a full `cargo test`
  workspace run that builds all bin targets). Not caused by this plan; not fixed.

## Verification

- `lacon validate bundled-rules/test-base.yaml` → exit 0
- `lacon validate bundled-rules/cargo-test.yaml` → exit 0 (extends resolves)
- `lacon doctor` → exit 0 (my rules not flagged broken)
- `cargo test --test bundled_rules` → green, 2 cargo-test fixtures asserted
- `cargo test` (full workspace) → ALL GREEN after the two fixes (test_emitter built)
- clean-run reduction 0.139 ≤ 0.5; test-failure must_keep_lines all survive on_error

## No max_bytes / no look-around / adjacency

- No hand-placed `max_bytes` in either YAML (auto-injected 32768, D-07).
- No look-around / backreferences in any regex (RE2-safe).
- on_error keep is a single alternation `keep_regex` (one stage) — no adjacency hazard;
  `keep_tail` placed after it.

## Self-Check: PASSED

- All 8 created fixture/rule files + SUMMARY.md exist on disk.
- All 3 task commits exist (d6b485b, 97cef1c, eafc2d1).
- STATE.md / ROADMAP.md untouched (orchestrator-owned).
