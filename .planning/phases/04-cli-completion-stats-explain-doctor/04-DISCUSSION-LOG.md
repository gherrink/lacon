# Phase 4: CLI completion (`stats`, `explain`, `doctor`) - Discussion Log (Assumptions Mode)

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions captured in CONTEXT.md — this log preserves the analysis.

**Date:** 2026-05-21
**Phase:** 04-cli-completion-stats-explain-doctor
**Mode:** assumptions
**Areas analyzed:** Tracking read/query API placement; `explain` re-derivation path; `doctor` checklist & DB-open posture; `stats` filters/output/`--since`; `explain` diff rendering; six-command surface cap.

## Assumptions Presented

### Tracking read/query API placement
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| New module `lacon-core/src/tracking/query.rs`; `lacon-cli` keeps `rusqlite` dev-only (no runtime dep) | Confident | `lacon-cli/Cargo.toml:23-30`, Phase 2 D-01, `record.rs`→`run.rs:273-280` |
| Query commands open DB read-only (no migrate/no prune) | Likely | Phase 2 D-04, `tracking/mod.rs:104-107` |
| Missing-DB is a normal state (graceful per command) | Confident | fresh-user / empty-DB handling |

### `explain <id>` re-derivation path
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| New byte-replay entry point (Runner-side); cannot reuse `Runner::run` (always spawns) | Likely | `runtime/mod.rs:189-203`, `pipeline/mod.rs:127-138` |
| Flow: parse id→i64, SELECT row, NULL `raw_output_id`→error, load BLOBs, resolve rule, branch on stored `exit_code` | Likely | `loader.rs:59-77,127-151`, `runtime/mod.rs:342-359,327-333`, `tracking-data-model.md:27` |
| Hand-rolled side-by-side render, no diff-crate dep | Likely | CLAUDE.md lean-deps; Phase 3 hand-rolled precedents |

### `doctor` checklist & DB-open posture
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Fixed checklist (hook install / config per layer / rule sweep / 0700 / health), exit 0 only if all pass | Likely | Phase 2 D-13 + `health.rs`, Phase 3 D-28 + `init.rs:143-145,318-329`, `validate/mod.rs:45`, `loader.rs:156`, `mod.rs:165-190` |
| Doctor uses read-only DB open; never migrates/prunes/INSERTs | Likely | Phase 2 D-04, `mod.rs:104-107` |

### `stats` filters/output/`--since`/arg threading
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Filters re-query base `invocations` (views lack `ts`/most lack project/rule), not the views directly | Likely | `tracking-data-model.md:96-141`, `idx_inv_ts/project/rule` |
| `--since` relative-only (`7d`/`24h`) → unix-ms cutoff; ISO deferred | Likely | no `chrono`/`time` workspace dep |
| `main.rs:15-16` discards parsed args → thread them through | Confident | `main.rs:15-16`, `cli.rs:18-54` |
| Six-command cap already satisfied & tested (confirming work only) | Confident | `cli.rs:18-54`, `cli_surface.rs:6-41` |

## Corrections Made

No corrections — all assumptions confirmed on first pass ("Yes, proceed").

## External Research

None spawned. The lone flagged item (diff-crate choice for `explain`) was a design preference, not a factual gap: `similar` is the known Rust diff crate and is already transitively present via the `insta` dev-dep. Folded into CONTEXT.md D-06 as a hand-rolled default with `similar` documented as an escape hatch.
