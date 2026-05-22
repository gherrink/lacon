---
phase: 05-bundled-tier-1-rules
plan: 04
subsystem: testing
tags: [bundled-rules, cargo, yaml, drop_regex, keep_around_match, on_error, fixtures, regex]

# Dependency graph
requires:
  - phase: 05-bundled-tier-1-rules (plan 05-01)
    provides: "bundled_rules.rs fixture-walking runner + tests/fixtures tree convention + meta.yaml schema (exit_code, must_keep_lines, exempt_from_reduction_check)"
provides:
  - "bundled-rules/cargo-build.yaml — first real bundled rule (cargo build / cargo check)"
  - "Blacklist-drop success pipeline pattern that preserves variable-shape warning blocks"
  - "Context-preserving on_error pattern (keep_around_match on ^error + keep_tail) for cargo diagnostics"
  - "Two real-capture cargo fixtures (multi-dep success exit 0, compile-error exit 101)"
affects: [05-05, 05-06, 05-07, 05-08, 05-09, 05-10, cargo-test rule (shares cargo status-line drops)]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Blacklist drop_regex success pipeline (drop known noise, let signal pass) over keep_regex whitelist when the kept block has variable shape"
    - "on_error keep_around_match on '^error' captures both error[E0xxx]: and error: forms plus --> / | context"
    - "Fixture path anonymization (/tmp/<rand>/demo -> /tmp/demo) for deterministic byte-exact expected.txt"

key-files:
  created:
    - "bundled-rules/cargo-build.yaml"
    - "tests/fixtures/cargo-build/multi-dep-warning/{input,expected,meta}.{txt,yaml}"
    - "tests/fixtures/cargo-build/compile-error/{input,expected,meta}.{txt,yaml}"
  modified:
    - "crates/lacon-core/src/rules/bundled.rs"

key-decisions:
  - "Blacklist drop_regex (not keep_regex) for the success path — warning diagnostic blocks (warning:, -->, |, = note:) have variable shape and survive untouched when only Compiling/Updating/Locking/Finished are dropped (D-08, RESEARCH recommendation)"
  - "Captured 8 dependencies (9 Compiling lines) so the headline status-line drop clears the >=50% floor (ratio 0.469) even though the preserved warning block is large (Pitfall 5)"
  - "Success fixture keeps Updating/Locking lines (real first-build cargo output) since the rule drops them too — exercises all four drop_regex stages"
  - "compile-error fixture marked exempt_from_reduction_check: true — the error IS the signal; asserts survival via must_keep_lines instead of the 50% floor"

patterns-established:
  - "Pattern: blacklist-drop success pipeline preserving variable-shape diagnostic blocks"
  - "Pattern: real-capture-then-replay fixtures (input.txt = real cargo output, expected.txt = lacon run pipeline replay, never hand-authored)"

requirements-completed: [REQ-bundled-rules-tier1, REQ-bundled-rules-format]

# Metrics
duration: 22min
completed: 2026-05-22
---

# Phase 5 Plan 04: cargo-build Rule Summary

**cargo-build rule (cargo build / cargo check) that blacklist-drops Compiling/Updating/Locking/Finished status lines for 53% reduction while preserving warning blocks with file:line context, plus an on_error path that keeps error[E0xxx]/error: diagnostics intact.**

## Performance

- **Duration:** ~22 min
- **Started:** 2026-05-22 (worktree agent-adec8ce907b8a58b2)
- **Completed:** 2026-05-22
- **Tasks:** 2
- **Files modified/created:** 8 (1 rule, 6 fixture files, 1 deviation fix)

## Accomplishments
- Authored `bundled-rules/cargo-build.yaml` — the first real bundled Tier 1 rule. `match.any` covers both `cargo build` and `cargo check` (D-10).
- Success pipeline: `strip_ansi` then `drop_regex` on `^\s*Compiling`/`^\s*Updating`/`^\s*Locking`/`^\s*Finished`. Blacklist approach lets the variable-shape warning diagnostic block pass through unmodified.
- `on_error` pipeline: `strip_ansi` -> `keep_around_match {pattern: '^error', before: 0, after: 15}` -> `keep_tail {lines: 40}`. The `^error` anchor captures both `error[E0308]:` and `error:` plus the `-->`/`|` context.
- Two real-capture fixtures from cargo 1.95.0: a multi-dep (8 deps) success build with one unused-variable warning (exit 0, reduction ratio 0.469), and an E0308 type-mismatch compile error (exit 101, routes through on_error).
- No hand-placed `max_bytes` (D-07 auto-injects 32768); no `script:` in pipeline; RE2 regex with no look-around.

## Task Commits

1. **Task 1: Author cargo-build.yaml** - `1d907e3` (feat)
2. **Task 2: cargo-build fixtures (multi-dep success + compile-error)** - `58b4fa0` (test, includes Rule 1 deviation fix)

## Files Created/Modified
- `bundled-rules/cargo-build.yaml` - The cargo build/check rule: blacklist-drop success pipeline + context-preserving on_error.
- `tests/fixtures/cargo-build/multi-dep-warning/input.txt` - Real cargo build output, 8 deps + 1 warning, path anonymized to /tmp/demo.
- `tests/fixtures/cargo-build/multi-dep-warning/expected.txt` - Pipeline replay output (warning block preserved, status lines dropped).
- `tests/fixtures/cargo-build/multi-dep-warning/meta.yaml` - exit_code 0, must_keep_lines ["warning:"], not exempt.
- `tests/fixtures/cargo-build/compile-error/input.txt` - Real E0308 compile error, path anonymized.
- `tests/fixtures/cargo-build/compile-error/expected.txt` - on_error replay (error[E0308]/error: + --> context preserved).
- `tests/fixtures/cargo-build/compile-error/meta.yaml` - exit_code 101, must_keep_lines ["error[E0308]", "mismatched types"], reduction-exempt.
- `crates/lacon-core/src/rules/bundled.rs` - Updated the Phase-1 empty-dir placeholder test to its durable invariant (see Deviations).

## Decisions Made
- **Blacklist over whitelist for success:** dropping only the four status-line families and letting everything else through keeps warning blocks intact regardless of their internal shape (multiple `|`, `= note:`, `= help:` lines vary). A `keep_regex` whitelist would have to enumerate every diagnostic line form.
- **8 deps to clear the 50% floor:** the preserved warning block is ~330 bytes of signal that cannot be dropped, so the input must carry enough droppable Compiling/Updating/Locking/Finished noise. With 9 Compiling lines + 3 status lines the ratio lands at 0.469.
- **compile-error reduction-exempt:** failure output is small and the error IS the signal; survival is asserted via `must_keep_lines` rather than the byte-ratio floor (D-05).

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Updated stale Phase-1 bundled-dir placeholder test**
- **Found during:** Task 2 (running `cargo test` workspace-wide for self-verification)
- **Issue:** `crates/lacon-core/src/rules/bundled.rs` contained `bundled_iter_does_not_panic_on_empty_dir`, which asserted `iter_bundled().count() == 0`. That assertion was a Phase-1 placeholder (its own comment said "Phase 5 will add real rules; this test just ensures no panic"). Adding the first real bundled rule (`cargo-build.yaml`) made the count 1, so the test failed — directly invalidated by this plan's legitimate work.
- **Fix:** Renamed to `bundled_iter_does_not_panic_and_filters_non_yaml` and replaced the empty-count assertion with the durable invariant the test was actually protecting: `iter_bundled()` yields only `.yaml` files and filters out `.gitkeep`.
- **Files modified:** crates/lacon-core/src/rules/bundled.rs
- **Verification:** `cargo test` full workspace green (0 failures); the new assertions pass with `cargo-build.yaml` present.
- **Committed in:** 58b4fa0 (part of Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug — stale placeholder test)
**Impact on plan:** Necessary to keep the workspace test suite green. The fix preserves the test's real intent (no panic + `.gitkeep` filtered) rather than weakening it. No scope creep — confined to the one test directly broken by adding the first bundled rule.

## Issues Encountered
- **50% floor on first capture:** the initial 4-dep capture produced ratio 0.545 (Pitfall 5) because the preserved warning block dominates. Resolved by re-capturing a real build with 8 deps (9 Compiling lines), bringing the ratio to 0.469.
- **One-time `cargo new` `note:` line:** the first build emitted a manifest-keys `note:` line; a clean rebuild on a settled lockfile produced the representative output without it. The fixture uses the clean form.

## Threat Surface
- Per the plan's `<threat_model>`, T-5-03 (info disclosure via dropping the diagnostic) is mitigated: `must_keep_lines` asserts `warning:` survives the success reduction and `error[E0308]`/`mismatched types` survive the on_error path — both verified green by `cargo test --test bundled_rules`.
- T-5-SC (cargo install tampering): the throwaway capture crate lived in `/tmp` and was deleted after capture; its scratch dependencies never entered this repo's Cargo.toml.
- No new threat surface beyond the plan's register: this plan adds only declarative YAML and static fixture text.

## Verification
- `lacon validate bundled-rules/cargo-build.yaml` -> exit 0.
- `cargo test --test bundled_rules` -> 1 test, asserted 2 cargo-build fixtures, passed.
- `cargo build` (workspace) -> green.
- `cargo test` (workspace) -> 0 failures.
- Reduction ratio (multi-dep-warning): 369/787 = 0.469 (<= 0.5).
- No `max_bytes` literal, no look-around (`(?[=!<]`) in the rule.

## Next Phase Readiness
- cargo-build establishes the blacklist-drop + context-preserving on_error pattern that the remaining Tier 1 rules (and cargo-test, which shares the Compiling/Finished status-line drops) can follow.
- The bundled-dir test is now Phase-5-ready: adding more `<id>.yaml` files will not re-break it.

## Self-Check: PASSED

All 8 created files verified present on disk; both task commits (`1d907e3`, `58b4fa0`) verified in git log.

---
*Phase: 05-bundled-tier-1-rules*
*Completed: 2026-05-22*
