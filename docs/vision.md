# Vision

## The problem

AI coding assistants (Claude Code, Cursor, aider, and similar) execute bash commands on the user's behalf and feed the output back into the model's context window. A meaningful fraction of every coding session's token spend goes to bash output the model didn't need:

- Package installers print hundreds of progress lines, deprecation warnings, and peer-dep advice
- Build tools print verbose compilation traces even when nothing failed
- Test runners print every passing test by default
- `git status` in a busy monorepo can produce hundreds of lines

This output rarely affects the model's next action — when it does (errors, explicit summaries), only a small fraction of the bytes matters. The rest is paid-for noise.

## The approach

`lacon` is a small Rust CLI that integrates with coding assistants through their hook systems and applies configurable rules to bash output before the assistant sees it. The rules can:

- **Filter** noisy lines from output (regex drop, ANSI strip, repeat collapse, tail-only, etc.)
- **Rewrite** commands to add quiet flags before they run, so the noise is never produced in the first place
- **Bypass** filtering when the agent needs the raw output (`!!` prefix)
- **Preserve more on error** so failures aren't silently truncated

A small SQLite tracking layer records every invocation so users can see where their tokens go and which rules are pulling weight.

## Who it's for

Developers using AI coding assistants on real projects. Especially:

- Long sessions where token budget matters
- Monorepos with verbose tooling (cargo, pnpm, jest, etc.)
- Teams who want to share filter rules per project

## What success looks like

- 30–70% reduction in bash output bytes on common commands without measurable loss in assistant quality
- Negligible runtime overhead (<10ms per command on the hook hot path)
- Project rules can be added in a single YAML file with no code changes
- Trust: the user can always see what was filtered (`lacon explain <id>`) and bypass when needed

## Non-goals

- **Not an LLM.** No model calls, no embeddings. Pure deterministic filtering.
- **Not a shell.** Doesn't replace bash, doesn't intercept interactive sessions.
- **Not a remote service.** All processing and storage is local. No telemetry.
- **Not a general-purpose log filter.** Optimized for command output in coding-assistant contexts; happens to be useful elsewhere but won't be designed for it.

## Architectural commitments

- Local-only by default
- Streaming over buffered (memory bounded for long builds)
- Fast startup (<10ms cold) since the binary is invoked on every matched-command hot path
- Cross-assistant from day two: Claude Code is the first adapter, but the core engine is assistant-agnostic
