---
status: accepted
schema-version: 2
---

# 0001: Use Claude Code hooks for integration

## Context

`lacon` must intercept the bash commands Claude Code runs on the user's behalf so it can filter their output before the model ingests it. Three integration mechanisms were available: (1) Claude Code's `PreToolUse` / `PostToolUse` hooks for the Bash tool; (2) PATH-wrapping shims that shadow individual binaries such as `pnpm` or `cargo`; and (3) shell-function injection into the user's `.bashrc` / `.zshrc`. The mechanisms differ sharply in how invasive they are and how far they leak into interactive shells outside the assistant.

## Options

Three integration mechanisms were weighed:

- **Claude Code hooks (chosen).** `PreToolUse` / `PostToolUse` on the Bash tool. Assistant-specific, but a clean per-project opt-in that never touches the shell environment.
- **PATH-wrapping shims.** Shadow each binary (`pnpm`, `cargo`, …) on `PATH`. Assistant-agnostic, but invasive: leaks into interactive shells, fragile against a user's own tooling, and needs careful argv passthrough per binary. Rejected as too high-overhead for v1.
- **Shell-function injection.** Inject functions into the user's `.bashrc` / `.zshrc`. Even more fragile than PATH shims, varies by shell, and puts a new dependency in the worst possible place. Rejected.

## Decision

Use Claude Code's `PreToolUse` / `PostToolUse` hooks for the Bash tool as the primary integration mechanism for v1.

## Consequences

- Clean opt-in: a hook config entry enables `lacon`; removing it disables it, per project, via `.claude/settings.json`.
- No PATH manipulation and no shell-config mutation, so nothing leaks into interactive sessions.
- The integration is bound to Claude Code's hook contract; if that contract changes we update a single adapter.
- Commands that bypass the Bash tool (SDK- or skill-spawned subprocesses) are not intercepted — a documented known limitation.
