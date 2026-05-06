//! Real-subprocess integration tests for Runner.
//!
//! Tests use `/bin/sh -c '...'` to emit known stdout+stderr patterns and
//! assert the runtime's filtered output, exit code, and byte counts.

use lacon_core::pipeline::Pipeline;
use lacon_core::pipeline::stages::Stage;
use lacon_core::rules::loader::{ResolvedRule, RuleSource};
use lacon_core::rules::schema::RuleFile;
use lacon_core::runtime::{Runner, RunOptions};
use regex::Regex;
use regex::RegexSet;

fn make_rule(success: Pipeline, on_error: Option<Pipeline>) -> ResolvedRule {
    ResolvedRule {
        id: "test-rule".to_owned(),
        source: RuleSource::Project,
        rule: RuleFile {
            id: "test-rule".to_owned(),
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
fn echo_passes_through_with_no_pipeline() {
    let rule = make_rule(Pipeline::new(vec![]), None);
    let mut runner = Runner::new(rule, RunOptions::default());
    let mut buf = Vec::new();
    let outcome = runner
        .run(
            &["/bin/sh".into(), "-c".into(), "echo hello".into()],
            &mut buf,
        )
        .unwrap();
    assert_eq!(outcome.exit_code, 0);
    assert_eq!(String::from_utf8_lossy(&buf).trim(), "hello");
    assert!(!outcome.bypassed);
}

#[test]
fn drop_regex_filters_stderr_into_stdout() {
    // emit "skip me" to stderr and "keep me" to stdout; rule drops "skip me".
    let pipeline = Pipeline::new(vec![Stage::DropRegex(
        Regex::new(r"^skip me").unwrap(),
    )]);
    let rule = make_rule(pipeline, None);
    let mut runner = Runner::new(rule, RunOptions::default());
    let mut buf = Vec::new();
    runner
        .run(
            &[
                "/bin/sh".into(),
                "-c".into(),
                "echo skip me 1>&2; echo keep me".into(),
            ],
            &mut buf,
        )
        .unwrap();
    let out = String::from_utf8_lossy(&buf);
    assert!(out.contains("keep me"), "kept line present: {:?}", out);
    assert!(!out.contains("skip me"), "dropped line absent: {:?}", out);
}

#[test]
fn exit_code_propagated_unchanged() {
    let rule = make_rule(Pipeline::new(vec![]), None);
    let mut runner = Runner::new(rule, RunOptions::default());
    let mut buf = Vec::new();
    let outcome = runner
        .run(
            &["/bin/sh".into(), "-c".into(), "exit 7".into()],
            &mut buf,
        )
        .unwrap();
    assert_eq!(outcome.exit_code, 7);
}

#[test]
fn empty_argv_returns_error() {
    let rule = make_rule(Pipeline::new(vec![]), None);
    let mut runner = Runner::new(rule, RunOptions::default());
    let mut buf = Vec::new();
    let res = runner.run(&[], &mut buf);
    assert!(matches!(
        res,
        Err(lacon_core::error::RuntimeError::EmptyArgv)
    ));
}

#[test]
fn bytes_counted_matches_subprocess_output() {
    let rule = make_rule(Pipeline::new(vec![]), None);
    let mut runner = Runner::new(rule, RunOptions::default());
    let mut buf = Vec::new();
    let outcome = runner
        .run(
            &[
                "/bin/sh".into(),
                "-c".into(),
                "printf 'abc\\ndef\\n'".into(),
            ],
            &mut buf,
        )
        .unwrap();
    // 8 raw bytes (abc\ndef\n).
    assert_eq!(outcome.byte_counts.raw_stdout_bytes, 8);
}

#[test]
fn max_bytes_overflow_emits_byte_exact_truncation_marker() {
    // W3 acceptance test: subprocess emits ~4 KiB; rule has Stage::MaxBytes
    // { cap: 200 }. Per D-08 the byte-exact `[lacon: truncated, N more bytes
    // dropped]` marker MUST appear in the runtime's stdout, AND it must come
    // from the pipeline's MaxBytes stage (NOT from any runtime-level pre-cap,
    // which was REMOVED in revision 1 of this plan).
    let pipeline = Pipeline::new(vec![Stage::MaxBytes {
        cap: 200,
        written: 0,
        truncated: false,
    }]);
    let rule = make_rule(pipeline, None);
    let mut runner = Runner::new(rule, RunOptions::default());
    let mut buf = Vec::new();
    let outcome = runner
        .run(
            &[
                "/bin/sh".into(),
                "-c".into(),
                "for i in $(seq 1 200); do echo aaaaaaaaaaaaaaaaaaaa; done".into(),
            ],
            &mut buf,
        )
        .unwrap();
    let out = String::from_utf8_lossy(&buf);
    assert!(
        out.contains("[lacon: truncated, "),
        "byte-exact truncation marker present: {:?}",
        out
    );
    assert!(
        out.contains(" more bytes dropped]"),
        "byte-exact truncation marker suffix present: {:?}",
        out
    );
    assert!(
        outcome.truncated,
        "RunOutcome.truncated reflects pipeline truncation"
    );
}

#[test]
fn keep_regex_filters_output() {
    // Only "KEEP" lines pass the KeepRegex filter.
    let pipeline = Pipeline::new(vec![Stage::KeepRegex(
        RegexSet::new([r"^KEEP"]).unwrap(),
    )]);
    let rule = make_rule(pipeline, None);
    let mut runner = Runner::new(rule, RunOptions::default());
    let mut buf = Vec::new();
    runner
        .run(
            &[
                "/bin/sh".into(),
                "-c".into(),
                "echo KEEP this; echo drop this; echo KEEP that".into(),
            ],
            &mut buf,
        )
        .unwrap();
    let out = String::from_utf8_lossy(&buf);
    assert!(out.contains("KEEP this"), "KEEP this present: {:?}", out);
    assert!(out.contains("KEEP that"), "KEEP that present: {:?}", out);
    assert!(!out.contains("drop this"), "drop this absent: {:?}", out);
}
