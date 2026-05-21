---
phase: 03-claude-code-adapter-lacon-init
plan: 04
subsystem: adapter
tags: [adapter, orchestration, hook, pretooluse, chained-commands, bypass, tui-bypass, pipes-passthrough, cold-start, e2e]

# Dependency graph
requires:
  - phase: 03-claude-code-adapter-lacon-init
    plan: 01
    provides: run_hook skeleton + HookOutcome + protocol structs (HookInput/BashToolInput/build_rewrite_response) + lacon_core::rules::match_argv_via_load_all
  - phase: 03-claude-code-adapter-lacon-init
    plan: 02
    provides: chain::split_chain + Segment + trailing_op_span byte-exact reassembly contract
  - phase: 03-claude-code-adapter-lacon-init
    plan: 03
    provides: tui::is_tui, quote::quote_for_shell, lacon_core::rules::apply_rewrite
  - phase: 02-local-tracking
    provides: LACON_ASSISTANT / LACON_SESSION_ID env-var contract consumed at run.rs:270-272
provides:
  - "Full run_hook orchestration: non-Bash guard → detect_bypass (D-23/24/25) → split_chain → per-segment TUI bypass (D-15) → per-segment resolve → apply_rewrite → quote_for_shell → wrap with D-26 env-var prefix → byte-exact reassembly → emit hookSpecificOutput"
  - "argv_for_resolution(text) — quote-aware secondary tokenizer (D-08 revised; no \$(...) opacity in the resolver tokenizer)"
  - "chain::has_top_level_pipe(segment) — opacity-aware top-level pipe detector; pipelined matched segments preserved byte-exact (pipes out of v1 filter scope)"
  - "lacon-claude-hook end-to-end behavior locked by 11 hook_e2e assert_cmd tests"
  - "cold_start_probe extended: hook passthrough + hook rewrite scenarios with soft 2ms/5ms targets"
affects:
  - "03-05 (lacon init): writes the .claude/settings.json PreToolUse hook entry that invokes this binary"

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Orchestration-by-composition: lib.rs wires Plan 1/2/3 primitives, zero novel algorithm except the resolver tokenizer + pipe detector"
    - "Conservative whole-chain bypass extended from TUI (D-15) to pipelined-matched-segment (chained-commands.md:17) — preserve byte-exact rather than risk wrong semantics"
    - "Cheapest-hot-path short-circuits: non-Bash guard, bypass-detect, all-unmatched → PassThrough (empty stdout, exit 0)"
    - "assert_cmd write_stdin + tempdir(.lacon/rules) + serde_json::from_slice shape assertions for hook e2e"
    - "Test-local Mutex ENV_LOCK to serialize process-global LACON_DISABLE env mutation under parallel cargo test"

key-files:
  created:
    - crates/lacon-adapter-claudecode/tests/hook_e2e.rs
  modified:
    - crates/lacon-adapter-claudecode/src/lib.rs
    - crates/lacon-adapter-claudecode/src/chain.rs
    - benches/cold_start.rs
    - benches/Cargo.toml
    - Cargo.lock

key-decisions:
  - "Pipelined matched segments are NOT wrapped (Rule 1 fix): lacon run executes Command::new(argv[0]).args(...) with no shell hop, so re-quoting a top-level | would make it a literal arg and destroy the pipe. has_top_level_pipe gates this; segment preserved byte-exact per chained-commands.md:17."
  - "bin/hook.rs needed no change — Plan 1 Task 3 already implemented the full rewrite-path JSON emit (lock stdout + serde_json::to_writer + newline)."
  - "ENV_LOCK Mutex serializes the LACON_DISABLE-touching unit tests (Rule 1 flaky-test fix) without adding a serial_test dependency."
  - "Session/tool-use IDs are shell-quoted defensively before inlining into the wrapper (UUIDs in practice, but the field is treated as untrusted)."
  - "D-26 env-var prefix is LACON_ASSISTANT=claude-code LACON_SESSION_ID=<id> LACON_TOOL_USE_ID=<id> (extended per RESEARCH Q3 RESOLVED 2026-05-16) inlined on every wrapped command."

patterns-established:
  - "When a wrapped form cannot honor shell semantics (pipes), prefer byte-exact pass-through over a lossy rewrite — same conservative philosophy as TUI whole-chain bypass."
  - "Cold-start probe hook scenarios are telemetry-not-gate (T-cold-start-regression accepted-with-monitoring); Phase 6 owns the formal acceptance gate."

requirements-completed: [REQ-adapter-pretooluse-only, REQ-adapter-bypass-detection, REQ-adapter-chained-commands, REQ-adapter-tui-bypass, REQ-adapter-pipes-passthrough]

# Metrics
duration: 7min
completed: 2026-05-21
---

# Phase 3 Plan 04: Hook orchestration & end-to-end wiring Summary

**The `lacon-claude-hook` `run_hook` orchestration that composes every Plan 1/2/3 primitive into one PreToolUse rewrite pipeline — bypass-detect → chain-split → per-segment TUI bypass → per-segment rule resolve → rewrite → shell-quote → `lacon run` wrap with the D-26 tracker env-var prefix → byte-exact chain reassembly → `hookSpecificOutput` emit — locked by 11 assert_cmd end-to-end tests and an extended cold-start probe showing ~1ms passthrough / ~1.1ms rewrite medians.**

## Performance

- **Duration:** ~7 min
- **Started:** 2026-05-21T19:28:08Z
- **Completed:** 2026-05-21T19:35:00Z
- **Tasks:** 3
- **Files modified:** 5 (1 created, 4 modified)

## Accomplishments

- **`run_hook` full orchestration (lib.rs).** Replaced the Plan 1 pass-through skeleton with the composed pipeline: a defensive non-Bash guard (RESEARCH Q4), `detect_bypass` for `!!` (D-23) and `LACON_DISABLE=1` (D-24, exact-`"1"` match mirroring runtime/mod.rs:175), whole-command bypass granularity (D-25), `chain::split_chain`, per-segment `tui::is_tui` BEFORE resolve with whole-chain bypass (D-15), one `RuleLoader` per invocation (D-14), per-segment `match_argv_via_load_all` → `apply_rewrite` → `quote_for_shell` → wrap as `lacon run --rule <id> -- <quoted argv>` with the `LACON_ASSISTANT=claude-code LACON_SESSION_ID=<id> LACON_TOOL_USE_ID=<id>` prefix (D-26), and byte-exact reassembly via `trailing_op_span`. All-unmatched chains short-circuit to PassThrough (cheapest hot path). Plus a quote-aware `argv_for_resolution` secondary tokenizer (D-08 revised) with 6 unit tests.
- **11 hook_e2e tests (assert_cmd, tempdir, JSON-shape assertions).** Cover the full requirement+threat matrix: pass-through, single-match rewrite shape (hookEventName/permissionDecision/updatedInput.command), D-03 description/timeout/run_in_background echo-back, chain rewrite with byte-exact operator preservation, `!!` bypass, `LACON_DISABLE=1` bypass, whole-chain TUI bypass (`vim file && ls`), the D-26 LACON_ASSISTANT/LACON_SESSION_ID/LACON_TOOL_USE_ID prefix, pipe-passthrough, and the non-Bash defensive guard.
- **Cold-start probe extended (benches/cold_start.rs).** Added `HOOK_BIN`, `measure_hook` (piped-stdin spawn + wait), `measure_cold_start_hook`, and `run_hook_scenario`; `main()` emits two new rows — measured on Linux at passthrough median ~1029µs (target ≤2000) and rewrite median ~1146µs (target ≤5000). Soft targets only; probe exits 0 on breach (T-cold-start-regression: visibility, Phase 6 owns the gate). Existing `--version`/`validate` rows still print.
- **Closed five requirements at the orchestration level:** REQ-adapter-pretooluse-only, REQ-adapter-bypass-detection, REQ-adapter-chained-commands, REQ-adapter-tui-bypass, REQ-adapter-pipes-passthrough. Threats T-03-04-01..07 mitigated/accepted via the e2e gates + bench.

## Task Commits

Each task was committed atomically:

1. **Task 1: Wire run_hook orchestration in lib.rs** - `f32e2e9` (feat)
2. **Task 2: hook_e2e integration suite + pipe-segment correctness fix** - `37ced1b` (test)
3. **Task 3: Extend cold_start probe with hook scenarios** - `45f2707` (perf)
4. **Style: drop needless reference in has_top_level_pipe** - `8c50ac7` (style)

_TDD note: lib.rs unit tests + e2e tests were authored alongside the orchestration (pure composition over primitives whose full behavior Plans 1/2/3 already enumerated and gated). The pipe-segment bug was caught RED by `pipe_in_segment_preserved_not_split` before the fix landed GREEN — a genuine test-first catch._

## Files Created/Modified

- `crates/lacon-adapter-claudecode/src/lib.rs` - Full `run_hook` body + `detect_bypass` + `argv_for_resolution`; 16 inline unit tests (bypass paths, non-Bash, empty-cmd, env exact-match, argv tokenizer) + ENV_LOCK serialization.
- `crates/lacon-adapter-claudecode/tests/hook_e2e.rs` (created) - 11 end-to-end tests via `assert_cmd::cargo_bin("lacon-claude-hook")`.
- `crates/lacon-adapter-claudecode/src/chain.rs` - Added `pub fn has_top_level_pipe` (opacity-aware) + 6 unit tests.
- `benches/cold_start.rs` - `HOOK_BIN`, `measure_hook`, `measure_cold_start_hook`, `run_hook_scenario`, two scenarios + soft-target println.
- `benches/Cargo.toml` - Added `serde_json` + `tempfile` workspace deps.
- `Cargo.lock` - lacon_benches dep entries for serde_json + tempfile.
- `crates/lacon-adapter-claudecode/src/bin/hook.rs` - No change required (Plan 1 Task 3 already wrote the rewrite-path JSON emit).

## Decisions Made

- **Pipelined matched segments are preserved byte-exact, not wrapped.** See deviation 1 below — this is the load-bearing correctness decision of the plan.
- **`bin/hook.rs` left untouched.** The plan's Task 1 action described replacing the `Rewrite` arm, but Plan 1 Task 3 had already shipped the full implementation (lock stdout, `serde_json::to_writer`, trailing newline). Re-writing it would have been a no-op; verified the existing code matches the spec.
- **Defensive shell-quoting of session_id / tool_use_id.** The IDs are inlined into the wrapper string; even though Claude Code emits UUIDs, the fields are treated as untrusted and passed through `quote_for_shell`.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Pipelined matched segment destroyed pipe semantics when wrapped**
- **Found during:** Task 2 (the `pipe_in_segment_preserved_not_split` e2e test, RED)
- **Issue:** The chain splitter correctly keeps `echo hi | grep h` as ONE segment (pipes never split, D-09). But the orchestrator then tokenized that segment via `argv_for_resolution` → `["echo","hi","|","grep","h"]`, matched the `echo` rule, and `quote_for_shell` quoted the `|` to `'|'`, emitting `lacon run --rule echo-rule -- echo hi '|' grep h`. Because `lacon run` executes `Command::new(&argv[0]).args(&argv[1..])` with NO shell hop (quote.rs / runtime/mod.rs:138-141), the `|`, `grep`, `h` would become literal arguments to `echo` — silently breaking the pipeline.
- **Fix:** Added `chain::has_top_level_pipe(segment)` (reuses the splitter's opacity DFA: quotes / `$(...)` / subshell / backtick / process-sub / heredoc all suppress detection; `||` excluded). In `run_hook`, a segment with a top-level pipe is preserved byte-exact (treated as unmatched) so the shell still sees the real `|`. Aligns with `docs/specs/chained-commands.md:17` ("filtering inside pipes is explicitly out of scope for v1") and the conservative whole-chain-bypass philosophy used for TUI.
- **Files modified:** `crates/lacon-adapter-claudecode/src/chain.rs`, `crates/lacon-adapter-claudecode/src/lib.rs`, `crates/lacon-adapter-claudecode/tests/hook_e2e.rs`
- **Verification:** `has_top_level_pipe` has 6 unit tests (bare pipe, no pipe, quoted pipe, subshell/cmdsub/backtick pipe, `||`); the `pipe_in_segment_preserved_not_split` e2e now asserts the pipe segment leads the chain unwrapped while the `ls` segment is wrapped. All green.
- **Committed in:** `37ced1b` (Task 2 commit)

**2. [Rule 1 - Bug] LACON_DISABLE env-touching unit tests raced under parallel cargo test**
- **Found during:** Task 2 (running the lib suite after Task 1)
- **Issue:** `LACON_DISABLE` is process-global; cargo runs tests in parallel. `detect_bypass_only_exact_one_disables`, `detect_bypass_bang_bang`, and `lacon_disable_env_passes_through` set/read it concurrently, so a test asserting "no bypass" intermittently saw another test's transient `LACON_DISABLE=1`.
- **Fix:** Added a test-local `static ENV_LOCK: Mutex<()>`; every env-touching test acquires it (poison-tolerant via `into_inner`) and removes the var before releasing. No new crate dependency (avoided `serial_test`, which would breach the D-02 dep budget).
- **Files modified:** `crates/lacon-adapter-claudecode/src/lib.rs`
- **Verification:** Lib suite run 3× consecutively, 41/41 green each time.
- **Committed in:** `37ced1b` (Task 2 commit)

**3. [Rule 1 - Bug] Clippy op_ref lint on new heredoc-delimiter comparison**
- **Found during:** Post-task clippy sweep
- **Issue:** `has_top_level_pipe` compared `&segment[..] == ctx.delimiter`, tripping clippy's `op_ref` lint (needless reference of left operand).
- **Fix:** Dropped the `&`. Behavior unchanged.
- **Files modified:** `crates/lacon-adapter-claudecode/src/chain.rs`
- **Verification:** `cargo clippy -p lacon-adapter-claudecode` clean (zero adapter warnings); chain + lib tests still green.
- **Committed in:** `8c50ac7` (style commit)

---

**Total deviations:** 3 auto-fixed (3 Rule 1 bugs).
**Impact on plan:** Deviation 1 is a genuine correctness fix the plan's own behavior spec implied but did not implement (the plan's draft `pipe_in_segment_preserved_not_split` assertion assumed the pipe could be wrapped intact, which is impossible under the no-shell-hop Runner). Deviations 2 and 3 are hygiene. No scope creep; all stay within the adapter crate + bench.

## Issues Encountered

- Pre-existing `lacon-core` (lib) clippy warnings (4: collapsible-if ×2, overindented doc list, manual case-insensitive ASCII compare), unchanged since Plans 02/03 — out of scope per the SCOPE BOUNDARY rule. The adapter crate itself is clippy-clean.

## User Setup Required

None — pure library/test/bench code, no external service or config. (The `.claude/settings.json` hook registration is Plan 03-05's job.)

## Next Phase Readiness

- The `lacon-claude-hook` binary is functionally complete and end-to-end gated. Plan 03-05 (`lacon init`) only needs to write the `.claude/settings.json` `PreToolUse(Bash)` hook entry pointing at this binary; the wire protocol, bypass, chain, TUI, and rewrite behavior are all locked.
- Cold-start telemetry is in place for the hook hot path; Phase 6 owns the formal acceptance gate (REQ-acceptance-cold-start-budget).
- Phase 2 tracker env-var contract (D-26) is satisfied: every wrapped command inlines LACON_ASSISTANT/LACON_SESSION_ID/LACON_TOOL_USE_ID, so tracker rows populate without further adapter work.

## TDD Gate Compliance

- The pipe-segment bug followed a true RED→GREEN cycle: `pipe_in_segment_preserved_not_split` failed (the `'|'` re-quote), then `has_top_level_pipe` made it pass. Other tests were authored alongside their (composition-only) implementation, consistent with Plans 01/03's documented approach for primitive-composition code whose behavior is fully enumerated upstream.

## Self-Check: PASSED

---
*Phase: 03-claude-code-adapter-lacon-init*
*Completed: 2026-05-21*
