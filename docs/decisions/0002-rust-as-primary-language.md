---
status: accepted
schema-version: 2
---

# 0002: Rust as primary language

## Context

`lacon` is a CLI that processes high-throughput text streams and is invoked thousands of times per coding session via hooks. Two characteristics dominate: regex throughput on the hot path, and cold-start time. The choice of language is largely a choice of which performance ceiling we accept.

## Options

- **Rust (chosen).** Best-in-class regex throughput and sub-millisecond cold start; the established pattern for fast text-processing CLIs (ripgrep, fd, bat, sd, delta) is overwhelmingly Rust, so their patterns are copyable.
- **Go.** Faster to ship, simpler stdio concurrency, more mature official Starlark. But Go's `regexp` is meaningfully slower than Rust's `regex` (same RE2 lineage, far less optimization) — a real ceiling for a tool whose whole job is regex over streamed text. Rejected.
- **Node/TypeScript.** Familiar and a natural fit for the JS/TS ecosystem we filter output from, but Node's 50–100 ms cold start is incompatible with per-hook invocation thousands of times per session; Bun/Deno help but add deployment friction. Rejected.
- **Zig, Nim, etc.** Too niche to attract contributors and lacking the crate ecosystem. Rejected.

## Decision

Rust, using the `regex`, `clap`, `rusqlite`, and `starlark-rust` crates as the foundation.

## Consequences

- Best-in-class regex throughput — the `regex` crate is the fastest production RE2-derived engine.
- Sub-millisecond cold start with minimal dependencies, important because the binary is invoked on every hook fire.
- Mature crates for everything we need (CLI parsing, SQLite, Starlark, YAML).
- Cross-compilation via `cargo zigbuild` or `cross` is solid for macOS + Linux distribution.
- Steeper learning curve for contributors whose primary stack is TypeScript/JavaScript.
- Compile times are slower than Go, but acceptable for a project of this size.
