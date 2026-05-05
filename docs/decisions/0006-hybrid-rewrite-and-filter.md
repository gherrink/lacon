# 0006: Hybrid command rewriting and output filtering

**Status:** Accepted

## Context

Many noisy commands have flags that suppress the noise at the source: `pnpm install --reporter=silent`, `cargo build --quiet`, `pytest -q`, `docker build --progress=plain`. Rewriting the command to add such a flag before execution is strictly cheaper than filtering the noise after it's generated — you never produce the bytes in the first place.

But not every kind of noise has a flag, and some flags suppress useful signal alongside the noise. We need both mechanisms.

## Decision

Rules can specify both a `rewrite` step (modify the command pre-execution) and a `pipeline` (filter the resulting output post-execution). Both are first-class. Rule authors choose: rewrite for known flags, filter for everything else, both for hybrid cases.

## Consequences

- Bundled rules can choose the cheapest mechanism that works for each command
- Pre-execution rewriting requires hook support; the `PreToolUse` hook in Claude Code provides it. Adapters that can't rewrite gracefully degrade to filter-only and log a warning at install time.
- Tracking distinguishes "saved via rewrite" (`rewritten = 1`) from "saved via filter" so users can see which mechanism is doing the work
- Slight risk: a user-written rewrite step that adds a too-aggressive flag could suppress important signal. Mitigated by `on_error` swapping to a more verbose pipeline on failure.

## Alternatives considered

**Filter-only.** Simpler — just one mechanism, no per-rule decisions. Rejected because it leaves the easy wins on the table; a single `--reporter=silent` flag often beats any amount of regex filtering for the same command.

**Rewrite-only.** Insufficient. Most tools don't have a flag for every kind of noise (e.g. there's no `--quiet` for `git status` in a monorepo with thousands of untracked files). Filtering remains necessary as the universal fallback.
