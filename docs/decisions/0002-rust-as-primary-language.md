# 0002: Rust as primary language

**Status:** Accepted

## Context

`lacon` is a CLI tool that processes high-throughput text streams and is invoked thousands of times per coding session via hooks. Two characteristics matter most: regex throughput on the hot path, and cold-start time. The choice of language is largely a choice of which performance ceiling we want.

## Decision

Rust, using the `regex`, `clap`, `rusqlite`, and `starlark-rust` crates as the foundation.

## Consequences

- Best-in-class regex throughput — the `regex` crate is the fastest production RE2-derived engine
- Sub-millisecond cold start with minimal dependencies, important because the binary is invoked on every hook fire
- Mature crates for everything we need (CLI parsing, SQLite, Starlark, YAML)
- Cross-compilation via `cargo zigbuild` or `cross` is solid for macOS + Linux distribution
- Steeper learning curve for contributors whose primary stack is TypeScript/JavaScript
- Compile times are slower than Go, but acceptable for a project of this size

## Alternatives considered

**Go.** Faster to ship, simpler concurrency for managing multiple stdio handles, the official Starlark implementation is more mature. But Go's `regexp` is meaningfully slower than Rust's `regex` (same RE2 lineage, much less optimization work). For a tool whose entire job is running regexes against streamed text, that's a real ceiling. Also: the established pattern for "fast text-processing CLI" (ripgrep, fd, bat, sd, dust, delta) is overwhelmingly Rust, and copying their patterns is easier than reinventing them.

**Node/TypeScript.** Familiar to the author and natural fit for the JS/TS ecosystem we're filtering output from. But Node's startup overhead (50–100ms cold) is incompatible with hook-based invocation thousands of times per session. Bun and Deno would help but add their own deployment friction.

**Zig, Nim, etc.** Too niche to attract contributors and lack the crate ecosystem.
