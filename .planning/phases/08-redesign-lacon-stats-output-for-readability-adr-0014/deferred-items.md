# Phase 08 — Deferred Items

Out-of-scope discoveries logged during execution (not fixed; tracked here per the
executor SCOPE BOUNDARY rule — only auto-fix issues DIRECTLY caused by the current
task's changes).

## Pre-existing rustfmt drift (discovered during 08-02)

`cargo fmt --check` reports drift in files NOT touched by plan 08-02. All predate
this phase (verified byte-identical at base commit `ddc4dde`):

| File | Location | Nature |
|------|----------|--------|
| `crates/lacon-cli/src/commands/stats.rs` | `pub fn execute(...)` signature (line ~27) | long single-line fn signature rustfmt would wrap |
| `crates/lacon-cli/src/commands/stats.rs` | `normalize_project_strips_trailing_separator` test `format!` call | long single-line `format!` rustfmt would wrap |
| `crates/lacon-cli/tests/cli_stats.rs` | multiple (lines ~157–226) | pre-existing test formatting |
| `benches/cold_start.rs` | lines 105, 126, 186 | long `eprintln!`/`println!`/`run_scenario` calls |
| `crates/lacon-adapter-claudecode/src/lib.rs` | line 11 | `use` import grouping |

Note: CI (`.github/workflows/ci.yml`) does NOT run `cargo fmt --check` — it gates on
`cargo build` + `cargo test` + the cold-start bench only. So this drift is not a CI
break today. A separate cleanup pass (or a `cargo fmt` of the whole tree) would
resolve all of it; deferred so the 08-02 diff stays additive and scoped to the new
helpers. The new 08-02 helper code IS fmt-conformant.

## Pre-existing clippy warnings (discovered during 08-02)

`cargo clippy -p lacon-cli --all-targets` reports warnings in files NOT touched by
08-02 (e.g. `lacon-core` lib: collapsible-if, doc-list-overindent,
manual-ascii-comparison; `tracking_e2e` test: `&PathBuf` vs `&Path`). None reference
`commands/stats.rs`. Out of scope for 08-02; the new helpers introduce zero clippy
warnings.
