//! lacon-core — assistant-agnostic engine for filter rule loading,
//! streaming pipeline execution, Starlark post-processing, and the
//! `lacon run` runtime.
//!
//! Module map (filled across PLAN-02..PLAN-06):
//! - `error` — ValidationError + RuntimeError (PLAN-03)
//! - `config` — Config struct, layer merge (PLAN-03)
//! - `rules` — Rule schema, RuleLoader, extends flatten (PLAN-03)
//! - `pipeline` — Stage enum + 10 native primitives (PLAN-02)
//! - `starlark_host` — post_process VM bridge (PLAN-04)
//! - `runtime` — Runner: spawn, merge, signal forwarding, on_error (PLAN-05)
//! - `validate` — ValidateDispatch entry point (PLAN-03 / PLAN-06)

pub mod error;
pub mod config;
pub mod rules;
pub mod pipeline;
pub mod starlark_host;
pub mod runtime;
pub mod validate;
