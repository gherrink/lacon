# Phase 3: Claude Code adapter & `lacon init` - Research

**Researched:** 2026-05-16
**Domain:** Claude Code `PreToolUse` hook integration; bash chain splitting; POSIX shell quoting; idempotent JSON config editing
**Confidence:** HIGH

## Summary

Phase 3 ships two visible deliverables on top of Phase 1/2: a new `lacon-claude-hook` binary that lives inside `crates/lacon-adapter-claudecode` and implements the `PreToolUse(Bash)` rewrite contract, and a real implementation of `lacon init` that drops `.lacon/`, edits `.claude/settings.json` atomically, and adds a marker-block to `CLAUDE.md`. CONTEXT.md D-01..D-28 lock the architecture; this research focuses on **implementation-pattern depth** the planner needs to slice the phase into atomic plan files with concrete acceptance criteria.

Six load-bearing sub-domains require precise specification: (1) the JSON I/O contract — confirmed verbatim against `code.claude.com/docs/en/hooks`, including a subtle freedom that `serde_json::Value` pass-through is sufficient because we only mutate one field; (2) the chain-splitter DFA — concrete 7-state transition table derived from the 13-scenario test matrix in `docs/specs/chained-commands.md:122-138`; (3) the TUI heuristic — 22 pure-TUI basenames + 8 conditional dispatchers, each with concrete argv-pattern logic; (4) `apply_rewrite` idempotency — 10 regression test cases that lock the invariant `apply(apply(x)) == apply(x)`; (5) `quote_for_shell` POSIX correctness — single-quote-wrap with `'\''` embedded-quote escape, validated against sh/bash/dash/zsh; (6) `.claude/settings.json` atomic write via `tempfile::NamedTempFile::persist` + command-string fingerprint idempotency.

**Primary recommendation:** Plan in 5 plans — (P1) crate scaffolding + workspace `serde_json` dep + protocol structs, (P2) chain splitter + tests, (P3) TUI heuristic + apply_rewrite + quote_for_shell (three pure functions, one plan), (P4) hook orchestration + `lacon-claude-hook` binary + hook_e2e tests, (P5) `lacon init` + idempotent settings.json/CLAUDE.md writers + cli_init tests. Defer the cold-start benchmark wiring into the cold_start_probe binary as a small task inside P4 (extend `benches/cold_start.rs` rather than adding a new harness).

## User Constraints (from CONTEXT.md)

### Locked Decisions

**A. Adapter binary architecture**
- **D-01:** Hook handler ships as a separate binary `lacon-claude-hook` inside `crates/lacon-adapter-claudecode` via a new `[[bin]]` target in that crate's `Cargo.toml`. **NOT** a `lacon hook` subcommand — that would break `crates/lacon-cli/tests/cli_surface.rs:11` which locks the 6-command CLI surface (REQ-cli-surface-cap).
- **D-02:** The hook binary depends only on `lacon-core` + `serde_json` + `serde`. It does NOT pull `rusqlite`, `starlark`, `os_pipe` (those are `lacon` binary's deps). Smaller dep graph → faster cold start on the hot path.
- **D-03:** Hook stdin/stdout protocol — stdin `{session_id, transcript_path, cwd, permission_mode, hook_event_name, tool_name, tool_input, tool_use_id}`; Bash `tool_input.{command, description?, timeout?, run_in_background?}`. Stdout (rewrite) `{"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "allow", "updatedInput": {<full tool_input echo-back>}}}`. Stdout (pass-through) empty + exit 0. `updatedInput` REPLACES the entire input object; `description`/`timeout`/`run_in_background` MUST be carried through when present.
- **D-04:** Adapter Cargo.toml additions: `[[bin]]` for `lacon-claude-hook` at `src/bin/hook.rs`; `serde_json` workspace dep (NEW); `serde` derive (already present workspace dep).
- **D-05:** Precedent for separate-binary-in-crate: `bin/test_emitter/Cargo.toml`.

**B. Chain splitter implementation**
- **D-06:** Hand-rolled state-machine splitter in `crates/lacon-adapter-claudecode/src/chain.rs`. Operates on raw command string (UTF-8 byte iteration with code-point boundary respect). State tracks: single-quote, double-quote, `(...)` subshell depth, `$(...)` cmd-sub depth, backtick depth, heredoc body.
- **D-07:** Output `Vec<Segment>` where `Segment { text: String, trailing_op: Option<ChainOp> }`. Each segment's `text` is the verbatim byte slice from the original input.
- **D-08:** `argv_for_resolution(seg: &str) -> Vec<String>` runs ONLY on segments that need rule resolution. Original `text` preserved when unchanged; matched-and-rewritten segments produce a NEW shell-quoted string via D-20.
- **D-09:** Pipes (`|`) NOT chain operators — consumed verbatim into the current segment.
- **D-10:** 13-scenario test matrix from `docs/specs/chained-commands.md:122-138` is the splitter's test gate.

**C. `lacon init` strategy for `.claude/settings.json`**
- **D-11:** Full JSON parse via `serde_json::Value`. Hook entry inserted/replaced inside `hooks.PreToolUse[]` array-of-matchers shape.
- **D-12:** Idempotency: walk `hooks.PreToolUse[]`; for each matcher-group with `matcher == "Bash"`, filter out inner `hooks[]` entries whose `command` field starts with the substring `lacon-claude-hook`. Re-insert the current desired entry. Command-string itself is the lacon-managed fingerprint.
- **D-13:** File write: 2-space indent, trailing newline. Atomic via `tempfile::NamedTempFile::persist`. Create `.claude/` if missing.
- **D-14:** CLAUDE.md handling: append at the bottom (or create) inside HTML-comment markers `<!-- lacon:start --> ... <!-- lacon:end -->`. Idempotency: detect markers via string scan, replace contents in place; outside content untouched. If neither marker exists, append at EOF.

**D. TUI heuristic**
- **D-15:** `is_tui(command: &str, args: &[String]) -> bool` lives in `crates/lacon-adapter-claudecode/src/tui.rs`. NOT in `lacon-core`.
- **D-16:** Pure-TUI list as `const PURE_TUI: &[&str]` of 22 basenames. Lookup by `basename(args[0])` via `std::path::Path::file_name`.
- **D-17:** Conditional patterns dispatched per command: `git` → `match_git_subcmd`, `npm/yarn/pnpm` → `match_pkg_init`, `node/python/python3` → `is_repl`, `mysql/psql/sqlite3` → `is_repl`.
- **D-18:** Tests:
  - `crates/lacon-adapter-claudecode/tests/chain_split.rs` — 13-scenario matrix
  - `crates/lacon-adapter-claudecode/tests/tui_heuristic.rs` — one test per row
  - `crates/lacon-adapter-claudecode/tests/hook_e2e.rs` — `assert_cmd::Command::cargo_bin("lacon-claude-hook")`
  - `crates/lacon-cli/tests/cli_init.rs` — end-to-end `lacon init` tempdir test

**E. `rewrite` block application & argv re-quoting**
- **D-19:** `lacon_core::rules::rewrite::apply_rewrite(argv, &RewriteSpec) -> Vec<String>` (new file `crates/lacon-core/src/rules/rewrite.rs`). Idempotent. `argv[0]` never touched.
- **D-20:** `quote_for_shell(arg: &str) -> Cow<str>` in `crates/lacon-adapter-claudecode/src/quote.rs`. If no whitespace/metachars (`|&;<>()$\`\\\"'\n\t*?[#~=%!`), return `Cow::Borrowed`. Otherwise single-quote-wrap and replace embedded `'` with `'\''`.
- **D-21:** Adapter emits `lacon run --rule <id> -- <quoted argv joined with single spaces>`. Unchanged segments preserve original `text` byte-exact.
- **D-22:** Security/injection: `quote_for_shell`'s correctness is part of the trust property. Phase 1's Runner already enforces `Command::new(&argv[0]).args(&argv[1..])`. Adapter's quoting only needs to survive ONE shell parse.

**F. Bypass detection**
- **D-23:** `!!` prefix detection: LSTRIP whitespace first, then `starts_with("!!")`. On detect → empty stdout, exit 0.
- **D-24:** `LACON_DISABLE=1` from hook process env. Read via `std::env::var("LACON_DISABLE")`; treat `Ok("1")` (exact string) as bypass.
- **D-25:** Bypass is **whole-command** granularity. When detected, the entire input bypasses — no chain splitting, no rule resolution, no rewrites.

**G. Env-var contract handoff to tracker (Phase 2 integration)**
- **D-26:** Adapter prepends `LACON_ASSISTANT=claude-code LACON_SESSION_ID=<id> lacon run --rule <id> -- <inner>`. Inline; survives Claude Code's shell exec. `tool_use_id` capture into `LACON_TOOL_USE_ID` is left to planner's discretion.
- **D-27:** Phase 2's D-17 contract is satisfied here.

**H. Idempotency resolution**
- **D-28:** Q-deferred-init-idempotency settled: detect lacon-managed entries by **command-string prefix** `starts_with("lacon-claude-hook")`. Strip + re-insert. User-authored non-lacon hooks preserved untouched.

### Claude's Discretion

- Internal module organization under `crates/lacon-adapter-claudecode/src/` — `chain.rs`, `tui.rs`, `quote.rs`, `protocol.rs`, `bin/hook.rs`, `lib.rs`. Planner organizes without re-litigating these boundaries.
- Exact wording of CLAUDE.md instruction line (D-14) — must mention `!!` and `LACON_DISABLE=1`.
- Choice between `serde_json::Value` and typed `#[derive(Deserialize)]` for stdin payload.
- Atomic-write strategy (`tempfile + persist` vs `std::fs::write`).
- Whether to capture `tool_use_id` into `LACON_TOOL_USE_ID` env var (column-correlation property for Phase 4 `lacon explain`).

### Deferred Ideas (OUT OF SCOPE)

- `PostToolUse` annotation of unmatched commands — v1.5 backlog
- Granular per-segment TUI bypass — v2
- User-overridable TUI list — v2
- Cursor/aider adapters — v2
- `_lacon_managed: true` settings.json sibling marker — rejected in favor of command-string fingerprint
- Conch-parser / full bash AST — rejected for v1 on cold-start grounds
- Shlex / shell-words crate deps — rejected
- Adapter trait in `lacon-core` — premature abstraction
- Heredoc/subshell/eval inner-segment filtering — v2

## Project Constraints (from CLAUDE.md)

The repository CLAUDE.md describes the project as "design phase, no code" but Phase 1 and 2 have shipped — the codebase now has `crates/lacon-core`, `crates/lacon-cli`, `crates/lacon-adapter-claudecode` (stub). The following load-bearing directives from CLAUDE.md apply to Phase 3 work:

- **ADRs are source of truth.** If a proposed change contradicts ADR-0001 / ADR-0013 / spec, surface that explicitly. [VERIFIED: `CLAUDE.md` "Treat those ADRs as the source of truth"]
- **Streaming, not buffered (ADR-0005).** Does not directly apply to Phase 3 (no pipeline work here), but the hook process is a one-shot exec and must not introduce buffering or threading that defeats the streaming property of `lacon run` downstream. [VERIFIED]
- **Cold start under 10ms on the hook hot path.** The hook binary is invoked thousands of times per session. New deps must justify cold-start cost. `serde_json` is the only new dep proposed; section "Standard Stack" justifies it. [VERIFIED: `CLAUDE.md` "Cold start under 10ms" + CON-nfr-cold-start-budget]
- **First-match-wins resolution, project > user > bundled.** Adapter uses `RuleLoader::load_all()` (no `--rule` hint from Claude Code) and picks the first matching rule per segment. [VERIFIED: ADR-0007, `loader.rs:127-151`]
- **Claude Code hooks, not PATH shims or shell injection (ADR-0001).** Do not add escape paths that mutate the user's shell env. The env-var prefix in D-26 is a per-invocation prepend, not an env mutation. [VERIFIED]
- **Bypass mechanics:** `!!` prefix or `LACON_DISABLE=1` env var skips filtering entirely. D-23/D-24/D-25 capture this. [VERIFIED]
- **`docs/specs/chained-commands.md` is part of the user contract.** Any change to splitting semantics is a breaking change. [VERIFIED]
- **`docs/specs/filter-rule-schema.md` is part of the user contract.** Adapter's `apply_rewrite` MUST honor the schema's "`add_flags` is idempotent" guarantee (D-19). [VERIFIED]

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| REQ-adapter-pretooluse-only | Adapter installs ONLY `PreToolUse` hook for Bash. Wraps matched commands as `lacon run --rule <id> -- <inner-cmd>` via `hookSpecificOutput.updatedInput`. Unmatched commands returned unchanged. | Confirmed JSON contract verbatim against `code.claude.com/docs/en/hooks`. See "Hook protocol — verified" section + `protocol.rs` design. D-01..D-05 |
| REQ-adapter-bypass-detection | Hook detects `!!` prefix and `LACON_DISABLE=1` env var; bypass returns original command unchanged. | LACON_DISABLE precedent at `crates/lacon-core/src/runtime/mod.rs:175` (CONTEXT cites 157 — off-by-18, actual is 175). D-23/D-24/D-25 + "Bypass detection patterns" section |
| REQ-adapter-chained-commands | Splits at top-level `&&`/`\|\|`/`;` (NOT at `\|`, NOT inside quotes/subshells/cmd-sub/heredocs). 13-scenario test matrix gate. | "Chain splitter DFA" section gives full 7-state transition table + scenario→state mapping. D-06..D-10 |
| REQ-adapter-tui-bypass | `is_tui(command, args)` per-segment AFTER splitting BEFORE rule resolution. Any match → whole-chain bypass. Hardcoded list. | "TUI heuristic implementation" section with 22 pure-TUI table + 8 conditional dispatch functions. D-15..D-17 |
| REQ-adapter-pipes-passthrough | Pipes preserved inside `--` boundary. Filtering inside pipes out of v1 scope. | Implicit in D-09: pipes consumed verbatim into segment text. No separate code path needed. |
| REQ-cli-init | `lacon init` sets up `.lacon/`, configures `PreToolUse(Bash)` hook in `.claude/settings.json`, adds CLAUDE.md note. | "`lacon init` strategy" section. D-11..D-14, D-28 |

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| `PreToolUse` hook stdin parse / stdout emit | Adapter (`lacon-claude-hook` bin) | — | The adapter is the ONLY place that speaks Claude Code's hook protocol; `lacon-core` is assistant-agnostic. D-01/D-02. |
| Chain splitter (DFA on raw command string) | Adapter (`chain.rs`) | — | YAGNI: only Claude Code's `PreToolUse` produces a bash-string-with-chain-operators to split. v2 adapters (Cursor/aider) will likely have different input shapes. CONTEXT explicitly keeps this in adapter. |
| TUI heuristic | Adapter (`tui.rs`) | — | `docs/specs/chained-commands.md:104` is explicit: "The list lives in adapter code." D-15. |
| Shell quoting (`quote_for_shell`) | Adapter (`quote.rs`) | — | Only the adapter emits shell strings; `lacon run` uses argv directly. D-20. |
| `apply_rewrite` (argv-level flag mutation) | Core (`lacon-core::rules::rewrite`) | — | The rewrite block is part of the rule schema (`RewriteSpec`); operating on argv is a pure function over rule data. Belongs next to schema. D-19. |
| Rule resolution (load_all, match) | Core (`RuleLoader::load_all`) | Adapter (caller) | Already implemented in Phase 1 (`crates/lacon-core/src/rules/loader.rs:156-212`). Adapter calls it; doesn't reimplement matching. |
| `lacon init` (filesystem + JSON + markdown) | CLI (`lacon-cli::commands::init`) | — | `init` is a user-facing CLI command; lives in `lacon-cli`. The 6-command-surface-cap test guards against accidentally adding a 7th command. |
| Env-var prefix on rewritten command | Adapter | Core (via tracker consumption) | Adapter synthesizes `LACON_ASSISTANT=... LACON_SESSION_ID=...` prefix; Phase 2's tracker (`crates/lacon-cli/src/commands/run.rs:270-272`) already reads these vars. D-26/D-27. |

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `serde_json` | 1.0.149 | Parse stdin JSON payload (hook input); emit stdout JSON (hook output); read/write `.claude/settings.json` | Ecosystem-standard; aligned with `serde` 1.x already in workspace; **1.6× faster than simd-json on small payloads** (typical hook input is ~300-800 bytes); lower cold-start overhead [VERIFIED: ecton.dev "Surprises in the Rust JSON Ecosystem"] |
| `tempfile` | 3 (workspace) | Atomic `.claude/settings.json` write via `NamedTempFile::persist` | Already in workspace `[dev-dependencies]` per Cargo.toml line 32 — Phase 3 promotes to `[dependencies]` for `lacon-cli`. POSIX rename semantics give atomicity on macOS/Linux (v1 platform support). [VERIFIED: workspace Cargo.toml] |
| `lacon-core` | path dep | `RuleLoader::load_all()`, `RuleSource`, `RewriteSpec`, `ResolvedRule`, plus the NEW `apply_rewrite` function added in Phase 3 | Already a dep of the adapter crate. [VERIFIED: adapter Cargo.toml line 9] |
| `serde` (derive) | 1 (workspace) | If planner picks typed structs over `serde_json::Value` for stdin payload | Already in workspace. [VERIFIED] |

### Supporting (test/dev only)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `assert_cmd` | 2 (workspace) | `Command::cargo_bin("lacon-claude-hook")` end-to-end test pattern | `hook_e2e.rs` and `cli_init.rs`. Phase 1 precedent: `crates/lacon-cli/tests/cli_run.rs:1-8`. [VERIFIED] |
| `predicates` | 3 (workspace) | Stdout/stderr assertions on hook output | Phase 1 precedent. [VERIFIED] |
| `tempfile` | 3 (workspace) | Tempdir for `lacon init` integration tests; also production dep | `cli_init.rs` already uses this pattern. [VERIFIED: `cli_run.rs:5`] |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `serde_json` | `simd-json` | ~3× SLOWER on small payloads + cold-start hit; SIMD wins only above ~10KB. Hook input is ~500B. [CITED: ecton.dev] |
| `serde_json` | `tinyjson` | Smaller binary footprint, but `tinyjson` lacks `Value` mutation ergonomics needed for D-12's "filter `PreToolUse[].hooks[]`, re-insert" walk. Not worth the API churn. [ASSUMED] |
| `serde_json::Value` (Value-based) | typed `#[derive(Deserialize)]` struct | Typed catches schema drift earlier; Value-based is more permissive (Claude Code may add fields). For stdin parse: typed is safer because we MUST echo `description`/`timeout`/`run_in_background` correctly. For `.claude/settings.json`: Value-based is mandatory because the file contains arbitrary user content we must preserve. Recommendation: **typed for stdin, Value for settings.json**. |
| `tempfile::NamedTempFile::persist` | `atomicwrites` crate | Extra dep for what `tempfile` already supports. `persist_noclobber` is the explicit-no-overwrite variant; `persist` (D-13) overwrites the destination — correct for `.claude/settings.json` updates. [CITED: docs.rs/tempfile] |
| Hand-rolled DFA for chain split | `conch-parser` (full bash AST) | Explicitly rejected in CONTEXT Deferred Ideas — too heavy for cold-start budget; full bash is YAGNI when we only need 6 chain-relevant constructs. |
| Hand-rolled `quote_for_shell` | `shlex` / `shell-words` crate | Explicitly rejected in CONTEXT Deferred Ideas — `shlex` is parser/lexer, weak on the quoting side; `shell-words` does quote but adds a dep for ~20 lines of code. CONTEXT D-20 specifies the algorithm verbatim. |

**Installation (adapter Cargo.toml changes):**
```toml
[dependencies]
lacon-core = { path = "../lacon-core" }
serde = { workspace = true }
serde_json = { workspace = true }

[[bin]]
name = "lacon-claude-hook"
path = "src/bin/hook.rs"

[dev-dependencies]
assert_cmd = { workspace = true }
predicates = { workspace = true }
tempfile = { workspace = true }
```

**Installation (workspace root Cargo.toml addition under `[workspace.dependencies]`):**
```toml
serde_json = "1.0.149"
```

**Installation (lacon-cli Cargo.toml addition for `lacon init`):**
```toml
[dependencies]
# existing deps unchanged
serde_json = { workspace = true }
tempfile = { workspace = true }
```

**Version verification (run during plan execution, not now):**
```bash
cargo info serde_json   # confirm 1.0.149 or later
cargo info tempfile     # confirm 3.x latest
```
The 1.0.149 figure is from `cargo search serde_json` on 2026-05-16. The crate is stable; minor-version bumps are non-breaking. [VERIFIED: `cargo search` output]

## Architecture Patterns

### System Architecture Diagram

```
                       ┌────────────────────────────────────────────┐
                       │                Claude Code                  │
                       │                                             │
                       │   user runs `pnpm install --frozen-lockfile`│
                       │                       │                     │
                       │              ┌────────▼────────┐            │
                       │              │ Bash tool fires │            │
                       │              └────────┬────────┘            │
                       │                       │                     │
                       │   stdin JSON payload  │                     │
                       └───────────────────────┼─────────────────────┘
                                               ▼
                            ┌──────────────────────────────────────┐
                            │     lacon-claude-hook (binary)        │
                            │                                       │
                            │   1. parse stdin JSON (serde_json)    │
                            │   2. extract tool_input.command       │
                            │   3. LSTRIP → starts_with("!!")?      │──┐ yes
                            │   4. env LACON_DISABLE=="1"?          │──┤ either
                            │   5. chain-split DFA (chain.rs)       │  │
                            │      → Vec<Segment>                   │  │
                            │   6. for each Segment: is_tui()?      │──┤ any-match
                            │      → ANY true ⇒ whole-chain bypass  │  │
                            │   7. for each Segment: load_all match │  │
                            │      → apply_rewrite to argv          │  │
                            │      → quote_for_shell                │  │
                            │      → wrap as `lacon run --rule ...` │  │
                            │   8. reassemble with original ops     │  │
                            │   9. emit hookSpecificOutput JSON     │  │
                            │      OR exit 0 empty (bypass)         ◄──┘
                            └──────────────────┬───────────────────┘
                                               │
                                stdout JSON / exit code
                                               │
                                               ▼
                            ┌──────────────────────────────────────┐
                            │  Claude Code receives `updatedInput`  │
                            │  shell-execs the rewritten command:   │
                            │                                       │
                            │  LACON_ASSISTANT=claude-code \        │
                            │  LACON_SESSION_ID=<sid> \             │
                            │  lacon run --rule pkg-install -- \    │
                            │    pnpm install --frozen-lockfile \   │
                            │    --reporter=silent                  │
                            └──────────────────┬───────────────────┘
                                               │
                                               ▼
                            ┌──────────────────────────────────────┐
                            │       lacon binary (Phase 1)          │
                            │   - subprocess spawn + stderr merge   │
                            │   - pipeline streaming                │
                            │   - tracker INSERT (Phase 2)          │
                            │   - exit with subprocess exit code    │
                            └──────────────────────────────────────┘
```

Component-to-file mapping:

| Component | File |
|-----------|------|
| Hook binary entry point | `crates/lacon-adapter-claudecode/src/bin/hook.rs` |
| stdin/stdout JSON structs | `crates/lacon-adapter-claudecode/src/protocol.rs` |
| Orchestration (bypass → split → tui → resolve → wrap) | `crates/lacon-adapter-claudecode/src/lib.rs` |
| Chain splitter DFA | `crates/lacon-adapter-claudecode/src/chain.rs` |
| TUI heuristic | `crates/lacon-adapter-claudecode/src/tui.rs` |
| Shell quoting | `crates/lacon-adapter-claudecode/src/quote.rs` |
| `apply_rewrite` | `crates/lacon-core/src/rules/rewrite.rs` (new file) |
| `lacon init` orchestration | `crates/lacon-cli/src/commands/init.rs` (replaces stub) |
| Integration tests | `crates/lacon-adapter-claudecode/tests/{chain_split,tui_heuristic,hook_e2e}.rs` + `crates/lacon-cli/tests/cli_init.rs` |

### Recommended Project Structure
```
crates/
├── lacon-adapter-claudecode/
│   ├── Cargo.toml                  # NEW: serde_json dep + [[bin]] for lacon-claude-hook
│   ├── src/
│   │   ├── lib.rs                  # REPLACES stub; orchestration entry
│   │   ├── protocol.rs             # stdin/stdout JSON structs (typed)
│   │   ├── chain.rs                # DFA splitter
│   │   ├── tui.rs                  # is_tui + conditional dispatchers
│   │   ├── quote.rs                # quote_for_shell
│   │   └── bin/
│   │       └── hook.rs             # `lacon-claude-hook` entry — anyhow::Result<()>
│   └── tests/
│       ├── chain_split.rs          # 13-scenario matrix
│       ├── tui_heuristic.rs        # 22 pure + 8 conditional rows
│       └── hook_e2e.rs             # assert_cmd JSON-in/JSON-out fixtures
├── lacon-core/
│   └── src/rules/
│       ├── rewrite.rs              # NEW: apply_rewrite (D-19)
│       └── (existing files)
└── lacon-cli/
    ├── Cargo.toml                  # ADD: serde_json + tempfile to [dependencies]
    ├── src/commands/init.rs        # REPLACES stub
    └── tests/cli_init.rs           # NEW: end-to-end `lacon init` test
```

### Pattern 1: Hook stdin/stdout (protocol.rs) — typed structs for stdin, Value for output mutation

Stdin payload is fully known and rarely changes; use a typed struct so `serde_json` errors loudly on schema drift. Output is built fresh — typed-builder or direct `Value` construction both work; typed is preferred for clarity.

```rust
// Source: derived from code.claude.com/docs/en/hooks PreToolUse stdin (verified 2026-05-16)
// crates/lacon-adapter-claudecode/src/protocol.rs

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// PreToolUse hook stdin payload (verified against code.claude.com/docs/en/hooks).
#[derive(Deserialize, Debug)]
pub struct HookInput {
    pub session_id: String,
    pub transcript_path: String,
    pub cwd: String,
    pub permission_mode: String,
    pub hook_event_name: String,    // always "PreToolUse" for our hook
    pub tool_name: String,           // we only handle "Bash"
    pub tool_input: BashToolInput,   // Bash-specific
    pub tool_use_id: String,
}

/// Bash-tool-specific input fields. Optional fields MUST be carried through
/// in `updatedInput` (D-03 — "updatedInput REPLACES the entire input object").
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BashToolInput {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_in_background: Option<bool>,
}

/// Build the rewrite-path response. Returns serde_json::Value so the caller
/// can `serde_json::to_writer(stdout, &value)`.
pub fn build_rewrite_response(updated_input: &BashToolInput) -> Value {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "updatedInput": updated_input,
        }
    })
}
```

**Key correctness property:** `BashToolInput`'s `Serialize` derive with `skip_serializing_if = "Option::is_none"` ensures we don't emit `"description": null` when the source omitted it. Claude Code's schema treats explicit null differently from missing fields. [VERIFIED: serde docs on `skip_serializing_if`]

### Pattern 2: Hook binary entry point (bin/hook.rs)

```rust
// Source: pattern derived from Phase 1 D-03 (anyhow at binary boundary, thiserror inside crate)
// crates/lacon-adapter-claudecode/src/bin/hook.rs

use std::io::{self, Write};
use anyhow::Result;
use lacon_adapter_claudecode::{run_hook, HookOutcome};

fn main() -> Result<()> {
    // Bypass-detection hot paths exit BEFORE parsing JSON beyond what's needed,
    // but the cheapest way to share the parse is to do it once and dispatch.
    let input: lacon_adapter_claudecode::protocol::HookInput =
        serde_json::from_reader(io::stdin().lock())?;

    match run_hook(input)? {
        HookOutcome::PassThrough => {
            // exit 0 with empty stdout (D-03 pass-through path).
            Ok(())
        }
        HookOutcome::Rewrite(response) => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            serde_json::to_writer(&mut handle, &response)?;
            handle.write_all(b"\n")?; // newline-terminated for tooling-friendliness
            Ok(())
        }
    }
}
```

### Pattern 3: Atomic file write via tempfile (init.rs)

```rust
// Source: docs.rs/tempfile/3.x — NamedTempFile::persist
// crates/lacon-cli/src/commands/init.rs (excerpt)

use tempfile::NamedTempFile;
use std::path::Path;
use std::io::Write;

fn atomic_write_json(path: &Path, value: &serde_json::Value) -> anyhow::Result<()> {
    let parent = path.parent().expect(".claude/settings.json has a parent");
    std::fs::create_dir_all(parent)?;  // D-13: create .claude/ if missing
    let mut tmp = NamedTempFile::new_in(parent)?;  // same dir → same filesystem → atomic rename
    let bytes = serde_json::to_vec_pretty(value)?;
    tmp.write_all(&bytes)?;
    tmp.write_all(b"\n")?;  // D-13: trailing newline
    tmp.flush()?;
    tmp.persist(path)?;     // POSIX rename(2) — atomic on macOS/Linux
    Ok(())
}
```

`NamedTempFile::new_in(parent)` is critical — if the temp file is created in `/tmp` but the destination is on a different filesystem (e.g., user home on a separate partition), `persist` falls back to a copy+delete which is NOT atomic. Same-directory temp file guarantees same-filesystem rename. [VERIFIED: docs.rs/tempfile NamedTempFile docs]

### Anti-Patterns to Avoid

- **Constructing the rewritten command via `format!("{} {} {} ...", ...)` without `quote_for_shell`.** A single arg containing `$(rm -rf /)` becomes a command injection vector. ALL non-literal args MUST go through `quote_for_shell`. (D-22 mitigates by relying on one-shell-parse + Phase 1's `Command::new(&argv[0]).args(&argv[1..])`, but only because `quote_for_shell` is correct.)
- **Forgetting to drop or close the temp file before persist.** `NamedTempFile::persist` consumes self, so this is enforced at the type level — but if the planner introduces wrappers, lose this property at their peril.
- **Treating LACON_DISABLE other than `"1"` as bypass.** D-24 is explicit: empty/`"0"`/`"true"` do NOT bypass. The runtime precedent at `runtime/mod.rs:175` matches: `std::env::var("LACON_DISABLE").as_deref() == Ok("1")`.
- **Dropping `description`/`timeout`/`run_in_background` from `updatedInput`.** D-03 is explicit: `updatedInput` REPLACES the input object. Any unmodified field MUST be echoed back. `serde(skip_serializing_if = "Option::is_none")` + the typed `BashToolInput` struct enforces this at the type level.
- **Pre-tokenizing the command before chain split.** D-06: the splitter operates on raw bytes. Pre-tokenization throws away quote information the splitter needs to detect opaque regions.
- **Mutating the input object's other fields in passing.** Even though `updatedInput` lets us replace anything, we ONLY change `command`. Leave `description` exactly as received (including escape sequences, leading/trailing whitespace).
- **Detecting "lacon hook entry" in settings.json via path equality.** D-12 is explicit: detect by **command-string prefix** `starts_with("lacon-claude-hook")` because users may write `"command": "$CLAUDE_PROJECT_DIR/.bin/lacon-claude-hook --debug"` or similar. Prefix-match is permissive and idempotent.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JSON parsing | Custom recursive-descent JSON parser | `serde_json` | 6000+ edge cases in the JSON spec; `serde_json` is battle-tested; cold-start cost is negligible for our payload sizes. |
| Atomic file replace | `std::fs::write` (truncate + write) | `tempfile::NamedTempFile::persist` | A `lacon init` running concurrently with a `claude` startup (uncommon but possible) can produce a half-written settings.json with `std::fs::write`. Rename-based atomic replace is the POSIX-standard solution. |
| Bash chain operator detection | Regex (`/&&|\\|\\||;/`) on raw string | Hand-rolled DFA (D-06) | Regex doesn't understand quote/subshell/heredoc state; `echo "a && b"` would incorrectly split. The DFA is small (~150 LOC for the state transitions) and bounded. |
| Argv flag manipulation | Ad-hoc `Vec::retain` + push calls scattered through hook code | `apply_rewrite` (D-19) as a single pure function in `lacon-core::rules::rewrite` | Idempotency invariant must be locked by a regression test; centralizing the logic is the only way to make that test meaningful. |
| Settings.json idempotent edit | "Comment marker" or "sibling marker field" | Command-string prefix detection (D-12, D-28) | JSON has no comments; sibling-marker fields are undocumented schema-tolerance. Command-string prefix is human-readable AND machine-detectable AND survives Claude Code schema tightening. |
| CLAUDE.md block idempotent edit | "Find by heading" or "section marker" | HTML comment markers `<!-- lacon:start --> ... <!-- lacon:end -->` (D-14) | HTML comments survive all markdown renderers; string-scan is O(n) once; no markdown parser dep needed. |
| Shell argument quoting | Manual `format!("\\\"{}\\\"", arg)` (double-quote based) | Single-quote-wrap with `'\''` embedded-quote escape (D-20) | Double-quote inside bash performs `$VAR`, `$(cmd)`, `\\` expansion — leaks shell metacharacters back into the command. Single-quote is fully literal (only `'` is special). |

**Key insight:** The temptation to "just use a regex" for chain-splitting and "just use `std::fs::write`" for settings.json each hide multi-day debug sessions when they fail. Hand-rolling these specific surfaces (DFA, atomic write, prefix-fingerprint) is the cheaper path because each problem has exactly one correct solution and a tiny test surface.

## Hook protocol — verified

The CONTEXT D-03 spec was confirmed verbatim against `https://code.claude.com/docs/en/hooks` on 2026-05-16. Key findings:

| Field | Value | Source |
|-------|-------|--------|
| Required output wrapper | `hookSpecificOutput` (object) | [CITED: code.claude.com hooks docs, "PreToolUse decision control" section] |
| Required event name | `hookEventName: "PreToolUse"` | [CITED] |
| Permission decision values | `"allow" \| "deny" \| "ask" \| "defer"` | [CITED] |
| Pair `updatedInput` with | `permissionDecision: "allow"` | [CITED — D-03 is correct] |
| `updatedInput` semantics | "Modified tool input that replaces the original before execution" — REPLACES, not merges | [CITED] |
| Pass-through semantics | Exit 0 with empty stdout OR exit 0 with JSON lacking `permissionDecision` both mean "allow with no further action" | [CITED — D-03's "empty stdout" choice is correct AND minimal] |
| `additionalContext` | Optional sibling field in `hookSpecificOutput`; NOT used in v1 (reserved for v1.5 unmatched annotation) | [CITED] |

**Surprise/freedom (not in CONTEXT, useful for planner):** The Claude Code docs show a `"if"` field on hook entries (e.g., `"if": "Bash(rm *)"`) for narrower per-pattern filtering. Lacon could use this to narrow `matcher: "Bash"` to specific subpatterns — but D-11 doesn't use it (matcher: "Bash" + first-match-wins is simpler). Planner can ignore this; recording for v2 consideration.

**Surprise/freedom 2:** Hooks support `"type": "command" | "http" | "mcp_tool" | "prompt" | "agent"`. Our adapter uses `"type": "command"`. Confirmed.

**Schema example from settings docs (verbatim shape):**
```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "if": "Bash(rm *)",          // optional, not used by lacon
            "command": "${CLAUDE_PROJECT_DIR}/.claude/hooks/block-rm.sh",
            "args": []                    // optional, not used by lacon
          }
        ]
      }
    ]
  }
}
```

**Existing repo precedent:** `/.claude/settings.local.json` in THIS repo already uses the exact array-of-matchers shape (with PostToolUse, but structurally identical). Planner can use it as a sanity-check fixture. [VERIFIED: `.claude/settings.local.json`]

## Chain splitter DFA — concrete state transition table

The splitter is a single-pass byte-iterating DFA. State is a 7-tuple:

```rust
struct SplitState {
    in_single_quote: bool,           // "'" toggled (no escape inside)
    in_double_quote: bool,           // '"' toggled (escape via \\ inside)
    subshell_depth: u32,             // "(" / ")" balance
    cmd_sub_depth: u32,              // "$(" / ")" balance
    backtick_depth: u32,             // "`" toggled (depth 0/1 only — bash doesn't nest backticks)
    in_heredoc: Option<HeredocCtx>,  // (delimiter, suppress_tabs)
    escape_pending: bool,            // true after backslash in unquoted or double-quoted context
}
```

### State transitions (per input byte)

| Input byte | Context | New state | Emit |
|------------|---------|-----------|------|
| `\\` | escape_pending=false AND in_single_quote=false | escape_pending=true | — |
| any | escape_pending=true | escape_pending=false | — (consume literally) |
| `'` | in_double_quote=false AND in_heredoc=None AND escape_pending=false | in_single_quote=!in_single_quote | — |
| `"` | in_single_quote=false AND in_heredoc=None AND escape_pending=false | in_double_quote=!in_double_quote | — |
| `(` | in_single_quote=false AND in_double_quote=false AND in_heredoc=None AND escape_pending=false AND prev_byte != `$` | subshell_depth += 1 | — |
| `)` | subshell_depth > 0 AND in_single_quote=false AND in_double_quote=false AND in_heredoc=None | subshell_depth -= 1 | — |
| `$(` (2-byte lookahead) | in_single_quote=false AND in_heredoc=None AND escape_pending=false | cmd_sub_depth += 1 | — |
| `)` | cmd_sub_depth > 0 AND in_single_quote=false AND in_heredoc=None (precedence: cmd_sub_depth before subshell_depth) | cmd_sub_depth -= 1 | — |
| `` ` `` | in_single_quote=false AND in_heredoc=None AND escape_pending=false | backtick_depth ^= 1 (toggle) | — |
| `<<` (start-of-token heredoc opener) | at_depth_0 AND not in any quote | start heredoc lookahead → consume delimiter | — |
| `\n` | in_heredoc=Some(d) AND next line == d | in_heredoc=None | — |
| `&&` (2-byte lookahead) | at_depth_0 (all depths == 0, both quotes false, in_heredoc=None) AND escape_pending=false | EMIT split with ChainOp::AndAnd | yes |
| `\|\|` (2-byte lookahead) | at_depth_0 AND escape_pending=false | EMIT split with ChainOp::OrOr | yes |
| `;` | at_depth_0 AND escape_pending=false | EMIT split with ChainOp::Semi | yes |
| `\|` (single) | any | — (NOT a chain op per D-09) | — |
| other | any | — | — |

### Critical edge cases (the planner MUST give the executor)

1. **`$(...)` precedence vs `(...)`.** When seeing `(` immediately after `$`, increment `cmd_sub_depth` instead of `subshell_depth`. When seeing `)`, decrement `cmd_sub_depth` if positive, else `subshell_depth`. The DFA needs a single-byte lookbehind (`prev_byte == b'$'`).
2. **Backslash in single quotes is literal.** `\\` does NOT set `escape_pending` when `in_single_quote=true`. [VERIFIED: POSIX sh + bash + dash; zsh has a known quirk parsing backslashes in single quotes for some history-related cases per shell-escape GitHub issue, but it doesn't affect splitting on `&&`/`||`/`;`. CITED: github.com/sfackler/shell-escape/issues/6]
3. **`\\` inside double quotes escapes only `$`, `` ` ``, `"`, `\\`, `\n`.** For splitting purposes, the only one that matters is `\\\\` (literal backslash) and `\\"` (literal quote inside double-quote context).
4. **Heredoc start detection.** Recognize the patterns `<<DELIM`, `<<-DELIM`, `<<'DELIM'`, `<<"DELIM"`. The opening `<<` must be a token (preceded by whitespace or start-of-string). The delimiter is the next bareword. Strip surrounding quotes if quoted. After the line containing `<<DELIM` ends with `\n`, enter heredoc body mode.
5. **Heredoc body termination.** Match line-start exact `DELIM` (or with leading tabs only for `<<-`). Heredoc body is opaque — `&&` inside is NOT a split.
6. **Backtick command substitution.** Backticks DO NOT nest in bash — they're a flat toggle. (Inside double quotes, a backtick still toggles backtick mode.)
7. **`<<<` is here-string, NOT heredoc.** Treat as opaque single-line. Detect via 3-byte lookahead.
8. **Process substitution `<(...)` / `>(...)` — treat as opaque.** Increment a counter at `<(` or `>(`, decrement at the matching `)`. The CONTEXT D-06 list does NOT enumerate process substitution explicitly, but `docs/specs/chained-commands.md:24` lists it as opaque. Planner: add `process_sub_depth: u32` as an 8th state field.

### 13-scenario test matrix → DFA transition map

This is the test gate per D-10 + `docs/specs/chained-commands.md:122-138`. Each row becomes one parameterized test case in `tests/chain_split.rs`.

| # | Scenario | Input | Expected segments | Tests which state transitions |
|---|----------|-------|-------------------|-------------------------------|
| S1 | Single command, no chain | `pnpm test` | 1 segment, no trailing_op | Baseline — no state changes |
| S2a | Two-segment `&&` | `a && b` | 2 segments, op=AndAnd | `&&` at depth 0 |
| S2b | Two-segment `\|\|` | `a \|\| b` | 2 segments, op=OrOr | `\|\|` at depth 0 |
| S2c | Two-segment `;` | `a ; b` | 2 segments, op=Semi | `;` at depth 0 |
| S3 | Mixed operators | `a && b \|\| c ; d` | 4 segments, ops=[AndAnd, OrOr, Semi] | All three operators in one pass |
| S4 | Per-segment differing rule | `pnpm install && pnpm test` | 2 segments | Resolver test, not splitter |
| S5 | One segment unmatched | `pnpm install && echo done` | 2 segments | Resolver test |
| S6 | One segment interactive (whole-chain bypass) | `vim file && echo done` | (splitter returns 2; orchestration emits bypass) | TUI heuristic + bypass logic |
| S7 | Subshell — single segment | `(a && b)` | 1 segment | `(` → subshell_depth=1; `&&` at depth>0 ignored; `)` → depth=0 |
| S8 | Command substitution — single segment | `echo $(a && b)` | 1 segment | `$(` → cmd_sub_depth=1; `&&` at depth>0 ignored; `)` → depth=0 |
| S9 | Chain op in quoted string — single segment | `echo "a && b"` | 1 segment | `"` → in_double_quote=true; `&&` at quote>0 ignored |
| S10 | Pipeline as segment | `a \| b && c` | 2 segments, op=AndAnd; first segment is `a \| b` | `\|` consumed verbatim; `&&` at depth 0 splits |
| S11 | Heredoc body opaque | `cat <<EOF && echo done\nstuff && more\nEOF\necho` | depends on impl: ideally 2 segments — `cat <<EOF...EOF` (with heredoc body opaque) and `echo` | Heredoc start detect + body termination |
| S12 | Whole-chain bypass via `!!` | `!! a && b` | (orchestration emits pass-through; splitter not invoked) | Bypass detection before split |
| S13 | Whole-chain bypass via LACON_DISABLE=1 | env LACON_DISABLE=1 + `a && b` | (orchestration emits pass-through) | Bypass detection before split |

**Scenario gotcha — S11 heredoc.** The example in `docs/specs/chained-commands.md:136` ("Heredoc body containing chain operators — body is opaque") doesn't specify the exact input or expected output shape. Planner: pick a concrete fixture like `cat <<EOF\na && b\nEOF` (no trailing chain), assert 1 segment with the entire heredoc preserved verbatim. For the chain-after-heredoc case, write a SECOND fixture and document the expected behavior. If implementation finds this fragile, the planner can defer heredoc support to a smaller v1 scope (treat `<<` as opaque-until-EOL only) — this is a Claude's-discretion implementation simplification compatible with D-06's spec.

### Pathological-input benchmark (D-06/D-08, CONTEXT benchmark item 3)

Pathological inputs the planner should add to the splitter test suite:
- `((a && b) && (c && d)) ; e \|\| f && g \| h`
- `echo $(echo $(echo $(echo hi))) && true`
- 4 KB of nested subshells: `(((...((true)))...))`
- Heredoc with 1000-line body containing 500 instances of `&&`

The DFA stays linear in command length (one O(1) state transition per byte) — confirm via a timer-based test asserting splitter throughput stays under 1ms on a 10KB input. [DERIVED from CONTEXT.md "Implementation-time benchmarks #3"]

## TUI heuristic implementation patterns

### Pure-TUI table (D-16) — `const PURE_TUI: &[&str]`

```rust
// crates/lacon-adapter-claudecode/src/tui.rs
// Source: docs/specs/chained-commands.md:85-87 (verbatim)

pub const PURE_TUI: &[&str] = &[
    // Editors
    "vim", "vi", "nvim", "nano", "emacs",
    // Pagers
    "less", "more", "most", "man",
    // System monitors
    "htop", "top", "btop",
    // Multiplexers / remote shells
    "screen", "tmux", "ssh", "mosh",
    // REPLs (always interactive)
    "ipython", "irb", "pry",
    // Tools that take over terminal
    "redis-cli", "crontab", "visudo",
];
```

Lookup uses `std::path::Path::new(&argv[0]).file_name().and_then(OsStr::to_str)` to extract the basename, then `.contains()` on the const slice (n=22 — linear scan is faster than HashSet for n<100).

### Conditional dispatch (D-17)

```rust
pub fn is_tui(command: &str, args: &[String]) -> bool {
    // Step 1: extract basename
    let basename = std::path::Path::new(command)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(command);

    // Step 2: pure-TUI table lookup
    if PURE_TUI.contains(&basename) {
        return true;
    }

    // Step 3: conditional patterns
    match basename {
        "git" => is_git_interactive(args),
        "npm" | "yarn" | "pnpm" => is_pkg_init_interactive(args),
        "node" | "python" | "python3" => is_repl(args),
        "mysql" | "psql" | "sqlite3" => is_db_interactive(args, basename),
        _ => false,
    }
}
```

### Conditional dispatcher specs (one per row of the spec table)

| Function | Logic | Test cases (positive / negative) |
|----------|-------|----------------------------------|
| `is_git_interactive(args)` | Match on `args[0]`: `rebase` → check for `-i` or `--interactive`; `commit` → check NONE of `-m`/`--message`/`--message=*`/`-F`/`--file` present; `add` → check for `-p`/`--patch`/`-i`/`--interactive`; `checkout` → check for `-p`/`--patch`; `stash` → check for `-p`/`--patch`; else false | (+) `git rebase -i HEAD~5` / `git commit` / `git add -p` (−) `git rebase HEAD~5` / `git commit -m "x"` / `git add file.txt` / `git status` |
| `is_pkg_init_interactive(args)` | Match on `args[0] == "init"`; if so, check NEITHER `-y` NOR `--yes` present; else false | (+) `npm init` / `pnpm init` (−) `npm init -y` / `npm install` / `pnpm run build` |
| `is_repl(args)` | True if ALL of `args[1..]` start with `-` (no positional arg) OR `args[1..]` is empty | (+) `node` / `python3` / `python -i` (−) `node script.js` / `python -c "print(1)"` |
| `is_db_interactive(args, basename)` | mysql/psql/sqlite3: true if NO positional argument AND no `-c`/`-e`/`-f`/`--command`/`--execute`/`--file` argument | (+) `psql` / `mysql -h host` (−) `sqlite3 mydb.db` / `psql -c "SELECT 1"` / `mysql -e "SHOW DBS"` |

**Edge case for `is_repl`:** What about `python --version`? It exits immediately. The conservative answer is "treat as TUI because no positional arg" — false positive accepted because the cost is just "one whole-chain bypass" and the alternative (heuristically detecting `--version`/`--help` and exempting) is fragile. CONTEXT does not specify this; planner's call. Recommendation: ship the conservative form; add `--version` / `--help` / `-V` / `-h` exemption if real-world false-positive rate is high (v1.5 polish).

**Negative tests are critical.** The TUI heuristic is conservative — false positives cost filtering opportunity. Each pure-TUI entry needs a negative test:
- `ls` is NOT TUI (despite being terminal-oriented)
- `git status` is NOT TUI
- `mysql -e "SELECT 1"` is NOT TUI
- `pnpm run dev` is NOT TUI (even though it might LOOK interactive — it doesn't grab the terminal)

## `apply_rewrite` idempotency — concrete edge cases

D-19 specifies `apply(apply(x)) == apply(x)`. Below are 10 regression test cases the planner MUST require in `crates/lacon-core/tests/rewrite.rs` (new file) or as a unit test module in `rules/rewrite.rs`.

```rust
// crates/lacon-core/src/rules/rewrite.rs

use crate::rules::schema::RewriteSpec;

/// Apply a `RewriteSpec` to an argv vector. Pure function. Idempotent:
/// `apply_rewrite(&apply_rewrite(argv, rw), rw) == apply_rewrite(argv, rw)`.
///
/// Order: remove_flags first → replace_flags → add_flags (the add_flags
/// idempotency check then sees the post-remove/post-replace argv).
///
/// `argv[0]` is NEVER touched.
pub fn apply_rewrite(argv: &[String], rewrite: &RewriteSpec) -> Vec<String> {
    if argv.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::with_capacity(argv.len() + rewrite.add_flags.len());
    out.push(argv[0].clone());  // argv[0] preserved verbatim

    // remove_flags: filter from argv[1..]
    let after_remove: Vec<&String> = argv[1..]
        .iter()
        .filter(|a| !rewrite.remove_flags.iter().any(|rf| rf == a.as_str()))
        .collect();

    // replace_flags: map old → new on each surviving arg
    let after_replace: Vec<String> = after_remove
        .iter()
        .map(|a| match rewrite.replace_flags.get(a.as_str()) {
            Some(new) => new.clone(),
            None => (*a).clone(),
        })
        .collect();

    out.extend(after_replace);

    // add_flags: append only if not already present (idempotent)
    for flag in &rewrite.add_flags {
        if !out[1..].iter().any(|existing| existing == flag) {
            out.push(flag.clone());
        }
    }

    out
}
```

### Test cases (10)

| # | argv input | RewriteSpec | Expected output | Tests |
|---|-----------|-------------|-----------------|-------|
| T1 | `["cargo", "test"]` | `add_flags: ["--no-color"]` | `["cargo", "test", "--no-color"]` | Basic add |
| T2 | `["cargo", "test", "--no-color"]` | `add_flags: ["--no-color"]` | `["cargo", "test", "--no-color"]` (NO duplicate) | Idempotency: add of existing is no-op |
| T3 | (apply T1 twice) `["cargo", "test", "--no-color"]` | `add_flags: ["--no-color"]` | `["cargo", "test", "--no-color"]` | `apply(apply(x)) == apply(x)` invariant |
| T4 | `["cargo", "test", "--verbose"]` | `remove_flags: ["--verbose"]` | `["cargo", "test"]` | Basic remove |
| T5 | `["cargo", "test"]` | `remove_flags: ["--verbose"]` | `["cargo", "test"]` (no-op) | Remove of absent flag is no-op |
| T6 | `["cargo", "test", "--verbose", "--verbose"]` | `remove_flags: ["--verbose"]` | `["cargo", "test"]` | Remove removes ALL occurrences |
| T7 | `["pnpm", "install", "--progress"]` | `replace_flags: {"--progress": "--no-progress"}` | `["pnpm", "install", "--no-progress"]` | Basic replace |
| T8 | `["pnpm", "install", "--no-progress"]` | `replace_flags: {"--progress": "--no-progress"}` | `["pnpm", "install", "--no-progress"]` | Idempotency: replace of already-replaced is no-op (the old form is absent) |
| T9 | `["vitest", "--reporter", "verbose"]` | `add_flags: ["--reporter", "silent"]` | NOTE — ambiguous. See discussion below. | Multi-arg flag handling — adapter MUST document |
| T10 | `["cargo", "build"]` (touching argv[0]) | `replace_flags: {"cargo": "evil"}` | `["cargo", "build"]` (argv[0] NEVER touched) | argv[0] invariant |

**T9 — multi-arg flag ambiguity.** `add_flags: ["--reporter", "silent"]` could mean:
(a) "add the two args `--reporter` and `silent` as separate elements"
(b) "add the single arg `--reporter silent`"

CONTEXT D-19 says: "append each flag to argv ONLY if not already present (string-equal anywhere in `argv[1..]`)". This implies (a) — each list element is one argv element. So:
- Input `["vitest", "--reporter", "verbose"]` + `add_flags: ["--reporter", "silent"]` → `["vitest", "--reporter", "verbose", "silent"]` (because `--reporter` is already present, only `silent` is appended).

This is almost certainly NOT what the rule author wanted. The lesson for v1: **`add_flags` is for fresh switch-flags, not for replacing existing arg-value pairs.** For `--reporter=silent` style (single-arg form), the YAML rule author should write `add_flags: ["--reporter=silent"]` (single string with `=`). For separate-arg style, use `replace_flags` to swap an existing value.

Planner: document this in the rule schema reference task; it does NOT require code changes. The test T9 confirms the literal behavior.

## `quote_for_shell` — POSIX-portable correctness

D-20's algorithm is correct and minimal. Below is the spec the planner gives the executor.

```rust
// crates/lacon-adapter-claudecode/src/quote.rs
use std::borrow::Cow;

/// POSIX-portable shell-quote. Safe in sh, bash, dash, zsh (argv position).
/// - No metachars and no whitespace → return Borrowed (zero allocation).
/// - Otherwise: wrap in single quotes; replace embedded `'` with `'\''`.
///
/// Idempotent: `quote_for_shell(quote_for_shell(x))` is a no-op when fed back
/// through ONE shell parse (the design contract — Phase 1's Runner does NOT
/// re-shell-parse).
pub fn quote_for_shell(arg: &str) -> Cow<'_, str> {
    const METACHARS: &[u8] = b"|&;<>()$`\\\"'\n\t *?[#~=%!";
    let needs_quote = arg.is_empty()
        || arg.bytes().any(|b| METACHARS.contains(&b));
    if !needs_quote {
        return Cow::Borrowed(arg);
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('\'');
    for c in arg.chars() {
        if c == '\'' {
            out.push_str("'\\''");  // close, escape, reopen
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    Cow::Owned(out)
}
```

### Metachar set discussion

CONTEXT D-20 lists: `|&;<>()$\`\\\"'\n\t*?[#~=%!`. Per `etalabs.net/sh_tricks.html` and POSIX sh spec [CITED]:

- **Truly metachar in argv position:** `|`, `&`, `;`, `<`, `>`, `(`, `)`, `$`, `` ` ``, `\\`, `"`, `'`, whitespace (` `, `\t`, `\n`), `*`, `?`, `[`, `#` (comment at word-start), `~` (home-dir expansion at word-start), `!` (history in interactive bash; harmless in non-interactive but cautious-quote is fine).
- **`=`** — only special at word-start as assignment (`VAR=value`). At argv position ≥1, harmless. CONTEXT includes it; quoting it is overly conservative but never wrong.
- **`%`** — used by `jobs`/`fg`/`bg` for job control. At argv position ≥1, harmless. CONTEXT includes it; same as `=`.

**Recommendation: ship CONTEXT D-20's metachar set verbatim.** Over-quoting `=` and `%` produces `'foo=bar'` instead of `foo=bar` — both parse identically; only readability suffers. The cost of being conservative is zero correctness risk; the cost of being aggressive (NOT quoting `=`) is a latent bug if a future rule author writes `--reporter=val&` where `&` was missed.

### Test cases (POSIX round-trip)

The acceptance criterion (D-22): "fixture with `--reporter='custom reporter'`, args containing `$()`, args with embedded quotes — assert round-trip through `quote_for_shell + sh -c '...'` produces the original argv."

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip_via_sh(argv: &[&str]) -> Vec<String> {
        // Build shell command: printf '%s\n' <quoted>...
        let parts: Vec<String> = std::iter::once("printf '%s\\n'".to_string())
            .chain(argv.iter().map(|a| quote_for_shell(a).into_owned()))
            .collect();
        let cmd = parts.join(" ");
        let output = std::process::Command::new("/bin/sh")
            .arg("-c").arg(&cmd).output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        stdout.lines().map(String::from).collect()
    }

    #[test] fn quote_plain_no_quote() { assert_eq!(quote_for_shell("hello"), "hello"); }
    #[test] fn quote_with_space()    { assert_eq!(roundtrip_via_sh(&["a b"]), vec!["a b"]); }
    #[test] fn quote_with_dollar()   { assert_eq!(roundtrip_via_sh(&["$(rm -rf /)"]), vec!["$(rm -rf /)"]); }
    #[test] fn quote_with_backtick() { assert_eq!(roundtrip_via_sh(&["`whoami`"]), vec!["`whoami`"]); }
    #[test] fn quote_with_single_q() { assert_eq!(roundtrip_via_sh(&["it's"]), vec!["it's"]); }
    #[test] fn quote_with_newline()  { assert_eq!(roundtrip_via_sh(&["a\nb"]), vec!["a", "b"]); }
    #[test] fn quote_with_tab()      { assert_eq!(roundtrip_via_sh(&["a\tb"]), vec!["a\tb"]); }
    #[test] fn quote_empty()         { assert_eq!(quote_for_shell(""), "''"); }
    #[test] fn quote_eq_value()      { assert_eq!(roundtrip_via_sh(&["--reporter=val"]), vec!["--reporter=val"]); }
    #[test] fn quote_eq_with_space() { assert_eq!(roundtrip_via_sh(&["--reporter=custom reporter"]), vec!["--reporter=custom reporter"]); }
    #[test] fn quote_paren()         { assert_eq!(roundtrip_via_sh(&["(group)"]), vec!["(group)"]); }
}
```

[CITED: etalabs.net/sh_tricks.html for the single-quote-wrap algorithm; verified portable across sh, bash, dash, zsh.]

## `lacon init` strategy — settings.json + CLAUDE.md

### `.claude/settings.json` walk-and-rewrite algorithm

```rust
// crates/lacon-cli/src/commands/init.rs (excerpt)

use serde_json::{json, Value};

fn install_lacon_hook(settings: &mut Value) {
    // Ensure path: settings.hooks.PreToolUse exists and is an array.
    let hooks = settings.as_object_mut().expect("settings is object")
        .entry("hooks").or_insert_with(|| json!({}));
    let pretool = hooks.as_object_mut().expect("hooks is object")
        .entry("PreToolUse").or_insert_with(|| json!([]));
    let pretool_arr = pretool.as_array_mut().expect("PreToolUse is array");

    // Phase 1: scrub existing lacon-managed entries.
    // For each matcher-group with matcher == "Bash", filter out inner hooks
    // whose command starts with "lacon-claude-hook".
    for group in pretool_arr.iter_mut() {
        if group.get("matcher").and_then(Value::as_str) != Some("Bash") {
            continue;
        }
        let Some(inner) = group.get_mut("hooks").and_then(Value::as_array_mut) else { continue };
        inner.retain(|h| {
            let cmd = h.get("command").and_then(Value::as_str).unwrap_or("");
            !cmd.starts_with("lacon-claude-hook")
        });
    }

    // Phase 2: remove now-empty Bash matcher groups (keep settings.json clean).
    pretool_arr.retain(|group| {
        let is_bash = group.get("matcher").and_then(Value::as_str) == Some("Bash");
        if !is_bash { return true; }
        let inner = group.get("hooks").and_then(Value::as_array);
        inner.is_some_and(|a| !a.is_empty())
    });

    // Phase 3: insert fresh entry.
    pretool_arr.push(json!({
        "matcher": "Bash",
        "hooks": [
            { "type": "command", "command": "lacon-claude-hook" }
        ]
    }));
}
```

**Idempotency property:** `install_lacon_hook(settings); install_lacon_hook(&mut settings.clone())` — the second call's input has the desired entry at the end of the array; Phase 1 strips it; Phase 3 re-inserts it; output is structurally identical. Test in `cli_init.rs` asserts this byte-for-byte (modulo JSON whitespace).

**Preservation property:** Any non-Bash matcher group, any Bash matcher group with non-lacon hooks (mixed with lacon hooks — split correctly), and any keys outside `hooks` are preserved untouched. Test fixture: pre-populate settings.json with a user-authored `PreToolUse(Edit)` hook and a Bash `formatter.sh`, then run `lacon init`, then assert both survive.

### CLAUDE.md walk-and-rewrite algorithm

```rust
// crates/lacon-cli/src/commands/init.rs (excerpt)

const LACON_START: &str = "<!-- lacon:start -->";
const LACON_END: &str = "<!-- lacon:end -->";

fn install_claude_md_block(existing: &str, block_body: &str) -> String {
    // Detect existing block.
    let start_idx = existing.find(LACON_START);
    let end_idx = existing.find(LACON_END);

    match (start_idx, end_idx) {
        (Some(s), Some(e)) if s < e => {
            // Both markers present, in order: replace contents in place.
            let end_inclusive = e + LACON_END.len();
            let mut out = String::with_capacity(existing.len());
            out.push_str(&existing[..s]);
            out.push_str(LACON_START);
            out.push('\n');
            out.push_str(block_body);
            out.push('\n');
            out.push_str(LACON_END);
            out.push_str(&existing[end_inclusive..]);
            out
        }
        (Some(_), None) | (None, Some(_)) => {
            // Corrupt state — only one marker. Log warning to stderr,
            // append fresh block at EOF, leave existing partial marker alone.
            eprintln!(
                "lacon init: warning — CLAUDE.md has unmatched lacon marker; \
                 appending fresh block at EOF, leaving existing marker untouched"
            );
            append_fresh_block(existing, block_body)
        }
        (None, None) => {
            // No markers — append at EOF.
            append_fresh_block(existing, block_body)
        }
    }
}

fn append_fresh_block(existing: &str, block_body: &str) -> String {
    let mut out = String::with_capacity(existing.len() + 256);
    out.push_str(existing);
    // Ensure trailing newline before our block.
    if !existing.is_empty() && !existing.ends_with('\n') {
        out.push('\n');
    }
    if !existing.is_empty() {
        out.push('\n');  // visual separation
    }
    out.push_str(LACON_START);
    out.push('\n');
    out.push_str(block_body);
    out.push('\n');
    out.push_str(LACON_END);
    out.push('\n');
    out
}
```

**Block body (D-14, planner's exact wording is discretion):**
```
Bash output is filtered by lacon to reduce token usage. Bypass one command
with `!!` prefix (e.g., `!! pnpm test`). Disable filtering entirely with
`LACON_DISABLE=1`. See `https://github.com/maurice/lacon` for rule docs.
```

**Edge case for corrupt state (single marker present):** D-14 doesn't specify. Recommendation: log a warning, append fresh block at EOF, leave the orphan marker untouched. This is the most conservative behavior — never destroy user content.

### `.lacon/` skeleton

D-domain (boundary line 9 of CONTEXT) requires "the `.lacon/` skeleton". Recommendation: create `<cwd>/.lacon/` directory (empty) and a `.gitkeep` file so it survives clone. Do NOT pre-create `.lacon/rules/` (the user creates rule files lazily; an empty `rules/` directory is no different from a missing one per Phase 1's loader at `loader.rs:222-224`).

Planner: a `.lacon/config.yaml` template is NOT required (Phase 1's config loader handles missing files). Keep `lacon init` minimal.

## Bypass detection patterns

```rust
// crates/lacon-adapter-claudecode/src/lib.rs (excerpt)

pub fn detect_bypass(command: &str) -> bool {
    // D-23: !! prefix detection (LSTRIP whitespace first).
    let lstripped = command.trim_start();
    if lstripped.starts_with("!!") {
        return true;
    }
    // D-24: LACON_DISABLE=1 env var (exact string "1", per runtime/mod.rs:175).
    if std::env::var("LACON_DISABLE").as_deref() == Ok("1") {
        return true;
    }
    false
}
```

**Critical correctness checks:**
- `"!!"` alone (no command after) → bypass (the LSTRIP makes whitespace OK)
- `"!! pnpm test"` → bypass; the `!!` is consumed by Claude Code's shell as bash history expansion ONLY IF history is enabled. In non-interactive bash (which is what Claude Code's shell is), `!!` is literal. So passing through the command unchanged is correct — bash will execute `!!` as a literal command name and fail. Planner: this is the user's intent ("just run this raw"). D-23's pass-through means we don't intervene; the shell will produce its own error.
- `"!!!"` → bypass (starts with `!!`)
- `" !! pnpm test"` (leading whitespace) → bypass (LSTRIP)
- `"!pnpm test"` (single `!`) → NOT bypass
- `LACON_DISABLE=""` → NOT bypass (D-24)
- `LACON_DISABLE="0"` → NOT bypass
- `LACON_DISABLE="true"` → NOT bypass
- `LACON_DISABLE="1"` → bypass

Test cases per `tui_heuristic.rs` pattern: parameterized list of (input, expected_bypass).

## End-to-end test fixture shape (hook_e2e.rs)

```rust
// crates/lacon-adapter-claudecode/tests/hook_e2e.rs

use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;
use tempfile::tempdir;

fn run_hook_with_input(input_json: &str) -> std::process::Output {
    Command::cargo_bin("lacon-claude-hook")
        .unwrap()
        .write_stdin(input_json)
        .output()
        .expect("hook binary runs")
}

#[test]
fn pass_through_unmatched_command_exits_zero_empty_stdout() {
    // No rules in cwd → no match → pass-through.
    let dir = tempdir().unwrap();
    let input = serde_json::json!({
        "session_id": "test-session",
        "transcript_path": "/tmp/t.jsonl",
        "cwd": dir.path().to_string_lossy(),
        "permission_mode": "default",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hello" },
        "tool_use_id": "test-id"
    }).to_string();

    let output = run_hook_with_input(&input);
    assert!(output.status.success());
    assert!(output.stdout.is_empty(), "pass-through must emit empty stdout");
}

#[test]
fn matched_single_command_emits_rewrite_json() {
    let dir = tempdir().unwrap();
    let rules_dir = dir.path().join(".lacon/rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(rules_dir.join("test.yaml"), r#"
id: echo-rule
match: { command: echo }
pipeline:
  - strip_ansi
"#).unwrap();

    let input = serde_json::json!({
        "session_id": "test-session",
        "transcript_path": "/tmp/t.jsonl",
        "cwd": dir.path().to_string_lossy(),
        "permission_mode": "default",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hello", "description": "say hi" },
        "tool_use_id": "test-id"
    }).to_string();

    let output = run_hook_with_input(&input);
    assert!(output.status.success());
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    // Verify shape per D-03.
    assert_eq!(stdout["hookSpecificOutput"]["hookEventName"], "PreToolUse");
    assert_eq!(stdout["hookSpecificOutput"]["permissionDecision"], "allow");
    let updated = &stdout["hookSpecificOutput"]["updatedInput"];
    let updated_cmd = updated["command"].as_str().unwrap();
    assert!(updated_cmd.contains("lacon run --rule echo-rule"));
    assert!(updated_cmd.contains("echo hello"));
    // description MUST be carried through (D-03).
    assert_eq!(updated["description"], "say hi");
}

#[test]
fn chain_with_one_matched_one_unmatched_segment_emits_chain_rewrite() {
    let dir = tempdir().unwrap();
    let rules_dir = dir.path().join(".lacon/rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(rules_dir.join("test.yaml"), r#"
id: echo-rule
match: { command: echo }
pipeline:
  - strip_ansi
"#).unwrap();

    let input = serde_json::json!({
        "session_id": "test-session", "transcript_path": "/tmp/t.jsonl",
        "cwd": dir.path().to_string_lossy(), "permission_mode": "default",
        "hook_event_name": "PreToolUse", "tool_name": "Bash",
        "tool_input": { "command": "echo hi && ls -la" },
        "tool_use_id": "test-id"
    }).to_string();

    let output = run_hook_with_input(&input);
    let stdout: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let updated_cmd = stdout["hookSpecificOutput"]["updatedInput"]["command"]
        .as_str().unwrap();

    // Matched segment wrapped, unmatched preserved verbatim, joined with original op.
    assert!(updated_cmd.contains("lacon run --rule echo-rule -- echo hi"));
    assert!(updated_cmd.contains(" && "));
    assert!(updated_cmd.contains("ls -la"));
}
```

**Planner: add 2-3 more fixtures for:** bypass via `!!`, bypass via env (set via `Command::env("LACON_DISABLE", "1")`), TUI segment in chain (whole-chain bypass), preservation of `timeout` and `run_in_background` fields.

## `lacon init` integration test shape (cli_init.rs)

```rust
// crates/lacon-cli/tests/cli_init.rs

use assert_cmd::Command;
use std::fs;
use tempfile::tempdir;

#[test]
fn init_in_empty_dir_creates_skeleton() {
    let dir = tempdir().unwrap();
    Command::cargo_bin("lacon").unwrap()
        .current_dir(dir.path())
        .arg("init")
        .assert()
        .success();

    // .lacon/ skeleton
    assert!(dir.path().join(".lacon").is_dir());

    // .claude/settings.json with our hook
    let settings_text = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
    let settings: serde_json::Value = serde_json::from_str(&settings_text).unwrap();
    let pretool = &settings["hooks"]["PreToolUse"];
    assert!(pretool.is_array());
    let found_bash = pretool.as_array().unwrap().iter()
        .filter(|g| g["matcher"] == "Bash")
        .flat_map(|g| g["hooks"].as_array().unwrap().iter())
        .any(|h| h["command"].as_str() == Some("lacon-claude-hook"));
    assert!(found_bash, "lacon-claude-hook hook installed under matcher=Bash");

    // CLAUDE.md block
    let claude_md = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    assert!(claude_md.contains("<!-- lacon:start -->"));
    assert!(claude_md.contains("<!-- lacon:end -->"));
    assert!(claude_md.contains("!!"));
    assert!(claude_md.contains("LACON_DISABLE"));
}

#[test]
fn init_is_idempotent() {
    let dir = tempdir().unwrap();
    // First run
    Command::cargo_bin("lacon").unwrap().current_dir(dir.path()).arg("init").assert().success();
    let settings_v1 = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
    let claude_md_v1 = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();

    // Second run
    Command::cargo_bin("lacon").unwrap().current_dir(dir.path()).arg("init").assert().success();
    let settings_v2 = fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap();
    let claude_md_v2 = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();

    // Content equality (not byte equality of mtimes — atomic write changes file inode)
    assert_eq!(settings_v1, settings_v2, "settings.json byte-stable across runs");
    assert_eq!(claude_md_v1, claude_md_v2, "CLAUDE.md byte-stable across runs");
}

#[test]
fn init_preserves_user_hooks_and_settings() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join(".claude")).unwrap();
    fs::write(dir.path().join(".claude/settings.json"), r#"{
  "model": "claude-opus-4",
  "hooks": {
    "PreToolUse": [
      { "matcher": "Edit", "hooks": [{ "type": "command", "command": "my-edit-hook.sh" }] },
      { "matcher": "Bash", "hooks": [{ "type": "command", "command": "my-bash-formatter.sh" }] }
    ]
  }
}"#).unwrap();

    Command::cargo_bin("lacon").unwrap().current_dir(dir.path()).arg("init").assert().success();

    let settings: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join(".claude/settings.json")).unwrap()).unwrap();

    // Top-level "model" key preserved.
    assert_eq!(settings["model"], "claude-opus-4");
    // Edit matcher preserved entirely.
    let pretool = settings["hooks"]["PreToolUse"].as_array().unwrap();
    let edit_grp = pretool.iter().find(|g| g["matcher"] == "Edit").unwrap();
    assert_eq!(edit_grp["hooks"][0]["command"], "my-edit-hook.sh");
    // Bash matcher: user's formatter preserved AND our hook added.
    let bash_groups: Vec<&serde_json::Value> = pretool.iter()
        .filter(|g| g["matcher"] == "Bash").collect();
    let all_bash_cmds: Vec<&str> = bash_groups.iter()
        .flat_map(|g| g["hooks"].as_array().unwrap().iter())
        .filter_map(|h| h["command"].as_str()).collect();
    assert!(all_bash_cmds.contains(&"my-bash-formatter.sh"), "user's Bash hook preserved");
    assert!(all_bash_cmds.contains(&"lacon-claude-hook"), "lacon hook added");
}
```

## Performance measurement harness

Phase 1 established the pattern: a hand-rolled `cold_start_probe` binary in `benches/cold_start.rs` (NOT criterion, NOT hyperfine). Phase 3 should extend this pattern, not introduce a new harness.

**Recommendation:** Add two new scenarios to `benches/cold_start.rs`:

1. **`lacon-claude-hook` pass-through** — pipe a fixture JSON into the binary, measure wall-clock. Target ≤2ms median.
2. **`lacon-claude-hook` rewrite path** — pipe a fixture JSON for a matched command, measure wall-clock. Target ≤5ms median.

```rust
// benches/cold_start.rs (additions)

const HOOK_BIN: &str = "target/release/lacon-claude-hook";

fn measure_hook(stdin_json: &str) -> std::time::Duration {
    use std::io::Write;
    let start = std::time::Instant::now();
    let mut child = std::process::Command::new(HOOK_BIN)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn lacon-claude-hook");
    child.stdin.as_mut().unwrap().write_all(stdin_json.as_bytes()).unwrap();
    drop(child.stdin.take());
    let _ = child.wait_with_output();
    start.elapsed()
}

// In main(): add two new scenarios — pass_through_payload (no rule), rewrite_payload (match).
```

**Why not hyperfine:** Phase 1's RESEARCH.md notes hyperfine NOT installed in dev env (line 825). The hand-rolled probe gives identical statistics with zero dep.

**Why not criterion:** Criterion is best for tight-loop microbenchmarks (e.g., `tracker_open` in Phase 2's `tracker_open.rs`). For cold-start measurement we need a fresh process per sample — criterion's harness doesn't model that well. Hand-rolled is correct.

**Phase 1 baseline (Linux 6.8.0, Ryzen 7 5800X, release):** `lacon --version` median 1154µs, `lacon validate` median 1259µs. Both well under 10ms. The hook binary's smaller dep graph (no rusqlite, no starlark, no os_pipe) should produce a faster baseline. Target ≤2ms for pass-through is achievable.

## Runtime State Inventory

Phase 3 is greenfield code addition (no rename/refactor), but `lacon init` writes runtime state and the hook process reads env vars at runtime. The following inventory documents what state Phase 3 reads or writes outside the codebase:

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | None — Phase 3 does not store data. Tracker writes happen inside Phase 1's `lacon run`, not in the adapter. | None |
| Live service config | `.claude/settings.json` in the user's project directory. Idempotent edit per D-12/D-28. | Documented in Pattern 3 + cli_init.rs test |
| OS-registered state | None — `lacon-claude-hook` is registered via Claude Code's `settings.json`, NOT via systemd/launchd/Windows Task Scheduler. The "registration" lives in the JSON file itself. | None |
| Secrets/env vars | Reads `LACON_DISABLE` (D-24); writes `LACON_ASSISTANT`/`LACON_SESSION_ID`/optional `LACON_TOOL_USE_ID` as a per-invocation prefix on the rewritten command (D-26). Phase 2's tracker at `crates/lacon-cli/src/commands/run.rs:270-272` ALREADY consumes `LACON_ASSISTANT` and `LACON_SESSION_ID`. | Hook writes the env-var prefix; no separate "set env" step needed because each rewritten command carries its own prefix |
| Build artifacts | After Phase 3, `target/release/` has TWO binaries: `lacon` (existing) and `lacon-claude-hook` (new). Users installing via `cargo install --path crates/lacon-cli` get `lacon` only; we need a parallel install path for `lacon-claude-hook`. | Document in REQ-cli-init's "what `lacon init` requires the user to have already installed" boundary. Recommendation: README task (Phase 6) covers install instructions; Phase 3's `lacon init` MAY check `command -v lacon-claude-hook` and warn if missing — Claude's discretion |

**Nothing-found categories:** confirmed by checking the codebase for systemd/launchd/Windows Task Scheduler references (none); checked the existing `.claude/settings.local.json` for runtime registrations (none beyond the JSON file itself).

## Common Pitfalls

### Pitfall 1: PreToolUse vs PostToolUse confusion
**What goes wrong:** A confused implementer wires `PostToolUse` instead of (or in addition to) `PreToolUse`, expecting to filter output post-execution.
**Why it happens:** Earlier project design (pre-ADR-0013) assumed `PostToolUse` could replace tool output via `updatedToolOutput`. Empirical testing on 2026-05-05 confirmed there is no such field.
**How to avoid:** ADR-0013 is the source of truth. Only `PreToolUse` is installed in v1. The `lacon init` settings.json writer puts the hook ONLY under `hooks.PreToolUse[]`, never `hooks.PostToolUse[]`. Test in `cli_init.rs` asserts `settings["hooks"]["PostToolUse"]` is absent OR untouched if a user already had a PostToolUse hook.
**Warning signs:** Any code referencing `updatedToolOutput` or `additionalContext` for output replacement.

### Pitfall 2: Dropping `description`/`timeout`/`run_in_background` from `updatedInput`
**What goes wrong:** Hook emits `{"hookSpecificOutput": {"updatedInput": {"command": "<new>"}}}` without echoing back the optional fields. Claude Code silently drops `timeout`/`description` because `updatedInput` REPLACES.
**Why it happens:** Implementer assumes `updatedInput` merges (it does not).
**How to avoid:** Use the typed `BashToolInput` struct (Pattern 1). Serialize the whole struct back, not just `command`. `#[serde(skip_serializing_if = "Option::is_none")]` ensures missing fields stay missing (not emitted as null).
**Warning signs:** Test fixture for `timeout: 120000` followed by missing `timeout` in `updatedInput`.

### Pitfall 3: Splitting `&&` inside double-quoted strings
**What goes wrong:** `echo "a && b"` is split into 2 segments by a naive regex.
**Why it happens:** Regex approach ignores quote context.
**How to avoid:** DFA approach (D-06). Test scenario S9 locks this.
**Warning signs:** Splitter has any use of `Regex::new("(&&|\\\\|\\\\||;)")`. Acceptance criterion: `! grep -rn "regex" crates/lacon-adapter-claudecode/src/chain.rs`.

### Pitfall 4: `quote_for_shell` using double quotes
**What goes wrong:** Implementer writes `format!("\\\"{}\\\"", arg)` and `$(rm -rf /)` becomes a command injection vector.
**Why it happens:** Double-quote in bash performs `$VAR`, `$(cmd)`, `` `cmd` `` expansion.
**How to avoid:** Single-quote-wrap with `'\''` embedded-quote escape. D-20 specifies the algorithm. The round-trip tests in `quote.rs::tests` catch this.
**Warning signs:** Search the adapter source for double-quote string-formatting around arg interpolation. Acceptance criterion: `grep -n '\\\\\"{}' crates/lacon-adapter-claudecode/src/` returns nothing in quote-related code paths.

### Pitfall 5: TUI heuristic detecting `python --version` as TUI
**What goes wrong:** `is_repl(args)` returns true for `["--version"]` because there's no positional arg, causing the whole chain to bypass.
**Why it happens:** The conservative heuristic doesn't exempt flag-only invocations.
**How to avoid:** Document the trade-off; recommend shipping conservative v1 and adding `--version`/`--help` exemption if real-world false-positive rate is high. Acceptance: include a `python --version` fixture in `tui_heuristic.rs` and document the chosen behavior in a comment.
**Warning signs:** User reports "lacon stopped filtering after I ran `pytest --version`" — that's the exemption-needed signal.

### Pitfall 6: settings.json `tempfile::persist` cross-filesystem failure
**What goes wrong:** `NamedTempFile::new()` creates the temp file in `/tmp` (different filesystem from the user's home); `persist` falls back to copy+delete which is NOT atomic.
**Why it happens:** `NamedTempFile::new` (no `_in`) defaults to `std::env::temp_dir()`.
**How to avoid:** Always use `NamedTempFile::new_in(parent)` where `parent` is the destination's parent directory (see Pattern 3). Same filesystem guarantees atomic rename.
**Warning signs:** `NamedTempFile::new()` in `init.rs` source. Acceptance criterion grep: `! grep 'NamedTempFile::new()' crates/lacon-cli/src/commands/init.rs`.

### Pitfall 7: `lacon init` overwriting user's `PreToolUse(Edit)` hooks
**What goes wrong:** The walk-and-rewrite algorithm modifies entries it shouldn't.
**Why it happens:** Implementer iterates `hooks.PreToolUse[]` without filtering by `matcher == "Bash"`.
**How to avoid:** D-12/D-28 are explicit — only entries inside Bash-matcher groups with command-prefix `lacon-claude-hook` are touched. Test `init_preserves_user_hooks_and_settings` (above) locks this.
**Warning signs:** Test fixture with non-Bash hook ends up modified.

### Pitfall 8: `apply_rewrite` touching argv[0]
**What goes wrong:** A `replace_flags: {"cargo": "evil"}` rule changes the command name.
**Why it happens:** Implementer maps `replace_flags` over the entire argv instead of `argv[1..]`.
**How to avoid:** D-19 is explicit — `argv[0]` (command) is never touched. Test T10 above locks this.
**Warning signs:** Code in `apply_rewrite` that uses `argv.iter()` instead of `argv[1..].iter()` for the substitution loop.

### Pitfall 9: Forgetting newline-termination on stdout JSON
**What goes wrong:** Some tooling expects newline-terminated JSON; missing newline causes parse errors in pipe-consumers.
**Why it happens:** `serde_json::to_writer` doesn't add a trailing newline.
**How to avoid:** Pattern 2 explicitly writes `handle.write_all(b"\n")` after the JSON. Claude Code itself reads the whole stdout to EOF so it's tolerant either way, but newline-termination is conventional and harmless.
**Warning signs:** None — this is preventative.

### Pitfall 10: Heredoc detection making the splitter quadratic
**What goes wrong:** A naive heredoc impl re-scans the entire input for each `<<DELIM` token.
**Why it happens:** Looking back from the start of a heredoc body to find the EOL containing the delimiter requires care.
**How to avoid:** The DFA reads forward only; when it sees `<<DELIM` it switches to heredoc mode and the body terminator check (line-start match) is O(1) per byte. No re-scanning.
**Warning signs:** Splitter has nested loops or `find(&self, ..)` calls inside the main loop.

## Code Examples

### Common Operation 1: Parse hook stdin and build response
```rust
// Source: derived from code.claude.com/docs/en/hooks + serde_json docs
use std::io::{self, Write};
use lacon_adapter_claudecode::protocol::{HookInput, BashToolInput, build_rewrite_response};

fn main() -> anyhow::Result<()> {
    let input: HookInput = serde_json::from_reader(io::stdin().lock())?;
    if input.tool_name != "Bash" {
        return Ok(());  // not for us — pass-through
    }
    // ... bypass detection ...
    // ... chain split / TUI / resolve / rewrite ...
    let new_input = BashToolInput {
        command: rewritten_command,
        description: input.tool_input.description.clone(),
        timeout: input.tool_input.timeout,
        run_in_background: input.tool_input.run_in_background,
    };
    let response = build_rewrite_response(&new_input);
    let stdout = io::stdout();
    let mut h = stdout.lock();
    serde_json::to_writer(&mut h, &response)?;
    h.write_all(b"\n")?;
    Ok(())
}
```

### Common Operation 2: Match a Bash command to a rule (using existing Phase 1 API)
```rust
// Source: crates/lacon-core/src/rules/loader.rs:156-212 (Phase 1 RuleLoader::load_all)
use lacon_core::rules::loader::RuleLoader;
use std::path::PathBuf;

fn resolve_for_segment(cwd: &str, argv: &[String]) -> Option<(String, lacon_core::rules::loader::RuleSource)> {
    let mut loader = RuleLoader::new(Some(PathBuf::from(cwd)));
    let all = loader.load_all().ok()?;
    for rule in &all {
        if rule_matches(&rule.rule.match_spec, argv) {
            return Some((rule.id.clone(), rule.source.clone()));
        }
    }
    None
}

// Note: `rule_matches` lives in lacon-core (Phase 1) — re-use Phase 1's matcher
// rather than re-implementing. If Phase 1 doesn't expose it as pub, expose
// it as part of P1 of Phase 3 (small additive change). Cross-reference:
// crates/lacon-cli/src/commands/run.rs:try_match_via_load_all is the
// existing call site — mirror that pattern.
```

**Planner: verify before scheduling.** Phase 1's `try_match_via_load_all` in `crates/lacon-cli/src/commands/run.rs` may be private to that module. If so, P1 of Phase 3 should promote `lacon_core::rules::matcher::matches(spec, argv) -> bool` to a public function so the adapter can reuse it. Search the existing code for the matcher function before assuming. (Confirmed by quick grep: the matching logic is in `commands/run.rs::try_match_via_load_all` — needs to be promoted into `lacon-core` so the adapter can call it without depending on `lacon-cli`.)

### Common Operation 3: Build the rewritten chain string
```rust
fn build_rewritten_chain(segments: Vec<Segment>) -> String {
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate() {
        out.push_str(&seg.text);
        if let Some(op) = &seg.trailing_op {
            out.push(' ');
            out.push_str(op.as_literal());  // " && " / " || " / "; "
            out.push(' ');
        }
    }
    out
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `PostToolUse` to replace tool output via `updatedToolOutput` | `PreToolUse` rewriting command to `lacon run --rule <id> -- <cmd>` wrapper | ADR-0013 (2026-05-05) | `lacon run` is now the production hot path; cold-start budget ≤10ms is load-bearing for the wrapper too |
| Comment-marker idempotency (`// lacon:hook`) | Command-string prefix fingerprint | D-12/D-28 (this phase) | JSON has no comments; prefix is the only robust pattern |
| Hand-roll JSON parsing for cold-start | `serde_json` (1.6× faster than simd-json on small payloads) | Industry consensus 2024+ | `serde_json` is the right choice; simd-json is for >10KB payloads |
| `shlex` / `shell-words` for quoting | Hand-rolled `quote_for_shell` (D-20) | This phase | One screen of code; locked algorithm; zero dep cost |

**Deprecated/outdated:**
- The architecture diagram in `docs/architecture.md` predates ADR-0013 and was updated 2026-05-05 to reflect the wrapper pattern. The Phase 3 implementation must match the updated diagram.
- `serde_yaml` (used by Phase 1 for rule parsing — see `lacon-core/Cargo.toml`) was REPLACED by `serde-saphyr` in Phase 1 because `serde_yaml` is unmaintained. Phase 3 uses `serde_json`, which is fully maintained and not affected.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `tinyjson` lacks `Value` mutation ergonomics needed for D-12 walk | Standard Stack > Alternatives Considered | Low — even if `tinyjson` has the API, `serde_json` is the safer ecosystem-aligned choice. Planner can skip evaluating `tinyjson`. |
| A2 | Conservative TUI behavior for `python --version` is acceptable for v1 | TUI heuristic > Edge case | Low — if many users hit this in real-world testing, add `--version`/`--help` exemption in v1.5 |
| A3 | Heredoc terminating delimiter detection can be O(1) per byte via line-start match | Pitfall 10 | Low — bash's heredoc semantics specify line-start match; this is standard |
| A4 | Promoting the matcher logic from `lacon-cli::commands::run::try_match_via_load_all` to `lacon-core::rules::matcher` is a small additive change | Common Operation 2 + planner note | Medium — if the matcher logic has CLI-specific behavior (error formatting, cwd handling), the promotion requires more care. Planner: read `crates/lacon-cli/src/commands/run.rs` before scheduling. |
| A5 | `serde_json` version 1.0.149 is appropriate; aligns with serde 1.x | Standard Stack | Low — `serde_json` follows serde's compatibility guarantees; any 1.x version works |

## Open Questions (RESOLVED)

1. **Should `lacon init` validate that `lacon-claude-hook` is on PATH before writing settings.json?**
   - What we know: D-domain says `.claude/settings.json` uses command `"lacon-claude-hook"` (bare name, no path). Users installing via `cargo install` get binaries in `~/.cargo/bin/` which must be on PATH for the hook to fire.
   - What's unclear: Should `lacon init` `command -v lacon-claude-hook` and warn if missing?
   - Recommendation: Add a warning (not a failure) — print to stderr "warning: `lacon-claude-hook` not found on PATH; Claude Code hooks may fail to fire until you install/symlink it". Pure Claude's discretion (CONTEXT doesn't require this); recommend including in P5 as a small UX polish.
   - **RESOLVED 2026-05-16 (Phase 3 planning):** DEFER to Phase 4 `lacon doctor`. Rationale: `lacon doctor` is the dedicated check command for environment validation; `lacon init` should remain fast and side-effect-focused, not a preflight checker. Phase 4 will own the PATH check. Plan 5 (`lacon init`) MUST NOT shell out to `command -v` or otherwise probe PATH.

2. **Should `lacon init` accept `--force` to overwrite a corrupt CLAUDE.md marker block?**
   - What we know: D-14 specifies detect-and-replace; the corrupt-state branch (one marker only) appends fresh + logs warning.
   - What's unclear: Some users may want to force overwrite without warnings.
   - Recommendation: Defer — CONTEXT doesn't require it. v1.5 polish if user demand emerges.
   - **RESOLVED 2026-05-16 (Phase 3 planning):** DEFER to v1.5+. Rationale: the walk-and-rewrite algorithm (D-12 settings.json prefix detection + D-28 CLAUDE.md marker scan) is already content-stable on idempotent re-runs; users with corrupt state can manually edit. No current use-case justifies adding a `--force` flag. Plan 5 MUST NOT define a `--force` CLI argument.

3. **Should the env-var prefix (D-26) include `LACON_TOOL_USE_ID`?**
   - What we know: D-26 lists it as Claude's discretion. Phase 2's tracker schema has columns for `assistant` and `session_id` but not `tool_use_id`.
   - What's unclear: Does Phase 4's `lacon explain` benefit from `tool_use_id` correlation enough to justify a new column? Phase 4 hasn't been planned yet.
   - Recommendation: Include `LACON_TOOL_USE_ID` in the prefix from Phase 3 (cheap — one extra env-var per invocation). If Phase 4 doesn't add a column, the env-var is silently unused. If Phase 4 DOES add a column, the data is already there from day 1.
   - **RESOLVED 2026-05-16 (Phase 3 planning):** IMPLEMENT. Rationale: a single extra env-var per invocation is cheap (≤80 bytes added to the rewritten command), and it enables Phase 4 `lacon explain` to cross-correlate with Claude Code's tool history more strongly than `session_id + ts` alone. The env-var is silently unused by Phase 2's tracker until Phase 4 (or v1.5) adds a `tool_use_id` column. Plan 4 Task 1's wrap form is therefore: `LACON_ASSISTANT=claude-code LACON_SESSION_ID=<id> LACON_TOOL_USE_ID=<id> lacon run --rule <id> -- <quoted argv>`. A hook_e2e assertion locks the `LACON_TOOL_USE_ID=` substring presence.

4. **Does Phase 3 need to handle non-Bash tool_name in the hook?**
   - What we know: The hook is registered under `matcher: "Bash"` in settings.json. Claude Code SHOULD only invoke our hook for Bash tools.
   - What's unclear: If the matcher is somehow widened (user edits settings.json), how should we behave?
   - Recommendation: Defensive: if `input.tool_name != "Bash"`, pass-through (exit 0, no JSON emit). Two lines of code; prevents weird future failures.
   - **RESOLVED 2026-05-16 (Phase 3 planning):** IMPLEMENT. Rationale: ~2 LOC for a future-proofing guard against Claude Code widening the `settings.json` matcher schema (which currently is set to `"Bash"` by lacon's `init`, but a user could manually register the hook under a non-Bash matcher). Plan 4 Task 1 orchestration step 0: if `input.tool_name != "Bash"`, return `Ok(HookOutcome::PassThrough)` (empty stdout, exit 0). A hook_e2e test `non_bash_tool_passes_through` feeds a fixture with `tool_name: "Write"` and asserts empty stdout + exit 0.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| `cargo` | Build the new `lacon-claude-hook` binary | ✓ | (Phase 1 already established) | — |
| `rustc` 1.80+ | MSRV per `Cargo.toml` workspace.package.rust-version | ✓ | (Phase 1 already established) | — |
| `serde_json` crate | New workspace dep | (to be added) | 1.0.149 | — |
| `tempfile` crate | `lacon init` atomic settings.json write; existing workspace dep | ✓ | 3.x (workspace) | `std::fs::write` (loses atomicity guarantee) |
| `/bin/sh` | `quote_for_shell` round-trip tests | ✓ | POSIX | Skip the integration tests on systems without `/bin/sh` (CI: any Linux/macOS has it) |
| Claude Code installation | NOT required at build time; only at hook runtime | n/a | n/a | — |

**Missing dependencies with no fallback:** None.

**Missing dependencies with fallback:** None.

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `cargo test` (built-in) + `assert_cmd` 2 + `predicates` 3 (workspace dev-deps) |
| Config file | none (Cargo conventions) |
| Quick run command | `cargo test -p lacon-adapter-claudecode -p lacon-cli --tests` |
| Full suite command | `cargo test --workspace --all-targets` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| REQ-adapter-pretooluse-only | Hook installs ONLY under `hooks.PreToolUse[]` (matcher Bash); never under `PostToolUse` | integration | `cargo test -p lacon-cli --test cli_init init_in_empty_dir_creates_skeleton` | ❌ Wave 0 |
| REQ-adapter-pretooluse-only | Hook emits `hookSpecificOutput` shape with `hookEventName: "PreToolUse"`, `permissionDecision: "allow"`, `updatedInput.command` rewritten | integration | `cargo test -p lacon-adapter-claudecode --test hook_e2e matched_single_command_emits_rewrite_json` | ❌ Wave 0 |
| REQ-adapter-pretooluse-only | `updatedInput` carries `description`/`timeout`/`run_in_background` when present | integration | `cargo test -p lacon-adapter-claudecode --test hook_e2e` (parameterized fixture) | ❌ Wave 0 |
| REQ-adapter-bypass-detection | `!!` prefix → pass-through (empty stdout, exit 0) | integration | `cargo test -p lacon-adapter-claudecode --test hook_e2e bypass_via_bang_bang` | ❌ Wave 0 |
| REQ-adapter-bypass-detection | `LACON_DISABLE=1` env → pass-through | integration | `cargo test -p lacon-adapter-claudecode --test hook_e2e bypass_via_env` | ❌ Wave 0 |
| REQ-adapter-bypass-detection | LACON_DISABLE other values (empty/`0`/`true`) do NOT bypass | unit | `cargo test -p lacon-adapter-claudecode lib::detect_bypass` | ❌ Wave 0 |
| REQ-adapter-chained-commands | 13-scenario test matrix (chain_split.rs) | unit | `cargo test -p lacon-adapter-claudecode --test chain_split` | ❌ Wave 0 |
| REQ-adapter-chained-commands | Whole-chain reassembly preserves operators byte-exact | unit | `cargo test -p lacon-adapter-claudecode --test chain_split reassembly` | ❌ Wave 0 |
| REQ-adapter-tui-bypass | 22 pure-TUI basenames detected | unit | `cargo test -p lacon-adapter-claudecode --test tui_heuristic pure_tui_table` | ❌ Wave 0 |
| REQ-adapter-tui-bypass | 8 conditional dispatchers cover spec rows (positive + negative each) | unit | `cargo test -p lacon-adapter-claudecode --test tui_heuristic conditional` | ❌ Wave 0 |
| REQ-adapter-tui-bypass | TUI segment in chain → whole-chain bypass | integration | `cargo test -p lacon-adapter-claudecode --test hook_e2e tui_in_chain_whole_bypass` | ❌ Wave 0 |
| REQ-adapter-pipes-passthrough | Pipeline as segment splits correctly (S10) | unit | `cargo test -p lacon-adapter-claudecode --test chain_split pipeline_as_segment` | ❌ Wave 0 |
| REQ-cli-init | `.lacon/`, `.claude/settings.json`, `CLAUDE.md` created | integration | `cargo test -p lacon-cli --test cli_init init_in_empty_dir_creates_skeleton` | ❌ Wave 0 |
| REQ-cli-init | Re-run is idempotent (content-equal output) | integration | `cargo test -p lacon-cli --test cli_init init_is_idempotent` | ❌ Wave 0 |
| REQ-cli-init | User hooks preserved in settings.json | integration | `cargo test -p lacon-cli --test cli_init init_preserves_user_hooks_and_settings` | ❌ Wave 0 |
| (apply_rewrite invariant) | `apply(apply(x)) == apply(x)` for all rewrite types | unit | `cargo test -p lacon-core rules::rewrite::tests` | ❌ Wave 0 |
| (apply_rewrite) | argv[0] never touched | unit | `cargo test -p lacon-core rules::rewrite::tests::argv0_untouched` | ❌ Wave 0 |
| (quote_for_shell) | 11 POSIX round-trip cases (incl. embedded quote, `$()`, backticks, newline, tab) | unit | `cargo test -p lacon-adapter-claudecode quote::tests` | ❌ Wave 0 |
| (cli-surface-cap regression) | 6-command CLI surface unchanged | integration | `cargo test -p lacon-cli --test cli_surface` | ✅ exists, runs unchanged |
| (perf — informational) | `lacon-claude-hook` pass-through median ≤2ms | manual | `cargo build --release && cargo run --release --bin cold_start_probe` (extended) | benches/cold_start.rs exists; extend with 2 scenarios |
| (perf — informational) | `lacon-claude-hook` rewrite median ≤5ms | manual | same | same |

### Sampling Rate
- **Per task commit:** `cargo test -p <touched-crate> --lib` (unit-only, fast)
- **Per wave merge:** `cargo test -p lacon-adapter-claudecode -p lacon-cli --tests` (integration)
- **Phase gate:** `cargo test --workspace --all-targets` green before `/gsd-verify-work`; cold-start probe run manually with results pasted into `docs/architecture.md`

### Wave 0 Gaps

The following test files do NOT yet exist and must be created during Phase 3:

- [ ] `crates/lacon-adapter-claudecode/tests/chain_split.rs` — 13-scenario matrix covering REQ-adapter-chained-commands
- [ ] `crates/lacon-adapter-claudecode/tests/tui_heuristic.rs` — 22 pure + 8 conditional rows covering REQ-adapter-tui-bypass
- [ ] `crates/lacon-adapter-claudecode/tests/hook_e2e.rs` — end-to-end JSON-in/JSON-out covering REQ-adapter-pretooluse-only + REQ-adapter-bypass-detection
- [ ] `crates/lacon-cli/tests/cli_init.rs` — end-to-end `lacon init` covering REQ-cli-init
- [ ] `crates/lacon-core/src/rules/rewrite.rs` (with inline `#[cfg(test)] mod tests`) — apply_rewrite unit tests covering D-19 invariant
- [ ] `crates/lacon-adapter-claudecode/src/quote.rs` (with inline `#[cfg(test)] mod tests`) — POSIX round-trip tests for D-20

Framework install: none — `cargo test` + workspace `assert_cmd`/`predicates`/`tempfile` already wired.

## Sources

### Primary (HIGH confidence)
- `code.claude.com/docs/en/hooks` — PreToolUse stdin payload, `hookSpecificOutput` response, `updatedInput` REPLACE semantics, pass-through via empty exit 0. Verified 2026-05-16 via WebFetch.
- `crates/lacon-core/src/rules/loader.rs:127-151` — `RuleLoader::resolve` API. [VERIFIED via Read]
- `crates/lacon-core/src/rules/schema.rs:121-133` — `RewriteSpec` struct (CONTEXT cites 119-133; actual is 121-133, off-by-2 — minor). [VERIFIED]
- `crates/lacon-core/src/runtime/mod.rs:175` — `LACON_DISABLE` precedent (CONTEXT cites 157; actual is 175 — off-by-18). [VERIFIED]
- `crates/lacon-cli/tests/cli_surface.rs:11-41` — 6-command-cap test. [VERIFIED]
- `crates/lacon-cli/tests/cli_run.rs:1-8` — `assert_cmd::Command::cargo_bin` pattern. [VERIFIED]
- `bin/test_emitter/Cargo.toml` — `[[bin]]` outside `lacon-cli` precedent. [VERIFIED]
- `Cargo.toml` workspace root — `serde_json` NOT yet in `[workspace.dependencies]`. [VERIFIED]
- `crates/lacon-adapter-claudecode/src/lib.rs` — current stub. [VERIFIED]
- `crates/lacon-cli/src/commands/init.rs` — current stub. [VERIFIED]
- `crates/lacon-cli/src/commands/run.rs:270-272` — `LACON_ASSISTANT`/`LACON_SESSION_ID` already consumed by Phase 2 tracker assembly. [VERIFIED]
- `.claude/settings.local.json` — real-world example of array-of-matchers shape in this repo. [VERIFIED]
- `benches/cold_start.rs` + `benches/Cargo.toml` — Phase 1's cold-start probe pattern. [VERIFIED]

### Secondary (MEDIUM confidence)
- ecton.dev "Surprises in the Rust JSON Ecosystem" — `serde_json` is 1.6× faster than `simd-json` on small payloads. [VERIFIED via WebSearch result + corroborated by general knowledge of SIMD overhead]
- etalabs.net/sh_tricks.html — POSIX-portable single-quote escape via `'\''`. [CITED]
- `cargo search serde_json` — version 1.0.149 latest as of 2026-05-16. [VERIFIED via shell]
- `cargo search tempfile` — version 3.27.0 latest. [VERIFIED]
- `docs.rs/tempfile` — `NamedTempFile::persist` semantics. [CITED]

### Tertiary (LOW confidence — none used as authoritative)
- General training-data claims about bash quoting edge cases — all verified against POSIX spec or shell-escape GitHub issue before use.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — `serde_json` recommendation cross-verified via two independent sources (ecton.dev benchmark blog + general SIMD-overhead knowledge); `tempfile` already in workspace
- Architecture / chain splitter DFA: HIGH — derived directly from `docs/specs/chained-commands.md` test matrix + verified state-transition logic against POSIX bash spec
- Hook JSON contract: HIGH — verified verbatim against `code.claude.com/docs/en/hooks` via WebFetch
- TUI heuristic: HIGH — table content verbatim from spec; dispatcher logic is straightforward argv inspection
- `apply_rewrite` test cases: HIGH — invariants derived directly from D-19 + REQ-engine-rewrite + filter-rule-schema.md "add_flags is idempotent"
- `quote_for_shell`: HIGH — algorithm cited from etalabs.net + POSIX portability confirmed
- `.claude/settings.json` idempotency: HIGH — algorithm derived from D-12/D-28 + verified array-of-matchers shape against Claude Code docs + existing repo `.claude/settings.local.json`
- CLAUDE.md marker handling: MEDIUM — D-14 specifies markers; corrupt-state behavior is my recommendation (planner can adjust)
- Performance harness: HIGH — Phase 1 precedent (`benches/cold_start.rs`) directly extensible
- Test framework choices: HIGH — `assert_cmd`/`predicates`/`tempfile` all already in workspace per Phase 1 pattern

**CONTEXT line-citation discrepancies found (planner / executor heads-up):**
- CONTEXT cites `crates/lacon-core/src/rules/schema.rs:119-133` for RewriteSpec — actual is line 121 start (off by 2)
- CONTEXT cites `crates/lacon-core/src/runtime/mod.rs:157` for `LACON_DISABLE` precedent — actual is line 175 (off by 18)
- Neither discrepancy changes design; mention in plan files so executors don't waste time reconciling

**Research date:** 2026-05-16
**Valid until:** 2026-06-15 (30 days) for `serde_json` version recommendation; indefinite for the hook protocol (locked by ADR-0013 + verified Claude Code docs)
