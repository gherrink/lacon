# Phase 3: Claude Code adapter & `lacon init` - Context

**Gathered:** 2026-05-16 (assumptions mode)
**Status:** Ready for planning

<domain>
## Phase Boundary

A user runs `lacon init` in a fresh project and ends up with `.lacon/` skeleton, a Claude Code `PreToolUse(Bash)` hook entry in `.claude/settings.json`, and a `<!-- lacon:start --> ... <!-- lacon:end -->` block in CLAUDE.md. From then on, every Bash tool invocation flows through the Claude Code adapter:

1. Hook reads stdin JSON (`session_id`, `cwd`, `permission_mode`, `tool_input.{command, description, timeout, run_in_background}`, `tool_use_id`).
2. Bypass detection (`!!` prefix on `tool_input.command`, `LACON_DISABLE=1` from process env via the hook process — Claude Code inherits user env).
3. Chain-split the command at top-level `&&`/`||`/`;` (NOT inside quotes/`(...)`/`$(...)`/backticks/heredocs).
4. TUI heuristic per segment AFTER splitting BEFORE rule resolution; any match → whole-chain bypass.
5. Per matched segment: resolve rule via `lacon_core::rules::loader::RuleLoader::resolve`, apply `rewrite` block to inner argv (idempotent `add_flags`, `remove_flags`, `replace_flags`), re-quote and wrap as `lacon run --rule <id> -- <rewritten>`.
6. Reassemble chain joining segments with original operators preserved byte-exact.
7. Emit stdout JSON `{hookSpecificOutput: {hookEventName: "PreToolUse", permissionDecision: "allow", updatedInput: {<all original tool_input fields with command replaced>}}}` — OR `exit 0` with empty stdout for pass-through.

Subsumes: replacing the `ClaudeCodeAdapterStub` in `crates/lacon-adapter-claudecode`, adding a new `[[bin]] lacon-claude-hook`, adding `serde_json` to workspace deps, filling `crates/lacon-cli/src/commands/init.rs`, implementing the chain splitter (DFA), the TUI heuristic, the `rewrite` application function in `lacon-core`, and the shell-quote helper in the adapter.

Out of scope: `lacon stats` / `lacon explain` / `lacon doctor` (Phase 4), the 6-command-surface-cap finalization (Phase 4), bundled rule files (Phase 5), v2 adapters (Cursor / aider), `PostToolUse` annotation of unmatched commands (deferred to v1.5).

**Requirements covered:** REQ-adapter-pretooluse-only, REQ-adapter-bypass-detection, REQ-adapter-chained-commands, REQ-adapter-tui-bypass, REQ-adapter-pipes-passthrough, REQ-cli-init.
</domain>

<decisions>
## Implementation Decisions

### A. Adapter binary architecture

- **D-01:** Hook handler ships as a separate binary `lacon-claude-hook` inside `crates/lacon-adapter-claudecode` via a new `[[bin]]` target in that crate's `Cargo.toml`. **NOT** a `lacon hook` subcommand — that would break `crates/lacon-cli/tests/cli_surface.rs:11` which locks the 6-command CLI surface (REQ-cli-surface-cap).
- **D-02:** The hook binary depends only on `lacon-core` + `serde_json` + `serde`. It does NOT pull `rusqlite`, `starlark`, `os_pipe` (those are `lacon` binary's deps). Smaller dep graph → faster cold start on the hot path (cold-start ≤10ms is load-bearing per ADR-0013 + CON-nfr-cold-start-budget).
- **D-03:** Hook stdin/stdout protocol (locked by 2026-05-16 external research against `code.claude.com/docs/en/hooks.md`):
  - **Stdin:** `{session_id, transcript_path, cwd, permission_mode, hook_event_name, tool_name, tool_input, tool_use_id}`. Bash tool: `tool_input.{command: string, description?: string, timeout?: number, run_in_background?: boolean}`.
  - **Stdout (rewrite path):** `{"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "allow", "updatedInput": {<full tool_input echo-back with command field replaced>}}}`.
  - **Stdout (pass-through path):** empty (exit 0, no stdout). Used for: unmatched commands, bypass-detected, whole-chain TUI bypass.
  - `updatedInput` REPLACES the entire input object — `description`, `timeout`, `run_in_background` MUST be carried through when present. Dropping any field silently removes it from the tool call.
  - `hookEventName: "PreToolUse"` is REQUIRED on the output object. Pair `updatedInput` with `permissionDecision: "allow"` (otherwise the input may be shown to the user under `"ask"` or ignored under `"defer"`).
- **D-04:** Adapter crate Cargo.toml additions:
  - `[[bin]]` table for `name = "lacon-claude-hook"`, `path = "src/bin/hook.rs"`.
  - `serde_json` from workspace (NEW workspace dep — `serde_json` is not currently in `[workspace.dependencies]`; add to root `Cargo.toml` first).
  - `serde` with `derive` (workspace dep, already present).
- **D-05:** Project precedent for a separate-binary-in-crate pattern: `bin/test_emitter/Cargo.toml` (read during analysis). The hook binary follows the same shape: `[[bin]] name = "lacon-claude-hook" path = "src/bin/hook.rs"`.

### B. Chain splitter implementation

- **D-06:** Hand-rolled state-machine splitter in `crates/lacon-adapter-claudecode/src/chain.rs`. Operates on the **raw command string** (UTF-8 byte iteration with code-point boundary respect), NOT on pre-tokenized argv. State tracks: single-quote, double-quote, `(...)` subshell depth, `$(...)` cmd-sub depth, backtick depth, heredoc body. Emits a split only when encountering `&&`/`||`/`;` at depth 0 outside all quote/heredoc states.
- **D-07:** Output shape: `Vec<Segment>` where `Segment { text: String, trailing_op: Option<ChainOp> }` and `enum ChainOp { AndAnd, OrOr, Semi }`. Each segment's `text` is the verbatim byte slice from the original input — preserves spacing, quoting, and operator whitespace untouched. The chain reassembly walks segments and joins each `text` with its `trailing_op`'s literal form (`" && "`, `" || "`, `"; "` or whatever the original used — preserved by remembering the operator's original span).
- **D-08:** A small **secondary** tokenizer `argv_for_resolution(seg: &str) -> Vec<String>` runs ONLY on segments that need rule resolution. It handles single/double quoting and `$(...)` opacity. Its output is fed to `RuleLoader::resolve` and `apply_rewrite`. The segment's original `text` is preserved for reassembly when unchanged; matched-and-rewritten segments produce a NEW shell-quoted string via D-15 (quote_for_shell), wrapped as `lacon run --rule <id> -- <quoted-argv>`.

    **Revised 2026-05-16 (Phase 3 planning):** for v1, `argv_for_resolution` handles single + double quoting only; `$(...)` is treated as part of the surrounding token (NOT opaquely state-machine-tracked at the resolver-input level). Rationale: rule predicates today are whitespace-token-based (`command` + `args_prefix` + `args_contain` + `command_regex`) and `$(...)` rarely appears as a top-level argv element in real-world commands; promoting to full `$(...)` opacity at the resolver tokenizer is deferred to v1.5+ if a real-world rule needs it. This deviation does NOT change the chain splitter's `$(...)` opacity in `chain.rs` — the top-level operator-detection DFA (D-06) remains locked with full opacity, so chain splits like `foo $(bar && baz) && qux` still produce exactly 2 segments (not 4). Only the secondary tokenizer is scope-reduced.
- **D-09:** Pipes (`|`) are NOT chain operators — they are consumed verbatim into the current segment text. A pipeline like `pnpm test | grep foo` is one segment; the rule's `match` predicate sees the whole pipeline. Per REQ-adapter-pipes-passthrough, filtering inside pipes is out of v1 scope.
- **D-10:** 13-scenario test matrix from `docs/specs/chained-commands.md:122-138` is the splitter's test gate. Each scenario becomes one row in a table-driven test in `crates/lacon-adapter-claudecode/tests/chain_split.rs`. Tests assert: split point positions, segment count, operator types, and that opaque-construct constents are preserved verbatim in their containing segment.

### C. `lacon init` strategy for `.claude/settings.json`

- **D-11:** Full JSON parse via `serde_json::Value`. Reads `.claude/settings.json` if present; creates `{}` if missing. The hook entry is inserted/replaced inside `hooks.PreToolUse[]` (the exact array-of-matchers shape — confirmed by external research):

  ```json
  {
    "hooks": {
      "PreToolUse": [
        {
          "matcher": "Bash",
          "hooks": [
            {
              "type": "command",
              "command": "lacon-claude-hook"
            }
          ]
        }
      ]
    }
  }
  ```

- **D-12:** Idempotency strategy: on second `lacon init`, walk `hooks.PreToolUse[]`. For each matcher-group with `matcher == "Bash"`, filter out inner `hooks[]` entries whose `command` field starts with the substring `lacon-claude-hook`. Re-insert the current desired entry. This uses the **command-string itself** as the lacon-managed fingerprint — avoids the unknown-key risk flagged by external research (the `_lacon_managed: true` sibling marker is permissive-but-undocumented territory, and could break on a future schema tightening). User-authored non-lacon `PreToolUse(Bash)` hooks are preserved alongside our entry.
- **D-13:** File write semantics: 2-space indent (Claude Code's conventional style), trailing newline. Write atomically via `tempfile::NamedTempFile::persist` to avoid partial-write corruption on concurrent edits. If `.claude/` directory doesn't exist, create it.
- **D-14:** CLAUDE.md handling per REQ-cli-init "adds a tiny CLAUDE.md instruction line." Append at the bottom of `<cwd>/CLAUDE.md` (or create the file if missing) inside HTML-comment markers — markdown supports HTML comments verbatim, and they survive any renderer:

  ```markdown
  <!-- lacon:start -->
  Bash output filtered by lacon. Bypass one command with `!!` prefix, or set `LACON_DISABLE=1` to disable filtering entirely.
  <!-- lacon:end -->
  ```

  Idempotency: detect the `<!-- lacon:start -->...<!-- lacon:end -->` block via string scan, replace contents in place; outside content is untouched. If neither marker exists, append at EOF.

### D. TUI heuristic location & test strategy

- **D-15:** `is_tui(command: &str, args: &[String]) -> bool` is a public function in `crates/lacon-adapter-claudecode/src/tui.rs`. NOT in `lacon-core` — `docs/specs/chained-commands.md:104` is explicit: "The list lives in adapter code." YAGNI on cross-adapter reuse.
- **D-16:** Pure-TUI list is a `const PURE_TUI: &[&str] = &[...]` table of the 22 basenames from `docs/specs/chained-commands.md:85-89`:

  `vim`, `vi`, `nvim`, `nano`, `emacs`, `less`, `more`, `most`, `man`, `htop`, `top`, `btop`, `screen`, `tmux`, `ssh`, `mosh`, `ipython`, `irb`, `pry`, `redis-cli`, `crontab`, `visudo`.

  Lookup is by `basename(args[0])` — extract via `std::path::Path::file_name`.
- **D-17:** Conditional patterns dispatched via `match command { "git" => match_git_subcmd(args), "npm" | "yarn" | "pnpm" => match_pkg_init(args), "node" | "python" | "python3" => is_repl(args), "mysql" | "psql" | "sqlite3" => is_repl(args), _ => false }`. Coverage exact-matches the spec table at `docs/specs/chained-commands.md:91-101`.
- **D-18:** Test layout:
  - `crates/lacon-adapter-claudecode/tests/chain_split.rs` — the 13-scenario matrix.
  - `crates/lacon-adapter-claudecode/tests/tui_heuristic.rs` — one test per row of the spec's TUI table (pure-TUI basenames + conditional patterns + negative tests for non-TUI lookalikes).
  - `crates/lacon-adapter-claudecode/tests/hook_e2e.rs` — end-to-end: pipe a fixture JSON into `lacon-claude-hook` (via `assert_cmd::Command::cargo_bin("lacon-claude-hook")`, mirroring Phase 1's `cli_run.rs` pattern), assert stdout JSON shape + exit code.
  - `crates/lacon-cli/tests/cli_init.rs` — end-to-end: run `lacon init` in a tempdir, verify `.lacon/`, `.claude/settings.json` shape, CLAUDE.md block. Re-run, verify idempotent no-op (file mtime may change due to atomic write — assert content equality, not mtime).

### E. `rewrite` block application & argv re-quoting

- **D-19:** Add `lacon_core::rules::rewrite::apply_rewrite(argv: &[String], rewrite: &RewriteSpec) -> Vec<String>` (new file `crates/lacon-core/src/rules/rewrite.rs`). Operations:
  1. `add_flags`: append each flag to argv ONLY if not already present (string-equal anywhere in `argv[1..]`). **Idempotent.**
  2. `remove_flags`: filter out matching flags from `argv[1..]`.
  3. `replace_flags`: walk argv; for each `old → new` mapping in the spec, replace any string-equal occurrence in `argv[1..]`.
  4. `argv[0]` (command) is never touched.
  - Idempotency invariant: `apply(apply(x)) == apply(x)` for any rewrite block and argv. Locked by a regression test.
- **D-20:** `quote_for_shell(arg: &str) -> Cow<str>` lives in `crates/lacon-adapter-claudecode/src/quote.rs`. Rule: if `arg` contains no whitespace and no shell metacharacters (`|&;<>()$\`\\\"'\n\t*?[#~=%!`), return `Cow::Borrowed`. Otherwise single-quote-wrap and replace embedded `'` with `'\''`. POSIX-portable (sh, bash, dash, zsh).
- **D-21:** Adapter emits the rewritten segment as `lacon run --rule <id> -- <quoted argv joined with single spaces>`. For unchanged segments (no rule match), the segment's original `text` is preserved byte-exact — no round-trip through quoting.
- **D-22:** Security/injection note: `quote_for_shell`'s correctness is part of the trust property. Phase 1's Runner already enforces `Command::new(&argv[0]).args(&argv[1..])` (no re-shell-interpret) per `crates/lacon-core/src/runtime/mod.rs:138-141` — the adapter's quoting only needs to survive ONE shell parse (Claude Code's shell invocation of `lacon run`), not two. Test obligation: fixture with `--reporter='custom reporter'`, args containing `$()`, args with embedded quotes — assert round-trip through `quote_for_shell + sh -c '...'` produces the original argv.

### F. Bypass detection

- **D-23:** `!!` prefix detection on `tool_input.command` (LSTRIP whitespace first, then check `starts_with("!!")`). On detect → emit empty stdout, exit 0 (pass-through).
- **D-24:** `LACON_DISABLE=1` from the hook process's environment (Claude Code spawns hooks with the user's environment inherited). The hook reads `std::env::var("LACON_DISABLE")` and treats `Ok("1")` (exact string) as bypass — same semantics as `crates/lacon-core/src/runtime/mod.rs:157` (Phase 1 precedent). Other values (empty, "0", "true") do NOT bypass.
- **D-25:** Bypass is **whole-command** granularity (per `docs/specs/chained-commands.md:62-66` + REQ-engine-bypass). When `!!` is detected on the command string OR `LACON_DISABLE=1` is set, the entire input bypasses — no chain splitting, no rule resolution, no rewrites. Hook exits with empty stdout immediately (cheapest possible hot path).

### G. Env-var contract handoff to tracker (Phase 2 integration)

- **D-26:** When the adapter wraps a matched segment as `lacon run --rule <id> -- <inner>`, it inherits Claude Code's process env via the shell. To populate Phase 2's tracker fields (`invocations.assistant`, `invocations.session_id`), the rewritten command MUST carry the env vars per Phase 2 D-17. Two options for transport:
  1. **Prepend env vars to the rewritten command** — `LACON_ASSISTANT=claude-code LACON_SESSION_ID=<id> lacon run --rule <id> -- <inner>`. Inline; survives Claude Code's shell exec.
  2. **Adapter exports env vars in the hook process** — won't propagate (each Claude Code shell invocation is a fresh `bash -c`, no inherited adapter env).

  Decision: option 1 (inline prepend). The adapter reads `session_id` from stdin JSON and synthesizes the prefix. `LACON_ASSISTANT=claude-code` is hardcoded by the adapter. `tool_use_id` is also worth capturing for stronger tracker correlation — add `LACON_TOOL_USE_ID=<id>` if the planner agrees (otherwise drop and revisit).
- **D-27:** Phase 2's D-17 already says: "Phase 3 (the adapter) MUST satisfy" the env-var contract. D-26 is the concrete satisfaction.

### H. Idempotency resolution (Q-deferred-init-idempotency)

- **D-28:** Q-deferred-init-idempotency (from `docs/open-questions.md:23-27`) is settled here as:
  - Detect lacon-managed `hooks.PreToolUse[].hooks[]` entries by **command-string prefix** (`starts_with("lacon-claude-hook")`), NOT by a `// lacon:hook` comment marker (impossible — JSON doesn't permit comments) and NOT by a `_lacon_managed: true` sibling field (undocumented schema tolerance per external research, Medium confidence).
  - Strip all matching entries on re-run, re-insert the current desired entry. Result: re-running `lacon init` produces a settings.json byte-stable with the previous successful run when the lacon version is unchanged. The CLAUDE.md block is also detect-and-replace per D-14.
  - User-authored hooks in `hooks.PreToolUse[]` (matcher != "Bash" OR matcher == "Bash" but command doesn't start with `lacon-claude-hook`) are preserved untouched.

### Implementation-time benchmarks for the planner to schedule into Phase 3

These are not gating decisions but measurements to take during Phase 3 work:

1. **`lacon-claude-hook` cold-start** — measure `lacon-claude-hook < pass-through-fixture.json` median over 50 runs against the 10ms budget. Pass-through is the cheapest branch (exit 0, no JSON emit) and is the most common in practice (most commands won't have a rule until Phase 5 lands the bundled rules). Target: ≤2ms median in release build. If exceeded, investigate `serde_json` startup cost vs. hand-rolled JSON parsing for the stdin payload only.
2. **Rewrite-path cold-start** — same fixture but the command matches a rule and emits JSON. Target: ≤5ms median.
3. **Chain-splitter throughput on pathological inputs** — `((a && b) && (c && d)) ; e || f && g | h` and similar nested forms. Confirm DFA stays linear in command length.

### Claude's discretion

- Internal module organization under `crates/lacon-adapter-claudecode/src/` — `chain.rs`, `tui.rs`, `quote.rs`, `protocol.rs` (stdin/stdout JSON structs), `bin/hook.rs` (entry), `lib.rs` (orchestration). Planner organizes for readability without re-litigating these boundaries.
- Exact wording of the CLAUDE.md instruction line (D-14) — must mention `!!` and `LACON_DISABLE=1` per the user-trust property; exact phrasing left to author.
- Choice between `serde_json::Value` and a typed `#[derive(Deserialize)]` struct for the stdin payload — both work; typed struct catches schema drift earlier but adds maintenance cost when Claude Code adds new fields. Pick by preference.
- Atomic-write strategy for `.claude/settings.json` — `tempfile + persist` vs `std::fs::write` (race-prone on concurrent `claude` startup). Recommendation: `tempfile`, but the failure mode without it is rare. Pick by preference.
- Whether to capture `tool_use_id` into `LACON_TOOL_USE_ID` env var for the tracker (D-26 trailing). Adds a column-correlation property for Phase 4's `lacon explain`; not strictly required for Phase 3 to compile.

### Folded todos

None — `gsd-sdk query todo.match-phase 3` returned 0 matches.
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### External docs (verified 2026-05-16 via WebFetch)

- `code.claude.com/docs/en/hooks.md` — hook registration schema, stdin/stdout payload shapes, `hookSpecificOutput.updatedInput` semantics
- `code.claude.com/docs/en/settings.md` — `.claude/settings.json` schema and the JSON-schema-warns-but-doesn't-reject behavior

### ADRs (LOCKED, all status: Accepted)

- `docs/decisions/0001-use-claude-code-hooks.md` — original hook decision, narrowed by ADR-0013 to `PreToolUse` only
- `docs/decisions/0004-config-precedence.md` — project > user > bundled
- `docs/decisions/0006-hybrid-rewrite-and-filter.md` — `rewrite` is first-class alongside `pipeline`; adapter applies rewrite
- `docs/decisions/0007-first-match-wins.md` — per-segment resolution; first match wins, no merging
- `docs/decisions/0013-filter-via-pretooluse-wrapper.md` — hook protocol details, `lacon run --rule <id> -- <cmd>` wrapping contract, `hookSpecificOutput.updatedInput` replaces entire input

### Specs (load-bearing contract)

- `docs/specs/chained-commands.md` — splitter rules, opaque constructs, 13-scenario test matrix, TUI list (`docs/specs/chained-commands.md:85-101`), bypass semantics
- `docs/specs/filter-rule-schema.md` — `rewrite` block shape (`add_flags`, `remove_flags`, `replace_flags`); `add_flags` is idempotent
- `docs/specs/config-schema.md` — config layer merge (consumed unchanged here)
- `docs/specs/tracking-data-model.md` — `invocations.assistant`, `invocations.session_id` columns the adapter must populate via env-var contract (Phase 2 D-17)

### Architecture and project context

- `docs/architecture.md` — adapter responsibilities (lines 72-75); explicit "adapter applies rewrite before wrapping"
- `docs/v1-scope.md` — explicit in/out of scope
- `docs/open-questions.md:23-31` — Q-deferred-init-idempotency (settled in D-28)
- `docs/backlog.md` — granular per-segment TUI bypass (v2), user-overridable TUI list (v2)
- `.planning/PROJECT.md`, `.planning/REQUIREMENTS.md`, `.planning/ROADMAP.md`
- `.planning/phases/01-engine-core-lacon-run-wrapper/01-CONTEXT.md` — Phase 1 D-01..D-18 (dep set, resolver API, `!!`/`LACON_DISABLE` runtime precedent)
- `.planning/phases/02-local-tracking/02-CONTEXT.md` — Phase 2 D-17 env-var contract Phase 3 satisfies
- `.planning/intel/constraints.md` — full CON-chained-* (8), CON-nfr-cold-start-budget

### Existing source files Phase 3 directly extends or replaces

- `crates/lacon-adapter-claudecode/src/lib.rs` — current stub `ClaudeCodeAdapterStub`; REPLACED in Phase 3
- `crates/lacon-adapter-claudecode/Cargo.toml` — needs `serde_json` workspace dep + new `[[bin]] lacon-claude-hook`
- `crates/lacon-cli/src/commands/init.rs` — current stub `eprintln!("not yet implemented"); Ok(2)`; REPLACED in Phase 3
- `crates/lacon-cli/src/cli.rs:33-35` — `Init` subcommand already declared; may need flags (e.g., `--force`, `--dry-run`) — planner's call
- `crates/lacon-cli/tests/cli_surface.rs:11-41` — 6-command-cap test; Phase 3 must not break this (no new subcommand)
- `crates/lacon-core/src/rules/loader.rs:127-151` — `RuleLoader::resolve` API the adapter calls
- `crates/lacon-core/src/rules/schema.rs:119-133` — `RewriteSpec` struct; D-19 adds `apply_rewrite` next to this
- `crates/lacon-core/src/runtime/mod.rs:157` — `LACON_DISABLE` precedent (env-var detection logic to mirror)
- `Cargo.toml` (workspace root) — add `serde_json = { version = "...", default-features = false }` to `[workspace.dependencies]`
- `bin/test_emitter/Cargo.toml` — precedent for `[[bin]]` outside `lacon-cli`
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- **`lacon_core::rules::loader::RuleLoader`** (`crates/lacon-core/src/rules/loader.rs`) — Phase 1's resolver. `RuleLoader::new(project_path)` + `resolve(rule_id)` (hot path, parses only the matching file) + `load_all()` (eager). The adapter calls `load_all()` to find a matching rule for a given command (no `--rule` hint from Claude Code), then `resolve` could be used if the matched rule_id is known.
- **`RewriteSpec`** (`crates/lacon-core/src/rules/schema.rs:119-133`) — schema already parses `add_flags`, `remove_flags`, `replace_flags`. Phase 3 implements the application logic (D-19).
- **`LACON_DISABLE` env-var detection** (`crates/lacon-core/src/runtime/mod.rs:157`) — Phase 1 precedent for the env-var check; the adapter mirrors this pattern.
- **`Command::new(&argv[0]).args(&argv[1..])` security pattern** (`crates/lacon-core/src/runtime/mod.rs:138-141`) — Phase 1's argv-injection mitigation. Adapter doesn't directly use this (it emits a shell command string), but the design assumes ONE shell parse hop downstream (Claude Code's bash invocation of `lacon run`).
- **`assert_cmd::Command::cargo_bin`** for invoking workspace binaries from integration tests — Phase 1's `cli_run.rs` pattern. Used in `tests/hook_e2e.rs` for `lacon-claude-hook`.
- **`bin/test_emitter/Cargo.toml`** — precedent for adding a `[[bin]]` target in a non-CLI crate.

### Established Patterns

- **No async runtime** (Phase 1 D-04) — hook is sync. `serde_json::from_reader(std::io::stdin().lock())` for stdin parse; `serde_json::to_writer_pretty(std::io::stdout().lock(), &response)` for emit.
- **Lazy-resolve-on-the-hot-path** (Phase 1 D-14) — adapter uses `load_all` for matching (no `--rule` hint from Claude Code), but matched rule resolves to its compiled form once.
- **`thiserror` inside crates, `anyhow` at the binary boundary** (Phase 1 D-03) — adapter follows: `HookError` (thiserror), `main.rs` returns `anyhow::Result<()>`.
- **Bundled-assets-via-const / rust-embed** (Phase 1 D-03) — adapter has no bundled assets; the TUI list is `const &[&str]`.
- **6-subcommand surface cap test** (`crates/lacon-cli/tests/cli_surface.rs`) — Phase 3 must NOT add a subcommand. The hook is a separate binary.
- **Atomic-write-via-tempfile** — not yet used in the project; Phase 3 may introduce `tempfile` workspace dep for `.claude/settings.json` writes.
- **JSON tooling** — `serde_json` is NOT currently a workspace dep. Phase 3 adds it (used by adapter for hook protocol AND by `lacon init` for `.claude/settings.json` parse/write).

### Integration Points

Phase 3 outputs that downstream phases consume:

- **For Phase 4 (`lacon doctor`):** `lacon doctor` verifies the hook is installed and the file paths are valid. Phase 3 establishes the canonical settings.json shape and the `lacon-claude-hook` command-string fingerprint that doctor uses for detection.
- **For Phase 4 (`lacon explain`):** the env-var `LACON_TOOL_USE_ID` (if D-26 trailing is adopted) becomes the strongest cross-correlation between Claude Code's tool history and lacon's `invocations` rows. Without it, correlation is by `session_id + ts`.
- **For Phase 5 (bundled rules):** the chain-splitter's reassembly is the production hot path; Phase 5's fixtures don't exercise it directly, but the end-to-end acceptance tests (Phase 6) do.
- **For Phase 6 (acceptance):** REQ-acceptance-pnpm-end-to-end is the gate — `lacon init` → `pnpm install` → filtered output reaches assistant. Phase 3 must make this work without manual config.

### Performance contract

The cold-start budget is load-bearing (CON-nfr-cold-start-budget, ADR-0013). Phase 1 baseline: `--version` 1154µs, `validate` 1259µs. Phase 2 baseline: tracker open + INSERT adds ≤25ms on first run (ext4 fsync dominant), expected microseconds on warm cache.

For `lacon-claude-hook`:
- **Pass-through branch** (no match, bypass-detected, TUI-bypassed) is the most common case until Phase 5 bundled rules land. Target ≤2ms median.
- **Rewrite branch** (matched + chain-splitter + apply_rewrite + JSON emit) targets ≤5ms median.

Total user-facing budget = `lacon-claude-hook cold start` + Claude Code's shell-spawn for `lacon run` + Phase 1 `lacon run` cold start (≤1.2ms) + Phase 2 tracker write (≤25ms first ever, microseconds warm). The hook's own budget is therefore ≤7-8ms to keep end-to-end under 10ms-on-the-hot-path (the 10ms is the hook's, per ADR-0013; the actual subprocess execution time is the user's command's runtime, not lacon's).
</code_context>

<specifics>
## Specific Ideas

External research (2026-05-16) confirmed Claude Code hook schema details that elevate ADR-0013's empirical claim to doc-confirmed contract:

- `updatedInput` REPLACES the entire input object (verbatim quote: "Replaces the entire input object, so include unchanged fields alongside modified ones") — this is now a hard implementation requirement, not an inferred best-practice. D-03 captures this.
- The pass-through path is `exit 0` with empty stdout — the cheapest hot-path branch. D-03 captures this.
- The hook registration schema is array-of-matchers, NOT object-keyed. D-11 captures the exact JSON shape.

Other than these, no specific user references — assumptions confirmed as-is on first pass. Approaches above are derived from locked ADRs/specs + verified external doc + the patterns Phase 1/2 established.

**Suggested followup for `docs/decisions/0013-filter-via-pretooluse-wrapper.md`:** add the verbatim doc quote about `updatedInput`'s replacement semantics — currently the ADR records the 2026-05-05 empirical probe; the 2026-05-16 doc fetch makes it a primary-source citation worth adding. (Planner's call whether to schedule this as a doc task in Phase 3 or defer to Phase 6 docs work.)
</specifics>

<deferred>
## Deferred Ideas

- **`PostToolUse` annotation of unmatched commands** — explicitly v1.5 backlog per ADR-0013. "lacon could have stripped ~3 kB if it had a rule for this" feedback loop. Adapter shape stays compatible (just adds a second hook entry in `lacon init`'s settings.json write).
- **Granular per-segment TUI bypass** — v2 backlog per `docs/specs/chained-commands.md:81` + `docs/backlog.md`. Gated on tracking data showing the lost filtering opportunity is material.
- **User-overridable TUI list** — v2 backlog. Deferred until clear false-positive pattern emerges.
- **Cursor / aider adapters** — v2. The TUI heuristic (D-15) stays in `lacon-adapter-claudecode` for v1; v2 either duplicates or refactors into core at refactor time.
- **`_lacon_managed: true` settings.json sibling marker** — rejected in favor of command-string fingerprint (D-12), per external research's Medium-confidence finding that unknown-key tolerance is permissive-but-undocumented. Revisit if Claude Code formalizes a managed-metadata field.
- **Conch-parser / full bash AST** — rejected for v1 (D-06) on cold-start grounds. Revisit only if the hand-rolled DFA exhibits a pattern of correctness bugs in the 13-scenario matrix.
- **Shlex / shell-words crate deps** — rejected (D-06). Hand-rolled DFA owns the splitter; hand-rolled `quote_for_shell` (D-20) owns the inverse.
- **Adapter trait in `lacon-core`** — premature abstraction with one adapter. v2 problem.
- **Heredoc/subshell/eval handling for inner-segment filtering** — explicitly v2 backlog per `docs/specs/chained-commands.md:30`.
- **`tool_use_id` capture for tracker correlation** — listed in D-26 as Claude's discretion. If adopted, becomes a column-correlation property for Phase 4's `lacon explain`; if not, defer to Phase 4 or v1.5.

### Reviewed Todos (not folded)

None reviewed — `gsd-sdk query todo.match-phase 3` returned 0 matches.
</deferred>
