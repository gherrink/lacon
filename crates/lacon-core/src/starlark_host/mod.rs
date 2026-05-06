//! Starlark `post_process` host. Hermetic mode: Globals::standard() only.
//!
//! Per CONTEXT.md D-08 + ADR-0003 + ADR-0008:
//! - The Starlark function operates on the aggregated post-pipeline output.
//! - No per-line evaluation (out of v1 scope).
//! - No `load`, no file I/O, no `print` — `Globals::standard()` provides
//!   none of these. The evaluator is constructed without a file loader.
//!
//! Cold-start note: The AstModule is parsed once at rule load (D-14) and
//! cloned before each `run()` call (eval_module consumes the AST). A
//! cold-start microbenchmark in PLAN-07 will determine whether this is
//! adequate or requires lazy-init. AstModule::clone() is cheap (it is
//! Arc-backed internally per the starlark-0.13 implementation).
//!
//! API deviations from RESEARCH.md sketch:
//! - `eval_module` consumes the AstModule; we store it and clone per call.
//! - `ctx` is passed as a Starlark dict built via `heap.alloc(SmallMap)`.
//!   The spec allows `ctx["exit_code"]` dict access (not `ctx.exit_code`
//!   attribute access). v1 uses the dict form for simplicity; documented here.
//! - `Value::iterate(heap)` returns a `StarlarkIterator<'v>` which implements
//!   `Iterator<Item = Value<'v>>`.
//! - No `with_temp_heap` used; Module::new() + Evaluator::new(&module) is
//!   the correct 0.13 pattern.

use starlark::collections::SmallMap;
use starlark::environment::{Globals, Module};
use starlark::eval::Evaluator;
use starlark::syntax::{AstModule, Dialect};
use std::path::PathBuf;

use crate::error::{RuntimeError, ValidationError};

/// Parsed Starlark script for `post_process` (or inline `script:`).
///
/// `parse` is called once at rule load; `run` is called once per invocation
/// after the native pipeline completes. The `AstModule` is cloned before each
/// `eval_module` call because `eval_module` consumes the AST.
pub struct StarlarkScript {
    ast: AstModule,
    function_name: String,
    source_path: PathBuf,
}

/// Context passed to the Starlark `process(ctx, lines)` function.
///
/// Exposed to Starlark as a dict with keys:
/// `exit_code`, `duration_ms`, `command`, `args`, `project_path`.
///
/// v1 uses dict-style access (`ctx["exit_code"]`). Attribute-style access
/// (`ctx.exit_code`) would require a custom `StarlarkValue` impl and is
/// deferred to a future plan if ergonomics become a concern.
#[derive(Debug, Clone, Default)]
pub struct ScriptCtx {
    pub exit_code: i32,
    pub duration_ms: u64,
    pub command: String,
    pub args: Vec<String>,
    pub project_path: Option<String>,
}

impl StarlarkScript {
    /// Parse a Starlark script from source content.
    ///
    /// The `function_name` is the name of the entry function (typically `"process"`).
    /// The `source_path` is used for error messages and must already be validated
    /// (no `..`, not absolute) by the caller (see `resolve_script_path` in loader.rs).
    ///
    /// # Errors
    /// Returns `ValidationError::ParseError` if the Starlark source is syntactically invalid.
    /// Note: hermetic violations (e.g. `load()`) may not be caught at parse time under
    /// `Dialect::Standard` — they are caught at evaluation time in `run()`.
    pub fn parse(
        content: &str,
        function_name: String,
        source_path: PathBuf,
    ) -> Result<Self, ValidationError> {
        // Pitfall 4 (RESEARCH.md): AstModule::parse takes owned String.
        let display_name = source_path.display().to_string();
        match AstModule::parse(&display_name, content.to_owned(), &Dialect::Standard) {
            Ok(ast) => Ok(Self {
                ast,
                function_name,
                source_path,
            }),
            Err(e) => Err(ValidationError::ParseError {
                path: source_path,
                line: 0, // starlark::Error does not expose line/col via a stable public API in 0.13
                message: e.to_string(),
            }),
        }
    }

    /// Evaluate the `process(ctx, lines) -> list[str]` function hermetially.
    ///
    /// Hermetic guarantees (T-04-01):
    /// - `Globals::standard()` only — no file I/O, no network, no `load`.
    /// - No file loader is registered on the evaluator (hermetic by construction).
    ///
    /// # Errors
    /// - `StarlarkEvalError` if the module or function call raises.
    /// - `StarlarkResultTypeError` if the function returns a non-list or a list
    ///   containing non-string elements.
    pub fn run(
        &self,
        ctx: &ScriptCtx,
        lines: Vec<String>,
    ) -> Result<Vec<String>, RuntimeError> {
        let globals = Globals::standard();
        let module = Module::new();

        // Phase 1: evaluate the module body (defines `process` etc.) into the
        // module's namespace. eval_module consumes the AST, so we clone.
        {
            let mut eval = Evaluator::new(&module);
            let ast = self.ast.clone();
            eval.eval_module(ast, &globals)
                .map_err(|e| RuntimeError::StarlarkEvalError {
                    path: self.source_path.clone(),
                    message: e.to_string(),
                })?;
        }

        // Phase 2: look up `process` in the module namespace.
        let process_val =
            module
                .get(&self.function_name)
                .ok_or_else(|| RuntimeError::StarlarkEvalError {
                    path: self.source_path.clone(),
                    message: format!("function `{}` not defined", self.function_name),
                })?;

        // Phase 3: build ctx as a Starlark dict and lines as a Starlark list,
        // then call process(ctx, lines).
        let heap = module.heap();

        // Build ctx dict: keys are &str, values are Starlark-allocated scalars.
        let ctx_dict = {
            let mut map: SmallMap<&str, starlark::values::Value<'_>> = SmallMap::new();
            map.insert("exit_code", heap.alloc(ctx.exit_code));
            // u64 implements AllocValue via bigint path; use it directly.
            map.insert("duration_ms", heap.alloc(ctx.duration_ms));
            map.insert("command", heap.alloc(ctx.command.as_str()));
            // args: Vec of &str → Vec<Value> → Starlark list
            let args_vals: Vec<starlark::values::Value<'_>> = ctx
                .args
                .iter()
                .map(|s| heap.alloc(s.as_str()))
                .collect();
            map.insert("args", heap.alloc(args_vals));
            // project_path: None → Starlark None, Some(s) → str
            let pp_val = match &ctx.project_path {
                Some(p) => heap.alloc(p.as_str()),
                None => starlark::values::Value::new_none(),
            };
            map.insert("project_path", pp_val);
            heap.alloc(map)
        };

        // Build lines list.
        let lines_val = {
            let vals: Vec<starlark::values::Value<'_>> =
                lines.iter().map(|s| heap.alloc(s.as_str())).collect();
            heap.alloc(vals)
        };

        // Call process(ctx, lines).
        let result_val = {
            let mut eval = Evaluator::new(&module);
            eval.eval_function(process_val, &[ctx_dict, lines_val], &[])
                .map_err(|e| RuntimeError::StarlarkEvalError {
                    path: self.source_path.clone(),
                    message: e.to_string(),
                })?
        };

        // Extract result as Vec<String>. result_val must be a list of strings.
        let iter = result_val
            .iterate(heap)
            .map_err(|e| RuntimeError::StarlarkResultTypeError {
                path: self.source_path.clone(),
                function: self.function_name.clone(),
                got: format!("{} (iterate failed: {})", result_val.get_type(), e),
            })?;

        let mut out = Vec::new();
        for v in iter {
            let s = v.unpack_str().ok_or_else(|| RuntimeError::StarlarkResultTypeError {
                path: self.source_path.clone(),
                function: self.function_name.clone(),
                got: format!("list element of type {}", v.get_type()),
            })?;
            out.push(s.to_owned());
        }
        Ok(out)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Inline unit tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_round_trip() {
        let script = StarlarkScript::parse(
            "def process(ctx, lines):\n    return lines\n",
            "process".to_owned(),
            PathBuf::from("identity.star"),
        )
        .unwrap();
        let out = script
            .run(&ScriptCtx::default(), vec!["a".to_owned(), "b".to_owned()])
            .unwrap();
        assert_eq!(out, vec!["a".to_owned(), "b".to_owned()]);
    }

    #[test]
    fn ctx_exit_code_visible_to_script() {
        let script = StarlarkScript::parse(
            "def process(ctx, lines):\n    return [str(ctx['exit_code'])]\n",
            "process".to_owned(),
            PathBuf::from("ctx.star"),
        )
        .unwrap();
        let ctx = ScriptCtx {
            exit_code: 42,
            ..Default::default()
        };
        let out = script.run(&ctx, vec![]).unwrap();
        assert_eq!(out, vec!["42".to_owned()]);
    }

    #[test]
    fn non_list_return_is_type_error() {
        let script = StarlarkScript::parse(
            "def process(ctx, lines):\n    return 42\n",
            "process".to_owned(),
            PathBuf::from("bad.star"),
        )
        .unwrap();
        let err = script.run(&ScriptCtx::default(), vec![]).unwrap_err();
        assert!(
            matches!(err, RuntimeError::StarlarkResultTypeError { .. }),
            "expected StarlarkResultTypeError, got: {err}"
        );
    }

    #[test]
    fn load_statement_rejected_by_hermetic_runtime() {
        // `load` must be rejected — either at parse time or at eval time.
        // Verifies T-04-01 mitigation: hermetic by construction.
        let result = StarlarkScript::parse(
            "load(\"foo.bzl\", \"bar\")\ndef process(ctx, lines):\n    return lines\n",
            "process".to_owned(),
            PathBuf::from("loady.star"),
        );
        match result {
            Err(_) => {} // parse-time rejection — fine
            Ok(script) => {
                let err = script.run(&ScriptCtx::default(), vec![]).unwrap_err();
                assert!(
                    matches!(err, RuntimeError::StarlarkEvalError { .. }),
                    "load() must produce StarlarkEvalError if not caught at parse time"
                );
            }
        }
    }
}
