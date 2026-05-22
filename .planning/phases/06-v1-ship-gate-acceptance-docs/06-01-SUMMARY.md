---
phase: 06-v1-ship-gate-acceptance-docs
plan: 01
subsystem: acceptance-validation
tags: [acceptance, tests, traceability, explain, hot-reload, pnpm-e2e, ship-gate]
requires:
  - "Phase 1-5 complete (engine, tracking, adapter, CLI, bundled rules)"
  - "crates/lacon-core/tests/{primitives,bundled_rules}.rs"
  - "crates/lacon-adapter-claudecode/tests/{chain_split,hook_e2e}.rs"
  - "crates/lacon-cli/src/commands/explain.rs (Runner::filter_bytes byte-replay)"
  - "crates/lacon-core/src/rules/loader.rs (mtime cache, ADR-0013 no-daemon)"
provides:
  - "REQ->test acceptance traceability map (D-01/D-02)"
  - "explain byte-equality proof (SC3, D-03)"
  - "hot-reload two-invocation proof (SC2 second half, D-06)"
  - "hermetic + #[ignore] real pnpm E2E pair (SC2 first half + SC4, D-07)"
affects:
  - ".planning/phases/06-v1-ship-gate-acceptance-docs/ (Plan 02 owns cold-start + CI)"
tech-stack:
  added:
    - "lacon-cli dev-dependency on lacon-adapter-claudecode (test-only, for CARGO_BIN_EXE_lacon-claude-hook)"
  patterns:
    - "assert_cmd::cargo::cargo_bin anti-spoofing binary resolution (T-06-04)"
    - "tempdir cwd + XDG_DATA_HOME/XDG_CONFIG_HOME redirect sandboxing (T-06-02)"
    - "#[ignore = \"<runbook line>\"] for tool-dependent tests (house style)"
    - "deterministic mtime bump via std File::set_modified (no flaky sleep)"
    - "plain assert_eq!/assert! (insta deliberately NOT introduced)"
key-files:
  created:
    - ".planning/phases/06-v1-ship-gate-acceptance-docs/06-ACCEPTANCE-MAP.md"
    - "crates/lacon-cli/tests/hot_reload.rs"
    - "crates/lacon-cli/tests/pnpm_e2e.rs"
  modified:
    - "crates/lacon-cli/tests/cli_explain.rs (added byte-equality test)"
    - "crates/lacon-cli/Cargo.toml (test-only adapter dev-dep)"
decisions:
  - "D-03/D-06 are PROOF tests of shipped behavior — green on first run, no new production code (prove-not-build)"
  - "explain byte-equality uses a plain-ASCII payload so WR-01 safe-view sanitization is a no-op, keeping the comparison a true byte-equality without regressing the C0/C1/ESC neutralization"
  - "hot-reload mtime bumped to an absolute future instant (now+10s) via File::set_modified, not a sleep — deterministic on coarse-resolution filesystems"
  - "pnpm real test runs the existing pnpm binary inside the #[ignore]d body; it never INSTALLS pnpm — CI hermeticity preserved"
  - "added lacon-adapter-claudecode as a lacon-cli dev-dep so cargo_bin('lacon-claude-hook') resolves deterministically in isolation (no cycle: adapter -> lacon-core only)"
metrics:
  duration: ~30min
  completed: 2026-05-22
  tasks: 3
  files: 5
---

# Phase 6 Plan 01: v1 ship gate acceptance (audit + proof tests) Summary

Audited the existing Phase 1-5 acceptance coverage into a REQ->test traceability map, then added the three genuine proof/gate tests the audit exposed — `explain` byte-equality (SC3), a two-invocation hot-reload proof (SC2 second half), and a hermetic + `#[ignore]`d real pnpm end-to-end pair (SC2 first half + SC4) — all hermetic, all green, with no new product machinery.

## What Was Built

### Task 1 — REQ->test acceptance traceability map (D-01/D-02)
`.planning/phases/06-v1-ship-gate-acceptance-docs/06-ACCEPTANCE-MAP.md` maps all 6 Phase-6 acceptance REQs to their proving test(s), the exact `cargo test`/bench command, and an audited green/red status. `REQ-acceptance-test-coverage` is broken into its three sub-claims (primitives / splitter / bundled). `REQ-acceptance-cold-start-budget` and the CI-hermetic sub-claim are explicitly cross-referenced to **Plan 02**. `REQ-acceptance-bundled-reduction` is documented as already met by `bundled_rules.rs` (cites the `<=0.5` ratio + `must_keep_lines` assertions, D-02).

The three audited suites were run and confirmed green (not assumed):
- `cargo test --test primitives` -> 10 passed
- `cargo test --test bundled_rules` -> walker green, 20 fixtures asserted
- `cargo test -p lacon-adapter-claudecode --test chain_split` -> 19 passed

### Task 2 — explain byte-equality (D-03) + hot-reload proof (D-06)
- `crates/lacon-cli/tests/cli_explain.rs`: added `explain_filtered_column_byte_equals_run_output` alongside the existing 5 tests (now 6). It seeds raw bytes + an invocation row, then asserts the filtered column `explain` re-derives via `Runner::filter_bytes` byte-equals the rule's filter result (`assert_eq!` on the column lines, not a `.contains(` check). Uses a plain-ASCII payload so the WR-01 safe-view C0/C1/ESC neutralization is a no-op (no regression).
- `crates/lacon-cli/tests/hot_reload.rs` (new): `rule_edit_takes_effect_on_next_invocation` runs two `lacon run` invocations across an mtime-bumped rule rewrite (v1 keeps lines, v2 drops them) and asserts the second fresh process reflects v2 — proving hot reload via the no-daemon model (ADR-0013) with no watcher/daemon/new cache.

### Task 3 — hermetic + real pnpm E2E (D-07)
`crates/lacon-cli/tests/pnpm_e2e.rs` (new) with two tests:
- `pnpm_e2e_hermetic` (default lane): drives init->hook-rewrite->run with the `test_emitter` stub. Asserts the `lacon-claude-hook` rewrite wraps the matched command as `lacon run --rule pnpm-stub -- ...`, then executes it and asserts the filtered stub output reaches the caller.
- `pnpm_e2e_real` (`#[ignore]`d, verbatim runbook line): real `pnpm install` through `lacon init` -> PreToolUse rewrite (`pkg-install`) -> `lacon run`, asserting non-empty, reduced output. Skipped by default `cargo test` so CI stays hermetic.

All binaries resolved via `assert_cmd::cargo::cargo_bin` (anti-spoofing, T-06-04); all tests sandboxed via tempdir cwd + XDG redirection (T-06-02).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking config] Added lacon-adapter-claudecode as a lacon-cli dev-dependency**
- **Found during:** Task 3
- **Issue:** `assert_cmd::cargo::cargo_bin("lacon-claude-hook")` resolved only via the `target/debug` fallback (requires the hook to have been built by a prior invocation). Running `cargo test -p lacon-cli --test pnpm_e2e` in isolation without a prior build would leave `CARGO_BIN_EXE_lacon-claude-hook` unset.
- **Fix:** Added `lacon-adapter-claudecode = { path = "../lacon-adapter-claudecode" }` to `[dev-dependencies]` in `crates/lacon-cli/Cargo.toml`, so cargo guarantees the hook binary is built and sets the env var. No dependency cycle (the adapter depends only on `lacon-core`).
- **Files modified:** `crates/lacon-cli/Cargo.toml`, `Cargo.lock`
- **Commit:** 245f602

**2. [Rule 1 - Cleanup] Removed an unused import in the new explain test**
- **Found during:** Task 2
- **Issue:** A draft `use std::io::Write;` left in `explain_filtered_column_byte_equals_run_output` produced an `unused_imports` warning.
- **Fix:** Removed the import and tightened the stale capture-approach comment.
- **Files modified:** `crates/lacon-cli/tests/cli_explain.rs`
- **Commit:** 0ef9d51 (cleaned before commit)

### Audit findings (not deviations, recorded for accuracy)

- **chain_split count correction (cosmetic):** the splitter suite has **19** `#[test]` functions, not the 20 the plan stated. The "20" counted a literal `#[test]` inside the file's module doc-comment (`chain_split.rs:2`). All 13 spec scenarios remain fully covered. Recorded in 06-ACCEPTANCE-MAP.md.
- **No genuine coverage gaps** in primitives/splitter/bundled rules (matches RESEARCH Open Question 3's "few-to-zero gaps" expectation). The three additions are new acceptance proofs, not patches to missing Phase 1-5 coverage.

### TDD note
Task 2 was marked `tdd="true"`, but D-03/D-06 are explicitly prove-not-build proofs of already-shipped Phase 1-5 behavior. Both tests passed on first run against existing code (investigated per the fail-fast rule: this is intentional — there is no new production code to make them go from RED to GREEN). Committed as a single `test(...)` commit. The plan-level MVP+TDD runtime gate was not active (config `tdd_mode: false`, orchestrator did not pass MVP_MODE/TDD_MODE).

## Verification

| Command | Result |
|---------|--------|
| `cargo test --test primitives` | 10 passed |
| `cargo test --test bundled_rules` | walker green (20 fixtures) |
| `cargo test -p lacon-adapter-claudecode --test chain_split` | 19 passed |
| `cargo test -p lacon-cli --test cli_explain` | 6 passed (incl. byte-equality) |
| `cargo test -p lacon-cli --test hot_reload` | 1 passed |
| `cargo test -p lacon-cli --test pnpm_e2e` | 1 passed, 1 ignored (real test) |
| `cargo test --workspace` (no `--ignored`) | all green, 0 failed, never installs pnpm |
| `cargo build --workspace --all-targets` | 0 warnings (excl. benign pre-existing test_emitter lib-target note) |

## Authentication Gates
None.

## Known Stubs
None. The `pnpm_e2e_real` test is `#[ignore]`d by design (opt-in, not a stub); the hermetic variant fully covers the CI-facing acceptance.

## Threat Flags
None. This plan adds tests + a planning doc only, runs hermetically, and the single network-touching path (real `pnpm install`) is `#[ignore]`d out of CI. The threat register dispositions (T-06-01 explain neutralization preserved, T-06-02 XDG sandboxing, T-06-03 `#[ignore]`d real pnpm, T-06-04 cargo_bin anti-spoofing) are all honored.

## Scope Boundary
This plan closes SC4 (test-coverage audit + green confirmation), SC3 (explain byte-equality), SC2 second-half (hot reload), and SC2 first-half (pnpm end-to-end). SC1 (cold-start budget / `tracker_open` steady-state split) and the CI-hermetic sub-claim (`.github/workflows/ci.yml`) are owned by **Plan 02** and were intentionally not touched here.

## Self-Check: PASSED
- Files (all FOUND): 06-ACCEPTANCE-MAP.md, 06-01-SUMMARY.md, cli_explain.rs, hot_reload.rs, pnpm_e2e.rs, Cargo.toml
- Commits (all FOUND): 9c92619 (map), 0ef9d51 (explain+hot-reload), 245f602 (pnpm E2E), a7f9700 (summary)
- Working tree clean; full workspace suite green (no `--ignored`); 0 compiler warnings.
