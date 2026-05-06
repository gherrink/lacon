# Phase 1: Engine core & `lacon run` wrapper - Discussion Log (Assumptions Mode)

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions captured in CONTEXT.md — this log preserves the analysis.

**Date:** 2026-05-06
**Phase:** 01-engine-core-lacon-run-wrapper
**Mode:** assumptions
**Areas analyzed:** Rust workspace layout & dependency selection; Pipeline primitive abstraction & `max_bytes` enforcement; `lacon run` runtime model; Rule loader, regex cache, and `lacon validate` dispatch

## Assumptions Presented

### A. Rust workspace layout & dependency selection

| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Three-crate workspace (`lacon-core`, `lacon-cli`, `lacon-adapter-claudecode` as Phase-1 stub) | Likely | `docs/architecture.md` 122–134 ("File layout"); CLAUDE.md "planned crate layout" |
| Pin v1 deps: `regex` (not `fancy-regex`), `serde`+`serde_yaml`, `clap` v4 derive, `starlark-rust`, `os_pipe`+`std::process`, `crossbeam`/`mpsc`, `nix`, `thiserror` internally + `anyhow` at CLI boundary, `etcetera`, `rust-embed`/`include_str!` | Likely | ADR-0002 names regex/clap/starlark-rust; cold-start <10ms (ADR-0013) rules out tokio runtime init |
| No async runtime — synchronous `std::process::Command` + dedicated OS threads | Likely | ADR-0013 "thousands of times per session"; tokio reactor init ~2-5ms standalone |
| Edition 2021; MSRV pinned at start of Phase 1 | Confident | Standard Rust practice for new crates in 2026 |

### B. Pipeline primitive abstraction & `max_bytes` enforcement

| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Closed `enum Stage` with `step(&mut self, line, out)`, dispatched by `match`; stateful primitives carry buffers inline | Likely | ADR-0005 streaming + bounded memory; enum+match = zero vtable on hot path |
| Multiple `keep_regex` stages OR-merged into single `RegexSet` at load time | Confident | `docs/specs/filter-rule-schema.md`: "Multiple `keep_regex` stages are OR'd" |
| `max_bytes` enforced as: explicit pipeline stage AND implicit final cap injected at load when rule omits its own (sourced from `defaults.max_bytes`, default 32768) | Confident | CON-config-v1-keys; `docs/specs/config-schema.md` lines 41–43 |
| Truncation marker `[lacon: truncated, N more bytes dropped]` byte-exact | Confident | CON-filter-rule-native-primitives |

### C. `lacon run` runtime model

| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| `std::process::Command` with `os_pipe` pipes; NO PTY allocation | Confident | ADR-0013 explicitly: "the wrapper does not allocate a PTY" |
| Two dedicated OS threads (one per stream) read complete lines into a single channel; main thread runs pipeline loop | Likely | Canonical no-async POSIX pattern; alternative (kernel-level dup2 merge) loses line-level atomicity |
| Stream merge guarantee: best-effort line atomicity, no cross-stream order guarantee (settles Q-deferred-merge-ordering) | Unclear | `docs/open-questions.md` Q-deferred-merge-ordering; ADR-0013 "Acceptable v1 trade-off" |
| Signal forwarding via `nix::sys::signal::kill` to subprocess PID, no drain, exit with subprocess code (or 128+sig) (settles Q-deferred-signal-forwarding) | Unclear | `docs/open-questions.md` lines 19–23 likely-answer "SIGTERM forward + immediate exit" |
| Success buffer bounded by `max_bytes`+stateful-primitive reservation; discarded if `on_error` selected; raw line stream retained until exit code is known | Likely | ADR-0010 + ADR-0013; CON-nfr-streaming-memory |

### D. Rule loader, regex cache, and `lacon validate` dispatch

| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Lazy-resolve-on-demand on the hot path (`lacon run --rule <id>` parses only the matching file); eager for `validate`/`doctor`/manual `run` | Likely | REQ-acceptance-cold-start + ADR-0013 "thousands of times per session" |
| Compiled regex cache in-process only for v1 (no disk persistence); mtime-based hot-reload | Likely | `docs/architecture.md` line 60: "invalidates on rule file mtime change"; per-rule regex compile is sub-ms |
| `extends` flattening at load time; cycles → `CircularExtends` error | Confident | CON-filter-rule-extends-semantics: "chains flattened at load time" |
| `lacon validate <path>` dispatches by content: top-level `id`+`match` → rule validator; otherwise → config validator. No silent fallback on malformed config. | Confident | CON-config-validation-dispatch; `docs/specs/config-schema.md` lines 113–121 |
| `thiserror` enum with categories `InvalidRegex \| UnknownPrimitive \| CircularExtends \| MissingScriptFile \| UserOnlyKeyInProject \| UnknownKey`; format `<path>:<line>: <category>: <message>` | Likely | CON-filter-rule-validation; example in `docs/specs/config-schema.md` line 103 |

## Corrections Made

No corrections — user selected "Yes, proceed" on the first pass. All assumptions confirmed as-is and locked into CONTEXT.md.

## External Research

Skipped. Items flagged in the analyzer's "Needs External Research" section are implementation-time benchmarks (not knowledge gaps), folded into CONTEXT.md as "Implementation-time benchmarks for the planner to schedule into Phase 1":

1. `starlark-rust` cold-start cost — measure load+evaluate trivial `process(ctx, lines)`. If >2ms, lazy-init the VM only when matched rule has `post_process`.
2. `clap` v4 vs `pico-args`/hand-rolled startup cost.
3. `os_pipe` + threads vs `duct` vs raw `nix` micro-benchmark for the merge.
4. POSIX signal-forwarding macOS vs Linux — single-PID vs process-group semantics.

These belong in the Phase 1 plan as research/benchmark tasks, not as pre-context lookups.
