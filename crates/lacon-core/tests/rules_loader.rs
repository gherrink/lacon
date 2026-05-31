//! Integration tests for RuleLoader: resolve, mtime cache, layer fallback, error cases.
//!
//! Tests the lazy hot path (D-14), mtime cache invalidation (D-15), first-match-wins
//! layer walk (ADR-0007), and path traversal rejection (T-03-04).

use std::path::PathBuf;

use lacon_core::error::ValidationError;
use lacon_core::rules::loader::RuleLoader;

/// Helper: path to the fixtures directory from the crate manifest dir.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("rules")
}

/// Write fixture rule files into `<tempdir>/.lacon/rules/`, return the tempdir.
fn setup_project_with_rules(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::TempDir::new().unwrap();
    let rules_dir = tmp.path().join(".lacon").join("rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    for (name, content) in files {
        std::fs::write(rules_dir.join(name), content).unwrap();
    }
    tmp
}

// ─── Test cases ───────────────────────────────────────────────────────────────

#[test]
fn resolve_valid_simple() {
    let content = std::fs::read_to_string(fixtures_dir().join("valid_simple.yaml")).unwrap();
    let tmp = setup_project_with_rules(&[("valid_simple.yaml", &content)]);
    let mut loader = RuleLoader::new(Some(tmp.path().to_owned()));
    let resolved = loader.resolve("valid-simple").expect("valid-simple resolves");
    assert_eq!(resolved.id, "valid-simple");
    // Pipeline has strip_ansi + drop_regex + max_bytes(1024) = 3 stages.
    // Explicit max_bytes present → no implicit injection → still 3.
    assert_eq!(resolved.success_pipeline.stage_count(), 3);
}

#[test]
fn mtime_cache_hit() {
    let content = std::fs::read_to_string(fixtures_dir().join("valid_simple.yaml")).unwrap();
    let tmp = setup_project_with_rules(&[("valid_simple.yaml", &content)]);
    let mut loader = RuleLoader::new(Some(tmp.path().to_owned()));

    // First call: parses from disk.
    let r1 = loader.resolve("valid-simple").expect("first resolve");

    // Second call: should hit cache (no disk parse).
    let r2 = loader.resolve("valid-simple").expect("second resolve (cache hit)");

    assert_eq!(r1.id, r2.id);
    assert_eq!(r1.success_pipeline.stage_count(), r2.success_pipeline.stage_count());
}

#[test]
fn mtime_invalidation() {
    let content = std::fs::read_to_string(fixtures_dir().join("valid_simple.yaml")).unwrap();
    let tmp = setup_project_with_rules(&[("valid_simple.yaml", &content)]);
    let rule_path = tmp.path().join(".lacon").join("rules").join("valid_simple.yaml");
    let mut loader = RuleLoader::new(Some(tmp.path().to_owned()));

    // First resolve.
    let r1 = loader.resolve("valid-simple").expect("first resolve");
    assert_eq!(r1.success_pipeline.stage_count(), 3);

    // Touch the file with modified content (add an extra drop_regex stage).
    let _new_content = format!("{}\n  - drop_regex: '^extra'", content.trim_end());
    // Sleep a tiny bit to guarantee mtime changes (filesystem resolution may be 1s).
    std::thread::sleep(std::time::Duration::from_millis(50));
    // Overwrite with same content + extra stage.
    let updated = r#"id: valid-simple
description: A trivial rule
match:
  command: echo
pipeline:
  - strip_ansi
  - drop_regex: '^npm warn'
  - drop_regex: '^extra'
  - max_bytes: 1024
"#;
    // Force mtime change by writing to file.
    std::fs::write(&rule_path, updated).unwrap();
    // Also touch with a shell command to guarantee mtime changes.
    // (On some filesystems, writing is enough.)

    // Second resolve: should reload.
    let r2 = loader.resolve("valid-simple").expect("second resolve after invalidation");
    // Now has 4 stages.
    assert_eq!(r2.success_pipeline.stage_count(), 4,
        "reloaded rule should have 4 stages after modification");
}

#[test]
fn bundled_layer_fallback() {
    // Project layer has a rule; the same id in bundled should NOT win.
    // (In Phase 1, bundled is empty, so this just tests project wins over an empty bundled.)
    let content = std::fs::read_to_string(fixtures_dir().join("valid_simple.yaml")).unwrap();
    let tmp = setup_project_with_rules(&[("valid_simple.yaml", &content)]);
    let mut loader = RuleLoader::new(Some(tmp.path().to_owned()));
    let resolved = loader.resolve("valid-simple").expect("project rule found");
    assert_eq!(resolved.source, lacon_core::rules::RuleSource::Project);
}

#[test]
fn unknown_rule_id_returns_error() {
    let tmp = setup_project_with_rules(&[]);
    let mut loader = RuleLoader::new(Some(tmp.path().to_owned()));
    let err = loader.resolve("no-such-rule").err().expect("should fail");
    match err {
        ValidationError::ParseError { message, .. } => {
            assert!(
                message.contains("no rule with id"),
                "error should mention missing id, got: {message}"
            );
        }
        other => panic!("expected ParseError, got {other:?}"),
    }
}

#[test]
fn invalid_regex_rejected() {
    let content = std::fs::read_to_string(fixtures_dir().join("invalid_regex.yaml")).unwrap();
    let tmp = setup_project_with_rules(&[("invalid_regex.yaml", &content)]);
    let mut loader = RuleLoader::new(Some(tmp.path().to_owned()));
    let err = loader.resolve("bad-regex").err().expect("invalid regex must fail");
    assert!(
        matches!(err, ValidationError::InvalidRegex { .. }),
        "expected InvalidRegex, got {err:?}"
    );
}

#[test]
fn unknown_primitive_rejected() {
    let content = std::fs::read_to_string(fixtures_dir().join("unknown_primitive.yaml")).unwrap();
    let tmp = setup_project_with_rules(&[("unknown_primitive.yaml", &content)]);
    let mut loader = RuleLoader::new(Some(tmp.path().to_owned()));
    let err = loader.resolve("bad-prim").err().expect("unknown primitive must fail");
    // deny_unknown_fields on StageSpec enum → surfaces as UnknownKey or ParseError.
    assert!(
        matches!(err, ValidationError::UnknownKey { .. } | ValidationError::ParseError { .. }),
        "expected UnknownKey or ParseError for unknown primitive, got {err:?}"
    );
}

#[test]
fn missing_script_rejected() {
    let content = std::fs::read_to_string(fixtures_dir().join("missing_script.yaml")).unwrap();
    let tmp = setup_project_with_rules(&[("missing_script.yaml", &content)]);
    let mut loader = RuleLoader::new(Some(tmp.path().to_owned()));
    let err = loader.resolve("bad-script").err().expect("missing script must fail");
    // Either MissingScriptFile (path validation) or ParseError (v1 inline script unsupported).
    assert!(
        matches!(err, ValidationError::MissingScriptFile { .. } | ValidationError::ParseError { .. }),
        "expected MissingScriptFile or ParseError for missing script, got {err:?}"
    );
}

#[test]
fn path_traversal_rejected() {
    // A rule referencing `script: { path: '../../etc/passwd' }` must be rejected
    // with MissingScriptFile (T-03-04 threat mitigation).
    let rule_yaml = r#"id: traversal-rule
match:
  command: foo
pipeline:
  - script:
      path: ../../etc/passwd
      function: process
"#;
    let tmp = setup_project_with_rules(&[("traversal.yaml", rule_yaml)]);
    let mut loader = RuleLoader::new(Some(tmp.path().to_owned()));
    let err = loader.resolve("traversal-rule").err().expect("path traversal must be rejected");
    assert!(
        matches!(err, ValidationError::MissingScriptFile { .. }),
        "expected MissingScriptFile for path traversal, got {err:?}"
    );
}

#[test]
fn implicit_max_bytes_injected_when_missing() {
    // A rule with no max_bytes stage should get one injected at the end.
    let rule_yaml = r#"id: no-cap
match:
  command: echo
pipeline:
  - strip_ansi
  - drop_regex: '^warn'
"#;
    let tmp = setup_project_with_rules(&[("no_cap.yaml", rule_yaml)]);
    let mut loader = RuleLoader::new(Some(tmp.path().to_owned()));
    loader.defaults_max_bytes = 32768;
    let resolved = loader.resolve("no-cap").expect("no-cap resolves");
    // strip_ansi + drop_regex + implicit MaxBytes = 3
    assert_eq!(resolved.success_pipeline.stage_count(), 3,
        "implicit MaxBytes must be appended");
}

#[test]
fn explicit_max_bytes_not_double_injected() {
    // valid_simple has explicit max_bytes: 1024 → should NOT get an extra MaxBytes.
    let content = std::fs::read_to_string(fixtures_dir().join("valid_simple.yaml")).unwrap();
    let tmp = setup_project_with_rules(&[("valid_simple.yaml", &content)]);
    let mut loader = RuleLoader::new(Some(tmp.path().to_owned()));
    let resolved = loader.resolve("valid-simple").expect("valid-simple resolves");
    // strip_ansi + drop_regex + max_bytes(1024) = 3 (no extra MaxBytes)
    assert_eq!(resolved.success_pipeline.stage_count(), 3,
        "explicit MaxBytes must not be duplicated (Pitfall 7)");
}

#[test]
fn keep_tail_lines_zero_rejected() {
    // WR-01: keep_tail with lines: 0 is a degenerate case — must be rejected at parse time.
    let content = std::fs::read_to_string(
        fixtures_dir().join("zero_lines_keep_tail.yaml")
    ).unwrap();
    let tmp = setup_project_with_rules(&[("zero_lines_keep_tail.yaml", &content)]);
    let mut loader = RuleLoader::new(Some(tmp.path().to_owned()));
    let err = loader.resolve("zero-lines-keep-tail").err()
        .expect("keep_tail lines: 0 must be rejected");
    assert!(
        matches!(err, ValidationError::ParseError { ref message, .. } if message.contains("must be > 0")),
        "expected ParseError with 'must be > 0', got {err:?}"
    );
}

#[test]
fn keep_head_lines_zero_rejected() {
    // WR-01: keep_head with lines: 0 is also degenerate — must be rejected at parse time.
    let rule_yaml = r#"id: zero-head
match:
  command: echo
pipeline:
  - keep_head:
      lines: 0
"#;
    let tmp = setup_project_with_rules(&[("zero_head.yaml", rule_yaml)]);
    let mut loader = RuleLoader::new(Some(tmp.path().to_owned()));
    let err = loader.resolve("zero-head").err()
        .expect("keep_head lines: 0 must be rejected");
    assert!(
        matches!(err, ValidationError::ParseError { ref message, .. } if message.contains("must be > 0")),
        "expected ParseError with 'must be > 0', got {err:?}"
    );
}

#[test]
fn collapse_repeated_without_summary_loads() {
    // CR-01: the spec (filter-rule-schema.md:140) instructs rules to DROP the
    // deprecated `summary` key. `summary` is `#[serde(default)]`, so a
    // `collapse_repeated` stage with NO `summary:` must load successfully.
    let rule_yaml = r#"id: collapse-no-summary
match:
  command: echo
pipeline:
  - collapse_repeated:
      pattern: '^Progress:'
      max_kept: 1
"#;
    let tmp = setup_project_with_rules(&[("collapse_no_summary.yaml", rule_yaml)]);
    let mut loader = RuleLoader::new(Some(tmp.path().to_owned()));
    let resolved = loader
        .resolve("collapse-no-summary")
        .expect("collapse_repeated without summary must load");
    // collapse_repeated + implicit MaxBytes = 2 stages.
    assert_eq!(resolved.success_pipeline.stage_count(), 2);
}
