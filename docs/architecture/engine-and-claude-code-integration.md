---
cites: [adr:0013-filter-via-pretooluse-rewritten]
schema-version: 1
---

# Engine and Claude Code integration

## Overview

`lacon` sits between a coding assistant and the shell. The core engine is assistant-agnostic; a thin per-assistant adapter translates the assistant's hook contract into a rewritten command that invokes the engine. Per ADR 0013, filtering happens inside a subprocess wrapper (`lacon run`) invoked by a `PreToolUse`-rewritten command — not inside a `PostToolUse` hook (empirical testing showed `PostToolUse` cannot replace tool output).

Flow for Claude Code (v1): the `PreToolUse` hook receives the Bash tool's command; the adapter checks `!!` / `LACON_DISABLE`, resolves a rule (project > user > bundled, first-match-wins), applies the rule's `rewrite` block to the inner argv, and rewrites a matched command to `lacon run --rule <id> -- <inner-cmd>`, returned as `hookSpecificOutput.updatedInput` (which replaces the whole input object, so unchanged fields are echoed back). The shell executes the rewritten command; `lacon run` spawns the subprocess with stderr merged into stdout, streams the merged output through the rule's pipeline (or the `on_error` pipeline on non-zero exit), optionally runs the Starlark `post_process` stage on the aggregated result, applies the `max_bytes` cap, writes a tracking row to SQLite, writes filtered bytes to its own stdout (captured by Claude Code as the tool result), and exits with the subprocess's exit code.

Configuration is three layers — bundled (embedded), user (`~/.config/lacon/`), project (`<cwd>/.lacon/`) — each optionally holding `config.yaml`, `rules/*.yaml`, and `scripts/*.star`; resolution walks the layers in priority order and returns the first matching rule, with `extends` resolved relative to the loading layer. Output is processed streaming (line-by-line transformers), so memory stays bounded on long builds — `keep_tail N` holds only N lines and `max_bytes` is a hard cap; the Starlark stage is the deliberate exception, running once on the already-reduced aggregate. The exact rule and SQLite schemas live in the filter-rule-schema and tracking-data-model specs; the rationale for Rust, hooks, and Starlark lives in the ADRs under `docs/decisions/`. A load-bearing constraint throughout is the ≤ 10 ms cold-start budget on the hook hot path, guarded by a deterministic in-process `tracker_open` steady-state benchmark (`cargo bench -p lacon-core --bench tracker_open`); wall-clock cold-start figures are soft-reported.

## Components

### Rule resolver  {#rule-resolver}

Loads rule files from project (`<cwd>/.lacon/rules/`), user (`~/.config/lacon/rules/`), and bundled (embedded) layers and resolves which rule applies to a command via pattern match, with project > user > bundled precedence and first-match-wins. Caches compiled regexes and invalidates on rule-file mtime change; flattens `extends` chains at load time.

<!-- fields -->
- implemented-by: crates/lacon-core/src/rules/loader.rs#RuleLoader

### Pipeline runner  {#pipeline-runner}

Streams the merged stdout/stderr line-by-line through the rule's chain of native primitives (`strip_ansi`, `drop_regex`, `keep_regex`, `replace_regex`, `dedupe`, `collapse_repeated`, `keep_head`/`keep_tail`, `keep_around_match`, `max_bytes`), holding only bounded state such as the `keep_tail` ring buffer. On non-zero exit it swaps in the rule's `on_error` pipeline (replace, not merge).

<!-- fields -->
- implemented-by: crates/lacon-core/src/pipeline/mod.rs#Pipeline

### Starlark host (post_process)  {#starlark-host-post-process}

Runs the optional Starlark `post_process` function once on the aggregated, already-reduced output — the deliberate exception to streaming. Hermetic by construction (no I/O, clock, or network). Per ADR 0008 the escape hatch is aggregated rather than per-line, because per-line Starlark would dominate runtime at typical output volumes.

<!-- fields -->
- implemented-by: crates/lacon-core/src/starlark_host/mod.rs#StarlarkScript

### Tracker  {#tracker}

Records every invocation to SQLite (`~/.local/share/lacon/history.db`, WAL mode) with a cheap synchronous single-INSERT write on the hot path. Optional `raw_outputs` storage is off by default. Pruning runs on startup; the read side backs `lacon stats` and `lacon explain`. `Tracker::open` on the steady-state path is the deterministic cold-start gate.

<!-- fields -->
- implemented-by: crates/lacon-core/src/tracking/mod.rs#Tracker

### CLI  {#cli}

The `lacon` binary and its six subcommands: `init` (writes adapter hook config), `run` (the production wrapper and manual-debug entry), and the read-only `stats`, `explain`, `doctor`, and `validate`, which query SQLite or the rule set directly.

### Claude Code adapter  {#claude-code-adapter}

Translates Claude Code's `PreToolUse` hook contract into a rewritten command. It checks `!!` / `LACON_DISABLE`, splits top-level command chains, runs the TUI-bypass heuristic, resolves matched segments, applies each rule's `rewrite` block, wraps matched segments as `lacon run --rule <id> -- <seg>`, and returns `hookSpecificOutput.updatedInput`. It is otherwise dumb — it knows nothing about primitives or storage. No `PostToolUse` hook is installed in v1.

<!-- fields -->
- implemented-by: crates/lacon-adapter-claudecode/src/lib.rs#run_hook

### Wrapper (lacon run)  {#wrapper-lacon-run}

`lacon run --rule <id> -- <cmd> [args...]` spawns the subprocess, reads its merged stdout+stderr line-by-line (best-effort line atomicity, no cross-stream order guarantee), runs the success or `on_error` pipeline selected by the exit code, writes filtered bytes to its own stdout, writes the tracking row, forwards SIGTERM/SIGINT to the child, and exits with the subprocess's exit code. Without `--rule` it runs the resolver inline — the same code path the hook uses.

<!-- fields -->
- implemented-by: crates/lacon-cli/src/commands/run.rs#execute
