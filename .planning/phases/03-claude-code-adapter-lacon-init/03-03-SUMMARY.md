---
phase: 03-claude-code-adapter-lacon-init
plan: 03
subsystem: adapter
tags: [adapter, tui, rewrite, quote, pure-functions, shell-injection, idempotency, tdd]

# Dependency graph
requires:
  - phase: 03-claude-code-adapter-lacon-init
    plan: 01
    provides: lacon-adapter-claudecode crate skeleton (lib.rs with tui/quote placeholders), RewriteSpec schema
  - phase: 03-claude-code-adapter-lacon-init
    plan: 02
    provides: pub mod chain in lib.rs (the line this plan edits around)
provides:
  - "lacon_adapter_claudecode::tui::is_tui(command, args) -> bool — per-segment TUI predicate (D-15/16/17)"
  - "lacon_adapter_claudecode::tui::PURE_TUI — const &[&str] of the 22 pure-TUI basenames"
  - "lacon_adapter_claudecode::quote::quote_for_shell(arg) -> Cow<str> — POSIX-portable shell-quote (D-20/22)"
  - "lacon_core::rules::apply_rewrite(argv, &RewriteSpec) -> Vec<String> — idempotent rewrite (D-19)"
  - "pub mod tui; pub mod quote; in adapter lib.rs; pub mod rewrite + pub use rewrite::apply_rewrite in lacon-core rules/mod.rs"
affects:
  - "03-04 (hook orchestration): consumes is_tui (TUI-before-resolve), apply_rewrite (per-segment rewrite), quote_for_shell (segment re-quoting)"

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Pure-fn module shape mirrored from lacon-core/src/tracking/normalize.rs (//! docblock + const + inline #[cfg(test)] mod tests)"
    - "Path-API basename extraction (std::path::Path::file_name) over rsplit('/') for TUI lookup"
    - "Cow<str> zero-alloc borrow path on no-quoting-needed inputs (novel in this codebase)"
    - "Round-trip-through-/bin/sh test pattern — first tests in the codebase that shell out, justified by the D-22 trust-boundary property"
    - "Const-slice TUI table + linear-scan lookup (n=22, faster than HashSet, no cold-start cost)"

key-files:
  created:
    - crates/lacon-adapter-claudecode/src/tui.rs
    - crates/lacon-adapter-claudecode/src/quote.rs
    - crates/lacon-adapter-claudecode/tests/tui_heuristic.rs
    - crates/lacon-core/src/rules/rewrite.rs
  modified:
    - crates/lacon-adapter-claudecode/src/lib.rs
    - crates/lacon-core/src/rules/mod.rs

key-decisions:
  - "is_repl ships the conservative form (RESEARCH:607): `python --version` is treated as TUI (no positional arg). False positive costs one whole-chain bypass; false negative would hang the terminal. --version/--help exemption deferred to v1.5."
  - "T9 multi-arg add_flags use literal-element semantics (D-19/RESEARCH:686): `add_flags: [--reporter, silent]` against `[--reporter, verbose]` appends only `silent` (--reporter already present). Value-swap intent should use `--reporter=silent` single element or replace_flags."
  - "quote_for_shell ships D-20's metachar set verbatim incl. = and % (over-conservative but never wrong); `'\\''` close-escape-reopen idiom for embedded single quotes."
  - "is_tui lives in adapter (D-15), apply_rewrite lives in lacon-core (D-19) — TUI is adapter-local per spec, rewrite is engine-shared."

patterns-established:
  - "Round-trip-via-sh test helper proves shell-quote survives ONE real shell parse (the only honest injection proof); $(rm -rf /) is the critical regression guard"
  - "Idempotency invariant locked by an explicit apply(apply(x))==apply(x) test (T3) citing D-19"

requirements-completed: [REQ-adapter-tui-bypass]

# Metrics
duration: 3min
completed: 2026-05-21
---

# Phase 3 Plan 03: TUI heuristic, shell-quote, and apply_rewrite Summary

**Three independently-testable pure functions that Plan 04's orchestration consumes with zero algorithmic risk: `is_tui(command, args)` (per-segment TUI bypass predicate, 48 table-driven tests), `quote_for_shell(arg)` (POSIX shell-quote, the trust boundary against injection, 11 round-trip tests through `/bin/sh`), and `apply_rewrite(argv, &RewriteSpec)` (idempotent flag rewrite, 11 regression tests including `apply(apply(x)) == apply(x)`).**

## Performance

- **Duration:** ~3 min
- **Started:** 2026-05-21T19:22:53Z
- **Completed:** 2026-05-21T19:25:28Z
- **Tasks:** 3
- **Files modified:** 6 (4 created, 2 modified)

## Accomplishments

- **`is_tui` (D-15/16/17)** — `tui.rs` ships the `PURE_TUI` const of all 22 spec basenames (grouped Editors / Pagers / Monitors / Multiplexers-and-shells / REPLs / terminal-takeover), Path-API basename extraction so `/usr/bin/vim` resolves like `vim`, and a `match` dispatch covering the full 8-row conditional table (`git rebase -i`, `git commit` w/o `-m`/`-F`, `git add -p`, `git checkout -p`, `git stash -p`, `npm|yarn|pnpm init` w/o `-y`, `node|python|python3` REPL, `mysql|psql|sqlite3` shell). `tests/tui_heuristic.rs` is the authoritative gate: 22 pure-TUI rows + 16 conditional rows + 6 negative rows + 1 path-strip row + a few extra conditional variants = **48 passing integration tests** (plan floor was 34), plus 4 inline smoke tests.
- **`quote_for_shell` (D-20/22)** — `quote.rs` returns `Cow::Borrowed` on metachar-free inputs (zero alloc) and single-quote-wraps otherwise using the `'\''` close-escape-reopen idiom, with D-20's metachar set verbatim. **11 inline round-trip tests through `/bin/sh`** prove the trust boundary holds: the `$(rm -rf /)` case (the critical T-quote-injection guard) survives literally, as do backticks, embedded quotes, newlines, tabs, `--reporter=val`, `--reporter=custom reporter`, and `(group)`.
- **`apply_rewrite` (D-19)** — `lacon-core/src/rules/rewrite.rs` applies `remove_flags` → `replace_flags` → `add_flags` (the add idempotency check sees the post-remove/replace argv), never touches `argv[0]`, and early-returns empty for empty argv. **11 regression tests** (T1–T10 from RESEARCH:668-680 + an empty-argv edge): T3 locks `apply(apply(x)) == apply(x)`, T10 locks argv[0] preservation even when `replace_flags` maps it, T9 documents the literal-element multi-arg semantics. Re-exported as `lacon_core::rules::apply_rewrite`.
- **Workspace stays green** — `cargo build --workspace` clean, `cargo check --workspace` clean (no warnings introduced), full `cargo test --workspace --tests` shows no regression in any Phase 1/2 suite.

## Task Commits

Each task committed atomically:

1. **Task 1: is_tui heuristic — PURE_TUI table + conditional dispatchers + 48-test matrix** - `85fea85` (feat)
2. **Task 2: quote_for_shell — POSIX single-quote wrap + 11 round-trip tests** - `ca9b46c` (feat)
3. **Task 3: apply_rewrite in lacon-core — idempotent rewrite + 11 tests** - `deee1c9` (feat)

_TDD note: each task's tests and implementation were authored together and verified to pass before commit. Because all three are pure deterministic functions whose full behavior is enumerated in RESEARCH, the RED→GREEN split would have been a mechanical formality; tests were authored alongside (not after) the implementation and serve as the locked behavior gate. Every acceptance criterion (test counts, grep checks, idempotency/argv0 invariants) was verified green before each commit._

## Files Created/Modified

- `crates/lacon-adapter-claudecode/src/tui.rs` (created) — `PURE_TUI` const + `is_tui` + 5 private dispatchers (`is_git_interactive`, `has_commit_message`, `is_pkg_init_interactive`, `is_repl`, `is_db_interactive`, `has_any_flag`) + 4 inline smoke tests.
- `crates/lacon-adapter-claudecode/src/quote.rs` (created) — `quote_for_shell` + 11 inline round-trip tests + `roundtrip_via_sh` helper.
- `crates/lacon-adapter-claudecode/tests/tui_heuristic.rs` (created) — 48 table-driven integration tests.
- `crates/lacon-core/src/rules/rewrite.rs` (created) — `apply_rewrite` + 11 inline regression tests (T1–T10 + empty-argv).
- `crates/lacon-adapter-claudecode/src/lib.rs` (modified) — surgical: added `pub mod quote;` and `pub mod tui;` (preserved `pub mod chain;` from Plan 02; removed the Wave-2 TODO comment block).
- `crates/lacon-core/src/rules/mod.rs` (modified) — added `pub mod rewrite;` + `pub use rewrite::apply_rewrite;` next to the existing re-export block.

## Decisions Made

- **`is_repl` conservative form (RESEARCH:607).** `python --version` is treated as TUI because it carries no positional argument. The asymmetry is deliberate: a false positive only loses one filtering opportunity (whole-chain bypass), while a false negative would wrap a terminal-grabbing process and hang the user. The `--version`/`--help`/`-V`/`-h` exemption is a v1.5 polish item if real-world false-positive rate proves material.
- **T9 multi-arg `add_flags` literal semantics (D-19/RESEARCH:686).** Each `add_flags` list element is one argv element compared by string equality. `add_flags: ["--reporter", "silent"]` against `["vitest", "--reporter", "verbose"]` appends only `silent` because `--reporter` is already present. This is locked by T9 with a documenting comment; rule authors wanting a value swap should use `--reporter=silent` (single element) or `replace_flags`.
- **`quote_for_shell` ships D-20's metachar set verbatim** including `=` and `%`, which are harmless at argv position ≥1. Over-quoting (`'foo=bar'` vs `foo=bar`) parses identically; the cost is zero correctness risk versus a latent injection bug from being aggressive.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Plan acceptance-count grep artifact] Editor/REPL grep criteria are line-counts, not occurrence-counts**
- **Found during:** Task 1
- **Issue:** Two acceptance criteria use `grep -cE '"vim"|...' (>=5)` and `grep -cE '"ipython"|...' (>=6)`. `grep -c` counts matching *lines*; the `PURE_TUI` const groups all editors on one line and all REPLs/tools across two lines, so `grep -c` returns 1 and 2 respectively even though all 5 editor strings and all 6 REPL/tool strings are present.
- **Fix:** No code change needed — verified the intent (all strings present) with `grep -oE ... | wc -l`, which returns 5 and 6 exactly. The const is correct per RESEARCH:554-567's grouped-comment layout; the criteria's line-count expectation assumed one-entry-per-line formatting that the research spec did not require.
- **Files modified:** none (verification-only)
- **Commit:** n/a

## Issues Encountered

- Pre-existing clippy warnings in `lacon-core` (lib), noted by Plan 03-02, are unchanged and out of scope (the SCOPE BOUNDARY rule). The three new files generate zero clippy warnings and `cargo check --workspace` is clean.

## User Setup Required

None — pure library code, no external service or config.

## Next Phase Readiness

- Plan 03-04 (hook orchestration) can call all three functions directly: `lacon_adapter_claudecode::tui::is_tui` for the TUI-before-resolve per-segment check, `lacon_core::rules::apply_rewrite` for the per-segment rewrite step, and `lacon_adapter_claudecode::quote::quote_for_shell` to re-quote each rewritten segment before wrapping as `lacon run --rule <id> -- <quoted argv>`.
- `lib.rs` now declares `chain` (Plan 02), `tui`, `quote`, and `protocol` — the full adapter module surface Plan 04 orchestrates over.
- REQ-adapter-tui-bypass has its full positive+negative predicate test gate; the whole-chain bypass *enforcement* (any segment matching → entire input passes through) lands in Plan 04.

## Self-Check: PASSED

All four created files exist on disk (tui.rs 161, quote.rs 131, rewrite.rs 197, tui_heuristic.rs 277 lines — all exceed plan min_lines). All three task commits (`85fea85`, `ca9b46c`, `deee1c9`) present in git history.

---
*Phase: 03-claude-code-adapter-lacon-init*
*Completed: 2026-05-21*
