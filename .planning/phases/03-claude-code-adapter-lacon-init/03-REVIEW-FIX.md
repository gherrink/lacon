---
phase: 03-claude-code-adapter-lacon-init
fixed_at: 2026-05-21T20:09:31Z
review_path: .planning/phases/03-claude-code-adapter-lacon-init/03-REVIEW.md
iteration: 3
findings_in_scope: 3
fixed: 3
skipped: 0
status: all_fixed
---

# Phase 3: Code Review Fix Report

**Fixed at:** 2026-05-21T20:09:31Z
**Source review:** .planning/phases/03-claude-code-adapter-lacon-init/03-REVIEW.md
**Iteration:** 3

**Summary:**
- Findings in scope: 3 (1 Critical + 2 Warning; Info findings IN-01..IN-03 out of scope)
- Fixed: 3
- Skipped: 0

All three in-scope findings live in the same function,
`has_unwrappable_construct` in `crates/lacon-adapter-claudecode/src/chain.rs`.
They were fixed in three atomic commits, ordered dead-code-removal (WR-02) →
flag-correctness (WR-01) → widen-the-guard (CR-01), so each commit left the crate
compiling and its tests green.

## Fixed Issues

### CR-01: Variable / tilde / glob expansion silently neutralized when a matched segment is wrapped

**Files modified:** `crates/lacon-adapter-claudecode/src/chain.rs`, `crates/lacon-adapter-claudecode/tests/hook_e2e.rs`
**Commit:** 477b5d3
**Status:** fixed
**Applied fix:** Widened `has_unwrappable_construct` so that, after the existing
quote/opacity-aware scan, ANY unquoted shell-expansion metacharacter marks the
segment unwrappable (byte-exact pass-through) rather than being re-tokenized and
single-quoted by `quote_for_shell`:
- A bare `$` not part of `${`/`$(` (checked AFTER those two branches so they keep
  their dedicated handling) → return true. Because the scan is already past the
  single-quote early-continue, any `$` reaching this point is outside single
  quotes; this fires for `$VAR`, `$1`, `$?`, `$@`, and also inside double quotes
  (`"$HOME"` still expands in bash), which is strictly more conservative than the
  review's `at_top_level()`-only snippet and matches the stated principle ("extend
  to every shell expansion `quote_for_shell` neutralizes").
- Unquoted glob metacharacters `*` / `?` / `[` at top level → return true.
- A leading `~` in word position (`prev_was_ws_or_start`) at top level → return
  true; a mid-token `~` (e.g. `a~b`) is correctly NOT flagged.

Reused the existing chain.rs DFA opacity tracking (single/double-quote and
subshell/cmd-sub depth) so bytes inside single quotes are still treated as literal
and faithfully reproduced. Corrected the iteration-1 test that wrongly asserted
`echo $var` was wrappable: replaced it with `unwrappable_detects_bare_variable_expansion`
(plus `unwrappable_detects_glob_metacharacters` and `unwrappable_detects_tilde_expansion`)
in the chain.rs unit tests, and added e2e regressions
`bare_variable_expansion_segment_passes_through_unwrapped` (`echo $HOME`,
`echo $1`), `glob_segment_passes_through_unwrapped` (`echo *.txt`), and
`tilde_segment_passes_through_unwrapped` (`echo ~`) in hook_e2e.rs proving the
compiled hook now passes these through unwrapped.

### WR-01: `#` comment detector false-positive after a closed `${...}` or `(...)` (stale word-position flag)

**Files modified:** `crates/lacon-adapter-claudecode/src/chain.rs`
**Commit:** df0fbfa
**Status:** fixed
**Applied fix:** Set `prev_was_ws_or_start = false` before the `continue` in the
`${...}` interior `{`/`}` branch and in the `(` / `)` branches of
`has_unwrappable_construct`. Since `{`, `}`, `(`, `)` are non-whitespace bytes, a
`#` glued immediately after a closed construct is not in word position and must
not start a comment. Added unit test
`unwrappable_glued_hash_after_closed_construct_is_not_a_comment` covering
`(echo x )#tag` and `(echo ${x} )#tag` (both now NOT unwrappable). Note the
review's literal example `echo ${x}#tag` cannot exercise this in THIS predicate
because a top-level `${` already short-circuits to `true`; the realistic trigger
is a construct closing back to top level inside/after a subshell, which the new
tests cover.

### WR-02: `has_unwrappable_construct` carried unreachable heredoc / process-sub machinery

**Files modified:** `crates/lacon-adapter-claudecode/src/chain.rs`
**Commit:** 94134f1
**Status:** fixed
**Applied fix:** Deleted the unreachable heredoc-body block from
`has_unwrappable_construct` (the function has no `<<DELIM` opener branch, so
`state.in_heredoc` is never set) and removed the dead `process_sub_depth`
decrement arm from its `)` branch (no `<(`/`>(` opener branch increments it). Both
are subsumed by the top-level `<`/`>` short-circuit that returns `true` for ANY
redirection byte before opacity could matter. Replaced the removed code with a
short explanatory comment documenting that any top-level `<`/`>` (which subsumes
heredoc / here-string / process-sub openers) short-circuits to unwrappable. The
full heredoc/process-sub machinery is intentionally retained only in `split_chain`
and `has_top_level_pipe`, where it is live.

## Skipped Issues

None.

## Verification

- `cargo test -p lacon-adapter-claudecode`: all green (chain.rs unit tests 25
  passed; hook_e2e.rs 20 passed; full crate suite passed). The three new e2e
  pass-through regressions and the corrected/added unit tests all pass.
- `cargo test --workspace`: the only failures are 5 pre-existing tests in
  `crates/lacon-cli/tests/end_to_end.rs` that panic with
  `CARGO_BIN_EXE_test_emitter is unset` — a test-harness fixture issue unrelated
  to these fixes. Verified identical failures on the pre-fix base commit
  (94183bb), confirming they pre-date and are independent of this work. Every
  crate I touched (lacon-adapter-claudecode) is fully green.
- `cargo clippy --workspace --all-targets`: zero warnings in the adapter crate or
  in either edited file. The remaining warnings are pre-existing in `lacon-core` /
  `lacon-cli` (outside the critical_warning fix scope; some overlap the Info-tier
  IN-* findings that were not in scope for this run).
- Hot-path posture (ADR-0013, ≤10ms cold start): the CR-01 widening adds only a
  handful of byte comparisons to the existing single-pass DFA and allocates
  nothing extra; WR-02 removes code. No new allocations on the success path.
- Byte-exact chain-reassembly invariant preserved: the widened guard only changes
  which segments are passed through verbatim vs. wrapped; pass-through segments are
  emitted byte-exact via `segment.text`, and reassembly via `trailing_op_span` is
  unchanged. The chain_split.rs reassembly matrix remains green.

---

_Fixed: 2026-05-21T20:09:31Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 3_
