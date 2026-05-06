---
phase: 02
status: partial-fixed
depth: standard
critical_count: 0
warning_count: 5
warning_fixed: 4
warning_deferred: 1
info_count: 6
files_reviewed: 22
date: 2026-05-06
fix_applied: 2026-05-06
---

# Phase 2: Local Tracking — Code Review Report

> **Fix status (2026-05-06):**
> - WR-01 — DEFERRED (requires `load_layered` API change; awaiting human decision)
> - WR-02 — FIXED (commit a53b345)
> - WR-03 — FIXED (commit ab2fdd8) + new regression test
> - WR-04 — FIXED (commit 5bc2308)
> - WR-05 — FIXED (commit 1e15395)
> - Tests: 219 passed (218 baseline + 1 new for WR-03 skew)
> - Info findings: not addressed (out of scope for this fix pass).

## Summary

Phase 2 introduces SQLite-backed history persistence with a privacy contract,
schema migrations, retention pruning, and best-effort wire-up at the
`lacon run` boundary. The implementation is well-documented, defensively
constructed, and exercises every locked decision (D-01..D-18) with
integration tests. Adversarial review found **no Critical issues**: all
SQL queries use bound parameters, the lazy-open invariant is sound, the
privacy marker uses the race-free `OpenOptions::create_new(true)` primitive,
the foreign-keys-per-connection pitfall is correctly addressed, and the
best-effort error contract is preserved end-to-end.

The five **Warnings** below are correctness or contract-fidelity concerns
that should be addressed but do not block ship: a privacy-layer attribution
bug when the user layer enables `store_raw_outputs` while a project config
file exists, a debug-build panic risk in WAL pragma verification, a sign
issue in the prune throttle gate under clock-skew, an `expect()` call
guarded by a logical (not type-system) invariant, and a silent NULL on
non-UTF8 project paths. The **Info** items are style / minor lossiness
notes that surface no contract or correctness regressions.

## Critical Findings

None.

## Warnings

### WR-01: Privacy marker layer is misattributed when only the user layer opts in

**File:** `crates/lacon-cli/src/commands/run.rs:353-356`
**Issue:** The heuristic for splitting `cfg.store_raw_outputs` back into
per-layer booleans is unsound. The current implementation:

```rust
let project_store_raw =
    cfg.store_raw_outputs && project_config_path.is_some();
let user_store_raw =
    cfg.store_raw_outputs && !project_store_raw;
```

If the user-layer config sets `store_raw_outputs: true` AND the project also
has a `.lacon/config.yaml` (even one that does NOT mention
`store_raw_outputs` at all), this routes the marker to the project's
`.lacon/.store_raw_outputs_acked` instead of the user-config dir's marker.
This violates D-14 ("project layer wins when both are true; user layer is
the fallback") because attribution is now driven by file existence, not by
which layer actually contained the opt-in. Operationally:

- The user opts in once globally and the warning text instructs them they
  can suppress with `rm <marker-path>`. They `rm`'s the user-dir marker
  but the warning re-fires per-project as soon as any new project's
  `.lacon/config.yaml` (which is unrelated to `store_raw_outputs`) is
  encountered without a project marker.
- Conversely, a `rm` of the project-dir marker re-fires the warning for
  THIS project — even though the project never opted in.

This is an SC2 contract regression. Today's e2e test (`sc2_privacy_warning_via_cli`)
only exercises the project-only case, so this path is unverified.

**Fix:** Plumb per-layer `store_raw_outputs` from `load_layered` instead of
collapsing to one bool. Smallest change: have `load_layered` return both
the merged `Config` and the partial values seen at each layer (or a
`(project_store_raw, user_store_raw)` tuple alongside `Config`). Then drop
the heuristic in `run.rs` entirely:

```rust
// Hypothetical signature
let (cfg, project_store_raw, user_store_raw) =
    config::load_layered_with_layers(...);
// pass project_store_raw, user_store_raw straight to tracker.record(...)
```

If touching `load_layered` is too invasive, a minimum surgical fix is to
re-parse `parse_partial(project_config_path, ...)` and check whether
`store_raw_outputs` is `Some(true)` on the returned `PartialConfig`. The
heuristic on file existence is wrong regardless.

---

### WR-02: `apply_connection_pragmas` `debug_assert_eq!` panics in debug builds if WAL silently degrades

**File:** `crates/lacon-core/src/tracking/mod.rs:144-145`
**Issue:** SQLite's `journal_mode=WAL` pragma can silently fall back to a
different mode (e.g., `delete` or `memory`) on filesystems that don't
support shared memory, certain network mounts, or read-only mounts.
`pragma_update_and_check` returns the *actual* mode SQLite ended up
using — the contract here is to verify it is "wal". The current code:

```rust
let mode: String = conn
    .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
debug_assert_eq!(mode.to_ascii_lowercase(), "wal");
```

In debug builds, this panics if WAL was rejected. In release builds, the
mismatch is silently ignored — the tracker continues with whatever mode
SQLite picked. Either is a problem:

- **Debug:** Cargo tests run on dev machines where `/tmp` may be backed by a
  filesystem that doesn't support WAL. A panic here masquerades as a test
  bug rather than as the legitimate "this filesystem can't host the
  tracker" diagnostic.
- **Release:** Silent degradation to non-WAL mode means concurrent
  `lacon run` from sibling sessions will hit `SQLITE_BUSY` more often
  (D-11's 200ms timeout was sized for WAL contention, not exclusive locks).

**Fix:** Replace the `debug_assert_eq!` with an explicit error return
in production:

```rust
let mode: String = conn
    .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
if mode.to_ascii_lowercase() != "wal" {
    return Err(TrackingError::Sqlite {
        source: rusqlite::Error::ExecuteReturnedResults, // or a new variant
    });
}
```

Or add a new `TrackingError::WalRejected { actual_mode: String }` variant
for clarity. Either way, do not rely on `debug_assert_eq!` for a contract
that affects release behaviour.

---

### WR-03: `prune_if_due` throttle gate under clock-skew lock-out

**File:** `crates/lacon-core/src/tracking/prune.rs:49,63`
**Issue:** The throttle math is:

```rust
let now_i64 = now_ms as i64;
// ...
if now_i64 - last < PRUNE_THROTTLE_MS {
    return Ok(());
}
```

If a sibling `lacon run` (or a manually-edited `lacon_meta` row, or a
machine whose clock was set backwards) leaves `last_pruned_ts` at a value
*greater than* the current `now_i64`, then `now_i64 - last` is negative,
which is less than `PRUNE_THROTTLE_MS` (positive), so prune is skipped.
This is permanent until wall-clock time exceeds the recorded `last`
timestamp — for a clock that was briefly advanced by N hours and then
corrected, prune is locked out for ~N hours after the correction.

In a debug build, `now_i64 - last` could also underflow if `last` is large
(unlikely with realistic ms timestamps, but the cast `as i64` of a `u64`
near `i64::MAX` would produce a negative `now_i64` and arbitrary
subtraction behaviour).

**Fix:** Use saturating subtraction and an explicit "future timestamp"
guard:

```rust
let elapsed = now_i64.saturating_sub(last);
if elapsed < 0 {
    // Clock skew: last_pruned_ts is in the future. Treat as "due now"
    // and reset to recover from the skew.
    // (Or alternatively: return Ok(()) and re-try on the next run.)
}
if elapsed < PRUNE_THROTTLE_MS {
    return Ok(());
}
```

A simpler defensive form:

```rust
if last > now_i64 || (now_i64 - last) < PRUNE_THROTTLE_MS {
    // Skew or throttled — but in the skew case, also rewrite last_pruned_ts
    // to now_ms to re-anchor.
    if last > now_i64 {
        // Re-anchor; do not run DELETEs this invocation.
        let _ = conn.execute(
            "UPDATE lacon_meta SET value = ?1 WHERE key = 'last_pruned_ts'",
            params![now_ms.to_string()],
        );
    }
    return Ok(());
}
```

---

### WR-04: `record.rs` uses `expect()` on a logical invariant the type system does not enforce

**File:** `crates/lacon-core/src/tracking/record.rs:73-76`
**Issue:**

```rust
let want_raw_insert = self.cfg_store_raw_outputs && raw_opt.is_some();
let raw_output_id: Option<i64> = if want_raw_insert {
    let raw = raw_opt.expect("guarded by want_raw_insert");
    Some(self.insert_raw_output(raw, meta.ts_unix_ms)?)
} else {
    None
};
```

The `expect("guarded by want_raw_insert")` is correct today, but it relies
on a *logical* invariant — `want_raw_insert == true ⇒ raw_opt.is_some()` —
that the compiler cannot enforce. A future change to either the gate
expression or the destructuring side could silently break it, and the
panic would surface to end users on the hot `lacon run` path. The
best-effort posture (D-12) catches the panic? No — `panic!` from a library
function propagates as a panic; `record_invocation` in `run.rs` only
matches `Err(TrackingError)`, not panics.

**Fix:** Restructure to avoid the unprovable invariant:

```rust
let raw_output_id: Option<i64> = match (self.cfg_store_raw_outputs, raw_opt) {
    (true, Some(raw)) => Some(self.insert_raw_output(raw, meta.ts_unix_ms)?),
    _ => None,
};
```

This eliminates the `expect()` and makes the structure obvious to readers.

---

### WR-05: `record.rs` silently drops non-UTF8 project paths to NULL

**File:** `crates/lacon-core/src/tracking/record.rs:118-121`
**Issue:** Project paths on Linux can contain arbitrary byte sequences;
they are not guaranteed UTF-8. The current conversion:

```rust
let project_path_str: Option<String> = meta
    .project_path
    .as_ref()
    .and_then(|p| p.to_str().map(|s| s.to_string()));
```

Returns `None` for any path containing a non-UTF8 byte, which is then
inserted as SQL NULL into `invocations.project_path`. That row will then
be missing from `v_project_savings` (which `GROUP BY project_path`).

This is silent data loss on the analytics path. The schema is already
`TEXT NULL` for `project_path`, so a non-UTF8 path being NULL is not a
data corruption — but it's a stealth feature: a user with such a path
will never appear in `lacon stats` and there is no warning.

**Fix:** Use `to_string_lossy()` to preserve as much of the path as
possible, replacing invalid sequences with U+FFFD; or store the raw bytes
as a BLOB column variant; or surface a tracker write warning when the
conversion is lossy:

```rust
let project_path_str: Option<String> = meta.project_path.as_ref()
    .map(|p| p.to_string_lossy().into_owned());
```

The lossy conversion is acceptable for analytics; pure correctness would
require a schema change to BLOB which is out of scope for v1.

## Info

### IN-01: `Tracker.conn` is `pub` (not `pub(crate)`)

**File:** `crates/lacon-core/src/tracking/mod.rs:53-57`
**Issue:** `pub conn: Connection` exposes the rusqlite `Connection` to all
consumers of `lacon-core`, not just the integration test crate. The
documentation explains this is intentional ("integration tests under
`tests/` are external to the crate boundary"), but exposing `Connection`
publicly means any future caller of `lacon-core` could bypass `Tracker`'s
contract (PRAGMA state, transaction discipline) by calling raw SQL.

**Suggested:** Move the integration tests into `lacon-core/src/` as
`#[cfg(test)] mod` so the field can stay `pub(crate)`. Or add a
`pub fn conn(&self) -> &Connection` accessor and keep the field private
— the accessor still exposes the full Connection API but at least
documents the intentional crack.

---

### IN-02: `run.rs:303` silently swallows config validation errors on the run path

**File:** `crates/lacon-cli/src/commands/run.rs:299-303`
**Issue:**

```rust
let cfg: Config = config::load_layered(
    project_config_path.as_deref(),
    user_config_path.as_deref(),
)
.unwrap_or_else(|_| Config::default());
```

If a user introduces a typo in their config (e.g., `retentions` instead of
`retention`), the next `lacon run` will silently use `Config::default()`
without any warning. The contract (D-12) is that tracker failures are
best-effort, but this also masks a *user-config* error from being surfaced
on the run path. The user can `lacon validate` to find it, but there is
no signal at run time.

**Suggested:** Emit a single stderr line ("`lacon: config validation
failed; using defaults — run \`lacon validate\` for details`") on the
error path, even if the actual errors aren't enumerated. Keeps the
best-effort posture but lights up a breadcrumb.

---

### IN-03: `raw_outputs.invocation_id = 0` placeholder

**File:** `crates/lacon-core/src/tracking/record.rs:86-93`
**Issue:** Already documented in code and in the verification report's
anti-patterns section. The `invocation_id INTEGER NOT NULL` column on
`raw_outputs` is filled with literal `0` as a placeholder because the FK
direction is `invocations.raw_output_id → raw_outputs.id`, not the
reverse. v1 keeps this loose; future migrations may bidirectionalize.

**Suggested:** None for v1. The placeholder is documented and
forward-compatible. Consider a v2 migration that adds the reverse FK and
populates `invocation_id` correctly during the dual INSERT (i.e., insert
invocations row first, then raw_outputs with the real id, then UPDATE
invocations with the raw_output_id — a 3-statement pattern).

---

### IN-04: `xdg_db_path` returns `Option<PathBuf>` instead of `Result`

**File:** `crates/lacon-core/src/tracking/mod.rs:120-125`
**Issue:** `etcetera::choose_base_strategy()` returns a `Result` whose
error variant is silently mapped to `None`. The caller in `run.rs:329-332`
prints "could not resolve XDG data dir" without saying what etcetera
actually complained about.

**Suggested:** Return `Result<PathBuf, TrackingError>` with the etcetera
error preserved (add a `TrackingError::XdgResolve` variant or wrap
inside `Sqlite`). Improves diagnostics on the rare platforms where
etcetera fails.

---

### IN-05: Bench panic gate uses runtime-mean as a proxy for criterion-median

**File:** `crates/lacon-core/benches/tracker_open.rs:30-86`
**Issue:** The bench claims the gate fires "at 3700µs" and compares
`mean_micros < BUDGET_MICROS`, but the documented contract is
"median <3700µs". On a long-tail distribution the mean overstates the
typical case; on a bimodal distribution it under- or over-states. Today's
hardware reports 25020µs median (way over 3700µs) and the mean assertion
fires correctly — but the choice of mean-as-proxy is a surface-level
defect.

**Suggested:** Either rename the constant to `BUDGET_MEAN_MICROS` and
update PHASE-BENCH.md to match, or compute a true median by sorting the
per-iteration `elapsed` samples (but criterion's `iter_custom` only
exposes batch-level `elapsed`, not per-sample). Acceptable for v1 as a
smoke gate; flag for Phase 6 re-measurement work.

---

### IN-06: Two unused `pub use` exports widen the public surface

**File:** `crates/lacon-core/src/tracking/mod.rs:25-26`
**Issue:**

```rust
pub use normalize::normalize;
pub use migrations::migrate;
```

`migrate` is exposed at the public crate root (`lacon_core::tracking::migrate`)
even though it's only meaningfully called from inside `Tracker::open`.
Tests reach it via `lacon_core::tracking::migrate` (e.g.
`tracking_schema.rs:13`), but a public surface for migrations seems
unintended — a future refactor that changes the migration signature would
be a breaking change for any external consumer.

**Suggested:** Make `migrate` `pub(crate)` and have integration tests use
a test-only re-export module. Or accept the API surface as documented;
this is style.

## Strengths Worth Calling Out

- **All SQL queries use bound parameters** (`params![...]` or `?N` with
  bind values). No string interpolation. No SQL injection surface.
- **`OpenOptions::create_new(true)` for the privacy marker** — the
  textbook race-free atomic-create primitive. Confirmed at
  `privacy.rs:90-98` with a concurrent-thread smoke test
  (`tracking_privacy.rs:71-93`).
- **`PRAGMA foreign_keys=ON` is set per-connection via
  `set_db_config(SQLITE_DBCONFIG_ENABLE_FKEY, true)`** — the correct
  primitive (the SQL pragma alone has been documented to no-op in some
  rusqlite-bundled configurations). Plus a regression canary
  (`fk_silent_no_op_without_pragma`) that proves the pragma is
  load-bearing if the build flips.
- **Capture-before-move pattern** in `run.rs:163-167` is correct — the
  `resolved.id.clone()` and `resolved.source.clone()` happen before
  `Runner::new(resolved, ...)` consumes the value. Compiles and
  preserves rule identity for the tracker write.
- **Best-effort error handling** in `record_invocation` is consistent
  across every failure mode: `eprintln!("lacon: tracker ...: {e}");
  return;` — the wrapper's exit code is genuinely never altered by
  tracker failure. The `tracking_best_effort` integration tests prove
  this end-to-end with an unwritable XDG path AND with a non-zero
  subprocess exit code.
- **Lazy-open invariant (D-04) is hard-locked by source-grep tests** that
  use `env!("CARGO_MANIFEST_DIR")` rather than fragile relative paths
  (`tracking_coldstart.rs:28-30`). Phase 4 cannot regress this without
  the test failing in CI.
- **Migration is transactional** — `BEGIN IMMEDIATE` + `tx.commit()?`
  pattern (`migrations.rs:45-51`). Auto-rollback on Drop covers the
  failure path.
- **`format_warning` is byte-stable** (D-16) with a per-character test
  (`tracking_privacy.rs:127-140`). Reordering or rewording will fail
  the test.
- **Bench includes a real panic gate** (3700µs) — the Phase 6 cold-start
  budget cannot regress silently; any commit that pushes
  `Tracker::open` over budget trips the gate at CI time.

## Recommendations

1. **Fix WR-01 before Phase 3 lands.** Phase 3 (Claude Code adapter) will
   exercise multi-project workflows where a user's global
   `store_raw_outputs: true` interacts with arbitrary project configs.
   The current heuristic will mis-route markers in production within
   the first week of dogfooding. Suggested approach: have
   `load_layered` return per-layer presence info, not just the merged
   config.
2. **Decide WR-02 explicitly.** Either keep `debug_assert_eq!` and
   document that filesystems without WAL support are unsupported (then
   add an end-to-end test on tmpfs to confirm), or convert to a hard
   error return so production deployments on those filesystems get a
   clear "tracker unavailable" diagnostic.
3. **Address WR-03 with saturating arithmetic** — cheap, defensive, and
   handles a real-world scenario (machine clock corrections after
   misconfiguration).
4. **Refactor WR-04** — small change, removes a nontrivial panic surface
   from the hot path.
5. **Phase 6 should** revisit the bench median proxy (IN-05) along with
   the cold-start re-measurement on tmpfs.
6. **Consider** moving integration tests inside the crate
   (`#[cfg(test)] mod`) to remove the `pub Tracker.conn` exposure
   (IN-01) — makes the API surface honest about what's actually
   public-by-design vs public-for-tests.

---

_Reviewed: 2026-05-06_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
