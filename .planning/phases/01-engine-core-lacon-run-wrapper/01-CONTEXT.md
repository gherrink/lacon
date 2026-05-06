# Phase 1: Engine core & `lacon run` wrapper - Context

**Gathered:** 2026-05-06 (assumptions mode)
**Status:** Ready for planning

<domain>
## Phase Boundary

A `lacon` binary that, given a YAML rule, can spawn a subprocess, merge stderr into stdout, run the streaming pipeline (or `on_error` on non-zero exit), enforce the `max_bytes` cap, and write filtered output to its own stdout. Also delivers `lacon validate` for rule+config dispatch.

Subsumes Rust workspace scaffolding, `Cargo.toml` setup, and dependency selection. Adapter integration (Phase 3) and tracking persistence (Phase 2) are out of scope — the `lacon-adapter-claudecode` crate exists in Phase 1 only as a stub that keeps the workspace compiling and the assistant-agnostic boundary intact.

**Requirements covered:** REQ-engine-streaming-primitives, REQ-engine-starlark-postprocess, REQ-engine-rule-loading, REQ-engine-extends, REQ-engine-on-error, REQ-engine-rewrite, REQ-engine-bypass, REQ-engine-max-bytes-cap, REQ-cli-run, REQ-cli-validate.
</domain>

<decisions>
## Implementation Decisions

### A. Workspace layout & dependency selection

- **D-01:** Three-crate Cargo workspace: `crates/lacon-core` (resolver, pipeline, primitives, Starlark host, validators), `crates/lacon-cli` (clap, subcommand dispatch, `lacon run` hot path), `crates/lacon-adapter-claudecode` (Phase-1 stub — empty crate or trivial trait impl that keeps the workspace compiling so Phase 3 has a real boundary to fill in).
- **D-02:** Edition 2021. MSRV pinned at start of Phase 1 (record actual version in `Cargo.toml`); pin in `rust-toolchain.toml` for reproducibility.
- **D-03:** v1 dependency set, locked here:
  - `regex` (NOT `fancy-regex` — no backreferences used by any primitive)
  - `serde` + `serde_yaml` for YAML parsing
  - `clap` v4 with `derive`
  - `starlark-rust` (Meta's crate) for the Starlark `post_process` stage
  - `os_pipe` + `std::process` for subprocess spawn and stream merge — **NO `tokio` or any async runtime** (cold-start budget)
  - `crossbeam-channel` (or `std::sync::mpsc`) for the stderr/stdout merge channel
  - `nix` for POSIX signal forwarding (`kill(pid, SIGTERM)` etc.)
  - `thiserror` for crate-internal error enums
  - `anyhow` ONLY at the CLI boundary (`crates/lacon-cli/src/main.rs`)
  - `etcetera` (or hand-rolled XDG resolution) for `~/.config/lacon/`, `~/.local/share/lacon/`
  - `rust-embed` or `include_str!`/`include_dir!` for the bundled rules (Phase 5 fills the rules; Phase 1 stands up the embedding mechanism)
- **D-04:** No async runtime in v1. `lacon run` uses synchronous `std::process::Command` plus dedicated OS threads for the merge, channeled via `crossbeam`/`mpsc`.

### B. Pipeline primitive abstraction & `max_bytes` enforcement

- **D-05:** Primitives implemented as a single closed `enum Stage { ... }` with a `step(&mut self, line: Cow<str>, out: &mut SmallVec<...>)` method dispatched via `match`. Stateful primitives (`keep_tail`, `dedupe`, `collapse_repeated`, `keep_around_match`) carry their bounded buffers inline as enum-variant fields. Pipeline = `Vec<Stage>`. No `Box<dyn Stage>`, no vtable indirection on the hot path.
- **D-06:** Multiple `keep_regex` stages OR-merged into a single `RegexSet` at load time (per `docs/specs/filter-rule-schema.md` "Multiple `keep_regex` stages are OR'd").
- **D-07:** `max_bytes` enforcement lives in two places:
  1. Any explicit `max_bytes:` stage in the rule's pipeline, evaluated as just-another-stage.
  2. An implicit final cap injected at load time when the rule omits its own `max_bytes` primitive — sourced from `defaults.max_bytes` (default 32768) per CON-config-v1-keys and `docs/specs/config-schema.md` lines 41–43.
- **D-08:** Truncation marker `[lacon: truncated, N more bytes dropped]` is byte-exact (per CON-filter-rule-native-primitives) and emitted by the `MaxBytes` stage when overflow occurs.

### C. `lacon run` runtime model

- **D-09:** Spawn via `std::process::Command` with stdout/stderr connected through `os_pipe` pipes. **NO PTY allocation** (already locked by ADR-0013 — most tools emit less noise in non-TTY mode).
- **D-10:** Two dedicated OS threads (one per stream) read complete lines and emit them into a single `crossbeam-channel`/`mpsc`. The main thread runs the pipeline loop pulling lines off the channel.
- **D-11:** Stream merge guarantee (Q-deferred-merge-ordering, settled here): **best-effort line atomicity, no cross-stream order guarantee.** Each individual line from stderr or stdout is emitted whole; stderr-line vs stdout-line interleaving is wall-clock-arrival order from the threads' perspective. Document this guarantee in `docs/architecture.md` once the implementation lands.
- **D-12:** Signal forwarding (Q-deferred-signal-forwarding, settled here): SIGTERM and SIGINT received by `lacon run` are forwarded to the subprocess PID via `nix::sys::signal::kill`. The wrapper does **NOT** drain or flush remaining buffered output. Wrapper exits with the subprocess's exit code (or `128 + sig` if the subprocess was killed by a signal).
- **D-13:** Success-pipeline output is buffered in a bounded ring (capped by `max_bytes` + the largest stateful primitive's reservation, satisfying CON-nfr-streaming-memory) and written to wrapper stdout only after the subprocess exits with code 0. On non-zero exit, the success buffer is **discarded** and the raw line stream is run through the `on_error` pipeline instead (per ADR-0010 + ADR-0013). The raw line stream must be retained alongside the success buffer until exit code is known — bounded by the same memory cap.

### D. Rule loader, regex cache, and `lacon validate` dispatch

- **D-14:** Loader is **lazy-resolve-on-demand on the hot path**: `lacon run --rule <id>` walks `<cwd>/.lacon/rules/` → `~/.config/lacon/rules/` → bundled embedded set, parses ONLY the file matching the requested `id`, applies `extends` flattening at parse time. Manual `lacon run` (no `--rule`), `lacon validate`, and `lacon doctor` use the eager path (parse all reachable rule files).
- **D-15:** Compiled regex cache is **in-process only for v1** (no disk persistence). Per-rule regex compile is sub-millisecond. Hot-reload (REQ-acceptance-hot-reload) is via mtime check at resolve time — invalidate cache entry when `mtime` differs. Revisit on-disk cache only if Phase 6 cold-start benchmarks demand it.
- **D-16:** `extends` flattens at load time to a single concrete rule (CON-filter-rule-extends-semantics). Cycles rejected with `CircularExtends`. Single-level only — multi-hop chains are flattened recursively but the chain itself is not exposed.
- **D-17:** `lacon validate <path>` dispatch: parse YAML to `serde_yaml::Value`, look for top-level `id` AND `match` (both required per CON-config-validation-dispatch). If both present → rule validator. Otherwise → config validator. Files that fail validation are rejected; `lacon` does NOT silently fall back to defaults on malformed config.
- **D-18:** Validation error type is a `thiserror`-derived enum with categories: `InvalidRegex`, `UnknownPrimitive`, `CircularExtends`, `MissingScriptFile`, `UserOnlyKeyInProject`, `UnknownKey`. Output format is one error per line: `<path>:<line>: <category>: <message>` (matches the example in `docs/specs/config-schema.md` line 103). Errors carry file path + line/column from `serde_yaml::Error` where available.

### Implementation-time benchmarks for the planner to schedule into Phase 1

These are not gating decisions but measurements to take during Phase 1 work — the planner should fold them into the plan as research/benchmark tasks:

1. **`starlark-rust` cold-start cost** — measure "load + evaluate trivial `process(ctx, lines)`". If >2ms, lazy-init the Starlark VM only when the matched rule actually has `post_process`.
2. **`clap` v4 vs `pico-args`/hand-rolled startup cost** — `clap` derive is the documented choice (ADR-0002) but adds ~1-3ms. If >2ms on a 6-subcommand surface, fall back to `pico-args` is recoverable plan-B.
3. **`os_pipe` + threads vs `duct` vs raw `nix`** — micro-benchmark the chosen merge approach; sub-ms differences matter on a 10ms budget.
4. **POSIX signal-forwarding: macOS vs Linux** — pick single-PID vs process-group semantics, verify on both target platforms (CON-nfr-platform-support). `nix::sys::signal::kill` is portable but PG semantics differ subtly.

### Claude's discretion

- Internal module organization within `lacon-core` (e.g., `pipeline/`, `rules/`, `runtime/`, `validate/`) — assumed sensible; planner should organize for readability without re-litigating crate boundaries.
- Specific error message wording inside each `thiserror` variant — must include enough context to act on, exact phrasing left to author.
- Choice between `crossbeam-channel` and `std::sync::mpsc` for the merge channel — both work; `crossbeam` is preferred for unbounded with bounded backpressure semantics, but stdlib `mpsc` is acceptable if dependency surface matters more.
- Choice between `rust-embed` and `include_str!` for bundled-rule embedding — same outcome, pick whichever is simpler at point of use.

### Folded todos

None — `gsd-sdk query todo.match-phase 1` returned 0 matches.
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### ADRs (all status: Accepted, LOCKED)

- `docs/decisions/0001-use-claude-code-hooks.md` — narrowed by ADR-0013 to PreToolUse-only for v1
- `docs/decisions/0002-rust-as-primary-language.md`
- `docs/decisions/0003-starlark-for-escape-hatch.md`
- `docs/decisions/0004-config-precedence.md`
- `docs/decisions/0005-streaming-first.md`
- `docs/decisions/0006-hybrid-rewrite-and-filter.md`
- `docs/decisions/0007-first-match-wins.md`
- `docs/decisions/0008-aggregated-starlark.md`
- `docs/decisions/0010-on-error-replaces-pipeline.md`
- `docs/decisions/0012-append-only-inheritance.md`
- `docs/decisions/0013-filter-via-pretooluse-wrapper.md` — `lacon run` is the production hot path

### Specs (load-bearing contract)

- `docs/specs/filter-rule-schema.md` — full YAML rule format, all 10 primitives, Starlark stage, `on_error`, `extends`
- `docs/specs/config-schema.md` — config layer merge, USER-ONLY keys, `lacon validate` dispatch, error format
- `docs/specs/tracking-data-model.md` — informs validation only (Phase 1 doesn't write to it; Phase 2 does)
- `docs/specs/chained-commands.md` — informs `lacon run --` boundary semantics (Phase 3 implements the splitter)

### Architecture and project context

- `docs/architecture.md` — component boundaries, file layout, hook flow
- `docs/v1-scope.md` — explicit in-scope/out-of-scope list
- `docs/open-questions.md` — Q-deferred-signal-forwarding (settled in D-12), Q-deferred-merge-ordering (settled in D-11)
- `.planning/PROJECT.md`, `.planning/REQUIREMENTS.md`, `.planning/ROADMAP.md`
- `.planning/intel/constraints.md` — full 29-entry CON-* list (filter rule, config, tracking, chained, NFRs)
- `.planning/intel/decisions.md` — decision log mirroring ADRs
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

**None** — greenfield Rust project. No `Cargo.toml`, no `src/`, no `crates/` directory exists on disk. Phase 1 stands up the workspace from scratch.

### Established Patterns

The 13 ADRs and 4 specs ARE the established pattern. Notable shape constraints flowing into Phase 1 implementation:

- **Streaming-first, no buffering** (ADR-0005) — every primitive is a line-by-line transformer; `post_process` (ADR-0008) is the only deliberate exception, on aggregated output.
- **First-match-wins, no merging** (ADR-0007) — the resolver walks layers in priority order and returns the first matching rule. `extends` (ADR-0012) is the only explicit cross-rule layering, and it append-only-prepends.
- **`on_error` REPLACES, never merges** (ADR-0010) — when the subprocess exits non-zero, the success buffer is discarded entirely.
- **No daemon, no network, no LLM calls** (architectural commitment). All processing local. Cold-start <10ms is the contract.

### Integration Points

Phase 1 outputs that downstream phases consume:

- **For Phase 2 (tracking):** the `lacon run` wrapper must expose enough metadata at exit (rule_id, rule_source, exit_code, byte counts, timing) for the tracker write-path to record an `invocations` row. Define the metadata struct in `lacon-core` so Phase 2 can hang the SQLite writer off it without refactor.
- **For Phase 3 (adapter):** the rule resolver must be callable as a library function — the adapter's `PreToolUse` hook does its own resolve before emitting the rewritten command. The adapter does not invoke `lacon run --rule <id>` to find the rule; it knows the rule_id by then.
- **For Phase 5 (bundled rules):** the embedding mechanism (`rust-embed` or `include_str!`) and the bundled-rules layer in the resolver are stood up here. Phase 5 only adds rule files + fixtures.
- **For Phase 6 (acceptance):** `REQ-acceptance-cold-start-budget` (<10ms) and `REQ-acceptance-hot-reload` are validated end-to-end here. Phase 1 must not architecturally preclude either — hence the no-async, mtime-based-cache decisions above.
</code_context>

<specifics>
## Specific Ideas

No specific user references — assumptions confirmed as-is on first pass. Approaches above are derived from locked ADRs/specs plus standard Rust idioms for a sub-10ms-cold-start CLI.
</specifics>

<deferred>
## Deferred Ideas

- **On-disk persisted regex cache** — deferred to Phase 6 if cold-start benchmarks demand it. Per-rule regex compile is sub-ms, so likely not needed.
- **Per-line streaming Starlark** — explicitly out of v1 scope (ADR-0008, backlog).
- **`Box<dyn Stage>` extensible primitive trait** — rejected for v1 in favor of closed `enum Stage`. Revisit only if external rule authors need to ship their own primitives, which is not in v1 or v2 scope today.
- **`tokio` async runtime** — rejected on cold-start grounds. Revisit only if signal-forwarding semantics turn out to require it.

### Reviewed Todos (not folded)

None reviewed — `gsd-sdk query todo.match-phase 1` returned 0 matches.
</deferred>
