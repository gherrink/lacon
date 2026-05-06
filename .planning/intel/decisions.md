# Decisions (synthesized from ADRs)

All 13 ADRs in `docs/decisions/` are classified ADR with `locked: true`. They form an internally consistent, additively-related set as of 2026-05-05 (ADR 0013 closes the last load-bearing open question). Listed in numeric order.

---

## ADR-0001 — Use Claude Code hooks for integration

- **source:** docs/decisions/0001-use-claude-code-hooks.md
- **status:** Accepted (LOCKED)
- **scope:** Claude Code hooks, PreToolUse hook, PostToolUse hook, Bash tool integration, assistant adapter, PATH shim alternative, shell function injection alternative
- **decision:** Use Claude Code's `PreToolUse` and `PostToolUse` hooks as the primary integration mechanism for v1. Reject PATH-wrapping shims and shell function injection.
- **note (subsequent narrowing):** ADR 0013 narrows v1 integration to `PreToolUse` only; `PostToolUse` is reserved for v1.5 unmatched-command annotation.

---

## ADR-0002 — Rust as primary language

- **source:** docs/decisions/0002-rust-as-primary-language.md
- **status:** Accepted (LOCKED)
- **scope:** language choice, Rust, regex crate, clap, rusqlite, starlark-rust, cold start performance, cross-compilation
- **decision:** Implement lacon in Rust using `regex`, `clap`, `rusqlite`, and `starlark-rust`. Drives sub-millisecond cold start and best-in-class regex throughput.

---

## ADR-0003 — Starlark for escape-hatch scripting

- **source:** docs/decisions/0003-starlark-for-escape-hatch.md
- **status:** Accepted (LOCKED)
- **scope:** scripting language, Starlark, filter pipeline, escape-hatch scripting, starlark-rust
- **decision:** Embed Starlark via the `starlark-rust` crate (Meta's implementation) as the scripting language for filter logic beyond native primitives. Hermetic by design — no I/O, no clock, no network.
- **cross_refs:** ADR 0008

---

## ADR-0004 — Project > User > Bundled config precedence

- **source:** docs/decisions/0004-config-precedence.md
- **status:** Accepted (LOCKED)
- **scope:** configuration precedence, rule resolution, bundled rules, user rules, project rules, extends mechanism
- **decision:** Project rules win over user rules, which win over bundled rules. Resolution is first-match-wins; rules from different layers do not merge. Layering is explicit only via `extends`.

---

## ADR-0005 — Streaming-first output processing

- **source:** docs/decisions/0005-streaming-first.md
- **status:** Accepted (LOCKED)
- **scope:** pipeline primitives, streaming output, memory bounds, Starlark post_process
- **decision:** Native pipeline primitives are implemented as streaming line-by-line transformers. Each primitive takes a line and may yield zero, one, or many lines downstream. The Starlark `post_process` stage is the explicit exception (operates on aggregated output — see ADR 0008).
- **cross_refs:** ADR 0008

---

## ADR-0006 — Hybrid command rewriting and output filtering

- **source:** docs/decisions/0006-hybrid-rewrite-and-filter.md
- **status:** Accepted (LOCKED)
- **scope:** filter rules, command rewriting, output filtering pipeline, PreToolUse hook, adapters, tracking
- **decision:** Rules support both pre-execution command rewriting (`rewrite`) and post-execution output filtering (`pipeline`) as first-class mechanisms. Rule authors choose the cheapest tactic per command.

---

## ADR-0007 — First-match-wins rule resolution

- **source:** docs/decisions/0007-first-match-wins.md
- **status:** Accepted (LOCKED)
- **scope:** rule resolver, rule layering, match resolution, extends mechanism
- **decision:** First-match-wins rule resolution. The resolver walks layers in priority order (project → user → bundled), returns the first rule whose `match` block matches. Within a single layer, rules are checked in lexicographic order of their filenames. No merging. No specificity ranking.

---

## ADR-0008 — Aggregated post-process Starlark, not per-line

- **source:** docs/decisions/0008-aggregated-starlark.md
- **status:** Accepted (LOCKED)
- **scope:** Starlark, pipeline, post_process stage, streaming engine
- **decision:** Starlark stages run on aggregated post-pipeline output, not per-line. Native pipeline does bulk reduction first; Starlark gets the small remaining payload. Per-line streaming Starlark is backlogged.

---

## ADR-0009 — Separated raw_outputs table

- **source:** docs/decisions/0009-separated-raw-outputs.md
- **status:** Accepted (LOCKED)
- **scope:** tracking database, SQLite schema, raw_outputs table, invocations table, retention policy, lacon explain
- **decision:** Store raw stdout/stderr blobs in a separate `raw_outputs` table referenced from `invocations.raw_output_id`. Different retention policies per table (default: 30 days for invocations, 3 days for raw outputs). Raw output storage is OFF by default.

---

## ADR-0010 — `on_error` replaces the pipeline, doesn't merge

- **source:** docs/decisions/0010-on-error-replaces-pipeline.md
- **status:** Accepted (LOCKED)
- **scope:** filter rule schema, on_error behavior, pipeline semantics, post_process
- **decision:** A rule's `on_error` block fully replaces both `pipeline` and (optionally) `post_process` when the command exits non-zero. No merging or appending of stages.

---

## ADR-0011 — SQLite for local tracking

- **source:** docs/decisions/0011-sqlite-for-tracking.md
- **status:** Accepted (LOCKED)
- **scope:** tracking, SQLite, rusqlite, history database, WAL mode, stats, explain
- **decision:** Local tracking store is SQLite via `rusqlite`. Database lives at `~/.local/share/lacon/history.db` with WAL mode. Migrations are append-only files run on startup. No daemon, no network.

---

## ADR-0012 — Append-only inheritance via `extends`

- **source:** docs/decisions/0012-append-only-inheritance.md
- **status:** Accepted (LOCKED)
- **scope:** rule inheritance, extends keyword, pipeline composition, filter rule schema
- **decision:** Rule `extends` inherits scalar fields (`description`, `match`, `bypass_when`, `rewrite`, `on_error`, `post_process`) where the child doesn't define them, and *prepends* the parent's `pipeline` stages to the child's. No remove/reorder/insert operations on inherited stages.

---

## ADR-0013 — Filter via PreToolUse-rewritten subprocess wrapper

- **source:** docs/decisions/0013-filter-via-pretooluse-wrapper.md
- **status:** Accepted 2026-05-05 (LOCKED)
- **scope:** Claude Code adapter, PreToolUse hook, lacon run wrapper, subprocess execution, stderr/stdout merging, exit code propagation, on_error pipeline implementation, chained commands wrapping, PostToolUse v1.5 reservation
- **decision:** Filtering happens through a subprocess wrapper invoked by a `PreToolUse` hook that rewrites matched commands. `lacon run --rule <id> -- <cmd> [args...]` spawns the subprocess, merges stderr into stdout, runs the rule's pipeline (or `on_error`), writes filtered bytes to its own stdout — which becomes Claude Code's tool result — and exits with the subprocess's exit code. No `PostToolUse` hook installed in v1.
- **rationale:** Empirical probe 2026-05-05 confirmed `PostToolUse` cannot replace tool output; the prior assumption of a `hookSpecificOutput.updatedToolOutput` field was false. This ADR is the recovery design.
- **cross_refs:** docs/architecture.md, docs/v1-scope.md, docs/open-questions.md, docs/specs/filter-rule-schema.md, docs/specs/chained-commands.md, docs/specs/tracking-data-model.md, ADRs 0001, 0003, 0004, 0005, 0006, 0007, 0008, 0010, 0011, 0012
- **note (additive):** ADR 0013 is explicitly additive — no prior ADR is amended; only execution location changes.

---

## Cross-reference summary (no cycles detected)

- ADR 0003 → ADR 0008 (Starlark performance rationale)
- ADR 0005 → ADR 0008 (streaming-vs-aggregated boundary)
- ADR 0013 → ADRs 0001, 0003, 0004, 0005, 0006, 0007, 0008, 0010, 0011, 0012 (consequence statements; no contradictions)

DFS over the directed graph completed cleanly. Max depth observed: 2.
