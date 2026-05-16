---
phase: 3
slug: claude-code-adapter-lacon-init
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-16
---

# Phase 3 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (built-in) + `assert_cmd` 2.x + `predicates` 3.x (already in workspace dev-deps) |
| **Config file** | none (Cargo conventions) |
| **Quick run command** | `cargo test -p lacon-adapter-claudecode -p lacon-cli --tests` |
| **Full suite command** | `cargo test --workspace --all-targets` |
| **Estimated runtime** | ~30–60 seconds full suite (Phase 1 + 2 baseline) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p <touched-crate> --lib` (unit-only, fast — typically <5s)
- **After every plan wave:** Run `cargo test -p lacon-adapter-claudecode -p lacon-cli --tests` (integration)
- **Before `/gsd-verify-work`:** `cargo test --workspace --all-targets` must be green
- **Max feedback latency:** ~10 seconds for per-task unit runs; ~60 seconds for full suite

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 3-01-* | 01 | 1 | (scaffolding) | — | Workspace builds clean; no surface drift | unit | `cargo build --workspace` && `cargo test -p lacon-cli --test cli_surface` | ✅ cli_surface.rs exists | ⬜ pending |
| 3-02-* | 02 | 2 | REQ-adapter-chained-commands | T-injection-chain-reassembly | Chain reassembly preserves operators byte-exact; no eval-form leak | unit | `cargo test -p lacon-adapter-claudecode --test chain_split` | ❌ W0 | ⬜ pending |
| 3-03-* | 03 | 2 | REQ-adapter-tui-bypass | — | TUI commands not wrapped (avoids hang/garbled stdin) | unit | `cargo test -p lacon-adapter-claudecode --test tui_heuristic` | ❌ W0 | ⬜ pending |
| 3-03-* | 03 | 2 | (apply_rewrite invariant) | — | `apply(apply(x)) == apply(x)`; `argv[0]` never touched | unit | `cargo test -p lacon-core rules::rewrite::tests` | ❌ W0 | ⬜ pending |
| 3-03-* | 03 | 2 | (quote_for_shell) | T-quote-injection | POSIX round-trip via `/bin/sh`; embedded quote/`$()`/backticks/newline safe | unit | `cargo test -p lacon-adapter-claudecode quote::tests` | ❌ W0 | ⬜ pending |
| 3-04-* | 04 | 3 | REQ-adapter-pretooluse-only | T-hook-output-shape | `hookSpecificOutput.{hookEventName, permissionDecision: "allow", updatedInput}` shape locked; `description`/`timeout`/`run_in_background` echo-back | integration | `cargo test -p lacon-adapter-claudecode --test hook_e2e` | ❌ W0 | ⬜ pending |
| 3-04-* | 04 | 3 | REQ-adapter-bypass-detection | T-bypass-failsafe | `!!` prefix + `LACON_DISABLE=1` → pass-through (empty stdout, exit 0) | integration | `cargo test -p lacon-adapter-claudecode --test hook_e2e bypass_*` | ❌ W0 | ⬜ pending |
| 3-04-* | 04 | 3 | REQ-adapter-pipes-passthrough | — | Pipelines (`a \| b`) flow as one segment; not split on `\|` | unit | `cargo test -p lacon-adapter-claudecode --test chain_split pipeline_as_segment` | ❌ W0 (covered by 3-02) | ⬜ pending |
| 3-04-* | 04 | 3 | REQ-adapter-tui-bypass | T-tui-whole-chain | TUI segment in a chain triggers whole-chain bypass | integration | `cargo test -p lacon-adapter-claudecode --test hook_e2e tui_in_chain_whole_bypass` | ❌ W0 | ⬜ pending |
| 3-05-* | 05 | 4 | REQ-cli-init | T-settings-clobber | `.lacon/`, `.claude/settings.json`, CLAUDE.md created; user-authored hooks preserved | integration | `cargo test -p lacon-cli --test cli_init` | ❌ W0 | ⬜ pending |
| 3-05-* | 05 | 4 | REQ-cli-init | T-init-idempotency | Re-running `lacon init` is content-equal no-op | integration | `cargo test -p lacon-cli --test cli_init init_is_idempotent` | ❌ W0 | ⬜ pending |
| 3-05-* | 05 | 4 | REQ-cli-surface-cap | — | 6-command CLI surface unchanged (no `lacon hook` subcommand) | integration | `cargo test -p lacon-cli --test cli_surface` | ✅ exists | ⬜ pending |
| (perf) | 04 | 3 | CON-nfr-cold-start-budget | — | `lacon-claude-hook` pass-through ≤2ms median; rewrite ≤5ms median | manual | `cargo run --release --bin cold_start_probe` (extended in P4) | benches/cold_start.rs exists | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

The following test files do NOT yet exist and must be created during Phase 3 work itself (Wave 0 stubs created in the same plan that adds the code — Rust convention is inline `#[cfg(test)] mod tests` for unit tests and `tests/*.rs` for integration tests):

- [ ] `crates/lacon-adapter-claudecode/tests/chain_split.rs` — 13-scenario matrix for REQ-adapter-chained-commands (Plan 02)
- [ ] `crates/lacon-adapter-claudecode/tests/tui_heuristic.rs` — 22 pure + 8 conditional rows for REQ-adapter-tui-bypass (Plan 03)
- [ ] `crates/lacon-adapter-claudecode/tests/hook_e2e.rs` — end-to-end JSON-in/JSON-out for REQ-adapter-pretooluse-only + REQ-adapter-bypass-detection (Plan 04)
- [ ] `crates/lacon-cli/tests/cli_init.rs` — end-to-end `lacon init` for REQ-cli-init (Plan 05)
- [ ] `crates/lacon-core/src/rules/rewrite.rs` (with inline `#[cfg(test)] mod tests`) — apply_rewrite invariant tests covering D-19 (Plan 03)
- [ ] `crates/lacon-adapter-claudecode/src/quote.rs` (with inline `#[cfg(test)] mod tests`) — POSIX round-trip tests for D-20 (Plan 03)

Framework install: none — `cargo test` + workspace `assert_cmd`/`predicates`/`tempfile` are already wired from Phases 1–2.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `lacon-claude-hook` cold-start budgets (pass-through ≤2ms, rewrite ≤5ms) | CON-nfr-cold-start-budget | Performance budgets require warm-cache `--release` runs in a stable environment; cannot be CI-gated without flake risk | `cargo build --release && cargo run --release --bin cold_start_probe -- --scenarios hook-passthrough,hook-rewrite --iters 50`. Record median into `docs/architecture.md`. |
| Real Claude Code dogfood loop | REQ-adapter-* | Validates the hook actually works when Claude Code invokes it (assert_cmd is a stand-in, not the real spawning shell) | After `lacon init` in a scratch project, run `claude` and exercise `pnpm install`, `cargo test`, `git status`. Verify lacon-managed entries appear in `.claude/settings.json`; verify rule-matched commands get wrapped. |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies declared in the plan task
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references (chain_split.rs, tui_heuristic.rs, hook_e2e.rs, cli_init.rs, rewrite.rs tests, quote.rs tests)
- [ ] No watch-mode flags in any test command
- [ ] Feedback latency < 60s for full suite
- [ ] `nyquist_compliant: true` set in frontmatter (after plans pass and Wave 0 stubs created)

**Approval:** pending
