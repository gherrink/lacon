# Phase 7: Close gap — capture raw output on opt-in - Discussion Log (Assumptions Mode)

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions captured in CONTEXT.md — this log preserves the analysis.

**Date:** 2026-05-22
**Phase:** 07-close-gap-capture-raw-output-on-opt-in-so-lacon-explain-work
**Mode:** assumptions
**Areas analyzed:** Capture gating, stdout/stderr split, RawOutput construction, Test obligations

## Assumptions Presented

### Capture gating (RunOutcome / RunOptions)
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| New `Option<Vec<u8>>` field on `RunOutcome`, populated only when a new `RunOptions{capture_raw}` flag is set; default-off path consumes `raw_buffer` as today (zero extra copy) | Likely | `runtime/mod.rs:48-55` (RunOptions `#[derive(Default)]`), `:71-85` (RunOutcome), `:344/350/358` (raw_buffer moved), `:382`/`:516` + `run.rs:126-133` (construction sites); ADR-0013 cold-start, ADR-0005 memory bound |

### stdout/stderr split (column mapping)
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Merged stream → `stdout` column; `stderr` empty; capture bytes = `raw_buffer.join("\n")` (no trailing newline) | Confident | `runtime/mod.rs:60-66` (`raw_stderr_bytes = 0` merged stream), `:440-443` (filter_bytes split), `:267-270` (raw_buffer build); `explain.rs:106-107` (stdout++stderr concat); masked test `cli_explain.rs:243-247` |

### RawOutput construction & ownership
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Construct `RawOutput{stdout, stderr: Vec::new()}` in `run.rs`; pass `Some(&raw)` replacing `None` at `run.rs:275` | Confident | `tracking/mod.rs:40-44` (RawOutput), `record.rs:44-52`/`:81-84`/`:90-106` (record + double-gate), `run.rs:250/265-268` (opt-in computed in run.rs) |

### Test obligations
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Add true E2E `lacon run → explain` byte-exact test (hermetic via test_emitter); keep `raw_outputs_empty_by_default` green; add `RunOutcome` shape unit test; masked seeded test stays | Confident | `tracking_e2e.rs:40-64`/`:123-143`, `sc2_privacy_warning_via_cli:240-297`, `bin/test_emitter`, `cli_explain.rs:217` |

## Corrections Made

No corrections — all assumptions confirmed. User selected "Yes, proceed" on the single confirmation gate. The only Likely item (Area 1 capture gating) was accepted with its recommended alternative (gated `RunOptions{capture_raw}` flag, off by default).

## External Research

None performed — pure internal-wiring phase; every decision point answerable from existing source (reader path, write API, data contract, and test harness all already implemented).
