---
status: accepted
schema-version: 2
---

# 0006: Hybrid command rewriting and output filtering

## Context

Many noisy commands have flags that suppress the noise at the source: `pnpm install --reporter=silent`, `cargo build --quiet`, `pytest -q`, `docker build --progress=plain`. Rewriting the command to add such a flag before execution is strictly cheaper than filtering the noise afterward — you never produce the bytes. But not every kind of noise has a flag, and some flags suppress useful signal alongside the noise. Both mechanisms are needed.

## Options

- **Hybrid: rewrite + filter (chosen).** Rule authors pick the cheapest mechanism per command — rewrite for known flags, filter for everything else, both for hybrid cases.
- **Filter-only.** Simpler (one mechanism), but leaves the easy wins on the table; a single `--reporter=silent` often beats any amount of regex filtering for the same command. Rejected.
- **Rewrite-only.** Insufficient — most tools lack a flag for every kind of noise (e.g. no `--quiet` for `git status` in a monorepo with thousands of untracked files); filtering remains the universal fallback. Rejected.

## Decision

Rules can specify both a `rewrite` step (modify the command pre-execution) and a `pipeline` (filter the resulting output post-execution). Both are first-class; rule authors choose rewrite, filter, or both per command.

## Consequences

- Bundled rules can choose the cheapest mechanism that works for each command.
- Pre-execution rewriting requires hook support; Claude Code's `PreToolUse` hook provides it. Adapters that can't rewrite degrade gracefully to filter-only and warn at install time.
- Tracking distinguishes "saved via rewrite" (`rewritten = 1`) from "saved via filter", so users can see which mechanism does the work.
- A too-aggressive user `rewrite` flag could suppress important signal; mitigated by `on_error` swapping to a more verbose pipeline on failure.
