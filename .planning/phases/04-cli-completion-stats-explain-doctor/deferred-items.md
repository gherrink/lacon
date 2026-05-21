# Deferred Items — Phase 04

Out-of-scope discoveries logged during execution. Not fixed in their originating
plan because they predate it and fall outside the plan's scope boundary.

## Pre-existing clippy lints surfaced by clippy 1.95.0 (logged in Plan 04-01)

`cargo clippy -p lacon-core -- -D warnings` reports 4 errors in Phase 1/2 code.
All predate Plan 04-01 and are NOT in any file this plan owns; surfaced only
because the toolchain's clippy lint set tightened. NOT fixed here — fixing
other phases' code from a Phase 4 read-layer plan would be scope creep and could
mask a regression. The new Plan 04-01 files (`tracking/query.rs`, the
`open_readonly` helper, and the two test files) are clippy-clean.

| Location | Lint | Origin commit |
|----------|------|---------------|
| `crates/lacon-core/src/pipeline/stages.rs:438` | `collapsible_if` (if can collapse into outer match) | `8924ff0` (Phase 1) |
| `crates/lacon-core/src/pipeline/stages.rs:451` | `collapsible_if` | `8924ff0` (Phase 1) |
| `crates/lacon-core/src/tracking/record.rs:8` | `doc_overindented_list_items` | `192e2c2` (Phase 2) |
| `crates/lacon-core/src/tracking/mod.rs:201` | `manual_ignore_case_cmp` (use `eq_ignore_ascii_case`) | `9798e78` (Phase 2) |

Recommended owner: a small Phase 6 (hardening) cleanup pass, or whichever later
plan next edits these files. Each is a one-line mechanical fix.

**Re-confirmed in Plan 04-02:** the same 4 lints persist (unchanged). Plan 04-02's
only source change, `crates/lacon-core/src/runtime/mod.rs` (`Runner::filter_bytes`),
is clippy-clean. No new lints introduced.

**Re-confirmed in Plan 04-04:** the same 4 lints persist (unchanged). Plan 04-04's
files (`crates/lacon-cli/src/commands/doctor.rs`, `tests/cli_doctor.rs`,
`tests/cli_surface.rs`, `tests/tracking_coldstart.rs`) are all clippy-clean
(`cargo clippy -p lacon-cli --bins --tests` reports 0 hits on them). The 4
lacon-core lints surface only because the workspace `-D warnings` gate compiles
lacon-core as a dependency; fixing them remains a Phase 6 hardening item.
