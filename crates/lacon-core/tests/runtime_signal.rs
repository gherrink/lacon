//! Signal-forwarding integration test. Linux + macOS only.
//!
//! Per VALIDATION.md "Manual-Only Verifications", the formal cross-platform
//! signal-forwarding verification is a manual procedure. This test provides
//! an automated probe gated behind `#[ignore]`.
//!
//! Run explicitly:
//!   cargo test --test runtime_signal -- --include-ignored

#![cfg(unix)]

use lacon_core::pipeline::Pipeline;
use lacon_core::rules::loader::{ResolvedRule, RuleSource};
use lacon_core::rules::schema::RuleFile;
use lacon_core::runtime::{Runner, RunOptions};

fn make_rule() -> ResolvedRule {
    ResolvedRule {
        id: "test".into(),
        source: RuleSource::Project,
        rule: RuleFile {
            id: "test".into(),
            description: None,
            extends: None,
            match_spec: None,
            bypass_when: None,
            rewrite: None,
            pipeline: None,
            on_error: None,
            post_process: None,
        },
        success_pipeline: Pipeline::new(vec![]),
        on_error_pipeline: None,
        post_process: None,
        on_error_post_process: None,
    }
}

/// Verifies that SIGTERM forwarded to the wrapper process is propagated to
/// the subprocess, which then exits with 128 + 15 = 143.
///
/// This test is gated `#[ignore]` because it sends SIGTERM to the test
/// harness's own process, which can interfere with parallel test runners.
/// Run explicitly:
///   cargo test --test runtime_signal -- --include-ignored
#[test]
#[ignore = "interactive — run via `cargo test --test runtime_signal -- --include-ignored`"]
fn sigterm_forwarded_to_child() {
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::Duration;

    // We run Runner::run in a thread; from the main thread we send SIGTERM to
    // our own process after a brief delay. The signal forwarder inside Runner
    // should propagate it to the subprocess (a long-running `sleep 60`), which
    // then terminates with SIGTERM and we observe exit code 128+15=143.

    let result: Arc<Mutex<Option<i32>>> = Arc::new(Mutex::new(None));
    let result_clone = result.clone();

    let handle = thread::spawn(move || {
        let rule = make_rule();
        let mut runner = Runner::new(rule, RunOptions::default());
        let mut buf = Vec::new();
        let outcome = runner
            .run(
                &["/bin/sh".into(), "-c".into(), "sleep 60".into()],
                &mut buf,
            )
            .unwrap();
        *result_clone.lock().unwrap() = Some(outcome.exit_code);
    });

    // Give the subprocess time to start.
    thread::sleep(Duration::from_millis(200));

    // Send SIGTERM to our own process — the signal forwarder should relay it.
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(std::process::id() as i32),
        nix::sys::signal::Signal::SIGTERM,
    )
    .expect("kill self with SIGTERM");

    // Wait for runner to finish (subprocess should terminate quickly).
    let _ = handle.join();

    let exit_code = result.lock().unwrap().expect("runner must have set exit code");
    // 128 + 15 (SIGTERM) = 143
    assert_eq!(
        exit_code, 143,
        "subprocess killed by SIGTERM → exit code 143 (128+15)"
    );
}

/// Smoke test: Runner starts, subprocess completes normally, signal forwarder cleans up.
/// This verifies the forwarder thread lifecycle under non-signal conditions.
#[test]
fn signal_forwarder_does_not_hang_on_normal_exit() {
    let rule = make_rule();
    let mut runner = Runner::new(rule, RunOptions::default());
    let mut buf = Vec::new();
    let outcome = runner
        .run(
            &["/bin/sh".into(), "-c".into(), "echo done; exit 0".into()],
            &mut buf,
        )
        .unwrap();
    assert_eq!(outcome.exit_code, 0, "normal exit propagated");
    let out = String::from_utf8_lossy(&buf);
    assert!(out.contains("done"), "output present: {:?}", out);
}
