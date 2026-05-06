//! RuleLoader — lazy-resolve hot path (D-14), eager path for validate/doctor.
//! mtime-based regex cache invalidation (D-15). Filled by PLAN-03.
//!
//! WAVE-0 FINDING (PLAN-01 task 3): serde-saphyr 0.0.26 does NOT expose
//! `serde_saphyr::Value`. PLAN-03 dispatch path: use the TopLevelKeyProbe
//! pattern — a partial struct with `Option<serde::de::IgnoredAny>` fields
//! for `id` and `match`, deserialized via `serde_saphyr::from_str`. This
//! pattern is validated in `crates/lacon-core/tests/wave0_smoke.rs`
//! (`smoke_serde_saphyr_value_dispatch`). Do NOT use `serde_saphyr::Value`.
//!
//! # Architecture (D-14)
//! - `resolve(rule_id)`: lazy hot path — parses only the first matching file.
//! - `load_all()`: eager path — parses every reachable rule file.
//!
//! # Layer walk order (ADR-0007, first-match-wins)
//! Project (`<cwd>/.lacon/rules/`) → User (`~/.config/lacon/rules/`) → Bundled (embedded)
//!
//! # Extends flattening (D-16, ADR-0012)
//! Parent pipeline PREPENDED to child pipeline. Scalar fields inherited when child omits.
//! Cycles detected via `HashSet<String>` of visited IDs → `CircularExtends`.
//!
//! # max_bytes injection (D-07, Pitfall 7)
//! Implicit `Stage::MaxBytes` appended to BOTH success_pipeline AND on_error_pipeline
//! AFTER extends flattening, when the fully-flattened pipeline has no `MaxBytes` stage.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use regex::{Regex, RegexSet};

use etcetera::BaseStrategy;

use crate::error::ValidationError;
use crate::pipeline::stages::{HeadTailMode, Stage};
use crate::pipeline::Pipeline;
use crate::rules::bundled::{get_bundled, iter_bundled};
use crate::rules::schema::{
    CollapseArgs, HeadTailArgs, KeepAroundArgs, ReplaceRegexArgs, RuleFile,
    ScriptSpec, StageSpec,
};

// ─── Public types ─────────────────────────────────────────────────────────────

/// The layer a rule was found in (first-match-wins, ADR-0007).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleSource {
    Project,
    User,
    Bundled,
}

/// A fully resolved and compiled rule — ready for the pipeline runner.
// Note: Pipeline does not implement Debug (it contains state). Custom Debug omits pipelines.
pub struct ResolvedRule {
    /// Stable rule identifier.
    pub id: String,
    /// Which layer this rule came from.
    pub source: RuleSource,
    /// Post-flatten raw schema (extends cleared, parent fields merged in).
    pub rule: RuleFile,
    /// Success pipeline (exit code 0). Always has a terminal MaxBytes stage (D-07).
    pub success_pipeline: Pipeline,
    /// On-error pipeline (non-zero exit). `None` if the rule has no `on_error` block.
    /// Also always has a terminal MaxBytes stage when present (D-07, independent of success pipeline).
    pub on_error_pipeline: Option<Pipeline>,
    /// Parsed Starlark `post_process` script for the success path (if present).
    /// Populated at parse time (D-14 lazy resolve); PLAN-05 calls
    /// `Pipeline::run_with_post_process` with this value.
    pub post_process: Option<crate::starlark_host::StarlarkScript>,
    /// Parsed Starlark `post_process` script for the on_error path (if present).
    pub on_error_post_process: Option<crate::starlark_host::StarlarkScript>,
}

/// Cache value: flat RuleFile + metadata needed to recompile without disk I/O.
#[derive(Clone)]
struct CachedRule {
    flat_rule: RuleFile,
    source: RuleSource,
    source_path: PathBuf,
}

/// Cache key: (absolute file path, last-modified time) — D-15.
type CacheKey = (PathBuf, SystemTime);

/// RuleLoader — resolves rules from the three-layer stack (project, user, bundled).
///
/// Thread safety: NOT thread-safe (uses `&mut self`). Create one per invocation context.
pub struct RuleLoader {
    /// Optional project rules directory (`<cwd>/.lacon/rules/`).
    project_dir: Option<PathBuf>,
    /// Optional user rules directory (`~/.config/lacon/rules/`).
    user_dir: Option<PathBuf>,
    /// In-process mtime cache (D-15). Stores the flat (post-extends) RuleFile so we can
    /// recompile quickly without disk I/O on cache hits.
    cache: HashMap<CacheKey, CachedRule>,
    /// Default max_bytes cap to inject when a rule omits its own MaxBytes stage (D-07).
    pub defaults_max_bytes: usize,
}

impl RuleLoader {
    /// Create a new `RuleLoader`.
    ///
    /// `project_dir`: if `Some(p)`, `p.join(".lacon/rules")` is the project layer.
    /// User dir is resolved via `etcetera` (XDG). `defaults_max_bytes` defaults to 32768.
    pub fn new(project_dir: Option<PathBuf>) -> Self {
        let user_dir = etcetera::choose_base_strategy()
            .ok()
            .map(|s| s.config_dir().join("lacon").join("rules"));

        Self {
            project_dir: project_dir.map(|p| p.join(".lacon").join("rules")),
            user_dir,
            cache: HashMap::new(),
            defaults_max_bytes: 32768,
        }
    }

    /// Lazy hot path: parses ONLY the file matching `rule_id` (D-14).
    ///
    /// Walks layers in priority order: project → user → bundled.
    /// Returns `Err(ValidationError::ParseError)` if no layer hits.
    pub fn resolve(&mut self, rule_id: &str) -> Result<ResolvedRule, ValidationError> {
        // Project and user layers (filesystem).
        let dirs: Vec<(Option<PathBuf>, RuleSource)> = vec![
            (self.project_dir.clone(), RuleSource::Project),
            (self.user_dir.clone(), RuleSource::User),
        ];

        for (dir_opt, source) in dirs {
            let Some(dir) = dir_opt else { continue };
            if let Some(result) = self.try_resolve_from_dir(rule_id, &dir, source)? {
                return Ok(result);
            }
        }

        // Bundled layer.
        if let Some(result) = try_resolve_from_bundled(rule_id, self.defaults_max_bytes)? {
            return Ok(result);
        }

        Err(ValidationError::ParseError {
            path: PathBuf::from("<resolver>"),
            line: 0,
            message: format!("no rule with id `{rule_id}`"),
        })
    }

    /// Eager path: parses every reachable rule file across all three layers.
    ///
    /// Returns `Ok(vec)` only when zero errors occurred.
    pub fn load_all(&mut self) -> Result<Vec<ResolvedRule>, Vec<ValidationError>> {
        let mut rules = Vec::new();
        let mut errors = Vec::new();

        let dirs: Vec<(Option<PathBuf>, RuleSource)> = vec![
            (self.project_dir.clone(), RuleSource::Project),
            (self.user_dir.clone(), RuleSource::User),
        ];

        for (dir_opt, source) in dirs {
            let Some(dir) = dir_opt else { continue };
            let entries = match std::fs::read_dir(&dir) {
                Ok(e) => e,
                Err(_) => continue,
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
                    continue;
                }
                let content = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => { errors.push(ValidationError::Io { path, source: e }); continue; }
                };
                match self.parse_flatten_compile(&content, &path, &dir, source.clone()) {
                    Ok(r) => rules.push(r),
                    Err(e) => errors.push(e),
                }
            }
        }

        // Bundled layer.
        for name in iter_bundled() {
            let content = match get_bundled(&name) {
                Some(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
                None => continue,
            };
            let synthetic_path = PathBuf::from("bundled").join(&name);
            match parse_one(&content, &synthetic_path) {
                Ok(rule) => {
                    let mut visited = HashSet::new();
                    match flatten_extends_with_lookup(rule, &synthetic_path, &mut visited, &|_, _| None) {
                        Ok(flat) => {
                            match compile_resolved(flat, &synthetic_path, RuleSource::Bundled, self.defaults_max_bytes) {
                                Ok(r) => rules.push(r),
                                Err(e) => errors.push(e),
                            }
                        }
                        Err(e) => errors.push(e),
                    }
                }
                Err(e) => errors.push(e),
            }
        }

        if errors.is_empty() { Ok(rules) } else { Err(errors) }
    }

    /// Try to resolve `rule_id` from a filesystem directory.
    /// Returns `Ok(None)` if not found in this dir; `Ok(Some(...))` on match.
    fn try_resolve_from_dir(
        &mut self,
        rule_id: &str,
        dir: &Path,
        source: RuleSource,
    ) -> Result<Option<ResolvedRule>, ValidationError> {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(None), // dir missing → skip
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
                continue;
            }
            let content = match std::fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => return Err(ValidationError::Io { path, source: e }),
            };
            let probed_id = shallow_probe_id(&content);
            if probed_id.as_deref() == Some(rule_id) {
                let result = self.parse_flatten_compile(&content, &path, dir, source)?;
                return Ok(Some(result));
            }
        }
        Ok(None)
    }

    /// Parse + flatten + compile a rule from disk with mtime caching.
    fn parse_flatten_compile(
        &mut self,
        content: &str,
        path: &Path,
        dir: &Path,
        source: RuleSource,
    ) -> Result<ResolvedRule, ValidationError> {
        // mtime cache check.
        let cache_hit = if let Ok(meta) = std::fs::metadata(path) {
            meta.modified().ok().and_then(|mtime| {
                let key = (path.to_owned(), mtime);
                self.cache.get(&key).cloned().map(|v| (key, v))
            })
        } else {
            None
        };

        if let Some((_key, cached)) = cache_hit {
            return compile_resolved(cached.flat_rule, &cached.source_path, cached.source, self.defaults_max_bytes);
        }

        // Full parse + flatten.
        let rule = parse_one(content, path)?;
        let dir_clone = dir.to_owned();
        let flat = flatten_extends_with_lookup(rule, path, &mut HashSet::new(), &|parent_id, child_path| {
            // Look up parent in the same directory.
            find_rule_in_dir(parent_id, &dir_clone, child_path)
        })?;

        // Compile.
        let resolved = compile_resolved(flat.clone(), path, source.clone(), self.defaults_max_bytes)?;

        // Cache the flat RuleFile (saves disk I/O + serde parse on subsequent hits).
        if let Ok(meta) = std::fs::metadata(path) {
            if let Ok(mtime) = meta.modified() {
                let key = (path.to_owned(), mtime);
                self.cache.insert(key, CachedRule {
                    flat_rule: flat,
                    source,
                    source_path: path.to_owned(),
                });
            }
        }

        Ok(resolved)
    }

    /// Parse a rule YAML string into a `RuleFile`, mapping serde errors.
    pub fn parse_one(&self, content: &str, source_path: &Path) -> Result<RuleFile, ValidationError> {
        parse_one(content, source_path)
    }
}

// ─── Standalone functions (also used by validate module) ──────────────────────

/// Parse a rule YAML string into a `RuleFile`, mapping serde errors to `ValidationError`.
pub fn parse_one(content: &str, source_path: &Path) -> Result<RuleFile, ValidationError> {
    serde_saphyr::from_str::<RuleFile>(content).map_err(|e| {
        let line = e.location().map(|l| l.line() as usize).unwrap_or(0);
        let msg = e.to_string();
        if msg.contains("unknown field") {
            ValidationError::UnknownKey {
                path: source_path.to_owned(),
                line,
                message: msg,
            }
        } else {
            ValidationError::ParseError {
                path: source_path.to_owned(),
                line,
                message: msg,
            }
        }
    })
}

/// Shallow YAML probe: extract only the `id` string field.
///
/// Used for the lazy hot path to avoid full deserialization.
/// `serde_saphyr` is used — NOT `serde_saphyr::Value` (which doesn't exist in 0.0.26).
pub fn shallow_probe_id(content: &str) -> Option<String> {
    #[derive(serde::Deserialize)]
    struct IdProbe {
        id: Option<String>,
        #[serde(flatten)]
        _rest: HashMap<String, serde::de::IgnoredAny>,
    }
    serde_saphyr::from_str::<IdProbe>(content)
        .ok()
        .and_then(|p| p.id)
}

/// Try to find and parse a parent rule by ID inside a given directory.
fn find_rule_in_dir(
    rule_id: &str,
    dir: &Path,
    child_path: &Path,
) -> Option<Result<RuleFile, ValidationError>> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("yaml") {
            continue;
        }
        let content = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => return Some(Err(ValidationError::Io { path, source: e })),
        };
        if shallow_probe_id(&content).as_deref() == Some(rule_id) {
            return Some(parse_one(&content, child_path));
        }
    }
    None
}

/// Flatten an `extends` chain for a rule.
///
/// `parent_lookup` is called to resolve a parent rule by ID, given the child path.
/// Cycle detection uses `visited: &mut HashSet<String>`.
///
/// ADR-0012: parent pipeline PREPENDED; scalar fields inherited when child omits them.
pub fn flatten_extends_with_lookup<F>(
    rule: RuleFile,
    source_path: &Path,
    visited: &mut HashSet<String>,
    parent_lookup: &F,
) -> Result<RuleFile, ValidationError>
where
    F: Fn(&str, &Path) -> Option<Result<RuleFile, ValidationError>>,
{
    // Cycle detection (T-03-03, Pitfall 6).
    if visited.contains(&rule.id) {
        return Err(ValidationError::CircularExtends {
            path: source_path.to_owned(),
            line: 0,
            message: format!(
                "circular `extends` chain: rule `{}` is already in the chain {:?}",
                rule.id,
                visited.iter().collect::<Vec<_>>()
            ),
        });
    }
    visited.insert(rule.id.clone());

    let Some(ref parent_id) = rule.extends.clone() else {
        return Ok(rule);
    };

    let bare_id = strip_layer_prefix(parent_id);

    // Try to look up the parent.
    let parent_raw = match parent_lookup(bare_id, source_path) {
        Some(Ok(p)) => p,
        Some(Err(e)) => return Err(e),
        None => {
            // Parent not found in the provided lookup → error.
            return Err(ValidationError::ParseError {
                path: source_path.to_owned(),
                line: 0,
                message: format!("could not find parent rule `{bare_id}` for extends chain"),
            });
        }
    };

    // Recursively flatten the parent first (multi-hop chains flatten to one, D-16).
    let flat_parent = flatten_extends_with_lookup(parent_raw, source_path, visited, parent_lookup)?;

    Ok(merge_rules(flat_parent, rule))
}

/// Merge a flattened parent into a child (ADR-0012).
fn merge_rules(parent: RuleFile, mut child: RuleFile) -> RuleFile {
    // Scalar fields: child wins; fall back to parent.
    if child.description.is_none()  { child.description  = parent.description;  }
    if child.match_spec.is_none()   { child.match_spec   = parent.match_spec;   }
    if child.bypass_when.is_none()  { child.bypass_when  = parent.bypass_when;  }
    if child.rewrite.is_none()      { child.rewrite       = parent.rewrite;      }
    if child.on_error.is_none()     { child.on_error      = parent.on_error;     }
    if child.post_process.is_none() { child.post_process  = parent.post_process; }

    // Pipeline: prepend parent's stages (ADR-0012).
    let parent_stages = parent.pipeline.unwrap_or_default();
    let child_stages  = child.pipeline.unwrap_or_default();
    child.pipeline = Some([parent_stages, child_stages].concat());

    child.extends = None; // chain is now flat
    child
}

/// Strip optional `bundled/`, `user/`, `project/` prefix from an extends ID.
fn strip_layer_prefix(id: &str) -> &str {
    for prefix in &["bundled/", "user/", "project/"] {
        if let Some(bare) = id.strip_prefix(prefix) {
            return bare;
        }
    }
    id
}

/// Try to resolve `rule_id` from the bundled layer.
fn try_resolve_from_bundled(
    rule_id: &str,
    defaults_max_bytes: usize,
) -> Result<Option<ResolvedRule>, ValidationError> {
    for name in iter_bundled() {
        let content = match get_bundled(&name) {
            Some(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            None => continue,
        };
        if shallow_probe_id(&content).as_deref() == Some(rule_id) {
            let synthetic_path = PathBuf::from("bundled").join(&name);
            let rule = parse_one(&content, &synthetic_path)?;
            // Bundled rules extending other bundled rules: look up within bundled set.
            let mut visited = HashSet::new();
            let flat = flatten_extends_with_lookup(rule, &synthetic_path, &mut visited, &|pid, cpath| {
                find_in_bundled(pid, cpath)
            })?;
            let resolved = compile_resolved(flat, &synthetic_path, RuleSource::Bundled, defaults_max_bytes)?;
            return Ok(Some(resolved));
        }
    }
    Ok(None)
}

/// Find a rule by ID in the bundled layer.
fn find_in_bundled(rule_id: &str, child_path: &Path) -> Option<Result<RuleFile, ValidationError>> {
    for name in iter_bundled() {
        let content = match get_bundled(&name) {
            Some(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            None => continue,
        };
        if shallow_probe_id(&content).as_deref() == Some(rule_id) {
            return Some(parse_one(&content, child_path));
        }
    }
    None
}

/// Compile a flat `RuleFile` into a `ResolvedRule`.
///
/// - Converts each `StageSpec` to a runtime `Stage`.
/// - Injects implicit `MaxBytes` into success AND on_error pipelines independently (D-07).
/// - Validates script paths (T-03-04).
pub fn compile_resolved(
    rule: RuleFile,
    source_path: &Path,
    source: RuleSource,
    defaults_max_bytes: usize,
) -> Result<ResolvedRule, ValidationError> {
    let success_stages = rule.pipeline.clone().unwrap_or_default();
    let success_pipeline = compile_pipeline(success_stages, source_path, defaults_max_bytes)?;

    let on_error_pipeline = if let Some(ref on_err) = rule.on_error {
        let stages = on_err.pipeline.clone();
        Some(compile_pipeline(stages, source_path, defaults_max_bytes)?)
    } else {
        None
    };

    // Resolve and parse post_process Starlark scripts (T-04-03 path-traversal guard).
    let post_process = if let Some(ref pp) = rule.post_process {
        Some(resolve_script(pp, source_path)?)
    } else {
        None
    };

    let on_error_post_process = if let Some(ref on_err) = rule.on_error {
        if let Some(ref pp) = on_err.post_process {
            Some(resolve_script(pp, source_path)?)
        } else {
            None
        }
    } else {
        None
    };

    Ok(ResolvedRule {
        id: rule.id.clone(),
        source,
        rule,
        success_pipeline,
        on_error_pipeline,
        post_process,
        on_error_post_process,
    })
}

/// Compile a list of `StageSpec` into a runtime `Pipeline`.
///
/// After converting all stages, injects an implicit `MaxBytes` cap if none is present.
pub fn compile_pipeline(
    specs: Vec<StageSpec>,
    source_path: &Path,
    defaults_max_bytes: usize,
) -> Result<Pipeline, ValidationError> {
    let mut stages: Vec<Stage> = Vec::with_capacity(specs.len() + 1);

    for spec in specs {
        let stage = spec_to_stage(spec, source_path)?;
        stages.push(stage);
    }

    // Inject implicit MaxBytes AFTER extends flatten (Pitfall 7).
    let has_max_bytes = stages.iter().any(|s| matches!(s, Stage::MaxBytes { .. }));
    if !has_max_bytes {
        stages.push(Stage::MaxBytes {
            cap: defaults_max_bytes,
            written: 0,
            truncated: false,
            dropped_bytes: 0,
        });
    }

    Ok(Pipeline::new(stages))
}

/// Convert a single `StageSpec` to a runtime `Stage`.
fn spec_to_stage(spec: StageSpec, source_path: &Path) -> Result<Stage, ValidationError> {
    match spec {
        StageSpec::StripAnsi => Ok(Stage::StripAnsi),

        StageSpec::DropRegex(pattern) => {
            Ok(Stage::DropRegex(compile_regex(&pattern, source_path)?))
        }

        StageSpec::KeepRegex(pattern) => {
            let set = RegexSet::new([&pattern]).map_err(|e| ValidationError::InvalidRegex {
                path: source_path.to_owned(),
                line: 0,
                message: e.to_string(),
            })?;
            Ok(Stage::KeepRegex(set))
        }

        StageSpec::ReplaceRegex(ReplaceRegexArgs { pattern, replacement }) => {
            Ok(Stage::ReplaceRegex {
                pattern: compile_regex(&pattern, source_path)?,
                replacement,
            })
        }

        StageSpec::Dedupe(args) => {
            let max_kept = args.map(|a| a.max_kept).unwrap_or(1);
            Ok(Stage::Dedupe {
                last: None,
                max_kept,
                repeat_count: 0,
                kept_so_far: 0,
            })
        }

        StageSpec::CollapseRepeated(CollapseArgs { pattern, max_kept, summary }) => {
            Ok(Stage::CollapseRepeated {
                pattern: compile_regex(&pattern, source_path)?,
                max_kept,
                summary_template: summary,
                kept_so_far: 0,
                dropped: 0,
            })
        }

        StageSpec::KeepHead(HeadTailArgs { lines, bytes }) => {
            let mode = head_tail_mode(lines, bytes, "keep_head", source_path)?;
            let (lr, br) = match &mode {
                HeadTailMode::Lines(n) => (*n, 0),
                HeadTailMode::Bytes(n) => (0, *n),
            };
            Ok(Stage::KeepHead { mode, lines_remaining: lr, bytes_remaining: br })
        }

        StageSpec::KeepTail(HeadTailArgs { lines, bytes }) => {
            let mode = head_tail_mode(lines, bytes, "keep_tail", source_path)?;
            Ok(Stage::KeepTail {
                mode,
                ring: std::collections::VecDeque::new(),
                byte_count: 0,
            })
        }

        StageSpec::KeepAroundMatch(KeepAroundArgs { pattern, before, after }) => {
            Ok(Stage::KeepAroundMatch {
                pattern: compile_regex(&pattern, source_path)?,
                before,
                after,
                ctx_buf: std::collections::VecDeque::new(),
                emit_after: 0,
            })
        }

        StageSpec::MaxBytes(cap) => Ok(Stage::MaxBytes { cap, written: 0, truncated: false, dropped_bytes: 0 }),

        StageSpec::Script(ScriptSpec { path, .. }) => {
            // Validate the path (T-03-04) and then reject with a clear message.
            // Inline pipeline `script:` stages are deferred to PLAN-04 (StarlarkHost).
            // Validate the path so T-03-04 tests pass.
            resolve_script_path(&path, source_path)?;
            Err(ValidationError::ParseError {
                path: source_path.to_owned(),
                line: 0,
                message: "inline `script:` in `pipeline:` is not supported in v1; use top-level `post_process:`".to_owned(),
            })
        }
    }
}

/// Compile a regex pattern, mapping errors to `ValidationError::InvalidRegex`.
fn compile_regex(pattern: &str, source_path: &Path) -> Result<Regex, ValidationError> {
    Regex::new(pattern).map_err(|e| ValidationError::InvalidRegex {
        path: source_path.to_owned(),
        // Per plan: line 0 acceptable for v1; serde-saphyr does not expose per-stage
        // line numbers cheaply (the YAML mapping gives us only top-level positions).
        line: 0,
        message: e.to_string(),
    })
}

/// Build a `HeadTailMode` from optional `lines` / `bytes` spec fields.
///
/// WR-01: rejects `n == 0` as degenerate — the PLAN-03 comment in stages.rs
/// documented that zero should be rejected; this is now enforced at parse time.
fn head_tail_mode(
    lines: Option<usize>,
    bytes: Option<usize>,
    stage_name: &str,
    source_path: &Path,
) -> Result<HeadTailMode, ValidationError> {
    match (lines, bytes) {
        // WR-01: zero is a degenerate count — reject at load time.
        (Some(0), None) | (None, Some(0)) => Err(ValidationError::ParseError {
            path: source_path.to_owned(),
            line: 0,
            message: format!("`{stage_name}` count/bytes must be > 0"),
        }),
        (Some(n), None)  => Ok(HeadTailMode::Lines(n)),
        (None,    Some(n)) => Ok(HeadTailMode::Bytes(n)),
        (Some(_), Some(_)) => Err(ValidationError::ParseError {
            path: source_path.to_owned(),
            line: 0,
            message: format!("`{stage_name}` must specify exactly one of `lines` or `bytes`"),
        }),
        (None, None) => Err(ValidationError::ParseError {
            path: source_path.to_owned(),
            line: 0,
            message: format!("`{stage_name}` requires either `lines` or `bytes`"),
        }),
    }
}

/// Resolve and validate a Starlark script path (T-03-04 path traversal mitigation).
///
/// Enforces:
/// 1. Path must be relative (not absolute).
/// 2. No `..` components allowed.
/// 3. File must exist at the resolved path.
fn resolve_script_path(script_path: &Path, rule_path: &Path) -> Result<PathBuf, ValidationError> {
    // 1. Reject absolute paths.
    if script_path.is_absolute() {
        return Err(ValidationError::MissingScriptFile {
            path: rule_path.to_owned(),
            line: 0,
            message: format!(
                "`script.path` must be relative, not absolute: `{}`",
                script_path.display()
            ),
        });
    }

    // 2. Reject `..` components (T-03-04: path traversal guard).
    if script_path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err(ValidationError::MissingScriptFile {
            path: rule_path.to_owned(),
            line: 0,
            message: format!(
                "`script.path` must not contain `..` (path traversal rejected): `{}`",
                script_path.display()
            ),
        });
    }

    // 3. Resolve relative to rule file's parent directory.
    let rule_dir = rule_path.parent().unwrap_or(Path::new("."));
    let resolved = rule_dir.join(script_path);

    // 4. File must exist.
    if !resolved.exists() {
        return Err(ValidationError::MissingScriptFile {
            path: rule_path.to_owned(),
            line: 0,
            message: format!(
                "Starlark script not found: `{}` (resolved from `{}` relative to rule dir `{}`)",
                resolved.display(),
                script_path.display(),
                rule_dir.display(),
            ),
        });
    }

    Ok(resolved)
}

/// Resolve a `ScriptSpec` from a rule file: validate path, read content, parse Starlark.
///
/// Reuses `resolve_script_path` for the path-traversal guard (T-04-03).
/// Parse errors are returned as `ValidationError::ParseError` (fail at rule load time,
/// not at run time).
fn resolve_script(
    spec: &ScriptSpec,
    rule_path: &Path,
) -> Result<crate::starlark_host::StarlarkScript, ValidationError> {
    // Path validation and existence check (T-04-03).
    let resolved = resolve_script_path(&spec.path, rule_path)?;

    // Read the script content.
    let content = std::fs::read_to_string(&resolved).map_err(|e| ValidationError::Io {
        path: resolved.clone(),
        source: e,
    })?;

    // Parse the Starlark source — fail at rule load time if invalid.
    crate::starlark_host::StarlarkScript::parse(&content, spec.function.clone(), resolved)
}
