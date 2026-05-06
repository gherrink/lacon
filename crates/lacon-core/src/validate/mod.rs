//! `lacon validate` content-dispatched entry point.
//!
//! `validate_file(path)` reads a YAML file, introspects its top-level keys, and
//! dispatches to the rule validator or config validator (D-17):
//! - Top-level `id` AND `match` both present → rule file → rule validator.
//! - Anything else → config file → config validator.
//!
//! Returns an empty `Vec<ValidationError>` on success (no errors found).
//!
//! # WAVE-0 FINDING applied here
//! `serde_saphyr::Value` does NOT exist in 0.0.26. Dispatch uses the
//! `TopLevelKeyProbe` pattern (validated in `wave0_smoke.rs`): a partial struct
//! with `Option<serde::de::IgnoredAny>` fields for `id` and `match`.
//!
//! # Layer hint heuristic (D-17)
//! For standalone `validate_file` calls, the layer is inferred from the path:
//! - Path contains `.lacon/` component → project layer.
//! - Otherwise → user layer (assumed for paths not under `.lacon/`).
//!
//! PLAN-06 (CLI wiring) can override this heuristic via a `--layer` flag if needed.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use crate::config::{parse_partial, ConfigLayer};
use crate::error::ValidationError;
use crate::rules::loader::parse_one;

/// Content-dispatched validation entry point. Returns empty `Vec` on success.
///
/// Dispatch logic (D-17):
/// - Top-level `id` AND `match` → validate as rule file.
/// - Otherwise → validate as config file.
///
/// Neither path falls back to defaults on malformed input (D-17: "reject malformed").
pub fn validate_file(path: &Path) -> Vec<ValidationError> {
    let content = match std::fs::read_to_string(path) {
        Ok(c)  => c,
        Err(e) => return vec![ValidationError::Io { path: path.to_owned(), source: e }],
    };

    // Use TopLevelKeyProbe to detect `id` and `match` keys (WAVE-0 FINDING pattern).
    // This avoids `serde_saphyr::Value` which does not exist in 0.0.26.
    let probe = match probe_top_level_keys(&content) {
        Ok(p)  => p,
        Err(e) => return vec![e],
    };

    if probe.has_id && probe.has_match {
        // Rule file path.
        validate_rule(path, &content)
    } else {
        // Config file path.
        let layer = infer_config_layer(path);
        validate_config(path, &content, layer)
    }
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Probe struct for top-level key presence (WAVE-0 FINDING TopLevelKeyProbe pattern).
struct TopLevelProbe {
    has_id:    bool,
    has_match: bool,
}

/// Probe the YAML content for top-level `id` and `match` keys.
fn probe_top_level_keys(content: &str) -> Result<TopLevelProbe, ValidationError> {
    // Use a partial struct with IgnoredAny so we don't pay deserialization costs.
    // serde_saphyr 0.0.26 supports this pattern (confirmed by wave0_smoke.rs).
    #[derive(Deserialize)]
    struct Probe {
        id:    Option<serde::de::IgnoredAny>,
        #[serde(rename = "match")]
        match_key: Option<serde::de::IgnoredAny>,
        #[serde(flatten)]
        _rest: HashMap<String, serde::de::IgnoredAny>,
    }

    match serde_saphyr::from_str::<Probe>(content) {
        Ok(p) => Ok(TopLevelProbe {
            has_id:    p.id.is_some(),
            has_match: p.match_key.is_some(),
        }),
        Err(e) => {
            // Malformed YAML — return a ParseError.
            let line = e.location().map(|l| l.line() as usize).unwrap_or(0);
            Err(ValidationError::ParseError {
                path: Path::new("<probe>").to_owned(),
                line,
                message: e.to_string(),
            })
        }
    }
}

/// Infer `ConfigLayer` from path (heuristic).
fn infer_config_layer(path: &Path) -> ConfigLayer {
    // Paths under `.lacon/` are project-layer files.
    if path.components().any(|c| c.as_os_str() == ".lacon") {
        ConfigLayer::Project
    } else {
        ConfigLayer::User
    }
}

/// Validate a rule file at `path` with content `content`.
fn validate_rule(path: &Path, content: &str) -> Vec<ValidationError> {
    // Full typed parse via RuleFile (deny_unknown_fields fires for unknown keys).
    match parse_one(content, path) {
        Ok(_rule) => {
            // Schema valid. Full compile (regex, script paths) would require
            // RuleLoader with layer context — that's the `lacon validate` full
            // path wired in PLAN-06. For standalone `validate_file`, schema
            // correctness is sufficient.
            //
            // Note: `extends` resolution is not attempted here because we don't
            // have a layer context to look up parents. PLAN-06 wires the full eager
            // load path (`RuleLoader::load_all`) for the `lacon validate` CLI command.
            Vec::new()
        }
        Err(e) => vec![e],
    }
}

/// Validate a config file at `path` with content `content`.
fn validate_config(path: &Path, _content: &str, layer: ConfigLayer) -> Vec<ValidationError> {
    // Delegate to config::parse_partial which handles:
    // - UserOnlyKeyInProject check (for project layer).
    // - deny_unknown_fields via PartialConfig serde derive.
    match parse_partial(path, layer) {
        Ok(_)   => Vec::new(),
        Err(es) => es,
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures_dir() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
    }

    #[test]
    fn validate_rule_file_valid() {
        let path = fixtures_dir().join("rules").join("valid_simple.yaml");
        let errs = validate_file(&path);
        assert!(errs.is_empty(), "valid rule should produce no errors: {:?}", errs);
    }

    #[test]
    fn validate_config_file_valid_user() {
        let path = fixtures_dir().join("configs").join("valid_user.yaml");
        let errs = validate_file(&path);
        assert!(errs.is_empty(), "valid user config should produce no errors: {:?}", errs);
    }

    #[test]
    fn validate_config_unknown_key() {
        let path = fixtures_dir().join("configs").join("unknown_key.yaml");
        let errs = validate_file(&path);
        assert!(
            !errs.is_empty() && errs.iter().any(|e| matches!(e, ValidationError::UnknownKey { .. })),
            "unknown key in config should produce UnknownKey error"
        );
    }

    #[test]
    fn dispatch_rule_by_id_and_match() {
        // A YAML file with top-level `id` AND `match` → rule validator.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("rule.yaml");
        std::fs::write(&path, r#"id: test
match:
  command: echo
pipeline:
  - strip_ansi
"#).unwrap();
        let errs = validate_file(&path);
        assert!(errs.is_empty(), "valid rule should have no errors: {:?}", errs);
    }

    #[test]
    fn dispatch_config_when_no_id_or_match() {
        // A YAML with only `defaults` → config validator.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("config.yaml");
        std::fs::write(&path, "defaults:\n  max_bytes: 4096\n").unwrap();
        let errs = validate_file(&path);
        assert!(errs.is_empty(), "valid config should have no errors: {:?}", errs);
    }
}
