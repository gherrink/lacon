---
phase: 07-close-gap-capture-raw-output-on-opt-in-so-lacon-explain-work
reviewed: 2026-05-22T21:27:41Z
depth: standard
files_reviewed: 4
files_reviewed_list:
  - crates/lacon-core/src/runtime/mod.rs
  - crates/lacon-cli/src/commands/run.rs
  - crates/lacon-cli/src/commands/explain.rs
  - crates/lacon-cli/tests/tracking_e2e.rs
findings:
  critical: 0
  warning: 3
  info: 3
  total: 6
status: issues_found
---

# Phase 7: Code Review Report

**Reviewed:** 2026-05-22T21:27:41Z
**Depth:** standard
**Files Reviewed:** 4
**Status:** issues_found

## Summary

Phase 7 wires opt-in raw-byte capture so `lacon explain` can replay a real
`lacon run` byte-for-byte. The implementation is disciplined: the new
`capture_raw` flag is correctly gated so the join-to-bytes serialization runs
only when opted in (runtime/mod.rs:370-374); all three `RunOutcome`
construction sites set `raw_captured` (main → gated, bypass → `None`,
unmatched → `None`); the privacy double-gate in `Tracker::record` is left
intact and is provably TOCTOU-safe in both flip directions (a config edit
mid-run fails closed, never weakening the privacy gate). No `unwrap`/`expect`
on user input, no new panic surface, no injection surface, no hardcoded
secrets. The integer-cast paths (`exit_code_from_stored`, PID `try_from`)
remain guarded.

The central correctness claim of the phase — "byte-exact round-trip" — holds
for non-empty output but has **one demonstrable divergence on empty
subprocess output** (zero captured lines vs. one empty replayed line). The
E2E test does not cover this case, and the documented contract at
runtime/mod.rs:96-99 over-claims as a result. There is also a cold-start
hot-path regression: config is now resolved redundantly (3+ times per run,
including filesystem `exists()` probes) on a path ADR-0013 explicitly budgets.

No Critical findings. Three Warnings, three Info.

## Narrative Findings (AI reviewer)

## Warnings

### WR-01: Empty-output round-trip diverges — zero captured lines replay as one empty line

**File:** `crates/lacon-core/src/runtime/mod.rs:370-371` (capture) and `crates/lacon-cli/src/commands/explain.rs:185-190` / `crates/lacon-core/src/runtime/mod.rs:482-485` (replay split)

**Issue:** The capture form `raw_buffer.join("\n").into_bytes()` is NOT a clean
inverse of `bytes.split(b'\n')` for the empty case, so the "byte-exact
reproduction" contract documented at runtime/mod.rs:96-99 is violated when the
subprocess produces no output.

Trace:
- Subprocess emits nothing → reader's `read_until` returns `n == 0` on the
  first read → loop breaks → `raw_buffer == []` (zero elements). The live
  success pipeline consumes `[].into_iter()` → **zero lines**.
- Capture: `[].join("\n")` == `""` → `into_bytes()` == `[]`. Stored
  `raw_outputs.stdout` is an empty BLOB.
- Replay: `explain` loads `merged == []`, then `filter_bytes` does
  `[].split(b'\n')` which yields **one** element — the empty string `""`. The
  replay pipeline therefore consumes `[""]` → **one (empty) line**.

So the live pipeline saw 0 lines and the replay sees 1 line. For a passthrough
or any rule that does not drop empty lines, `lacon explain` renders one extra
blank filtered row that the live run never emitted. This is not masked by the
E2E test's `trim_one_trailing_blank` helper (that trims the single trailing
blank from the non-empty case; here the divergence is the existence of a line
at all). The `explain_reproduces_real_run_byte_for_byte` test only exercises
non-empty output (3 stdout + 2 error lines), so the regression is uncaught.

**Fix:** Make the split the exact inverse of the join by special-casing the
empty BLOB in `filter_bytes` (and in `explain::split_lines` for symmetry):

```rust
// runtime/mod.rs filter_bytes(), replacing the unconditional split:
let lines: Vec<String> = if merged_bytes.is_empty() {
    Vec::new() // empty input == zero lines, mirroring the live raw_buffer
} else {
    merged_bytes
        .split(|&b| b == b'\n')
        .map(|l| String::from_utf8_lossy(l).into_owned())
        .collect()
};
```

Apply the same guard to `explain.rs::split_lines` so the rendered raw column
also shows zero rows for an empty capture. Add an E2E (or `filter_bytes` unit)
case driving `test_emitter` with `--stdout-lines 0 --errors 0` and asserting
the explain filtered column is empty.

### WR-02: Config resolved redundantly on the cold-start hot path (ADR-0013 regression)

**File:** `crates/lacon-cli/src/commands/run.rs:77`, `246-248`

**Issue:** Phase 7 moves a full config resolution to *before* the run
(`resolve_store_raw_outputs` → `load_cfg` → `config_paths`) while keeping the
existing post-run resolution in `record_invocation`. The net effect is that
`config_paths` — which performs up to two `Path::exists()` syscalls — now runs
at least **three** times per `lacon run` on the matched path:

1. `run_with_rule:77` → `resolve_store_raw_outputs` → `load_cfg` →
   `config_paths` (2 stats)
2. `record_invocation:247` → `config_paths` directly (2 stats)
3. `record_invocation:248` → `load_cfg` → `config_paths` again (2 stats)

Plus `user_config_dir()` is recomputed independently each time. ADR-0013 and the
file's own header (runtime/mod.rs:5-7) call out `lacon run` as the production
hot path spawned thousands of times per session with a sub-10ms cold-start
budget. The phase context explicitly lists "default-off hot path paying zero
extra cost" as a goal; the *serialization* is correctly zero-cost, but the
*config resolution* is now a measurable per-invocation regression. This is a
correctness-adjacent maintainability defect (the same value is resolved 3x and
the comments assert "never diverge" precisely because the duplication is
fragile).

**Fix:** Resolve the config (and the `store_raw_outputs` decision) once and
thread it through. Resolve in `run_with_rule`, then pass the loaded `Config`
(or the resolved bool plus the already-computed `config_paths`/`user_config_dir`)
into `record_invocation` instead of re-deriving them:

```rust
fn run_with_rule<W: Write>(...) -> anyhow::Result<i32> {
    let cfg = load_cfg(project_path.as_deref()); // resolve ONCE
    let capture_raw = cfg.store_raw_outputs;
    // ...
    record_invocation(..., outcome, cfg); // pass it through, do not re-load
}
```

This both restores the cold-start budget and removes the "must stay in sync"
hazard the current comments are working hard to defend.

### WR-03: `lacon explain` merges stored stderr after stdout, but capture always stores stderr empty — silent contract drift risk

**File:** `crates/lacon-cli/src/commands/explain.rs:106-107` and `crates/lacon-cli/src/commands/run.rs:310-313`

**Issue:** The capture path hardcodes `stderr: Vec::new()` (run.rs:312) because
v1 has a single merged stream by the time raw bytes exist. But `explain`
reconstructs the replay input as `stdout` **then** `stderr`
(`merged.extend_from_slice(&stderr)`, explain.rs:107). These two halves of the
contract live in different crates with no shared invariant. If any future code
path (or a hand-seeded test row like the one in `cli_explain.rs`) ever stores
non-empty `stderr`, `explain` will append it *after* the entire stdout stream —
re-ordering interleaved output and breaking the byte-exact replay, because the
live runner interleaves stdout/stderr in arrival order via the os_pipe merge,
not stdout-then-stderr. The capture side and replay side encode contradictory
assumptions about what the two columns mean.

**Fix:** Make the dependency explicit. Either (a) assert in `explain` that
`stderr.is_empty()` for v1-captured rows and document that non-empty stderr is
a pre-v1/legacy seeding artifact, or (b) add a comment cross-referencing
run.rs:312 at explain.rs:106 stating that v1 capture guarantees empty stderr so
the append order is a no-op. Long-term, drop the separate `stderr` column from
the replay merge entirely for capture-originated rows so the two sides cannot
drift.

## Info

### IN-01: `RunOptions` literal in `run_with_rule` does not use `..Default::default()`

**File:** `crates/lacon-cli/src/commands/run.rs:79-83`

**Issue:** `run_with_rule` constructs `RunOptions` with all fields spelled out
(`project_path`, `extra_env: Default::default()`, `capture_raw`), whereas
`explain.rs:126-132` uses `..Default::default()`. The struct already derives
`Default`. Adding a future field forces a manual edit here and silently breaks
compilation only if the field has no default — inconsistent with the sibling
call site.

**Fix:** Use `RunOptions { project_path, capture_raw, ..Default::default() }`
for consistency and forward-compatibility.

### IN-02: `_user_config_path` computed then discarded

**File:** `crates/lacon-cli/src/commands/run.rs:247`

**Issue:** `config_paths` returns a 2-tuple and `record_invocation` binds the
second element to `_user_config_path` only to drop it, while `load_cfg`
(called on the very next line) recomputes both. The discarded probe is a wasted
`Path::exists()` syscall (compounding WR-02) and signals the function is doing
more work than it needs.

**Fix:** Resolving config once (per WR-02) removes this dead binding entirely.
If kept separate, call a project-only variant rather than computing and
discarding the user path.

### IN-03: Round-trip fidelity invariant is documented in prose but not locked by a test

**File:** `crates/lacon-core/src/runtime/mod.rs:92-99` (the `raw_captured` doc comment)

**Issue:** The comment asserts that `raw_buffer.join("\n")` is "the exact
inverse of the per-line build ... so `Runner::filter_bytes`' re-split
regenerates the identical `Vec<String>`." That round-trip property is only
spot-checked by one happy-path E2E (non-empty, ASCII-only payload). The empty
case (WR-01), invalid-UTF-8 lines, CRLF line endings, and per-line-truncated
lines (`[lacon: line truncated]` suffix) are not asserted anywhere. A unit test
that feeds known `raw_buffer` contents through `join → into_bytes → split →
from_utf8_lossy` and asserts equality would lock the invariant the comment
claims and catch WR-01 directly.

**Fix:** Add a `runtime` unit test parameterized over: empty buffer, single
line, trailing-empty line, an invalid-UTF-8 line, and a CRLF line, asserting
the re-split equals the original `raw_buffer` (with the documented empty-case
fix from WR-01 applied).

---

_Reviewed: 2026-05-22T21:27:41Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
