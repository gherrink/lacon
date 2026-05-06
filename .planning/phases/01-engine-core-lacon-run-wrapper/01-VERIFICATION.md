---
phase: 01-engine-core-lacon-run-wrapper
verified: 2026-05-06T12:00:00Z
status: gaps_found
score: 4/5
overrides_applied: 0
gaps:
  - truth: "`lacon validate <path>` rejects invalid regex / unknown primitive / circular `extends` / missing referenced Starlark file at load time without falling back to defaults"
    status: failed
    reason: "`validate_file()` in `crates/lacon-core/src/validate/mod.rs` calls only `parse_one()` (serde schema parse) for rule files — it does NOT call `compile_resolved()` which is where regex compilation, script path validation, and circular-extends detection happen. Confirmed: `lacon validate` exits 0 for a rule with `drop_regex: '['` (invalid regex) and for a rule referencing a missing Starlark script. The `RuleLoader::resolve()` path correctly rejects these (unit-tested in `rules_loader.rs`), but the `lacon validate` CLI command does not invoke that path."
    artifacts:
      - path: "crates/lacon-core/src/validate/mod.rs"
        issue: "`validate_rule()` calls `parse_one()` only; must also call `compile_resolved()` (or equivalent) to catch InvalidRegex, MissingScriptFile, and circular extends at the validate command boundary"
      - path: "crates/lacon-cli/src/commands/validate.rs"
        issue: "Calls `lacon_core::validate::validate_file(path)` which does not do the full compile pass; no alternative compile path wired"
    missing:
      - "Wire `compile_resolved()` (or a standalone regex-compile + script-path-check pass) into the `validate_rule()` code path in `validate/mod.rs` so that `lacon validate` catches all four error categories specified in SC4: InvalidRegex, unknown primitive, CircularExtends, missing Starlark file"
      - "Add integration tests to `crates/lacon-core/tests/validate_dispatch.rs` and/or `crates/lacon-cli/tests/cli_validate.rs` asserting that `lacon validate` returns exit code 1 for each of the four invalid-rule fixture files"
---

# Phase 1: Engine core & `lacon run` wrapper — Verification Report

**Phase Goal:** A `lacon` binary that, given a YAML rule, can spawn a subprocess, merge stderr into stdout, run the streaming pipeline (or `on_error` on non-zero exit), enforce the `max_bytes` cap, and write filtered output to its own stdout — everything downstream depends on this working.

**Verified:** 2026-05-06T12:00:00Z
**Status:** gaps_found
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths (ROADMAP Success Criteria)

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `lacon run --rule <id> -- <cmd>` spawns subprocess, captures merged stdout+stderr, applies pipeline, writes filtered bytes to stdout, exits with subprocess exit code | VERIFIED | `Runner::run()` in `crates/lacon-core/src/runtime/mod.rs`: `os_pipe` merge, `BufReader::read_until` reader thread, exit code propagation (`status.code()` or `128+sig`). E2E tests pass (e.g. `exit_code_propagated_unchanged` confirms exit 7 round-trips). Behavioral spot-check: `lacon run -- sh -c 'exit 2'` exits 2. |
| 2 | All ten native primitives operate as line-by-line streaming transformers individually round-trippable through fixture-based unit tests | VERIFIED | All 10 variants in `Stage` enum in `pipeline/stages.rs`. Ten fixture tests in `crates/lacon-core/tests/primitives.rs` each run against `tests/fixtures/primitives/<name>/input.txt` + `expected.txt`. 135 tests pass (0 failures). Inline unit tests cover edge cases (truncation marker format, dedupe max_kept, KeepAroundMatch overlapping windows, etc.). |
| 3 | `on_error` block fully replaces the success pipeline when subprocess exits non-zero, success buffer discarded | VERIFIED | `runtime/mod.rs` lines 305-322: on exit_code == 0 runs `success_pipeline`; on non-zero, if `on_error_pipeline` is `Some`, runs that instead; `raw_buffer` discarded in both branches (never written to sink before the choice). Tests `on_error_swap_runs_on_non_zero_exit` and `success_buffer_discarded_on_non_zero_exit` confirm swap semantics. E2E test `end_to_end_on_error_swap_with_failing_subprocess` confirms from CLI. |
| 4 | `lacon validate <path>` accepts both rule files and `config.yaml` files, dispatches by content (`id`+`match` → rule), rejects invalid regex / unknown primitive / circular `extends` / missing referenced Starlark file at load time | FAILED | Dispatch by content works correctly (TopLevelKeyProbe pattern, verified by `dispatch_by_content_not_filename` test). Unknown key in rule/config is rejected. Project `retention` block rejected. However: `validate_file()` calls only `parse_one()` for rule files — it does NOT call `compile_resolved()`. Manual test confirmed: `lacon validate invalid_regex.yaml` exits 0 (should exit 1 with InvalidRegex). `lacon validate missing_script.yaml` also exits 0 (should exit 1 with MissingScriptFile). The compile-time rejection of invalid regex and missing scripts only happens via `RuleLoader::resolve()`, not via the `lacon validate` CLI path. |
| 5 | `extends` prepends parent pipeline, inherits scalar fields child omits, flattens single-level chains, rejects cycles | VERIFIED | `flatten_extends_with_lookup()` and `merge_rules()` in `loader.rs` implement ADR-0012. `merge_rules()` prepends parent pipeline at line 424: `[parent_stages, child_stages].concat()`. Cycle detection via `HashSet<String>` visited set returns `ValidationError::CircularExtends`. Tests: `extends_prepends_parent_pipeline`, `extends_inherits_scalar_fields`, `extends_cycle_detected`, `implicit_max_bytes_injected_after_flatten`, `explicit_max_bytes_not_double_injected` all pass. |

**Score:** 4/5 truths verified

---

### Deferred Items

No items deferred to later phases. The SC4 gap is directly in Phase 1's scope.

---

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/lacon-core/src/pipeline/stages.rs` | Closed `enum Stage` with 10 variants, `step()` + `flush()` | VERIFIED | 793 lines; all 10 variants with inline state; no vtable indirection (D-05 honored) |
| `crates/lacon-core/src/pipeline/mod.rs` | `Pipeline::new` with KeepRegex OR-merge, `run()`, `run_with_post_process()` | VERIFIED | OR-merge via `merge_keep_regex_stages()`; `run_with_post_process` calls Starlark post after native |
| `crates/lacon-core/src/runtime/mod.rs` | `Runner::run`, subprocess spawn, stderr merge, on_error swap, bypass, signal forwarding | VERIFIED | 457 lines; os_pipe + crossbeam-channel + signal-hook; `LACON_DISABLE` bypass; D-12 signal forwarding |
| `crates/lacon-core/src/rules/loader.rs` | `RuleLoader` with lazy resolve, mtime cache, extends flatten, max_bytes injection | VERIFIED | D-14 lazy resolve, D-15 mtime cache, D-16 extends flatten, D-07 injection |
| `crates/lacon-core/src/rules/schema.rs` | `RuleFile` + `StageSpec` covering all YAML fields | VERIFIED | 11 `StageSpec` variants; `deny_unknown_fields` on all nested structs |
| `crates/lacon-core/src/starlark_host/mod.rs` | `StarlarkScript::parse()` + `run()`, hermetic Globals | VERIFIED | `Globals::standard()` only; no file loader; `set_loader` returns 0 matches |
| `crates/lacon-core/src/validate/mod.rs` | `validate_file()` with content dispatch, full error detection | PARTIAL | Dispatch works; but rule validation path does not call `compile_resolved()` — invalid regex and missing script files pass undetected (SC4 blocker) |
| `crates/lacon-cli/src/commands/run.rs` | `lacon run --rule <id> -- <cmd>`, exit code propagation | VERIFIED | Wires `RuleLoader::resolve()` + `Runner::run()` + `std::process::exit(exit_code)` |
| `crates/lacon-cli/src/commands/validate.rs` | `lacon validate <path>` calling full compile pass | PARTIAL | Calls `validate_file()` which only does schema-level parse for rules; regex compilation not triggered |
| `crates/lacon-cli/src/cli.rs` | 6-subcommand clap surface | VERIFIED | Exactly 6 subcommands; `cli_surface_exposes_exactly_six_subcommands` test passes |
| `bin/test_emitter/src/main.rs` | Deterministic stdout+stderr emitter for integration tests | VERIFIED | `--stdout-lines`, `--stderr-lines`, `--ansi`, `--errors`, `--exit`, `--bytes` flags |
| `benches/cold_start.rs` | Cold-start probe with documented measurements | VERIFIED | Probe exists; measurements in `docs/architecture.md`: 1154 µs median for `--version`, 1259 µs for `validate <rule>` — both under 10ms budget |
| `tests/fixtures/primitives/` | Fixture files for all 10 primitives | VERIFIED | All 10 primitives have `input.txt` + `expected.txt` at workspace root |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `Runner::run` | subprocess stdout+stderr | `os_pipe` + single write-end cloned | WIRED | Lines 167-174; `writer` and `writer_clone` both connected to same pipe |
| `Runner::run` | pipeline exit-code branch | `if exit_code == 0 { success } else { on_error }` | WIRED | Lines 305-322 |
| `Runner::run` | `Stage::MaxBytes` truncation marker | Pipeline's `MaxBytes` stage; runtime scans output for `[lacon: truncated, ` | WIRED | Line 328 detection; D-08 comment confirms Stage::MaxBytes is sole enforcement point |
| `Runner::run` | signal forwarding | `install_signal_forwarder(child_pid)` → `nix::kill` | WIRED | Lines 197 + 416-448; `signal_hook::iterator::Signals::pending()` poll |
| `Runner::run_bypassed` | bypass on `LACON_DISABLE=1` | `std::env::var("LACON_DISABLE")` check at entry | WIRED | Lines 157-159 |
| `Pipeline::run_with_post_process` | Starlark `post_process` | `StarlarkScript::run(ctx, aggregated)` | WIRED | `pipeline/mod.rs` lines 127-138 |
| `validate_file` | dispatch by content | `TopLevelKeyProbe` + `has_id && has_match` | WIRED | `validate/mod.rs` lines 51-58 |
| `validate_file` → rule validator | `compile_resolved()` | NOT called — `parse_one()` only | NOT_WIRED | SC4 gap: invalid regex / missing scripts not caught by `lacon validate` |
| `RuleLoader::resolve` | implicit max_bytes injection | `compile_pipeline()` line 544: `push(Stage::MaxBytes { cap: defaults_max_bytes, ..})` | WIRED | Both success and on_error pipelines get injection independently (D-07) |
| `extends` flattening | cycle detection | `HashSet<String> visited` + `CircularExtends` error | WIRED | `loader.rs` lines 372-383 |

---

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|-------------------|--------|
| `Runner::run` | `raw_buffer: Vec<String>` | `os_pipe` reader thread via `crossbeam_channel` | Yes — real subprocess stdout+stderr bytes | FLOWING |
| `Pipeline::run` | `output: Vec<String>` | `Stage::step()` line-by-line transforms on `raw_buffer` | Yes | FLOWING |
| `StarlarkScript::run` | `aggregated: Vec<String>` | `Pipeline::run()` output | Yes | FLOWING |
| `RunOutcome::exit_code` | `status.code()` or `128+sig` | `child.wait()` | Yes | FLOWING |

---

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Subprocess exit code propagated | `/path/to/lacon run -- sh -c 'exit 2'` | Exit 2 | PASS |
| Unknown subcommand rejected | `/path/to/lacon unknown-subcommand` | Exit 2, clap error | PASS |
| Valid rule validates clean | `/path/to/lacon validate /tmp/test_rule.yaml` | Exit 0 | PASS |
| Invalid regex NOT caught by validate | `/path/to/lacon validate invalid_regex.yaml` | Exit 0 (should be 1) | FAIL — SC4 gap |
| Missing script NOT caught by validate | `/path/to/lacon validate missing_script.yaml` | Exit 0 (should be 1) | FAIL — SC4 gap |
| All tests pass | `cargo test --workspace` | 135 passed, 1 ignored | PASS |

---

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|-------------|-------------|--------|----------|
| REQ-engine-streaming-primitives | PLAN-02 | 10 native primitives as line-by-line transformers | SATISFIED | `stages.rs` + 10 fixture tests in `primitives.rs` all pass |
| REQ-engine-starlark-postprocess | PLAN-04 | `post_process` on aggregated output, hermetic | SATISFIED | `starlark_host/mod.rs`; `Globals::standard()` only; 4 inline tests pass |
| REQ-engine-rule-loading | PLAN-03 | Three-layer walk, first-match-wins | SATISFIED | `RuleLoader::resolve()` + `load_all()`; layer walk confirmed by `bundled_layer_fallback` test |
| REQ-engine-extends | PLAN-03 | Append-only inheritance, cycle detection | SATISFIED | `flatten_extends_with_lookup()` + 5 extends integration tests pass |
| REQ-engine-on-error | PLAN-05 | `on_error` replaces success pipeline on non-zero exit | SATISFIED | `runtime/mod.rs` lines 305-322; 3 on_error integration tests + 1 e2e test pass |
| REQ-engine-rewrite | PLAN-03 | `rewrite.add_flags / remove_flags / replace_flags` parsed; application is adapter's responsibility | SATISFIED (schema only) | `RewriteSpec` in `schema.rs`; scalar field inherited via extends; application deferred to Phase 3 adapter per ADR-0006 and CONTEXT.md — this is by design |
| REQ-engine-bypass | PLAN-05 | `LACON_DISABLE=1` bypass | SATISFIED | `run_bypassed()` path; `lacon_disable_bypasses_filtering` + e2e bypass test pass |
| REQ-engine-max-bytes-cap | PLAN-02/03 | Hard cap with byte-exact marker, implicit injection | SATISFIED | D-07 implicit injection in `compile_pipeline()`; D-08 marker in `Stage::MaxBytes::step()`; W3 fix confirmed in comments |
| REQ-cli-run | PLAN-06 | `lacon run --rule <id> -- <cmd>`, exit code | SATISFIED | `commands/run.rs` fully wired; e2e tests pass |
| REQ-cli-validate | PLAN-06 | `lacon validate <path>` dispatches by content | BLOCKED | Dispatch works; BUT invalid regex / missing script not caught (SC4 gap). Requirement text says "rejects invalid regex ... at load time" — `lacon validate` does not do this |

---

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `crates/lacon-core/src/validate/mod.rs` | 112-125 | `validate_rule()` comment explicitly notes "Full compile (regex, script paths) would require RuleLoader with layer context — that's the `lacon validate` full path wired in PLAN-06" but PLAN-06 did NOT wire the full compile path | BLOCKER | `lacon validate` exits 0 for rules with invalid regex or missing script files — violates SC4 |
| `crates/lacon-cli/src/commands/init.rs` | 3-5 | Phase 3 stub printing "not yet implemented" | INFO | Expected — these are placeholder stubs for Phase 3/4; not part of Phase 1 scope |
| `crates/lacon-cli/src/commands/stats.rs` | 3-5 | Phase 4 stub | INFO | Expected — Phase 4 scope |
| `crates/lacon-cli/src/commands/explain.rs` | (same) | Phase 4 stub | INFO | Expected — Phase 4 scope |
| `crates/lacon-cli/src/commands/doctor.rs` | (same) | Phase 4 stub | INFO | Expected — Phase 4 scope |

---

### Human Verification Required

None. All gaps are programmatically verifiable.

---

### Gaps Summary

One gap blocks goal achievement:

**SC4 — `lacon validate` does not catch invalid regex or missing Starlark scripts**

The `validate_file()` function in `crates/lacon-core/src/validate/mod.rs` routes rule-file validation through `parse_one()` (serde schema parse only). The comment in the code says "Full compile (regex, script paths) would require RuleLoader with layer context — that's the `lacon validate` full path wired in PLAN-06", but PLAN-06's `validate.rs` only calls `validate_file()`, which has this incomplete path. The result is:

- `lacon validate` on a rule with `drop_regex: '['` (syntactically valid YAML, invalid regex) → exit 0. Expected: exit 1, error `InvalidRegex`.
- `lacon validate` on a rule with `post_process: { path: does_not_exist.star, function: process }` → exit 0. Expected: exit 1, error `MissingScriptFile`.

The `RuleLoader::resolve()` path DOES reject these correctly (confirmed by `invalid_regex_rejected` and `missing_script_rejected` unit tests), but the `lacon validate` command bypasses that path entirely.

**Root cause:** `validate/mod.rs::validate_rule()` needs to call `compile_pipeline()` (or `compile_resolved()`) after the serde schema parse to perform regex compilation and script path checks. Alternatively, the CLI `validate` command could use `RuleLoader::load_all()` (the eager path) on the provided file path to get full compile-time validation.

All other phase goals are fully achieved:
- The `lacon run` binary correctly spawns subprocesses, merges stderr, runs the pipeline, handles on_error swap, enforces max_bytes via Stage::MaxBytes (W3 fix confirmed), and propagates exit codes.
- All 10 native primitives are streaming line-by-line transformers with fixture-based round-trip tests.
- The extends mechanism correctly prepends, inherits, flattens, and detects cycles.
- Cold-start is ~1.2ms (under 10ms budget). Tests: 135 passed, 0 failed.

---

_Verified: 2026-05-06T12:00:00Z_
_Verifier: Claude (gsd-verifier)_
