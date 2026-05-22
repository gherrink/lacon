# Phase 7: Close gap — capture raw output on opt-in so `lacon explain` works end-to-end - Context

**Gathered:** 2026-05-22 (assumptions mode)
**Status:** Ready for planning

<domain>
## Phase Boundary

Capture the **pre-filter (raw) bytes** of a `lacon run` invocation **when `store_raw_outputs` is enabled**, persist them to the existing `raw_outputs` table, so `lacon explain <id>` reproduces a **real** invocation end-to-end (driven by `lacon run`, not a hand-seeded SQL row).

This is the single root-cause gap from the v1.0 milestone audit (`.planning/v1.0-MILESTONE-AUDIT.md`): the reader side (`explain.rs`, `Runner::filter_bytes`, `query::fetch_raw_output`) and the write API (`Tracker::record(meta, raw_opt: Option<&RawOutput>, …)`) are already built and tested — **only the capture path is missing**. `run.rs:275` hard-codes `raw=None`, so opting in produces an empty `raw_outputs` table and every real `lacon explain` hits the "no stored raw output" branch.

**IN scope:** thread buffered raw bytes onto `RunOutcome`; construct `RawOutput` in `run.rs`; pass `Some(&raw)` to `tracker.record()` on the opt-in path; a true E2E `lacon run → lacon explain` test.

**OUT of scope (do not build):** redaction of `raw_outputs`, `lacon purge`, encryption-at-rest, separate stderr capture beyond what the merged runtime stream provides, any new schema/columns, a seventh CLI command. The remediation is "modest, not architectural" — no ADR is amended.
</domain>

<decisions>
## Implementation Decisions

### Capture gating (`RunOutcome` / `RunOptions`)
- **D-01:** Add a new field to `RunOutcome` carrying the captured raw bytes — `Option<Vec<u8>>` (or `Option<RawOutput>`; planner's discretion on the exact wrapper type, but it must be `None` by default). All three `RunOutcome` construction sites set it: `Runner::run` (`runtime/mod.rs:382`), `run_bypassed` (`:516`), and `run_unmatched` (`run.rs:126-133`).
- **D-02:** Capture is **gated** behind a new `RunOptions` flag (e.g. `capture_raw: bool`). `RunOptions` is `#[derive(Default)]` (`runtime/mod.rs:48-55`), so the new bool defaults to `false` and every existing call site (`run.rs:70-73`, `explain.rs:126-129`, filter_bytes tests, all `RunOptions::default()` callers) keeps compiling and stays OFF.
- **D-03:** The default-off hot path pays **zero extra cost**: when the flag is `false`, `raw_buffer` is consumed exactly as today (`raw_buffer.into_iter()` moved into the pipeline at `runtime/mod.rs:344/350/358`). The join-to-bytes cost is paid ONLY when capture is requested — which only `run.rs` does, and only when `store_raw_outputs` is enabled. This preserves the ADR-0013 cold-start budget on the dominant path and keeps memory bounded per ADR-0005 (capture re-serializes the already-bounded `raw_buffer`; no new unbounded buffer).

### stdout/stderr split (column mapping + byte-exact form)
- **D-04:** Store the **entire merged stream in the `raw_outputs.stdout` column** and leave **`stderr` empty** (zero-length BLOB). The runtime has a single interleaved stream by the time `raw_buffer` exists (`ByteCounts.raw_stderr_bytes = 0 // merged single stream in v1`, `runtime/mod.rs:60-66`; D-11 merge contract) — there is no separable stderr to put in the stderr column.
- **D-05:** The captured bytes are **`raw_buffer.join("\n")`** — NOT the original pipe bytes, and with **NO re-added trailing newline**. This is the canonical form `Runner::filter_bytes` expects: it splits `merged_bytes` on `\n` and lossy-decodes each piece (`runtime/mod.rs:440-443`), the exact inverse of how `raw_buffer` was built (lossy decode + strip one trailing `\n`, `runtime/mod.rs:267-270`). So `filter_bytes(raw_buffer.join("\n"))` regenerates the identical `Vec<String>` the live pipeline consumed → byte-exact `lacon explain` reproduction. The read path's `stdout ++ stderr` concatenation (`explain.rs:106-107`) is then a no-op since `stderr` is empty.

### `RawOutput` construction & ownership
- **D-06:** Construct `RawOutput { stdout: <captured bytes>, stderr: Vec::new() }` in **`run.rs`** (not inside the core runner), from the new `RunOutcome` field. Pass `Some(&raw)` to `tracker.record()`, replacing the hard-coded `None` at `run.rs:275`. `RawOutput` is `{ pub stdout: Vec<u8>, pub stderr: Vec<u8> }` (`tracking/mod.rs:40-44`).
- **D-07:** Rely on the existing **double-gate**: `run.rs` already computes `cfg.store_raw_outputs` (`:250`), `project_store_raw` (`:265-266`), `user_store_raw` (`:267-268`); and `Tracker::record` re-gates on `(self.cfg_store_raw_outputs, raw_opt)` (`record.rs:81-84`). So passing `Some` is always safe — the `raw_outputs` INSERT only fires when the config is truly on. Keeping construction in `run.rs` avoids leaking config awareness into the core runner.

### Test obligations
- **D-08:** Add a **true E2E test** that drives `lacon run` against `bin/test_emitter` with a project `.lacon/config.yaml` setting `store_raw_outputs: true`, then drives `lacon explain <id>` on the **same DB**, asserting the filtered column matches the captured `lacon run` stdout **byte-for-byte**. Reuse the existing harness (`tracking_e2e.rs:40-64` XDG-tempdir runner; opt-in config pattern from `sc2_privacy_warning_via_cli:240-297`). Hermetic via `test_emitter` — no real tools.
- **D-09:** Keep the negative guard green: `raw_outputs_empty_by_default` (`tracking_e2e.rs:123-143`) must still pass (default-off ⇒ zero `raw_outputs` rows, `raw_output_id` NULL) — it directly protects D-03's "off path unchanged" claim.
- **D-10:** Add a `runtime/mod.rs` unit test asserting `RunOutcome` raw field is `Some(..)` when `capture_raw=true` and `None` otherwise, so a future edit can't silently drop capture. The masked seeded test (`cli_explain.rs:217`, `explain_filtered_column_byte_equals_run_output`) stays as-is — it is no longer the only proof.

### Claude's Discretion
- Exact wrapper type for the new `RunOutcome` field (`Option<Vec<u8>>` vs `Option<RawOutput>`) and the precise flag name on `RunOptions` — pick whatever reads cleanest in-context, honoring D-01/D-02 semantics.
- Whether the new E2E test lives in `tracking_e2e.rs` (reuses `run_lacon_with_xdg` + config-writing helpers — most direct) or `cli_explain.rs` (co-located with the masked test for contrast, but needs the `test_emitter` driver imported).

### Folded Todos
None — no pending todos matched this phase.
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

- `.planning/v1.0-MILESTONE-AUDIT.md` — the gap source; "Headline finding (BLOCKER)" + remediation paragraph define this phase precisely.
- `docs/specs/tracking-data-model.md` — `raw_outputs` schema (the contract: `:92` "points to the row in `raw_outputs` storing the original stdout/stderr"; `:151` "mainly useful for recent `lacon explain` calls").
- `docs/decisions/0009-separated-raw-outputs.md` — ADR-0009: separated `raw_outputs` table, OFF by default, 3-day retention.
- `docs/decisions/0013-filter-via-pretooluse-wrapper.md` — ADR-0013: `lacon run` is the production hot path; cold-start budget is load-bearing (constrains D-02/D-03).
- `docs/decisions/0005-streaming-first.md` — ADR-0005: streaming-first; memory bounded by largest stateful primitive + `max_bytes` cap (constrains D-03).
- `docs/decisions/0010-on-error-replaces-pipeline.md` — ADR-0010: `on_error` replaces pipeline; explain branch fidelity already handled by `Runner::filter_bytes` (no change needed, but capture must round-trip through it).
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`Tracker::record(meta, raw_opt: Option<&RawOutput>, …)`** (`crates/lacon-core/src/tracking/record.rs:44-52`) + `insert_raw_output` (`:90-106`) — write API already accepts and inserts raw; binds `&raw.stdout`/`&raw.stderr` to the columns. No change needed beyond being called with `Some`.
- **`RawOutput { pub stdout: Vec<u8>, pub stderr: Vec<u8> }`** (`crates/lacon-core/src/tracking/mod.rs:40-44`) — the carrier struct, already public.
- **`Runner::filter_bytes`** (`crates/lacon-core/src/runtime/mod.rs:440-443`) — explain's subprocess-free replay; splits merged bytes on `\n`, applies the ADR-0010 exit-code branch. Fully tested; the capture form (D-05) is designed to round-trip through it unchanged.
- **`explain.rs` read path** (`crates/lacon-cli/src/commands/explain.rs:106-107`, `:126-129`) — concatenates `stdout ++ stderr`, feeds `filter_bytes`. Confirms D-04 column mapping.
- **E2E harness** — `tracking_e2e.rs:40-64` (`lacon run` under XDG tempdirs), `sc2_privacy_warning_via_cli:240-297` (writes `store_raw_outputs: true` project config), `raw_outputs_empty_by_default:123-143` (off-path negative guard). `bin/test_emitter` for hermetic deterministic output.

### Established Patterns
- **Additive struct extension** — `RunOutcome`/`InvocationMeta` are extended additively, never redefined (`runtime/mod.rs:89` note). New field on `RunOutcome` follows this.
- **`#[derive(Default)]` on options structs** — new `RunOptions` flag defaults `false`, no call-site churn.
- **Double-gating opt-in writes** — config decision in `run.rs`, defensive re-gate in `record.rs`.
- **`raw_buffer` already exists** for the `on_error`/`post_process` paths (`runtime/mod.rs:284-295`) — capture re-serializes it; no new buffer.

### Integration Points
- `crates/lacon-core/src/runtime/mod.rs` — `RunOutcome` (new field, 3 construction sites), `RunOptions` (new flag), capture serialization gated on the flag before `raw_buffer` is moved (`:344/350/358`).
- `crates/lacon-cli/src/commands/run.rs` — set the flag from resolved `store_raw_outputs`; construct `RawOutput`; replace `None` at `:275` with `Some(&raw)`.
- New + existing tests in `crates/lacon-cli/tests/` (`tracking_e2e.rs` / `cli_explain.rs`) and a `runtime/mod.rs` unit test.
</code_context>

<specifics>
## Specific Ideas

- Byte-exact reproduction is the acceptance bar (REQ-acceptance-explain-reproducibility): the filtered column of `lacon explain` on a captured invocation must equal the original `lacon run` stdout byte-for-byte. The capture form `raw_buffer.join("\n")` (D-05) is chosen specifically to make `filter_bytes`'s re-split regenerate the identical lines — this is the load-bearing detail.
</specifics>

<deferred>
## Deferred Ideas

- **Separate real stderr capture** — the v1 runtime merges stderr into stdout (D-11); there is no separable stderr stream at capture time. A future stream-split capture would be a runtime redesign, out of scope.
- **Redaction / `lacon purge` / encryption-at-rest for `raw_outputs`** — already backlog (`docs/backlog.md`); all presuppose capture works, which this phase delivers.

### Reviewed Todos (not folded)
None — no pending todos matched this phase.
</deferred>
