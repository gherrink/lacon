//! End-to-end Starlark host integration tests.
//!
//! Exercises the parse → run loop using on-disk .star fixtures, verifying
//! hermetic mode and the Pipeline::run_with_post_process bridge.
//!
//! Fixture files live under `crates/lacon-core/tests/fixtures/scripts/`.

use lacon_core::error::RuntimeError;
use lacon_core::pipeline::stages::Stage;
use lacon_core::pipeline::Pipeline;
use lacon_core::starlark_host::{ScriptCtx, StarlarkScript};
use regex::Regex;
use std::path::PathBuf;

fn fixture(name: &str) -> PathBuf {
    // Integration tests run with CWD = workspace root when run via `cargo test`.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/scripts").join(name)
}

fn invoke_process(
    fixture_name: &str,
    ctx: ScriptCtx,
    lines: Vec<String>,
) -> Result<Vec<String>, RuntimeError> {
    let path = fixture(fixture_name);
    let content = std::fs::read_to_string(&path).expect("read script fixture");
    let script =
        StarlarkScript::parse(&content, "process".to_owned(), path).expect("parse script");
    script.run(&ctx, lines)
}

#[test]
fn identity_passthrough() {
    let out = invoke_process(
        "identity.star",
        ScriptCtx::default(),
        vec!["a".into(), "b".into(), "c".into()],
    )
    .unwrap();
    assert_eq!(out, vec!["a".to_string(), "b".into(), "c".into()]);
}

#[test]
fn uppercase_transforms_lines() {
    let out = invoke_process(
        "uppercase.star",
        ScriptCtx::default(),
        vec!["abc".into(), "Def".into()],
    )
    .unwrap();
    assert_eq!(out, vec!["ABC".to_string(), "DEF".into()]);
}

#[test]
fn error_filter_uses_ctx_exit_code() {
    let ctx = ScriptCtx {
        exit_code: 1,
        ..Default::default()
    };
    let out = invoke_process(
        "error_filter.star",
        ctx,
        vec![
            "info: starting".into(),
            "error: bad".into(),
            "ok".into(),
            "FAIL".into(),
        ],
    )
    .unwrap();
    // Two matches + one summary line because exit_code != 0.
    assert_eq!(out.len(), 3, "expected 3 output lines, got: {out:?}");
    assert!(out.iter().any(|s| s.contains("error: bad")));
    assert!(out.iter().any(|s| s.contains("FAIL")));
    assert!(out.iter().any(|s| s.starts_with("(2 errors total")));
}

#[test]
fn load_statement_rejected_at_parse_or_eval() {
    let path = fixture("hermetic_violation.star");
    let content = std::fs::read_to_string(&path).expect("read script");
    let parse_result =
        StarlarkScript::parse(&content, "process".into(), path.clone());
    match parse_result {
        Err(_) => {} // parse-time rejection — accepted
        Ok(script) => {
            let run_result = script.run(&ScriptCtx::default(), vec![]);
            assert!(
                run_result.is_err(),
                "load() must be rejected at parse or eval time"
            );
        }
    }
}

#[test]
fn pipeline_run_with_post_process_chains_native_then_starlark() {
    // Native: drop empty lines. post_process: uppercase the remainder.
    let mut pipeline = Pipeline::new(vec![Stage::DropRegex(Regex::new(r"^$").unwrap())]);
    let path = fixture("uppercase.star");
    let content = std::fs::read_to_string(&path).expect("read script");
    let script =
        StarlarkScript::parse(&content, "process".into(), path).expect("parse");

    let lines = vec!["foo".into(), "".into(), "bar".into()];
    let out = pipeline
        .run_with_post_process(lines.into_iter(), Some(&script), &ScriptCtx::default())
        .unwrap();
    assert_eq!(out, vec!["FOO".to_string(), "BAR".into()]);
}

#[test]
fn pipeline_run_with_post_process_none_returns_native_output() {
    let mut pipeline = Pipeline::new(vec![Stage::StripAnsi]);
    let out = pipeline
        .run_with_post_process(
            vec!["plain".to_string()].into_iter(),
            None,
            &ScriptCtx::default(),
        )
        .unwrap();
    assert_eq!(out, vec!["plain".to_string()]);
}
