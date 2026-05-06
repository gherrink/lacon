//! YAML rule deserialization structs.
//!
//! Maps every field in `docs/specs/filter-rule-schema.md` to a Rust type.
//!
//! # Design notes
//! - `#[serde(deny_unknown_fields)]` on every struct — unknown keys produce a
//!   serde error that the loader maps to `ValidationError::UnknownKey` (T-03-01).
//! - This module defines the WIRE FORMAT only. PLAN-03's loader converts
//!   `StageSpec` → runtime `Stage` (PLAN-02 type) after deserialization.
//! - `StageSpec` uses serde's default externally-tagged enum repr, which maps
//!   `- strip_ansi` → `StageSpec::StripAnsi` and `- drop_regex: "..."` →
//!   `StageSpec::DropRegex(String)`. Combined with `rename_all = "snake_case"`.
//!
//! # serde-saphyr externally-tagged enum note (PLAN-01 WAVE-0 FINDING)
//! `serde_saphyr::Value` does NOT exist in 0.0.26. This module uses typed
//! `Deserialize` structs only. The dispatch path uses `TopLevelKeyProbe`
//! (see `validate/mod.rs`), NOT a generic Value type.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Top-level rule file structure (wire format).
///
/// A valid rule file must have at least `id` (and either its own or inherited `match` +
/// `pipeline`). The serde representation mirrors `filter-rule-schema.md` exactly.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct RuleFile {
    /// Stable rule identifier (kebab-case convention).
    pub id: String,

    /// Human-readable description shown in `lacon doctor` / `lacon stats`.
    #[serde(default)]
    pub description: Option<String>,

    /// Parent rule ID for inheritance. Flattened at load time by RuleLoader (D-16).
    #[serde(default)]
    pub extends: Option<String>,

    /// Command pattern matcher. Required unless inherited via `extends`.
    #[serde(default, rename = "match")]
    pub match_spec: Option<MatchSpec>,

    /// Bypass condition — if matched, rule is skipped entirely.
    #[serde(default)]
    pub bypass_when: Option<BypassWhen>,

    /// Pre-execution command rewrite (flag add/remove/replace).
    #[serde(default)]
    pub rewrite: Option<RewriteSpec>,

    /// Ordered list of pipeline stages. Required unless inherited.
    #[serde(default)]
    pub pipeline: Option<Vec<StageSpec>>,

    /// Replacement pipeline for non-zero subprocess exit codes (ADR-0010).
    #[serde(default)]
    pub on_error: Option<OnErrorSpec>,

    /// Starlark post-process function run on aggregated post-pipeline output (ADR-0008).
    #[serde(default)]
    pub post_process: Option<ScriptSpec>,
}

/// Command matcher specification.
///
/// All sub-fields are OR'd when combined via `any:` or AND'd via `all:`.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct MatchSpec {
    /// Exact match against argv[0] basename.
    #[serde(default)]
    pub command: Option<String>,

    /// argv[1..N] must start with these tokens.
    #[serde(default)]
    pub args_prefix: Option<Vec<String>>,

    /// argv[1..] must include these tokens (any position).
    #[serde(default)]
    pub args_contain: Option<Vec<String>>,

    /// Regex against the full normalized command line.
    #[serde(default)]
    pub command_regex: Option<String>,

    /// OR semantics — any sub-match suffices.
    #[serde(default)]
    pub any: Option<Vec<MatchSpec>>,

    /// AND semantics — all sub-matches required.
    #[serde(default)]
    pub all: Option<Vec<MatchSpec>>,
}

/// Bypass condition. If this matches, the rule is skipped entirely.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct BypassWhen {
    /// Bypass if any of these flags is present in argv.
    #[serde(default)]
    pub has_flag: Option<Vec<String>>,

    /// Bypass if stdout is a TTY.
    #[serde(default)]
    pub is_tty: Option<bool>,

    /// Bypass if all env var key=value pairs match.
    #[serde(default)]
    pub env: Option<BTreeMap<String, String>>,

    /// OR semantics — any sub-condition suffices.
    #[serde(default)]
    pub any: Option<Vec<BypassWhen>>,
}

/// Pre-execution command rewrite specification.
#[derive(Deserialize, Debug, Clone, Default)]
#[serde(deny_unknown_fields)]
pub struct RewriteSpec {
    /// Flags to add (idempotent — won't add if already present).
    #[serde(default)]
    pub add_flags: Vec<String>,

    /// Flags to remove from argv.
    #[serde(default)]
    pub remove_flags: Vec<String>,

    /// Flag substitution map (old_flag → new_flag).
    #[serde(default)]
    pub replace_flags: BTreeMap<String, String>,
}

/// `on_error` block — completely replaces the success pipeline on non-zero exit (ADR-0010).
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct OnErrorSpec {
    /// Replacement pipeline stages.
    pub pipeline: Vec<StageSpec>,

    /// Optional Starlark post-process for the error path.
    #[serde(default)]
    pub post_process: Option<ScriptSpec>,
}

/// Starlark script reference (used in `post_process` or inline `script:` stage).
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ScriptSpec {
    /// Path to the `.star` file, relative to the rule file's directory.
    /// Absolute paths and paths with `..` components are rejected at load time (T-03-04).
    pub path: PathBuf,

    /// Name of the Starlark function to call (`def <function>(ctx, lines) -> list[str]`).
    pub function: String,
}

/// YAML wire-format pipeline stage enum.
///
/// Maps every native primitive name (snake_case) and the Starlark `script:` stage.
/// Loader converts these to runtime `Stage` variants (PLAN-02's `pipeline::stages::Stage`).
///
/// # serde externally-tagged repr
/// The default serde enum repr is externally-tagged:
/// - Unit variant: `- strip_ansi` → `StageSpec::StripAnsi`
/// - Newtype variant: `- drop_regex: '^npm warn'` → `StageSpec::DropRegex("^npm warn")`
/// - Struct variant: `- replace_regex: { pattern: ..., replacement: ... }` → struct deserialised
///
/// `rename_all = "snake_case"` maps PascalCase variant names to their YAML equivalents.
///
/// # serde-saphyr compatibility note
/// serde-saphyr 0.0.26 uses the same externally-tagged enum deserialization as serde_yaml
/// for the mapping form (`- key: value`). Unit variants (`- strip_ansi`) map to a YAML
/// sequence element containing a single-key mapping with a null/empty value, which serde
/// handles as a unit variant. Confirmed compatible in inline test below.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum StageSpec {
    /// Remove ANSI escape sequences. No args.
    StripAnsi,

    /// Drop any line matching this regex.
    DropRegex(String),

    /// Keep only lines matching this regex (whitelist mode). OR'd with adjacent keep_regex.
    KeepRegex(String),

    /// Substitute matched text in every line.
    ReplaceRegex(ReplaceRegexArgs),

    /// Collapse consecutive duplicate lines.
    Dedupe(Option<DedupeArgs>),

    /// Collapse consecutive runs of matching lines into examples + summary.
    CollapseRepeated(CollapseArgs),

    /// Keep only the first N lines or bytes.
    KeepHead(HeadTailArgs),

    /// Keep only the last N lines or bytes (ring-buffer).
    KeepTail(HeadTailArgs),

    /// Grep -B/-A semantics: keep lines around each match.
    KeepAroundMatch(KeepAroundArgs),

    /// Hard cap on total output bytes. Always the last meaningful stage.
    MaxBytes(usize),

    /// Inline Starlark script stage.
    Script(ScriptSpec),
}

/// Arguments for the `replace_regex` primitive.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct ReplaceRegexArgs {
    pub pattern: String,
    pub replacement: String,
}

/// Optional arguments for the `dedupe` primitive.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct DedupeArgs {
    /// Maximum consecutive duplicate lines to emit (default 1).
    #[serde(default = "default_max_kept_one")]
    pub max_kept: usize,
}

fn default_max_kept_one() -> usize {
    1
}

/// Arguments for the `collapse_repeated` primitive.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct CollapseArgs {
    /// Pattern — lines matching this are collapsed.
    pub pattern: String,
    /// How many example lines to emit before suppressing.
    pub max_kept: usize,
    /// Summary template with `{count}` placeholder.
    pub summary: String,
}

/// Arguments for `keep_head` and `keep_tail` (one of lines/bytes required).
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct HeadTailArgs {
    /// Keep by line count.
    #[serde(default)]
    pub lines: Option<usize>,
    /// Keep by byte count.
    #[serde(default)]
    pub bytes: Option<usize>,
}

/// Arguments for the `keep_around_match` primitive.
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct KeepAroundArgs {
    /// Trigger pattern.
    pub pattern: String,
    /// Context lines before each match.
    pub before: usize,
    /// Context lines after each match.
    pub after: usize,
}

// ───────────────────────────────────────────────────────────────────────────────
// Inline tests
// ───────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip: parse a 5-stage YAML pipeline through `RuleFile` and verify
    /// that all stage variants deserialize to the expected types.
    ///
    /// This is the acceptance test required by Task 1's TDD RED/GREEN cycle.
    #[test]
    fn rule_file_five_stage_round_trip() {
        let yaml = r#"
id: test-rule
description: Five stage round-trip test
match:
  command: cargo
  args_prefix: [build]
pipeline:
  - strip_ansi
  - drop_regex: '^warning:'
  - keep_regex: '(error|FAIL)'
  - max_bytes: 4096
  - replace_regex:
      pattern: '/Users/[^/]+/'
      replacement: '~/'
"#;
        let rule: RuleFile = serde_saphyr::from_str(yaml).expect("five-stage YAML round-trips");
        assert_eq!(rule.id, "test-rule");
        assert!(rule.description.is_some());

        let match_spec = rule.match_spec.as_ref().expect("match_spec present");
        assert_eq!(match_spec.command.as_deref(), Some("cargo"));

        let pipeline = rule.pipeline.as_ref().expect("pipeline present");
        assert_eq!(pipeline.len(), 5, "five stages deserialized");

        // Check variant types
        assert!(matches!(pipeline[0], StageSpec::StripAnsi), "stage 0: strip_ansi");
        assert!(matches!(pipeline[1], StageSpec::DropRegex(_)), "stage 1: drop_regex");
        assert!(matches!(pipeline[2], StageSpec::KeepRegex(_)), "stage 2: keep_regex");
        assert!(matches!(pipeline[3], StageSpec::MaxBytes(4096)), "stage 3: max_bytes");
        assert!(matches!(pipeline[4], StageSpec::ReplaceRegex(_)), "stage 4: replace_regex");
    }

    #[test]
    fn rule_file_with_on_error() {
        let yaml = r#"
id: error-rule
match:
  command: cargo
pipeline:
  - strip_ansi
on_error:
  pipeline:
    - keep_regex: '(error|FAIL)'
    - keep_tail:
        lines: 50
    - max_bytes: 8192
"#;
        let rule: RuleFile = serde_saphyr::from_str(yaml).expect("on_error YAML round-trips");
        let on_error = rule.on_error.as_ref().expect("on_error present");
        assert_eq!(on_error.pipeline.len(), 3);
        assert!(matches!(on_error.pipeline[2], StageSpec::MaxBytes(8192)));
    }

    #[test]
    fn rule_file_with_extends() {
        let yaml = r#"
id: child-rule
extends: parent-rule
pipeline:
  - drop_regex: '^Done'
"#;
        let rule: RuleFile = serde_saphyr::from_str(yaml).expect("extends YAML round-trips");
        assert_eq!(rule.extends.as_deref(), Some("parent-rule"));
    }

    #[test]
    fn rule_file_unknown_field_rejected() {
        let yaml = r#"
id: bad-rule
match:
  command: cargo
pipeline:
  - strip_ansi
unknown_top_level_key: yes
"#;
        let result: Result<RuleFile, _> = serde_saphyr::from_str(yaml);
        assert!(result.is_err(), "unknown top-level field must be rejected");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("unknown") || err_msg.contains("field"),
            "error message should mention unknown field: {err_msg}"
        );
    }

    #[test]
    fn stage_spec_dedupe_with_args() {
        let yaml = r#"
id: dedupe-rule
match:
  command: foo
pipeline:
  - dedupe:
      max_kept: 3
"#;
        let rule: RuleFile = serde_saphyr::from_str(yaml).expect("dedupe with args round-trips");
        let pipeline = rule.pipeline.as_ref().unwrap();
        match &pipeline[0] {
            StageSpec::Dedupe(Some(args)) => assert_eq!(args.max_kept, 3),
            other => panic!("expected Dedupe(Some(DedupeArgs)), got {other:?}"),
        }
    }

    #[test]
    fn stage_spec_collapse_repeated() {
        let yaml = r#"
id: collapse-rule
match:
  command: foo
pipeline:
  - collapse_repeated:
      pattern: '^Progress:'
      max_kept: 1
      summary: '… {count} progress lines'
"#;
        let rule: RuleFile = serde_saphyr::from_str(yaml).expect("collapse_repeated round-trips");
        let pipeline = rule.pipeline.as_ref().unwrap();
        match &pipeline[0] {
            StageSpec::CollapseRepeated(args) => {
                assert_eq!(args.max_kept, 1);
                assert!(args.summary.contains("{count}"));
            }
            other => panic!("expected CollapseRepeated, got {other:?}"),
        }
    }

    #[test]
    fn stage_spec_keep_around_match() {
        let yaml = r#"
id: around-rule
match:
  command: foo
pipeline:
  - keep_around_match:
      pattern: '^FAIL '
      before: 0
      after: 20
"#;
        let rule: RuleFile = serde_saphyr::from_str(yaml).expect("keep_around_match round-trips");
        let pipeline = rule.pipeline.as_ref().unwrap();
        match &pipeline[0] {
            StageSpec::KeepAroundMatch(args) => {
                assert_eq!(args.after, 20);
                assert_eq!(args.before, 0);
            }
            other => panic!("expected KeepAroundMatch, got {other:?}"),
        }
    }

    #[test]
    fn rewrite_spec_defaults() {
        let yaml = r#"
id: rewrite-rule
match:
  command: cargo
rewrite:
  add_flags: ['--quiet']
  remove_flags: ['--verbose']
pipeline:
  - strip_ansi
"#;
        let rule: RuleFile = serde_saphyr::from_str(yaml).expect("rewrite spec round-trips");
        let rewrite = rule.rewrite.as_ref().expect("rewrite present");
        assert_eq!(rewrite.add_flags, vec!["--quiet"]);
        assert_eq!(rewrite.remove_flags, vec!["--verbose"]);
        assert!(rewrite.replace_flags.is_empty());
    }

    #[test]
    fn bypass_when_spec() {
        let yaml = r#"
id: bypass-rule
match:
  command: cargo
bypass_when:
  has_flag: ['--verbose', '-v']
pipeline:
  - strip_ansi
"#;
        let rule: RuleFile = serde_saphyr::from_str(yaml).expect("bypass_when round-trips");
        let bypass = rule.bypass_when.as_ref().expect("bypass_when present");
        assert_eq!(bypass.has_flag.as_ref().unwrap().len(), 2);
    }
}
