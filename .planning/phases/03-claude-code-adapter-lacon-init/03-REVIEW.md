---
phase: 03-claude-code-adapter-lacon-init
reviewed: 2026-05-21T19:55:00Z
depth: standard
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
  critical: 4
  warning: 6
  info: 4
  total: 14
status: issues_found
---

# Phase 3: Code Review Report

**Reviewed:** 2026-05-21T19:55:00Z
**Depth:** standard
**Files Reviewed:** 19
**Status:** issues_found

## Summary

This phase wires the Claude Code `PreToolUse(Bash)` hook: parse stdin → bypass-detect →
chain-split → TUI-bypass → per-segment rule resolve → rewrite → shell-quote → wrap as
`lacon run --rule <id> -- <argv>` → reassemble → emit. I reviewed it adversarially with
particular focus on the four security-sensitive areas called out in the brief (shell
quoting, chain-splitter opacity, settings.json/CLAUDE.md mutation, cold start).

**Good news first:** `quote_for_shell` (quote.rs) is sound. The single-quote
`'\''` close-escape-reopen idiom and the conservative metachar set survive a single
`/bin/sh` parse for every adversarial input I threw at it (`$(rm -rf /)`, backticks,
`a' ; rm -rf / ; '`, `${HOME}`, `%n`, trailing backslash). The direct command-injection
threat (T-quote-injection / T-03-03-01) is mitigated.

**Bad news:** the *orchestrator* (lib.rs) wraps segments based on `argv_for_resolution`,
a lossy whitespace tokenizer that does not model redirections, command substitution,
shell comments, or `${...}` expansion — and it has **no guard** for these constructs
(only `has_top_level_pipe` is guarded). As a result the hook silently rewrites correct
commands into commands with *different runtime semantics*. I confirmed four distinct
behavior-changing rewrites end-to-end against the compiled `lacon-claude-hook` binary:
a dropped file redirect (data loss), a neutralized command substitution, a destroyed
shell comment, and — via a separate `${...}` opacity gap in the splitter — a mis-split
that turns one command into a broken two-command chain. These are the BLOCKERs below.

The `lacon init` config mutation (init.rs) is well-structured and idempotent, with one
real defect (file-permission clobbering on atomic write) and one orphan-marker edge case.

## Critical Issues

### CR-01: Redirections are silently destroyed when a matched segment is wrapped (data loss)

**File:** `crates/lacon-adapter-claudecode/src/lib.rs:62-111, 163-208`
**Issue:** `argv_for_resolution` whitespace-splits a segment without modeling shell
redirections. `echo hi > out.txt` tokenizes to `["echo","hi",">","out.txt"]`. The rule
matches `command: echo`, every token is run through `quote_for_shell`, and `>` becomes a
quoted literal `'>'`. Confirmed end-to-end against the compiled hook:

```
input : echo hi > out.txt
output: LACON_ASSISTANT=claude-code ... lacon run --rule echo-rule -- echo hi '>' out.txt
```

The original wrote `hi` to `out.txt`; the rewrite makes `echo` print `hi > out.txt` to
stdout and **never creates the file**. This is silent data loss: the user's redirect
target is never written. The orchestrator only guards `has_top_level_pipe`; it has no
equivalent guard for `>`, `>>`, `<`, `2>`, `&>`, etc.

**Fix:** Before wrapping, detect whether the segment contains any top-level construct the
re-quoted-argv form cannot reproduce (redirections, command/process substitution,
comments, expansions). If so, pass the segment through byte-exact (the same conservative
posture already used for pipes per `chained-commands.md:17`). Reuse the chain.rs DFA to
expose a `has_unwrappable_construct(segment)` predicate rather than relying on the lossy
`argv_for_resolution` tokenizer:

```rust
if crate::chain::has_top_level_pipe(&segment.text)
    || crate::chain::has_unwrappable_construct(&segment.text) {
    rendered.push(segment.text.clone());
    continue;
}
```

### CR-02: Command substitution is neutralized when a matched segment is wrapped

**File:** `crates/lacon-adapter-claudecode/src/lib.rs:62-111, 182-208`
**Issue:** Same root cause as CR-01. `argv_for_resolution` does NOT track `$(...)` /
backticks (this is the documented D-08 2026-05-16 scope reduction), so the substitution
text becomes a single literal token, then `quote_for_shell` single-quotes it, neutralizing
it. Confirmed end-to-end:

```
input : echo $(whoami)
output: ... lacon run --rule echo-rule -- echo '$(whoami)'
```

The original echoes the current username; the rewrite echoes the literal string
`$(whoami)`. The chain splitter (chain.rs) correctly keeps `echo $(a && b)` as one
segment (test `s8`), and quote.rs proves `quote_for_shell` neutralizes `$(rm -rf /)` — but
those mitigations are aimed at injection. Here the bug is the *opposite*: the hook
silently strips a substitution the user intended to execute. The D-08 scope note says
full `$(...)` opacity in the resolver tokenizer is deferred, but it does NOT authorize
silently rewriting such segments — they must pass through unwrapped.

**Fix:** Same `has_unwrappable_construct` guard as CR-01 — treat any segment containing
top-level `$(...)`, `` `...` ``, or `<(...)`/`>(...)` as unwrappable and pass it through
byte-exact.

### CR-03: Shell comments are converted into literal arguments

**File:** `crates/lacon-adapter-claudecode/src/lib.rs:62-111, 182-208`
**Issue:** `argv_for_resolution` does not recognize the `#` comment token. `echo hi # do
thing` tokenizes to `["echo","hi","#","do","thing"]` and is wrapped. Confirmed end-to-end:

```
input : echo hi # do thing
output: ... lacon run --rule echo-rule -- echo hi '#' do thing
```

The original prints `hi` (the rest is a comment); the rewrite prints `hi # do thing`.
Note `#` is in `quote.rs`'s METACHARS set, so it is correctly quoted (no injection) — but
the comment semantics are destroyed, changing program output. This is a correctness defect
of the same family as CR-01/CR-02.

**Fix:** Treat a top-level `#` (preceded by whitespace or at start-of-segment) as
making the segment unwrappable; pass it through byte-exact. The `has_unwrappable_construct`
predicate from CR-01 should cover comments.

### CR-04: `${...}` parameter expansion is not opaque in the chain splitter — causes a dangerous mis-split

**File:** `crates/lacon-adapter-claudecode/src/chain.rs:139-314`
**Issue:** `docs/specs/chained-commands.md:15` explicitly lists `${...}` parameter
expansion as a top-level-suppressing opaque construct ("not inside `${...}` parameter
expansion"). The DFA does not handle it: the `$(` opener at line 215 requires
`bytes[i+1] == b'('`, so `${` falls through, the `{` is consumed as an ordinary byte, and
a `&&`/`||`/`;` inside the braces splits at top level. Confirmed end-to-end against the
compiled hook:

```
input : echo ${x:-a && b}
output: ... lacon run --rule echo-rule -- echo '${x:-a' && b}
```

One command (echo with a default-value expansion) becomes a broken two-segment chain: the
first segment is wrapped with a truncated `${x:-a`, and `b}` is emitted as a second
top-level command that the shell will try to execute and fail on. This is a spec
violation (the splitter is the authoritative opacity layer per D-06) and produces a
materially different, broken command — strictly worse than the conservative single-segment
behavior the spec mandates. The 13-scenario test matrix has no `${...}` case, so the gap
is untested.

**Fix:** Add a `${`-opener branch to the DFA (in both `split_chain` and
`has_top_level_pipe`) that tracks brace-expansion depth, opening on `${` and closing on the
matching `}`, suppressing chain operators while depth > 0. Add a corresponding scenario to
`tests/chain_split.rs` (e.g. `echo ${x:-a && b}` must yield exactly one segment).

## Warnings

### WR-01: `git commit -am "msg"` is misclassified as interactive (false-positive whole-chain bypass)

**File:** `crates/lacon-adapter-claudecode/src/tui.rs:87-95`
**Issue:** `has_commit_message` exact-matches `-m` / `--message` / `-F` / `--file` but not
the combined short-flag form `-am` (or `-m`-bundled forms like `-vm`). `git commit -am
"msg"` is an extremely common non-interactive invocation; it returns `false` from
`has_commit_message`, so `is_git_interactive` reports TUI, forcing a whole-chain bypass.
Per the spec a false positive only costs filtering opportunity (not a hang), so this is a
WARNING — but `-am` is common enough to materially reduce coverage.

**Fix:** Detect a bundled short flag containing `m`: for any arg matching `^-[a-zA-Z]*m`
(no `=`, no `--`), treat a message as present. Add a test row `git commit -am msg → not
TUI`.

### WR-02: Backslash-escaped whitespace in a wrapped segment changes argument boundaries

**File:** `crates/lacon-adapter-claudecode/src/lib.rs:62-111`
**Issue:** `argv_for_resolution` does not process backslash escapes at all, so a literal
backslash is retained in the token and whitespace is always a separator. `echo a\ b`
(bash: one argument `a b`) tokenizes to `["echo","a\\","b"]`. Confirmed end-to-end:

```
input : echo a\ b
output: ... lacon run --rule echo-rule -- echo 'a\' b
```

The original echoes `a b` (one arg); the rewrite echoes `a\` and `b` (two args). No
injection (the backslash is safely single-quoted), but the argument vector — and therefore
the program's behavior — changes. Same family as CR-01..CR-03 but lower impact (no data
loss).

**Fix:** Either fold this into the `has_unwrappable_construct` guard (treat a top-level
unescaped backslash as unwrappable), or teach `argv_for_resolution` to honor backslash
escapes the way bash does outside quotes. The guard approach is simpler and matches the
conservative posture.

### WR-03: `lacon init` atomic write silently resets `settings.json` file permissions

**File:** `crates/lacon-cli/src/commands/init.rs:235-250`
**Issue:** `atomic_write_json` creates a fresh `NamedTempFile` (default mode `0600`) and
`persist`es it over `settings.json`. `persist` keeps the *tempfile's* permissions, not the
original file's. If a user previously had `settings.json` with non-default permissions
(e.g. group-readable for a shared dev box), re-running `lacon init` silently narrows them
to `0600`. The doc comment claims only atomicity; permission preservation is neither
documented nor implemented. This can also surprise users who expect their VCS-tracked
file's mode to be stable.

**Fix:** Before `persist`, if the destination exists, read its metadata and re-apply the
mode to the tempfile (`std::fs::set_permissions(tmp.path(), orig_perms)`), or document the
`0600` normalization explicitly as intended behavior.

### WR-04: Orphan-marker recovery in CLAUDE.md is not idempotent (block accretes on every run)

**File:** `crates/lacon-cli/src/commands/init.rs:179-207`
**Issue:** When CLAUDE.md has exactly one marker (orphan/corrupt state), the code appends a
*fresh* full block at EOF and leaves the orphan untouched (lines 196-204). On the next
`lacon init`, the file now has the orphan start marker AND the freshly appended
start+end. `existing.find(LACON_START)` returns the FIRST occurrence (the orphan), and
`find(LACON_END)` returns the appended end — so `(Some(s), Some(e)) if s < e` matches and
the code replaces everything *between the orphan start and the appended end*, including the
appended block's start marker and any user content in between. Repeated runs do not
converge to a stable file, and user content sandwiched between the orphan and the new block
can be clobbered. The test `claude_md_orphan_marker_appends_fresh_and_keeps_orphan` only
runs once, so it does not catch the non-convergence.

**Fix:** When recovering from an orphan marker, either (a) strip the orphan marker first so
the file returns to a clean single-block state, or (b) search for a *well-formed* matched
pair (nearest `start` whose next marker is an `end`) rather than the first-of-each. Add a
double-run idempotency test for the orphan case.

### WR-05: `record_invocation` re-reads layered config on every `lacon run` — cold-start cost on the hot path

**File:** `crates/lacon-cli/src/commands/run.rs:182-204`
**Issue:** Every wrapped command runs `lacon run`, which (after filtering) calls
`record_invocation`, which calls `etcetera::choose_base_strategy()`, two `Path::exists()`
syscalls, and `config::load_layered` (YAML parse of up to two files) on *every* invocation.
ADR-0013 budgets ≤10ms cold start for the hook hot path; `lacon run` is on that same hot
path (it is spawned for every matched segment, thousands of times per session). Config
parsing + filesystem probes per invocation is exactly the kind of avoidable startup work
the budget warns against. Tracking is best-effort and could defer or cache this.

**Fix:** Gate the config load behind `cfg.store_raw_outputs` necessity, or cache the
resolved config path/values, or move the privacy-marker resolution out of the per-run path.
At minimum, avoid parsing YAML when no project/user config file exists (the common case).

### WR-06: Rule-load errors are logged but the offending command is silently passed through

**File:** `crates/lacon-adapter-claudecode/src/lib.rs:215-222`
**Issue:** On `Err(errors)` from `match_argv_via_load_all`, the hook prints to stderr and
treats the segment as unmatched. Claude Code does not surface hook stderr to the user in
the normal flow (it captures stdout as the tool result). A user with a broken rule file
gets *no filtering and no visible warning* — the failure is invisible. This is a
robustness/observability gap: a malformed rule silently disables filtering for matching
commands with no feedback loop. (Best-effort pass-through is the right safety posture; the
problem is the silence.)

**Fix:** Keep the pass-through, but make the failure observable: increment a counter the
`lacon doctor` command can surface, or write the error to a known log location, or (if the
protocol allows) emit a non-blocking `additionalContext` notice. At minimum document that
rule-load errors degrade silently so it is a known operational characteristic.

## Info

### IN-01: Dead no-op call kept "for documentation"

**File:** `crates/lacon-adapter-claudecode/src/chain.rs:312`
**Issue:** `let _ = state.in_opaque();` calls a side-effect-free predicate and discards the
result purely as a comment surrogate. It is dead code that a reader may mistake for
meaningful state handling, and `in_opaque()` has no other caller in the split path.
**Fix:** Delete the line and rely on the existing comment, or add a `debug_assert!` that
documents the invariant meaningfully (e.g. that opaque bytes only reach this fall-through).

### IN-02: Unused parameter `_basename` in `is_db_interactive`

**File:** `crates/lacon-adapter-claudecode/src/tui.rs:116`
**Issue:** `is_db_interactive(args, _basename)` ignores its second parameter; all three DB
tools share identical logic. The parameter is vestigial.
**Fix:** Drop the parameter and the argument at the call site (tui.rs:57) unless a
per-tool branch is imminent.

### IN-03: `is_repl` flags `python -cCODE` (glued short flag) as a REPL

**File:** `crates/lacon-adapter-claudecode/src/tui.rs:109-111`
**Issue:** `is_repl` returns true when every arg starts with `-`. `python -c'print(1)'`
(code glued to the flag, no space) tokenizes to one arg `-cprint(1)` → all start with `-`
→ classified as REPL/TUI. This is a false positive (one-shot execution misread as
interactive). `python -c 'print(1)'` (with a space) is handled correctly. Conservative
direction (extra bypass), hence Info.
**Fix:** Special-case `-c`/`-m`-prefixed args (and `-e` for the DB tools) as non-REPL when
followed by glued content, or defer with the documented v1.5 polish note already in the
module docs.

### IN-04: Cold-start probe percentile/`run_scenario` ignores the `lacon` binary's stderr and exit status

**File:** `benches/cold_start.rs:22-26, 80-90`
**Issue:** `measure_one` discards `Command::output()`'s `Result` (`let _ =`) and never
checks exit status, so a binary that crashes or errors still records a "successful" timing
sample, silently skewing the baseline the Phase 6 gate depends on. The probe is operator
tooling (not CI), so this is Info, but a failed run masquerading as a fast run is
misleading.
**Fix:** Assert `output.status.success()` (or at least warn) before recording a sample, so
a broken binary fails the probe loudly instead of reporting bogus sub-millisecond times.

---

_Reviewed: 2026-05-21T19:55:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
