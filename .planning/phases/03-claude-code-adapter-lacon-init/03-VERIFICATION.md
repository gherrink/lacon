---
phase: 03-claude-code-adapter-lacon-init
verified: 2026-05-21T22:30:00+02:00
status: passed
score: 9/9 must-haves verified
overrides_applied: 0
re_verification: false
---

# Phase 3: Claude Code Adapter + `lacon init` Verification Report

**Phase Goal:** A user can run `lacon init` in a fresh project and have the Claude Code `PreToolUse` hook installed, the `.lacon/` skeleton created, and a CLAUDE.md instruction line added — and from then on every Bash tool invocation that matches a rule is rewritten to `lacon run --rule <id> -- <inner-cmd>` (or whole-chain bypassed when interactive or user-bypassed), reassembled with original operators preserved.
**Verified:** 2026-05-21T22:30:00+02:00
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| #  | Truth                                                                                                                  | Status     | Evidence                                                                                                                                                                          |
|----|------------------------------------------------------------------------------------------------------------------------|------------|-----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| 1  | `lacon init` creates `.lacon/` skeleton, `.claude/settings.json` with PreToolUse Bash hook, CLAUDE.md block          | ✓ VERIFIED | Live smoke: `target/debug/lacon init` in fresh tempdir created all three; `settings.json` contains `"command": "lacon-claude-hook"` under matcher=Bash; CLAUDE.md has `<!-- lacon:start -->` |
| 2  | Re-running `lacon init` produces byte-stable output (idempotency)                                                     | ✓ VERIFIED | `init_is_idempotent` e2e test asserts `assert_eq!(settings_v1, settings_v2)` and CLAUDE.md content-equality; `init_re_runs_drop_old_lacon_entries` asserts exactly one `lacon-claude-hook` entry after drift |
| 3  | Every Bash tool invocation that matches a rule is rewritten to `lacon run --rule <id> -- <inner-cmd>`                 | ✓ VERIFIED | `matched_single_command_emits_rewrite_json` e2e passes; lib.rs line 214: `"LACON_ASSISTANT=claude-code LACON_SESSION_ID={} LACON_TOOL_USE_ID={} lacon run --rule {} -- {}"` |
| 4  | `!!` prefix and `LACON_DISABLE=1` bypass filtering (whole-command granularity, empty stdout + exit 0)                 | ✓ VERIFIED | `bypass_via_bang_prefix_emits_empty_stdout` and `bypass_via_lacon_disable_env_emits_empty_stdout` e2e pass; `detect_bypass` uses `as_deref() == Ok("1")` — exact match only |
| 5  | Chained commands split at `&&`/`||`/`;`, each matched segment wrapped independently, original operators preserved      | ✓ VERIFIED | 19 chain_split scenarios all pass (including S14/S14b for `${...}` opacity); `chain_with_one_matched_one_unmatched_emits_chain_rewrite` e2e asserts ` && ls -la` preserved; byte-exact reassembly via `trailing_op_span` |
| 6  | TUI segment in any chain position triggers whole-chain bypass                                                          | ✓ VERIFIED | `tui_segment_in_chain_triggers_whole_chain_bypass` e2e: `vim file && ls -la` with ls rule → empty stdout + exit 0; 52 tui_heuristic tests (22 pure-TUI + conditional + negative) |
| 7  | Pipe (`\|`) and other shell-active constructs pass through byte-exact (not split, not wrapped)                         | ✓ VERIFIED | `pipe_in_segment_preserved_not_split` e2e passes; `is_wrap_safe` allowlist rejects `\|` as not a safe literal byte; `brace_expansion_segment_passes_through_unwrapped` confirms the allowlist posture |
| 8  | Chain reassembly is byte-exact: operators and unmatched segments preserve the original text                            | ✓ VERIFIED | Every chain_split scenario test asserts `segment.text + trailing_op_span.unwrap_or_default()` reconstruction equals original input; e2e `chain_with_one_matched_one_unmatched_emits_chain_rewrite` asserts both wrapped and literal segments appear |
| 9  | 6-command CLI surface remains intact (no `lacon hook` subcommand)                                                      | ✓ VERIFIED | `cli_surface_exposes_exactly_six_subcommands` and `unknown_subcommand_rejected_with_nonzero_exit` pass; `lacon-claude-hook` is a separate binary (D-01), not a CLI subcommand |

**Score:** 9/9 truths verified

### Required Artifacts

| Artifact                                                          | Expected                                              | Status     | Details                                                                              |
|-------------------------------------------------------------------|-------------------------------------------------------|------------|--------------------------------------------------------------------------------------|
| `crates/lacon-adapter-claudecode/Cargo.toml`                      | `[[bin]] lacon-claude-hook`, D-02 dep set             | ✓ VERIFIED | Deps: lacon-core, serde, serde_json, anyhow only; no rusqlite/starlark/os_pipe; binary at `src/bin/hook.rs` |
| `Cargo.toml` (workspace root)                                     | `serde_json = "1.0.149"` workspace dep                | ✓ VERIFIED | Line 16: `serde_json = "1.0.149"`                                                   |
| `crates/lacon-adapter-claudecode/src/protocol.rs`                 | `HookInput`, `BashToolInput`, `build_rewrite_response` | ✓ VERIFIED | 7,438 bytes; 3 inline round-trip tests; `skip_serializing_if = "Option::is_none"` on optional fields |
| `crates/lacon-adapter-claudecode/src/chain.rs`                    | `split_chain`, `is_wrap_safe`, DFA                    | ✓ VERIFIED | 852 lines; `is_wrap_safe` at line 425; positive allowlist replaces old denylist (CR-01 root-cause fix) |
| `crates/lacon-adapter-claudecode/tests/chain_split.rs`            | 13+ scenario tests + pathological inputs              | ✓ VERIFIED | 20 `#[test]` functions (19 scenarios including S14/S14b + pathological); all pass   |
| `crates/lacon-adapter-claudecode/src/tui.rs`                      | `is_tui`, `PURE_TUI` 22-entry list                    | ✓ VERIFIED | 182 lines; `PURE_TUI` at line 20; conditional dispatchers for git/npm/node/db       |
| `crates/lacon-adapter-claudecode/tests/tui_heuristic.rs`          | 34+ tests covering all TUI patterns                   | ✓ VERIFIED | 52 `#[test]` functions; pure-TUI, conditional, negative, path-stripping              |
| `crates/lacon-adapter-claudecode/src/quote.rs`                    | `quote_for_shell`, POSIX round-trips                  | ✓ VERIFIED | `METACHARS` includes all required chars; 11 inline tests including `$(rm -rf /)`    |
| `crates/lacon-core/src/rules/rewrite.rs`                          | `apply_rewrite`, idempotency                          | ✓ VERIFIED | 11 inline tests; T3 idempotency, T10 argv[0]-preservation confirmed                 |
| `crates/lacon-adapter-claudecode/src/lib.rs`                      | Full `run_hook` orchestration                         | ✓ VERIFIED | 18,027 bytes; all 8 key symbols present: `detect_bypass`, `split_chain`, `is_tui`, `is_wrap_safe`, `apply_rewrite`, `quote_for_shell`, `match_argv_via_load_all`, `LACON_ASSISTANT=claude-code` literal |
| `crates/lacon-adapter-claudecode/src/bin/hook.rs`                 | Thin binary entry, stdin→run_hook→stdout              | ✓ VERIFIED | 1,404 bytes; `serde_json::from_reader`; `HookOutcome::Rewrite` → JSON write + `\n`; `PassThrough` → exit 0 empty stdout |
| `crates/lacon-adapter-claudecode/tests/hook_e2e.rs`               | 11+ e2e tests covering full requirement matrix        | ✓ VERIFIED | 22 `#[test]` functions; all 11 required scenario tests present including brace-expansion regressions |
| `crates/lacon-core/src/rules/loader.rs`                           | `pub fn match_argv_via_load_all`                      | ✓ VERIFIED | Line 332; promoted from lacon-cli; empty-argv returns `Ok(None)`                   |
| `crates/lacon-core/src/rules/mod.rs`                              | Re-exports `match_argv_via_load_all`, `pub mod rewrite` | ✓ VERIFIED | Line 5: `pub mod rewrite`; Line 8: `match_argv_via_load_all` in re-export           |
| `crates/lacon-cli/src/commands/init.rs`                           | Full `execute()`, helpers, idempotency                | ✓ VERIFIED | 474 lines; `lacon-claude-hook` fingerprint, `NamedTempFile`, `lacon:start`, `LACON_DISABLE`, no stub message |
| `crates/lacon-cli/tests/cli_init.rs`                              | 4+ e2e tests for create/idempotent/preserve/drift     | ✓ VERIFIED | 6 `#[test]` functions including permissions preservation and orphan-marker recovery  |

### Key Link Verification

| From                                          | To                                              | Via                                           | Status     | Details                                                              |
|-----------------------------------------------|-------------------------------------------------|-----------------------------------------------|------------|----------------------------------------------------------------------|
| `lacon-adapter-claudecode/src/lib.rs`         | `chain::split_chain`                            | `crate::chain::split_chain` at line 141       | ✓ WIRED    | Direct call in run_hook orchestration                                |
| `lacon-adapter-claudecode/src/lib.rs`         | `chain::is_wrap_safe`                           | `crate::chain::is_wrap_safe` at line 189      | ✓ WIRED    | Allowlist gate before every wrap attempt                             |
| `lacon-adapter-claudecode/src/lib.rs`         | `tui::is_tui`                                   | `crate::tui::is_tui` at line 150             | ✓ WIRED    | Per-segment TUI check before rule resolution                         |
| `lacon-adapter-claudecode/src/lib.rs`         | `quote::quote_for_shell`                        | `crate::quote::quote_for_shell` at line 201   | ✓ WIRED    | Applied to each argv token in wrap path                              |
| `lacon-adapter-claudecode/src/lib.rs`         | `lacon_core::rules::apply_rewrite`              | import line 15                                | ✓ WIRED    | Applied to matched segment's argv before quoting                     |
| `lacon-adapter-claudecode/src/lib.rs`         | `lacon_core::rules::match_argv_via_load_all`    | import line 15, call at line 194              | ✓ WIRED    | Per-segment rule resolution                                          |
| `lacon-adapter-claudecode/src/lib.rs`         | `protocol::build_rewrite_response`              | called when at least one segment wrapped      | ✓ WIRED    | Emits `hookSpecificOutput` JSON with `hookEventName: PreToolUse` + `permissionDecision: allow` |
| `lacon-adapter-claudecode/src/bin/hook.rs`    | `lacon_adapter_claudecode::run_hook`            | library call                                  | ✓ WIRED    | stdin parse → `run_hook` → stdout write or empty exit 0             |
| `crates/lacon-core/src/rules/mod.rs`          | `loader::match_argv_via_load_all`               | `pub use loader::match_argv_via_load_all`     | ✓ WIRED    | Re-exported for adapter and other dependents                         |
| `crates/lacon-core/src/rules/mod.rs`          | `rewrite::apply_rewrite`                        | `pub mod rewrite` + `pub use`                 | ✓ WIRED    | Exported from lacon-core::rules                                      |
| `crates/lacon-cli/src/commands/init.rs`       | `.claude/settings.json`                         | `serde_json::Value` walk + `NamedTempFile::persist` | ✓ WIRED | Scrub-then-reinsert installs `lacon-claude-hook` under matcher=Bash  |
| `crates/lacon-cli/src/commands/init.rs`       | `CLAUDE.md`                                     | `<!-- lacon:start -->` marker scan            | ✓ WIRED    | In-place replace or EOF append; mentions `!!` and `LACON_DISABLE=1` |

### Data-Flow Trace (Level 4)

| Artifact                                           | Data Variable        | Source                                    | Produces Real Data | Status     |
|----------------------------------------------------|----------------------|-------------------------------------------|--------------------|------------|
| `lacon-adapter-claudecode/src/lib.rs::run_hook`   | `input.tool_input.command` | `serde_json::from_reader(stdin)` — real Claude Code JSON | Yes      | ✓ FLOWING  |
| `run_hook` → `split_chain`                        | `segments`           | live DFA over command string              | Yes                | ✓ FLOWING  |
| `run_hook` → `match_argv_via_load_all`            | `resolved`           | `RuleLoader::load_all` reads real rule files from `input.cwd` | Yes | ✓ FLOWING |
| `run_hook` → `build_rewrite_response`             | `hookSpecificOutput` | constructed from real session_id, tool_use_id, wrapped command | Yes | ✓ FLOWING |
| `init.rs::install_lacon_hook`                     | `settings`           | `serde_json::from_str` of real `settings.json` content | Yes | ✓ FLOWING |

### Behavioral Spot-Checks

| Behavior                              | Command                                                                  | Result                      | Status  |
|---------------------------------------|--------------------------------------------------------------------------|-----------------------------|---------|
| Pass-through: no matching rule        | `echo '...{"command":"echo hi"}...' \| lacon-claude-hook`               | exit 0, empty stdout        | ✓ PASS  |
| `!!` bypass                           | `echo '...{"command":"!! pnpm test"}...' \| lacon-claude-hook`          | exit 0, empty stdout, 0 bytes | ✓ PASS |
| `lacon init` creates skeleton         | `lacon init` in fresh tempdir                                            | `.lacon/.gitkeep`, `.claude/settings.json`, `CLAUDE.md` created | ✓ PASS |
| `cargo build --workspace`             | full workspace build                                                     | Finished in 0.79s, 0 errors | ✓ PASS  |
| `cargo test --workspace`              | 393 tests                                                                | 0 failed                    | ✓ PASS  |

### Requirements Coverage

| Requirement                   | Source Plan | Description                                                                                                            | Status       | Evidence                                                                                                      |
|-------------------------------|-------------|------------------------------------------------------------------------------------------------------------------------|--------------|---------------------------------------------------------------------------------------------------------------|
| REQ-adapter-pretooluse-only   | 03-01, 03-04 | Only PreToolUse hook installed; no PostToolUse; hook rewrites via `hookSpecificOutput.updatedInput`                   | ✓ SATISFIED  | `hook_e2e.rs::matched_single_command_emits_rewrite_json`; `non_bash_tool_passes_through`; no PostToolUse hook registered |
| REQ-adapter-bypass-detection  | 03-04        | `!!` prefix and `LACON_DISABLE=1` bypass whole command                                                                 | ✓ SATISFIED  | `detect_bypass` in lib.rs; two hook_e2e bypass tests; `as_deref() == Ok("1")` exact match                    |
| REQ-adapter-chained-commands  | 03-02, 03-04 | Split at `&&`/`||`/`;`, NOT at `\|`; opaque constructs suppressed; 13-scenario matrix; byte-exact reassembly          | ✓ SATISFIED  | 19 chain_split tests (incl. S14/S14b `${...}` fix); `chain_with_one_matched_one_unmatched_emits_chain_rewrite` |
| REQ-adapter-tui-bypass        | 03-03, 03-04 | `is_tui` per-segment before resolve; any TUI segment → whole-chain bypass; 22-entry PURE_TUI list + conditional patterns | ✓ SATISFIED | 52 tui_heuristic tests; `tui_segment_in_chain_triggers_whole_chain_bypass` e2e                               |
| REQ-adapter-pipes-passthrough | 03-02, 03-04 | Pipes are not split; segment containing `\|` passes through byte-exact (allowlist posture)                             | ✓ SATISFIED  | `pipe_in_segment_preserved_not_split` e2e; `is_wrap_safe` rejects `\|` as unsafe byte; S10 chain_split test  |
| REQ-cli-init                  | 03-05        | `lacon init` creates `.lacon/`, installs PreToolUse(Bash) hook, adds CLAUDE.md note                                   | ✓ SATISFIED  | Live smoke confirms all three; 6 cli_init.rs e2e tests (create/idempotent/preserve/permissions/orphan/drift)  |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| (none found) | — | — | — | All modified files are clean: no TBD/FIXME/XXX debt markers, no `todo!()` bodies, no placeholder implementations |

### Human Verification Required

(None — all must-haves verified programmatically.)

### Gaps Summary

No gaps. All 6 phase requirement IDs are covered. The full workspace test suite passes with 393 tests and zero failures. The `lacon-claude-hook` binary builds and runs correctly. The critical code-review finding (CR-01, shell-expansion neutralization) was resolved via the `is_wrap_safe` allowlist inversion before this verification; the allowlist posture is verified both by unit tests in `chain.rs` and by the brace-expansion e2e regressions in `hook_e2e.rs`.

---

_Verified: 2026-05-21T22:30:00+02:00_
_Verifier: Claude (gsd-verifier)_
