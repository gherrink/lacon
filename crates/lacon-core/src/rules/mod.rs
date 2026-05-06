//! Rule schema, loader, extends flatten — filled by PLAN-03.

pub mod bundled;
pub mod loader;
pub mod schema;

pub use loader::{RuleLoader, ResolvedRule, RuleSource};
pub use schema::{
    BypassWhen, MatchSpec, OnErrorSpec, RewriteSpec, RuleFile, ScriptSpec, StageSpec,
    // Arg types
    CollapseArgs, DedupeArgs, HeadTailArgs, KeepAroundArgs, ReplaceRegexArgs,
};
