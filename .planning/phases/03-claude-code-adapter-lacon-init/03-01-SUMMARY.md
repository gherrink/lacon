---
phase: 03-claude-code-adapter-lacon-init
plan: 01
subsystem: infra
tags: [adapter, scaffolding, protocol, serde_json, claude-code-hooks, cargo-workspace]

# Dependency graph
requires:
  - phase: 01-engine-core-lacon-run-wrapper
    provides: RuleLoader::load_all + ResolvedRule + RewriteSpec + MatchSpec; lacon-cli try_match_via_load_all matcher
  - phase: 02-local-tracking
    provides: LACON_ASSISTANT / LACON_SESSION_ID env-var contract the adapter will satisfy in Plan 03-04
provides:
  - serde_json as a [workspace.dependencies] entry (pinned 1.0.149)
  - lacon-adapter-claudecode [[bin]] lacon-claude-hook target (D-01) with minimal dep set (D-02)
  - Typed PreToolUse protocol — HookInput + BashToolInput + build_rewrite_response (D-03)
  - lacon-core::rules::match_argv_via_load_all — first-match-wins matcher reachable from any lacon-core dependent (no lacon-cli dep)
  - HookOutcome enum + pass-through run_hook skeleton (Plan 03-04 fills the body)
affects: [03-02 chain splitter, 03-03 tui/quote/apply_rewrite, 03-04 hook orchestration, 03-05 lacon init]

# Tech tracking
tech-stack:
  added: [serde_json 1.0.149 (workspace dep)]
  patterns:
    - "Separate-binary-in-crate via [[bin]] (bin/test_emitter precedent, D-05)"
    - "Typed stdin payload + skip_serializing_if echo-back contract (D-03)"
    - "anyhow at binary boundary, serde structs inside crate"
    - "Promote shared core logic out of lacon-cli so adapters never depend on the CLI"

key-files:
  created:
    - crates/lacon-adapter-claudecode/src/protocol.rs
    - crates/lacon-adapter-claudecode/src/bin/hook.rs
  modified:
    - Cargo.toml
    - crates/lacon-adapter-claudecode/Cargo.toml
    - crates/lacon-adapter-claudecode/src/lib.rs
    - crates/lacon-core/src/rules/loader.rs
    - crates/lacon-core/src/rules/mod.rs
    - crates/lacon-cli/src/commands/run.rs

key-decisions:
  - "serde_json pinned at 1.0.149 in [workspace.dependencies]; adapter + future lacon-cli inherit via { workspace = true }"
  - "Adapter dep set locked to lacon-core + serde + serde_json + anyhow (D-02 cold-start budget); no rusqlite/starlark/os_pipe/regex/etcetera/signal-hook/nix"
  - "HookInput deliberately omits deny_unknown_fields (Claude Code may add fields, T-03-01-01); required fields strictly typed so a missing one is a hard parse error"
  - "BashToolInput uses skip_serializing_if = Option::is_none so updatedInput never injects null for absent optionals (T-03-01-02)"
  - "Matcher promoted to lacon-core::rules::loader; lacon-cli run::execute delegates; empty-argv returns Ok(None) so the adapter can call it without a CLI-boundary gate"

patterns-established:
  - "Pass-through skeleton: run_hook returns HookOutcome::PassThrough unconditionally in 03-01; orchestration body deferred to 03-04 (no decision logic to attack, T-03-01-04)"
  - "Wave-2 modules (chain/tui/quote) left commented in lib.rs with TODO — declaring modules whose files don't exist is a compile error"

requirements-completed: [REQ-adapter-pretooluse-only]

# Metrics
duration: 5min
completed: 2026-05-21
---

# Phase 3 Plan 01: Adapter scaffolding & PreToolUse protocol Summary

**`lacon-claude-hook` binary target + typed `PreToolUse` stdin/stdout protocol (lossless echo-back) + a `lacon-core`-resident first-match-wins matcher, all built on a new `serde_json` workspace dependency.**

## Performance

- **Duration:** ~5 min
- **Started:** 2026-05-21T21:09:40+02:00
- **Completed:** 2026-05-21T21:14:03+02:00
- **Tasks:** 3
- **Files modified:** 6 (2 created, 4 modified)

## Accomplishments
- Added `serde_json = "1.0.149"` to `[workspace.dependencies]` and configured `lacon-adapter-claudecode` with the `lacon-claude-hook` `[[bin]]` target plus the minimal D-02 dep set (lacon-core + serde + serde_json + anyhow only).
- Introduced typed `HookInput` / `BashToolInput` protocol structs that round-trip a real `PreToolUse` payload losslessly, and `build_rewrite_response` that locks the D-03 output shape (`hookEventName: PreToolUse` + `permissionDecision: allow` + `updatedInput`).
- Promoted the rule matcher (`try_match_via_load_all` + `rule_matches_argv`) from `lacon-cli` into `lacon_core::rules::match_argv_via_load_all`, so the adapter never imports from `lacon-cli` (avoids a cold-start/layering regression). `lacon run` behavior preserved byte-for-byte (cli_run.rs green).
- `lacon-claude-hook` builds and runs end-to-end: minimal stdin → exit 0 / empty stdout (cheapest pass-through hot path).

## Task Commits

Each task was committed atomically:

1. **Task 1: serde_json workspace dep + adapter Cargo.toml scaffolding** - `fb2eca4` (chore)
2. **Task 2: Promote rule-matching helper into lacon-core::rules::loader** - `d6a58f9` (refactor)
3. **Task 3: Protocol structs + lib.rs skeleton + bin/hook.rs stub** - `b2bf2dd` (feat)

_Note: Tasks 1 and 2 are manifest/refactor work; the build gate (both binaries compile) is verified at the end of Task 3 per the plan's atomic-wave note. Each task was independently grep- and test-verified before commit._

## Files Created/Modified
- `Cargo.toml` - Added `serde_json = "1.0.149"` to `[workspace.dependencies]`.
- `crates/lacon-adapter-claudecode/Cargo.toml` - `[[bin]] lacon-claude-hook` at `src/bin/hook.rs`; deps locked to lacon-core/serde/serde_json/anyhow; dev-deps assert_cmd/predicates/tempfile.
- `crates/lacon-adapter-claudecode/src/protocol.rs` (created) - `HookInput`, `BashToolInput`, `build_rewrite_response` + 3 round-trip tests.
- `crates/lacon-adapter-claudecode/src/bin/hook.rs` (created) - `lacon-claude-hook` entry: stdin parse → `run_hook` dispatch → pass-through (exit 0) or rewrite-JSON emit.
- `crates/lacon-adapter-claudecode/src/lib.rs` - Dropped `ClaudeCodeAdapterStub`; added `pub mod protocol`, `HookOutcome` enum, pass-through `run_hook` skeleton.
- `crates/lacon-core/src/rules/loader.rs` - Added `pub fn match_argv_via_load_all` + private `rule_matches_argv` + empty-argv unit test.
- `crates/lacon-core/src/rules/mod.rs` - Re-export `match_argv_via_load_all`.
- `crates/lacon-cli/src/commands/run.rs` - Delegate to promoted matcher; removed local copies; dropped now-unused `ValidationError` import.

## Decisions Made
- Chose typed structs (not `serde_json::Value`) for the stdin payload — catches schema drift loudly and makes the echo-back contract type-enforced (D-03 / RESEARCH Pattern 1).
- Left `regex` in `lacon-cli`'s `[dependencies]` — it is still consumed by `cli_validate.rs` integration tests via `regex::Regex::new`. Moving or removing it is out of scope for this plan (see Issues Encountered).

## Deviations from Plan

None - plan executed exactly as written.

## Issues Encountered
- After removing the matcher from `run.rs`, the `regex` crate was no longer used in `lacon-cli/src/` but is still used by `crates/lacon-cli/tests/cli_validate.rs` (via `regex::Regex::new`). Since `regex` lives in `[dependencies]` (not `[dev-dependencies]`), the test crate continues to resolve it; Cargo emits no unused-dependency error. Left as-is per the scope boundary (pre-existing dep, still consumed). `cargo check --workspace` is clean with no warnings.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- The adapter public surface is in place: Plans 03-02 (chain splitter), 03-03 (tui/quote + `apply_rewrite`), and 03-04 (hook orchestration) all plug into `protocol.rs`, `HookOutcome`, and `lacon_core::rules::match_argv_via_load_all` without re-litigating the protocol or the dep graph.
- Per phase planning guidance, Plans 02 and 03 begin Wave 2 immediately after this plan's commit (independent files, no cross-dependency).
- `lacon init` (Plan 03-05) will additionally need `serde_json` + `tempfile` in `lacon-cli`'s `[dependencies]` — the workspace `serde_json` entry added here is the prerequisite.

## Self-Check: PASSED

---
*Phase: 03-claude-code-adapter-lacon-init*
*Completed: 2026-05-21*
