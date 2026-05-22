//! Phase 2 cold-start microbench (Issue #3 Option A), re-targeted in Phase 6 (D-05).
//!
//! Measures `Tracker::open` in two variants:
//!
//! 1. `tracker_open_first_run` — a fresh tempdir DB per iteration, so the
//!    first-ever migration `COMMIT` fsync is included in every sample. On
//!    ext4 this is the dominant ~25ms-per-iteration number (Phase 2
//!    `02-PHASE-BENCH.md`). This variant is **reported only, NOT gated** — it
//!    is a once-per-machine cost the hook hot path never pays after the DB
//!    exists, so failing the build on it would gate the wrong thing.
//!
//! 2. `tracker_open_steady_state` — the DB is created ONCE outside the timed
//!    loop, then re-opened inside the timed loop. This is the real hook hot
//!    path: `lacon run` is invoked thousands of times per session against an
//!    EXISTING DB. `migrate()` early-returns when
//!    `PRAGMA user_version >= TARGET_VERSION`
//!    (`crates/lacon-core/src/tracking/migrations.rs:41-43`), so the
//!    second-and-later open does NO `BEGIN IMMEDIATE`/`COMMIT` — there is no
//!    migration-COMMIT fsync. `prune_if_due`'s 24h throttle likewise skips
//!    after the first run. The hard `assert!(mean < BUDGET_MICROS)` budget
//!    gate lives on THIS variant.
//!
//! D-05 (Phase 6) resolves the deferred Phase 2 regression
//! (`02-PHASE-BENCH.md`: "split first-ever vs steady-state / re-measure on
//! tmpfs") purely as a measurement-protocol change: the steady-state split is
//! a NEW BENCH VARIANT plus a gate re-target, NOT an edit to the
//! `Tracker::open` source path. `Tracker::open` already costs less on the
//! second-and-later call because of the `migrate()` early-return.
//!
//! On steady-state assertion failure the panic propagates → `cargo bench`
//! exits non-zero → CI gates the cold-start contract per ADR-0013.
//!
//! Invocation: `cargo bench -p lacon-core --bench tracker_open`

use std::path::PathBuf;
use std::time::{Duration, Instant};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lacon_core::config::Retention;
use lacon_core::tracking::Tracker;

const FIXED_NOW_MS: u64 = 1_700_000_000_000;
/// Phase 1 baseline (1154µs) + Phase 2 target (2500µs) = 3700µs ceiling.
const BUDGET_MICROS: u128 = 3_700;

fn default_retention() -> Retention {
    Retention {
        invocations_days: 30,
        raw_outputs_days: 3,
    }
}

/// First-ever DB creation cost (fresh tempdir + migration COMMIT fsync per
/// iteration). REPORTED as a diagnostic; NOT gated (D-05) — this is the
/// once-per-machine cost, not the hook hot path. See module docs and
/// `02-PHASE-BENCH.md` for the fsync-at-COMMIT root cause.
fn bench_tracker_open(c: &mut Criterion) {
    let mut total = Duration::ZERO;
    let mut samples: u64 = 0;

    c.bench_function("tracker_open_first_run", |b| {
        b.iter_custom(|iters| {
            let mut elapsed = Duration::ZERO;
            for _ in 0..iters {
                // Fresh tempdir per iteration so first-run migration cost is
                // included. The TempDir RAII drop happens after the timed
                // section — drop time is excluded from the measurement.
                let tmp = tempfile::TempDir::new().unwrap();
                let db_path: PathBuf = tmp.path().join("lacon").join("history.db");

                let start = Instant::now();
                let tracker = Tracker::open(
                    black_box(&db_path),
                    black_box(&default_retention()),
                    false,
                    FIXED_NOW_MS,
                )
                .expect("open ok");
                elapsed += start.elapsed();
                drop(tracker);
                drop(tmp);
            }
            total += elapsed;
            samples += iters;
            elapsed
        });
    });

    // Reported-only (D-05): first-ever DB creation is fsync-dominated and
    // once-per-machine. We print the mean for PHASE-BENCH.md / docs, but do
    // NOT assert against BUDGET_MICROS — the steady-state variant carries the
    // hard gate. criterion's stored median (target/criterion/) is the ground
    // truth for the reported number.
    let mean_micros = if samples > 0 {
        total.as_micros() / samples as u128
    } else {
        0
    };
    eprintln!(
        "tracker_open_first_run mean={mean_micros}µs over {samples} samples \
         (REPORTED, not gated — once-per-machine fsync cost; see D-05)"
    );
}

/// Steady-state `Tracker::open` — the real hook hot path. The DB is created
/// ONCE outside the timed loop (paying the migration cost once), then re-opened
/// inside the timed loop. Because `migrate()` early-returns on an existing DB
/// (`migrations.rs:41-43` — `PRAGMA user_version >= TARGET_VERSION`), the
/// re-open does NO migration `COMMIT` fsync, and `prune_if_due`'s 24h throttle
/// also skips. This is the number the cold-start budget (ADR-0013) gates on.
///
/// See `02-PHASE-BENCH.md` for the deferred Phase 2 regression this resolves
/// and the fsync-at-COMMIT root cause that motivated the first-ever vs
/// steady-state split.
fn bench_tracker_open_steady_state(c: &mut Criterion) {
    // One-time DB creation OUTSIDE the timed loop — migration COMMIT fsync is
    // paid exactly once here, never inside a sample. The tempdir is held in
    // `_tmp` for the whole bench so the timed loop re-opens the SAME db_path.
    let _tmp = tempfile::TempDir::new().unwrap();
    let db_path: PathBuf = _tmp.path().join("lacon").join("history.db");
    drop(
        Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS)
            .expect("first-ever open (DB creation) ok"),
    );

    let mut total = Duration::ZERO;
    let mut samples: u64 = 0;

    c.bench_function("tracker_open_steady_state", |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            for _ in 0..iters {
                // Re-open the EXISTING db_path: migrate() early-returns, so no
                // BEGIN IMMEDIATE / COMMIT / fsync. This is the steady-state
                // hook hot path.
                let tracker = Tracker::open(
                    black_box(&db_path),
                    black_box(&default_retention()),
                    false,
                    FIXED_NOW_MS,
                )
                .expect("steady-state re-open ok");
                drop(tracker);
            }
            let elapsed = start.elapsed();
            total += elapsed;
            samples += iters;
            elapsed
        });
    });

    let mean_micros = if samples > 0 {
        total.as_micros() / samples as u128
    } else {
        0
    };
    eprintln!(
        "tracker_open_steady_state mean={mean_micros}µs over {samples} samples \
         (budget {BUDGET_MICROS}µs)"
    );

    // HARD GATE (D-05): the budget is asserted on the steady-state mean — the
    // real hook hot path. criterion's stored median is the ground truth; the
    // runtime mean is a fail-fast smoke gate. If this exceeds the ceiling,
    // the cold-start contract is violated; do NOT loosen BUDGET_MICROS to
    // make it pass (see ADR-0013, RESEARCH Pitfall 2 / Open Question 2).
    assert!(
        mean_micros < BUDGET_MICROS,
        "Tracker::open steady-state mean {mean_micros}µs exceeds budget \
         {BUDGET_MICROS}µs (1154µs Phase 1 baseline + 2500µs Phase 2 target). \
         Cold-start contract violated; see ADR-0013 and 02-PHASE-BENCH.md."
    );
}

criterion_group!(benches, bench_tracker_open, bench_tracker_open_steady_state);
criterion_main!(benches);
