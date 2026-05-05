# Backlog

Things deliberately deferred from v1, grouped by category. This is not a promise to build any of them — it's a holding place for ideas worth revisiting once v1 is in real use.

## Adapters

- **Cursor adapter** — when their hook/extension API stabilizes
- **aider adapter** — they're more shell-friendly, possibly easier than Claude Code
- **Generic shell wrapper** — for assistants without hooks; opt-in PATH shim
- **Editor-side adapters** (Continue, etc.)

## Engine features

- **Per-line streaming Starlark** — for filters that genuinely need scripted line decisions, not just post-processing
- **Filter inside pipes** — track `cmd1 | cmd2`, filter the pipeline output as a whole
- **Heredoc / subshell / eval handling** — currently passes through; could be parsed
- **Multi-rule merging** — when more than one rule could apply, merge stages instead of first-match-wins. Probably a bad idea, but worth the option to revisit.
- **Conditional pipeline stages** — `if exit_code == 0: keep_tail 5; else: ...` inline rather than via the `on_error` block
- **Stage-level inheritance operations** — insert/remove/replace specific stages from a parent rule (rather than append-only `extends`)

## Tracking

- **Per-token accounting** — needs a tokenizer choice (tiktoken? Claude's? both?). Useful but not blocking.
- **Session-aware aggregation** — group invocations by Claude Code session so users can see "this session cost X tokens, of which Y were saved"
- **Cost estimation** — multiply tokens by current model pricing for a dollar figure
- **Trend graphs** — token spend over time, per project, per rule

## Sharing & discovery

- **Public rule registry** — `lacon install gh:user/repo` to pull a rule pack
- **Sync between machines** — Git-backed user config, optional encrypted sync
- **Suggestion engine** — based on tracked unmatched commands, suggest "you might want a rule for X"

## UI

- **Web UI for stats** — `lacon stats --serve`
- **TUI dashboard** — live view during a Claude Code session
- **VS Code extension** — surface filter activity in the editor

## Platforms

- **Native Windows** — v1 is macOS + Linux + WSL only
- **Static musl builds** for distroless containers

## Programmatic

- **Library API** — expose the engine as a Rust crate / WASM module so other tools can embed it
- **Plugin protocol** — primitives written in any language, communicating over stdio (slower than Starlark but more flexible)

## Quality-of-life

- **Rule hot-reload notifications** — toast or log line when a rule file is reloaded
- **Filter dry-run mode in CI** — run rules against fixture output, fail if regressions
- **Rule profiler** — measure per-stage time on real output, surface slow stages
- **Redaction patterns** — auto-strip lines matching common secret patterns before storing raw output
