//! LACON_DISABLE=1 bypass integration tests for Runner.
//!
//! IMPORTANT: This test MUTATES the process env. Tests in this file are
//! serialized via a static Mutex to prevent env bleed between concurrent tests.
//! The file is its own test binary (Cargo per-binary isolation), so it cannot
//! bleed into other test suites.

use std::sync::Mutex;

use lacon_core::pipeline::Pipeline;
use lacon_core::rules::loader::{ResolvedRule, RuleSource};
use lacon_core::rules::schema::RuleFile;
use lacon_core::runtime::{Runner, RunOptions};

/// Mutex to serialize env-mutating tests within this binary.
static ENV_LOCK: Mutex<()> = Mutex::new(());

fn make_passthrough_rule() -> ResolvedRule {
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

#[test]
fn lacon_disable_bypasses_filtering() {
    // Serialize with the env lock so set_var/remove_var don't race with other tests.
    let _lock = ENV_LOCK.lock().unwrap();

    // Set LACON_DISABLE=1 in this test's process env; Runner observes it.
    unsafe {
        std::env::set_var("LACON_DISABLE", "1");
    }

    let mut runner = Runner::new(make_passthrough_rule(), RunOptions::default());
    // sink is unused in bypass mode — child inherits stdout.
    let mut buf = Vec::new();
    let outcome = runner
        .run(
            &["/bin/sh".into(), "-c".into(), "exit 0".into()],
            &mut buf,
        )
        .unwrap();
    assert!(outcome.bypassed, "expected bypassed=true when LACON_DISABLE=1");
    assert_eq!(outcome.exit_code, 0);

    // Cleanup so subsequent tests in this binary aren't polluted.
    unsafe {
        std::env::remove_var("LACON_DISABLE");
    }
}

#[test]
fn lacon_disable_not_set_does_filter() {
    // Without LACON_DISABLE, the pipeline should run normally.
    let _lock = ENV_LOCK.lock().unwrap();
    unsafe {
        std::env::remove_var("LACON_DISABLE");
    }

    let mut runner = Runner::new(make_passthrough_rule(), RunOptions::default());
    let mut buf = Vec::new();
    let outcome = runner
        .run(
            &["/bin/sh".into(), "-c".into(), "echo hello".into()],
            &mut buf,
        )
        .unwrap();
    assert!(!outcome.bypassed, "not bypassed when LACON_DISABLE not set");
    assert_eq!(outcome.exit_code, 0);
}
