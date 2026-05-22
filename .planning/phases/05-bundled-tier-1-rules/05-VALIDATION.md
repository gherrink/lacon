---
phase: 5
slug: bundled-tier-1-rules
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-22
---

# Phase 5 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` + cargo test harness (no external test crate; `insta` declared but unused — do NOT introduce, D-09) |
| **Config file** | none — cargo auto-discovers `crates/lacon-core/tests/*.rs` |
| **Quick run command** | `cargo test --test bundled_rules` |
| **Full suite command** | `cargo test` (workspace) |
| **Estimated runtime** | ~2 seconds (subprocess-free byte replay — no tool spawns) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --test bundled_rules`
- **After every plan wave:** Run `cargo test` (full workspace — ensures no regression in Phase 1–4 suites)
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** ~5 seconds

---

## Per-Task Verification Map

> Task IDs are provisional until the planner finalizes plan decomposition. Both phase requirements are covered by the single fixture-walking integration test, which is created in Wave 0 and grows green as each rule's fixtures land.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| W0: test runner + meta schema | — | 0 | REQ-bundled-rules-format | T-5-V5 / — | Rule YAML validated at load; regex linear-time (no ReDoS) | integration | `cargo test --test bundled_rules` | ❌ W0 (new file) | ⬜ pending |
| Per-rule YAML + fixtures (×10) | — | 1+ | REQ-bundled-rules-tier1 | — | ≥50% reduction on primary success, zero error-line drops | integration (fixture-walk) | `cargo test --test bundled_rules` | ❌ W0 | ⬜ pending |
| meta.yaml `exit_code` field (D-02) | — | 0 | REQ-bundled-rules-format | — | Failure fixtures route through `on_error` (ADR-0010) not success pipeline | integration | `cargo test --test bundled_rules` | ❌ W0 | ⬜ pending |
| docs/specs/testing-rules.md schema update (D-02) | — | 0 | REQ-bundled-rules-format | — | meta.yaml schema documents `exit_code` | manual doc check | `grep exit_code docs/specs/testing-rules.md` | ❌ W0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `crates/lacon-core/tests/bundled_rules.rs` — the fixture-walking runner (D-01/D-04/D-05/D-09); does not exist yet
- [ ] `tests/fixtures/<rule-id>/<scenario>/` trees — 10 rules × ≥2 scenarios (`input.txt`, `expected.txt`, `meta.yaml`); none exist yet (`tests/fixtures/` has only `primitives/`)
- [ ] `bundled-rules/*.yaml` — 10 rule files (+ optional `test-base.yaml`); dir has only `.gitkeep`
- [ ] `docs/specs/testing-rules.md` — add `exit_code` to meta.yaml schema (D-02)
- [ ] Framework install: none — cargo harness already present

*The integration test is itself a Wave 0 deliverable. It walks an empty/partial fixture tree and goes green incrementally as each rule's fixtures are authored — so the runner + meta-schema (incl. `exit_code`) MUST land before any rule fixtures can be asserted.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real captured fixtures for `tsc`/`eslint`/`vitest`/`jest` are genuine tool output (not hand-synthesized) | REQ-bundled-rules-format (D-03) | These four tools are NOT installed; executor must capture via `npx` in a throwaway node project during execution | Run `npx tsc`/`npx eslint`/`npx vitest run`/`npx jest` against a tiny fixture project, save merged stdout+stderr to `input.txt`, regenerate `expected.txt` via the rule pipeline |
| Each rule has a matching note in `docs/bundled-rules-roadmap.md` | REQ-bundled-rules-format | Doc cross-reference, not code | Confirm all 10 rule ids appear in the Tier 1 table |

*All byte-reduction and error-preservation behaviors have automated verification via the fixture-walk test.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags (the four JS test runners must use one-shot invocations: `vitest run`, `jest` without `--watch`)
- [ ] Feedback latency < 5s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
