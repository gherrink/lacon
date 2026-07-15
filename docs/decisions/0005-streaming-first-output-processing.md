---
status: accepted
schema-version: 2
---

# 0005: Streaming-first output processing

## Context

Some commands produce large output: multi-minute cold-cache `cargo build`, `pnpm install` in large monorepos, `docker build` with verbose layers — each can emit many megabytes. Buffering all output in memory before filtering is wasteful and breaks any meaningful memory bound on long-running commands.

## Options

- **Streaming line-by-line (chosen).** Native primitives are lazy transformers processed as output arrives; memory is bounded regardless of total output size.
- **Buffer-then-filter.** Read all output into memory, then run primitives — simpler, but puts no upper bound on memory for exactly the long-running case where filtering matters most. Rejected.
- **Hybrid (buffer up to N MB, then stream).** Adds complexity without meaningful benefit; if streaming works for the large-output case, it works for everything. Rejected.

## Decision

Native pipeline primitives are implemented as streaming line-by-line transformers. Each primitive takes a line and may yield zero, one, or many lines downstream; the pipeline is a chain of these transformers, processed lazily as output arrives. The Starlark `post_process` stage is an explicit exception — it operates on the aggregated post-pipeline output, not per-line (see ADR 0008).

## Consequences

- Memory usage is bounded by the largest stateful primitive (typically `keep_tail N`) plus the final `max_bytes` cap — no relationship to total command output size.
- Long builds don't OOM the hook process or the parent assistant.
- Implementation is more complex than buffer-then-filter, especially for stateful primitives like `dedupe` (windowed) and `keep_tail` (ring buffer).
- Primitives that would require global reordering (e.g. "sort all lines") cannot fit a streaming model and are out of scope.
