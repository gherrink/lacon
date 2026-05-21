---
phase: 03-claude-code-adapter-lacon-init
fixed_at: 2026-05-21T19:57:42Z
review_path: .planning/phases/03-claude-code-adapter-lacon-init/03-REVIEW.md
iteration: 1
findings_in_scope: 10
fixed: 10
skipped: 0
status: all_fixed
---

# Phase 3: Code Review Fix Report

**Fixed at:** 2026-05-21T19:57:42Z
**Source review:** .planning/phases/03-claude-code-adapter-lacon-init/03-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope (Critical + Warning): 10
- Fixed: 10
- Skipped: 0
- Info findings (IN-01..IN-04): out of scope (fix_scope = critical_warning), not addressed

**Verification:** `cargo test --workspace` passes (36 test binaries, 0 failures) and
`cargo clippy --workspace --all-targets` introduces no new warnings in any modified
file. The remaining clippy warnings all live in untouched `lacon-core` modules
(`pipeline/stages.rs`, `tracking/*`) and a `lacon-cli` tracking test — pre-existing and
outside this phase's review scope. The byte-exact chain-reassembly invariant in
`tests/chain_split.rs` continues to hold (all 19 scenarios pass, including the two new
`${...}` scenarios).

## Fixed Issues

### CR-01 / CR-02 / CR-03 / CR-04 / WR-02: Unwrappable shell constructs are silently corrupted on wrap

**Files modified:** `crates/lacon-adapter-claudecode/src/chain.rs`, `crates/lacon-adapter-claudecode/src/lib.rs`, `crates/lacon-adapter-claudecode/tests/chain_split.rs`, `crates/lacon-adapter-claudecode/tests/hook_e2e.rs`
**Commit:** fb59ee1
**Applied fix:** The four BLOCKERs and WR-02 share one root cause: the orchestrator
wrapped matched segments via the lossy `argv_for_resolution` whitespace tokenizer +
`quote_for_shell`, guarding only top-level pipes. Two coordinated changes:

1. **`${...}` opacity in the chain splitter DFA (CR-04).** Added a
   `param_expansion_depth` field to `SplitState` and a `${`-opener / `}`-closer branch
   to both `split_chain` and `has_top_level_pipe`. A `&&`/`||`/`;`/`|` inside a `${x:-a
   && b}` default value no longer splits, so `echo ${x:-a && b}` stays one segment
   instead of mis-splitting into a broken `echo '${x:-a'` + `b}` two-command chain
   (`docs/specs/chained-commands.md:15`). Added test-matrix scenarios S14 / S14b to
   `tests/chain_split.rs` plus three unit tests.

2. **`has_unwrappable_construct(segment)` predicate (CR-01/02/03/WR-02).** A new
   single-pass DFA (same construction as `has_top_level_pipe`) that detects top-level
   redirections (`>`/`>>`/`<`/`2>`/`&>`/`<<<`/`<<DELIM`), command/process substitution
   (`$(...)`, backticks, `<(...)`/`>(...)`), `${...}` parameter expansion, shell
   comments (`#` in word position), and escaped whitespace (a top-level unescaped
   backslash). `run_hook` now passes such segments through byte-exact alongside the
   existing pipe guard, so the shell still sees the real construct instead of a
   neutralized literal. Added unit tests in `chain.rs` and six end-to-end regression
   tests in `hook_e2e.rs` (the previously-corrupted commands now pass through unwrapped,
   and a sibling matched segment is still wrapped, proving the guard is per-segment).

**Note (logic-sensitive):** these are correctness/security fixes that change runtime
behavior; the test matrix asserts the new byte-exact pass-through, but a human should
sanity-check the `has_unwrappable_construct` coverage matches the intended posture for
their environment before the phase proceeds to verification.

### WR-01: `git commit -am "msg"` misclassified as interactive

**Files modified:** `crates/lacon-adapter-claudecode/src/tui.rs`, `crates/lacon-adapter-claudecode/tests/tui_heuristic.rs`
**Commit:** b14d679
**Applied fix:** Extended `has_commit_message` with `is_bundled_short_flag_with_m`,
which treats any single-dash ASCII-letter cluster containing `m` (e.g. `-am`, `-vm`) as
supplying an inline message — matching the review's `^-[a-zA-Z]*m` shape while excluding
`--` long flags and non-letter clusters. Added test rows: `-am` / `-vm` → not TUI, and a
guard that `-a` alone (no `m`) is still TUI.

### WR-03: `lacon init` atomic write silently resets `settings.json` permissions

**Files modified:** `crates/lacon-cli/src/commands/init.rs`, `crates/lacon-cli/tests/cli_init.rs`
**Commit:** 4a86d52
**Applied fix:** In `atomic_write_json`, before `persist`, read the destination's
existing mode (when it exists) and re-apply it to the tempfile (Unix-gated, matching the
v1 macOS+Linux scope; best-effort so a metadata read failure never aborts the more
important atomic write). Added a Unix `#[cfg(unix)]` e2e test that pre-creates
`settings.json` at `0644`, runs `lacon init`, and asserts the mode is preserved.

### WR-04: Orphan-marker recovery in CLAUDE.md is not idempotent

**Files modified:** `crates/lacon-cli/src/commands/init.rs`, `crates/lacon-cli/tests/cli_init.rs`
**Commit:** 4a86d52
**Applied fix:** Chose the review's option (a): added `strip_lacon_markers` and call it
in the orphan/corrupt-ordering branch of `install_claude_md_block` to scrub the stray
marker token(s) (and their own trailing newline) before appending a fresh well-formed
block. This makes recovery converge — a subsequent run sees exactly one well-formed pair
and takes the in-place-replace branch instead of accreting blocks or clobbering
sandwiched user content. Updated the existing orphan test and added a double-run
idempotency unit test, an orphan-end-marker test, a `strip_lacon_markers` unit test, and
a double-run e2e idempotency test.

### WR-05: `record_invocation` re-reads layered config on every `lacon run` (cold-start cost)

**Files modified:** `crates/lacon-cli/src/commands/run.rs`
**Commit:** 1f93513
**Applied fix:** Gated `config::load_layered` behind the existence of at least one config
file. When neither a project nor a user config file exists (the common case, already
established by the cheap `Path::exists()` probes), the YAML parse is skipped entirely and
`Config::default()` is used — avoiding the per-invocation startup work ADR-0013's ≤10ms
cold-start budget warns against. Behavior is unchanged when a config file is present.

### WR-06: Rule-load errors logged to stderr but invisible to the user

**Files modified:** `crates/lacon-adapter-claudecode/src/lib.rs`
**Commit:** 94183bb
**Applied fix:** Kept the best-effort pass-through (the correct safety posture) but made
the failure observable: added `log_rule_load_errors`, which appends the error(s) to a
discoverable log file `<XDG_DATA_HOME>/lacon/hook-errors.log` (sibling of the tracker DB,
reusing the existing `Tracker::xdg_db_path` helper — no new dependency). This is the
exceptional malformed-rule path only, so it never touches the success hot path or the
cold-start budget. Fully best-effort: any path-resolution or write failure is swallowed.
Implements the review's "log the error to a known location" option.

---

_Fixed: 2026-05-21T19:57:42Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
