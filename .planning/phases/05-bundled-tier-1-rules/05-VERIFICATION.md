---
phase: 05-bundled-tier-1-rules
verified: 2026-05-22T05:41:48Z
status: passed
score: 3/3
overrides_applied: 0
re_verification: null
---

# Phase 5: Bundled Tier 1 Rules — Verification Report

**Phase Goal:** Ten Tier 1 YAML rules ship in `bundled-rules/` (pkg-install, cargo-build, cargo-test, vitest, jest, pytest, tsc, eslint, git-status, docker-build), each with a success-path fixture and a failure-path fixture under `tests/fixtures/<rule-id>/<scenario>/`, and an integration test (`cargo test --test bundled_rules`) asserting ≥50% reduction with zero error-line drops — all hermetically (CI installs none of the ten tools).
**Verified:** 2026-05-22T05:41:48Z
**Status:** passed
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | All ten Tier 1 rule files exist in `bundled-rules/` and load successfully via the resolver — each has a defined `match`, a non-empty `pipeline`, an `on_error` block, and uses only the ten native primitives (no `script:` in pipeline) | VERIFIED | `ls bundled-rules/*.yaml` shows 10 active rules + test-base.yaml; `lacon validate bundled-rules/<id>.yaml` exits 0 for all 10; `grep -r "script:"` finds nothing in any pipeline block |
| 2 | Each of the ten rules has at minimum one success-path fixture and one failure-path fixture under `tests/fixtures/<rule-id>/<scenario>/` with `input.txt`, `expected.txt`, `meta.yaml` (with `exit_code` field) | VERIFIED | `ls tests/fixtures/` shows 10 rule dirs, each with exactly 2 scenarios; all 20 meta.yaml files contain `command`, `tool_version`, `os`, `notes`, and the load-bearing `exit_code` field; success fixtures: 10 (exit_code: 0), failure fixtures: 10 (exit_code: 1, 2, 101, or 128) |
| 3 | `cargo test --test bundled_rules` walks 20 fixtures asserting byte-exact match, ≥50% reduction on non-exempt success fixtures, and must_keep_lines survival — subprocess-free (no tool installs) — and `cargo test --workspace` is fully green (0 failing) | VERIFIED | `cargo test --test bundled_rules` output: "1 passed; 0 failed; finished in 0.15s; asserted 20 fixture(s)"; `cargo test --workspace` output: 444 total tests, 0 failures; `bundled_rules.rs` contains no `Command::new`/`std::process`/`spawn` calls |

**Score:** 3/3 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `bundled-rules/cargo-build.yaml` | Rule with match, pipeline, on_error | VERIFIED | `lacon validate` exit 0; uses `strip_ansi`, `drop_regex`, `keep_around_match`, `keep_head` |
| `bundled-rules/cargo-test.yaml` | Rule extending test-base, on_error | VERIFIED | `extends: bundled/test-base`; 8-stage success pipeline + on_error with `keep_regex` |
| `bundled-rules/docker-build.yaml` | Rule with BuildKit noise drops | VERIFIED | Drops `#N CACHED/DONE/sha256/transferring` patterns; `keep_head` on error |
| `bundled-rules/eslint.yaml` | Rule with keep_around_match | VERIFIED | `keep_around_match` on ` (error\|warning) ` pattern, before:1 for file headers |
| `bundled-rules/git-status.yaml` | Rule with collapse_repeated | VERIFIED | `collapse_repeated` on `^\t` with max_kept:5; on_error keeps `^fatal:` |
| `bundled-rules/jest.yaml` | Rule extending test-base | VERIFIED | `extends: bundled/test-base`; drops `^PASS`, `^Snapshots:`, `^Time:`, `^Ran all test suites` |
| `bundled-rules/pkg-install.yaml` | Rule with NO rewrite block (D-11) | VERIFIED | No `rewrite:` key; comment documents D-11 rationale; `keep_head` on error |
| `bundled-rules/pytest.yaml` | Rule extending test-base | VERIFIED | `extends: bundled/test-base`; drops `PASSED\s+\[`, platform/cachedir/rootdir blocks |
| `bundled-rules/test-base.yaml` | Inert parent with strip_ansi + on_error | VERIFIED | Sentinel regex `^__lacon_test_base_never_matches__$`; comment corrected per WR-02 to not claim enforced inertness |
| `bundled-rules/tsc.yaml` | Rule with dedupe + keep_tail | VERIFIED | Comment documents WR-03: `args_prefix: []` intentionally matches all tsc invocations |
| `bundled-rules/vitest.yaml` | Rule extending test-base | VERIFIED | Drops `\(\d+ tests?\) \d+\s*ms$` per-file PASS lines; `keep_regex` on error |
| `crates/lacon-core/tests/bundled_rules.rs` | Subprocess-free fixture-walking integration test | VERIFIED | 209 lines; `RuleLoader::new(None)` → `Runner::filter_bytes`; no shell-out; three assertions (byte-exact, ≥50% reduction, must_keep_lines); skips `primitives/` subtree |
| `docs/testing-rules.md` | Documents `exit_code` meta.yaml field | VERIFIED | Line 48: `exit_code` schema documented with ADR-0010 routing explanation |
| `docs/bundled-rules-roadmap.md` | Per-rule doc notes for all 10 rules | VERIFIED | All 10 rule IDs appear with doc notes; pkg-install note explicitly states "No `rewrite` block" per D-11 |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `bundled_rules.rs` | `lacon_core::rules::loader::RuleLoader` | `RuleLoader::new(None).resolve(rule_id)` | WIRED | Line 80: `let mut loader = RuleLoader::new(None);` then `loader.resolve(rule_id)` |
| `bundled_rules.rs` | `lacon_core::runtime::Runner::filter_bytes` | `runner.filter_bytes(input, exit_code, 0, command, None)` | WIRED | Line 86: `runner.filter_bytes(input, exit_code, 0, command, None)` |
| `meta.yaml exit_code` | `filter_bytes` ADR-0010 branch | `exit_code: i32` in `FixtureMeta`, passed to `filter_bytes` | WIRED | `exit_code: 0` → success pipeline; nonzero → on_error (or raw passthrough when no on_error) |
| `cargo-test.yaml` | `test-base.yaml` | `extends: bundled/test-base` | WIRED | Loader's `load_all()` resolves bundled→bundled extends via `find_in_bundled`; produces 11 success stages on both lazy and eager paths |
| `bundled-rules/*.yaml` | Binary (embedded) | `include_dir!` macro | WIRED | `lacon validate` and `RuleLoader::new(None)` both resolve from embedded bundled layer without filesystem access |

### Data-Flow Trace (Level 4)

Not applicable — this phase delivers static YAML rules and a test runner, not a component rendering dynamic data. The data flow is: `input.txt` → `RuleLoader::new(None).resolve()` → `Runner::filter_bytes()` → compare to `expected.txt`. This flow is exercised live by `cargo test --test bundled_rules` and produces correct output (20 fixtures pass).

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Integration test walks all 20 fixtures | `cargo test --test bundled_rules -- --nocapture` | "asserted 20 fixture(s); 1 passed; 0 failed" | PASS |
| All 10 rules validate (exit 0) | `lacon validate bundled-rules/<id>.yaml` × 10 | All exit 0, no output | PASS |
| Full workspace test suite green | `cargo test --workspace` | 444 tests, 0 failures | PASS |
| No shelling out in bundled_rules.rs | `grep -n "Command::new\|std::process\|spawn" bundled_rules.rs` | No matches | PASS |
| No `script:` primitive in any bundled pipeline | `grep -r "script:" bundled-rules/` | No matches | PASS |

### Probe Execution

No `scripts/*/tests/probe-*.sh` probes declared for this phase. The integration test (`cargo test --test bundled_rules`) serves as the sole runnable verification contract and was executed directly.

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| REQ-bundled-rules-tier1 | 05-02 through 05-09 PLAN.md | Ten Tier 1 rules, ≥50% reduction, no error drops, ≥1 success + ≥1 failure fixture each | SATISFIED | 10 rule files load; `cargo test --test bundled_rules` asserts ≥50% on non-exempt success fixtures and must_keep_lines on all failure fixtures; 20 fixtures pass |
| REQ-bundled-rules-format | 05-01 through 05-09 PLAN.md | YAML file + fixture set (input/expected/meta) + integration test + roadmap doc note per rule | SATISFIED | All 4 format requirements met for all 10 rules; integration test passes; doc notes present in `bundled-rules-roadmap.md` |

**REQUIREMENTS.md traceability note:** Both REQ-bundled-rules-tier1 and REQ-bundled-rules-format are listed as "Pending" in REQUIREMENTS.md traceability table. This is a documentation status that should be updated to "Complete" — it does not indicate an implementation gap.

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| (none) | — | No TBD/FIXME/XXX/TODO/HACK/PLACEHOLDER found | — | — |

**Debt marker scan:** Zero unresolved debt markers across all phase-modified files (`bundled-rules/*.yaml`, `crates/lacon-core/tests/bundled_rules.rs`).

**Code review warnings (WR-01..04) — all addressed:**
- **WR-01** (keep_head over keep_tail in on_error): Fixed in test-base.yaml, cargo-build.yaml, docker-build.yaml, pkg-install.yaml — all now use `keep_head` with explanatory comment
- **WR-02** (test-base "INERT" false claim): Fixed — description now says "inertness is by convention here, not an enforced loader property"
- **WR-03** (tsc `args_prefix: []` breadth comment): Fixed — tsc.yaml comment now documents WR-03 and the intentional all-invocations behavior
- **WR-04** (pkg-install success-path `^warning ` drops peer-dep signal): Addressed with inline comment documenting the tradeoff

**INFO-level findings (IN-01..06):** All accepted as non-blocking for v1 — documented in 05-REVIEW.md.

### Minor Discrepancy: `captured_at` in 8 of 20 meta.yaml files

ROADMAP SC-2 lists `captured_at` as part of the meta.yaml shape alongside `command`, `tool_version`, `os`, `notes`. Eight of twenty fixtures omit this field:
- `cargo-test/clean-run`, `cargo-test/test-failure` (phases 05-02)
- `git-status/many-untracked`, `git-status/not-a-repo` (phase 05-05)
- `pkg-install/npm-deprecated`, `pkg-install/npm-e404` (phase 05-03)
- `pytest/assert-failure`, `pytest/verbose-pass` (phase 05-08)

**Assessment — not a blocker:** The PLAN explicitly designates `captured_at` as optional provenance ("tool_version/captured_at optional"). The FixtureMeta struct in bundled_rules.rs intentionally excludes `captured_at` from its fields (relying on `no deny_unknown_fields` to silently accept it when present). The integration test makes no assertion against `captured_at`. The missing field has no behavioral impact — it is metadata for human maintainers only. The 12 fixtures that do carry `captured_at` demonstrate the field is understood and used where authors captured real command output at a specific time. The SC-2 wording reflects the documentation template rather than a hard structural requirement.

### Human Verification Required

None. All phase-5 deliverables are mechanically verifiable via the integration test and static rule analysis. No visual UI, real-time behavior, or external service integration is involved.

## Gaps Summary

No gaps. All three roadmap success criteria are observably satisfied:

1. **SC-1 (10 rules load):** All 10 rules validate exit 0, use only native primitives, have `match`/`pipeline`/`on_error`, and resolve correctly from the embedded bundled layer.
2. **SC-2 (fixtures):** Each rule has exactly one success-path (exit_code: 0) and one failure-path (exit_code != 0) fixture with all load-bearing fields (`command`, `exit_code`, `tool_version`, `os`, `notes`). The optional `captured_at` field is present in 12/20 fixtures and absent in 8/20 — this is a documentation gap, not a functional gap.
3. **SC-3 (integration test):** `cargo test --test bundled_rules` asserts 20 fixtures with byte-exact match, ≥50% reduction on non-exempt success fixtures, and must_keep_lines survival — all without installing any of the ten tools. `cargo test --workspace` is fully green at 444 tests, 0 failures.

Code review found 0 Critical / 4 Warning / 6 Info. All 4 warnings were addressed in commit `079bd78` (fix(05): address code review WR-01..WR-04).

---

_Verified: 2026-05-22T05:41:48Z_
_Verifier: Claude (gsd-verifier)_
