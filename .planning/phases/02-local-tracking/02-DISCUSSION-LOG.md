# Phase 2: Local tracking - Discussion Log (Assumptions Mode)

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions captured in 02-CONTEXT.md — this log preserves the analysis.

**Date:** 2026-05-06
**Phase:** 02-local-tracking
**Mode:** assumptions
**Areas analyzed:** Crate layout & module placement, Cold-start strategy, Schema migration mechanism, Tracker write failure handling, Privacy marker file & env-var contract for adapter

## Assumptions Presented

### Crate layout & module placement

| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Tracker lives in `crates/lacon-core/src/tracking/` as a sibling module; surface is `Tracker::open` / `record(&InvocationMeta, Option<&RawOutput>)` / `prune(&Retention)`; called from `lacon-cli/src/commands/run.rs` after `Runner::run` returns. | Confident | `docs/architecture.md` places tracker inside core; `crates/lacon-core/src/runtime/mod.rs:90-113` already defines `InvocationMeta` for the hand-off; `crates/lacon-cli/src/commands/run.rs:48-50` is the un-instrumented call-site. |

### Cold-start strategy

| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Lazy DB connection (only on the write path); migrations via `PRAGMA user_version`; pruning gated by `lacon_meta(last_pruned_ts)` with 24h throttle; `rusqlite` with `bundled` feature. | Likely | Phase 1 baseline `--version` 1154µs / `validate` 1259µs (STATE.md:87) → ~8.7ms headroom; ADR-0013 makes `lacon run` the production hot path; `bundled` is hermetic for CI per REQ-acceptance-test-coverage. |

### Schema migration mechanism

| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Hand-rolled numbered migrations using `PRAGMA user_version`; SQL embedded as inline `const` strings in `tracking/migrations.rs`; migration `0001` ships full schema (3 tables + 6 indexes + 4 views) inside a single transaction. | Likely | `01-CONTEXT.md` D-03 keeps deps small; ADR-0011 specifies append-only files at startup; `user_version` is the SQLite-native idiom. |

### Tracker write failure handling

| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Best-effort: filtered output reaches stdout BEFORE the tracker call; tracker errors logged to stderr (`lacon: tracker write failed: <err>`); subprocess exit code propagates unchanged. DB-init errors disable the tracker for the rest of the invocation. | Confident | ADR-0013 contract — filtered bytes reach assistant + propagate exit code. `lacon-cli/src/commands/run.rs:165-172` already best-effort for runtime errors. |

### Privacy marker file & env-var contract for adapter

| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Marker file: zero-byte sentinel at `<project_dir>/.lacon/.store_raw_outputs_acked` (project layer) or `~/.config/lacon/.store_raw_outputs_acked` (user layer). Check happens in `Tracker::record` BEFORE first `raw_outputs` INSERT. `LACON_ASSISTANT` (default `"claude-code"`) and `LACON_SESSION_ID` (default unset → NULL) are the env-var contract Phase 3 satisfies. `command_normalized` derived from `<basename(argv[0])> <argv[1]>` per spec. | Likely | `docs/specs/tracking-data-model.md:168` — "marker in the project config dir"; project dir = `<cwd>/.lacon/` per config-schema.md:11-14; `LACON_DISABLE` precedent at `runtime/mod.rs:157`; `InvocationMeta` deliberately omits `assistant` / `session_id` per Phase 1 boundary. |

## Corrections Made

No corrections — all five assumptions confirmed as-is.

## External Research Flagged for Plan

The analyzer surfaced three benchmark/research items the planner should fold into Phase 2 plans rather than the context. Listed here for the audit trail:

- **`rusqlite` cold-start cost on the hot path** (`Connection::open` + WAL pragma + `busy_timeout` pragma + `user_version` check + single INSERT). Need to fit in ≤2.5ms additional cost on top of Phase 1's ~1.2ms baseline. Gates whether the once-per-day prune throttle is mandatory or merely belt-and-suspenders.
- **First-time migration cost** (apply migration `0001` against an empty DB) — should be one-time per machine; confirm <50ms for first-run UX.
- **WAL contention** with concurrent `lacon run` from parallel Claude sessions — the chosen 200ms `busy_timeout` (D-11) is a starting point; raise to 500ms or surface as v2 backlog if tests expose contention.

These are recorded in 02-CONTEXT.md `<decisions>` section under "Implementation-time benchmarks for the planner to schedule into Phase 2" so the planner sees them as research/benchmark tasks rather than gating decisions.

## Auto-Resolved

Not applicable — assumptions mode reached `present_assumptions` without `--auto`; user confirmed all five assumptions on first pass.

## External Research

No external WebSearch / Context7 lookups performed. Codebase + specs (`docs/specs/tracking-data-model.md`, `docs/specs/config-schema.md`) and ADRs (0009, 0011, 0013) provided complete decision input. The benchmark items above are the only research deliberately deferred to plan/execute time, where running code can be measured rather than predicted.
