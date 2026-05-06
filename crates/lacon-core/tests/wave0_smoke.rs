//! Wave 0 smoke tests for Phase 1.
//!
//! These tests exist to settle two open questions from RESEARCH.md before
//! PLAN-03 commits to the loader design:
//!
//! 1. Does `serde-saphyr` 0.0.26 expose a `Value` type adequate for the
//!    `lacon validate` content-dispatch check (top-level `id` AND `match`)?
//!    **FINDING:** serde-saphyr 0.0.26 does NOT expose `serde_saphyr::Value`.
//!    It is a pure typed-serde layer with no generic Value enum. The dispatch
//!    path in PLAN-03 must use the fallback: a partial struct with
//!    `Option<serde::de::IgnoredAny>` for the keys of interest (id + match).
//!    This approach is validated in `smoke_serde_saphyr_value_dispatch` below.
//!    See PLAN-03 acceptance — `serde_saphyr::Value` is NOT available.
//!
//! 2. Does `starlark` 0.13 compile under the workspace MSRV of 1.80?
//!    **FINDING:** Yes — verified by the smoke test below.

use serde::Deserialize;

/// Partial top-level-key probe used for D-17 content dispatch.
///
/// PLAN-03 will use this exact pattern in `lacon-core::validate` to decide
/// whether a YAML file is a rule file (has `id` AND `match`) or a config
/// file. `serde::de::IgnoredAny` lets us detect key presence without
/// deserializing the value — zero allocation, no schema coupling.
#[derive(Deserialize)]
struct TopLevelKeyProbe {
    id: Option<serde::de::IgnoredAny>,
    #[serde(rename = "match")]
    match_key: Option<serde::de::IgnoredAny>,
}

#[test]
fn smoke_serde_saphyr_value_dispatch() {
    // Mimics the D-17 `lacon validate` dispatch: parse YAML to a typed
    // probe struct, look for top-level `id` AND `match`. If both present →
    // rule file. Otherwise → config file.
    //
    // NOTE: serde-saphyr 0.0.26 does NOT expose serde_saphyr::Value.
    // The fallback path (TopLevelKeyProbe with IgnoredAny) is validated here.
    // PLAN-03 must use this pattern — not a generic Value type.

    let rule_yaml = r#"
id: example
match:
  command: echo
pipeline:
  - strip_ansi
"#;
    let config_yaml = r#"
defaults:
  max_bytes: 16384
"#;

    // Use typed partial struct with IgnoredAny for content dispatch.
    // This is the D-17 fallback confirmed by PLAN-01 Wave 0.
    let rule_probe: TopLevelKeyProbe = serde_saphyr::from_str(rule_yaml)
        .expect("rule yaml parses with TopLevelKeyProbe");
    let config_probe: TopLevelKeyProbe = serde_saphyr::from_str(config_yaml)
        .expect("config yaml parses with TopLevelKeyProbe");

    assert!(
        rule_probe.id.is_some(),
        "rule YAML must expose top-level `id` via TopLevelKeyProbe"
    );
    assert!(
        rule_probe.match_key.is_some(),
        "rule YAML must expose top-level `match` via TopLevelKeyProbe"
    );
    assert!(
        config_probe.id.is_none(),
        "config YAML must NOT expose top-level `id` (dispatch hinges on this)"
    );
}

#[test]
fn smoke_starlark_module_parses() {
    // Confirms starlark 0.13 compiles + can parse a trivial `process`
    // function body under the workspace MSRV. PLAN-04 builds on this.
    use starlark::syntax::{AstModule, Dialect};

    let src = r#"
def process(ctx, lines):
    return lines
"#;
    let _ast = AstModule::parse("smoke.star", src.to_owned(), &Dialect::Standard)
        .expect("trivial process() parses under starlark 0.13");
}
