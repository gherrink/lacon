---
phase: 6
slug: v1-ship-gate-acceptance-docs
status: ready
nyquist_compliant: true
wave_0_complete: false
created: 2026-05-22
---

# Phase 6 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` + `cargo test`; criterion for benches; `assert_cmd` for CLI black-box tests |
| **Config file** | none — Cargo workspace (`Cargo.toml`); no separate test config |
| **Quick run command** | `cargo test --workspace` |
| **Full suite command** | `cargo test --workspace && cargo bench -p lacon-core --bench tracker_open` |
| **Estimated runtime** | ~60–120 seconds (workspace test suite; bench adds ~10s) |

Notes:
- Default `cargo test` is **hermetic** — it never invokes `pnpm`/`vitest`/`cargo`-tools. The real-`pnpm` E2E test is `#[ignore]`d and runs only via `cargo test -p lacon-cli --test pnpm_e2e -- --ignored`.
- The hard cold-start gate is the deterministic in-process `tracker_open` criterion bench (steady-state variant). The wall-clock cold-start number on macOS is soft-reported from the `macos-latest` CI lane (shared-runner noise — never a hard `<10ms` assert).

---

## Sampling Rate

- **After every task commit:** Run `cargo test --workspace`
- **After every plan wave:** Run `cargo test --workspace && cargo bench -p lacon-core --bench tracker_open`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** ~120 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 06-01-01 | 01 | 1 | REQ-acceptance-test-coverage, REQ-acceptance-bundled-reduction | — | N/A (audit of existing tests) | integration | `cargo test --test primitives && cargo test --test bundled_rules && cargo test -p lacon-adapter-claudecode --test chain_split` | ✅ | ⬜ pending |
| 06-01-02 | 01 | 1 | REQ-acceptance-explain-reproducibility, REQ-acceptance-hot-reload | — | explain neutralizes terminal control bytes; tests sandbox XDG to tempdir | integration | `cargo test -p lacon-cli --test cli_explain && cargo test -p lacon-cli --test hot_reload` | ❌ W0 | ⬜ pending |
| 06-01-03 | 01 | 1 | REQ-acceptance-pnpm-end-to-end | T-06-01 (supply-chain) | real `pnpm install` kept `#[ignore]`d out of hermetic CI; stub via cargo artifact not PATH | integration | `cargo test -p lacon-cli --test pnpm_e2e` | ❌ W0 | ⬜ pending |
| 06-02-01 | 02 | 1 | REQ-acceptance-cold-start-budget, REQ-acceptance-test-coverage | — | no `Tracker::open` source edit (bench-only) | bench | `cargo bench -p lacon-core --bench tracker_open 2>&1 \| tee /tmp/tracker_open_bench.txt; grep -E 'tracker_open_steady_state' /tmp/tracker_open_bench.txt` | ✅ | ⬜ pending |
| 06-02-02 | 02 | 1 | REQ-acceptance-cold-start-budget | — | benchmark wraps existing probe; tempdir-isolated | script | `chmod +x scripts/bench-cold-start.sh && bash -n scripts/bench-cold-start.sh && ./scripts/bench-cold-start.sh 2>&1 \| tee /tmp/coldstart.txt; grep -iE 'os\|linux\|cold' /tmp/coldstart.txt` | ❌ W0 | ⬜ pending |
| 06-02-03 | 02 | 1 | REQ-acceptance-test-coverage, REQ-acceptance-cold-start-budget | T-06-02 (CI hermeticity / least-privilege) | hermetic by construction; no brew/npm/pip/apt; `--ignored` excluded; pinned actions | config | `test -f .github/workflows/ci.yml && python3 -c "import yaml; yaml.safe_load(open('.github/workflows/ci.yml')); print('yaml ok')" && grep -q 'macos-latest' .github/workflows/ci.yml && grep -q 'ubuntu-latest' .github/workflows/ci.yml && ! grep -nE 'brew install\|npm i\|npm install\|pip install\|apt-get install\|--ignored' .github/workflows/ci.yml` | ❌ W0 | ⬜ pending |
| 06-03-01 | 03 | 1 | REQ-docs-primitive-reference | — | examples derived from tested fixtures (no fabricated output) | docs | `test -f docs/primitive-reference.md && for p in strip_ansi drop_regex keep_regex replace_regex dedupe collapse_repeated keep_head keep_tail keep_around_match max_bytes; do grep -q "$p" docs/primitive-reference.md \|\| exit 1; done` | ❌ W0 | ⬜ pending |
| 06-03-02 | 03 | 1 | REQ-docs-worked-example | — | N/A | docs | `test -f docs/worked-example.md && grep -q 'our-monorepo-pnpm' docs/worked-example.md && grep -q 'extends' docs/worked-example.md` | ❌ W0 | ⬜ pending |
| 06-03-03 | 03 | 1 | REQ-docs-readme | — | N/A | docs | `grep -qi 'Quickstart' README.md && grep -qi 'Install' README.md && grep -q 'worked-example' README.md && grep -q 'primitive-reference' README.md && ! grep -q 'No installable artifact yet' README.md` | ✅ (rewrite) | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*
*File Exists "❌ W0" = the test/artifact file is created by the task itself during this phase (no pre-existing framework gap — the workspace test harness already exists).*

---

## Wave 0 Requirements

The Rust workspace test harness already exists (Phases 1–5). No framework install or shared-fixture scaffolding is needed. The "new" files below are authored by the phase tasks themselves, not Wave-0 prerequisites:

- `crates/lacon-cli/tests/hot_reload.rs` — created by 06-01-02
- `crates/lacon-cli/tests/pnpm_e2e.rs` — created by 06-01-03
- `scripts/bench-cold-start.sh` — created by 06-02-02
- `.github/workflows/ci.yml` — created by 06-02-03
- `docs/primitive-reference.md`, `docs/worked-example.md` — created by 06-03

*Existing infrastructure (cargo workspace + `assert_cmd` + criterion + `bin/test_emitter` + `tests/fixtures/`) covers all phase requirements.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Real `pnpm install` flows through `lacon init`→hook→`lacon run` | REQ-acceptance-pnpm-end-to-end | Requires `pnpm` toolchain; kept out of hermetic CI by design (`#[ignore]`) | `cargo test -p lacon-cli --test pnpm_e2e -- --ignored` on a machine with pnpm installed |
| macOS cold-start wall-clock <10ms on the hook hot path | REQ-acceptance-cold-start-budget | Dev machine is Linux-only; macOS number comes from the `macos-latest` CI runner; soft-gated due to shared-runner noise | Read the `macos-latest` lane output of the first CI run; confirm min-of-N reported and within budget |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references (none — workspace harness pre-exists)
- [x] No watch-mode flags
- [x] Feedback latency < 120s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved 2026-05-22
