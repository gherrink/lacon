# 0001: Use Claude Code hooks for integration

**Status:** Accepted

## Context

`lacon` needs to intercept bash commands run by Claude Code. Three integration mechanisms are available:

1. Claude Code's `PreToolUse` / `PostToolUse` hooks for the Bash tool
2. PATH-wrapping shims (e.g. shadow `pnpm`, `cargo`, etc. in PATH)
3. Shell function injection in the user's `.bashrc` / `.zshrc`

## Decision

Use Claude Code's `PreToolUse` and `PostToolUse` hooks as the primary integration mechanism for v1.

## Consequences

- Clean opt-in: a hook config file enables `lacon`; deleting it disables it
- No PATH manipulation, no shell config mutation, no leakage into interactive sessions
- Hooks can be enabled/disabled per project trivially (`.claude/settings.json`)
- The integration is bound to Claude Code's hook contract — if the contract changes, we update one adapter
- Doesn't intercept commands that bypass the Bash tool (e.g. SDK or skill-spawned subprocesses). Documented as a known limitation.

## Alternatives considered

**PATH-wrapping shims.** Works for any assistant (and any shell) without per-assistant code. But invasive: leaks into interactive shells, fragile when users have their own tooling, and requires careful handling of every binary's argv passthrough. Rejected as too high-overhead for v1.

**Shell function injection.** Even more fragile than PATH shims; varies by shell; user `.rc` files are the worst place to put new dependencies.
