---
schema-version: 1
---

# Vision

## Thesis

AI coding assistants (Claude Code, Cursor, aider, and similar) execute bash commands on the user's behalf and feed the output back into the model's context window. A meaningful fraction of every session's token spend goes to output the model didn't need — installer progress lines and deprecation warnings, verbose build traces when nothing failed, every passing test, hundreds of `git status` lines in a busy monorepo. That output rarely affects the model's next action; when it does (errors, explicit summaries) only a small fraction of the bytes matters, and the rest is paid-for noise.

`lacon` is a small Rust CLI that integrates with coding assistants through their hook systems and applies configurable rules to bash output before the assistant sees it: **filter** noisy lines (regex drop, ANSI strip, repeat collapse, tail-only), **rewrite** commands to add quiet flags so the noise is never produced in the first place, **bypass** filtering when the agent needs raw output (`!!`), and **preserve more on error** so failures aren't silently truncated. A small SQLite tracking layer records every invocation so users can see where their tokens go and which rules pull their weight.

It is for developers using AI coding assistants on real projects — long sessions where token budget matters, monorepos with verbose tooling (cargo, pnpm, jest), and teams sharing per-project filter rules. Success is a 30–70% reduction in bash-output bytes on common commands without measurable loss in assistant quality, negligible runtime overhead (<10 ms per command on the hook hot path), project rules added in a single YAML file with no code changes, and trust: the user can always see what was filtered (`lacon explain <id>`) and bypass when needed.

## Invariants

Non-goals:
- **Not an LLM.** No model calls, no embeddings — pure deterministic filtering.
- **Not a shell.** Doesn't replace bash or intercept interactive sessions.
- **Not a remote service.** All processing and storage is local; no telemetry.
- **Not a general-purpose log filter.** Optimized for command output in coding-assistant contexts; useful elsewhere but not designed for it.

Architectural commitments:
- Local-only by default.
- Streaming over buffered (memory bounded for long builds).
- Fast startup (<10 ms cold), since the binary is invoked on every matched-command hot path.
- Cross-assistant from day two: Claude Code is the first adapter, but the core engine is assistant-agnostic.

## Open Questions

The vision deliberately leaves room for directions beyond v1: additional assistant adapters beyond Claude Code (the engine is already assistant-agnostic), and token-based accounting rather than byte-based (deferred pending a tokenizer choice). These are tracked in the deferral ledger and roadmap rather than committed here.
