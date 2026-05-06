//! Cold-start probe — measures wall-clock startup of `target/release/lacon`.
//!
//! Per CONTEXT.md benchmark item 2 + REQ-acceptance-cold-start-budget (Phase 6).
//!
//! Usage:
//!   cargo build --release && cargo run --release --bin cold_start_probe
//!
//! Output: a markdown-table line ready to paste into docs/architecture.md.
//!
//! Note: This probe is NOT intended to be run in CI by default (T-07-02:
//! 50 × 2 subprocess invocations). It is an operator-level tool for establishing
//! baseline measurements before the Phase 6 acceptance gate.

use std::process::Command;
use std::time::Instant;

const RUNS: usize = 50;
const LACON_BIN: &str = "target/release/lacon";

fn measure_one(args: &[&str]) -> std::time::Duration {
    let start = Instant::now();
    let _ = Command::new(LACON_BIN).args(args).output();
    start.elapsed()
}

fn measure_cold_start(args: &[&str]) -> Vec<u128> {
    // Warm up filesystem cache with 3 discarded runs.
    for _ in 0..3 {
        let _ = measure_one(args);
    }
    let mut samples = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        samples.push(measure_one(args).as_micros());
    }
    samples
}

fn percentile(samples: &[u128], p: f64) -> u128 {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx]
}

fn run_scenario(name: &str, args: &[&str]) {
    let samples = measure_cold_start(args);
    let min = samples.iter().min().copied().unwrap_or(0);
    let max = samples.iter().max().copied().unwrap_or(0);
    let median = percentile(&samples, 0.5);
    let p95 = percentile(&samples, 0.95);
    println!(
        "| `lacon {}` | {} µs | {} µs | {} µs | {} µs |",
        name, min, median, p95, max
    );
}

fn main() {
    // Verify the release binary exists before running.
    if !std::path::Path::new(LACON_BIN).exists() {
        eprintln!("ERROR: {} not found. Run `cargo build --release` first.", LACON_BIN);
        std::process::exit(1);
    }

    println!(
        "# Cold-start measurements ({}, {} samples per scenario)\n",
        std::env::consts::OS,
        RUNS
    );
    println!("| Command | min | median | p95 | max |");
    println!("|---------|-----|--------|-----|-----|");

    run_scenario("--version", &["--version"]);

    // Write a minimal rule file for the `validate` scenario.
    let rule = r#"id: minimal
match: { command: echo }
pipeline: []
"#;
    let tmp = tempfile_path("minimal.yaml");
    std::fs::write(&tmp, rule).unwrap();
    run_scenario(
        "validate <rule>",
        &["validate", tmp.to_str().unwrap()],
    );
    // Clean up temp file.
    let _ = std::fs::remove_file(&tmp);

    println!("\nSample count: {} per scenario (+ 3 warm-up runs discarded)", RUNS);
    println!("Budget: <10000 µs (10ms) per the Phase 6 acceptance gate REQ-acceptance-cold-start-budget");
}

fn tempfile_path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir();
    dir.join(format!("lacon-coldstart-{}-{}", std::process::id(), name))
}
