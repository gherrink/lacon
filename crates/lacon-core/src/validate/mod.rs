//! `lacon validate` content-dispatched entry point.
//!
//! `validate_file(path)` reads a YAML file, introspects its top-level keys, and
//! dispatches to the rule validator or config validator (D-17):
//! - Top-level `id` present → rule file → rule validator (regardless of `match:`).
//! - Anything else → config file → config validator.
//!
//! ADR-0012: child rules that inherit `match:` via `extends:` are valid rule files
//! without a top-level `match:` key.  Routing on `has_id` alone covers both cases.
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

use crate::config::{parse_partial_from_str, ConfigLayer};
use crate::error::ValidationError;
use crate::rules::loader::parse_one;

/// Content-dispatched validation entry point. Returns empty `Vec` on success.
///
/// Dispatch logic (D-17, CR-02 fix):
/// - Top-level `id` present → validate as rule file.
/// - Otherwise → validate as config file.
///
/// Note: `match:` is NOT required for routing because ADR-0012 child rules
/// inherit `match:` from their parent via `extends:` and therefore have no
/// top-level `match:` key.  Routing on `has_id` alone covers both cases.
///
/// Neither path falls back to defaults on malformed input (D-17: "reject malformed").
pub fn validate_file(path: &Path) -> Vec<ValidationError> {
    let content = match std::fs::read_to_string(path) {
        Ok(c)  => c,
        Err(e) => return vec![ValidationError::Io { path: path.to_owned(), source: e }],
    };

    // Use TopLevelKeyProbe to detect `id` key (WAVE-0 FINDING pattern).
    // This avoids `serde_saphyr::Value` which does not exist in 0.0.26.
    let probe = match probe_top_level_keys(&content) {
        Ok(p)  => p,
        Err(e) => return vec![e],
    };

    if probe.has_id {
        // Rule file path (has_id alone is the routing criterion per ADR-0012).
        validate_rule(path, &content)
    } else {
        // Config file path.
        let layer = infer_config_layer(path);
        validate_config(path, &content, layer)
    }
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Probe struct for top-level key presence (WAVE-0 FINDING TopLevelKeyProbe pattern).
///
/// `has_match` is retained for potential future use (e.g., warning when a
/// non-extends rule omits `match:`), but is not used for dispatch routing.
struct TopLevelProbe {
    has_id:    bool,
    #[allow(dead_code)]
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
///
/// Wires the full compile pipeline so SC4 (Phase 1 success criterion) is satisfied:
/// 1. Schema parse via `parse_one` — catches `UnknownKey` (deny_unknown_fields) and
///    `ParseError` (malformed YAML, including unknown StageSpec variants like `reverse_lines`).
/// 2. `extends` flattening via `flatten_extends_with_lookup` with a same-directory parent
///    lookup closure — catches `CircularExtends`. Parents are looked up in `path.parent()`.
/// 3. `compile_resolved` — catches `InvalidRegex` (regex compile fails) and
///    `MissingScriptFile` (Starlark script path absolute / contains `..` / does not exist).
///
/// The resulting `ResolvedRule` is discarded — we only care whether the compile pass succeeded.
fn validate_rule(path: &Path, content: &str) -> Vec<ValidationError> {
    // Step 1: schema parse.
    let rule = match parse_one(content, path) {
        Ok(r)  => r,
        Err(e) => return vec![e],
    };

    // Step 2: flatten extends. Look up parents in the same directory as `path`.
    // For files outside any rules directory (ad-hoc validation), this still works
    // because `find_rule_in_dir` returns `None` when the parent ID is not present —
    // which produces a `ParseError` ("could not find parent rule") for the user.
    let parent_dir = path
        .parent()
        .map(|p| p.to_owned())
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let mut visited = std::collections::HashSet::new();
    let lookup = |parent_id: &str, child_path: &Path| {
        crate::rules::loader::find_rule_in_dir(parent_id, &parent_dir, child_path)
    };

    let flat = match crate::rules::loader::flatten_extends_with_lookup(
        rule,
        path,
        &mut visited,
        &lookup,
    ) {
        Ok(f)  => f,
        Err(e) => return vec![e],
    };

    // Step 3: compile pass. Catches InvalidRegex, MissingScriptFile, and any
    // remaining ParseError surfaces (e.g. Starlark parse failure).
    match crate::rules::loader::compile_resolved(
        flat,
        path,
        crate::rules::loader::RuleSource::Project, // synthetic — unused after compile
        crate::rules::loader::DEFAULT_MAX_BYTES,
    ) {
        Ok(_resolved) => Vec::new(),
        Err(e)        => vec![e],
    }
}

/// Validate a config file at `path` with already-loaded `content`.
///
/// WR-04 fix: previously delegated to `parse_partial(path, layer)` which
/// re-read the file from disk, ignoring the `content` parameter entirely.
/// Now uses `parse_partial_from_str` to avoid the redundant disk I/O and
/// the associated TOCTOU hazard (file could change between reads).
fn validate_config(path: &Path, content: &str, layer: ConfigLayer) -> Vec<ValidationError> {
    // Uses the already-loaded `content` string (read once in `validate_file`).
    // Handles:
    // - UserOnlyKeyInProject check (for project layer).
    // - deny_unknown_fields via PartialConfig serde derive.
    match parse_partial_from_str(content, path, layer) {
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

    #[test]
    fn dispatch_extend_only_rule_routed_to_rule_validator() {
        // ADR-0012 CR-02: a rule with `id` + `extends` but NO top-level `match:`
        // must be routed to the rule validator, not the config validator.
        // Previously this was misrouted because dispatch required `has_id && has_match`.
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("child-rule.yaml");
        std::fs::write(&path, r#"id: cargo-build-quiet
extends: cargo-build-base
pipeline:
  - strip_ansi
"#).unwrap();
        let errs = validate_file(&path);
        // The rule is structurally valid (id, extends, pipeline) — schema should
        // accept it even though parse_one cannot resolve the parent (no parent file).
        // The key assertion: it must NOT fail with UnknownKey (which would only happen
        // if it were misrouted to the config validator, which rejects `id`, `extends`,
        // and `pipeline` as unknown config keys).
        let misrouted = errs.iter().any(|e| matches!(e, ValidationError::UnknownKey { .. }));
        assert!(
            !misrouted,
            "extend-only rule must NOT produce UnknownKey (would indicate misrouting to config validator): {errs:?}"
        );
    }
}
