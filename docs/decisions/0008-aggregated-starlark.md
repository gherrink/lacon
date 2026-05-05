# 0008: Aggregated post-process Starlark, not per-line

**Status:** Accepted

## Context

The Starlark escape hatch could integrate with the streaming pipeline in two ways: per-line (called once for every output line, can decide to keep, drop, or transform) or aggregated (called once on the post-pipeline output as a list of strings).

Per-line is more powerful — Starlark could inspect each line in context. Aggregated is much faster — Starlark startup and per-call overhead happens once.

## Decision

Starlark stages run on aggregated output, not per-line. The native pipeline does bulk reduction first; Starlark gets the small remaining payload.

## Consequences

- Per-line Starlark would dominate runtime at typical output volumes (a 5MB `cargo build` could mean 100k+ Starlark calls)
- The 90% case (regex drops, ANSI strip, collapse, tail) stays fully native and fast
- The 10% case (structural decisions like "find the success summary line") gets all the expressive power it needs, since by the time Starlark runs the data is small
- We lose the ability to write streaming Starlark filters. Almost no real rules need this; it's listed in the backlog if a use case appears.

## Alternatives considered

**Per-line Starlark by default.** Maximum flexibility, simplest mental model (Starlark sees every line). Rejected on performance: 10–100x slowdown vs. native primitives, and the ergonomic invitation to write logic in scripting that should be in primitives is a long-term maintenance hazard.

**No Starlark at all.** 90% of rules are fine with native primitives, but the 10% complex cases become impossible without resorting to copy-paste regex hacks. Loses too much expressiveness for the simplification.

**Both, controlled by an explicit `streaming: true` flag on the script stage.** Possible future addition (filed in the backlog) but not v1.
