---
phase: 03-claude-code-adapter-lacon-init
plan: 02
subsystem: adapter
tags: [adapter, chain-splitter, dfa, tdd, byte-exact-reassembly, pipes-passthrough]

# Dependency graph
requires:
  - phase: 03-claude-code-adapter-lacon-init
    plan: 01
    provides: lacon-adapter-claudecode crate skeleton (lib.rs with commented chain mod placeholder), protocol structs, HookOutcome
provides:
  - "lacon_adapter_claudecode::chain::split_chain(&str) -> Vec<Segment> — top-level chain splitter (D-06/D-07)"
  - "chain::Segment { text, trailing_op, trailing_op_span } + chain::ChainOp { AndAnd, OrOr, Semi }"
  - "byte-exact reassembly invariant: join(text + trailing_op_span) == original input"
  - "pub mod chain declaration in lib.rs"
affects:
  - "03-03 (tui/quote): adds tui/quote mods alongside chain in lib.rs"
  - "03-04 (hook orchestration): consumes split_chain per-segment for resolution + reassembly"

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Hand-rolled single-pass byte-iterating DFA (first state machine in the codebase) — no shlex/conch-parser dep (D-06)"
    - "Pure-fn module shape mirrored from lacon-core/src/tracking/normalize.rs (module //! docblock + // Examples doctest + #[cfg(test)] mod tests)"
    - "Span-capture for byte-exact reassembly: trailing whitespace moved off segment.text into trailing_op_span"
    - "Table-driven scenario tests (one #[test] per row) — no proptest/rstest, consistent with normalize.rs"

key-files:
  created:
    - crates/lacon-adapter-claudecode/src/chain.rs
    - crates/lacon-adapter-claudecode/tests/chain_split.rs
  modified:
    - crates/lacon-adapter-claudecode/src/lib.rs

key-decisions:
  - "8-field SplitState (in_single_quote, in_double_quote, subshell_depth, cmd_sub_depth, backtick_depth, process_sub_depth, in_heredoc, escape_pending) per 03-RESEARCH.md:466-510"
  - "$(...) precedence over (...) via $ lookahead at `$(`; closing `)` decrements highest open depth in order cmd_sub > process_sub > subshell (03-RESEARCH.md:503)"
  - "Backtick is a flat toggle (backtick_depth ^= 1) — bash does not nest backticks (03-RESEARCH.md:508)"
  - "Backslash is literal inside single quotes (no escape_pending) per 03-RESEARCH.md:504"
  - "Heredoc body opaque until a line equal to the captured delimiter (with optional leading-tab strip for <<-); <<< here-string consumed as 3-byte opaque token"
  - "trailing_op_span captures leading whitespace (trimmed off segment.text) + operator bytes + trailing whitespace so text+span reassembly is byte-exact"
  - "Single `|` and single `&` are consumed verbatim — only `&&`/`||`/`;` split (D-09 / REQ-adapter-pipes-passthrough)"

requirements-completed: [REQ-adapter-chained-commands, REQ-adapter-pipes-passthrough]

# Metrics
duration: 3min
completed: 2026-05-21
---

# Phase 3 Plan 02: Chain splitter DFA Summary

**A hand-rolled single-pass byte-iterating DFA (`split_chain`) that splits bash command chains at top-level `&&`/`||`/`;` while keeping every opaque construct (quotes, subshells, `$(...)`, backticks, process-substitution, heredoc bodies) suppressed, with a `trailing_op_span` capture that guarantees byte-exact reassembly — gated by the full 13-scenario spec matrix.**

## Performance

- **Duration:** ~3 min
- **Started:** 2026-05-21T19:16:47Z
- **Completed:** 2026-05-21T19:23Z
- **Tasks:** 2 (TDD: RED then GREEN)
- **Files modified:** 3 (2 created, 1 modified)

## Accomplishments

- Implemented `split_chain(&str) -> Vec<Segment>` as a linear-time, single-pass DFA over UTF-8 bytes — the first byte-iterating state machine in the codebase. The 8-field `SplitState` tracks single/double quotes, `(...)` subshell depth, `$(...)` cmd-sub depth, backtick toggle, `<(...)`/`>(...)` process-sub depth, heredoc context, and escape-pending exactly per the 03-RESEARCH.md:466-510 transition table.
- Locked the byte-exact reassembly invariant (T-injection-chain-reassembly mitigation): every `Segment` records its verbatim `text` plus a `trailing_op_span` that carries leading whitespace (trimmed off the segment text), the operator bytes, and trailing whitespace — so `join(text + trailing_op_span)` reproduces the original input exactly. Asserted in every one of the 13 scenario tests.
- Honored D-09 / REQ-adapter-pipes-passthrough: a single `|` (and single `&`) is consumed verbatim into the current segment; only `&&` / `||` / `;` at top level split. S10 (`a | b && c` → `["a | b", "c"]`) is the regression lock.
- Shipped the authoritative test gate: 13-scenario matrix (S1, S2a/b/c, S3–S13) + 2 pathological-input throughput tests in `tests/chain_split.rs` (17 `#[test]` total), plus 4 inline smoke tests and a doctest. All green; pathological cases finish in <1ms (release).

## Task Commits

Each task committed atomically following the TDD RED→GREEN cycle:

1. **Task 1 (RED): failing 13-scenario matrix** - `19b2bbc` (test) — chain.rs scaffold with `todo!()`, `pub mod chain` in lib.rs, 17 failing tests.
2. **Task 2 (GREEN): chain splitter DFA** - `c165a7f` (feat) — replaced `todo!()` with the DFA; all 17 matrix + 4 inline + 1 doctest green.

No REFACTOR commit: the GREEN implementation is clean as written (helper-function decomposition, no duplication to extract).

## Files Created/Modified

- `crates/lacon-adapter-claudecode/src/chain.rs` (created) — `ChainOp` enum, `Segment` struct, `split_chain` DFA, `SplitState`/`HeredocCtx` internals, `scan_heredoc_delimiter` / `push_segment` / `consume_op_span` / `set_last_span` helpers, 4 inline smoke tests.
- `crates/lacon-adapter-claudecode/tests/chain_split.rs` (created) — 15 scenario tests + 2 pathological tests with a `reassemble()` helper asserting the byte-exact invariant per scenario.
- `crates/lacon-adapter-claudecode/src/lib.rs` (modified) — surgical: replaced the commented `// pub mod chain;` placeholder with `pub mod chain;` (left tui/quote commented for Plan 03-03).

## Decisions Made

- **8th DFA field wired (`process_sub_depth`).** 03-RESEARCH.md:510 calls for an 8th state field for `<(...)`/`>(...)` opacity even though CONTEXT D-06's enumeration omits it; implemented and grep-verified (`process_sub_depth` appears 7×).
- **Heredoc S11 fixture taken at full-body fidelity, not the opaque-until-EOL fallback.** The chosen fixture `cat <<EOF\na && b\nEOF` is matched by genuine delimiter-line tracking (delimiter captured, body opaque until a line equal to `EOF`), so a heredoc body containing `&&` correctly yields 1 segment. The 03-RESEARCH.md:534 simplification was available but not needed.
- **`set_last_span` moves trailing whitespace from `segment.text` into the span.** This is the mechanism that makes `text + span` byte-exact: the segment text ends at the last non-whitespace byte before the operator, and the span carries `<leading ws><op><trailing ws>`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Plan acceptance-count inconsistency] Test count is 17, not 15**
- **Found during:** Task 1
- **Issue:** The plan's acceptance criteria states "13 scenario tests plus 2 pathological-input tests (15 total)" and `grep -c '#\[test\]'` returns 15, but the same task's action body explicitly enumerates S2 as three separate `#[test]` rows (S2a `&&`, S2b `||`, S2c `;`) and names tests `s1_…` through `s13_…`. Counting the action's explicit scenario list gives 15 scenario tests + 2 pathological = 17.
- **Fix:** Implemented the action body's explicit enumeration (the authoritative, more-complete spec): S1, S2a, S2b, S2c, S3, S4, S5, S6, S7, S8, S9, S10, S11, S12, S13 (15 scenario tests) + 2 pathological = 17 `#[test]` attributes. The acceptance "15" lines treated S2's three variants as one row — an off-by-2 in the count, not in the required coverage.
- **Files modified:** `crates/lacon-adapter-claudecode/tests/chain_split.rs`
- **Commit:** `19b2bbc`

## Issues Encountered

- Pre-existing clippy warnings in `lacon-core` (lib) — 4 warnings (collapsible-if ×2, overindented doc list, manual case-insensitive ASCII compare). These are in code Plan 03-02 did not touch (out of scope per the SCOPE BOUNDARY rule). The adapter crate, including the new `chain.rs`, generates zero clippy warnings. Logged here for transparency; not fixed.

## User Setup Required

None — pure library code, no external service or config.

## Next Phase Readiness

- Plan 03-03 (tui/quote) and Plan 03-04 (hook orchestration) can call `lacon_adapter_claudecode::chain::split_chain` directly. The reassembly contract (`Segment.trailing_op_span`) is the property Plan 03-04 relies on to rebuild the chain after wrapping matched segments in `lacon run --rule <id> -- <inner>`.
- `lib.rs` still has `tui`/`quote` commented out for Plan 03-03 — the chain edit was kept surgical so the parallel/next plan's lib.rs edits do not conflict.

## TDD Gate Compliance

- RED gate: `19b2bbc` (`test(03-02): add failing 13-scenario chain split matrix`) — all 17 tests failing on `todo!()`.
- GREEN gate: `c165a7f` (`feat(03-02): implement chain splitter DFA`) — all 17 + inline + doctest green.
- Gate sequence (test → feat) verified in `git log`.

## Self-Check: PASSED

All created files exist on disk; both task commits (`19b2bbc`, `c165a7f`) present in git history.

---
*Phase: 03-claude-code-adapter-lacon-init*
*Completed: 2026-05-21*
