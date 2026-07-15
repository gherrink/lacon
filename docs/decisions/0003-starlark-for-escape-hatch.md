---
status: accepted
schema-version: 2
---

# 0003: Starlark for escape-hatch scripting

## Context

The native pipeline primitives (regex drop, ANSI strip, collapse-repeated, etc.) cover the ~90% of filter logic that is purely textual. The remaining ~10% needs real logic: stack-trace boundary detection, structured-output parsing (e.g. Jest's failure trees), and context-aware decisions like "find the success-summary line and trim everything before it." That 10% needs an embedded scripting language.

## Options

- **Starlark (chosen).** Hermetic by design — no I/O, clock, or network — with Python-like syntax and a mature implementation Meta maintains at scale (Buck2).
- **Lua.** Smaller and faster, but sandboxing means aggressively stripping its stdlib (file I/O, `os.execute`, …); "safe Lua" is its own learning curve. Hermetic-by-design beats hermetic-by-careful-configuration in a security-sensitive context. Rejected.
- **WASM.** Most flexible (any language compiling to WASM), but runtime startup cost and complexity are overkill for line filtering, and authoring WASM modules is far heavier than writing a Starlark function. Rejected.
- **Custom DSL.** Maintenance, debugging tools, and error messages would all be ours to build — and done worse than an established option. Rejected.
- **Rhai.** Rust-native and ergonomic, but a small ecosystem, and adopting it ties the scripting story to Rust forever (a problem if the engine is ever rewritten). Rejected.

## Decision

Starlark, embedded via the `starlark-rust` crate (Meta's implementation).

## Consequences

- Hermetic by design: no I/O, no clock, no network. Users can share filter scripts without supply-chain risk.
- Python-like syntax familiar to most developers.
- Mature implementation maintained at scale by Meta (Buck2).
- Slower than native primitives, but acceptable because Starlark stages run on already-reduced output, not per-line streaming (see ADR 0008).
- Adds a dependency that increases binary size by a few MB.
