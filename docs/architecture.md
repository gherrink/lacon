# Architecture

> **Updated 2026-05-05** per [ADR 0013](decisions/0013-filter-via-pretooluse-wrapper.md). Filtering happens inside a subprocess wrapper (`lacon run`) invoked by a `PreToolUse`-rewritten command, not inside a `PostToolUse` hook. Empirical testing showed `PostToolUse` cannot replace tool output, so the original "hook responds with filtered bytes" flow was abandoned. The internal pipeline, primitives, Starlark stage, tracker, and rule schema are unchanged — only their execution location moved.

## Overview

```
┌─────────────────────────────────────────────────────────────┐
│ Claude Code                                                 │
│                                                             │
│   Bash tool                                                 │
│      │                                                      │
│      │ PreToolUse (command + tool_input)                    │
│      ▼                                                      │
│   ┌────────────────────────────────────────┐                │
│   │ lacon adapter (Claude Code-specific)   │                │
│   │  - resolves rule (project > user > bundled) │           │
│   │  - applies rewrite block (flag add/remove)  │           │
│   │  - if matched: rewrites command to          │           │
│   │      lacon run --rule <id> -- <inner-cmd>   │           │
│   │  - if !! prefix or LACON_DISABLE: passes through │      │
│   │  - returns hookSpecificOutput.updatedInput  │           │
│   └────────────────┬───────────────────────┘                │
│                    │                                        │
│                    ▼                                        │
│            shell exec                                       │
│                    │                                        │
└────────────────────┼────────────────────────────────────────┘
                     │  (only when wrapped)
                     ▼
       ┌──────────────────────────────┐
       │  lacon run                   │
       │  ┌────────────────────────┐  │
       │  │  spawned subprocess    │  │  stdout + stderr
       │  │  (the original cmd)    │──┼──  merged via 2>&1
       │  └────────────┬───────────┘  │
       │               ▼              │
       │  ┌────────────────────────┐  │
       │  │  Pipeline runner       │◄─┼── primitives + Starlark VM
       │  │  (success or on_error  │  │
       │  │   selected by exit code)│ │
       │  └────────────┬───────────┘  │
       │               ▼              │
       │  ┌────────────────────────┐  │
       │  │  Tracker               │──┼──► history.db (SQLite)
       │  └────────────┬───────────┘  │
       │               ▼              │
       │       filtered stdout        │
       └───────────────┬──────────────┘
                       │
                       ▼
              tool_result Claude Code
              captures and shows the model
```

## Components

### Core (assistant-agnostic)

**Rule resolver.** Loads rule files from project (`<cwd>/.lacon/rules/`), user (`~/.config/lacon/rules/`), and bundled (embedded in the binary). Resolves which rule applies to a given command via pattern match, with project > user > bundled precedence and first-match-wins. Caches compiled regexes; invalidates on rule file mtime change.

**Pipeline runner.** Streams stdout/stderr line-by-line through the rule's pipeline of native primitives. Maintains a bounded ring buffer for `keep_tail`. On non-zero exit, swaps in the `on_error` pipeline. After the native pipeline completes, optionally invokes the Starlark `post_process` function on the aggregated result.

**Tracker.** Records every invocation to SQLite. Cheap synchronous write on the hot path (single INSERT). Optional `raw_outputs` storage off by default.

**CLI.** `init`, `run`, `stats`, `explain`, `doctor`, `validate`. Read-only commands query SQLite directly; `init` writes adapter-specific hook config.

### Adapters (assistant-specific)

An adapter translates the assistant's hook contract into a rewritten command that invokes `lacon run`. For Claude Code (v1):

- `PreToolUse` hook receives the Bash tool's input (command + args).
- The adapter checks `!!` prefix, `LACON_DISABLE`, and the rule resolver. If a rule matches, it applies the rule's `rewrite` block to the inner argv and wraps the result as `lacon run --rule <id> -- <inner-cmd>`.
- The adapter returns `hookSpecificOutput.updatedInput` with the rewritten command. (`updatedInput` replaces the entire input object, so unchanged fields — `description`, `timeout`, `run_in_background` — must be echoed back.)
- No `PostToolUse` hook is installed in v1. (Reserved for v1.5 — see [ADR 0013](decisions/0013-filter-via-pretooluse-wrapper.md).)

Adapters are otherwise dumb: they don't know about pipeline primitives or storage. They translate the hook contract; `lacon run` does the work.

### Wrapper (`lacon run`)

`lacon run --rule <id> -- <cmd> [args...]` spawns the subprocess, reads its merged stdout+stderr line-by-line, runs the pipeline (or the rule's `on_error` pipeline when the subprocess exits non-zero), writes filtered bytes to its own stdout, writes the tracking row to SQLite, and exits with the subprocess's exit code. `lacon run` without `--rule` runs the resolver inline — the same code path the hook uses, useful for manual testing.

## Lifecycle of an invocation

1. Claude Code is about to run `pnpm install --frozen-lockfile`.
2. `PreToolUse` hook fires. The Claude Code adapter:
   - Checks for `!!` prefix → not present, continue.
   - Checks `LACON_DISABLE` env var → not set.
   - Asks the rule resolver: which rule matches? → `pkg-install`.
   - Applies the rule's `rewrite` block → inner argv becomes `pnpm install --frozen-lockfile --reporter=silent`.
   - Wraps as `lacon run --rule pkg-install -- pnpm install --frozen-lockfile --reporter=silent`.
   - Returns `hookSpecificOutput.updatedInput` with the rewritten command.
3. Claude Code executes the rewritten command.
4. `lacon run` spawns `pnpm install ...` as a subprocess, with stderr redirected into stdout.
5. The pipeline runner streams the merged output through the rule's stages — `strip_ansi`, `drop_regex`, `dedupe`, etc. — into a bounded buffer.
6. The subprocess exits. `lacon run` reads the exit code:
   - On zero: flushes the success pipeline's output, optionally runs `post_process` Starlark, applies `max_bytes` cap, writes filtered bytes to stdout.
   - On non-zero: discards the success buffer, runs the buffered raw output through the `on_error` pipeline, then flushes that.
7. `lacon run` writes a row to `invocations` (and, if raw-output retention is enabled, to `raw_outputs`), then exits with the subprocess's exit code.
8. Claude Code captures `lacon run`'s stdout as the tool result and its exit code for the `PostToolUseFailure` event (if non-zero). The model sees only the filtered output.

## Configuration loading

Three layers, project highest priority:

| Layer | Location | Use |
|-------|----------|-----|
| Bundled | embedded in binary | shipped defaults |
| User | `~/.config/lacon/` | personal rules and overrides |
| Project | `<cwd>/.lacon/` | repo-specific rules |

Each layer can contain:

- `config.yaml` — engine and tracking settings (retention windows, default `max_bytes`, `store_raw_outputs` opt-in). Full schema in [config-schema](specs/config-schema.md).
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
    ├── fixtures/                  # captured rule fixtures, one tree per rule + scenario (see docs/testing-rules.md)
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

## Cold-start measurements (Phase 1)

Per [REQ-acceptance-cold-start-budget](../.planning/REQUIREMENTS.md) (Phase 6 ship gate), the cold-start binary invocation must be under 10ms on the hook hot path. Phase 1 records baseline measurements so Phase 6's acceptance test has a regression target.

**Measured:** 2026-05-06 on Linux 6.8.0-111-generic (AMD Ryzen 7 5800X 8-Core Processor). Sample size 50 per scenario, after 3-run warm-up. Release build with `opt-level = "z"` + `lto = "thin"` + `strip = "symbols"`.

| Command | min | median | p95 | max |
|---------|-----|--------|-----|-----|
| `lacon --version` | 982 µs | 1154 µs | 1301 µs | 1323 µs |
| `lacon validate <rule>` | 1082 µs | 1259 µs | 1401 µs | 1635 µs |

Both scenarios are comfortably under the 10ms Phase 6 budget. The dominant cost at these figures is likely process startup + dynamic linking; the clap parse and loader code paths add only ~100 µs on top of `--version`. To regenerate: `cargo build --release && cargo run --release --bin cold_start_probe`.

### Decisions from CONTEXT.md benchmark items

| Item | Measured | Decision | Reference |
|------|----------|----------|-----------|
| 1. Starlark cold-start | Dev-mode: 6 integration tests (~20ms total test-runner overhead); per-test parse+run estimated well under 1ms in debug. Release-mode estimate: negligible relative to 10ms budget. | Eager-init (parse AstModule at rule load time, store on ResolvedRule) is correct. Lazy-init not needed. | PLAN-04 |
| 2. clap v4 vs pico-args | `lacon --version` median: 1154 µs; `lacon validate <rule>` median: 1259 µs | Keep clap derive. Plan-B (pico-args) not triggered. Full cold-start chain well under 10ms budget. | PLAN-06, this section |
| 3. os_pipe + threads vs duct vs raw nix | os_pipe + 1 reader thread + crossbeam-channel adopted; alternatives not benchmarked because the chosen approach met the streaming and cold-start budgets on first implementation. | Keep os_pipe + crossbeam; revisit only if Phase 6 acceptance gate fails. | PLAN-05 |
| 4. POSIX signal-forwarding macOS vs Linux | Tested on Linux 6.8.0-111-generic only in Phase 1; macOS verification deferred. `nix::sys::signal::kill` is portable; the API is identical on macOS. | Cross-platform sign-off deferred to Phase 6 acceptance gate. See Signal forwarding (D-12) below. | PLAN-05 |

### Stream merge guarantee (D-11)

stdout/stderr merge in `lacon run` is **best-effort line atomicity, no cross-stream order guarantee**. Each individual line from stderr or stdout is emitted whole (via `read_until(b'\n', &mut buf)` on a single os_pipe read-end); stderr-line vs stdout-line interleaving is wall-clock-arrival order from the reader thread's perspective. The OS pipe buffer is a single FIFO; whichever stream's `write(2)` lands first wins. This matches CON-nfr-stderr-merge.

Resolves: `Q-deferred-merge-ordering` from `docs/open-questions.md`. See `crates/lacon-core/src/runtime/mod.rs` for the runtime implementation (PLAN-05).

### Signal forwarding (D-12)

SIGTERM and SIGINT received by `lacon run` are forwarded to the subprocess PID via `nix::sys::signal::kill(Pid::from_raw(child_pid), signal)`. The wrapper does **not** drain or flush remaining buffered output after forwarding; it exits with `128 + sig` after the subprocess terminates. Process-group kill (negative PID) is **not** v1 — children of the subprocess are not killed. This is documented as a known v1 limitation; granular process-group behavior is a v2 backlog item.

Resolves: `Q-deferred-signal-forwarding` from `docs/open-questions.md`. See `crates/lacon-core/src/runtime/mod.rs` for the signal-hook + nix wiring (PLAN-05 Task 2).
