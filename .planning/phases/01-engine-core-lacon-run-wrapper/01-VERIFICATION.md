---
phase: 01-engine-core-lacon-run-wrapper
verified: 2026-05-06T13:00:00Z
status: passed
score: 5/5
overrides_applied: 0
re_verification:
  previous_status: gaps_found
  previous_score: 4/5
  gaps_closed:
    - "`lacon validate <path>` rejects invalid regex / unknown primitive / circular `extends` / missing referenced Starlark file at load time without falling back to defaults"
  gaps_remaining: []
  regressions: []
---

# Phase 1: Engine core & `lacon run` wrapper — Verification Report

**Phase Goal:** A `lacon` binary that, given a YAML rule, can spawn a subprocess, merge stderr into stdout, run the streaming pipeline (or `on_error` on non-zero exit), enforce the `max_bytes` cap, and write filtered output to its own stdout — everything downstream depends on this working.

**Verified:** 2026-05-06T13:00:00Z
**Status:** passed
**Re-verification:** Yes — after SC4 gap closure (PLAN 01-08)

---

## Goal Achievement

### Observable Truths (ROADMAP Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `lacon run --rule <id> -- <cmd>` spawns subprocess, captures merged stdout+stderr, applies pipeline, writes filtered bytes to stdout, exits with subprocess exit code | VERIFIED | Unchanged from initial verification. `Runner::run()` in `runtime/mod.rs`: os_pipe merge, `BufReader::read_until` reader thread, exit code propagation. Behavioral spot-check confirmed in initial pass. |
| 2 | All ten native primitives operate as line-by-line streaming transformers individually round-trippable through fixture-based unit tests | VERIFIED | Unchanged. All 10 `Stage` variants; ten fixture tests in `crates/lacon-core/tests/primitives.rs` all pass. W3 truncation marker `[lacon: truncated, 510 more bytes dropped]` confirmed byte-exact in `tests/fixtures/primitives/max_bytes/expected.txt`. |
| 3 | `on_error` block fully replaces the success pipeline when subprocess exits non-zero, success buffer discarded | VERIFIED | Unchanged. `runtime/mod.rs` lines 305-322: exit_code == 0 runs success pipeline; non-zero runs on_error pipeline instead; buffer discarded before choice. |
| 4 | `lacon validate <path>` accepts both rule files and `config.yaml` files, dispatches by content (`id`+`match` → rule), rejects invalid regex / unknown primitive / circular `extends` / missing referenced Starlark file at load time | VERIFIED | **Gap closed by PLAN-08.** `validate_rule()` in `crates/lacon-core/src/validate/mod.rs` now calls full compile chain: `parse_one` → `flatten_extends_with_lookup` (same-directory parent lookup) → `compile_resolved`. Behavioral spot-checks (binary): `lacon validate invalid_regex.yaml` → exit 1, `InvalidRegex: regex parse error: [ ^ error: unclosed character class`; `lacon validate missing_script.yaml` → exit 1, `MissingScriptFile`; `lacon validate unknown_primitive.yaml` → exit 1, `ParseError: unknown variant 'reverse_lines'`; `lacon validate cycle_a.yaml` (with cycle_b in same dir) → exit 1, `CircularExtends`. Regression guard: `lacon validate valid_simple.yaml` → exit 0, empty stderr. 5 new CLI integration tests in `crates/lacon-cli/tests/cli_validate.rs` + 2 new library tests in `crates/lacon-core/tests/validate_dispatch.rs` all pass. |
| 5 | `extends` prepends parent pipeline, inherits scalar fields child omits, flattens single-level chains, rejects cycles | VERIFIED | Unchanged. `flatten_extends_with_lookup()` + `merge_rules()` in `loader.rs`. Cycle detection now also exercised by `sc4_validate_rejects_circular_extends` CLI test. |

**Score:** 5/5 truths verified

---

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/lacon-core/src/validate/mod.rs` | `validate_file()` with content dispatch, full compile-chain validation | VERIFIED | Full chain wired: `parse_one` → `flatten_extends_with_lookup` → `compile_resolved`. Old comment-acknowledged shortcut deleted. |
| `crates/lacon-core/src/rules/loader.rs` | `find_rule_in_dir` as `pub fn`, `DEFAULT_MAX_BYTES` as `pub const` | VERIFIED | Line 339: `pub fn find_rule_in_dir`. Line 45: `pub const DEFAULT_MAX_BYTES: usize = 32_768`. |
| `crates/lacon-core/tests/validate_dispatch.rs` | 2 new SC4 library tests | VERIFIED | `validate_file_rejects_invalid_regex` and `validate_file_rejects_missing_script` present and pass. |
| `crates/lacon-cli/tests/cli_validate.rs` | 5 new SC4 CLI tests (4 gap-closure + 1 regression) | VERIFIED | `sc4_validate_rejects_invalid_regex`, `sc4_validate_rejects_missing_script`, `sc4_validate_rejects_unknown_primitive`, `sc4_validate_rejects_circular_extends`, `sc4_regression_valid_rule_still_passes` all present and pass. |
| `crates/lacon-core/src/pipeline/stages.rs` | All 10 Stage variants; W3 truncation marker | VERIFIED | Unchanged; `#[allow(dead_code)]` on `stage_step_str` added by PLAN-08 for pre-existing lint. |
| `crates/lacon-core/src/runtime/mod.rs` | `Runner::run`, subprocess spawn, stderr merge, on_error swap, bypass, signal forwarding | VERIFIED | Unchanged. |
| `crates/lacon-core/src/rules/loader.rs` | `RuleLoader` with lazy resolve, mtime cache, extends flatten, max_bytes injection | VERIFIED | Unchanged core logic; `find_rule_in_dir` promoted to `pub`, `DEFAULT_MAX_BYTES` const added. |
| `crates/lacon-cli/src/commands/validate.rs` | `lacon validate <path>` calling full compile pass | VERIFIED | Calls `validate_file()` which now performs the full compile chain. |
| `crates/lacon-cli/src/cli.rs` | 6-subcommand clap surface | VERIFIED | Unchanged. |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `validate_rule` | `parse_one` | direct call (Step 1) | WIRED | `validate/mod.rs` line 133 |
| `validate_rule` | `flatten_extends_with_lookup` | same-directory parent lookup closure (Step 2) | WIRED | `validate/mod.rs` lines 148-160; `find_rule_in_dir` used as the lookup function |
| `validate_rule` | `compile_resolved` | direct call after flatten (Step 3) | WIRED | `validate/mod.rs` lines 164-172 |
| `validate_file` → rule validator | `compile_resolved` | via `validate_rule` | WIRED | SC4 gap closed; all four error categories caught |
| `Runner::run` | subprocess stdout+stderr | `os_pipe` + reader thread | WIRED | Unchanged |
| `Runner::run` | pipeline exit-code branch | `if exit_code == 0 { success } else { on_error }` | WIRED | Unchanged |
| `Runner::run` | `Stage::MaxBytes` truncation marker | Pipeline's `MaxBytes` stage | WIRED | Unchanged; W3 fixture confirmed byte-exact |
| `Pipeline::run_with_post_process` | Starlark `post_process` | `StarlarkScript::run(ctx, aggregated)` | WIRED | Unchanged |
| `RuleLoader::resolve` | implicit max_bytes injection | `compile_pipeline()` | WIRED | Unchanged |
| `extends` flattening | cycle detection | `HashSet<String> visited` + `CircularExtends` error | WIRED | Now also exercised via `lacon validate` CLI path |

---

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|-------------------|--------|
| `Runner::run` | `raw_buffer: Vec<String>` | `os_pipe` reader thread via `crossbeam_channel` | Yes | FLOWING |
| `Pipeline::run` | `output: Vec<String>` | `Stage::step()` line-by-line transforms | Yes | FLOWING |
| `StarlarkScript::run` | `aggregated: Vec<String>` | `Pipeline::run()` output | Yes | FLOWING |
| `RunOutcome::exit_code` | `status.code()` or `128+sig` | `child.wait()` | Yes | FLOWING |
| `validate_rule` | `Vec<ValidationError>` | `compile_resolved(flat, path, ...)` | Yes — real compile-time errors | FLOWING |

---

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| SC4-1: InvalidRegex caught | `lacon validate invalid_regex.yaml` | Exit 1, stderr: `<path>:0: InvalidRegex: regex parse error: [ ^ error: unclosed character class` | PASS |
| SC4-2: MissingScriptFile caught | `lacon validate missing_script.yaml` | Exit 1, stderr: `<path>:0: MissingScriptFile: Starlark script not found: ...does_not_exist.star` | PASS |
| SC4-3: UnknownPrimitive caught | `lacon validate unknown_primitive.yaml` | Exit 1, stderr: `<path>:5: ParseError: unknown variant 'reverse_lines', expected one of ...` | PASS |
| SC4-4: CircularExtends caught | `lacon validate cycle_a.yaml` (with cycle_b in same dir) | Exit 1, stderr: `<path>:0: CircularExtends: circular 'extends' chain: rule 'cycle-a' is already in the chain ["cycle-a", "cycle-b"]` | PASS |
| SC4-REGRESSION: valid rule still exits 0 | `lacon validate valid_simple.yaml` | Exit 0, empty stderr | PASS |
| Full workspace tests | `cargo test --workspace` | 147 passed, 1 ignored, 0 failed | PASS |
| Workspace clippy | `cargo clippy --workspace --all-targets -- -D warnings` | 0 errors, 0 lint warnings (1 cargo metadata note about test_emitter lib target — pre-existing, not a lint) | PASS |

---

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| REQ-engine-streaming-primitives | PLAN-02 | 10 native primitives as line-by-line transformers | SATISFIED | `stages.rs` + 10 fixture tests pass |
| REQ-engine-starlark-postprocess | PLAN-04 | `post_process` on aggregated output, hermetic | SATISFIED | `starlark_host/mod.rs`; `Globals::standard()` only |
| REQ-engine-rule-loading | PLAN-03 | Three-layer walk, first-match-wins | SATISFIED | `RuleLoader::resolve()` + `load_all()`; layer walk confirmed |
| REQ-engine-extends | PLAN-03 | Append-only inheritance, cycle detection | SATISFIED | `flatten_extends_with_lookup()` + extends integration tests pass |
| REQ-engine-on-error | PLAN-05 | `on_error` replaces success pipeline on non-zero exit | SATISFIED | `runtime/mod.rs` lines 305-322; on_error tests pass |
| REQ-engine-rewrite | PLAN-03 | `rewrite` spec parsed; application is adapter's responsibility | SATISFIED (schema only) | `RewriteSpec` in `schema.rs`; application deferred to Phase 3 per ADR-0006 — by design |
| REQ-engine-bypass | PLAN-05 | `LACON_DISABLE=1` bypass | SATISFIED | `run_bypassed()` path; bypass tests pass |
| REQ-engine-max-bytes-cap | PLAN-02/03 | Hard cap with byte-exact marker, implicit injection | SATISFIED | W3 fixture `[lacon: truncated, 510 more bytes dropped]` byte-exact confirmed; `DEFAULT_MAX_BYTES = 32_768` now a shared pub const |
| REQ-cli-run | PLAN-06 | `lacon run --rule <id> -- <cmd>`, exit code | SATISFIED | `commands/run.rs` fully wired; e2e tests pass |
| REQ-cli-validate | PLAN-06/08 | `lacon validate <path>` dispatches by content, full compile-time validation | SATISFIED | **Gap closed.** Full compile chain wired; all 4 SC4 error categories caught; 7 new tests pass (5 CLI + 2 library) |

---

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/lacon-cli/src/commands/init.rs` | 3-5 | Phase 3 stub printing "not yet implemented" | INFO | Expected — Phase 3 scope |
| `crates/lacon-cli/src/commands/stats.rs` | 3-5 | Phase 4 stub | INFO | Expected — Phase 4 scope |
| `crates/lacon-cli/src/commands/explain.rs` | 3-5 | Phase 4 stub | INFO | Expected — Phase 4 scope |
| `crates/lacon-cli/src/commands/doctor.rs` | 3-5 | Phase 4 stub | INFO | Expected — Phase 4 scope |

No blockers. The PLAN-08 commit also fixed two pre-existing clippy lints (`dead_code` on `stage_step_str` in `stages.rs`, `manual_str_repeat` in `test_emitter`) — no outstanding lint warnings remain.

---

### Human Verification Required

None. All phase-1 goals are programmatically verified.

---

### Gaps Summary

No gaps. All 5 ROADMAP success criteria are now verified.

The SC4 gap from the initial verification (2026-05-06) is closed:

- `validate_rule()` in `crates/lacon-core/src/validate/mod.rs` now calls `parse_one` → `flatten_extends_with_lookup` → `compile_resolved`, catching all four SC4 error categories at the `lacon validate` boundary.
- `find_rule_in_dir` is `pub fn` in `loader.rs` for cross-module reuse without code duplication.
- `DEFAULT_MAX_BYTES = 32_768` is a `pub const` in `loader.rs`, replacing the magic literal.
- 7 new tests added (5 CLI integration + 2 library), total 147 passing (was 135 at initial verification baseline, 140 before PLAN-08).
- Behavioral spot-checks on the actual binary confirm all four SC4 categories produce exit 1 with D-18-format errors; valid rules still exit 0 (regression guard).
- Commit `a71cb1c` on `main` contains all changes.

---

_Verified: 2026-05-06T13:00:00Z_
_Verifier: Claude (gsd-verifier)_
