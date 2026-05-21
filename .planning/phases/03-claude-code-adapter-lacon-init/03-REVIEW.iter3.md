---
phase: 03-claude-code-adapter-lacon-init
reviewed: 2026-05-21T20:10:00Z
depth: standard
iteration: 2
files_reviewed: 19
files_reviewed_list:
  - crates/lacon-adapter-claudecode/src/bin/hook.rs
  - crates/lacon-adapter-claudecode/src/chain.rs
  - crates/lacon-adapter-claudecode/src/lib.rs
  - crates/lacon-adapter-claudecode/src/protocol.rs
  - crates/lacon-adapter-claudecode/src/quote.rs
  - crates/lacon-adapter-claudecode/src/tui.rs
  - crates/lacon-adapter-claudecode/tests/chain_split.rs
  - crates/lacon-adapter-claudecode/tests/hook_e2e.rs
  - crates/lacon-adapter-claudecode/tests/tui_heuristic.rs
  - crates/lacon-adapter-claudecode/Cargo.toml
  - crates/lacon-cli/src/commands/init.rs
  - crates/lacon-cli/src/commands/run.rs
  - crates/lacon-cli/tests/cli_init.rs
  - crates/lacon-cli/Cargo.toml
  - crates/lacon-core/src/rules/loader.rs
  - crates/lacon-core/src/rules/mod.rs
  - crates/lacon-core/src/rules/rewrite.rs
  - benches/cold_start.rs
  - benches/Cargo.toml
findings:
  critical: 1
  warning: 2
  info: 3
  total: 6
status: issues_found
---

# Phase 3: Code Review Report (re-review, iteration 2)

**Reviewed:** 2026-05-21T20:10:00Z
**Depth:** standard
**Files Reviewed:** 19
**Status:** issues_found

## Summary

This re-review verifies the fixer's commits against iteration-1's 4 Critical + 6 Warning
findings, and re-attacks the same adversarial surfaces (shell-quoting / unwrappable
constructs in chain.rs + lib.rs, settings.json/CLAUDE.md mutation in init.rs, ≤10ms
cold-start budget per ADR-0013).

**Verification of the iteration-1 fixes — all confirmed correct and complete:**

- **CR-01 (redirections), CR-02 (command substitution), CR-03 (comments), WR-02 (escaped
  whitespace):** Fixed via the new `has_unwrappable_construct` DFA predicate
  (`chain.rs:545-683`), wired into the orchestrator alongside the existing pipe guard
  (`lib.rs:186-191`). Verified end-to-end against the compiled `lacon-claude-hook`: each
  construct now passes through byte-exact (`hook_e2e.rs` regression tests pass). The
  per-segment posture is preserved — an unwrappable segment is passed through while a
  sibling matched segment is still wrapped (`unwrappable_segment_preserved_while_sibling_wrapped`).
- **CR-04 (`${...}` mis-split):** Fixed — both `split_chain` and `has_top_level_pipe` now
  track `param_expansion_depth`, opening on `${` (checked BEFORE `$(`) and closing on the
  matching `}` (`chain.rs:225-253, 433-454`). `echo ${x:-a && b}` now yields one segment
  (new scenarios S14/S14b in `chain_split.rs`, e2e `param_expansion_with_chain_op_not_missplit`).
- **WR-01 (`git commit -am`):** Fixed via `is_bundled_short_flag_with_m` (`tui.rs:106-116`);
  `-am`/`-vm` no longer misclassified as TUI, while `-a` (no `m`) still correctly is
  (tests `git_commit_with_dash_am_false`, `git_commit_with_dash_a_only_true`).
- **WR-03 (settings.json mode):** Fixed — `atomic_write_json` reads the destination's
  mode and re-applies it to the tempfile before `persist` (`init.rs:300-308`); e2e test
  `init_preserves_existing_settings_file_permissions` locks `0644` survival.
- **WR-04 (orphan-marker convergence):** Fixed — the orphan branch now calls
  `strip_lacon_markers` before `append_fresh_block` (`init.rs:199-209`), so repeated runs
  converge; double-run idempotency tests added (`init_orphan_claude_md_marker_recovery_is_idempotent`).
- **WR-05 (cold-start config parse):** Fixed — `record_invocation` short-circuits to
  `Config::default()` when neither config file exists (`run.rs:207-215`). I verified this
  is behaviorally identical to the old path: `config::load_layered(None, None)` starts from
  `Config::default()` and applies no layer, so the skip changes timing only, not result.
- **WR-06 (silent rule-load failure):** Fixed — errors are now also appended to
  `<XDG_DATA_HOME>/lacon/hook-errors.log` via `log_rule_load_errors` (`lib.rs:226-295`),
  best-effort and off the success hot path.

**New finding from re-attacking the same surface:** the `has_unwrappable_construct` guard
closed the four named constructs but the *root cause* — that `argv_for_resolution` +
`quote_for_shell` cannot reproduce any shell expansion bash would perform — is wider than
the four constructs that were patched. Bare variable expansion (`$VAR`, `$1`, `$?`, `$@`),
tilde expansion (`~`), and pathname/glob expansion (`*`, `?`, `[...]`) are still wrapped
and silently neutralized. This is the same correctness family as CR-01..CR-04 and is the
BLOCKER below. The fix even codified the gap with a test asserting `echo $var` is wrappable
(`chain.rs:932-934`), so it is an explicit (incorrect) decision rather than an oversight.

## Narrative Findings (AI reviewer)

## Critical Issues

### CR-01: Variable / tilde / glob expansion is silently neutralized when a matched segment is wrapped (same family as the iter-1 CRs, not covered by the fix)

**File:** `crates/lacon-adapter-claudecode/src/chain.rs:545-683` (guard), `crates/lacon-adapter-claudecode/src/lib.rs:186-218` (wrap path), `crates/lacon-adapter-claudecode/src/quote.rs:26`
**Issue:** The whole rewrite design depends on this invariant: a wrapped segment, after
`argv_for_resolution` → `quote_for_shell` → execution by `lacon run`
(`Command::new(argv[0]).args(...)`, no shell hop), must observe the *same behavior* as the
original. `has_unwrappable_construct` now guards `$(...)`, backticks, `<(...)`/`>(...)`,
`${...}`, redirections, comments, and escaped whitespace — but it deliberately does NOT
guard bare `$`, `~`, or glob characters (the `unwrappable_ignores_plain_commands` test at
`chain.rs:932-934` asserts `echo $var` is wrappable). `quote_for_shell`'s METACHARS set
(`quote.rs:26`) includes `$`, `~`, `*`, `?`, `[` — correctly, for injection-safety — so
each of these is single-quoted, and `lacon run` then runs the literal with no shell to
expand it. Confirmed end-to-end against the compiled `lacon-claude-hook` (rule `command: echo`):

```
input : echo $HOME    output: ... lacon run --rule echo-rule -- echo '$HOME'
input : echo *.txt    output: ... lacon run --rule echo-rule -- echo '*.txt'
input : echo ~        output: ... lacon run --rule echo-rule -- echo '~'
input : echo $1       output: ... lacon run --rule echo-rule -- echo '$1'
```

The originals expand to the home dir / matching files / home / first positional arg; the
rewrites print the literal tokens `$HOME`, `*.txt`, `~`, `$1`. This is a behavior-changing
rewrite of the exact same family as iter-1's CR-01..CR-04 (it silently changes program
output), and `$VAR`/glob are far more common in real commands than the constructs that were
patched (`cargo build $FLAGS`, `ls *.rs`, `grep foo src/*`, `echo ~/.config`). The D-08
scope reduction (2026-05-16) authorized treating `$(...)` as a plain token for *rule
resolution*; it did NOT authorize silently rewriting segments whose expansion the wrap
form cannot reproduce. The conservative posture the fix already adopted for the four named
constructs (`chained-commands.md:17`) must extend to every shell expansion `quote_for_shell`
neutralizes.

**Fix:** Extend `has_unwrappable_construct` so a top-level token containing any character
`quote_for_shell` would single-quote-neutralize is treated as unwrappable. Concretely, add
top-level detection for: a bare `$` (variable / positional / special-param expansion,
i.e. `$` not already handled by the `${`/`$(` branches), a leading `~` in word position
(tilde expansion), and unquoted glob metacharacters `*` / `?` / `[`. Then pass such
segments through byte-exact, exactly as the pipe / redirection / substitution guards do:

```rust
// in has_unwrappable_construct, at top level, outside single quotes:
if b == b'$' && state.at_top_level() {
    // not ${ or $( (handled above) -> bare variable/positional/special expansion
    return true;
}
if (b == b'*' || b == b'?' || b == b'[') && state.at_top_level() {
    return true; // unquoted glob: lacon run cannot pathname-expand it
}
// leading `~` in word position (prev_was_ws_or_start) at top level -> tilde expansion
if b == b'~' && state.at_top_level() && prev_was_ws_or_start {
    return true;
}
```

Replace the `unwrappable_ignores_plain_commands` assertion that `echo $var` is wrappable
with one asserting it is now unwrappable, and add e2e regressions for `echo $HOME`,
`echo *.txt`, `echo ~`.

## Warnings

### WR-01: `#` comment detector can false-positive after a closed `${...}` or `(...)` (stale word-position flag)

**File:** `crates/lacon-adapter-claudecode/src/chain.rs:637-679`
**Issue:** `has_unwrappable_construct` tracks `prev_was_ws_or_start` to ensure a `#` only
starts a comment in word position (so `echo a#b` is not flagged — `chain.rs:931`). But the
`param_expansion_depth > 0` interior branch (lines 637-648) and the `(` / `)` branches
(lines 658-672) `continue` WITHOUT updating `prev_was_ws_or_start`. After a `}` closes a
`${...}` or a `)` closes a subshell at top level, the flag is stale-`true`, so a glued
`#` such as `echo ${x}#tag` or `(true)#tag` would be flagged as a comment and the segment
treated as unwrappable. This is the *conservative* direction (it over-reports unwrappable,
reducing filtering opportunity rather than producing a broken command), hence WARNING not
BLOCKER — but it contradicts the function's documented word-position intent and is
untested. Note this is a pre-existing characteristic of the new fix code, not a regression
from a prior version.

**Fix:** Set `prev_was_ws_or_start = false` before the `continue` in the `${...}` interior
branch and in the `(` / `)` branches (a `}`, `)`, `{` are non-whitespace bytes, so the
next `#` is glued and must not start a comment). Add a test row `echo ${x}#tag` →
not-unwrappable (or, if treating it as unwrappable is acceptable, document it).

### WR-02: `has_unwrappable_construct` carries unreachable heredoc/process-sub machinery (dead code in a security-critical predicate)

**File:** `crates/lacon-adapter-claudecode/src/chain.rs:564-585, 663-672`
**Issue:** In `has_unwrappable_construct`, the top-level `<` / `>` check (line 653) returns
`true` for ANY redirection byte before any heredoc-opener or process-sub-opener branch
could run — and indeed this function has no `<<DELIM` opener branch and no `<(`/`>(` opener
branch at all. Consequently `state.in_heredoc` is never set to `Some`, making the entire
heredoc-body block (lines 564-585) unreachable, and `process_sub_depth` is never
incremented, making the `process_sub_depth` arm of the `)` decrementer (lines 665-667) dead.
This is functionally correct (the early `return true` is the right conservative behavior),
but dead code inside a security-relevant predicate invites a future maintainer to "fix" the
heredoc handling and accidentally weaken the redirection guard, or to mis-read the function
as having heredoc-aware opacity it does not have.

**Fix:** Delete the unreachable heredoc block and the `process_sub_depth` decrement arm
from `has_unwrappable_construct`, and add a one-line comment that any top-level `<`/`>`
(which subsumes heredoc/here-string/process-sub openers) short-circuits to unwrappable.
Keep the full machinery only in `split_chain` / `has_top_level_pipe`, where it is live.

## Info

### IN-01: Dead no-op `let _ = state.in_opaque();` retained "for documentation" (carried over from iter-1 IN-01, not in fixer scope)

**File:** `crates/lacon-adapter-claudecode/src/chain.rs:346`
**Issue:** `let _ = state.in_opaque();` calls a side-effect-free predicate and discards the
result purely as a comment surrogate; `in_opaque()` has no other caller in the split path.
A reader may mistake it for meaningful state handling. (Info; outside the iter-2 fixer's
CR/WR scope.)
**Fix:** Delete the line and rely on the adjacent comment, or replace with a
`debug_assert!` that states the actual invariant.

### IN-02: Vestigial `_basename` parameter in `is_db_interactive` (carried over from iter-1 IN-02)

**File:** `crates/lacon-adapter-claudecode/src/tui.rs:137`
**Issue:** `is_db_interactive(args, _basename)` ignores its second parameter; all three DB
tools (`mysql`/`psql`/`sqlite3`) share identical logic. The parameter is dead.
**Fix:** Drop the parameter and the argument at the call site (`tui.rs:57`) unless a
per-tool branch is imminent.

### IN-03: Cold-start probe records timing samples without checking exit status (carried over from iter-1 IN-04)

**File:** `benches/cold_start.rs:22-26, 32-47`
**Issue:** `measure_one` and `measure_hook` discard the spawned process `Result` (`let _ =`)
and never check exit status, so a binary that crashes or errors still records a
"successful" timing sample, silently skewing the baseline the Phase 6 cold-start gate
(REQ-acceptance-cold-start-budget) depends on. Operator tooling, not CI, hence Info.
**Fix:** Assert (or at least warn on) `output.status.success()` before recording a sample
so a broken binary fails the probe loudly rather than reporting bogus sub-millisecond times.

---

_Reviewed: 2026-05-21T20:10:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard (re-review, iteration 2)_
