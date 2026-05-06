//! RuleLoader — lazy-resolve hot path (D-14), eager path for validate/doctor.
//! mtime-based regex cache invalidation (D-15). Filled by PLAN-03.
//!
//! WAVE-0 FINDING (PLAN-01 task 3): serde-saphyr 0.0.26 does NOT expose
//! `serde_saphyr::Value`. PLAN-03 dispatch path: use the TopLevelKeyProbe
//! pattern — a partial struct with `Option<serde::de::IgnoredAny>` fields
//! for `id` and `match`, deserialized via `serde_saphyr::from_str`. This
//! pattern is validated in `crates/lacon-core/tests/wave0_smoke.rs`
//! (`smoke_serde_saphyr_value_dispatch`). Do NOT use `serde_saphyr::Value`.
