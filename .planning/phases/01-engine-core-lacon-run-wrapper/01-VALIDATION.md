---
phase: 1
slug: engine-core-lacon-run-wrapper
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-06
---

# Phase 1 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | `cargo test` (built-in Rust test harness) + `assert_cmd` 2.x + `predicates` 3.x for CLI integration |
| **Config file** | `Cargo.toml` (workspace root) — Wave 0 installs |
| **Quick run command** | `cargo test --workspace --lib` |
| **Full suite command** | `cargo test --workspace --all-targets` |
| **Estimated runtime** | ~30 seconds (cold), ~5 seconds (warm incremental) |

---

## Sampling Rate

- **After every task commit:** Run `cargo test --workspace --lib` (unit tests only — fast feedback)
- **After every plan wave:** Run `cargo test --workspace --all-targets` (unit + integration + doctests)
- **Before `/gsd-verify-work`:** Full suite green + `cargo clippy --workspace --all-targets -- -D warnings` clean + `cargo fmt --check` clean
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD | TBD | ⬜ pending |

*Populated by planner. Per-primitive unit tests must be golden-fixture driven (one test per `Stage` variant); `lacon run` integration tests must spawn a real subprocess (test-only Rust binary or `sh -c`); `lacon validate` tests must assert on golden error output.*

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `Cargo.toml` (workspace root) — `[workspace]` with `members = ["crates/*"]`, `resolver = "2"`, `[workspace.package]` for shared edition/MSRV/license, `[workspace.dependencies]` for shared crate set
- [ ] `crates/lacon-core/Cargo.toml`, `crates/lacon-cli/Cargo.toml`, `crates/lacon-adapter-claudecode/Cargo.toml` — three workspace members
- [ ] `rust-toolchain.toml` — pinned MSRV (per CONTEXT.md D-02)
- [ ] `tests/` directory at workspace root — for end-to-end CLI integration tests (`assert_cmd`-driven)
- [ ] `tests/fixtures/` — golden in/out pairs per primitive (per success criterion #2)
- [ ] `[dev-dependencies]` block with `assert_cmd = "2"`, `predicates = "3"`, `tempfile = "3"`
- [ ] `serde-saphyr` smoke test — confirm `Value` type API supports the `has_id && has_match` content-dispatch check (per RESEARCH open question 1)
- [ ] `starlark` MSRV check after `cargo add starlark` — confirm workspace MSRV is sufficient (per RESEARCH open question 2)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Cold-start budget probe | REQ-acceptance-cold-start-budget (Phase 6 owns final gate) | Phase 1 owns the architecture decisions that determine whether <10ms is reachable; the formal benchmark is a Phase 6 criterion harness | After Phase 1 lands, run `time lacon validate ...` 100 times; record median; if >10ms, flag as architectural concern for Phase 6 |
| `starlark-rust` cold-start cost | CONTEXT.md benchmark item 1 | One-off measurement to settle "lazy-init or eager?" | After Starlark stage lands, micro-bench load+evaluate of trivial `def process(ctx, lines)` script; if >2ms, switch to lazy-init; record finding in `docs/architecture.md` |
| `clap` v4 vs `pico-args` startup cost | CONTEXT.md benchmark item 2 | One-off measurement to validate the locked clap-derive choice | Bench `lacon --version` startup; if >2ms cost attributable to clap derive, file plan-B note in `docs/architecture.md` |
| `os_pipe` + threads vs `duct` vs raw `nix` | CONTEXT.md benchmark item 3 | One-off measurement to lock in the merge approach | Bench three alternatives on a 10k-line subprocess; record the chosen approach + numbers in `docs/architecture.md` |
| POSIX signal-forwarding macOS vs Linux | CONTEXT.md benchmark item 4 | Cross-platform behavior verification | Run integration test that sends SIGTERM to a `sleep 60` subprocess wrapped in `lacon run`; verify exit code on both macOS and Linux |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
