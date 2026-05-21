//! Rule schema, loader, extends flatten — filled by PLAN-03.

pub mod bundled;
pub mod loader;
pub mod rewrite;
pub mod schema;

pub use loader::{match_argv_via_load_all, ResolvedRule, RuleLoader, RuleSource};
pub use rewrite::apply_rewrite;
pub use schema::{
    BypassWhen, MatchSpec, OnErrorSpec, RewriteSpec, RuleFile, ScriptSpec, StageSpec,
    // Arg types
    CollapseArgs, DedupeArgs, HeadTailArgs, KeepAroundArgs, ReplaceRegexArgs,
};
