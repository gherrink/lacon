---
phase: 06-v1-ship-gate-acceptance-docs
reviewed: 2026-05-22T10:05:00Z
depth: standard
files_reviewed: 11
files_reviewed_list:
  - .github/workflows/ci.yml
  - README.md
  - crates/lacon-cli/Cargo.toml
  - crates/lacon-cli/tests/cli_explain.rs
  - crates/lacon-cli/tests/hot_reload.rs
  - crates/lacon-cli/tests/pnpm_e2e.rs
  - crates/lacon-core/benches/tracker_open.rs
  - docs/architecture.md
  - docs/primitive-reference.md
  - docs/worked-example.md
  - scripts/bench-cold-start.sh
findings:
  critical: 0
  warning: 6
  info: 5
  total: 11
status: issues_found
---

# Phase 6: Code Review Report

**Reviewed:** 2026-05-22T10:05:00Z
**Depth:** standard
**Files Reviewed:** 11
**Status:** issues_found

## Summary

Phase 6 is a validation + docs ship gate: three acceptance tests, a criterion
budget gate, a wall-clock bench entry-point shell script, a hermetic GitHub
Actions workflow, and three Markdown docs. I verified each test's behavioral
claims against the actual `lacon` source (`explain.rs`, `run.rs`, `validate/mod.rs`,
the adapter's wrap form, the rule matcher's basename reduction, and the runtime's
trailing-newline emission) rather than trusting the in-test comments.

The good news first, since the adversarial stance demands proof either way: the
hermeticity invariants hold. The XDG redirection helpers in all three test files
correctly point `XDG_DATA_HOME`/`XDG_CONFIG_HOME` at tempdirs; `pnpm_e2e_real` is
genuinely `#[ignore]`d; the CI workflow installs no pnpm/vitest/cargo-tools/system
SQLite and pins `actions/checkout@v4`. The worked-example's claim that `lacon
validate` catches invalid regex / unknown primitive / circular extends / missing
Starlark file is accurate — `validate_rule` in `validate/mod.rs` wires all four
checks (parse_one, flatten_extends_with_lookup, compile_resolved). The
primitive-reference `max_bytes` byte-count and the rule-matcher basename reduction
(`loader.rs:353`) both check out.

No BLOCKER-class defects (no security holes, no data-loss, no real-user-state
mutation, no crash paths). The findings are correctness/maintainability risks: a
stale CI comment whose stated rationale contradicts the now-present dev-deps, two
docs claims that overstate or slightly misstate engine behavior, a couple of
fragile test assertions, and several smaller accuracy nits in the docs.

## Warnings

### WR-01: CI comment's bin-resolution rationale is stale and self-contradicting

**File:** `.github/workflows/ci.yml:57-67`
**Issue:** The comment justifying the extra `cargo build --workspace` step claims
the lacon-cli integration tests resolve helper bins "via `assert_cmd::cargo::cargo_bin`,
which falls back to `target/debug/<name>` when `CARGO_BIN_EXE_<name>` is unset (it
is, for cross-package bins on stable)." That rationale is no longer true for the
two bins it names. `crates/lacon-cli/Cargo.toml:27,33` now declares **both**
`test_emitter` and `lacon-adapter-claudecode` as dev-dependencies, which makes
cargo set `CARGO_BIN_EXE_test_emitter` and `CARGO_BIN_EXE_lacon-claude-hook` for
the lacon-cli test target (the Cargo.toml comment at lines 28-32 says exactly this).
So `CARGO_BIN_EXE_<name>` **is** set, and the "unset → fallback to target/debug"
premise is wrong. The hazard: a future maintainer who trusts this comment may
conclude the dev-deps are redundant and remove them (or remove the `cargo build
--workspace` step), silently breaking bin resolution in CI. The two statements
(CI comment vs Cargo.toml comment) directly contradict each other.
**Fix:** Reconcile the comment with reality. Either (a) drop the
`CARGO_BIN_EXE`-unset claim and justify the debug build solely as "materialize the
release-vs-debug bins the tests resolve" and note that the dev-deps are what
guarantee env-var resolution, or (b) if the debug build is in fact load-bearing
because some test resolves a bin that is NOT a dev-dep of lacon-cli, name that bin
explicitly. As written the comment is provably false for `test_emitter` and
`lacon-claude-hook`.

### WR-02: Worked-example claims `extends` inherits `on_error`, but pkg-install's child would re-run on_error semantics differently than implied

**File:** `docs/worked-example.md:53-56`
**Issue:** The doc states the child rule "inherits `match`, `rewrite`, and
`on_error` from `bundled/pkg-install`" and that "the parent's `pipeline` stages are
*prepended* to yours." Inheritance of scalar/omitted fields is correct per ADR-0012,
but the doc does not mention that the child's two `drop_regex` stages run **only on
the success path**. The bundled `pkg-install` has a distinct `on_error.pipeline`
(`bundled-rules/pkg-install.yaml:37-43`) which the child inherits wholesale; the
child's extra `drop_regex` stages are **not** appended to the inherited `on_error`
pipeline (extends prepends to the success `pipeline` only). A reader following this
worked example to "drop those two lines" will find the lines still appear on a
failed `pnpm install`, contradicting the doc's framing of "keep everything
pkg-install does and additionally drop those two lines." This is a real behavioral
gap between the doc's promise and the engine.
**Fix:** Add a sentence clarifying scope: the appended stages apply to the success
pipeline; the inherited `on_error` pipeline is unchanged, so on a failed command
the two extra lines are not dropped (the inherited error-path filtering governs).
If suppressing them on errors too is desired, the user must also redefine `on_error`.

### WR-03: architecture.md overstates the `lacon hook` cold-start figure context vs the documented 10ms budget

**File:** `docs/architecture.md:199-202`
**Issue:** The table reports `lacon hook passthrough` / `rewrite` at "~12 ms" /
"~13.6 ms" min — i.e. **above** the 10ms cold-start budget that the same doc
(line 156) and `benches/cold_start.rs:190` assert as the hook hot-path contract.
The prose explains this away as "spawn-dominated measurement overhead, not hook
execution" backed by an `strace -c` showing ~0.3ms of real work. That explanation
may be sound, but the doc presents a headline number that **exceeds the stated
budget** in a "ship gate acceptance docs" artifact without the table itself
flagging it. A reader scanning the table sees the hook at 12-13ms against a 10ms
budget and reasonably concludes the gate is failing. The narrative defense is
buried two sentences later.
**Fix:** Annotate the two hook rows in the table directly (e.g. a footnote marker
"† spawn-dominated wall clock, not hook work — see note") so the table is not
self-contradicting at a glance against the 10ms budget. Alternatively report the
strace-derived ~0.3ms in-process figure alongside the wall-clock number so the
comparison to the budget is apples-to-apples.

### WR-04: pnpm_e2e_hermetic asserts a substring that omits the env-var prefix the adapter actually emits

**File:** `crates/lacon-cli/tests/pnpm_e2e.rs:116-119`
**Issue:** The test asserts `rewritten.contains("lacon run --rule pnpm-stub -- {emitter_str} --stdout-lines 3")`. The adapter's real wrap form
(`crates/lacon-adapter-claudecode/src/lib.rs:213-219`) is
`LACON_ASSISTANT=claude-code LACON_SESSION_ID=... LACON_TOOL_USE_ID=... lacon run --rule pnpm-stub -- ...`.
The substring assertion happens to pass because `contains` matches the `lacon run
...` tail, but the test therefore does **not** verify the env-var prefix that is
load-bearing for the Phase 2 tracker contract (LACON_ASSISTANT / LACON_SESSION_ID)
and Phase 4 cross-correlation (LACON_TOOL_USE_ID). A regression that dropped the
env-var prefix entirely would leave this "end-to-end" acceptance test green while
breaking tracking. For an SC2/SC4 acceptance test, asserting only the tail is
weaker than the requirement it claims to cover.
**Fix:** Assert the full wrap form including the env-var prefix, or add a separate
assertion that the rewritten command begins with `LACON_ASSISTANT=claude-code` and
contains `LACON_SESSION_ID=` and `LACON_TOOL_USE_ID=`. This makes the test fail if
the tracker-contract prefix regresses.

### WR-05: hot_reload + pnpm_e2e rules carry a `match:` block that the `--rule` run path never consults — silent dead config that can mask a real matcher regression

**File:** `crates/lacon-cli/tests/hot_reload.rs:67-70,91-96` and `crates/lacon-cli/tests/pnpm_e2e.rs:101-106,126-135`
**Issue:** Both tests `lacon run --rule <id> -- ...` with an explicit `--rule`. Per
`crates/lacon-cli/src/commands/run.rs:28-35`, `--rule` calls `loader.resolve(rule_id)`
directly and **never** runs `match_argv_via_load_all` — the rule's `match:` block is
not consulted on the `lacon run` step. The `match: { command: <emitter_name> }`
lines in these rules are therefore dead config on the `run` path. In `pnpm_e2e` the
match block IS exercised once (the hook step, `pnpm_e2e.rs:113`), but in
`hot_reload.rs` the match block is never exercised at all. This is not a bug today,
but it is misleading: the tests read as if matching is part of what they prove, and
a future engine change that broke `--rule` resolution while leaving matching intact
(or vice versa) would not be caught where a reader expects.
**Fix:** Either drop the unused `match:` block from `hot_reload.rs`'s rules (it
proves nothing there) with a comment that `--rule` bypasses matching, or, better,
add an assertion path that exercises matching so the config is not dead. At minimum
add a comment in `hot_reload.rs` noting the `match:` is inert under `--rule`.

### WR-06: byte-equality test tolerates a trailing blank line by filtering it out, weakening the "byte-for-byte" claim it advertises

**File:** `crates/lacon-cli/tests/cli_explain.rs:280-291`
**Issue:** The test's docstring and assert message claim the filtered column is
verified "byte-for-byte." The comparison at lines 281-287 first strips **all** empty
lines from both sides (`filter(|s| !s.is_empty())`) before `assert_eq!`. The runtime
emits a single trailing newline only when output is non-empty
(`runtime/mod.rs:370-374`), and `split_lines` on a `\n`-terminated buffer yields one
trailing empty element — so there is exactly one expected trailing blank. Dropping
**all** blanks (not just the known single trailing one) means the test would also
pass if the filter spuriously dropped or inserted an interior blank line, or if it
emitted extra trailing blanks. The assertion is weaker than its "byte-for-byte"
billing: it proves the non-blank kept lines match in order, not byte equality of the
full column.
**Fix:** Compare the full column with a single, explicit trailing-blank tolerance
(e.g. `assert_eq!(rendered, expected_with_one_trailing_blank)`), or trim exactly one
trailing empty element from each side rather than filtering every empty line. That
preserves detection of interior-blank drift while tolerating only the one documented
trailing newline.

## Info

### IN-01: bench module-doc cites `migrations.rs:41-43` line numbers that will silently rot

**File:** `crates/lacon-core/benches/tracker_open.rs:18-19,109-110` (also `docs/architecture.md:176`)
**Issue:** Several comments hard-code source line numbers (`migrations.rs:41-43`,
`loader.rs:87-88, 262-274` in `hot_reload.rs:9-10`, `run.rs:270-272` in the adapter).
These drift the moment the referenced files change and there is no compile-time
check binding them.
**Fix:** Reference the function/symbol name (e.g. "`migrate()`'s early-return on
`PRAGMA user_version >= TARGET_VERSION`") rather than a line range, or drop the
line numbers.

### IN-02: README claims `cargo build --release` "produces two binaries" without noting workspace selection

**File:** `README.md:17-21`
**Issue:** `cargo build --release` from the repo root builds the whole workspace,
which includes `cold_start_probe` and `test_emitter` in addition to `lacon` and
`lacon-claude-hook`. The README's "This produces two binaries in `target/release/`"
is technically inaccurate — four bins land there. Minor, but a user copying "both"
to PATH per line 22 may be momentarily confused by the extra artifacts.
**Fix:** Reword to "produces the two binaries you need in `target/release/` (among
other workspace artifacts)" or list the two by name as the relevant ones.

### IN-03: bench BUDGET_MICROS provenance comment is a coincidental sum

**File:** `crates/lacon-core/benches/tracker_open.rs:43-44`
**Issue:** `BUDGET_MICROS = 3_700` is documented as "Phase 1 baseline (1154µs) +
Phase 2 target (2500µs) = 3700µs." 1154 + 2500 = 3654, not 3700; the budget is
rounded up but the comment presents it as an exact sum. Harmless, but the arithmetic
in the comment does not add up and the same "1154 + 2500" framing is repeated in the
assert message (line 170) and `architecture.md:176`.
**Fix:** Note the 3700 is a rounded-up ceiling over the ~3654 sum, or correct the
addends so the stated arithmetic is exact.

### IN-04: bench-cold-start.sh comment claims the probe ERRORS without the hook bin, but the probe only WARNs and skips

**File:** `scripts/bench-cold-start.sh:11-13`
**Issue:** The header comment says the probe "ERRORS without `target/release/lacon`
and SKIPS the hook scenarios without `target/release/lacon-claude-hook`." Only the
first half is true: `benches/cold_start.rs:107-110` errors+exits(1) when `lacon` is
missing, but `cold_start.rs:181-187` prints a `WARN:` and continues (does not error)
when the hook bin is missing. The script's comment phrasing ("ERRORS ... and SKIPS")
reads as if both are error conditions; it is a skip-with-warning, not an error.
**Fix:** Reword to "SKIPS the hook scenarios with a warning" to match the probe's
actual `eprintln!("WARN: ...")` behavior.

### IN-05: primitive-reference and architecture docs both narrate stderr/stdout merge but with slightly different ordering guarantees

**File:** `docs/architecture.md:216-217` vs `crates/lacon-cli/src/commands/explain.rs:11`
**Issue:** `architecture.md` (D-11) describes the live `lacon run` merge as
"best-effort line atomicity, no cross-stream order guarantee" (wall-clock arrival
order). `explain.rs` step 5 (line 106-107) and its doc comment merge stored bytes as
"stdout then stderr" — a deterministic concatenation, not arrival order. These are
intentionally different (live = arrival order; replay = stdout-then-stderr because
arrival order was not preserved in storage), but no doc states that the replayed
`explain` ordering can differ from what the live run actually emitted. For a feature
whose entire selling point is "re-derive what the model saw," this ordering caveat
is worth one sentence so users do not over-trust `explain`'s interleaving on
mixed-stream output.
**Fix:** Add a one-line caveat to the worked-example `lacon explain` section or
architecture D-11 noting that `explain` reconstructs stdout-then-stderr and may not
reproduce the live wall-clock interleaving of the two streams.

---

_Reviewed: 2026-05-22T10:05:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
