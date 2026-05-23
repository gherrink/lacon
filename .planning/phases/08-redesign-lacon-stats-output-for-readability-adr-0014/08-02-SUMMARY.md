---
phase: 08-redesign-lacon-stats-output-for-readability-adr-0014
plan: 02
subsystem: lacon-cli stats presentation helpers
tags: [stats, presentation, humanize, git-resolution, ephemeral, tdd]
requires:
  - "08-01 (OverallTotals headline reader — sibling, not a hard dep for these helpers)"
provides:
  - "humanize_bytes(i64) -> String (decimal-SI byte humanizer, D-13)"
  - "ephemeral_prefixes() + is_ephemeral(&str) -> bool (component-wise temp-root detection, D-08)"
  - "resolve_repo_root(&Path) -> Option<String> (.git dir/worktree/submodule resolution, D-09/D-10)"
  - "canonical_project_key(&str) -> String (precedence resolver ephemeral->repo-root->literal, D-07)"
affects:
  - "08-03 (wires these helpers into execute's output body + project rollup)"
tech-stack:
  added: []
  patterns:
    - "private fn + inline #[cfg(test)] mod tests (D-04, one-module-per-command)"
    - "literal-path fallback on every IO/parse error (no panic, no canonicalize)"
    - "bounded fs::metadata/read_to_string for .git resolution (no git subprocess)"
key-files:
  created: []
  modified:
    - "crates/lacon-cli/src/commands/stats.rs (+398 lines: 5 helpers + 8 new inline tests)"
decisions:
  - "D-04: helpers are private fns inside stats.rs, NOT a shared util module or lacon-core"
  - "D-07: canonical-key precedence ephemeral -> repo-root -> literal"
  - "D-08: ephemeral detection via component-wise Path::starts_with (NOT str::starts_with), match the stored string, no canonicalize"
  - "D-09: .git resolution via bounded reads — single gitdir hop + single commondir hop, no git subprocess"
  - "D-10: literal fallback on bare repo / any IO error / deleted dir; never panics"
  - "D-13: decimal-SI humanize_bytes (1000-based, 1 decimal above 1 KB, raw int below)"
metrics:
  duration: "~8 min"
  completed: "2026-05-23"
  tasks: 2
  files: 1
---

# Phase 8 Plan 02: stats read-time presentation helpers Summary

Added four pure, fully-unit-tested private helpers to `commands/stats.rs` — a
decimal-SI byte humanizer (`humanize_bytes`), ephemeral temp-root detection
(`ephemeral_prefixes`/`is_ephemeral`), bounded `.git` repo-root resolution
(`resolve_repo_root`), and the precedence resolver (`canonical_project_key`) — so
plan 08-03 can wire them into the stats output without re-deriving the contracts.
No `execute` body/signature change, no SQL, no flags.

## What was built

Two TDD tasks, each RED → GREEN (no refactor needed):

**Task 1 — `humanize_bytes` + ephemeral detection.**
- `humanize_bytes(i64) -> String`: decimal SI (1000-based, NOT binary) per D-13 /
  ADR §4. Below 1 KB → raw integer with `B` suffix; at/above 1 KB → divide by
  1000 walking `[KB,MB,GB,TB,PB]`, format with one decimal. Negative inputs are
  handled defensively (sign-prefixed) though never expected (all stored byte
  counts ≥ 0).
- `ephemeral_prefixes() -> Vec<PathBuf>`: runtime prefix set — `/tmp`,
  `/var/folders`, `/private/var/folders`, `/dev/shm` (Linux tmpfs, Claude's
  discretion), `std::env::temp_dir()`, and `$TMPDIR` when set. Deliberately omits
  `/var/tmp` (persistent, not boot-ephemeral).
- `is_ephemeral(&str) -> bool`: component-wise `Path::starts_with` against the
  prefix set, matching the STORED string with no `canonicalize`.

**Task 2 — `.git` resolution + canonical key.**
- `resolve_repo_root(&Path) -> Option<String>`: walks `path.ancestors()`; a `.git`
  **directory** → that ancestor (None if `core.bare = true`); a `.git` **file**
  (gitlink) → strip `gitdir:` + trim, resolve relative values lexically against
  the gitfile's own dir (submodules write relative; worktrees write absolute),
  read `<gitdir>/commondir` (one hop) to locate the main `.git`, return its
  parent. Bounded: one gitdir hop + one commondir hop, `fs::metadata`/
  `read_to_string` only — no git subprocess, no `canonicalize`. Any IO error /
  missing / malformed / bare → `None` (caller falls back to literal).
- `canonical_project_key(&str) -> String`: precedence (a) ephemeral → `(ephemeral)`,
  (b) repo root via `resolve_repo_root`, (c) literal stored string.

All four helpers carry `#[allow(dead_code)]` with a forward-reference comment:
they are exercised now only by the inline tests; plan 08-03 wires the call sites
in `execute`. This keeps the non-test bin build and `cargo clippy` clean while
preserving the additive-only diff for this plan.

## Tests

17 inline `#[cfg(test)] mod tests` cases in `stats.rs` (9 pre-existing + 8 new):
- `humanize_bytes_decimal_si_boundaries` — six points: 0→"0 B", 999→"999 B",
  1000→"1.0 KB", 1024→"1.0 KB" (proves decimal-SI), 22_800→"22.8 KB" (ADR
  literal), 1_000_000→"1.0 MB".
- `is_ephemeral_matches_temp_roots_but_not_tmpfoo` — `/tmp/x` true, **`/tmpfoo/x`
  FALSE** (the mandatory Path-vs-str regression guard), a `temp_dir()`-rooted path
  true.
- `resolve_repo_root_git_directory_rollup` — repo + subdir both → repo.
- `resolve_repo_root_worktree_absolute_gitdir` — `gitdir:` absolute + commondir
  `../..` → repo.
- `resolve_repo_root_submodule_relative_gitdir` — relative `gitdir:` resolves
  against the gitfile dir → superproject root.
- Three literal-fallback branches: `resolve_repo_root_no_git_returns_none`,
  `resolve_repo_root_bare_repo_returns_none` (core.bare=true),
  `resolve_repo_root_nonexistent_path_returns_none` — each `None`, no panic.
- `canonical_project_key_ephemeral_beats_git` — an ephemeral-rooted repo still
  keys `(ephemeral)`.
- `canonical_project_key_literal_fallback` — non-ephemeral unresolvable path →
  verbatim stored string.

All `.git` fixtures are built with `std::fs` under `tempfile::tempdir()` — no
`git` binary needed and the production code never shells out.

**Verification (full suite green):**
- `cargo test -p lacon-cli` → 35 bin tests + all integration tests pass.
- `cargo test --workspace` → 44 test-result groups, zero failures.
- `cargo build --workspace` clean (no dead-code warnings; helpers `#[allow(dead_code)]`).

## Acceptance criteria

| Criterion | Result |
|-----------|--------|
| `humanize` six-point boundary test | PASS |
| `is_ephemeral` incl. `/tmpfoo` negative | PASS |
| `resolve_repo_root` dir/worktree/submodule rollup | PASS (3) |
| three literal-fallback branches, no panic | PASS (3) |
| `canonical_project_key` ephemeral precedence | PASS |
| zero `canonicalize()` CALLS in stats.rs | PASS (`grep -c 'canonicalize(' == 0`) |
| zero git subprocess in stats.rs | PASS (`grep -c 'Command::new("git")\|process::Command' == 0`) |
| write-path unreachability (no refs from run.rs / lacon-core) | PASS (grep returns nothing) |
| component-wise `Path::starts_with` (no `str::starts_with` in code) | PASS |
| no change to `execute` signature/output body | PASS (signature byte-identical to base) |

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `--lib` test target does not exist on a binary crate**
- **Found during:** Task 1 RED verification.
- **Issue:** The plan's `<verify>` blocks use `cargo test -p lacon-cli --lib …`,
  but `lacon-cli` is a binary crate with no library target, so `--lib` errors
  ("no library targets found"). This blocked running the inline unit tests.
- **Fix:** Used `cargo test -p lacon-cli --bins …` (the bin target carries the
  `#[cfg(test)]` inline tests). Functionally identical filter, correct target.
- **Files modified:** none (invocation-only).
- **Commit:** n/a (test-running mechanics).

**2. [Rule 3 - Blocking] dead-code / clippy warnings on intentionally-deferred helpers**
- **Found during:** Task 1 GREEN (and confirmed for Task 2).
- **Issue:** The helpers are added now but wired by 08-03, so the non-test bin
  build emitted `function ... is never used` warnings (and clippy would flag them
  at the phase gate).
- **Fix:** Added `#[allow(dead_code)]` to each helper with a comment naming 08-03
  as the wiring plan. Keeps the build/clippy clean; the attribute is removable by
  08-03 once call sites exist.
- **Files modified:** `crates/lacon-cli/src/commands/stats.rs`.
- **Commit:** `96a8585` (Task 1), `ece56af` (Task 2).

**3. [Rule 3 - Blocking] rustfmt drift introduced by the new code**
- **Found during:** final verification (`cargo fmt --check`).
- **Issue:** A few new lines (the `is_ephemeral` iterator chain and two test
  `fs::write` calls) were not in rustfmt's preferred form.
- **Fix:** Hand-formatted the new code to rustfmt's form (committed as a `style`
  commit). NOT touched: two PRE-EXISTING fmt diffs in `stats.rs` (the long
  `execute` signature and the `normalize_project_strips_trailing_separator` test
  `format!`) which are byte-identical to base commit `ddc4dde` — out of scope per
  the executor SCOPE BOUNDARY rule.
- **Files modified:** `crates/lacon-cli/src/commands/stats.rs`.
- **Commit:** `70438b5`.

### Acceptance-criterion nuance (no code impact)

The plan's literal `grep -c 'canonicalize' crates/lacon-cli/src/commands/stats.rs ==
0` was unachievable from the start: the pre-existing `normalize_project` doc
comment at `stats.rs:57` already contained the word "canonicalize" before this
plan. The load-bearing intent — **no `canonicalize()` CALL** in the canonical-key
path — is satisfied: `grep -c 'canonicalize(' == 0`. The two textual matches that
remain are explanatory comments stating the deliberate absence of canonicalization
(line 57 pre-existing; one new doc line in `is_ephemeral`).

## Out-of-scope discoveries (deferred, not fixed)

Logged to
`.planning/phases/08-redesign-lacon-stats-output-for-readability-adr-0014/deferred-items.md`:
pre-existing rustfmt drift (`benches/cold_start.rs`,
`crates/lacon-adapter-claudecode/src/lib.rs`, `tests/cli_stats.rs`, two lines in
`stats.rs`) and pre-existing clippy warnings in `lacon-core`/`tracking_e2e`. None
touch the 08-02 helper code. Note CI does not gate on `cargo fmt --check`.

## Known Stubs

None. The helpers are complete, pure, and fully unit-tested. They are intentionally
not yet *called* by `execute` (plan 08-03 owns the output restructure that wires
them) — this is the planned plan boundary, documented via `#[allow(dead_code)]` and
the helper doc comments, not a stub.

## TDD Gate Compliance

Both tasks followed RED → GREEN with distinct commits:
- Task 1: `test(08-02): add failing tests …` (`06ef1e2`) → `feat(08-02): add
  humanize_bytes + ephemeral …` (`96a8585`).
- Task 2: `test(08-02): add failing tests for resolve_repo_root …` (`fb0a86d`) →
  `feat(08-02): add resolve_repo_root + canonical_project_key …` (`ece56af`).
No REFACTOR commits needed; one trailing `style` commit (`70438b5`) for fmt.

## Self-Check: PASSED

All created/modified files exist on disk; all five 08-02 commits
(`06ef1e2`, `96a8585`, `fb0a86d`, `ece56af`, `70438b5`) are present in git history.
