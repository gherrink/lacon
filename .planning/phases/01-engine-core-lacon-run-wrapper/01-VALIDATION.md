---
phase: 1
slug: engine-core-lacon-run-wrapper
status: revised
nyquist_compliant: true
wave_0_complete: false
created: 2026-05-06
revised: 2026-05-06
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
- **Max feedback latency:** 30 seconds (release builds excluded from per-task `<verify>` blocks; checked once at end-of-plan as acceptance criteria — see PLAN-01 Task 3 and PLAN-07 Task 1 W1 fix)

---

## Per-Task Verification Map

Populated from the seven PLAN files in this phase. Status legend: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky.

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 01-T1 | 01 | 0 | (workspace bootstrap) | T-01-01..05 | Pinned crate versions; Cargo.lock committed; no serde_yaml | Build | `cargo metadata --no-deps --format-version 1 > /dev/null` | ❌ Wave 0 | ⬜ pending |
| 01-T2 | 01 | 0 | (workspace bootstrap) | T-01-01 | Three-crate boundary; lacon-core declares ALL Wave-1+ deps so downstream plans don't edit Cargo.toml | Build | `cargo check --workspace` | ❌ Wave 0 | ⬜ pending |
| 01-T3 | 01 | 0 | (smoke) | — | serde-saphyr Value API + starlark MSRV settled | Unit | `cargo test --workspace --test wave0_smoke` | ❌ Wave 0 | ⬜ pending |
| 02-T1 | 02 | 1 | REQ-engine-streaming-primitives, REQ-engine-max-bytes-cap | T-02-01..05 | 10 enum variants; RegexSet OR-merge; byte-exact truncation marker | Unit + clippy | `cargo test -p lacon-core --lib && cargo clippy -p lacon-core --lib -- -D warnings` | ❌ Wave 0 | ⬜ pending |
| 02-T2 | 02 | 1 | REQ-engine-streaming-primitives, REQ-engine-max-bytes-cap | T-02-05 | Per-primitive golden fixtures; max_bytes byte-exact assertion | Integration (golden) | `cargo test -p lacon-core --test primitives` | ❌ Wave 0 | ⬜ pending |
| 03-T1 | 03 | 1 | REQ-cli-validate, REQ-engine-rule-loading | T-03-01, T-03-02 | deny_unknown_fields; rust-embed for bundled rules | Unit | `cargo test -p lacon-core --lib && cargo check -p lacon-core` | ❌ Wave 0 | ⬜ pending |
| 03-T2 | 03 | 1 | REQ-engine-rule-loading, REQ-engine-extends, REQ-engine-max-bytes-cap | T-03-03, T-03-04 | HashSet cycle detection; path-traversal rejection on script paths; mtime cache | Integration | `cargo test -p lacon-core --test rules_loader --test extends_flatten` | ❌ Wave 0 | ⬜ pending |
| 03-T3 | 03 | 1 | REQ-cli-validate | T-03-06 | Project-config retention -> UserOnlyKeyInProject; byte-exact error format | Integration | `cargo test -p lacon-core --test validate_dispatch` | ❌ Wave 0 | ⬜ pending |
| 04-T1 | 04 | 2 | REQ-engine-starlark-postprocess | T-04-01 | Hermetic Globals::standard(); negative grep `set_loader` | Unit + clippy | `cargo test -p lacon-core --lib starlark_host && cargo clippy -p lacon-core --lib -- -D warnings` | ❌ Wave 0 | ⬜ pending |
| 04-T2 | 04 | 2 | REQ-engine-starlark-postprocess | T-04-03 | Path-traversal rejection on .star file paths; load() rejected | Integration | `cargo test -p lacon-core --test starlark_host && cargo test -p lacon-core` | ❌ Wave 0 | ⬜ pending |
| 05-T1 | 05 | 3 | REQ-engine-on-error, REQ-engine-bypass, REQ-cli-run | T-05-01..05 | Pitfall 1 drop(command_builder); Pitfall 2 read_until not lines(); Stage::MaxBytes is sole truncation point (W3 fix); byte-exact marker emitted on overflow | Integration | `cargo test -p lacon-core --test runtime_subprocess --test runtime_on_error --test runtime_bypass` | ❌ Wave 0 | ⬜ pending |
| 05-T2 | 05 | 3 | REQ-cli-run | T-05-09 | SIGTERM/SIGINT forwarded via nix::kill; #[cfg(unix)] gate | Integration (signal probe gated #[ignore]) | `cargo test -p lacon-core` (signal probe behind --include-ignored) | ❌ Wave 0 | ⬜ pending |
| 06-T1 | 06 | 4 | REQ-cli-run, REQ-cli-validate | T-06-06 | clap 6-subcommand cap structurally enforced; 4 stubs return exit 2 | Build | `cargo build --release && target/release/lacon --help \| grep -cE '^\s+(run\|validate\|init\|stats\|explain\|doctor)' && target/release/lacon init` | ❌ Wave 0 | ⬜ pending |
| 06-T2 | 06 | 4 | REQ-cli-run | T-06-01, T-06-04 | argv passed via Command::args (no shell); --rule lookup is a logical ID | Integration (assert_cmd) | `cargo test -p lacon-cli --test cli_run` | ❌ Wave 0 | ⬜ pending |
| 06-T3 | 06 | 4 | REQ-cli-validate | T-06-06 | byte-exact `<path>:<line>: <Cat>: <msg>`; cli_surface guards 6-cmd cap | Integration (assert_cmd) | `cargo test -p lacon-cli` | ❌ Wave 0 | ⬜ pending |
| 07-T1 | 07 | 5 | REQ-engine-* + REQ-cli-* end-to-end | T-07-01..04 | test_emitter resolved via cargo_bin (no PATH); 5 e2e scenarios incl. max_bytes byte-exact | Integration (workspace e2e) | `cargo test --workspace --test end_to_end` (release build moved out of `<automated>` per W1; one-shot acceptance check) | ❌ Wave 0 | ⬜ pending |
| 07-T2 | 07 | 5 | REQ-acceptance-cold-start-budget (Phase 6) | T-07-03 | Cold-start probe baseline; D-11 + D-12 documented | Manual + build | `cargo build --release --bin cold_start_probe && grep -c '## Cold-start measurements (Phase 1)' docs/architecture.md` | ❌ Wave 0 | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

*Per-primitive unit tests are golden-fixture driven (one test per `Stage` variant); `lacon run` integration tests spawn a real subprocess via `/bin/sh -c '...'` or the `test_emitter` workspace member; `lacon validate` tests assert on golden error output via `assert_cmd::predicate::str::contains`.*

---

## Wave 0 Requirements

- [ ] `Cargo.toml` (workspace root) — `[workspace]` with `members = ["crates/*"]`, `resolver = "2"`, `[workspace.package]` for shared edition/MSRV/license, `[workspace.dependencies]` for shared crate set (regex, smallvec, serde, serde-saphyr, clap, starlark, os_pipe, crossbeam-channel, nix, signal-hook, thiserror, anyhow, etcetera, rust-embed, plus dev: assert_cmd, predicates, tempfile, insta)
- [ ] `crates/lacon-core/Cargo.toml`, `crates/lacon-cli/Cargo.toml`, `crates/lacon-adapter-claudecode/Cargo.toml` — three workspace members
- [ ] `crates/lacon-core/Cargo.toml [dependencies]` declares EVERY workspace dep PLAN-02..PLAN-05 will need (regex, smallvec, serde, serde-saphyr, rust-embed, etcetera, starlark, os_pipe, crossbeam-channel, nix, signal-hook, thiserror) — so Wave 1+ plans never edit lacon-core/Cargo.toml in parallel
- [ ] `rust-toolchain.toml` — pinned MSRV (per CONTEXT.md D-02)
- [ ] `tests/` directory at workspace root — for end-to-end CLI integration tests (`assert_cmd`-driven)
- [ ] `tests/fixtures/` — golden in/out pairs per primitive (per success criterion #2)
- [ ] `[dev-dependencies]` block with `assert_cmd = "2"`, `predicates = "3"`, `tempfile = "3"`
- [ ] `serde-saphyr` smoke test — confirm `Value` type API supports the `has_id && has_match` content-dispatch check (per RESEARCH "Open Questions (RESOLVED)" item 1)
- [ ] `starlark` MSRV check after `cargo add starlark` — confirm workspace MSRV is sufficient (per RESEARCH "Open Questions (RESOLVED)" item 2)

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Cold-start budget probe | REQ-acceptance-cold-start-budget (Phase 6 owns final gate) | Phase 1 owns the architecture decisions that determine whether <10ms is reachable; the formal benchmark is a Phase 6 criterion harness | After Phase 1 lands, run `time lacon validate ...` 100 times; record median; if >10ms, flag as architectural concern for Phase 6 |
| `starlark-rust` cold-start cost | CONTEXT.md benchmark item 1 | One-off measurement to settle "lazy-init or eager?" | After Starlark stage lands, micro-bench load+evaluate of trivial `def process(ctx, lines)` script; if >2ms, switch to lazy-init; record finding in `docs/architecture.md` |
| `clap` v4 vs `pico-args` startup cost | CONTEXT.md benchmark item 2 | One-off measurement to validate the locked clap-derive choice | Bench `lacon --version` startup; if >2ms cost attributable to clap derive, file plan-B note in `docs/architecture.md` |
| `os_pipe` + threads vs `duct` vs raw `nix` | CONTEXT.md benchmark item 3 | One-off measurement to lock in the merge approach | Bench three alternatives on a 10k-line subprocess; record the chosen approach + numbers in `docs/architecture.md` |
| POSIX signal-forwarding macOS vs Linux | CONTEXT.md benchmark item 4 | Cross-platform behavior verification | Run integration test that sends SIGTERM to a `sleep 60` subprocess wrapped in `lacon run`; verify exit code on both macOS and Linux |
| Release-mode workspace build | (PLAN-01 Task 3 + PLAN-07 Task 1, W1 revision fix) | Release builds take 30s–2min; running per-task would blow the 30s feedback-latency target | Run `cargo build --release --workspace` once at end-of-plan as an acceptance criterion; if red, fix `[profile.release]` settings before declaring plan done |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies (per the Per-Task Verification Map above; release builds split out per W1 fix)
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (PLAN-01 owns workspace + lacon-core/Cargo.toml as sole source so parallel Wave 1 plans don't conflict)
- [x] No watch-mode flags
- [x] Feedback latency < 30s (release builds excluded from per-task `<verify>` blocks)
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** populated 2026-05-06 (revision 1, post plan-checker feedback). Status fields remain `⬜ pending` until Wave 0 + downstream task execution lands.
