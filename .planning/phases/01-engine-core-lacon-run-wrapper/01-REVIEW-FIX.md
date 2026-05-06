---
phase: 01-engine-core-lacon-run-wrapper
fixed_at: 2026-05-06T00:00:00Z
review_path: .planning/phases/01-engine-core-lacon-run-wrapper/01-REVIEW.md
iteration: 1
findings_in_scope: 8
fixed: 8
skipped: 0
status: all_fixed
---

# Phase 01: Code Review Fix Report

**Fixed at:** 2026-05-06T00:00:00Z
**Source review:** `.planning/phases/01-engine-core-lacon-run-wrapper/01-REVIEW.md`
**Iteration:** 1

**Summary:**
- Findings in scope: 8 (4 Critical + 4 Warning; INFO excluded per request)
- Fixed: 8
- Skipped: 0

---

## Fixed Issues

### CR-01: AtomicI32 byte counter overflows at ~2 GiB

**Files modified:** `crates/lacon-core/src/runtime/mod.rs`
**Commit:** `095ebb9`
**Applied fix:** Changed `AtomicI32` to `AtomicUsize` in the import, the `Arc::new(...)` constructor, and the `fetch_add` call site. Removed the `as i32` cast on `fetch_add(n)` and the `as usize` cast on `load(...)`. The counter now handles up to ~18 EiB on 64-bit platforms without overflow or sign flip.

---

### CR-02: validate_file dispatch misroutes extend-only rule files

**Files modified:** `crates/lacon-core/src/validate/mod.rs`
**Commit:** `d5c3133`
**Applied fix:** Changed the dispatch condition from `probe.has_id && probe.has_match` to `probe.has_id` alone, with updated doc comments referencing ADR-0012. Added `#[allow(dead_code)]` to `has_match` in `TopLevelProbe` since it may be useful for future diagnostics but is not required for routing. Added a regression test `dispatch_extend_only_rule_routed_to_rule_validator` that verifies a rule with `id` + `extends` but no top-level `match:` does not produce `UnknownKey` errors (which would indicate misrouting to the config validator).

---

### CR-03: CollapseRepeated::flush emits spurious "... 0 lines" summary

**Files modified:** `crates/lacon-core/src/pipeline/stages.rs`
**Commit:** `476c025`
**Applied fix:** Changed the flush condition from `*kept_so_far > 0 || *dropped > 0` to `*dropped > 0`. Added a comment explaining why the old condition was wrong. Added a regression test `collapse_repeated_no_spurious_summary_when_nothing_dropped` that verifies a stream ending exactly at `max_kept` examples with nothing suppressed produces no summary line.

---

### CR-04: MaxBytes truncation marker reports only the first line's bytes

**Files modified:** `crates/lacon-core/src/pipeline/stages.rs`, `crates/lacon-core/src/pipeline/mod.rs`, `crates/lacon-core/src/rules/loader.rs`, `crates/lacon-core/tests/primitives.rs`, `crates/lacon-core/tests/runtime_subprocess.rs`, `tests/fixtures/primitives/max_bytes/expected.txt`
**Commit:** `6591b14` (main fix), `f4a7a83` (golden fixture update)
**Applied fix:** Added `dropped_bytes: usize` field to the `MaxBytes` variant. Changed `step()` to accumulate `dropped_bytes` instead of emitting the marker immediately. Changed `flush()` to emit the marker with the final cumulative `dropped_bytes` count. Updated all `Stage::MaxBytes { ... }` construction sites (loader.rs, pipeline/mod.rs tests, primitives.rs, runtime_subprocess.rs) to include `dropped_bytes: 0`. Updated the golden fixture `expected.txt` from `34 more bytes dropped` (single-line) to `510 more bytes dropped` (15 lines x 34 bytes each). Added `max_bytes_cumulative_drop_count` unit test.

---

### WR-01: KeepTail/KeepHead count/bytes == 0 silently degenerate

**Files modified:** `crates/lacon-core/src/rules/loader.rs`, `crates/lacon-core/tests/rules_loader.rs`, `crates/lacon-core/tests/fixtures/rules/zero_lines_keep_tail.yaml`
**Commit:** `5d8b551`
**Applied fix:** Added a `(Some(0), None) | (None, Some(0))` match arm in `head_tail_mode()` that returns `ValidationError::ParseError` with message `"<stage_name> count/bytes must be > 0"`. Added fixture file `zero_lines_keep_tail.yaml` and two regression tests: `keep_tail_lines_zero_rejected` and `keep_head_lines_zero_rejected`.

---

### WR-02: command_regex re-compiled on every match, silently ignoring errors

**Files modified:** `crates/lacon-cli/src/commands/run.rs`
**Commit:** `631619f`
**Applied fix:** Changed `rule_matches_argv` to return `Result<bool, ValidationError>` instead of `bool`. Renamed the inner `matches` closure to `spec_matches` (avoiding the shadow of `matches!` macro). `Regex::new` errors now produce `ValidationError::InvalidRegex` instead of silent `false`. Updated `try_match_via_load_all` to propagate the error via `return Err(vec![e])`. The regex is still compiled at match time (structural pre-compilation would require more invasive `ResolvedRule` changes), but errors are now surfaced.

---

### WR-03: child.id() as i32 silent truncation for large PIDs

**Files modified:** `crates/lacon-core/src/runtime/mod.rs`
**Commit:** `5f60d81`
**Applied fix:** Replaced `let child_pid = child.id() as i32` with `i32::try_from(child_pid_u32)`. On overflow (PID > i32::MAX), logs a warning via `eprintln!` and uses `-1` as a sentinel. Updated `install_signal_forwarder`'s signal-forwarding loop to guard against `child_pid <= 0`, preventing `kill(-1, sig)` which would broadcast to all processes. Added a comment documenting the Linux PID_MAX_LIMIT = 4,194,304 basis for why the cast is safe in practice.

---

### WR-04: validate_config ignores its content parameter, re-reads from disk

**Files modified:** `crates/lacon-core/src/validate/mod.rs`, `crates/lacon-core/src/config/mod.rs`
**Commit:** `5cc3c73`
**Applied fix:** Added `parse_partial_from_str(content, path, layer)` to `config/mod.rs` that accepts a pre-loaded string. Refactored `parse_partial` to call it after reading from disk. Updated `validate_config` to use `parse_partial_from_str` with the already-loaded `content` parameter (renamed from `_content`). Eliminates the redundant disk read and TOCTOU hazard.

---

## Skipped Issues

None — all 8 in-scope findings were fixed.

---

_Fixed: 2026-05-06T00:00:00Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
