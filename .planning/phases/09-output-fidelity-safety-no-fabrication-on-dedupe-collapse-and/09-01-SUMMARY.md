---
phase: 09-output-fidelity-safety-no-fabrication-on-dedupe-collapse-and
plan: 01
subsystem: claude-code-adapter
tags: [bypass, hook, lacon-disable, output-fidelity, safety]
requires:
  - "detect_bypass (Phase 03-04 orchestration)"
  - "run_bypassed (Phase 01/02 engine backstop, D-05)"
provides:
  - "inline LACON_DISABLE=1 env-prefix bypass in the PreToolUse hook"
  - "engine byte-exact run_bypassed passthrough assertion"
affects:
  - "crates/lacon-adapter-claudecode/src/lib.rs"
  - "crates/lacon-adapter-claudecode/tests/hook_e2e.rs"
  - "crates/lacon-cli/tests/cli_run.rs"
tech-stack:
  added: []
  patterns:
    - "leading shell-assignment scan (split_whitespace, break at command word)"
    - "one-layer balanced unquote matching engine as_deref()==Ok(\"1\") semantics"
key-files:
  created: []
  modified:
    - "crates/lacon-adapter-claudecode/src/lib.rs"
    - "crates/lacon-adapter-claudecode/tests/hook_e2e.rs"
    - "crates/lacon-cli/tests/cli_run.rs"
decisions:
  - "Inline parser is a NEW private helper (inline_disable_bypass), not shared with argv_for_resolution — leading-position semantics differ (D-04) and a standalone scan keeps the cold-start budget tight."
  - "Byte-exact backstop test placed in lacon-cli (assert_cmd) not lacon-core — the bypassed subprocess inherits lacon's stdout, which assert_cmd's pipe captures; the prior exit-code-only comment in cli_run was conservative (empirically verified capturable)."
metrics:
  duration: ~5min
  tasks: 2
  files: 3
  completed: 2026-05-31
---

# Phase 9 Plan 01: Inline LACON_DISABLE Bypass Summary

Inline `LACON_DISABLE=1 <cmd>` env-prefix on the PreToolUse hook command string now bypasses filtering (returns PassThrough before any chain split or wrap), with the engine `run_bypassed` byte-exact passthrough locked by a regression test.

## What Was Built

**Task 1 — `detect_bypass` inline env-prefix scan (TDD).** Extended `detect_bypass(command)` in `crates/lacon-adapter-claudecode/src/lib.rs` with a third trigger between the existing `!!` and process-env checks: a new private `inline_disable_bypass(command)` helper that scans the leading shell-assignment prefix of the command string. It iterates `split_whitespace`, and for each token attempts `split_leading_assignment` (`^[A-Za-z_][A-Za-z0-9_]*=`); the first token that is not an assignment is the command word, so the loop breaks there (this is what makes `echo LACON_DISABLE=1` NOT bypass — D-04). For each leading assignment named exactly `LACON_DISABLE`, `unquote_one_layer` strips one balanced `'..'`/`".."` layer and the scan bypasses iff the value equals exactly `"1"` — matching the locked engine `as_deref() == Ok("1")` rule (`1`/`"1"`/`'1'` bypass; `0`/`true`/empty do not). The scan continues past other leading assignments (`FOO=bar LACON_DISABLE=1 echo hi` bypasses). Allocation-light leading scan only, no full POSIX grammar (T-09-02 cold-start budget).

**Task 2 — hook-e2e + engine byte-exact backstop.** Added four hook-e2e tests in `tests/hook_e2e.rs` and one engine-level byte-exact test in `crates/lacon-cli/tests/cli_run.rs` (see Verification).

## TDD Gate Compliance

Task 1 followed RED → GREEN:
- RED: `4e65ba6` — `test(09-01)` added 5 failing unit tests (3 positive cases failed against the process-env-only impl, 2 negatives already passed trivially).
- GREEN: `33318f6` — `feat(09-01)` implemented `inline_disable_bypass` + helpers; all 7 `detect_bypass` tests pass.
- REFACTOR: none needed (code already minimal).

## Verification

| Check | Command | Result |
|-------|---------|--------|
| Unit bypass semantics | `cargo test -p lacon-adapter-claudecode --lib detect_bypass` | 7 passed |
| Hook-layer PassThrough | `cargo test -p lacon-adapter-claudecode --test hook_e2e` | 26 passed (4 new) |
| Engine byte-exact backstop | `cargo test -p lacon-cli --test cli_run run_lacon_disable` | 2 passed |
| Full suite (wave-merge gate) | `cargo test --workspace` | all green, 0 failures |
| Clippy (touched crates) | `cargo clippy -p lacon-adapter-claudecode -p lacon-cli --all-targets` | no warnings in touched files |

New hook-e2e tests:
- `inline_lacon_disable_prefix_passes_through` — `LACON_DISABLE=1 echo hi` (no process env) → empty stdout (PassThrough, no rewrite).
- `inline_lacon_disable_prefix_quoted_passes_through` — `LACON_DISABLE='1' echo hi` → empty stdout.
- `inline_lacon_disable_prefix_bypasses_whole_chain` — `LACON_DISABLE=1 echo a && echo b` → empty stdout (whole-command granularity, not per-segment).
- `non_leading_lacon_disable_does_not_bypass` — `echo LACON_DISABLE=1` → wrapped (`lacon run --rule echo-rule`), proving D-04.

Engine byte-exact backstop (success-criterion #2 engine half):
- `run_lacon_disable_is_byte_exact_passthrough` (`cli_run.rs`) runs `/bin/sh -c "printf 'line1\nline2\n'; echo skip"` twice — once raw, once under `lacon run` with own-env `LACON_DISABLE=1` and a `drop_regex: '.*'` rule that would wipe all output if the pipeline ran — and asserts `bypassed.stdout == raw.stdout` byte-for-byte. **Note for future readers:** the existing `run_lacon_disable_bypasses_filtering` test comment claimed bypass stdout is uncapturable by `assert_cmd` (only exit code). That is conservative: the bypassed inner subprocess inherits `lacon`'s stdout fd, which `assert_cmd` connects to a capture pipe, so the bytes ARE observable — empirically confirmed before adding the assertion.

## Success Criteria

- [x] Inline `LACON_DISABLE=1` (and `"1"`/`'1'`) bypass; `echo LACON_DISABLE=1` and non-"1" values do not.
- [x] Hook returns PassThrough before wrap for the inline prefix, incl. chain inputs (whole-command granularity).
- [x] Engine `run_bypassed` byte-exact passthrough asserted.
- [x] No new clippy warnings; cold-start budget respected (cheap leading scan).

## Deviations from Plan

None — plan executed exactly as written. The byte-exact backstop assertion did not exist (existing coverage only asserted `bypassed=true` / exit code), so it was newly added per the plan's "if no such literal byte-exact assertion exists, ADD one" instruction.

## Out-of-Scope Observations (not fixed)

Pre-existing clippy warnings observed in unrelated locations (NOT touched, logged here only): `lacon-core` collapsible-if (`runtime/mod.rs`), manual case-insensitive ASCII comparison, `&PathBuf`-instead-of-`&Path`, doc list overindent; `lacon-cli` `tracking_e2e` test warning; `test_emitter` missing-lib-target dep note. All pre-existing and outside this plan's scope.

## Self-Check: PASSED

All modified files and all four commits (`4e65ba6` test/RED, `33318f6` feat/GREEN, `6b17d8e` test/Task 2, `e8448ff` docs) verified present.
