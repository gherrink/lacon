//! on_error pipeline swap integration tests for Runner.
//!
//! Verifies that on non-zero exit, the success buffer is discarded
//! and the raw lines are run through the on_error pipeline (ADR-0010, D-13).

use lacon_core::pipeline::Pipeline;
use lacon_core::pipeline::stages::Stage;
use lacon_core::rules::loader::{ResolvedRule, RuleSource};
use lacon_core::rules::schema::RuleFile;
use lacon_core::runtime::{Runner, RunOptions};
use regex::{Regex, RegexSet};

fn make_rule(success: Pipeline, on_error: Option<Pipeline>) -> ResolvedRule {
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
        success_pipeline: success,
        on_error_pipeline: on_error,
        post_process: None,
        on_error_post_process: None,
    }
}

#[test]
fn on_error_swap_runs_on_non_zero_exit() {
    // Success pipeline: keep nothing. on_error pipeline: keep "FAIL" lines.
    let success = Pipeline::new(vec![Stage::DropRegex(Regex::new(r".*").unwrap())]);
    let on_error = Pipeline::new(vec![Stage::KeepRegex(
        RegexSet::new([r"^FAIL "]).unwrap(),
    )]);
    let rule = make_rule(success, Some(on_error));
    let mut runner = Runner::new(rule, RunOptions::default());
    let mut buf = Vec::new();
    let outcome = runner
        .run(
            &[
                "/bin/sh".into(),
                "-c".into(),
                "echo info; echo FAIL bad; exit 1".into(),
            ],
            &mut buf,
        )
        .unwrap();
    let out = String::from_utf8_lossy(&buf);
    assert_eq!(outcome.exit_code, 1);
    assert!(
        out.contains("FAIL bad"),
        "on_error kept FAIL line: {:?}",
        out
    );
    assert!(!out.contains("info"), "info line dropped by on_error: {:?}", out);
}

#[test]
fn success_buffer_discarded_on_non_zero_exit() {
    // Success pipeline: uppercase all lines via replace_regex.
    // on_error pipeline: passthrough (no filters).
    // Subprocess emits "kept" then exits 1.
    //
    // If the SUCCESS pipeline's output were emitted, we'd see "UPPERCASED".
    // If the on_error pipeline's output is emitted, we'd see "kept" (lowercase).
    // This proves the success buffer was discarded.
    let success = Pipeline::new(vec![Stage::ReplaceRegex {
        pattern: Regex::new(r".+").unwrap(),
        replacement: "UPPERCASED".into(),
    }]);
    let on_error = Pipeline::new(vec![]);
    let rule = make_rule(success, Some(on_error));
    let mut runner = Runner::new(rule, RunOptions::default());
    let mut buf = Vec::new();
    runner
        .run(
            &[
                "/bin/sh".into(),
                "-c".into(),
                "echo kept; exit 1".into(),
            ],
            &mut buf,
        )
        .unwrap();
    let out = String::from_utf8_lossy(&buf);
    assert!(
        out.contains("kept"),
        "raw lowercase line via on_error: {:?}",
        out
    );
    assert!(
        !out.contains("UPPERCASED"),
        "success pipeline output discarded: {:?}",
        out
    );
}

#[test]
fn no_on_error_passes_raw_on_nonzero_exit() {
    // Rule has no on_error block. On non-zero exit, raw output should pass through.
    let success = Pipeline::new(vec![Stage::DropRegex(Regex::new(r".*").unwrap())]);
    let rule = make_rule(success, None);
    let mut runner = Runner::new(rule, RunOptions::default());
    let mut buf = Vec::new();
    let outcome = runner
        .run(
            &[
                "/bin/sh".into(),
                "-c".into(),
                "echo raw_line; exit 2".into(),
            ],
            &mut buf,
        )
        .unwrap();
    let out = String::from_utf8_lossy(&buf);
    assert_eq!(outcome.exit_code, 2);
    assert!(
        out.contains("raw_line"),
        "raw output passes through when no on_error: {:?}",
        out
    );
}
