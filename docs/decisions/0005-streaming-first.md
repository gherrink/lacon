# 0005: Streaming-first output processing

**Status:** Accepted

## Context

Some commands produce large amounts of output. Multi-minute `cargo build` invocations on cold caches, `pnpm install` in large monorepos, and `docker build` with verbose layers can each produce many megabytes. Buffering all output in memory before filtering is wasteful and breaks any meaningful memory bound on long-running commands.

## Decision

Native pipeline primitives are implemented as streaming line-by-line transformers. Each primitive takes a line and may yield zero, one, or many lines downstream. The pipeline is a chain of these transformers, processed lazily as output arrives.

The Starlark `post_process` stage is an explicit exception — it operates on the aggregated post-pipeline output, not per-line. See ADR 0008 for the reasoning.

## Consequences

- Memory usage is bounded by the largest stateful primitive (typically `keep_tail N`) plus the final `max_bytes` cap — no relationship to total command output size
- Long builds don't OOM the hook process or the parent assistant
- Implementation is more complex than buffer-then-filter, especially for stateful primitives like `dedupe` (windowed) and `keep_tail` (ring buffer)
- Some primitives that would require global reordering (e.g. "sort all lines") cannot fit a streaming model. Such primitives are out of scope.

## Alternatives considered

**Buffer-then-filter.** Read all output into memory, then run primitives. Simpler implementation. Rejected because it puts no upper bound on memory usage for long-running commands, which is the case where filtering matters most.

**Hybrid (buffer up to N MB, then stream).** Adds complexity without meaningful benefit; if we can do streaming for the large-output case we might as well do it for everything.
