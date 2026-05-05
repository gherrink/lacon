# 0003: Starlark for escape-hatch scripting

**Status:** Accepted

## Context

The native pipeline primitives (regex drop, ANSI strip, collapse repeated, etc.) cover the ~90% of filter logic that's purely textual. The remaining 10% needs real logic: stack trace boundary detection, structured output parsing (e.g. Jest's failure trees), context-aware decisions like "find the success summary line and trim everything before it." We need an embedded scripting language for this 10%.

## Decision

Starlark, embedded via the `starlark-rust` crate (Meta's implementation).

## Consequences

- Hermetic by design: no I/O, no clock, no network. Users can share filter scripts without supply-chain risk.
- Python-like syntax familiar to most developers
- Mature implementation maintained at scale by Meta (Buck2)
- Slower than native primitives, but acceptable because Starlark stages run on already-reduced output, not per-line streaming (see ADR 0008)
- Adds a dependency that increases binary size by a few MB

## Alternatives considered

**Lua.** Smaller and faster than Starlark. But sandboxing requires aggressively stripping its standard library (file I/O, `os.execute`, etc.), and the resulting "safe Lua" is its own learning curve. Hermetic-by-design beats hermetic-by-careful-configuration for code that runs in a security-sensitive context.

**WASM.** Most flexible — any language that compiles to WASM. But the startup cost and complexity of a WASM runtime is overkill for line filtering, and the toolchain to author WASM modules is much heavier than writing a Starlark function.

**A custom DSL.** Maintenance burden, debugging tools, error messages — all things we'd have to build ourselves and inevitably do worse than an established option.

**Rhai.** Rust-native, ergonomic, but the ecosystem is small and adopting it ties our scripting story to Rust forever (an issue if we ever rewrite the engine in another language).
