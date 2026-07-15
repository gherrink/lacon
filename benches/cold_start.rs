//! Cold-start probe — measures wall-clock startup of `target/release/lacon`.
//!
//! Establishes the cold-start baseline for the hook hot path against the
//! ≤10 ms budget.
//!
//! Usage:
//!   cargo build --release && cargo run --release --bin cold_start_probe
//!
//! Output: a markdown-table line summarizing per-scenario cold-start timings
//! (min/median/p95/max).
//!
//! Note: This probe is NOT intended to be run in CI by default (T-07-02:
//! 50 × 2 subprocess invocations). It is an operator-level tool for establishing
//! baseline measurements before the Phase 6 acceptance gate.

use std::io::Write;
use std::process::{Command, Stdio};
use std::time::Instant;

const RUNS: usize = 50;
const LACON_BIN: &str = "target/release/lacon";
const HOOK_BIN: &str = "target/release/lacon-claude-hook";

fn measure_one(args: &[&str]) -> std::time::Duration {
    let start = Instant::now();
    let _ = Command::new(LACON_BIN).args(args).output();
    start.elapsed()
}

/// Measure one cold start of the hook binary: spawn it with piped stdin/stdout,
/// write the `PreToolUse` JSON payload, drop stdin, wait for output, and report
/// wall-clock elapsed. Mirrors `measure_one`'s shape so `measure_cold_start_hook`
/// can reuse the warm-up + percentile logic.
fn measure_hook(stdin_json: &str) -> std::time::Duration {
    let start = Instant::now();
    if let Ok(mut child) = Command::new(HOOK_BIN)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(stdin_json.as_bytes());
            // Drop stdin so the hook sees EOF and proceeds.
        }
        let _ = child.wait_with_output();
    }
    start.elapsed()
}

/// Cold-start sampler for the hook binary — parallel to `measure_cold_start`.
fn measure_cold_start_hook(stdin_json: &str) -> Vec<u128> {
    for _ in 0..3 {
        let _ = measure_hook(stdin_json);
    }
    let mut samples = Vec::with_capacity(RUNS);
    for _ in 0..RUNS {
        samples.push(measure_hook(stdin_json).as_micros());
    }
    samples
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

/// Same as `run_scenario`, but drives the hook binary via stdin JSON.
fn run_hook_scenario(name: &str, stdin_json: &str) {
    let samples = measure_cold_start_hook(stdin_json);
    let min = samples.iter().min().copied().unwrap_or(0);
    let max = samples.iter().max().copied().unwrap_or(0);
    let median = percentile(&samples, 0.5);
    let p95 = percentile(&samples, 0.95);
    println!(
        "| `lacon hook {}` | {} µs | {} µs | {} µs | {} µs |",
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

    // Hook hot-path scenarios (Plan 03-04). The hook is invoked thousands of
    // times per session, so its cold start is the load-bearing budget (ADR-0013).
    if std::path::Path::new(HOOK_BIN).exists() {
        // Pass-through: `echo hi` with no rule in cwd → empty stdout, exit 0.
        let passthrough_payload = serde_json::json!({
            "session_id": "bench-session",
            "transcript_path": "/tmp/bench-transcript.jsonl",
            "cwd": "/nonexistent-bench-cwd",
            "permission_mode": "default",
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": { "command": "echo hi" },
            "tool_use_id": "bench-tool-use"
        })
        .to_string();
        run_hook_scenario("passthrough (no rule)", &passthrough_payload);

        // Rewrite: a tempdir with a matching `echo` rule so the hook hits the
        // resolve → rewrite → wrap → emit path.
        let rewrite_dir = tempfile::tempdir().expect("tempdir for rewrite scenario");
        let rules_dir = rewrite_dir.path().join(".lacon").join("rules");
        std::fs::create_dir_all(&rules_dir).expect("create rules dir");
        std::fs::write(
            rules_dir.join("echo.yaml"),
            "id: echo-bench\nmatch: { command: echo }\npipeline:\n  - strip_ansi\n",
        )
        .expect("write bench rule");
        let rewrite_payload = serde_json::json!({
            "session_id": "bench-session",
            "transcript_path": "/tmp/bench-transcript.jsonl",
            "cwd": rewrite_dir.path().to_string_lossy(),
            "permission_mode": "default",
            "hook_event_name": "PreToolUse",
            "tool_name": "Bash",
            "tool_input": { "command": "echo hi" },
            "tool_use_id": "bench-tool-use"
        })
        .to_string();
        run_hook_scenario("rewrite (matched)", &rewrite_payload);
        // tempdir cleaned up on drop.

        println!(
            "\nHook targets: passthrough ≤2000 µs median, rewrite ≤5000 µs median. \
             Real measurements above. Phase 6 owns the formal gate."
        );
    } else {
        eprintln!(
            "WARN: {} not found — skipping hook scenarios. Build with \
             `cargo build --release --bin lacon-claude-hook`.",
            HOOK_BIN
        );
    }

    println!("\nSample count: {} per scenario (+ 3 warm-up runs discarded)", RUNS);
    println!("Budget: <10000 µs (10ms) per the Phase 6 acceptance gate REQ-acceptance-cold-start-budget");
}

fn tempfile_path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir();
    dir.join(format!("lacon-coldstart-{}-{}", std::process::id(), name))
}
