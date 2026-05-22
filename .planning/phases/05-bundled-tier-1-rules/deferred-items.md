# Phase 05 — Deferred Items

Out-of-scope discoveries found during execution. NOT fixed (SCOPE BOUNDARY: only
auto-fix issues directly caused by the current task's changes).

## Pre-existing clippy warnings in `lacon-core` lib (discovered in plan 05-01)

`cargo clippy --test bundled_rules` surfaces 4 pre-existing warnings, all in
`lacon-core` library source — none originate from the new `bundled_rules.rs`
test. Left untouched (Phase 1–4 code, unrelated to this plan):

- `crates/lacon-core/src/pipeline/stages.rs:438` — `if` collapsible into outer `match`
- `crates/lacon-core/src/pipeline/stages.rs:451` — `if` collapsible into outer `match`
- `crates/lacon-core/src/tracking/record.rs:8` — doc list item overindented
- `crates/lacon-core/src/tracking/mod.rs:201` — manual case-insensitive ASCII comparison

Also note (build-time, pre-existing): `lacon-cli` references an invalid dependency
`test_emitter` missing a lib target — a cargo warning unrelated to this plan.

## Pre-existing `cli_doctor` test failure (re-confirmed in plan 05-05)

`cargo test -p lacon-cli --test cli_doctor` fails on
`doctor_all_green_passes_and_exits_zero` with `CARGO_BIN_EXE_test_emitter is unset`.
Root cause is the missing `bin/test_emitter` crate referenced at
`crates/lacon-cli/Cargo.toml:27` (introduced in Phase 4, commit 690409b) — NOT the
git-status rule. `cargo test --test bundled_rules` (this plan's deliverable) is green;
the full-workspace red is confined to this one pre-existing infrastructure gap.
