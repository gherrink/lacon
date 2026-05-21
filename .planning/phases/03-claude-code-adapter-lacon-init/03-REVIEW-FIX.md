---
phase: 03-claude-code-adapter-lacon-init
fixed_at: 2026-05-21T20:30:00Z
review_path: .planning/phases/03-claude-code-adapter-lacon-init/03-REVIEW.md
iteration: 4
findings_in_scope: 1
fixed: 1
skipped: 0
status: all_fixed
---

# Phase 3: Code Review Fix Report (iteration 4 — allowlist inversion)

**Fixed at:** 2026-05-21T20:30:00Z
**Source review:** .planning/phases/03-claude-code-adapter-lacon-init/03-REVIEW.md
**Iteration:** 4

**Summary:**
- Findings in scope: 1 (the user-approved architectural fix for the CR-01 root cause)
- Fixed: 1
- Skipped: 0

This iteration is a single, user-approved architectural change: the wrap-gate
predicate that decides whether a matched chain segment may be re-tokenized and
re-quoted into `lacon run --rule <id> -- <argv>` was inverted from a **denylist**
of dangerous shell constructs (`has_unwrappable_construct` + the separate
`has_top_level_pipe` guard) to a positive **allowlist** (`is_wrap_safe`).

## Why the denylist had to go (CR-01 root cause)

When `run_hook` wraps a matched segment it tokenizes it (`argv_for_resolution`),
applies the rule's flag rewrite (`apply_rewrite`, D-19), and re-quotes every
token with `quote_for_shell` (single-quoting). Single-quoting neutralizes EVERY
shell expansion, and the downstream Runner executes
`Command::new(argv[0]).args(...)` with NO shell hop — so there is no place to
faithfully re-emit any expansion bash would have performed.

The denylist tried to enumerate every construct that breaks under that round-trip
and kept missing cases. The most recent miss was **brace expansion** (`{a,b}` /
`{1..10}`): `eslint src/{a,b}.js` was wrapped and silently corrupted into the
literal `'src/{a,b}.js'`. An allowlist instead wraps ONLY segments that are
*provably reproducible* by tokenize→requote and passes everything else through
byte-exact (the fail-safe direction, per `docs/specs/chained-commands.md:17`).

## Fixed Issues

### CR-01 (root cause): wrap gate inverted to `is_wrap_safe` allowlist

**Files modified:** `crates/lacon-adapter-claudecode/src/chain.rs`,
`crates/lacon-adapter-claudecode/src/lib.rs`,
`crates/lacon-adapter-claudecode/tests/hook_e2e.rs`
**Commit:** 2211010
**Status:** fixed
**Applied fix:**

1. **New predicate `is_wrap_safe(segment: &str) -> bool`** (replaces both
   `has_unwrappable_construct` and `has_top_level_pipe`). Single-pass,
   allocation-free, linear-time (preserves the ≤10ms cold-start budget,
   ADR-0013). Returns `true` ONLY when the segment is composed EXCLUSIVELY of:
   - whitespace separators (space, tab);
   - top-level "safe literal" bytes that are inert in the shell AND survive a
     `quote_for_shell` round-trip — ASCII alphanumerics plus `/ . - _ = : @ , + %`
     (via the tiny `is_safe_literal_byte` helper);
   - single-quoted spans `'...'` (always literal/safe; unterminated → unsafe);
   - double-quoted spans `"..."` containing NO `$`, backtick, or backslash (a
     double-quoted *literal* like `"a b"` is safe; `"$HOME"` is NOT; unterminated
     → unsafe).

   Any other top-level byte makes the segment NOT wrap-safe — `$`, backtick,
   `* ? [ ] { }`, `~`, `< >`, `|`, `&`, `;`, `( )`, `#`, `!`, `\`, newline/CR, and
   any control / non-printable / non-ASCII byte. Empty / whitespace-only segments
   are rejected (nothing to wrap).

2. **Call site (`lib.rs`, the wrap path) updated** to the allowlist semantics:
   `if !is_wrap_safe(&segment.text) { pass through byte-exact }`. The single
   allowlist check subsumes the old two-guard combination
   (`has_top_level_pipe || has_unwrappable_construct`): `|` is simply not a safe
   literal byte, so a pipelined segment is no longer wrap-safe.

3. **`has_top_level_pipe` removed entirely** (no remaining callers; the allowlist
   rejects `|`). Its five unit tests were removed and replaced with a documenting
   NOTE plus explicit `is_wrap_safe` pipe-rejection coverage; the
   pipe-passthrough e2e (`pipe_in_segment_preserved_not_split`) remains green.

### Round-trip correctness check (task step 3)

Confirmed that for every segment `is_wrap_safe` accepts, the existing
`argv_for_resolution` tokenizer + `quote_for_shell` round-trips faithfully:
- `argv_for_resolution` already keeps a quoted span's whitespace inside one token
  (`echo "a b"` → `["echo","a b"]`, `echo 'a b'` → `["echo","a b"]`) — it does
  NOT split the quoted span. So quoted spans containing whitespace are safe to
  accept; no tokenizer change and no extra `is_wrap_safe` tightening was needed.
- Bare safe-literal runs split on whitespace into inert tokens; `quote_for_shell`
  re-emits each (single-quoting any that contain `=`/`%`, both inert), so the
  single downstream `bash -c` parse reconstructs the identical argv.
- Adjacent-quote glue (`echo a'b'c` → `["echo","abc"]`) round-trips too.

Because a wrap-safe segment contains no expansion, redirection, or operator, the
requoted argv reproduces the exact same program invocation.

## Tests

**Unit (`chain.rs`)** — replaced the `has_unwrappable_construct` /
`has_top_level_pipe` suites with `is_wrap_safe` ACCEPT/REJECT tables:
- ACCEPT: `cargo build --release`, `pytest -k foo`, `eslint .`,
  `npm run test:unit`, `cmd --features=a,b,c`, `KEY=value cmd`,
  `echo 'literal text'`, `echo "literal text"`, plus quoted-literal-span,
  glued-quote, and inert-punctuation rows.
- REJECT: `echo $HOME`, `echo ${x}`, `echo $(whoami)`, ``echo `id` ``,
  `ls *.rs`, `ls src/{a,b}.js`, `echo {1..10}`, `echo ~`, `echo hi > out.txt`,
  `a | b`, `a & b`, `a # c`, `echo "$HOME"`, `echo a\ b`, plus
  unterminated/unsafe quoted spans, non-ASCII/control bytes, and
  empty/whitespace-only segments.

**e2e (`hook_e2e.rs`)** — added the brace-expansion regressions the denylist
missed:
- `brace_expansion_segment_passes_through_unwrapped` — `eslint src/{a,b}.js` and
  `eslint {1..10}.js` now pass through UNWRAPPED.
- `brace_expansion_segment_preserved_while_sibling_wrapped` — `cargo build {a,b}`
  is preserved byte-exact while the sibling `echo done` is still WRAPPED
  (per-segment posture intact).
- The prior CR-01..CR-04 / WR-02 pass-through scenarios (redirections,
  command/process substitution, comments, `${...}`, escaped whitespace, bare
  `$VAR`, globs, `~`) all still pass through unwrapped, and the plain matched
  commands (`matched_single_command_emits_rewrite_json`,
  `chain_with_one_matched_one_unmatched_emits_chain_rewrite`) still WRAP.

**chain reassembly (`chain_split.rs`)** — untouched; the byte-exact reassembly
matrix (19 tests) remains green (`is_wrap_safe` only changes wrap-vs-passthrough,
not how `split_chain` produces segments).

## Verification

- `cargo test -p lacon-adapter-claudecode`: all green — 52 lib unit tests
  (incl. 13 new `wrap_safe_*` tests), 19 `chain_split`, 22 `hook_e2e` (incl. the
  2 new brace-expansion regressions), 51 `tui_heuristic`, 1 doc-test.
- `cargo test --workspace`: the only failures are the 5 pre-existing
  `crates/lacon-cli/tests/end_to_end.rs` tests that panic with
  `CARGO_BIN_EXE_test_emitter is unset` (an `assert_cmd` fixture issue in a crate
  this fix does not touch). Verified identical failures on the pre-fix base
  commit (6daa9a9), confirming they pre-date and are independent of this work.
- `cargo clippy --workspace --all-targets`: zero warnings in
  `lacon-adapter-claudecode` (either edited file). The remaining warnings are
  all in `lacon-core` / `lacon-cli` (outside the critical_warning fix scope and
  not introduced by this change).
- Hot-path posture (ADR-0013, ≤10ms cold start): `is_wrap_safe` is a single-pass
  byte scan with one tiny inlined `matches!` helper and no allocations — strictly
  cheaper than the removed denylist DFA (no `SplitState`, no depth tracking).
- Byte-exact chain-reassembly invariant preserved: pass-through segments are
  emitted via `segment.text`, reassembly via `trailing_op_span` is unchanged.

## Skipped Issues

None.

---

_Fixed: 2026-05-21T20:30:00Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 4_
</content>
</invoke>
