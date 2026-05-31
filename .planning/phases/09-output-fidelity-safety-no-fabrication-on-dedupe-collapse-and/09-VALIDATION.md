---
phase: 9
slug: output-fidelity-safety-no-fabrication-on-dedupe-collapse-and
status: approved
nyquist_compliant: true
wave_0_complete: true
created: 2026-05-31
---

# Phase 9 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (Rust workspace, stable toolchain) + `assert_cmd` for CLI integration |
| **Config file** | none — workspace `Cargo.toml`; fixtures under `tests/fixtures/<rule-id>/<scenario>/` |
| **Quick run command** | `cargo test -p lacon-core stages` / `cargo test -p lacon-adapter-claudecode` |
| **Full suite command** | `cargo build --workspace && cargo test --workspace` |
| **Estimated runtime** | ~60–120 seconds (full workspace) |

> Note (from CLAUDE.md): a debug build of the workspace MUST precede `cargo test --workspace`
> on a fresh tree — `assert_cmd::cargo_bin` resolves `test_emitter` / `lacon-claude-hook`
> from `target/debug/`. The "Full suite command" above already chains the build.

---

## Sampling Rate

- **After every task commit:** Run the relevant `cargo test -p <crate> <substring>` quick command
- **After every plan wave:** Run `cargo build --workspace && cargo test --workspace`
- **Before `/gsd-verify-work`:** Full suite must be green + `cargo clippy --workspace --all-targets`
- **Max feedback latency:** ~120 seconds

---

## Per-Task Verification Map

> Plan IDs/tasks are filled in once plans exist; this maps the three requirements to test types.

| Requirement | Behavior under test | Test Type | Automated Command | Status |
|-------------|---------------------|-----------|-------------------|--------|
| REQ-engine-streaming-primitives | `collapse_repeated` emits only verbatim survivors + a single `[lacon: …]` elision marker (no substituted/placeholder line) | unit + fixture | `cargo test -p lacon-core stages` | ⬜ pending |
| REQ-engine-streaming-primitives | `dedupe` remains verbatim-only (regression guard) | unit | `cargo test -p lacon-core dedupe` | ⬜ pending |
| REQ-engine-streaming-primitives | fixture: aligned/tabular + repeated-prefix input → every surviving line byte-identical to an input line | fixture triple | `cargo test -p lacon-core --test fixtures` (or rule fixture harness) | ⬜ pending |
| REQ-adapter-bypass-detection | inline `LACON_DISABLE=1 <cmd>` env-prefix on the Bash hook command → PassThrough decision (no wrap) | unit | `cargo test -p lacon-adapter-claudecode bypass` | ⬜ pending |
| REQ-adapter-bypass-detection | interaction with chain split (`&&`/`||`/`;`) and `is_wrap_safe` — leading-assignment scan precedes wrap | unit | `cargo test -p lacon-adapter-claudecode` | ⬜ pending |
| REQ-engine-bypass | `run_bypassed` byte-exact passthrough backstop (stdout == unwrapped cmd stdout) | integration | `cargo test -p lacon-cli` (assert_cmd) | ⬜ pending |
| REQ-engine-bypass | end-to-end: hook PassThrough for inline prefix → no filtering applied | integration | `cargo test -p lacon-adapter-claudecode -- --include-ignored` (if real subprocess) | ⬜ pending |
| (re-audit) | bundled `git-status` success-path fixture no longer loses verbatim signal lines to collapse | fixture | `cargo test -p lacon-core bundled` (reduction-floor handling decided by planner) | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] New/updated fixture triples under `tests/fixtures/git-status/` (and any other rule using collapse_repeated) — aligned/tabular + repeated-prefix `input`/`expected`/`meta`
- [ ] New fixture(s) proving every surviving line is byte-identical to an input line (no-fabrication class fixtures)
- [ ] Bypass-detection unit test scaffolding in `crates/lacon-adapter-claudecode`

*Existing `cargo test` + `assert_cmd` + `test_emitter` infrastructure covers the harness; only fixtures/tests are new.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Live Claude Code Bash tool actually passes through `LACON_DISABLE=1 <cmd>` | REQ-adapter-bypass-detection | Requires the real Claude Code hook runtime; CI is hermetic and does not run the assistant | Reproduce the 2026-05-31 validation: issue `LACON_DISABLE=1 git status` from the Bash tool and confirm unfiltered output. (Automated proxy: hook-level unit test asserting PassThrough.) |

*All other phase behaviors have automated verification via cargo test + assert_cmd fixtures.*

---

## Validation Sign-Off

- [x] All tasks have an automated `cargo test` verify or a Wave 0 fixture dependency
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all new fixtures/tests
- [x] No watch-mode flags
- [x] Feedback latency < 120s
- [x] `nyquist_compliant: true` set in frontmatter (after planner maps tasks)

**Approval:** approved 2026-05-31
