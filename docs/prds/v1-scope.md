---
schema-version: 1
---

# v1 scope

## Vision

The first release is the smallest thing that is useful end-to-end for one user, on one assistant (Claude Code), on one OS family (macOS + Linux). Anything not in scope defers to the deferral ledger.

## Requirements

### Streaming engine  {#streaming-engine}

Streaming output processor with the native primitives (`strip_ansi`, `drop_regex`, `keep_regex`, `replace_regex`, `dedupe`, `collapse_repeated`, `keep_head`, `keep_tail`, `keep_around_match`, `max_bytes`), a Starlark `post_process` escape hatch (aggregated, not per-line, cold-start cost paid per invocation), rule loading from bundled / user / project layers with project > user > bundled first-match-wins resolution, append-only `extends` inheritance, an `on_error` block that fully replaces the pipeline on non-zero exit, pre-execution command rewriting (`add_flags`/`remove_flags`/`replace_flags`), bypass via `!!` prefix and `LACON_DISABLE=1`, and a hard `max_bytes` final-stage cap.

### Claude Code adapter  {#claude-code-adapter}

Installs only a `PreToolUse` hook for the Bash tool. The hook resolves the rule, applies its `rewrite` block to the inner argv, and rewrites a matched command to `lacon run --rule <id> -- <inner-cmd>` via `hookSpecificOutput.updatedInput`; detects `!!` and `LACON_DISABLE=1` and bypasses by returning the original command; splits chains at top-level `&&`/`||`/`;` and wraps each matched segment independently (unmatched pass through); bypasses the whole chain if any segment matches the TUI heuristic; wraps a pipe/subshell argv as a unit. No `PostToolUse` hook in v1.

### Tracking  {#tracking}

SQLite at `~/.local/share/lacon/history.db` per the tracking-data-model spec, with `raw_outputs` storage off by default (opt-in per project), a one-time stderr privacy notice on the off→on transition, no automatic redaction, and default retention of 30 days for invocations / 3 days for raw outputs.

### CLI commands  {#cli-commands}

Six commands: `lacon init` (set up `.lacon/`, configure the `PreToolUse` hook, add the CLAUDE.md instruction line), `lacon run [--rule <id>] -- <cmd>` (production wrapper; without `--rule` runs the resolver inline for manual testing), `lacon stats` (`--project`/`--since`/`--rule` filters), `lacon explain <id>` (side-by-side re-filter of stored raw output), `lacon doctor` (verify hooks/config/rules), and `lacon validate <path>` (lint a rule or `config.yaml`, file type detected by content).

### Bundled rule library  {#bundled-rule-library}

Ten Tier-1 rules covering the highest-impact commands: `pkg-install`, `cargo-build`, `cargo-test`, `vitest`, `jest`, `pytest`, `tsc`, `eslint`, `git-status`, `docker-build`.

### Documentation  {#documentation}

A README with install + quickstart, a worked example of writing a project-specific filter rule, and a reference for every primitive.

### Acceptance criteria  {#acceptance-criteria}

v1 ships when: all ten bundled rules reduce their target commands by at least 50% on representative output without dropping errors; `lacon init` followed by a `pnpm install` in a new project works end-to-end with no manual config; cold-start binary invocation is under 10 ms on the hook hot path; `lacon explain` reproduces the filtering decision for any tracked invocation; edited rule files take effect on the next invocation (no daemon, no restart); and the test suite covers each native primitive, the chained-command splitter, and every bundled rule via fixture-based integration tests.

## Context

Coverage boundary — what `lacon` can and can't see. **Invisible (fundamental):** subprocesses spawned by non-Bash tools or MCP servers, since only `PreToolUse(Bash)` fires. **Invisible by design:** output redirected to files/sockets that don't reach the Bash tool's stdout/stderr (the model only sees what a later `cat` surfaces, filtered normally), backgrounded processes whose continuing output isn't in the tool result, and direct `/dev/tty` writes (rare, since the Bash tool allocates a pipe). **Intentionally out of scope:** the user's own terminal sessions outside Claude Code. ANSI/cursor-control codes that flow through stdout/stderr are filterable (`strip_ansi`), not a coverage gap.

Explicitly out of scope for v1 (see the deferral ledger for the full list): adapters for Cursor / aider / any non-Claude-Code assistant, per-token (vs per-byte) accounting, per-line streaming Starlark, filtering inside pipes, native Windows support (WSL is fine), and sharing rules across machines / a public rule registry.
