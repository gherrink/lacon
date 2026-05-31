# Phase 9: Output-fidelity safety — Discussion Log (Assumptions Mode)

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions captured in CONTEXT.md — this log preserves the analysis.

**Date:** 2026-05-31
**Phase:** 09-output-fidelity-safety-no-fabrication-on-dedupe-collapse-and
**Mode:** assumptions
**Areas analyzed:** Bypass detection (inline LACON_DISABLE prefix), Defense-in-depth in `lacon run`, Fabrication source (dedupe/collapse_repeated), Bundled-rule re-audit, Spec/schema impact

## Assumptions Presented

### Bypass detection (inline `LACON_DISABLE=1` env prefix)
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Fix adds a NEW leading-env-assignment parser in `detect_bypass`; no reusable "D-26" parser exists (it only emits a prefix) | Confident | `lib.rs:45-49` reads only hook's own env; `lib.rs:209-219` emits prefix; `chain.rs:509,700` treats `KEY=value` as wrap-safe literal |
| Parser strips leading `NAME=value`, checks `LACON_DISABLE` for exact `"1"`, leading-position only | Likely | `lib.rs:384-394`, `runtime/mod.rs:191` (exact-"1" semantics); bash assignment-position rule |

### Defense-in-depth inside `lacon run`
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| `lacon run` already byte-exact bypasses on own-env `LACON_DISABLE=1`; gap is purely the hook; fixing `detect_bypass` is sufficient | Confident | `runtime/mod.rs:189-193, 525-567` (`run_bypassed`, `Stdio::inherit()`, no pipeline) |

### Fabrication source
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| `dedupe` never fabricates (verbatim-only); only non-verbatim emissions are `collapse_repeated` summary + `max_bytes` marker; user's `table table table` = the summary line | Likely | `stages.rs:256-270` (dedupe verbatim), `stages.rs:276-291,428-444` (summary), `stages.rs:450-457` (marker); `git-status` fixture collapses 118 filenames |
| Criterion #1 conflicts with by-design summary; needs reconciliation (standardized lacon marker vs treat-as-signal vs drop-only) | Unclear | `filter-rule-schema.md:128-137` documents summary; `stages.rs:452` marker convention |

### Bundled-rule re-audit
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Scope = exactly `git-status.yaml` (high risk) + `tsc.yaml` (low risk); no other bundled rule uses these primitives | Confident | grep of `bundled-rules/`; `tsc.yaml:11-14`, `git-status.yaml:14-17` |

### Spec/schema impact
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Marker fix touches user-facing `filter-rule-schema.md:128-137` — deliberate documented change | Likely | spec documents `summary`/`{count}`; CLAUDE.md: spec change = breaking |

## Corrections Made

### Fabrication fix (the one Unclear item — resolved by user choice)
- **Original assumption (Unclear):** Reconcile criterion #1 with the by-design summary line; three alternatives presented — (A) standardized `[lacon: …]` marker, (B) treat tabular as signal / don't collapse, (C) drop-only zero-non-verbatim.
- **User decision:** **Hybrid of A + B** — "remove collapse_repeated where it may bite us and add a marker." → Remove/narrow `collapse_repeated` from bundled rules where it collapses signal (esp. git-status tabular filenames), AND standardize the elision marker so any remaining collapse emits an unambiguous lacon-namespaced marker that can't be mistaken for real output. Captured as D-07 (marker) + D-08 (remove where it bites) + D-09 (never substitute).

### All other areas
- **User decision:** "Yes, lock them." Bypass-parser scope, edge-case handling (quoting, leading-position), defense-in-depth, dedupe, re-audit scope, and spec impact all locked as presented.

## External Research
None performed — local-only Rust workspace with locked ADRs; codebase evidence was sufficient. The single open judgment call was a product decision (resolved above), not a research item.
