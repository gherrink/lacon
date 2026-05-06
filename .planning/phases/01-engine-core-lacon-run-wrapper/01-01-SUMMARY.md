---
phase: 01-engine-core-lacon-run-wrapper
plan: 01
subsystem: infra
tags: [rust, cargo, workspace, serde-saphyr, starlark, os_pipe, crossbeam-channel, nix, signal-hook, clap, thiserror, etcetera, rust-embed, smallvec]

# Dependency graph
requires: []
provides:
  - Cargo workspace root with exhaustive [workspace.dependencies] for all v1 crates
  - Three member crates: lacon-core (engine), lacon-cli (binary), lacon-adapter-claudecode (stub)
  - Module skeleton: 7 stub modules in lacon-core (error, config, rules, pipeline, starlark_host, runtime, validate)
  - rust-toolchain.toml pinning stable channel
  - release profile: opt-level=z, lto=thin, panic=abort, strip=symbols, codegen-units=1
  - signal-hook declared in [workspace.dependencies] so PLAN-05 inherits without editing Cargo.toml
  - Wave 0 smoke tests resolving open questions 1 and 2 from RESEARCH.md
  - WAVE-0 FINDING: serde_saphyr::Value does NOT exist — PLAN-03 must use TopLevelKeyProbe pattern
affects: [01-02, 01-03, 01-04, 01-05, 01-06, 01-07]

# Tech tracking
tech-stack:
  added:
    - regex 1.x — NFA-based pattern matching, RegexSet OR-merge
    - serde 1.x with derive — typed struct deserialization
    - serde-saphyr 0.0.26 — YAML parsing (NOT serde_yaml which is deprecated)
    - clap 4.x with derive — CLI argument parsing
    - starlark 0.13 — hermetic Starlark VM for post_process
    - os_pipe 1.x — subprocess stdout+stderr merge
    - crossbeam-channel 0.5.x — reader-thread to pipeline-loop channel
    - nix 0.31 with signal feature — POSIX signal forwarding
    - signal-hook 0.3 — signal handler watcher thread
    - thiserror 2.x — error enum derive
    - anyhow 1.x — error propagation at CLI boundary only
    - etcetera 0.11 — XDG path resolution
    - rust-embed 8.x — compile-time bundled rule embedding
    - smallvec 1.x (stable; NOT 2.x alpha) — Stage::step output accumulator
    - assert_cmd 2.x, predicates 3.x, tempfile 3.x, insta 1.x — dev/test dependencies
  patterns:
    - Cargo workspace inheritance via version.workspace = true / {workspace = true}
    - All v1 deps declared exhaustively in root Cargo.toml so no Wave-1+ plan edits it
    - D-17 content dispatch via TopLevelKeyProbe struct with Option<serde::de::IgnoredAny>

key-files:
  created:
    - Cargo.toml — workspace root with exhaustive [workspace.dependencies] and [profile.release]
    - rust-toolchain.toml — stable toolchain pin
    - crates/lacon-core/Cargo.toml — all Wave-1+ deps; PLAN-02..PLAN-05 must NOT modify
    - crates/lacon-core/src/lib.rs — 7 module declarations (pub mod error/config/rules/pipeline/starlark_host/runtime/validate)
    - crates/lacon-core/src/error.rs — stub (PLAN-03 fills)
    - crates/lacon-core/src/pipeline/mod.rs + stages.rs — stub (PLAN-02 fills)
    - crates/lacon-core/src/rules/mod.rs + schema.rs + loader.rs + bundled.rs — stubs (PLAN-03 fills)
    - crates/lacon-core/src/config/mod.rs — stub (PLAN-03 fills)
    - crates/lacon-core/src/starlark_host/mod.rs — stub (PLAN-04 fills)
    - crates/lacon-core/src/runtime/mod.rs — stub (PLAN-05 fills)
    - crates/lacon-core/src/validate/mod.rs — stub (PLAN-03/06 fills)
    - crates/lacon-core/tests/wave0_smoke.rs — Wave 0 smoke tests (2 passing)
    - crates/lacon-cli/Cargo.toml — binary target manifest
    - crates/lacon-cli/src/main.rs — skeleton main() printing v0.1.0 marker
    - crates/lacon-adapter-claudecode/Cargo.toml — stub adapter manifest
    - crates/lacon-adapter-claudecode/src/lib.rs — ClaudeCodeAdapterStub (Phase 3 fills)
    - bundled-rules/.gitkeep — placeholder for Phase 5 YAML rules
    - tests/.gitkeep — placeholder for integration test directory
    - Cargo.lock — committed for reproducibility
  modified:
    - .gitignore — appended Rust build artifacts (/target, *.rs.bk, .idea/, .vscode/, *.swp)

key-decisions:
  - "serde-saphyr 0.0.26 confirmed as YAML parser (serde_yaml is deprecated 0.9.34+deprecated)"
  - "WAVE-0 FINDING: serde_saphyr::Value does NOT exist; PLAN-03 must use TopLevelKeyProbe with Option<serde::de::IgnoredAny> for D-17 content dispatch"
  - "starlark 0.13 compiles under workspace MSRV 1.80 — confirmed by Wave 0 smoke test"
  - "smallvec pinned to '1' (stable 1.14.0); NOT 2.x alpha"
  - "signal-hook declared in [workspace.dependencies] and lacon-core [dependencies] so PLAN-05 inherits without editing either Cargo.toml"
  - "Cargo.lock committed (not gitignored) — CLI binary requires reproducible builds"
  - "release profile: opt-level=z + lto=thin + panic=abort + strip=symbols — binary is 331K, produces correct output"

patterns-established:
  - "Pattern 1: Workspace dep inheritance — all crates use version.workspace = true + dep = { workspace = true }"
  - "Pattern 2: Exhaustive pre-declaration — root Cargo.toml is the sole owner of [workspace.dependencies]; Wave-1+ plans never edit it"
  - "Pattern 3: D-17 content dispatch — TopLevelKeyProbe struct with serde::de::IgnoredAny, not a generic Value type"
  - "Pattern 4: Stub modules with fill comments — each stub identifies the PLAN-NN that will implement it"

requirements-completed: []

# Metrics
duration: 11min
completed: 2026-05-06
---

# Phase 1 Plan 01: Workspace Scaffolding + Dependency Declaration Summary

**Three-crate Cargo workspace with exhaustive v1 dependency declaration, stub module skeletons, and Wave 0 smoke tests resolving the serde-saphyr Value API and starlark MSRV open questions**

## Performance

- **Duration:** 11 min
- **Started:** 2026-05-06T07:43:35Z
- **Completed:** 2026-05-06T07:54:35Z
- **Tasks:** 3
- **Files created:** 22 (including Cargo.lock)

## Accomplishments

- Complete Cargo workspace with 3 crates (`lacon-core`, `lacon-cli`, `lacon-adapter-claudecode`) — `cargo check --workspace` green
- All 14 v1 production dependencies declared exhaustively in `[workspace.dependencies]`; Wave-1+ plans (PLAN-02..PLAN-05) inherit without editing either `Cargo.toml`
- Release build (`cargo build --release --workspace`) produces 331K `target/release/lacon` binary that prints `lacon v0.1.0 (skeleton — Phase 1 in progress)`
- Wave 0 smoke tests pass (2/2): starlark 0.13 compiles under MSRV 1.80; serde-saphyr dispatch approach confirmed
- WAVE-0 FINDING documented: `serde_saphyr::Value` does not exist in 0.0.26 — PLAN-03 must use `TopLevelKeyProbe` with `Option<serde::de::IgnoredAny>` for D-17 content dispatch

## Task Commits

Each task was committed atomically:

1. **Task 1: Workspace root Cargo.toml + rust-toolchain.toml + .gitignore** - `3a22b68` (chore)
2. **Task 2: Three member crates with manifests + module skeleton** - `6b5248d` (feat)
3. **Task 3: Wave 0 smoke tests + serde-saphyr finding** - `7d38435` (test)

## Files Created/Modified

- `Cargo.toml` — workspace root with [workspace.dependencies] exhaustive v1 dep set and [profile.release]
- `rust-toolchain.toml` — pins stable channel with rustfmt + clippy
- `.gitignore` — appended Rust build artifacts (no Cargo.lock exclusion)
- `Cargo.lock` — committed for reproducibility
- `crates/lacon-core/Cargo.toml` — all Wave-1+ deps; PLAN-02..PLAN-05 must NOT modify
- `crates/lacon-core/src/lib.rs` — 7 module declarations
- `crates/lacon-core/src/{error,config/mod,pipeline/mod,pipeline/stages,rules/mod,rules/schema,rules/loader,rules/bundled,starlark_host/mod,runtime/mod,validate/mod}.rs` — stubs with PLAN-NN fill comments
- `crates/lacon-core/tests/wave0_smoke.rs` — Wave 0 smoke tests (2 passing)
- `crates/lacon-cli/Cargo.toml` — binary target manifest with [[bin]] name = "lacon"
- `crates/lacon-cli/src/main.rs` — skeleton main() printing v0.1.0 marker
- `crates/lacon-adapter-claudecode/Cargo.toml` — stub adapter manifest
- `crates/lacon-adapter-claudecode/src/lib.rs` — ClaudeCodeAdapterStub
- `bundled-rules/.gitkeep` — placeholder for Phase 5 YAML rules
- `tests/.gitkeep` — placeholder for integration tests

## Decisions Made

- **serde_saphyr::Value does not exist** — confirmed by Wave 0 smoke test; PLAN-03 must use `TopLevelKeyProbe` struct with `Option<serde::de::IgnoredAny>` fields for D-17 content dispatch. Pattern validated in `wave0_smoke.rs::smoke_serde_saphyr_value_dispatch`. Finding annotated in `crates/lacon-core/src/rules/loader.rs`.
- **starlark 0.13 MSRV compatibility** — confirmed. Compiles and parses trivial `process(ctx, lines)` function under rustc 1.94.1 (workspace MSRV 1.80). PLAN-04 can proceed without MSRV bump.
- **Cargo.lock committed** (not gitignored) — CLI binary requires reproducible builds per plan spec.
- **smallvec pinned to "1"** (not 2.x alpha per RESEARCH.md critical note).
- **signal-hook declared in workspace** — PLAN-05 inherits via `{ workspace = true }` without touching either Cargo.toml.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed smoke test: serde_saphyr::Value does not exist**
- **Found during:** Task 3 (Wave 0 smoke tests)
- **Issue:** RESEARCH.md Assumption A5 assumed `serde_saphyr::Value` exists (mirroring serde_yaml::Value). serde-saphyr 0.0.26 is a typed-serde-only layer with no generic Value enum.
- **Fix:** Rewrote `smoke_serde_saphyr_value_dispatch` to use `TopLevelKeyProbe` struct with `Option<serde::de::IgnoredAny>` for key presence detection — the fallback path described in RESEARCH.md Open Question 1. Annotated `loader.rs` with WAVE-0 FINDING for PLAN-03.
- **Files modified:** `crates/lacon-core/tests/wave0_smoke.rs`, `crates/lacon-core/src/rules/loader.rs`
- **Verification:** Both smoke tests pass (`cargo test --workspace --test wave0_smoke`)
- **Committed in:** `7d38435` (Task 3 commit)

---

**Total deviations:** 1 auto-fixed (Rule 1 - bug in smoke test assumption)
**Impact on plan:** Auto-fix was necessary — the RESEARCH.md note itself said "if smoke test fails, fall back to saphyr-fallback". This is exactly the fallback path, now validated and documented for PLAN-03. No scope creep.

## Wave 0 Open Questions Resolved

| Question | Resolution | Evidence |
|----------|-----------|---------|
| Q1: serde-saphyr Value API | **FALLBACK PATH**: `serde_saphyr::Value` does not exist; use `TopLevelKeyProbe` with `IgnoredAny` | `wave0_smoke.rs::smoke_serde_saphyr_value_dispatch` passes |
| Q2: starlark 0.13 MSRV | **CONFIRMED**: compiles under MSRV 1.80, tested with rustc 1.94.1 | `wave0_smoke.rs::smoke_starlark_module_parses` passes |

## Metrics

| Metric | Value |
|--------|-------|
| Cargo.lock dependency count | ~197 crates (full transitive closure) |
| Release binary size | 331K (`target/release/lacon`) |
| `cargo check --workspace` | 0 warnings, 0 errors (with RUSTFLAGS=-Dwarnings) |
| `cargo test --workspace` | 2 passed, 0 failed |
| signal-hook in [workspace.dependencies] | Yes — PLAN-05 inherits without Cargo.toml edits |
| signal-hook in lacon-core [dependencies] | Yes — declared so PLAN-05 uses `{ workspace = true }` |

## Issues Encountered

None beyond the serde-saphyr Value API finding, which was anticipated as a possible outcome in RESEARCH.md and handled via the pre-planned fallback path.

## Next Phase Readiness

- PLAN-02 (pipeline primitives): Module skeletons exist, all deps (`regex`, `smallvec`) declared and inherited. Fill `crates/lacon-core/src/pipeline/stages.rs` and `pipeline/mod.rs`.
- PLAN-03 (rule loading + config): All deps (`serde`, `serde-saphyr`, `rust-embed`, `etcetera`, `thiserror`) declared. **CRITICAL**: Use `TopLevelKeyProbe` pattern (not `serde_saphyr::Value`) for D-17 dispatch — see `loader.rs` WAVE-0 FINDING annotation.
- PLAN-04 (Starlark): `starlark` dep declared; MSRV compatibility confirmed. Fill `starlark_host/mod.rs`.
- PLAN-05 (runtime): `os_pipe`, `crossbeam-channel`, `nix`, `signal-hook` all declared in `lacon-core/Cargo.toml`. Fill `runtime/mod.rs`.
- PLAN-06 (CLI wiring): `clap`, `anyhow` in `lacon-cli/Cargo.toml`. Fill `main.rs`.
- No blockers. All Wave-1+ plans can proceed in parallel.

---
*Phase: 01-engine-core-lacon-run-wrapper*
*Completed: 2026-05-06*
