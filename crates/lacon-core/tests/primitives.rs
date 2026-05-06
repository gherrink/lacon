//! Golden-fixture tests for the 10 native primitives.
//!
//! Per VALIDATION.md "Per-Primitive Unit Test Pattern":
//! - Each primitive has tests/fixtures/primitives/<name>/input.txt + expected.txt
//! - Test reads input, builds a single-stage Pipeline, asserts byte-exact match against expected
//!
//! All fixtures live at the WORKSPACE root (`tests/fixtures/...`), not the
//! crate root, because they are shared with PLAN-05/PLAN-07 integration tests.

use lacon_core::pipeline::stages::{HeadTailMode, Stage};
use lacon_core::pipeline::Pipeline;
use regex::{Regex, RegexSet};
use std::collections::VecDeque;
use std::path::PathBuf;

fn fixture_path(primitive: &str, name: &str) -> PathBuf {
    // Integration tests run with the crate manifest directory as CWD.
    // The fixture tree lives at the workspace root: <workspace>/tests/fixtures/...
    // CARGO_MANIFEST_DIR is crates/lacon-core; workspace root is two levels up.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .join("../..")
        .join("tests/fixtures/primitives")
        .join(primitive)
        .join(name)
}

fn run_fixture(primitive: &str, stages: Vec<Stage>) -> (String, String) {
    let input_path = fixture_path(primitive, "input.txt");
    let expected_path = fixture_path(primitive, "expected.txt");
    let input = std::fs::read_to_string(&input_path)
        .unwrap_or_else(|e| panic!("read {}: {}", input_path.display(), e));
    let expected = std::fs::read_to_string(&expected_path)
        .unwrap_or_else(|e| panic!("read {}: {}", expected_path.display(), e));

    let mut pipeline = Pipeline::new(stages);
    let lines: Vec<String> = input.lines().map(str::to_owned).collect();
    let out = pipeline.run(lines.into_iter());
    let actual = out.join("\n");

    // Trim trailing newline from expected (text editors add one); compare normalised.
    let expected_trimmed = expected.trim_end_matches('\n').to_owned();
    (actual, expected_trimmed)
}

#[test]
fn strip_ansi_fixture() {
    let (actual, expected) = run_fixture("strip_ansi", vec![Stage::StripAnsi]);
    assert_eq!(actual, expected, "strip_ansi: actual vs expected mismatch");
}

#[test]
fn drop_regex_fixture() {
    let re = Regex::new(r"^npm warn deprecated").unwrap();
    let (actual, expected) = run_fixture("drop_regex", vec![Stage::DropRegex(re)]);
    assert_eq!(actual, expected, "drop_regex: actual vs expected mismatch");
}

#[test]
fn keep_regex_fixture() {
    let set = RegexSet::new([r"(error|ERROR|FAIL)"]).unwrap();
    let (actual, expected) = run_fixture("keep_regex", vec![Stage::KeepRegex(set)]);
    assert_eq!(actual, expected, "keep_regex: actual vs expected mismatch");
}

#[test]
fn replace_regex_fixture() {
    let pattern = Regex::new(r"/Users/[^/]+/").unwrap();
    let (actual, expected) = run_fixture(
        "replace_regex",
        vec![Stage::ReplaceRegex {
            pattern,
            replacement: "~/".to_owned(),
        }],
    );
    assert_eq!(actual, expected, "replace_regex: actual vs expected mismatch");
}

#[test]
fn dedupe_fixture() {
    let (actual, expected) = run_fixture(
        "dedupe",
        vec![Stage::Dedupe {
            last: None,
            max_kept: 1,
            repeat_count: 0,
            kept_so_far: 0,
        }],
    );
    assert_eq!(actual, expected, "dedupe: actual vs expected mismatch");
}

#[test]
fn collapse_repeated_fixture() {
    let pattern = Regex::new(r"^Progress: \d+%").unwrap();
    let (actual, expected) = run_fixture(
        "collapse_repeated",
        vec![Stage::CollapseRepeated {
            pattern,
            max_kept: 1,
            summary_template: "… {count} progress lines".to_owned(),
            kept_so_far: 0,
            dropped: 0,
        }],
    );
    assert_eq!(actual, expected, "collapse_repeated: actual vs expected mismatch");
}

#[test]
fn keep_head_fixture() {
    let (actual, expected) = run_fixture(
        "keep_head",
        vec![Stage::KeepHead {
            mode: HeadTailMode::Lines(5),
            lines_remaining: 5,
            bytes_remaining: 0,
        }],
    );
    assert_eq!(actual, expected, "keep_head: actual vs expected mismatch");
}

#[test]
fn keep_tail_fixture() {
    let (actual, expected) = run_fixture(
        "keep_tail",
        vec![Stage::KeepTail {
            mode: HeadTailMode::Lines(5),
            ring: VecDeque::new(),
            byte_count: 0,
        }],
    );
    assert_eq!(actual, expected, "keep_tail: actual vs expected mismatch");
}

#[test]
fn keep_around_match_fixture() {
    let pattern = Regex::new(r"^FAIL ").unwrap();
    let (actual, expected) = run_fixture(
        "keep_around_match",
        vec![Stage::KeepAroundMatch {
            pattern,
            before: 0,
            after: 15,
            ctx_buf: VecDeque::new(),
            emit_after: 0,
        }],
    );
    assert_eq!(actual, expected, "keep_around_match: actual vs expected mismatch");
}

#[test]
fn max_bytes_fixture_truncates_byte_exact() {
    // Cap deliberately small so input definitely overflows.
    let (actual, expected) = run_fixture(
        "max_bytes",
        vec![Stage::MaxBytes {
            cap: 200,
            written: 0,
            truncated: false,
        }],
    );
    assert_eq!(actual, expected, "max_bytes: actual vs expected mismatch");
    // Additional invariant: truncation marker present, byte-exact format.
    assert!(actual.contains("[lacon: truncated, "), "marker must be present in output");
    assert!(
        actual.contains(" more bytes dropped]"),
        "marker suffix must be present in output"
    );
}
