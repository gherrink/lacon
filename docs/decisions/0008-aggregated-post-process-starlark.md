---
status: accepted
schema-version: 2
---

# 0008: Aggregated post-process Starlark, not per-line

## Context

The Starlark escape hatch could integrate with the streaming pipeline in two ways: per-line (called once for every output line, deciding keep/drop/transform) or aggregated (called once on the post-pipeline output as a list of strings). Per-line is more powerful — Starlark inspects each line in context. Aggregated is much faster — startup and per-call overhead is paid once.

## Options

- **Aggregated (chosen).** Native pipeline does bulk reduction first; Starlark runs once on the small remaining payload.
- **Per-line Starlark by default.** Maximum flexibility and the simplest mental model, but a 10–100× slowdown vs native primitives (a 5 MB `cargo build` could mean 100k+ Starlark calls), and it invites putting logic in scripting that belongs in primitives. Rejected on performance.
- **No Starlark at all.** The 90% case is fine with native primitives, but the 10% complex cases become impossible without copy-paste regex hacks. Too much lost expressiveness. Rejected.
- **Both, via an explicit `streaming: true` flag.** A possible future addition (backlogged), not v1.

## Decision

Starlark stages run on aggregated output, not per-line. The native pipeline does bulk reduction first; Starlark gets the small remaining payload.

## Consequences

- Per-line Starlark would dominate runtime at typical output volumes (a 5 MB `cargo build` could mean 100k+ Starlark calls).
- The 90% case (regex drops, ANSI strip, collapse, tail) stays fully native and fast.
- The 10% case (structural decisions like "find the success-summary line") gets all the expressive power it needs, since by the time Starlark runs the data is small.
- We lose streaming Starlark filters. Almost no real rules need this; it is backlogged if a use case appears.
