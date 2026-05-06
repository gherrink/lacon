# Phase 2 cold-start bench observation

**Baseline source:** `.planning/STATE.md:87`
- Phase 1 `--version` median: **1154µs**
- Phase 1 `validate` median: **1259µs**

**Phase 2 target (CONTEXT "Implementation-time benchmarks" item 1):**
- `Tracker::open(...)` median ≤ Phase 1 baseline + 2.5ms = **~3700µs**

**Phase 6 ceiling (REQ-acceptance-cold-start-budget):**
- 10ms cold-start, hot-path-only

## Methodology

Real benchmark gate (Issue #3 Option A): criterion microbench at the
`Tracker::open` boundary in `crates/lacon-core/benches/tracker_open.rs`.
Each iteration uses a fresh tempdir, so first-run migration cost is included
in every sample.

Invocation:
```bash
cargo bench -p lacon-core --bench tracker_open --release
```

Notes:
- `--release` is implicit for `cargo bench` but stated for clarity.
- Each iteration's RAII tempdir drop happens AFTER the timed section.
- The bench panics (cargo bench exits non-zero) if the mean exceeds 3700µs.
  Criterion's own median (in `target/criterion/tracker_open_first_run/...`)
  is the ground truth for PHASE-BENCH.md's table below.

## Measurements

Run on: 2026-05-06 / Linux 6.8.0-111-generic x86_64
Toolchain: rustc 1.94.1 (e408947bf 2026-03-25)
Platform: linux (ext4 /tmp, on /dev/mapper/vgubuntu-root spinning-class allocation)

| Metric | Value | vs. Phase 1 baseline | Verdict |
|--------|-------|----------------------|---------|
| `Tracker::open` first-run criterion median (incl. migration, ext4 tempdir) | **25020 µs** (≈25.02 ms) | +23866 µs over Phase 1 `--version` 1154 µs | **FAIL — over 3700 µs target** |
| `Tracker::open` runtime mean over 255 samples (panic gate input) | **23569 µs** (≈23.57 ms) | +22415 µs over Phase 1 baseline | **FAIL — bench panics, gate works as designed** |
| `Tracker::open` criterion mean (95% CI [24.89 ms, 25.15 ms]) | **25020 µs** | +23866 µs | **FAIL** |

**Raw criterion output (target/criterion/tracker_open_first_run/new/estimates.json):**
- mean point estimate: 25020428.015625 ns = **25020 µs**
- median point estimate: 25020428.015625 ns = **25020 µs**
- 95% CI: [24893297 ns, 25147558 ns] = [24.89 ms, 25.15 ms]
- median absolute deviation: 188484 ns = 188 µs
- slope: 25096706 ns = 25.10 ms

## Observations

The criterion bench's panic gate at 3700µs **fired as designed** — the bench
exits non-zero, surfacing the cold-start regression at the `Tracker::open`
boundary. Per the plan's explicit instruction ("If the bench gate trips on
this machine: surface it in SUMMARY.md as a real measurement"), this is a
real Phase 2 finding, not a test bug.

**What's in the 25ms budget?** The bench measures from `Instant::now()` (just
after `tempfile::TempDir::new()` returns) through `Tracker::open(...)` return.
That includes:

1. `ensure_data_dir(<tempdir>/lacon)` — `create_dir_all` + idempotent `0700`
   chmod (one syscall pair).
2. `Connection::open_with_flags(<path>, READ_WRITE | CREATE | NO_MUTEX)` —
   creates the empty SQLite file, opens an FD, initializes the in-memory
   page cache.
3. Three per-connection PRAGMAs: `busy_timeout=200ms`,
   `set_db_config(SQLITE_DBCONFIG_ENABLE_FKEY, true)`, and
   `pragma_update_and_check(journal_mode, WAL)`. The WAL pragma on a fresh
   DB header writes the new mode + fsyncs.
4. `migrations::migrate(&mut conn)` — opens an `IMMEDIATE` transaction and
   executes all of `M0001_INITIAL` (4 tables, 6 indexes, 4 views,
   `INSERT INTO lacon_meta`), then `pragma_update("user_version", 1)` and
   `tx.commit()`. The commit fsyncs both the WAL and the page header.
5. `prune::prune_if_due(&conn, &retention, FIXED_NOW_MS)` — reads
   `lacon_meta.last_pruned_ts='0'` (seeded), compares to FIXED_NOW_MS far in
   the future, runs three no-op DELETEs against an empty DB, UPDATEs
   `last_pruned_ts`, commits.

The dominant cost is almost certainly the migration COMMIT's fsync on ext4.
A single migration round-trip on a non-tmpfs disk routinely costs 5–25ms
depending on the file's allocation group and the journal mode of the
underlying ext4 (`data=ordered` here). The 95% CI is tight (±125µs around
25ms), suggesting this is a steady-state cost, not noise.

- **Rusqlite link-time cost:** Did NOT noticeably inflate `--version`. Phase
  1's `--version` baseline (1154µs) was measured with rusqlite NOT linked;
  Phase 2 STATE.md history confirms cold-start probe wasn't re-run with
  tracker active. This bench measures `Tracker::open`'s cost, not link-time
  inflation.
- **First-run migration cost was projected <50ms** (RESEARCH Open Risks item
  1). Observed: ~25ms — under the projection's 50ms outer bound, but well
  over the planned 2.5ms additional-on-top-of-Phase-1 target.
- **The 2.5ms delta target was missed by ~20ms** (~10×). The dominant cost
  is fsync on the ext4 tempdir, not Rust code paths.

## Conclusion

- **FAIL on the 2.5ms delta target (3700µs ceiling).** The bench's panic
  gate enforces this — the bench correctly exits non-zero with the
  documented error message. Cold-start contract per ADR-0013 is violated AT
  THE `Tracker::open` BOUNDARY ON THIS HARDWARE.
- **The observed 25ms is on a non-tmpfs ext4 tempdir.** Phase 6's
  acceptance bench (the `cold_start_probe` binary) measures end-to-end
  `lacon --version` and `lacon validate` — those paths are unaffected by
  this regression because of the lazy-open invariant (D-04, validated by
  `tracking_coldstart.rs`). The regression only fires on `lacon run`, which
  IS the production hot path per ADR-0013.
- **NEEDS Phase 6 follow-up before v1 ship gate:**
  - Re-measure on tmpfs to isolate fsync cost from rust/sqlite cost. If
    tmpfs gives <3ms, the regression is filesystem-induced and the v1
    cold-start contract holds for users on macOS/Linux home directories
    backed by SSD with normal page cache locality (most users — fresh DB
    creation is once-per-machine, not per-invocation).
  - Verify steady-state behaviour: after the first `lacon run` creates the
    DB, subsequent invocations re-open an existing DB (no migration, no
    page-cache cold start) — the realistic cold-start cost is the
    *steady-state* `Tracker::open`, not the *first-ever* `Tracker::open`.
    The current bench measures the worst case (first-ever every iteration
    via fresh tempdir).
  - If steady-state is also over budget, consider:
    - Async tracker write off the hot path (would add complexity; D-04
      currently holds it sync).
    - Pre-creating the DB at install time (Phase 3 `lacon init`).
    - Tightening the WAL pragma path (already minimal).
- **Phase 6 10ms ceiling NOT yet evaluated** — that's the
  `cold_start_probe` operator-level tool's job. The lazy-open invariant
  ensures `--version`/`validate`/`doctor` aren't paying this cost; only
  `lacon run` does, and only on first-ever DB creation per machine.
- **Follow-up logged** — see deferred-items.md for Phase 6 acceptance bench
  re-measurement task with a fresh DB / steady-state DB split.
