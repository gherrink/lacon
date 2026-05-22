//! Fixture-walking integration test for the bundled Tier 1 rules (Phase 5).
//!
//! This is the Wave 0 foundation every rule plan depends on. It is data-driven:
//! it walks `tests/fixtures/<rule-id>/<scenario>/` (workspace-root, sibling to
//! the existing `primitives/` fixtures), and for each scenario directory that
//! contains a `meta.yaml` it replays `input.txt` through the rule's pipeline and
//! runs three assertions.
//!
//! Replay is **subprocess-free byte replay** (D-01), mirroring `lacon explain`
//! (`crates/lacon-cli/src/commands/explain.rs:116-159`):
//!   `RuleLoader::new(None).resolve(id)` → `Runner::new(resolved, default)` →
//!   `runner.filter_bytes(&input, exit_code, 0, &command, None)`.
//!
//! `meta.exit_code` (D-02) selects the ADR-0010 branch: `0` → success pipeline;
//! nonzero with `on_error` present → on_error pipeline; nonzero with none → raw
//! passthrough. Without it failure fixtures would silently run the success
//! pipeline and never exercise `on_error`.
//!
//! Per-fixture assertions (D-05):
//!   1. byte-exact: `out.join("\n")` vs `expected.trim_end_matches('\n')` (D-04 idiom),
//!   2. reduction `len(expected)/len(input) <= 0.5` on non-exempt success fixtures,
//!   3. every `must_keep_lines` substring survives in the joined output.
//!
//! The runner MUST go green on an EMPTY/ABSENT fixture tree so subsequent rule
//! plans turn it green incrementally by dropping fixtures in. Assertions use
//! plain `assert_eq!`/`assert!` (D-09 — snapshot libraries are NOT used).

use lacon_core::rules::loader::RuleLoader;
use lacon_core::runtime::{RunOptions, Runner};
use std::path::{Path, PathBuf};

/// Per-fixture provenance + assertion control read from `meta.yaml`.
///
/// Mirrors the `parse_one` deserialize idiom (`loader.rs:439`) using the
/// workspace YAML parser `serde_saphyr`. `command` and `exit_code` are required;
/// everything else is optional with a sensible default. NO `deny_unknown_fields`
/// — fixtures may carry `captured_at` and future provenance keys the runner
/// deliberately ignores.
#[derive(serde::Deserialize)]
struct FixtureMeta {
    /// Reconstructed `command_raw` passed to `filter_bytes` (populates ScriptCtx).
    command: String,
    /// D-02: selects the ADR-0010 branch. `0` = success pipeline; nonzero =
    /// on_error branch. Record the ACTUAL observed code (cargo build/test
    /// failures are 101, not 1 — RESEARCH A6).
    exit_code: i32,
    #[serde(default)]
    #[allow(dead_code)]
    tool_version: Option<String>,
    /// D-05: skip the ≥50% reduction assertion for already-small fixtures
    /// (e.g. tiny failure-path output where the output IS the signal).
    #[serde(default)]
    exempt_from_reduction_check: bool,
    /// D-05: every substring here must survive filtering (error-survival check).
    #[serde(default)]
    must_keep_lines: Vec<String>,
    #[serde(default)]
    #[allow(dead_code)]
    os: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    notes: Option<String>,
}

/// Workspace-root fixtures directory: `<workspace>/tests/fixtures`.
///
/// `CARGO_MANIFEST_DIR` is `crates/lacon-core`; the workspace root is two levels
/// up. Mirrors the `primitives.rs` idiom but joins `tests/fixtures` (NOT
/// `tests/fixtures/primitives`) so the per-rule subtrees are reachable.
fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("tests/fixtures")
}

/// Subprocess-free byte replay (D-01). Loads the rule from the embedded bundled
/// layer (`None` project_dir → hermetic), then replays `input` through the
/// branch selected by `exit_code`.
fn replay(rule_id: &str, input: &[u8], exit_code: i32, command: &str) -> Vec<String> {
    let mut loader = RuleLoader::new(None);
    let resolved = loader
        .resolve(rule_id)
        .unwrap_or_else(|e| panic!("resolve bundled rule `{rule_id}`: {e}"));
    let mut runner = Runner::new(resolved, RunOptions::default());
    runner
        .filter_bytes(input, exit_code, 0, command, None)
        .unwrap_or_else(|e| panic!("filter_bytes for `{rule_id}`: {e}"))
}

/// Run the three D-05 assertions for a single `<rule-id>/<scenario>` fixture.
/// `slug` is `<rule-id>/<scenario>` and appears in every panic message so a
/// failure is diagnosable to the exact fixture.
fn assert_fixture(rule_id: &str, slug: &str, scenario_dir: &Path, meta: &FixtureMeta) {
    let input_path = scenario_dir.join("input.txt");
    let expected_path = scenario_dir.join("expected.txt");

    let input = std::fs::read(&input_path)
        .unwrap_or_else(|e| panic!("[{slug}] read {}: {e}", input_path.display()));
    let expected_raw = std::fs::read_to_string(&expected_path)
        .unwrap_or_else(|e| panic!("[{slug}] read {}: {e}", expected_path.display()));

    let out = replay(rule_id, &input, meta.exit_code, &meta.command);

    // (1) Byte-exact match — D-04 idiom: a single trailing newline is tolerated
    // on BOTH sides. `filter_bytes` splits the merged bytes on `b'\n'`, so an
    // input ending in `\n` yields a trailing empty element that `join("\n")`
    // turns back into a trailing newline; editors likewise add one to
    // expected.txt. Trimming both sides keeps the comparison contract-faithful
    // without reddening on that cosmetic newline.
    let actual = out.join("\n");
    let actual = actual.trim_end_matches('\n');
    let expected = expected_raw.trim_end_matches('\n');
    assert_eq!(
        actual, expected,
        "[{slug}] byte-exact mismatch: rule output != expected.txt"
    );

    // (2) Reduction threshold — D-05: only on non-exempt fixtures (the primary
    // success fixtures). Measure against the un-trimmed expected/input bytes.
    if !meta.exempt_from_reduction_check {
        let input_len = input.len();
        let expected_len = expected_raw.len();
        assert!(
            input_len > 0,
            "[{slug}] reduction check requires non-empty input.txt \
             (set exempt_from_reduction_check: true for empty/near-empty fixtures)"
        );
        let ratio = expected_len as f64 / input_len as f64;
        assert!(
            ratio <= 0.5,
            "[{slug}] reduction {ratio:.3} exceeds 0.5 \
             (expected {expected_len} bytes / input {input_len} bytes); \
             pick a chattier primary fixture or set exempt_from_reduction_check: true"
        );
    }

    // (3) must_keep_lines — D-05: every listed substring must survive filtering.
    for needle in &meta.must_keep_lines {
        assert!(
            actual.contains(needle.as_str()),
            "[{slug}] must_keep_lines substring did not survive filtering: {needle:?}"
        );
    }
}

/// Read + parse a scenario's `meta.yaml` (mirrors `parse_one`, loader.rs:439).
fn load_meta(slug: &str, meta_path: &Path) -> FixtureMeta {
    let s = std::fs::read_to_string(meta_path)
        .unwrap_or_else(|e| panic!("[{slug}] read {}: {e}", meta_path.display()));
    serde_saphyr::from_str::<FixtureMeta>(&s)
        .unwrap_or_else(|e| panic!("[{slug}] parse {}: {e}", meta_path.display()))
}

/// Discover and assert every fixture under `tests/fixtures/<rule-id>/<scenario>/`.
///
/// Skips the existing `primitives/` subtree (owned by `primitives.rs`). On an
/// absent or empty fixture tree this discovers zero fixtures and passes — that
/// is the intended Wave 0 green state; later waves drop fixtures in and the same
/// runner asserts them.
#[test]
fn all_bundled_rule_fixtures() {
    let root = fixtures_root();
    if !root.is_dir() {
        // Absent tree → nothing to assert. Green by design (Wave 0).
        return;
    }

    let mut fixtures_seen = 0usize;

    let rule_dirs = std::fs::read_dir(&root)
        .unwrap_or_else(|e| panic!("read fixtures root {}: {e}", root.display()));
    for rule_entry in rule_dirs {
        let rule_entry = rule_entry.expect("read fixtures root entry");
        let rule_path = rule_entry.path();
        if !rule_path.is_dir() {
            continue;
        }
        let rule_id = rule_entry.file_name().to_string_lossy().into_owned();
        // `primitives/` is owned by primitives.rs — skip it here.
        if rule_id == "primitives" {
            continue;
        }

        let scenario_dirs = std::fs::read_dir(&rule_path)
            .unwrap_or_else(|e| panic!("read rule dir {}: {e}", rule_path.display()));
        for scenario_entry in scenario_dirs {
            let scenario_entry = scenario_entry.expect("read rule dir entry");
            let scenario_path = scenario_entry.path();
            if !scenario_path.is_dir() {
                continue;
            }
            let meta_path = scenario_path.join("meta.yaml");
            // A scenario dir without meta.yaml is not yet a fixture — skip it
            // (lets in-progress dirs sit on disk without reddening the suite).
            if !meta_path.is_file() {
                continue;
            }

            let scenario_id = scenario_entry.file_name().to_string_lossy().into_owned();
            let slug = format!("{rule_id}/{scenario_id}");
            let meta = load_meta(&slug, &meta_path);
            assert_fixture(&rule_id, &slug, &scenario_path, &meta);
            fixtures_seen += 1;
        }
    }

    // Diagnostic only — zero fixtures is a valid green state in Wave 0.
    eprintln!("bundled_rules: asserted {fixtures_seen} fixture(s)");
}
