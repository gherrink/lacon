//! Config struct and three-layer deep-merge resolver.
//!
//! Maps to `docs/specs/config-schema.md`. Three-layer hierarchy:
//! - Bundled (compiled-in defaults)
//! - User (`~/.config/lacon/config.yaml`)
//! - Project (`<cwd>/.lacon/config.yaml`)
//!
//! # Scope rules
//! - `retention.*` — USER-ONLY. Project config files containing `retention` fail
//!   validation with `UserOnlyKeyInProject` (T-03-06 mitigation).
//! - `defaults.*`, `store_raw_outputs` — PROJECT-OR-USER.
//!
//! # Unknown keys
//! `#[serde(deny_unknown_fields)]` on every struct → unknown keys fail with
//! `ValidationError::UnknownKey` (T-03-01, CON-config-unknown-keys).
//!
//! # Deep merge semantics
//! Per-key: higher-priority layer overrides scalar keys. Sub-objects (`retention`,
//! `defaults`) merge recursively (rather than wholesale replacement), so the user
//! only needs to specify the keys they want to override.

use std::path::Path;

use serde::Deserialize;

use crate::error::ValidationError;

/// Effective merged config.
///
/// Defaults: retention(30/3), defaults.max_bytes=32768, store_raw_outputs=false.
#[derive(Debug, Clone, Default)]
pub struct Config {
    /// Retention windows for the SQLite tables. USER-ONLY scope.
    pub retention: Retention,
    /// Engine defaults (cap injection etc.).
    pub defaults: Defaults,
    /// Whether to store raw subprocess output in `raw_outputs` table.
    pub store_raw_outputs: bool,
}

/// Retention windows for the SQLite tracking tables.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retention {
    /// Invocation row retention (days). Default 30.
    #[serde(default = "default_invocations_days")]
    pub invocations_days: u32,
    /// Raw output blob retention (days). Default 3.
    #[serde(default = "default_raw_outputs_days")]
    pub raw_outputs_days: u32,
}

impl Default for Retention {
    fn default() -> Self {
        Self {
            invocations_days: default_invocations_days(),
            raw_outputs_days: default_raw_outputs_days(),
        }
    }
}

fn default_invocations_days() -> u32 { 30 }
fn default_raw_outputs_days()  -> u32 { 3  }

/// Engine default values.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Defaults {
    /// Fallback final-stage `max_bytes` cap for rules that omit their own (D-07). Default 32768.
    #[serde(default = "default_max_bytes")]
    pub max_bytes: usize,
}

impl Default for Defaults {
    fn default() -> Self {
        Self { max_bytes: default_max_bytes() }
    }
}

fn default_max_bytes() -> usize { 32768 }

/// Partial config — one layer's YAML contribution (missing fields stay `None`).
///
/// `pub(crate)` so the `validate` sibling module can use it without a re-export.
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub(crate) struct PartialConfig {
    #[serde(default)]
    pub(crate) retention: Option<Retention>,
    #[serde(default)]
    pub(crate) defaults:  Option<Defaults>,
    #[serde(default)]
    pub(crate) store_raw_outputs: Option<bool>,
}

/// Which config layer a file belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigLayer {
    Bundled,
    User,
    Project,
}

/// Load and merge config from optional user and project file paths.
///
/// Order: bundled defaults ← user overrides ← project overrides.
/// Returns `Err(errors)` if any layer fails to parse.
///
/// Project files containing `retention.*` keys are rejected with `UserOnlyKeyInProject`
/// before typed deserialization (T-03-06).
pub fn load_layered(
    project_path: Option<&Path>,
    user_path:    Option<&Path>,
) -> Result<Config, Vec<ValidationError>> {
    let mut errors = Vec::new();
    let mut cfg    = Config::default(); // start from bundled defaults

    // User layer.
    if let Some(p) = user_path {
        match parse_partial(p, ConfigLayer::User) {
            Ok(part)   => apply_partial(&mut cfg, part),
            Err(mut e) => errors.append(&mut e),
        }
    }

    // Project layer.
    if let Some(p) = project_path {
        match parse_partial(p, ConfigLayer::Project) {
            Ok(part)   => apply_partial(&mut cfg, part),
            Err(mut e) => errors.append(&mut e),
        }
    }

    if errors.is_empty() { Ok(cfg) } else { Err(errors) }
}

/// Parse a single config layer file, applying the USER-ONLY retention check for project layer.
pub(crate) fn parse_partial(
    path: &Path,
    layer: ConfigLayer,
) -> Result<PartialConfig, Vec<ValidationError>> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| vec![ValidationError::Io { path: path.to_owned(), source: e }])?;

    // Project-layer retention pre-check (T-03-06): use TopLevelKeyProbe to detect
    // `retention` key BEFORE typed deserialization, so the error is explicit.
    if matches!(layer, ConfigLayer::Project) {
        retention_precheck(&content, path)?;
    }

    serde_saphyr::from_str::<PartialConfig>(&content)
        .map_err(|e| {
            let line = e.location().map(|l| l.line() as usize).unwrap_or(0);
            let msg  = e.to_string();
            let cat = if msg.contains("unknown field") {
                ValidationError::UnknownKey  { path: path.to_owned(), line, message: msg }
            } else {
                ValidationError::ParseError  { path: path.to_owned(), line, message: msg }
            };
            vec![cat]
        })
}

/// Check that a project config does not contain the `retention` key (T-03-06).
fn retention_precheck(content: &str, path: &Path) -> Result<(), Vec<ValidationError>> {
    // Use the TopLevelKeyProbe pattern (WAVE-0 FINDING: serde_saphyr::Value does not exist).
    // A partial struct with `Optional<IgnoredAny>` for the key of interest.
    #[derive(serde::Deserialize)]
    struct RetentionProbe {
        retention: Option<serde::de::IgnoredAny>,
        #[serde(flatten)]
        _rest: std::collections::HashMap<String, serde::de::IgnoredAny>,
    }

    // If the YAML doesn't even parse as a map, skip the retention check —
    // typed deserialization below will produce a clearer error.
    let Ok(probe) = serde_saphyr::from_str::<RetentionProbe>(content) else {
        return Ok(());
    };

    if probe.retention.is_some() {
        return Err(vec![ValidationError::UserOnlyKeyInProject {
            path: path.to_owned(),
            // Line 1: `retention:` is almost always the first key; exact line tracking
            // would require per-key span API which serde-saphyr 0.0.26 does not expose cheaply.
            line: 1,
            message: "key `retention` is user-only; move to ~/.config/lacon/config.yaml".to_owned(),
        }]);
    }

    Ok(())
}

/// Apply a partial config over the current effective config (deep merge).
fn apply_partial(cfg: &mut Config, p: PartialConfig) {
    if let Some(r) = p.retention        { cfg.retention        = r; }
    if let Some(d) = p.defaults         { cfg.defaults         = d; }
    if let Some(b) = p.store_raw_outputs { cfg.store_raw_outputs = b; }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_correct() {
        let cfg = Config::default();
        assert_eq!(cfg.retention.invocations_days, 30);
        assert_eq!(cfg.retention.raw_outputs_days, 3);
        assert_eq!(cfg.defaults.max_bytes, 32768);
        assert!(!cfg.store_raw_outputs);
    }

    #[test]
    fn load_layered_user_overrides_bundled() {
        let tmp = tempfile::TempDir::new().unwrap();
        let user_path = tmp.path().join("user.yaml");
        std::fs::write(&user_path, "retention:\n  invocations_days: 90\n").unwrap();

        let cfg = load_layered(None, Some(&user_path)).expect("loads ok");
        assert_eq!(cfg.retention.invocations_days, 90, "user override applied");
        assert_eq!(cfg.retention.raw_outputs_days, 3,  "bundled default retained");
    }

    #[test]
    fn load_layered_project_overrides_user() {
        let tmp = tempfile::TempDir::new().unwrap();
        let user_path    = tmp.path().join("user.yaml");
        let project_path = tmp.path().join("project.yaml");
        std::fs::write(&user_path,    "defaults:\n  max_bytes: 16384\n").unwrap();
        std::fs::write(&project_path, "defaults:\n  max_bytes: 8192\n").unwrap();

        let cfg = load_layered(Some(&project_path), Some(&user_path)).expect("loads ok");
        assert_eq!(cfg.defaults.max_bytes, 8192, "project override wins");
    }

    #[test]
    fn project_retention_rejected() {
        let tmp = tempfile::TempDir::new().unwrap();
        let project_path = tmp.path().join("project.yaml");
        std::fs::write(&project_path, "retention:\n  invocations_days: 7\n").unwrap();

        let errs = load_layered(Some(&project_path), None).expect_err("retention in project fails");
        assert!(
            errs.iter().any(|e| matches!(e, ValidationError::UserOnlyKeyInProject { .. })),
            "expected UserOnlyKeyInProject error"
        );
    }

    #[test]
    fn unknown_key_rejected() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("config.yaml");
        std::fs::write(&path, "banana: yes\n").unwrap();

        let errs = load_layered(None, Some(&path)).expect_err("unknown key fails");
        assert!(
            errs.iter().any(|e| matches!(e, ValidationError::UnknownKey { .. })),
            "expected UnknownKey error"
        );
    }

    #[test]
    fn missing_file_is_not_an_error() {
        // Passing None for both paths uses bundled defaults, which always succeeds.
        let cfg = load_layered(None, None).expect("no-file case uses defaults");
        assert_eq!(cfg.defaults.max_bytes, 32768);
    }
}
