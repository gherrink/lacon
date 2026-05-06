# v1 scope

The first release is the smallest thing that's useful end-to-end for one user on one assistant (Claude Code) on one OS family (macOS + Linux). Anything not on this list defers to the [backlog](backlog.md).

## In scope

### Engine

- Streaming output processor with the native primitives listed in [filter-rule-schema](specs/filter-rule-schema.md): `strip_ansi`, `drop_regex`, `keep_regex`, `replace_regex`, `dedupe`, `collapse_repeated`, `keep_head`, `keep_tail`, `keep_around_match`, `max_bytes`
- Starlark escape hatch via `post_process` step (aggregated, not per-line)
- Starlark cold-start cost is paid per invocation (no shared process, no IPC); accepted as a per-rule tax for rules that opt into `post_process`. Persistent-interpreter optimization is a v2 [backlog](backlog.md) item gated on benchmark data.
- Rule loading from `bundled/`, `~/.config/lacon/rules/`, `<project>/.lacon/rules/`
- Project > user > bundled precedence, first-match-wins resolution
- `extends` inheritance (append-only)
- `on_error` block that fully replaces the pipeline on non-zero exit
- Pre-execution command rewriting (`rewrite.add_flags`, `remove_flags`, `replace_flags`)
- Bypass via `!!` prefix and `LACON_DISABLE=1` env var
- Hard `max_bytes` cap as final-stage safety net

### Claude Code adapter

Per [ADR 0013](decisions/0013-filter-via-pretooluse-wrapper.md):

- Installs only a `PreToolUse` hook for the Bash tool
- Hook resolves the rule, applies the rule's `rewrite` block to the inner argv, and (if matched) rewrites the command to `lacon run --rule <id> -- <inner-cmd>` via `hookSpecificOutput.updatedInput`
- Detects `!!` prefix and `LACON_DISABLE=1` env var; bypasses by returning the original command unchanged
- Splits chained commands at top-level `&&`, `||`, `;` and wraps each matched segment independently; unmatched segments pass through. Full semantics (splitting boundary, exit-code propagation, TUI handling) in [chained-commands](specs/chained-commands.md)
- If any segment in a chain matches the TUI heuristic, the **whole chain** is bypassed (v1 conservative rule; granular per-segment bypass is a v2 candidate)
- Pipes (`|`) and subshells: the matched argv is wrapped as a unit (the user's pipe is preserved inside `--`)
- No `PostToolUse` hook is installed; reserved for v1.5 unmatched-command annotation

### Tracking

- SQLite at `~/.local/share/lacon/history.db`
- Schema as in [tracking-data-model](specs/tracking-data-model.md)
- `raw_outputs` storage **off** by default (opt-in per project)
- First-time enablement of `raw_outputs` (off → on transition) prints a one-time stderr privacy notice; no automatic redaction in v1 (see [tracking-data-model → Privacy](specs/tracking-data-model.md#privacy))
- Default retention: 30 days for invocations, 3 days for raw outputs

### CLI commands

- `lacon init` — sets up `.lacon/` in the current project, configures the Claude Code `PreToolUse` hook, adds the tiny CLAUDE.md instruction line
- `lacon run [--rule <id>] -- <cmd> [args...]` — production wrapper invoked by the `PreToolUse` rewrite. Spawns the subprocess, filters its merged stdout+stderr, propagates the subprocess's exit code. Without `--rule`, runs the resolver inline against `<cmd>` — useful for manual testing
- `lacon stats` — show top offenders, bypass rates, unmatched commands; supports `--project`, `--since`, `--rule` filters
- `lacon explain <id>` — re-run filtering against stored raw output, show side-by-side diff
- `lacon doctor` — verify hooks are installed, config files are valid, rules parse
- `lacon validate <rule.yaml>` — lint a rule file without running it

### Bundled rule library

Ten rules covering the highest-impact commands. See Tier 1 in [bundled-rules-roadmap](bundled-rules-roadmap.md):

`pkg-install`, `cargo-build`, `cargo-test`, `vitest`, `jest`, `pytest`, `tsc`, `eslint`, `git-status`, `docker-build`.

### Documentation

- README with install + quickstart
- A worked example: writing a project-specific filter rule
- Reference for every primitive

## Acceptance criteria

v1 ships when:

- [ ] All ten bundled rules reduce their target commands by at least 50% on representative output without dropping errors
- [ ] `lacon init` followed by a `pnpm install` in any new project works end-to-end with no manual config
- [ ] Cold-start binary invocation is under 10ms (measured on the hook hot path)
- [ ] `lacon explain` correctly reproduces the filtering decision for any tracked invocation
- [ ] Rule files can be edited and changes take effect on the next invocation (no daemon, no restart)
- [ ] Test suite covers each native primitive, the chained-command splitter, and every bundled rule via fixture-based integration tests (see [testing-rules](testing-rules.md))

## Explicitly out of scope for v1

See [backlog](backlog.md) for the full list. The most likely-to-be-asked-about exclusions:

- Adapters for Cursor, aider, or any non-Claude-Code assistant
- Per-token (vs per-byte) accounting
- Per-line streaming Starlark
- Filtering inside pipes
- Native Windows support (WSL is fine; native is deferred)
- Sharing rules across machines / a public rule registry

## Coverage boundary

What `lacon` can and can't see, so users know what to expect.

**Invisible to lacon (fundamental):**

- Subprocesses spawned by non-Bash tools or MCP servers — only `PreToolUse(Bash)` fires. The Read, Edit, Grep, Task, and WebFetch tools, and any subprocess launched by an MCP server, do not pass through `lacon run`.

**Invisible by design:**

- Output redirected to files, sockets, or other sinks that don't reach the Bash tool's stdout/stderr (e.g. `pnpm dev > log.txt &`). The model only sees what later commands like `cat log.txt` surface — those are filtered normally.
- Backgrounded long-running processes whose continuing output isn't captured in the tool result. The foreground chunk that *is* captured is filtered like any other command.
- Direct `/dev/tty` writes. Rare in practice because Claude Code's Bash tool allocates a pipe, so most tools fall back to non-interactive mode and don't reach for `/dev/tty`.

**Intentionally out of scope:**

- The user's own terminal sessions outside Claude Code.

ANSI escape sequences and cursor-control codes that *do* flow through stdout/stderr are filterable — that's what `strip_ansi` is for, not a coverage gap.
