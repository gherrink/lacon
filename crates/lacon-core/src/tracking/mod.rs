//! Tracking subsystem (Phase 2): SQLite-backed history of every `lacon run`.
//!
//! Lives at `~/.local/share/lacon/history.db` (XDG `data_dir/lacon/history.db`)
//! per REQ-tracking-sqlite-location. Tracker writes are best-effort (D-12) — the
//! CLI logs failures to stderr and never alters exit codes.
//!
//! Module layout:
//! - `migrations` — single inline `M0001_INITIAL` migration via `user_version`
//! - `normalize` — pure `fn normalize(argv) -> String` for command grouping
//! - `privacy` — first-time `store_raw_outputs` opt-in marker + warning text
//! - `health` — `Tracker::health_check` no-op probe (Phase 4 surface)
//! - `prune` — throttled retention pruning (24h gate via `lacon_meta`)
//!
//! Cold-start posture (D-04): Tracker::open is reachable ONLY from
//! `lacon-cli::commands::run` after `Runner::run` returns. `lacon --version`,
//! `lacon validate`, and `lacon doctor` MUST NOT call into this module.

pub mod normalize;

pub use normalize::normalize;

/// Raw subprocess output captured for `raw_outputs` storage (D-01).
/// Populated by `lacon-cli::commands::run` only when `cfg.store_raw_outputs == true`.
#[derive(Debug, Clone, Default)]
pub struct RawOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Tracker handle. One instance per `lacon run` invocation; dropped at function exit.
/// Phase 2 / Plan 04 adds `pub fn open(...)`, `pub fn record(...)`, `pub fn prune(...)`,
/// and `pub fn health_check(...)`. This skeleton exists so downstream plans can
/// import the type without a forward-reference dance.
pub struct Tracker {
    // Fields filled by Plan 04. Held private so the public API stabilizes from day one.
    #[allow(dead_code)]
    pub(crate) cfg_store_raw_outputs: bool,
}

/// Map a `RuleSource` to the spec-mandated TEXT value for `invocations.rule_source`.
/// Per `docs/specs/tracking-data-model.md:25`: `'project' | 'user' | 'bundled' | NULL`.
/// Pitfall 12 from RESEARCH.md.
pub fn rule_source_str(s: &crate::rules::RuleSource) -> &'static str {
    match s {
        crate::rules::RuleSource::Project => "project",
        crate::rules::RuleSource::User => "user",
        crate::rules::RuleSource::Bundled => "bundled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::RuleSource;

    #[test]
    fn rule_source_str_maps_all_three_variants() {
        assert_eq!(rule_source_str(&RuleSource::Project), "project");
        assert_eq!(rule_source_str(&RuleSource::User), "user");
        assert_eq!(rule_source_str(&RuleSource::Bundled), "bundled");
    }
}
