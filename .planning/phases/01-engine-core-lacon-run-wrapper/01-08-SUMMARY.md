---
phase: 01-engine-core-lacon-run-wrapper
plan: "08"
subsystem: validation
tags: [rust, validation, gap-closure, sc4, regex, starlark, extends, compile-pipeline]

requires:
  - phase: 01-engine-core-lacon-run-wrapper
    plan: "03"
    provides: "compile_resolved, flatten_extends_with_lookup, find_rule_in_dir — the compile machinery reused here"
  - phase: 01-engine-core-lacon-run-wrapper
    plan: "06"
    provides: "lacon validate CLI command (validate_file entry point)"

provides:
  - "validate_rule() wired to flatten_extends_with_lookup + compile_resolved — all four SC4 error categories now caught at the lacon validate boundary"
  - "find_rule_in_dir promoted to pub fn for cross-module reuse"
  - "DEFAULT_MAX_BYTES pub const in loader.rs (32_768) replacing literal"
  - "2 library-level tests in validate_dispatch.rs asserting compile-time rejection"
  - "5 CLI integration tests in cli_validate.rs asserting D-18 error format and exit 1 for all SC4 categories"

affects: [phase-01-verification, gsd-verify-phase-1, sc4, lacon-validate-command]

tech-stack:
  added: []
  patterns:
    - "Gap-closure TDD: RED tests written first (confirm failure), GREEN via minimal wiring, no new logic introduced"
    - "Standalone file validation uses same-directory parent lookup closure for extends resolution without requiring a full RuleLoader instance"
    - "pub const DEFAULT_MAX_BYTES as single source of truth for the 32_768 default"

key-files:
  created: []
  modified:
    - "crates/lacon-core/src/validate/mod.rs — validate_rule() rewritten with full compile chain"
    - "crates/lacon-core/src/rules/loader.rs — find_rule_in_dir pub, DEFAULT_MAX_BYTES pub const"
    - "crates/lacon-core/tests/validate_dispatch.rs — 2 new SC4 library tests"
    - "crates/lacon-cli/tests/cli_validate.rs — 5 new SC4 CLI tests (4 gap-closure + 1 regression)"
    - "crates/lacon-core/src/pipeline/stages.rs — #[allow(dead_code)] on stage_step_str (pre-existing lint)"
    - "bin/test_emitter/src/main.rs — str::repeat() fix for pre-existing clippy lint"

key-decisions:
  - "Strategy B (compile_resolved after flatten) chosen over Strategy A — single call site, reuses pre-built API, adds Starlark script parse as bonus"
  - "find_rule_in_dir promoted to pub fn (1-char change) rather than duplicating directory-walk logic in validate/mod.rs"
  - "validate_rule uses same-directory parent lookup for extends, not a full RuleLoader — correct for ad-hoc path validation outside the three-layer stack"
  - "Pre-existing clippy lints (dead_code in stages.rs, manual_str_repeat in test_emitter) fixed as part of this commit to achieve workspace-clean clippy"

requirements-completed:
  - REQ-cli-validate

duration: 8min
completed: 2026-05-06
---

# Phase 01 Plan 08: Gap Closure — Wire Compile-Time Validation into `lacon validate` Summary

**`lacon validate` now catches all four SC4 error categories (InvalidRegex, MissingScriptFile, CircularExtends, UnknownPrimitive/ParseError) via `flatten_extends_with_lookup + compile_resolved` wired into `validate_rule()`, closing the Phase 1 SC4 gap**

## Performance

- **Duration:** 8 min
- **Started:** 2026-05-06T10:22:15Z
- **Completed:** 2026-05-06T10:30:00Z
- **Tasks:** 3 (TDD Task 1 + CLI tests Task 2 + workspace sweep Task 3, all in one atomic commit)
- **Files modified:** 6

## Accomplishments

- Rewrote `validate_rule()` in `crates/lacon-core/src/validate/mod.rs` to call the full compile chain: `parse_one` → `flatten_extends_with_lookup` (same-directory parent lookup) → `compile_resolved`. The old comment-acknowledged shortcut ("schema correctness is sufficient") is replaced.
- Added `pub fn find_rule_in_dir` and `pub const DEFAULT_MAX_BYTES: usize = 32_768` to `loader.rs` to enable cross-module reuse without duplicating logic.
- Added 2 library-level RED→GREEN tests in `validate_dispatch.rs` confirming `validate_file` returns `InvalidRegex` and `MissingScriptFile`/`ParseError` for the respective fixtures.
- Added 5 CLI integration tests in `cli_validate.rs`: 4 SC4 gap-closure (InvalidRegex, MissingScriptFile, UnknownPrimitive, CircularExtends) + 1 regression guard (valid rule still exits 0).
- Fixed 2 pre-existing clippy lints to achieve workspace-clean clippy: `dead_code` on `stage_step_str` in `stages.rs`, `manual_str_repeat` in `test_emitter`.

## Task Commits

All three tasks are captured in one atomic commit per plan spec (Task 3 committed them all together):

1. **Tasks 1–3: SC4 gap closure + workspace sweep** — `a71cb1c` (fix)

**Plan metadata commit:** (below, docs)

## Files Created/Modified

- `crates/lacon-core/src/validate/mod.rs` — `validate_rule()` fully rewritten with flatten + compile chain; old shortcut comment deleted
- `crates/lacon-core/src/rules/loader.rs` — `find_rule_in_dir` promoted to `pub fn`; `pub const DEFAULT_MAX_BYTES = 32_768` added; literal replaced in `RuleLoader::new`
- `crates/lacon-core/tests/validate_dispatch.rs` — 2 new tests: `validate_file_rejects_invalid_regex`, `validate_file_rejects_missing_script`
- `crates/lacon-cli/tests/cli_validate.rs` — helper `lacon_core_rule_fixture()` added; 5 new tests: `sc4_validate_rejects_invalid_regex`, `sc4_validate_rejects_missing_script`, `sc4_validate_rejects_unknown_primitive`, `sc4_validate_rejects_circular_extends`, `sc4_regression_valid_rule_still_passes`
- `crates/lacon-core/src/pipeline/stages.rs` — `#[allow(dead_code)]` added on `stage_step_str` (pre-existing lint, Rule 3 fix)
- `bin/test_emitter/src/main.rs` — `"a".repeat(chunk)` replacing `std::iter::repeat('a').take(chunk).collect()` (pre-existing lint, Rule 3 fix)

## Decisions Made

- **Strategy B** (`compile_resolved` after `flatten_extends_with_lookup`) chosen over Strategy A (direct `compile_pipeline` call). Rationale: single call site, no code duplication, automatically catches script path issues and Starlark parse errors that compile_pipeline alone would miss.
- **Same-directory parent lookup** for extends resolution: when `lacon validate /some/path/rule.yaml` is called, parents are looked up in `/some/path/` via `find_rule_in_dir`. If the parent isn't there, a `ParseError("could not find parent rule")` is returned — consistent with `RuleLoader::resolve` semantics and safe (no escaping the directory).
- **`dispatch_extend_only_rule_routed_to_rule_validator` test still passes**: after the wiring, an extends-only child rule with no parent on disk produces `ParseError("could not find parent rule")` — not `UnknownKey`. The test only asserts `!misrouted` (i.e. no `UnknownKey`), so this is fine. Verified.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking pre-existing lint] `dead_code` warning in `pipeline/stages.rs`**
- **Found during:** Task 3 (workspace clippy sweep)
- **Issue:** `stage_step_str` helper function marked dead code — pre-existing before this plan, but blocks `cargo clippy --workspace -- -D warnings`
- **Fix:** Added `#[allow(dead_code)]` attribute on the function
- **Files modified:** `crates/lacon-core/src/pipeline/stages.rs`
- **Verification:** `cargo clippy -p lacon-core --all-targets -- -D warnings` exits 0
- **Committed in:** `a71cb1c`

**2. [Rule 3 - Blocking pre-existing lint] `manual_str_repeat` in `bin/test_emitter/src/main.rs`**
- **Found during:** Task 3 (workspace clippy sweep)
- **Issue:** `std::iter::repeat('a').take(chunk).collect::<String>()` flagged as clippy::manual_str_repeat
- **Fix:** Replaced with `"a".repeat(chunk)` — semantically identical
- **Files modified:** `bin/test_emitter/src/main.rs`
- **Verification:** `cargo clippy --workspace --all-targets -- -D warnings` exits 0
- **Committed in:** `a71cb1c`

---

**Total deviations:** 2 auto-fixed (both Rule 3 — blocking pre-existing clippy lints)
**Impact on plan:** Necessary to satisfy the `cargo clippy --workspace -- -D warnings` done criterion. No behavior changes. No scope creep.

## Test Counts Before/After

| Crate | Before | After | Delta |
|-------|--------|-------|-------|
| lacon-core | 118 | 120 | +2 (validate_dispatch.rs SC4 tests) |
| lacon-cli | 17 | 22 | +5 (cli_validate.rs SC4 tests) |
| Other crates | 5 | 5 | 0 |
| **Total** | **140** | **147** | **+7** |

(Baseline was 140 before this plan; 135 in 01-VERIFICATION.md reflects state before PLAN-07 additions.)

## SC4 Gap Status

| Error Category | Before | After |
|----------------|--------|-------|
| InvalidRegex | NOT caught (exit 0) | CAUGHT (exit 1, `<path>:0: InvalidRegex: ...`) |
| MissingScriptFile / inline script | NOT caught (exit 0) | CAUGHT (exit 1, `<path>:0: ParseError: inline script not supported`) |
| UnknownPrimitive / UnknownKey | Partially caught (schema-level only) | CAUGHT (schema + compile pass) |
| CircularExtends | NOT caught (exit 0) | CAUGHT (exit 1, `<path>:0: CircularExtends: ...`) |

**SC4 is now closed. Re-run `/gsd-verify-phase 1` to flip SC4 from FAILED to VERIFIED.**

## Issues Encountered

None beyond the two pre-existing clippy lints.

## Next Phase Readiness

Phase 1 is now complete with all 5 observable truths satisfied. Ready for `/gsd-verify-phase 1` re-verification.

---
*Phase: 01-engine-core-lacon-run-wrapper*
*Plan: 08 (gap closure)*
*Completed: 2026-05-06*
