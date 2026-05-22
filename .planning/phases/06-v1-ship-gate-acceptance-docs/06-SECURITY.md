---
phase: 6
slug: v1-ship-gate-acceptance-docs
status: verified
threats_open: 0
asvs_level: 1
created: 2026-05-22
---

# Phase 6 — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.

Phase 6 is a ship-gate phase (acceptance validation + CI + documentation). It adds
tests, a benchmark entry point, a hermetic GitHub Actions workflow, and user-facing
Markdown — no new product/runtime surface. The threat register below was authored at
plan time across the three Phase-6 plans (06-01 acceptance/proof tests, 06-02
infra/CI, 06-03 docs) and every mitigation was verified present in the codebase
during this audit (not assumed from the summaries).

---

## Trust Boundaries

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| stored raw build log → `lacon explain` rendering | Bytes captured from an untrusted subprocess are replayed; the filtered "safe view" column is shown to a human in a terminal. | Untrusted stdout/stderr bytes (may contain ANSI/OSC control sequences) |
| test process → developer real home dir | New tests could write to `~/.claude/settings.json` / `~/.local/share/lacon/history.db` if not sandboxed. | Local filesystem state (developer config + history DB) |
| `#[ignore]`d real-pnpm test / CI workflow → external package registry | A real `pnpm install` reaches the npm registry; any CI `install` step would pull untrusted code into the build (supply chain). | Network → third-party packages |
| third-party GitHub Action → CI runner | Actions execute with the repo token's permissions; a malicious/typo-squatted action could exfiltrate or tamper. | CI token scope + repo contents |
| documentation → user actions / schema contract | Docs instruct users to run commands and write config; examples that diverge from `filter-rule-schema.md` or actual primitive behavior create false expectations (drift). | User-facing instructions (no code executed by docs) |

---

## Threat Register

| Threat ID | Category | Component | Disposition | Mitigation | Status |
|-----------|----------|-----------|-------------|------------|--------|
| T-06-01 | Tampering | `lacon explain` filtered column (terminal-control injection via stored raw build logs) | mitigate | `sanitize_for_display` (`crates/lacon-cli/src/commands/explain.rs:235`) neutralizes C0/C1/ESC/DEL via `is_control()`; unit test `sanitize_escapes_ansi_and_control_bytes` proves ESC/CSI/OSC-52 do not survive. New byte-equality test (`cli_explain.rs:217`) uses plain-ASCII payload so it does not regress the safe-view neutralization (WR-01). | closed |
| T-06-02 | Tampering | new/modified tests writing to real user state | mitigate | `pnpm_e2e.rs` and `hot_reload.rs` both redirect `XDG_DATA_HOME`/`XDG_CONFIG_HOME` to a tempdir and use a tempdir cwd; no `.claude/settings.json` / `history.db` mutation. | closed |
| T-06-03 | Tampering (supply chain) | real `pnpm install` in `pnpm_e2e_real` | mitigate | Real-pnpm test carries `#[ignore = "requires pnpm …"]` (`pnpm_e2e.rs:161`); default `cargo test` and CI never run `--ignored`. Hermetic variant uses the in-repo `test_emitter` stub (no network). | closed |
| T-06-04 | Spoofing | locating the `test_emitter` / `lacon` / `lacon-claude-hook` binaries | mitigate | Resolved via `assert_cmd::cargo::cargo_bin(...)` (cargo artifact), never a PATH lookup a planted binary could hijack (`pnpm_e2e.rs:39/45/82`, `hot_reload.rs:42/48`). | closed |
| T-06-CI-01 | Tampering / Elevation | GitHub Action dependencies in `ci.yml` | mitigate | Only `actions/checkout@v4` is used and pinned (`.github/workflows/ci.yml:47`); the rest uses the runner's pre-installed Rust. No third-party actions. | closed |
| T-06-CI-02 | Information Disclosure | runner secrets / token exposure | mitigate | Top-level least-privilege `permissions: contents: read` (`ci.yml:34-35`); workflow references no `secrets.*` (grep clean). | closed |
| T-06-CI-SC | Tampering (supply chain) | npm/registry/system-lib installs in CI | mitigate | No `brew/npm/pip/apt install` and no `--ignored` (grep gate clean); `rusqlite[bundled]` vendors SQLite. The real-pnpm test is `#[ignore]`d so the npm registry is never reached. | closed |
| T-06-CI-03 | Denial of Service (flaky gate) | hard `<10ms` wall-clock assert on a shared macOS VM | accept (by design) | macOS cold start is soft-reported (min-of-N via `scripts/bench-cold-start.sh`, `ci.yml:81-82`), not a build-breaking assert; the deterministic hard gate is the in-process `tracker_open` steady-state criterion bench, which runs on both lanes. Residual risk is informational only. | closed |
| T-06-DOC-01 | Information Disclosure (misleading docs) | `primitive-reference` / `worked-example` accuracy | mitigate | Examples extracted from `docs/specs/filter-rule-schema.md` and verified against the tested golden fixtures (`tests/fixtures/primitives/<name>`); Plan-03 verify greps assert every primitive + the canonical truncation marker are present (drift-prevention, Pitfall 5). | closed |
| T-06-DOC-02 | Tampering (scope creep in docs) | README promising out-of-v1 features | accept (low) | README is constrained to the locked six-command surface and v1 platform scope; grep confirms no Windows/registry/purge promises. No code or runtime surface touched. | closed |

*Status: open · closed*
*Disposition: mitigate (implementation required) · accept (documented risk) · transfer (third-party)*

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| AR-06-01 | T-06-CI-03 | A hard sub-10ms wall-clock assert on a shared/noisy macOS CI VM would be flaky and break the build for non-regressions. The wall-clock cold start is reported (min-of-N) for visibility; the deterministic regression gate is the in-process `tracker_open` steady-state criterion bench on both OS lanes. Residual risk = a noisy informational number, no build impact. | Phase 6 plan 06-02 (verified by gsd-secure-phase) | 2026-05-22 |
| AR-06-02 | T-06-DOC-02 | Documentation scope creep is a low-severity, non-runtime risk. The README is bounded to the six-command surface and v1 platforms by construction; reviewer + grep confirm no out-of-v1 promises. | Phase 6 plan 06-03 (verified by gsd-secure-phase) | 2026-05-22 |

*Accepted risks do not resurface in future audit runs.*

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-05-22 | 10 | 10 | 0 | gsd-secure-phase (orchestrator-verified against codebase) |

Register origin: `register_authored_at_plan_time: true` — all three Phase-6 plans
contained a parseable `<threat_model>` block. Per the secure-phase short-circuit,
mitigations were verified present (no retroactive-STRIDE scan, no separate auditor
spawn needed); all plan-time threats confirmed CLOSED.

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] Accepted risks documented in Accepted Risks Log
- [x] `threats_open: 0` confirmed
- [x] `status: verified` set in frontmatter

**Approval:** verified 2026-05-22
