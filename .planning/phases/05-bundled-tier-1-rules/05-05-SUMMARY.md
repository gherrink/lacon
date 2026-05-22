---
phase: 05-bundled-tier-1-rules
plan: 05
subsystem: testing
tags: [bundled-rules, git-status, collapse_repeated, yaml-rule, fixtures]

# Dependency graph
requires:
  - phase: 05-bundled-tier-1-rules (plan 05-01)
    provides: "bundled_rules.rs fixture-walking runner + meta.yaml exit_code schema"
  - phase: 01-engine
    provides: "collapse_repeated primitive, strip_ansi, keep_regex, drop_regex, RuleLoader, Runner::filter_bytes, rust-embed bundled layer"
provides:
  - "bundled-rules/git-status.yaml — git status rule using collapse_repeated on the tab-indented file block"
  - "tests/fixtures/git-status/{many-untracked,not-a-repo} — success + failure fixtures with exit_code in meta.yaml"
affects: [05-09-verification, phase-06-ship-gate]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "collapse_repeated as the primary reducer (the only Tier 1 rule that uses it this way)"
    - "summary YAML key (NOT summary_template) for collapse_repeated args"
    - "on_error keep_regex for the small not-a-repo failure path; failure fixture exempt from reduction check"

key-files:
  created:
    - bundled-rules/git-status.yaml
    - tests/fixtures/git-status/many-untracked/input.txt
    - tests/fixtures/git-status/many-untracked/expected.txt
    - tests/fixtures/git-status/many-untracked/meta.yaml
    - tests/fixtures/git-status/not-a-repo/input.txt
    - tests/fixtures/git-status/not-a-repo/expected.txt
    - tests/fixtures/git-status/not-a-repo/meta.yaml
  modified:
    - .planning/phases/05-bundled-tier-1-rules/deferred-items.md

key-decisions:
  - "Used collapse_repeated on '^\\t' (max_kept 5) to collapse the untracked-file block; section headers are not tab-indented so they survive untouched."
  - "Dropped the parenthetical git hint lines via drop_regex '^\\s*\\(use ' — pure noise."
  - "Failure fixture is exit 128 (git status outside a repo); on_error keeps the fatal: line and the fixture is reduction-exempt."

patterns-established:
  - "collapse_repeated primary-reducer pattern: strip_ansi -> drop hint -> collapse_repeated on the bulk line class -> (auto max_bytes)."

requirements-completed: [REQ-bundled-rules-tier1, REQ-bundled-rules-format]

# Metrics
duration: 8min
completed: 2026-05-22
---

# Phase 5 Plan 5: git-status rule Summary

**git status rule that collapses the long tab-indented untracked-file block via collapse_repeated (~90% reduction on -uall) while preserving section headers and the fatal: not-a-repo error path.**

## Performance

- **Duration:** ~8 min
- **Started:** 2026-05-22T07:05Z (worktree spawn)
- **Completed:** 2026-05-22T07:11Z
- **Tasks:** 2
- **Files modified:** 7 created + 1 doc note

## Accomplishments
- Authored `bundled-rules/git-status.yaml`: matches `git status` (any args) and uses `collapse_repeated` on `^\t` as the primary reducer — the only Tier 1 rule built around collapse_repeated.
- Captured two REAL git fixtures (git 2.53.0): a chatty `git status -uall` with 123 untracked files (exit 0) and `git status` outside a repo (exit 128).
- Verified ~90% byte reduction on the success fixture (2399 → 235 bytes, ratio 0.098) and that the `fatal: not a git repository` line survives the on_error path.
- `cargo test --test bundled_rules` is green (asserts 2 git-status fixtures).

## Task Commits

Each task was committed atomically:

1. **Task 1: Author git-status.yaml (collapse_repeated on the file block)** - `e58f756` (feat)
2. **Task 2: git-status fixtures (many-untracked + not-a-repo)** - `3c286b3` (test)

**Tracking note:** `cf80dad` (docs: note pre-existing cli_doctor failure)

## Files Created/Modified
- `bundled-rules/git-status.yaml` - git status rule: strip_ansi → drop hint lines → collapse_repeated on `^\t` (max_kept 5, summary key); on_error strip_ansi → keep_regex `^fatal:`. No max_bytes (auto-injected, D-07); no look-around (RE2).
- `tests/fixtures/git-status/many-untracked/{input,expected,meta}` - real `git status -uall`, 123 untracked files, exit 0, primary success (reduction-checked).
- `tests/fixtures/git-status/not-a-repo/{input,expected,meta}` - real `git status` outside a repo, exit 128, on_error path, `must_keep_lines: ["fatal: not a git repository"]`, reduction-exempt.
- `.planning/phases/05-bundled-tier-1-rules/deferred-items.md` - appended note about the pre-existing cli_doctor failure.

## Decisions Made
- **collapse_repeated, not drop_regex, as the reducer.** The untracked-file block is the bulk; collapsing `^\t` lines into 5 examples + a summary keeps a representative sample while crushing the volume. Section headers (`On branch`, `Untracked files:`) are not tab-indented, so they pass through untouched.
- **`summary` YAML key, not `summary_template`** — confirmed against `CollapseArgs` (schema.rs:244) and PATTERNS delta #1. The runtime field is named `summary_template` but the YAML key is `summary`.
- **Failure fixture = not-a-repo (exit 128).** git status rarely fails; the only nonzero path is running outside a repo. on_error keeps the `fatal:` line; the output is tiny so the fixture is `exempt_from_reduction_check: true`.

## Deviations from Plan

None - plan executed exactly as written. The success pipeline includes the optional `drop_regex: '^\s*\(use '` hint-line drop the plan explicitly permitted; collapse_repeated handles the bulk reduction.

## Issues Encountered

- **Full-workspace `cargo test` has one pre-existing red:** `cli_doctor::doctor_all_green_passes_and_exits_zero` panics with `CARGO_BIN_EXE_test_emitter is unset`. Root cause is the missing `bin/test_emitter` crate referenced at `crates/lacon-cli/Cargo.toml:27` (introduced in Phase 4, commit 690409b), not the git-status rule. This is OUT OF SCOPE per the scope boundary (pre-existing failure in an unrelated file). Logged to `deferred-items.md`. This plan's deliverable test — `cargo test --test bundled_rules` — is green, as is `lacon validate bundled-rules/git-status.yaml`.

## Known Stubs

None.

## Next Phase Readiness
- git-status rule and fixtures complete and green under `cargo test --test bundled_rules`.
- Pre-existing `bin/test_emitter` infrastructure gap blocks a clean full-workspace `cargo test` (tracked in deferred-items.md) — independent of this plan, should be addressed before the Phase 6 ship gate.

## Self-Check: PASSED

All 7 created files exist on disk; both task commits (`e58f756`, `3c286b3`) are present in git history.

---
*Phase: 05-bundled-tier-1-rules*
*Completed: 2026-05-22*
