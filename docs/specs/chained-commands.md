# Chained commands

How `lacon` handles bash command chains formed with `&&`, `||`, and `;`. Behavior here is part of the user-facing contract — changes are breaking.

This spec assumes the [ADR 0013](../decisions/0013-filter-via-pretooluse-wrapper.md) execution model: the Claude Code `PreToolUse` hook rewrites matched commands into `lacon run --rule <id> -- <segment>` before the shell sees them. All chain handling happens at rewrite time in the hook; `lacon run` itself only ever wraps a single command.

## Splitting

Chains are split at **top-level** occurrences of:

- `&&` — run next on success
- `||` — run next on failure
- `;` — run next regardless

Top-level means: not inside quotes, not inside `(...)` subshells, not inside `$(...)` or `` `...` `` command substitution, not inside `${...}` parameter expansion, not inside heredoc bodies.

Pipes (`|`) are **not** chain operators. A pipeline is a single segment. `pnpm test | grep foo` is wrapped or bypassed as one unit; filtering inside pipes is explicitly out of scope for v1 (see [backlog](../backlog.md)).

Constructs not split — treated as part of whatever segment contains them:

- Subshells: `(cmd1 && cmd2)`
- Command substitution: `$(cmd1 && cmd2)`, `` `cmd1 && cmd2` ``
- Process substitution: `<(...)`, `>(...)`
- Heredocs: the text between `<<EOF` and `EOF`
- Quoted strings containing chain operators

If a sub-chain genuinely needs per-segment filtering, the user must refactor it into a top-level chain. Parsing nested constructs is on the [backlog](../backlog.md) under "Heredoc / subshell / eval handling".

## Rule resolution per segment

Each segment is resolved against the rule registry **independently**. First-match-wins applies per segment, with the usual project > user > bundled precedence ([ADR 0004](../decisions/0004-config-precedence.md), [ADR 0007](../decisions/0007-first-match-wins.md)). No merging across segments. No cross-segment rule effects.

A segment resolves to one of two outcomes:

| Outcome   | Meaning                            | Rewrite                                      |
| --------- | ---------------------------------- | -------------------------------------------- |
| Matched   | A rule's `match` predicate fires   | Wrapped as `lacon run --rule <id> -- <seg>` |
| Unmatched | No rule matches                    | Passed through unchanged                     |

User-driven bypass (`!!` prefix, `LACON_DISABLE=1`) is whole-command, not per segment — see [Bypass](#bypass) below.

## Rewrite emission

The hook reassembles the chain by joining segments with the original operators, preserving order and operator type.

Input:

```
pnpm install && pnpm test || echo failed
```

If `pnpm install` and `pnpm test` match different rules and `echo failed` matches none, the rewritten command becomes:

```
lacon run --rule pkg-install -- pnpm install && lacon run --rule vitest -- pnpm test || echo failed
```

## Exit codes and shell semantics

`lacon run` propagates its wrapped subprocess's exit code unchanged. The shell's `&&` / `||` / `;` semantics work exactly as if `lacon run` weren't present: the next segment runs (or doesn't) based on the real exit code, and only the *output that reaches Claude Code* is filtered.

A consequence: filtering one segment cannot change whether or how the next segment runs. The "second command depends on first command's output" concern is a non-issue at the chain level — filtering changes what the *model* sees, not what the *shell* sees.

## Bypass

`!!` prefix and `LACON_DISABLE=1` env var bypass at the **whole-command** granularity, not per segment. The whole rewrite is skipped; the original command is returned unchanged.

Rationale: bypass is a user-driven escape hatch ("just run this raw"). Making it segment-aware would surprise users and complicate the hook for no clear gain. If a single segment needs a different rule, edit the rule rather than reaching for `!!`.

## Interactive (TUI) commands in chains — v1

If **any** segment in a chain matches the TUI heuristic (e.g. `git rebase -i`, `vim`, `less`, `htop`, `git commit` without `-m`), the **entire chain** is bypassed. No segments are wrapped.

Reasoning:

- A wrapped TUI segment misroutes stdin/stderr/terminal control and breaks the user experience.
- TUI-in-chain is rare in practice; interactive commands are typically invoked solo.
- A whole-chain bypass is one branch instead of N, and is a strict subset of any future granular behavior — we can tighten in v2 without changing v1 semantics.

Granular per-segment bypass (wrap non-TUI segments, pass the TUI segment through) is a v2 candidate, gated on tracking data showing the lost filtering opportunity is material. Listed in [backlog](../backlog.md).

## What reaches the model

Each wrapped segment writes its filtered output to its own stdout. The shell concatenates segment outputs in order. Claude Code captures the combined stdout as the tool result. The model sees:

```
<filtered seg1 output>
<filtered seg2 output>
...
```

Operators (`&&`, `||`, `;`) themselves produce no output; only segments do.

## Test obligations

The chained-command splitter must have tests covering at minimum:

- Single command, no chain
- Two-segment chain with each operator (`&&`, `||`, `;`)
- Mixed-operator chain (`a && b || c ; d`)
- Chain where each segment matches a different rule
- Chain where one segment is unmatched (passes through)
- Chain where one segment is interactive (whole-chain bypass)
- Chain inside a subshell — `(a && b)` is a single segment, not split
- Chain inside command substitution — `echo $(a && b)` is a single segment
- Chain operator inside a quoted string — `echo "a && b"` is a single segment
- Pipeline as a segment — `a | b && c` splits as `[a | b, c]`
- Heredoc body containing chain operators — body is opaque
- Whole-chain bypass via `!!` prefix
- Whole-chain bypass via `LACON_DISABLE=1`
