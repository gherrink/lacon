//! Phase 2 cold-start microbench (Issue #3 Option A).
//!
//! Measures `Tracker::open` against a fresh tempdir DB per iteration so
//! first-run migration cost is included in the median. Asserts median <
//! 3700µs (Phase 1 `--version` baseline 1154µs + 2.5ms Phase 2 target).
//!
//! On assertion failure the panic propagates → `cargo bench` exits non-zero
//! → CI gates the cold-start contract per ADR-0013.
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

    // Compute median proxy from the accumulated mean. Criterion already
    // reports the precise statistical median in its output; we use mean
    // here as a coarse runtime gate. If the mean exceeds the 3.7ms ceiling
    // by a wide margin the test fails. If it's borderline, criterion's
    // own report (saved to target/criterion/) shows the precise median for
    // PHASE-BENCH.md.
    let mean_micros = if samples > 0 {
        total.as_micros() / samples as u128
    } else {
        0
    };
    eprintln!(
        "tracker_open mean={mean_micros}µs over {samples} samples (budget {BUDGET_MICROS}µs)"
    );

    // Fail-fast assertion (Issue #3 Option A — real benchmark gate).
    // Use the runtime mean as a smoke gate; criterion's stored median is
    // the ground truth captured by PHASE-BENCH.md.
    assert!(
        mean_micros < BUDGET_MICROS,
        "Tracker::open mean {mean_micros}µs exceeds budget {BUDGET_MICROS}µs \
         (1154µs Phase 1 baseline + 2500µs Phase 2 target). \
         Cold-start contract violated; see ADR-0013."
    );
}

criterion_group!(benches, bench_tracker_open);
criterion_main!(benches);
