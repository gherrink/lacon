//! Branch-fidelity tests for `Runner::filter_bytes` (D-04).
//!
//! `filter_bytes` re-derives filtered output from STORED stdout/stderr bytes
//! WITHOUT spawning a subprocess (never `Runner::run`). It must select the same
//! exit-code branch the live runner uses (runtime/mod.rs:342-359, ADR-0010):
//!   - exit_code == 0          -> success_pipeline (+ post_process)
//!   - exit_code != 0 + on_err -> on_error_pipeline (+ on_error_post_process)
//!   - exit_code != 0 + none   -> raw passthrough (lines unchanged)
//!
//! Rule construction mirrors `runtime_on_error.rs` — a directly-built
//! `ResolvedRule` with hand-assembled `Pipeline`s.

use lacon_core::pipeline::stages::Stage;
use lacon_core::pipeline::Pipeline;
use lacon_core::rules::loader::{ResolvedRule, RuleSource};
use lacon_core::rules::schema::RuleFile;
use lacon_core::runtime::{RunOptions, Runner};
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

/// Case 1 — success path: `exit_code == 0` runs the merged bytes through the
/// success pipeline. A line the success pipeline drops must be absent; a kept
/// line must survive.
#[test]
fn filter_bytes_success_path_runs_success_pipeline() {
    // Success pipeline drops lines starting with "DROP ".
    let success = Pipeline::new(vec![Stage::DropRegex(Regex::new(r"^DROP ").unwrap())]);
    let rule = make_rule(success, None);
    let mut runner = Runner::new(rule, RunOptions::default());

    let bytes = b"keep me\nDROP this\nkeep again\n";
    let out = runner
        .filter_bytes(bytes, 0, 5, "echo hi", None)
        .expect("filter_bytes success path");

    assert!(out.iter().any(|l| l == "keep me"), "kept line present: {out:?}");
    assert!(out.iter().any(|l| l == "keep again"), "second kept line present: {out:?}");
    assert!(
        !out.iter().any(|l| l.contains("DROP this")),
        "dropped line absent: {out:?}"
    );
}

/// Case 2 — on_error path: `exit_code != 0` with an `on_error_pipeline` present
/// runs through the on_error pipeline, NOT the success pipeline. The on_error
/// transform must apply and the success transform must NOT.
#[test]
fn filter_bytes_on_error_path_runs_on_error_pipeline() {
    // Success pipeline: drop everything (would emit nothing).
    let success = Pipeline::new(vec![Stage::DropRegex(Regex::new(r".*").unwrap())]);
    // on_error pipeline: keep only "FAIL " lines.
    let on_error = Pipeline::new(vec![Stage::KeepRegex(RegexSet::new([r"^FAIL "]).unwrap())]);
    let rule = make_rule(success, Some(on_error));
    let mut runner = Runner::new(rule, RunOptions::default());

    let bytes = b"info line\nFAIL bad\nmore info\n";
    let out = runner
        .filter_bytes(bytes, 1, 12, "echo hi", None)
        .expect("filter_bytes on_error path");

    assert!(
        out.iter().any(|l| l == "FAIL bad"),
        "on_error kept FAIL line: {out:?}"
    );
    assert!(
        !out.iter().any(|l| l.contains("info")),
        "on_error dropped info lines (success pipeline would have dropped EVERYTHING): {out:?}"
    );
}

/// Case 3 — no-on_error passthrough: `exit_code != 0` with NO `on_error_pipeline`
/// returns the raw input lines unchanged (ADR-0010 passthrough). Byte-identical.
#[test]
fn filter_bytes_no_on_error_passes_raw_unchanged() {
    // Success pipeline drops everything — must NOT run on the error path.
    let success = Pipeline::new(vec![Stage::DropRegex(Regex::new(r".*").unwrap())]);
    let rule = make_rule(success, None);
    let mut runner = Runner::new(rule, RunOptions::default());

    let bytes = b"raw one\nraw two\nraw three\n";
    let out = runner
        .filter_bytes(bytes, 2, 7, "echo hi", None)
        .expect("filter_bytes no-on_error passthrough");

    // Byte-identical passthrough of the input lines on a non-zero exit.
    // Trailing newline produces a final empty segment (matches the runtime's
    // split-on-b'\n' behaviour); assert the meaningful prefix is unchanged.
    assert_eq!(
        &out[..3],
        &["raw one".to_string(), "raw two".to_string(), "raw three".to_string()],
        "no-on_error returns raw lines unchanged: {out:?}"
    );
}

/// Fidelity assertion: feeding the same bytes/exit through `filter_bytes` yields
/// the same result as applying the success pipeline directly to the same lines.
#[test]
fn filter_bytes_success_matches_direct_pipeline_application() {
    let bytes = b"alpha\nDROP beta\ngamma\n";

    // Direct application of an equivalent success pipeline.
    let mut direct = Pipeline::new(vec![Stage::DropRegex(Regex::new(r"^DROP ").unwrap())]);
    let lines: Vec<String> = bytes
        .split(|&b| b == b'\n')
        .map(|l| String::from_utf8_lossy(l).into_owned())
        .collect();
    let expected = direct.run(lines.into_iter());

    // Via filter_bytes.
    let success = Pipeline::new(vec![Stage::DropRegex(Regex::new(r"^DROP ").unwrap())]);
    let rule = make_rule(success, None);
    let mut runner = Runner::new(rule, RunOptions::default());
    let actual = runner
        .filter_bytes(bytes, 0, 1, "echo hi", None)
        .expect("filter_bytes fidelity");

    assert_eq!(actual, expected, "filter_bytes matches direct success-pipeline output");
}
