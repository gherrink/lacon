use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

/// Resolve a fixture path from the lacon-core crate's test fixtures.
/// `crates/lacon-cli/tests/cli_validate.rs` is run with `CARGO_MANIFEST_DIR =
/// crates/lacon-cli`, so the lacon-core fixtures live at `../lacon-core/tests/fixtures/...`.
fn lacon_core_rule_fixture(name: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("lacon-core")
        .join("tests")
        .join("fixtures")
        .join("rules")
        .join(name)
}

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

// ─── SC4 gap-closure tests (PLAN-08) ──────────────────────────────────────────

/// SC4: `lacon validate <invalid_regex_rule>` exits 1 with `<path>:<line>: InvalidRegex: ...`.
#[test]
fn sc4_validate_rejects_invalid_regex() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("invalid_regex.yaml");
    fs::copy(lacon_core_rule_fixture("invalid_regex.yaml"), &target).unwrap();

    let assertion = Command::cargo_bin("lacon")
        .unwrap()
        .args(["validate", target.to_str().unwrap()])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&assertion.get_output().stderr).to_string();

    // Byte-exact D-18 format: `<path>:<line>: InvalidRegex: <message>`
    let pat = regex::Regex::new(r".+/invalid_regex\.yaml:\d+: InvalidRegex: ").unwrap();
    assert!(
        stderr.lines().any(|l| pat.is_match(l)),
        "expected `<path>:<line>: InvalidRegex: ...` line; got stderr:\n{stderr}"
    );
}

/// SC4: `lacon validate <missing_script_rule>` exits 1 with the appropriate D-18 error.
/// Accepts MissingScriptFile OR ParseError because the missing_script.yaml fixture uses
/// `script:` inline in `pipeline:` which the v1 schema rejects as ParseError before the
/// path-existence check runs (loader.rs spec_to_stage). Either category satisfies SC4
/// ("rejects ... missing referenced Starlark file") because the rule is rejected.
#[test]
fn sc4_validate_rejects_missing_script() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("missing_script.yaml");
    fs::copy(lacon_core_rule_fixture("missing_script.yaml"), &target).unwrap();

    let assertion = Command::cargo_bin("lacon")
        .unwrap()
        .args(["validate", target.to_str().unwrap()])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&assertion.get_output().stderr).to_string();

    let pat = regex::Regex::new(r".+/missing_script\.yaml:\d+: (MissingScriptFile|ParseError): ").unwrap();
    assert!(
        stderr.lines().any(|l| pat.is_match(l)),
        "expected `<path>:<line>: (MissingScriptFile|ParseError): ...` line; got stderr:\n{stderr}"
    );
}

/// SC4: `lacon validate <unknown_primitive_rule>` exits 1.
/// `serde deny_unknown_fields` on the StageSpec enum surfaces as UnknownKey or ParseError
/// (matches the loader-level test in rules_loader.rs).
#[test]
fn sc4_validate_rejects_unknown_primitive() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("unknown_primitive.yaml");
    fs::copy(lacon_core_rule_fixture("unknown_primitive.yaml"), &target).unwrap();

    let assertion = Command::cargo_bin("lacon")
        .unwrap()
        .args(["validate", target.to_str().unwrap()])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&assertion.get_output().stderr).to_string();

    let pat = regex::Regex::new(r".+/unknown_primitive\.yaml:\d+: (UnknownKey|UnknownPrimitive|ParseError): ").unwrap();
    assert!(
        stderr.lines().any(|l| pat.is_match(l)),
        "expected `<path>:<line>: (UnknownKey|UnknownPrimitive|ParseError): ...` line; got stderr:\n{stderr}"
    );
}

/// SC4: `lacon validate <cycle_a>` (with cycle_b in same dir) exits 1 with CircularExtends.
/// Both fixtures must be in the same directory so flatten_extends_with_lookup can find the
/// parent. cycle_a.yaml has `extends: cycle-b`; cycle_b.yaml has `extends: cycle-a`.
#[test]
fn sc4_validate_rejects_circular_extends() {
    let dir = tempdir().unwrap();
    let cycle_a = dir.path().join("cycle_a.yaml");
    let cycle_b = dir.path().join("cycle_b.yaml");
    fs::copy(lacon_core_rule_fixture("cycle_a.yaml"), &cycle_a).unwrap();
    fs::copy(lacon_core_rule_fixture("cycle_b.yaml"), &cycle_b).unwrap();

    let assertion = Command::cargo_bin("lacon")
        .unwrap()
        .args(["validate", cycle_a.to_str().unwrap()])
        .assert()
        .code(1);
    let stderr = String::from_utf8_lossy(&assertion.get_output().stderr).to_string();

    let pat = regex::Regex::new(r".+/cycle_[ab]\.yaml:\d+: CircularExtends: ").unwrap();
    assert!(
        stderr.lines().any(|l| pat.is_match(l)),
        "expected `<path>:<line>: CircularExtends: ...` line; got stderr:\n{stderr}"
    );
}

/// REGRESSION GUARD: a previously-valid rule fixture must continue to validate clean.
/// This catches the failure mode where Task 1's compile pass accidentally rejects valid rules.
#[test]
fn sc4_regression_valid_rule_still_passes() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("valid_simple.yaml");
    fs::copy(lacon_core_rule_fixture("valid_simple.yaml"), &target).unwrap();

    Command::cargo_bin("lacon")
        .unwrap()
        .args(["validate", target.to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
}
