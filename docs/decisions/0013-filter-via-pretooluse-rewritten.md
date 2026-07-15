---
status: accepted
date: 2026-05-05
schema-version: 2
---

# 0013: Filter via PreToolUse-rewritten subprocess wrapper

## Context

ADR 0001 chose Claude Code's `PreToolUse` / `PostToolUse` hooks. The architecture documentation and v1 scope both assumed `PostToolUse` could replace the bytes the model receives as the tool result — via a `hookSpecificOutput.updatedToolOutput` field earlier research suggested existed. An empirical probe against live Claude Code on 2026-05-05 (see the open-questions log → "Claude Code hook mechanics") confirmed there is no such field: a `PostToolUse` hook returning `updatedToolOutput` has no effect; the model receives the raw stdout. Only `additionalContext` reaches the model, appended *after* the raw output as a `<system-reminder>`, not as a replacement. This invalidates the central design assumption of post-execution byte reduction via `PostToolUse`.

## Options

Four alternatives were evaluated (see open-questions.md):
- **Rewrite-to-wrapper (chosen).** A `PreToolUse` hook rewrites matched commands into a `lacon run` wrapper that filters before the tool result is captured.
- **`additionalContext`-only annotation.** Annotate rather than replace output — but the raw bytes still reach the model, so no byte reduction. Rejected.
- **Flags-only.** Rewrite commands to add quiet flags but do no filtering — insufficient; most noise has no flag. Rejected.
- **Wait for a future `updatedToolOutput`.** Depend on a hook field that does not exist. Rejected as speculative.

## Decision

Filtering happens through a subprocess wrapper invoked by a `PreToolUse` hook that rewrites matched commands.

1. **`lacon run --rule <id> -- <cmd> [args...]`** is the wrapper: it spawns the command, reads merged stdout+stderr line-by-line, runs the rule's pipeline (or `on_error` pipeline on non-zero exit), writes filtered bytes to its own stdout, writes the tracking row, and exits with the subprocess's exit code.
2. **The adapter installs only a `PreToolUse` hook for the Bash tool.** On a matching command it returns `hookSpecificOutput.updatedInput` with the command rewritten to `lacon run --rule <id> -- <rewritten-cmd>`; unmatched commands pass through unchanged.
3. **stderr merges into stdout** inside `lacon run`; the pipeline operates on a single combined stream.
4. **Exit codes propagate naturally** — `lacon run` exits with the subprocess's code, so `PostToolUseFailure` still fires on failure.
5. **`on_error` is implemented inside `lacon run`**, switching pipelines on the observed exit code; ADR 0010 semantics are unchanged, only the implementation location moves.
6. **`PostToolUse` is not installed in v1.** Reserved for v1.5 (fork 5 in open-questions.md): attaching `additionalContext` annotations to *unmatched* commands for the unmatched-offenders feedback loop.

## Consequences

- The streaming pipeline, rule schema, primitives, Starlark stage, and bundled-rules roadmap survive unchanged — they execute inside `lacon run` instead of a hook responder. ADRs 0003, 0004, 0005, 0007, 0008, 0010, 0011, 0012 are unaffected by execution location.
- `lacon run` becomes a production code path, not just a manual testing entry. The cold-start budget (≤ 10 ms, ADR 0001) tightens correspondingly: the binary now runs on every matched command's hot path.
- Chained commands (top-level `&&`, `||`, `;`) are split before wrapping; each matched segment is wrapped independently with its own `lacon run --rule <id> --` prefix, and unmatched segments pass through untouched.
