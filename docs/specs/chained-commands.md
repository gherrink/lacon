---
schema-version: 1
---

# Chained commands

## Goal

Define how `lacon` splits and handles bash command chains formed with `&&`, `||`, and `;`. This behavior is part of the user-facing contract — changes here are breaking.

## Context

This spec assumes the ADR 0013 execution model: the Claude Code `PreToolUse` hook rewrites matched commands into `lacon run --rule <id> -- <segment>` before the shell sees them. All chain handling happens at rewrite time in the hook; `lacon run` itself only ever wraps a single command.

## Criteria

### Split at top-level operators only  {#split-at-top-level-operators}

Chains split at top-level `&&` (run next on success), `||` (run next on failure), and `;` (run next regardless). Top-level means not inside quotes, `(...)` subshells, `$(...)` or backtick command substitution, `${...}` parameter expansion, or heredoc bodies.

### Pipes are not chain operators  {#pipes-are-not-chain-operators}

`|` is not a chain operator; a pipeline is a single segment (`pnpm test | grep foo` is wrapped or bypassed as one unit). Filtering inside pipes is out of scope for v1.

### Nested constructs stay with their segment  {#nested-constructs-stay-with-their}

Subshells `(cmd1 && cmd2)`, command substitution `$(...)`/backticks, process substitution `<(...)`/`>(...)`, heredoc bodies, and quoted strings containing chain operators are not split — they are treated as part of the segment that contains them. Per-segment filtering of a sub-chain requires the user to refactor it into a top-level chain.

### Per-segment independent rule resolution  {#per-segment-independent-rule-resolution}

Each segment is resolved against the rule registry independently: first-match-wins with project > user > bundled precedence (ADR 0004, ADR 0007), no merging and no cross-segment effects. A matched segment is wrapped as `lacon run --rule <id> -- <seg>`; an unmatched segment is passed through unchanged.

### Rewrite reassembly preserves operators  {#rewrite-reassembly-preserves-operators}

The hook reassembles the chain by joining the (wrapped or passed-through) segments with the original operators, preserving order and operator type. e.g. `pnpm install && pnpm test || echo failed` becomes `lacon run --rule pkg-install -- pnpm install && lacon run --rule vitest -- pnpm test || echo failed` when the first two match and the third does not.

### Exit codes and shell semantics are unchanged  {#exit-codes-and-shell-semantics}

`lacon run` propagates its wrapped subprocess's exit code unchanged, so the shell's `&&`/`||`/`;` semantics behave exactly as if `lacon run` weren't present. Filtering one segment cannot change whether or how the next segment runs — filtering changes only what the model sees, not what the shell sees.

### Bypass is whole-command, not per-segment  {#bypass-is-whole-command-not}

The `!!` prefix and `LACON_DISABLE=1` env var bypass at whole-command granularity: the entire rewrite is skipped and the original command returned unchanged. Bypass is not segment-aware.

### TUI detection bypasses the whole chain, before resolution  {#tui-detection-bypasses-the-whole}

After chain splitting, the adapter calls a heuristic `is_tui(command, args) -> bool` on every segment before rule resolution. If any segment is interactive (`vim`, `less`, `htop`, `git rebase -i`, `mysql`/`psql`/`sqlite3` with no positional arg, etc.), the entire input is bypassed — original command runs unchanged, no wrapping, no resolution. A solo command is a 1-segment chain. Firing before resolution is required because most TUI tools have no rule; whole-chain bypass (rather than per-segment) is chosen because a wrapped TUI segment misroutes stdin/stderr/PTY, and it is a strict subset of any future granular behavior.

### The TUI list is hardcoded in v1  {#tui-list-is-hardcoded}

The interactive-command list lives in adapter code, not user config; adding or removing entries requires a `lacon` release. Users hitting a false positive use the escape hatches (`!!` or `LACON_DISABLE=1`). A user-overridable list is backlogged.

### What reaches the model  {#what-reaches-the-model}

Each wrapped segment writes its filtered output to its own stdout; the shell concatenates segment outputs in order and Claude Code captures the combined stdout as the tool result. Operators themselves produce no output; only segments do.

### Splitter test obligations  {#splitter-test-obligations}

The chained-command splitter must have tests covering at minimum: single command (no chain); two-segment chain for each operator; mixed-operator chain (`a && b || c ; d`); each segment matching a different rule; one unmatched segment; one interactive segment (whole-chain bypass); chain inside a subshell / command substitution / quoted string (each a single segment); pipeline as a segment (`a | b && c` → `[a | b, c]`); heredoc body with chain operators (opaque); whole-chain bypass via `!!` and via `LACON_DISABLE=1`.
