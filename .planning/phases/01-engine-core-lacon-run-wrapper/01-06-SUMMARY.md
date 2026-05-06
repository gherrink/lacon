---
phase: 01-engine-core-lacon-run-wrapper
plan: "06"
subsystem: cli
tags: [rust, clap, assert_cmd, predicates, tempfile, integration-tests, cli-surface]

requires:
  - phase: 01-engine-core-lacon-run-wrapper
    plan: "01"
    provides: workspace Cargo.toml with clap, anyhow, assert_cmd, predicates, tempfile deps
  - phase: 01-engine-core-lacon-run-wrapper
    plan: "03"
    provides: validate_file, RuleLoader, ValidationError Display with byte-exact D-18 format
  - phase: 01-engine-core-lacon-run-wrapper
    plan: "05"
    provides: Runner::new + Runner::run, RunOptions, RunOutcome, RuntimeError variants

provides:
  - lacon binary: clap derive surface with 6 subcommands (run/validate/init/stats/explain/doctor)
  - commands::run::execute: RuleLoader + Runner wiring, lazy hot path (--rule) + eager path (no rule), exit code propagation
  - commands::validate::execute: validate_file dispatch, D-18 byte-exact error format on stderr, exit 0/1
  - commands::init/stats/explain/doctor: stubs printing "not yet implemented" and exiting 2 (REQ-cli-surface-cap pre-enforcement)
  - 17 assert_cmd integration tests across 3 test binaries (cli_run, cli_validate, cli_surface)
  - cli_surface_exposes_exactly_six_subcommands test guards drift from 6-subcommand cap

affects: [02-sqlite-tracker, 03-adapter-claudecode, 04-stats-explain-doctor]

tech-stack:
  added:
    - regex = { workspace = true } added to lacon-cli/Cargo.toml (used in run.rs match evaluator)
  patterns:
    - "clap v4 derive: #[derive(Parser)] Cli + #[derive(Subcommand)] CliCommand enum"
    - "trailing_var_arg = true with allow_hyphen_values captures everything after -- as argv"
    - "std::process::exit(exit_code) called at the dispatch level to propagate subprocess exit codes"
    - "Stub commands return Ok(2) for exit-code convention, print to stderr"

key-files:
  created:
    - crates/lacon-cli/src/cli.rs
    - crates/lacon-cli/src/commands/mod.rs
    - crates/lacon-cli/src/commands/run.rs
    - crates/lacon-cli/src/commands/validate.rs
    - crates/lacon-cli/src/commands/init.rs
    - crates/lacon-cli/src/commands/stats.rs
    - crates/lacon-cli/src/commands/explain.rs
    - crates/lacon-cli/src/commands/doctor.rs
    - crates/lacon-cli/tests/cli_run.rs
    - crates/lacon-cli/tests/cli_validate.rs
    - crates/lacon-cli/tests/cli_surface.rs
  modified:
    - crates/lacon-cli/src/main.rs
    - crates/lacon-cli/Cargo.toml

key-decisions:
  - "regex added to lacon-cli [dependencies] (not just dev-deps) because run.rs rule_matches_argv uses it at runtime for command_regex matching"
  - "run.rs includes full rule_matches_argv implementation (command/args_prefix/args_contain/command_regex/any/all) for the eager no-rule path; Phase 3 match evaluator will replace/augment this"
  - "lacon --version cold-start ~1ms median (5 runs, release binary) — well within 10ms budget; plan-B (pico-args) not needed at this stage (PLAN-07 owns formal benchmark)"

patterns-established:
  - "assert_cmd::Command::cargo_bin builds from debug target; run tests via `cargo test -p lacon-cli`"
  - "Integration tests use tempdir + write_rule helper to create .lacon/rules/ structure in isolation"
  - "cli_surface.rs ALLOWED_SUBCOMMANDS constant guards against accidental 7th subcommand addition"

requirements-completed:
  - REQ-cli-run
  - REQ-cli-validate

duration: 8min
completed: "2026-05-06"
---

# Phase 01 Plan 06: CLI surface Summary

**clap v4 derive CLI with 6 subcommands (`run`/`validate` fully wired, `init`/`stats`/`explain`/`doctor` stubbed at exit 2), 17 assert_cmd integration tests, and a pre-enforcement guard for the REQ-cli-surface-cap structural constraint**

## Performance

- **Duration:** 8 min
- **Started:** 2026-05-06T09:08:43Z
- **Completed:** 2026-05-06T09:17:07Z
- **Tasks:** 3 (all feat, Tasks 2+3 TDD)
- **Files modified:** 13 (11 created, 2 modified)

## Cold-Start Measurement (CONTEXT.md Benchmark Item 2)

`lacon --version` median wall-clock: **~1ms** (5 runs, release binary with `opt-level = "z"` + `lto = "thin"` + `strip = "symbols"`).

Well within the 10ms cold-start budget for the production hot path (`lacon run`). Plan-B (replace clap derive with pico-args) is not triggered. PLAN-07 owns the formal benchmark with the full startup chain (clap + loader + runner).

## Accomplishments

- `lacon --version` prints `lacon 0.1.0`, `lacon --help` lists exactly 6 subcommands — final surface locked from Phase 1 onward
- `lacon run --rule <id> -- <cmd>` wires `RuleLoader::resolve` (lazy hot path D-14) → `Runner::new` → `runner.run` → `std::process::exit(outcome.exit_code)`
- `lacon run -- <cmd>` (no `--rule`) eager-resolves via `load_all` + `rule_matches_argv` supporting command/args_prefix/args_contain/command_regex/any/all
- `lacon validate <path>` calls `validate_file`, prints errors in D-18 byte-exact format (`<path>:<line>: <Category>: <message>`), exits 0 on clean / 1 on errors
- `init`, `stats`, `explain`, `doctor` stubs print "not yet implemented (Phase N)" to stderr and exit 2 — Phase 3/4 fills bodies without changing the surface
- `cli_surface_exposes_exactly_six_subcommands` test fails if anyone adds a 7th subcommand (T-06-06 mitigation)

## `try_match_via_load_all` Coverage for Phase 3

The Phase 1 eager-path match evaluator (`rule_matches_argv`) implements the full `MatchSpec` schema — `command`, `args_prefix`, `args_contain`, `command_regex`, `any`, `all`. This is sufficient for Phase 1 manual-test usage. Phase 3 (adapter wiring) will use the `--rule <id>` lazy path exclusively in production, so the eager path is only the developer convenience path. Phase 3 may extend or replace `rule_matches_argv` if more complex matching is needed.

## Task Commits

1. **Task 1: Clap derive surface + 6 subcommands + stubs + main entry** - `b705726` (feat)
2. **Task 2: lacon run integration tests (TDD GREEN)** - `077dbde` (feat)
3. **Task 3: lacon validate + cli_surface + cli_validate integration tests** - `4c6ca26` (feat)

**Plan metadata:** _(committed below)_

## Files Created/Modified

- `crates/lacon-cli/src/cli.rs` — clap derive `Cli` + `CliCommand` with 6 variants
- `crates/lacon-cli/src/commands/mod.rs` — barrel module for 6 command modules
- `crates/lacon-cli/src/commands/run.rs` — full `lacon run` dispatch
- `crates/lacon-cli/src/commands/validate.rs` — full `lacon validate` dispatch
- `crates/lacon-cli/src/commands/init.rs` — stub (exit 2, Phase 3)
- `crates/lacon-cli/src/commands/stats.rs` — stub (exit 2, Phase 4)
- `crates/lacon-cli/src/commands/explain.rs` — stub (exit 2, Phase 4)
- `crates/lacon-cli/src/commands/doctor.rs` — stub (exit 2, Phase 4)
- `crates/lacon-cli/src/main.rs` — replaced skeleton with clap parse → 6-subcommand dispatch
- `crates/lacon-cli/Cargo.toml` — added `regex = { workspace = true }`
- `crates/lacon-cli/tests/cli_run.rs` — 7 run integration tests
- `crates/lacon-cli/tests/cli_validate.rs` — 7 validate integration tests
- `crates/lacon-cli/tests/cli_surface.rs` — 3 surface cap enforcement tests

## Decisions Made

- **`regex` added to lacon-cli `[dependencies]`**: The `rule_matches_argv` function in `run.rs` uses `regex::Regex::new` for `command_regex` matching at runtime (not just in tests). Adding it to `[dependencies]` rather than `[dev-dependencies]` is correct.
- **`lacon --version` ~1ms**: Plan-B (pico-args) not warranted. Plan comment records the baseline for PLAN-07.
- **`rule_matches_argv` covers full MatchSpec in Phase 1**: Easier to implement the full schema now than to leave gaps. Phase 3 uses `--rule` path exclusively in production; the eager path is only for manual testing.

## Deviations from Plan

None - plan executed exactly as written. All code from the plan's `<interfaces>` and `<action>` blocks was implemented as specified with no rule violations.

## Issues Encountered

None. All acceptance criteria passed on first build.

## Known Stubs

The `init`, `stats`, `explain`, `doctor` command implementations are intentional stubs:
- `crates/lacon-cli/src/commands/init.rs:5` — `Ok(2)` stub, Phase 3 fills body
- `crates/lacon-cli/src/commands/stats.rs:5` — `Ok(2)` stub, Phase 4 fills body
- `crates/lacon-cli/src/commands/explain.rs:5` — `Ok(2)` stub, Phase 4 fills body
- `crates/lacon-cli/src/commands/doctor.rs:5` — `Ok(2)` stub, Phase 4 fills body

These stubs are architecturally intentional — they exist to lock the 6-subcommand surface in Phase 1 per REQ-cli-surface-cap. They do NOT prevent the plan's goal (wiring `run` and `validate`). Phase 3 and Phase 4 fill the bodies.

## Next Phase Readiness

- Phase 1 plan 07 (PLAN-07) can now measure the full cold-start chain: clap parse + `RuleLoader::resolve` + `Runner::run` against a real rule
- Phase 2 (SQLite tracker) can wrap `commands::run::execute` to capture `InvocationMeta`
- Phase 3 (adapter): PreToolUse hook emits `lacon run --rule <id> -- <cmd>` — the lazy hot path is fully wired
- The `cli_surface_exposes_exactly_six_subcommands` test will fail-fast if Phase 4 accidentally adds an 8th subcommand

## Self-Check

PASSED
- `crates/lacon-cli/src/cli.rs` — FOUND
- `crates/lacon-cli/src/commands/run.rs` — FOUND
- `crates/lacon-cli/src/commands/validate.rs` — FOUND
- `crates/lacon-cli/tests/cli_run.rs` — FOUND
- `crates/lacon-cli/tests/cli_validate.rs` — FOUND
- `crates/lacon-cli/tests/cli_surface.rs` — FOUND
- Commits b705726, 077dbde, 4c6ca26 — all in git log

---
*Phase: 01-engine-core-lacon-run-wrapper*
*Completed: 2026-05-06*
