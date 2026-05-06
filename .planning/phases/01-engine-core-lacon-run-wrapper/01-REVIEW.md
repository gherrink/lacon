---
phase: 01-engine-core-lacon-run-wrapper
reviewed: 2026-05-06T00:00:00Z
depth: standard
files_reviewed: 23
files_reviewed_list:
  - crates/lacon-core/src/lib.rs
  - crates/lacon-core/src/error.rs
  - crates/lacon-core/src/pipeline/mod.rs
  - crates/lacon-core/src/pipeline/stages.rs
  - crates/lacon-core/src/rules/mod.rs
  - crates/lacon-core/src/rules/schema.rs
  - crates/lacon-core/src/rules/loader.rs
  - crates/lacon-core/src/rules/bundled.rs
  - crates/lacon-core/src/config/mod.rs
  - crates/lacon-core/src/starlark_host/mod.rs
  - crates/lacon-core/src/runtime/mod.rs
  - crates/lacon-core/src/validate/mod.rs
  - crates/lacon-cli/src/main.rs
  - crates/lacon-cli/src/cli.rs
  - crates/lacon-cli/src/commands/mod.rs
  - crates/lacon-cli/src/commands/run.rs
  - crates/lacon-cli/src/commands/validate.rs
  - crates/lacon-cli/src/commands/init.rs
  - crates/lacon-cli/src/commands/stats.rs
  - crates/lacon-cli/src/commands/explain.rs
  - crates/lacon-cli/src/commands/doctor.rs
  - bin/test_emitter/src/main.rs
  - benches/cold_start.rs
findings:
  critical: 4
  warning: 4
  info: 2
  total: 10
status: fixed
fix_applied_at: 2026-05-06T00:00:00Z
findings_status:
  CR-01: fixed
  CR-02: fixed
  CR-03: fixed
  CR-04: fixed
  WR-01: fixed
  WR-02: fixed
  WR-03: fixed
  WR-04: fixed
  IN-01: skipped
  IN-02: skipped
---

# Phase 01: Code Review Report

**Reviewed:** 2026-05-06T00:00:00Z
**Depth:** standard
**Files Reviewed:** 23
**Status:** issues_found

## Summary

This is a greenfield Rust workspace implementing the `lacon` streaming pipeline engine. The overall structure is sound: subprocess argv is passed via `Command::new(prog).args(args)` (no shell re-interpretation), the `read_until` / `from_utf8_lossy` pattern correctly handles non-UTF-8 input, Starlark is hermetically sandboxed via `Globals::standard()` with no file loader registered, and `os_pipe` writer copies are dropped before the reader loop begins. The path-traversal guard for `script:` paths correctly rejects absolute paths and `..` components.

However, four blockers and four warnings were found. The most serious are an integer overflow in the runtime byte counter, a misrouted rule dispatch in `validate_file` for extend-only rules, incorrect `CollapseRepeated` flush semantics that emit spurious output, and a `MaxBytes` truncation marker that under-reports the bytes dropped.

---

## Critical Issues

### CR-01: `AtomicI32` byte counter overflows for output larger than ~2 GiB

**File:** `crates/lacon-core/src/runtime/mod.rs:204`

**Issue:** `raw_byte_counter` is typed `Arc<AtomicI32>` (signed 32-bit integer, max 2,147,483,647). The reader thread calls `fetch_add(n as i32, Ordering::Relaxed)` for every chunk of bytes read. When a subprocess produces more than ~2 GiB of raw output in a single `run()` call, the counter wraps to a large negative number. The final `byte_counts.raw_stdout_bytes` is loaded with `as usize`, converting a negative `i32` to a nonsensical `usize` (e.g. -1 → 18446744073709551615 on 64-bit). This corrupts the `ByteCounts` returned to callers and, in Phase 2, will silently corrupt the `invocations` tracking table.

The per-line `MAX_LINE_BYTES` cap (1 MiB) does not bound total output; a subprocess that emits many large lines can accumulate more than 2 GiB. Note: the v1 spec does not impose a total raw-bytes cap at the runtime level by design (W3 fix comment in the file).

**Fix:**
```rust
// line 204 — change AtomicI32 → AtomicUsize throughout
let raw_byte_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));
let raw_byte_counter_thread = raw_byte_counter.clone();

// line 216 — fetch_add with usize, no cast
raw_byte_counter_thread.fetch_add(n, Ordering::Relaxed);

// line 340 — load is already usize, no cast needed
raw_stdout_bytes: raw_byte_counter.load(Ordering::Relaxed),
```
Also remove the `AtomicI32` import from line 25 and add `AtomicUsize`.

---

### CR-02: `validate_file` dispatch misroutes extend-only rule files

**File:** `crates/lacon-core/src/validate/mod.rs:51`

**Issue:** The dispatch heuristic routes a file to the rule validator only when `has_id && has_match`. However, ADR-0012 explicitly supports rules that omit `match:` and inherit it via `extends:`. A rule file with `id`, `extends`, and `pipeline` but no `match:` key will have `has_id=true, has_match=false`, and will be routed to the **config validator** instead. The config validator will then fail with a spurious `UnknownKey` or `ParseError` for the `id` and `pipeline` keys, misrepresenting a valid rule as a malformed config. This makes `lacon validate` give wrong results on any valid child rule that inherits its match spec.

```yaml
# This valid child rule is misrouted to the config validator:
id: cargo-build-quiet
extends: cargo-build-base
pipeline:
  - strip_ansi
```

**Fix:**
Change the dispatch condition to route any file that has a top-level `id` key to the rule validator, regardless of whether `match:` is present:

```rust
// validate/mod.rs:51
if probe.has_id {
    validate_rule(path, &content)
} else {
    let layer = infer_config_layer(path);
    validate_config(path, &content, layer)
}
```

The `Probe` struct can retain `has_match` for other potential future uses, but the routing decision must not require it.

---

### CR-03: `CollapseRepeated::flush` emits spurious `"… 0 … dropped"` summary

**File:** `crates/lacon-core/src/pipeline/stages.rs:435`

**Issue:** The `flush` method emits a summary line whenever `*kept_so_far > 0 || *dropped > 0`. The `kept_so_far > 0` branch fires whenever example lines were emitted but the stream ended before a non-matching line triggered the in-`step` summary path — even if `dropped == 0`. When `dropped == 0`, the emitted summary is `"… 0 <noun>"`, which is semantically wrong noise in the output: no lines were suppressed, so no summary is warranted.

Concrete example: pipeline `CollapseRepeated { max_kept: 2, pattern: "^P:" }` on input `["P: 1", "P: 2"]` emits `["P: 1", "P: 2", "… 0 …"]` instead of `["P: 1", "P: 2"]`.

The same faulty condition exists in `step` at line 284, but there the non-matching line that triggers it always follows the run, so `kept_so_far` is reset. It is only in `flush` that the `kept_so_far > 0 && dropped == 0` case is reachable.

**Fix:**
```rust
// stages.rs:435 — in flush(), change the condition
Stage::CollapseRepeated {
    summary_template,
    kept_so_far,
    dropped,
    ..
} => {
    // Only emit summary when lines were actually suppressed.
    if *dropped > 0 {
        let summary = summary_template.replace("{count}", &dropped.to_string());
        out.push(Cow::Owned(summary));
        *kept_so_far = 0;
        *dropped = 0;
    }
}
```

---

### CR-04: `MaxBytes` truncation marker reports only the current line's bytes, not total dropped bytes

**File:** `crates/lacon-core/src/pipeline/stages.rs:401`

**Issue:** The D-08 specification requires the truncation marker to report `N more bytes dropped` where `N` is the total bytes dropped — all bytes from the overflowing line onward. The current implementation sets `delta = line_bytes` (the single overflowing line) and never updates it as subsequent lines are dropped. Every call to `step()` after truncation returns immediately (line 387) so `delta` can never be updated. The emitted marker is `"[lacon: truncated, 4 more bytes dropped]"` even if thousands of subsequent lines (potentially megabytes) are dropped.

This means callers using `RunOutcome::truncated` correctly detect that truncation occurred, but the marker's count is always wrong after the first dropped line. The comment on line 400 acknowledges this (`"at minimum the current overflowing line"`) but the spec says "N more bytes dropped" without the hedging qualifier.

**Fix:** Track a mutable `dropped_bytes: usize` field alongside `truncated: bool` in the `MaxBytes` variant, accumulate all subsequent line bytes in `step()`, and emit the marker only in `flush()` (changing `MaxBytes` from a stateless passthrough flush to a flushing stage):

```rust
// In Stage enum, replace MaxBytes variant:
MaxBytes {
    cap: usize,
    written: usize,
    truncated: bool,
    dropped_bytes: usize,   // NEW: accumulates all dropped line bytes
    pending_marker: bool,   // NEW: true once we've crossed the cap
}

// In step(): instead of emitting the marker immediately,
// set pending_marker=true, accumulate dropped_bytes,
// and emit nothing.

// In flush(): if pending_marker, emit the final marker with total dropped_bytes.
```

Alternatively (simpler but still correct), emit the marker immediately with `delta = line_bytes` but also handle the `truncated = true` path in `step()` by accumulating additional bytes into a counter that updates the marker text. The cleanest fix is the deferred-flush approach above.

---

## Warnings

### WR-01: `KeepTail { Lines(0) }` is silently degenerate — always retains one line

**File:** `crates/lacon-core/src/pipeline/stages.rs:330`, `crates/lacon-core/src/rules/loader.rs:601`

**Issue:** The loader documentation at line 130 of `stages.rs` says "PLAN-03 rejects `n == 0` (degenerate)" but `head_tail_mode` in `loader.rs` does not validate against zero. When `n = 0`, the `KeepTail { Lines(0) }` step logic at line 331 evaluates `ring.len() >= 0` which is always true, so `pop_front()` is called, then `push_back` — meaning the ring always holds exactly 1 entry. The flush emits that one entry. A user-supplied rule with `keep_tail: { lines: 0 }` silently behaves like `keep_tail: { lines: 1 }` rather than producing a validation error. The same degenerate case applies to `KeepHead { Lines(0) }` (lines_remaining starts at 0, every line is dropped — actually correct for Lines(0), but still not validated). `KeepTail { Bytes(0) }` correctly drops everything (the while-loop pops all entries, then pushes, then pop occurs in next step) — but this case is not validated either.

**Fix:** In `head_tail_mode`, reject `n == 0`:
```rust
fn head_tail_mode(lines, bytes, stage_name, source_path) -> Result<HeadTailMode, ValidationError> {
    match (lines, bytes) {
        (Some(0), _) | (_, Some(0)) => Err(ValidationError::ParseError {
            path: source_path.to_owned(),
            line: 0,
            message: format!("`{stage_name}` count/bytes must be > 0"),
        }),
        (Some(n), None) => Ok(HeadTailMode::Lines(n)),
        (None, Some(n)) => Ok(HeadTailMode::Bytes(n)),
        // ... existing cases
    }
}
```

---

### WR-02: `command_regex` is compiled on every call in `rule_matches_argv`, silently swallowing invalid-regex errors

**File:** `crates/lacon-cli/src/commands/run.rs:102`

**Issue:** In the no-`--rule` path, `try_match_via_load_all` calls `rule_matches_argv` for every candidate rule. Inside, when `command_regex` is present, `regex::Regex::new(re_str)` is called fresh at match time. On compile failure (`Err(_)`), the rule silently fails to match and the loop continues. This means a rule with an invalid `command_regex` (which should have been caught at load time by `compile_regex`) silently never matches instead of returning an error.

More importantly, this means any user-layer rule that was compiled by `load_all` and passed validation (because `compile_resolved` validates regexes) will still have its `command_regex` re-compiled in `rule_matches_argv` — this is wasted work and means the match logic is inconsistent with the schema (the resolved rule already stores the compiled regex in the `ResolvedRule` struct, but it is not used here).

Additionally, since `load_all` already validates and compiles the regex, the `command_regex` field in `ResolvedRule` should be pre-compiled. But `ResolvedRule` exposes the raw `RuleFile` (with the `String` pattern) rather than a pre-compiled `MatchSpec` type. This is an architecture gap that defers match-time compilation to the CLI layer.

**Fix (immediate):** Propagate the `Err` case from `Regex::new` rather than treating invalid regex as a non-match:
```rust
if let Some(re_str) = &spec.command_regex {
    let re = regex::Regex::new(re_str).map_err(|_| false)?; // or handle error
    if !re.is_match(&joined) {
        return false;
    }
}
```

**Fix (structural):** Pre-compile `MatchSpec` regexes in `compile_resolved` and store the compiled form in `ResolvedRule`, so `rule_matches_argv` can use the already-compiled version.

---

### WR-03: `child.id() as i32` silent truncation for large PIDs

**File:** `crates/lacon-core/src/runtime/mod.rs:196`

**Issue:** `child.id()` returns `u32`. Casting `u32 as i32` silently truncates PIDs above 2,147,483,647. On Linux with PID namespace remapping or systems configured with `kernel.pid_max > 4194304`, PIDs in the upper half of the u32 range produce negative `i32` values. `nix::sys::signal::kill(Pid::from_raw(negative), signal)` either fails with `ESRCH` (signal sent to wrong PID range and the process doesn't exist there) or — more dangerously — sends to a different process that happens to have that PID value when interpreted as signed. The `let _ = kill(...)` at line 441 discards this error, so the signal is silently never delivered.

**Fix:**
```rust
// runtime/mod.rs:196
use nix::unistd::Pid;
let child_pid = Pid::from_raw(child.id() as i32); // cast is safe for PIDs < 2^31
// Linux's PID_MAX_LIMIT is 4194304 (< i32::MAX) by default.
// Document the assumption:
// SAFETY: Linux PID_MAX_LIMIT is 4194304 (2^22) by default; the as i32 cast
// is safe on all supported platforms (macOS, Linux). Validated at startup if needed.
```

The cast is currently safe in practice because Linux's default `PID_MAX_LIMIT` is 4,194,304, but the code should document this assumption explicitly, and `install_signal_forwarder` should accept `u32` and do the cast internally with a debug assertion.

---

### WR-04: `validate_config` re-reads the file from disk, ignoring the already-loaded `content` parameter

**File:** `crates/lacon-core/src/validate/mod.rs:129`

**Issue:** `validate_config(path, _content, layer)` ignores its `_content: &str` parameter (note the leading underscore) and delegates to `parse_partial(path, layer)`, which calls `std::fs::read_to_string(path)` — reading the file a second time. This is an unconditional extra disk I/O on every `lacon validate` call for config files. The file was already read at line 39 in `validate_file`. This could produce inconsistent results (a file could be modified between the two reads), and the underscore prefix on `_content` is a tell that the parameter was designed to avoid the redundancy but was not wired through.

**Fix:** Pass `content` through to a new `parse_partial_from_str` variant, or refactor `parse_partial` to accept either a path or already-loaded content:
```rust
fn validate_config(path: &Path, content: &str, layer: ConfigLayer) -> Vec<ValidationError> {
    // Apply retention precheck directly, then parse from the already-loaded string.
    if matches!(layer, ConfigLayer::Project) {
        if let Err(mut es) = crate::config::retention_precheck(content, path) {
            return es;
        }
    }
    serde_saphyr::from_str::<crate::config::PartialConfig>(content)
        .map(|_| vec![])
        .unwrap_or_else(|e| { /* map to ValidationError */ vec![] })
}
```

---

## Info

### IN-01: `Dedupe` carries a dead `kept_so_far` field that is always ignored

**File:** `crates/lacon-core/src/pipeline/stages.rs:83`

**Issue:** The `Dedupe` variant has four fields: `last`, `max_kept`, `repeat_count`, and `kept_so_far`. The comment on line 82 says `kept_so_far` is "alias for repeat_count tracking (unused — kept for interface compat)". In `step()` at line 251, it is bound to `kept_so_far: _` (explicitly discarded). This dead field wastes 8 bytes per stage instance, creates naming confusion with `CollapseRepeated`'s `kept_so_far` field (which is live and meaningful), and any future maintainer changing `Dedupe` logic might accidentally use or update it.

**Fix:** Remove the `kept_so_far` field from the `Dedupe` variant entirely. Update all construction sites (tests + `spec_to_stage`):
```rust
Dedupe {
    last: Option<String>,
    max_kept: usize,
    repeat_count: usize,
    // kept_so_far removed
}
```

---

### IN-02: `rule_matches_argv` shadows Rust's built-in `matches!` macro

**File:** `crates/lacon-cli/src/commands/run.rs:69`

**Issue:** Inside `rule_matches_argv`, a nested closure named `matches` is defined at line 69. This shadows Rust's standard library `matches!` macro for the duration of that scope. While the macro and the closure have different call syntax (the macro uses `!`, the function doesn't), the shadowing is confusing for readers and a potential footgun if someone adds a `matches!` call inside that scope. Rust will not warn about this — closures can shadow macro names silently.

**Fix:** Rename the inner function to avoid the clash:
```rust
fn spec_matches(spec: &MatchSpec, prog: &str, args: &[String]) -> bool { ... }
```

---

_Reviewed: 2026-05-06T00:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
