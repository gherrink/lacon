---
phase: 01-engine-core-lacon-run-wrapper
plan: 03
subsystem: infra
tags: [rust, serde-saphyr, rule-loading, config, validation, extends, mtime-cache, path-traversal, bundled-rules, rust-embed, etcetera]

# Dependency graph
requires:
  - phase: 01-engine-core-lacon-run-wrapper
    plan: 01
    provides: "Cargo workspace with serde-saphyr, rust-embed, etcetera, regex, thiserror declared"
  - phase: 01-engine-core-lacon-run-wrapper
    plan: 02
    provides: "enum Stage + Pipeline::new() + Pipeline::run()"
provides:
  - "ValidationError thiserror enum with D-18 byte-exact format: <path>:<line>: <Category>: <message>"
  - "RuleFile serde struct (deny_unknown_fields on all nested structs) — wire format for filter-rule-schema.md"
  - "StageSpec externally-tagged enum (11 variants) mapping YAML to runtime Stage"
  - "BundledRules rust-embed wrapper over bundled-rules/ directory"
  - "RuleLoader with lazy resolve (D-14), mtime invalidation cache (D-15), extends flatten (D-16), first-match-wins walk (ADR-0007)"
  - "CircularExtends detection via HashSet<String> visited set (T-03-03)"
  - "Path traversal rejection in script: path field (T-03-04)"
  - "Implicit MaxBytes injection after extends flatten (D-07/Pitfall 7)"
  - "Config struct with 3-layer deep-merge (bundled <- user <- project)"
  - "UserOnlyKeyInProject check via RetentionProbe pattern (T-03-06)"
  - "validate_file() content-dispatched entry point (D-17)"
  - "TopLevelKeyProbe dispatch pattern: id+match -> rule validator; else -> config validator"
  - "22 unit tests (inline) + 22 integration tests across 3 integration test files = 90 total tests in lacon-core"
affects: [01-04, 01-05, 01-06, 01-07]

# Tech tracking
tech-stack:
  added:
    - "serde-saphyr 0.0.26: TopLevelKeyProbe pattern (Option<serde::de::IgnoredAny> + flatten) — serde_saphyr::Value does NOT exist"
    - "rust-embed 8.x: relative folder path '../../bundled-rules/' from crate manifest dir (no $CARGO_MANIFEST_DIR expansion needed)"
    - "etcetera: requires 'use etcetera::BaseStrategy;' trait in scope for config_dir() method"
    - "thiserror 2.x: Io variant uses struct form {path, source} not tuple — thiserror #[from] limitation with extra fields"
  patterns:
    - "Pattern A: TopLevelKeyProbe — partial struct with Option<IgnoredAny> fields + #[serde(flatten)] HashMap rest; avoids serde_saphyr::Value which does not exist in 0.0.26"
    - "Pattern B: CacheKey (PathBuf, SystemTime) -> CachedRule; stores flat RuleFile (Cloneable), recompiles on cache hit (saves disk I/O at cost of regex recompile)"
    - "Pattern C: shallow_probe_id() via HashMap<String, IgnoredAny> deserialization — cheap ID matching without full RuleFile parse"
    - "Pattern D: flatten_extends_with_lookup() takes F: Fn closure for parent lookup — enables testing without filesystem"

key-files:
  created:
    - path: "crates/lacon-core/src/error.rs"
      role: "ValidationError enum — 8 variants, D-18 format, path() accessor"
    - path: "crates/lacon-core/src/rules/schema.rs"
      role: "RuleFile + MatchSpec + StageSpec (11 variants) + auxiliary arg structs"
    - path: "crates/lacon-core/src/rules/bundled.rs"
      role: "BundledRules rust-embed wrapper; iter_bundled() + get_bundled()"
    - path: "crates/lacon-core/src/rules/loader.rs"
      role: "RuleLoader: resolve(), load_all(), extends flatten, mtime cache, script path validation"
    - path: "crates/lacon-core/tests/rules_loader.rs"
      role: "10 integration tests: resolve, mtime cache/invalidation, bundled fallback, error cases, path traversal, implicit MaxBytes"
    - path: "crates/lacon-core/tests/extends_flatten.rs"
      role: "5 integration tests: prepend, scalar inheritance, cycle detection, implicit/explicit MaxBytes"
    - path: "crates/lacon-core/tests/validate_dispatch.rs"
      role: "7 integration tests: rule/config dispatch, project retention, unknown key, D-18 format, content-not-filename"
    - path: "crates/lacon-core/tests/fixtures/rules/valid_simple.yaml"
      role: "Fixture: strip_ansi + drop_regex + explicit max_bytes:1024"
    - path: "crates/lacon-core/tests/fixtures/rules/parent.yaml"
      role: "Fixture: pnpm rule with on_error block"
    - path: "crates/lacon-core/tests/fixtures/rules/child.yaml"
      role: "Fixture: extends parent, adds drop_regex stage"
    - path: "crates/lacon-core/tests/fixtures/rules/cycle_a.yaml"
      role: "Fixture: extends cycle-b (circular extends test)"
    - path: "crates/lacon-core/tests/fixtures/rules/cycle_b.yaml"
      role: "Fixture: extends cycle-a (circular extends test)"
    - path: "crates/lacon-core/tests/fixtures/rules/invalid_regex.yaml"
      role: "Fixture: drop_regex with '[' (invalid regex)"
    - path: "crates/lacon-core/tests/fixtures/rules/unknown_primitive.yaml"
      role: "Fixture: pipeline: [reverse_lines] (unknown primitive)"
    - path: "crates/lacon-core/tests/fixtures/rules/missing_script.yaml"
      role: "Fixture: script: ./does_not_exist.star"
    - path: "crates/lacon-core/tests/fixtures/configs/valid_user.yaml"
      role: "Fixture: valid user config with retention + defaults + store_raw_outputs"
    - path: "crates/lacon-core/tests/fixtures/configs/project_with_retention.yaml"
      role: "Fixture: project config with retention block (triggers UserOnlyKeyInProject)"
    - path: "crates/lacon-core/tests/fixtures/configs/unknown_key.yaml"
      role: "Fixture: config with unknown key 'banana'"
  modified:
    - path: "crates/lacon-core/src/config/mod.rs"
      role: "Config struct + Retention + Defaults + PartialConfig + ConfigLayer + load_layered() + parse_partial()"
    - path: "crates/lacon-core/src/validate/mod.rs"
      role: "validate_file() + TopLevelProbe dispatch + infer_config_layer() + validate_rule() + validate_config()"
    - path: "crates/lacon-core/src/rules/mod.rs"
      role: "Re-exports: RuleLoader, ResolvedRule, RuleSource, schema types"
    - path: "crates/lacon-core/src/rules/loader.rs"
      role: "Clippy fixes: manual_inspect removal, derivable_impls for Config"

decisions:
  - "WAVE-0 FINDING confirmed: serde_saphyr::Value does NOT exist in 0.0.26. All dispatch uses TopLevelKeyProbe pattern with Option<IgnoredAny> partial structs. This is definitive — do not attempt serde_saphyr::Value in any future plan."
  - "StageSpec externally-tagged enum works with serde-saphyr 0.0.26 standard serde derive — no manual Deserialize impl needed. '- strip_ansi' (unit), '- drop_regex: pattern' (newtype), '- script: {path, function}' (struct) all round-trip correctly."
  - "ResolvedRule does not implement Debug or Clone because Pipeline lacks those impls. Tests use .err().expect() not .expect_err(). Cache stores CachedRule{flat_rule: RuleFile} and recompiles on hit."
  - "rust-embed folder path: '../../bundled-rules/' relative to $CARGO_MANIFEST_DIR works without interpolate-folder-path feature. Avoids Cargo.toml modification (B1 freeze constraint)."
  - "etcetera BaseStrategy trait must be in scope: 'use etcetera::BaseStrategy;' required for config_dir() method to be callable on Xdg strategy."
  - "Io variant uses struct form {path: PathBuf, source: io::Error} not tuple — thiserror #[from] does not accept tuple variants with extra fields beyond the source."
  - "Implicit MaxBytes injection: applied AFTER stage conversion in compile_pipeline(), scanning the stages Vec for any Stage::MaxBytes variant. Applied independently to both success_pipeline and on_error_pipeline (ADR-0010)."
  - "T-03-06 mitigation: uses RetentionProbe (TopLevelKeyProbe pattern) not serde_saphyr::Value. WAVE-0 FINDING forced this design. Probe has Option<IgnoredAny> for retention key + flatten HashMap for rest."

metrics:
  duration: "~150 minutes (across 2 sessions)"
  completed: "2026-05-06"
  tasks: 3
  files_created: 19
  files_modified: 4
  tests_total: 90
  tests_added_this_plan: 22
---

# Phase 01 Plan 03: Rule Loader, Config Layer-Merge, and Validate Dispatcher Summary

Rule loading layer walk with mtime cache, extends flattening with cycle detection, three-layer config deep-merge, and content-dispatched validate_file using the TopLevelKeyProbe pattern (WAVE-0 FINDING: serde_saphyr::Value does not exist in 0.0.26).

## What Was Built

### Task 1: ValidationError + RuleFile Schema + BundledRules Embedding

**ValidationError** (`error.rs`): 8-variant thiserror enum rendering `<path>:<line>: <Category>: <message>` per D-18. All 7 named categories (InvalidRegex, UnknownPrimitive, CircularExtends, MissingScriptFile, UserOnlyKeyInProject, UnknownKey, ParseError) plus Io with struct form `{path, source}`.

**RuleFile schema** (`rules/schema.rs`): Full `#[serde(deny_unknown_fields)]` serde deserialization for the filter-rule-schema.md wire format. `StageSpec` externally-tagged enum with 11 variants (10 native primitives + Script). Confirmed: standard serde derive works with serde-saphyr 0.0.26 for all YAML forms (unit, newtype, struct-valued).

**BundledRules** (`rules/bundled.rs`): rust-embed struct with `#[folder = "../../bundled-rules/"]` (relative path from crate manifest dir resolves without `interpolate-folder-path` feature — Cargo.toml B1 freeze respected).

### Task 2: RuleLoader

**RuleLoader** (`rules/loader.rs`): Implements the lazy hot path (D-14) and eager path (`load_all()`).

Layer walk order: project (`.lacon/rules/*.yaml`) → user (`~/.config/lacon/rules/*.yaml`) → bundled (rust-embed). Each candidate is shallow-probed for `id` via `shallow_probe_id()` before full parse.

**extends flattening** (D-16, ADR-0012): `flatten_extends_with_lookup()` takes a closure for parent lookup, enabling testable isolation. HashSet<String> visited set detects cycles and returns `CircularExtends` before stack overflow (T-03-03). Parent pipeline PREPENDED to child pipeline. Scalar fields (match_spec, bypass_when, rewrite, on_error, post_process, description) fall through to parent when child omits them.

**mtime cache** (D-15): `HashMap<(PathBuf, SystemTime), CachedRule>`. Cache stores flat `RuleFile` (which implements Clone) and recompiles on cache hit — saves disk I/O, regex compiles from memory. Bundled rules use `SystemTime::UNIX_EPOCH` as synthetic mtime (immutable at runtime).

**Path traversal guard** (T-03-04): `resolve_script_path()` rejects absolute paths and paths containing `Component::ParentDir` (`..`), returning `MissingScriptFile`.

**Implicit MaxBytes injection** (D-07/Pitfall 7): After `spec_to_stage()` conversion, `compile_pipeline()` scans for any `Stage::MaxBytes`; if absent, appends `Stage::MaxBytes { cap: defaults_max_bytes }`. Applied independently to both success_pipeline and on_error_pipeline.

### Task 3: Config Layer-Merge + Validate Dispatcher

**Config** (`config/mod.rs`): Three-layer deep-merge with `#[derive(Default)]`. `PartialConfig` (pub(crate)) for per-layer YAML contributions. `parse_partial()` uses `RetentionProbe` (TopLevelKeyProbe pattern — WAVE-0 FINDING forced this; serde_saphyr::Value unavailable) to detect project-layer `retention` keys before typed deserialization.

**validate_file** (`validate/mod.rs`): Reads content, uses `probe_top_level_keys()` to detect `id` + `match` at top level. Dispatches to `validate_rule()` (delegates to `parse_one()` which uses `serde_saphyr::from_str::<RuleFile>`) or `validate_config()` (delegates to `parse_partial()`). Config layer inferred from path: `.lacon/` component present → Project; else User.

## Test Results

```
cargo test -p lacon-core: 90 passed (7 suites)
```

Fixture tree:

```
crates/lacon-core/tests/fixtures/
├── configs/
│   ├── project_with_retention.yaml  (retention in project → UserOnlyKeyInProject)
│   ├── unknown_key.yaml             (banana: yes → UnknownKey)
│   └── valid_user.yaml              (retention + defaults + store_raw_outputs)
└── rules/
    ├── child.yaml                   (extends: parent + drop_regex)
    ├── cycle_a.yaml                 (extends: cycle-b)
    ├── cycle_b.yaml                 (extends: cycle-a)
    ├── invalid_regex.yaml           (drop_regex: '[' → InvalidRegex)
    ├── missing_script.yaml          (script: ./does_not_exist.star → MissingScriptFile)
    ├── parent.yaml                  (pnpm with on_error block)
    ├── unknown_primitive.yaml       (reverse_lines → UnknownKey/ParseError)
    └── valid_simple.yaml            (strip_ansi + drop_regex + max_bytes:1024)
```

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] WAVE-0 FINDING: serde_saphyr::Value does not exist**
- **Found during:** Task 3 implementation (config retention pre-check)
- **Issue:** Plan's Task 3 action block used `serde_saphyr::Value::get("retention")` and `serde_saphyr::Value::get("id")`/`"match"`, but `serde_saphyr::Value` does not exist in 0.0.26 (this was documented in WAVE-0 but the plan's code snippets used it)
- **Fix:** Used `TopLevelKeyProbe` pattern throughout: `Probe { id: Option<IgnoredAny>, match_key: Option<IgnoredAny>, _rest: HashMap<String, IgnoredAny> }` and `RetentionProbe { retention: Option<IgnoredAny>, _rest: HashMap<String, IgnoredAny> }` — same pattern already documented in 01-RESEARCH.md Pattern 5
- **Files modified:** `config/mod.rs`, `validate/mod.rs`
- **Commit:** c72bab1

**2. [Rule 1 - Bug] rust-embed path with $CARGO_MANIFEST_DIR variable not expanded**
- **Found during:** Task 1 (bundled.rs compile)
- **Issue:** `#[folder = "$CARGO_MANIFEST_DIR/../../bundled-rules/"]` requires `interpolate-folder-path` feature which cannot be added (Cargo.toml B1 freeze)
- **Fix:** Used `#[folder = "../../bundled-rules/"]` — rust-embed resolves relative paths from CARGO_MANIFEST_DIR automatically without the feature
- **Files modified:** `rules/bundled.rs`
- **Commit:** c20b5d7

**3. [Rule 1 - Bug] etcetera BaseStrategy trait not in scope**
- **Found during:** Task 2 (RuleLoader::new compilation)
- **Issue:** `etcetera::base_strategy::Xdg::new()?.config_dir()` failed because `config_dir()` is a trait method requiring `use etcetera::BaseStrategy;` in scope
- **Fix:** Added `use etcetera::BaseStrategy;` import to loader.rs
- **Files modified:** `rules/loader.rs`
- **Commit:** 550600a

**4. [Rule 1 - Bug] Clippy: derivable_impls, manual_inspect, doc_lazy_continuation**
- **Found during:** Post-Task 2, pre-Task 3 clippy run
- **Issue:** Manual `impl Default for Config` when all fields had `Default` (derivable); `map(|r| { let _ = var; r })` no-op (manual_inspect); doc comment list continuation without blank line
- **Fix:** `#[derive(Default)]` on Config; removed no-op map; added blank line before doc list
- **Files modified:** `config/mod.rs`, `rules/loader.rs`, `validate/mod.rs`
- **Commit:** c72bab1

**5. [Rule 1 - Bug] thiserror #[from] incompatible with tuple variant containing extra fields**
- **Found during:** Task 1 (error.rs compile)
- **Issue:** Plan's interfaces block showed `Io(#[from] std::io::Error, PathBuf)` tuple form, but thiserror 2.x does not support `#[from]` on multi-field tuple variants
- **Fix:** Used struct form `Io { path: PathBuf, source: std::io::Error }` as noted in the Task 1 action block footnote
- **Files modified:** `error.rs`
- **Commit:** c20b5d7

## Known Stubs

None. All implemented functionality is wired to real data:
- `BundledRules` embeds the actual `bundled-rules/` directory (currently contains only `.gitkeep`; zero rules returned by `iter_bundled()` which filters for `.yaml` extension)
- `RuleLoader::new()` resolves real XDG paths via etcetera
- `validate_file()` performs real serde deserialization, not placeholder logic

## Threat Flags

No new security surface beyond what the plan's threat model covers. All T-03-* mitigations implemented as specified.

## Self-Check: PASSED
