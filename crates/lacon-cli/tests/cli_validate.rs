use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn validate_valid_rule_file_succeeds() {
    let dir = tempdir().unwrap();
    let rule = dir.path().join("rule.yaml");
    fs::write(
        &rule,
        r#"
id: foo
match: { command: echo }
pipeline:
  - strip_ansi
  - max_bytes: 1024
"#,
    )
    .unwrap();
    Command::cargo_bin("lacon")
        .unwrap()
        .args(["validate", rule.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty());
}

#[test]
fn validate_valid_config_file_succeeds() {
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("config.yaml");
    fs::write(
        &cfg,
        r#"
defaults:
  max_bytes: 16384
store_raw_outputs: false
"#,
    )
    .unwrap();
    Command::cargo_bin("lacon")
        .unwrap()
        .args(["validate", cfg.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn validate_project_config_with_retention_fails_user_only_key() {
    let dir = tempdir().unwrap();
    let lacon_dir = dir.path().join(".lacon");
    fs::create_dir(&lacon_dir).unwrap();
    let cfg = lacon_dir.join("config.yaml");
    fs::write(
        &cfg,
        r#"
retention:
  invocations_days: 7
"#,
    )
    .unwrap();
    Command::cargo_bin("lacon")
        .unwrap()
        .args(["validate", cfg.to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("UserOnlyKeyInProject"))
        .stderr(predicate::str::contains("retention"))
        .stderr(predicate::str::contains("config.yaml:")); // path:line: prefix
}

#[test]
fn validate_unknown_top_level_key_in_rule_fails() {
    let dir = tempdir().unwrap();
    let rule = dir.path().join("rule.yaml");
    fs::write(
        &rule,
        r#"
id: foo
match: { command: echo }
pipeline: []
banana: yes
"#,
    )
    .unwrap();
    Command::cargo_bin("lacon")
        .unwrap()
        .args(["validate", rule.to_str().unwrap()])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("UnknownKey"));
}

#[test]
fn validate_missing_file_errors() {
    Command::cargo_bin("lacon")
        .unwrap()
        .args(["validate", "/nonexistent/path/rule.yaml"])
        .assert()
        .code(1)
        .stderr(predicate::str::contains("file not found"));
}

#[test]
fn validate_dispatch_id_match_routes_to_rule_validator() {
    // A file with `id` and `match` is a rule, even if placed at config-like path.
    let dir = tempdir().unwrap();
    let cfg = dir.path().join("looks_like_config.yaml");
    fs::write(
        &cfg,
        r#"
id: the_rule
match: { command: echo }
pipeline:
  - strip_ansi
"#,
    )
    .unwrap();
    Command::cargo_bin("lacon")
        .unwrap()
        .args(["validate", cfg.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn validate_error_format_is_byte_exact() {
    // Verify the exact error format: `<path>:<line>: <Category>: <message>`.
    let dir = tempdir().unwrap();
    let lacon_dir = dir.path().join(".lacon");
    fs::create_dir(&lacon_dir).unwrap();
    let cfg = lacon_dir.join("config.yaml");
    fs::write(
        &cfg,
        r#"
retention:
  invocations_days: 7
"#,
    )
    .unwrap();
    let assertion = Command::cargo_bin("lacon")
        .unwrap()
        .args(["validate", cfg.to_str().unwrap()])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&assertion.get_output().stderr).to_string();
    // Must match the byte-exact pattern from D-18 / docs/specs/config-schema.md line 103:
    // `<path>:<line>: UserOnlyKeyInProject: <message>`
    let pattern =
        regex::Regex::new(r"^.+/config\.yaml:\d+: UserOnlyKeyInProject: ").unwrap();
    let any_match = stderr.lines().any(|l| pattern.is_match(l));
    assert!(
        any_match,
        "expected byte-exact error format; got:\n{}",
        stderr
    );
}
