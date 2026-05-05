# Architecture

## Overview

```
┌─────────────────────────────────────────────────────────────┐
│ Coding assistant (Claude Code, Cursor, ...)                 │
│                                                             │
│   ┌──────────┐   PreToolUse                                 │
│   │   Bash   ├──────────────┐                               │
│   │   tool   │              │                               │
│   │          ├─exec─┐  PostToolUse                          │
│   └──────────┘      │       │                               │
└─────────────────────┼───────┼───────────────────────────────┘
                      │       │
                      ▼       ▼
              ┌──────────────────────┐
              │   lacon (this tool)  │
              │  ┌────────────────┐  │
              │  │   Adapter      │  │  one per assistant
              │  └───────┬────────┘  │
              │          ▼           │
              │  ┌────────────────┐  │
              │  │  Rule resolver │◄─┼── rules/  (project, user, bundled)
              │  └───────┬────────┘  │
              │          ▼           │
              │  ┌────────────────┐  │
              │  │ Pipeline runner│◄─┼── primitives + Starlark VM
              │  └───────┬────────┘  │
              │          ▼           │
              │  ┌────────────────┐  │
              │  │    Tracker     │──┼──► history.db (SQLite)
              │  └────────────────┘  │
              └──────────────────────┘
```

## Components

### Core (assistant-agnostic)

**Rule resolver.** Loads rule files from project (`<cwd>/.lacon/rules/`), user (`~/.config/lacon/rules/`), and bundled (embedded in the binary). Resolves which rule applies to a given command via pattern match, with project > user > bundled precedence and first-match-wins. Caches compiled regexes; invalidates on rule file mtime change.

**Pipeline runner.** Streams stdout/stderr line-by-line through the rule's pipeline of native primitives. Maintains a bounded ring buffer for `keep_tail`. On non-zero exit, swaps in the `on_error` pipeline. After the native pipeline completes, optionally invokes the Starlark `post_process` function on the aggregated result.

**Tracker.** Records every invocation to SQLite. Cheap synchronous write on the hot path (single INSERT). Optional `raw_outputs` storage off by default.

**CLI.** `init`, `run`, `stats`, `explain`, `doctor`, `validate`. Read-only commands query SQLite directly; `init` writes adapter-specific hook config.

### Adapters (assistant-specific)

An adapter translates from "the assistant just ran a command and got this output" to a core invocation. For Claude Code (v1):

- `PreToolUse` hook receives the command, decides whether to bypass (`!!` prefix, env var, no matching rule), and may rewrite it
- `PostToolUse` hook receives the raw output and exit code, runs the pipeline, returns the filtered result

Adapters are otherwise dumb: they don't know about rules, primitives, or storage. The core engine exposes a single function: `process(invocation) -> filtered_output`.

## Lifecycle of an invocation

1. Claude Code is about to run `pnpm install --frozen-lockfile`.
2. `PreToolUse` hook fires. The Claude Code adapter:
   - Checks for `!!` prefix → not present, continue
   - Checks `LACON_DISABLE` env var → not set
   - Asks the rule resolver: which rule matches this command? → `pkg-install`
   - Applies rewrite step from the rule → command becomes `pnpm install --frozen-lockfile --reporter=silent`
   - Returns the rewritten command to Claude Code
3. Claude Code executes the (possibly rewritten) command.
4. `PostToolUse` hook fires with stdout, stderr, exit code, duration.
5. The adapter passes everything to the core's `process()` function:
   - Pipeline runner streams the output through the rule's stages
   - On non-zero exit, swaps to `on_error` pipeline
   - Optionally runs `post_process` Starlark
   - Applies `max_bytes` cap as final stage
6. Tracker writes a row to `invocations`. If raw output retention is enabled, writes the original stdout/stderr to `raw_outputs`.
7. Filtered output is returned to Claude Code, which puts it in the model's context.

## Configuration loading

Three layers, project highest priority:

| Layer | Location | Use |
|-------|----------|-----|
| Bundled | embedded in binary | shipped defaults |
| User | `~/.config/lacon/` | personal rules and overrides |
| Project | `<cwd>/.lacon/` | repo-specific rules |

Each layer can contain:

- `config.yaml` — global settings (retention, default `max_bytes`, raw-output storage on/off, etc.)
- `rules/*.yaml` — one rule per file (or several per file)
- `scripts/*.star` — Starlark files referenced by rules

Rule resolution: walk all three layers in priority order, collect rules whose `match` block matches the command, return the first one. `extends` is resolved relative to the layer the rule was loaded from (a project rule can `extends: bundled/pkg-install`).

## File layout (repository)

```
lacon/
├── README.md
├── docs/                          # this directory
├── crates/
│   ├── lacon-core/                # rule resolver, pipeline, tracker
│   ├── lacon-cli/                 # CLI commands
│   └── lacon-adapter-claudecode/  # Claude Code hook integration
├── bundled-rules/                 # YAML rules embedded into the binary
└── tests/
    ├── fixtures/                  # captured raw outputs of common commands
    └── integration/
```

## Streaming model

Output is processed line-by-line. Each native primitive is implemented as a streaming transformer: takes a line, may yield zero, one, or many lines downstream. The pipeline is a chain of these transformers.

This matters because:

- Long builds (multi-minute cargo builds, large pnpm installs) shouldn't be buffered fully in memory
- Memory usage stays bounded — `keep_tail N` only ever holds N lines, not the full output
- The `max_bytes` final stage doubles as a hard memory cap

The exception is the Starlark `post_process` step, which receives the aggregated already-reduced output. Per-line Starlark is too slow; running it on the post-pipeline result is fine because by that point the data is small.

## What's deliberately not in this doc

- The exact YAML schema → see [filter-rule-schema](specs/filter-rule-schema.md)
- The exact SQLite schema → see [tracking-data-model](specs/tracking-data-model.md)
- Why we chose Rust, hooks, Starlark, etc. → see [decisions](decisions/)
