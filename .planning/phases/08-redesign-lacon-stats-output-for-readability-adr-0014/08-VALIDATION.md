---
phase: 8
slug: redesign-lacon-stats-output-for-readability-adr-0014
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-05-23
---

# Phase 8 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.
> Source: `08-RESEARCH.md` → "Validation Architecture". Nyquist enabled — sample at
> each behavior's **failure-mode boundaries** (thresholds, branch edges, empty/overflow),
> not redundant interior points.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` (libtest) + `assert_cmd` (black-box CLI) + `tempfile` + dev-only `rusqlite` (DB seeding) |
| **Config file** | none — Cargo-native; CI gates after `cargo build --workspace` (`.github/workflows/ci.yml`) |
| **Quick run command** | `cargo test -p lacon-cli stats` |
| **Full suite command** | `cargo build --workspace && cargo test --workspace` |
| **Estimated runtime** | ~30–60 seconds (build-first is load-bearing: `assert_cmd` resolves `target/debug/lacon`) |

> **Wave-0 build note:** the new `cli_stats.rs` black-box tests require `target/debug/lacon`
> to exist (`assert_cmd` `cargo_bin` fallback). Any plan running black-box tests must
> `cargo build -p lacon-cli` (or `--workspace`) first, or the tests panic on unresolved binary.

---

## Sampling Rate

- **After every task commit:** `cargo test -p lacon-cli stats` (inline `stats.rs` units + `cli_stats.rs` black-box). For the reader task, also `cargo test -p lacon-core overall` / `query`.
- **After every plan wave:** `cargo build --workspace && cargo test --workspace` (full hermetic suite — what CI gates on).
- **Before `/gsd:verify-work`:** Full suite green + `cargo clippy --workspace --all-targets` + `cargo fmt --check`.
- **Max feedback latency:** ~60 seconds.

---

## Per-Task Verification Map

> Task IDs assigned during planning. Rows are keyed by behavior (ADR 0014) until plans exist;
> the planner MUST map each behavior to a task whose `<acceptance_criteria>` runs the listed command.

| Behavior (ADR 0014) | Requirement | Test Type | Automated Command | Smallest sufficient test (Nyquist boundary) | File Exists | Status |
|---------------------|-------------|-----------|-------------------|---------------------------------------------|-------------|--------|
| `(ephemeral)` bucket collapse | ADR 0014 §2a | unit + black-box | `cargo test -p lacon-cli is_ephemeral` | `/tmp/a`, `/tmp/b`, `$TMPDIR/c` → all key `(ephemeral)`; project section shows ONE `(ephemeral)` line. Negative boundary: `/tmpfoo/x` is NOT ephemeral (`Path::starts_with` vs `str::starts_with`). | ❌ W0 | ⬜ pending |
| `.git` **directory** rollup (repo + subdir) | ADR 0014 §2b | unit + black-box | `cargo test -p lacon-cli resolve_repo_root` | `repo/.git/` (dir) + subdir `repo/sub/`; both resolve to `repo`; two rows roll into one line. | ❌ W0 | ⬜ pending |
| `.git` **file** worktree rollup (absolute gitdir) | ADR 0014 §2b | unit + black-box | `cargo test -p lacon-cli` (worktree) | `repo/.git/worktrees/wt/commondir`=`../..`, `wt/.git`(file)=`gitdir: <abs>/repo/.git/worktrees/wt` → `wt` resolves to `repo`. | ❌ W0 | ⬜ pending |
| relative-gitdir (submodule) resolution | ADR 0014 §2b | unit | `cargo test -p lacon-cli` (submodule) | `.git` file with **relative** `gitdir:` → resolves against gitfile's own dir (relative-vs-absolute branch boundary). | ❌ W0 | ⬜ pending |
| top-N capping + `… M more` | ADR 0014 §3 | black-box | `cargo test -p lacon-cli` (cap) | Seed N=11 distinct non-ephemeral/non-git projects → exactly 10 rows + `… more` line. 11 is the smallest input proving both cap AND overflow. | ❌ W0 | ⬜ pending |
| `--all` uncapping | ADR 0014 §3 (D-12) | black-box | `cargo test -p lacon-cli` (--all) | Same 11-project seed + `--all` → all 11 rows, no `… more`. | ❌ W0 | ⬜ pending |
| `--bytes` exact-integer escape | ADR 0014 §4 (D-14) | black-box | `cargo test -p lacon-cli` (--bytes) | Row totaling 22_800: no flag → `contains("22.8 KB")`; `--bytes` → `contains("22800")` AND NOT `KB`. | ❌ W0 | ⬜ pending |
| `humanize_bytes` decimal-SI boundaries | ADR 0014 §4 (D-13) | unit | `cargo test -p lacon-cli humanize` | Six points: `999→"999 B"`, `1000→"1.0 KB"`, `1024→"1.0 KB"`, `22_800→"22.8 KB"`, `1_000_000→"1.0 MB"`, `0→"0 B"`. | ❌ W0 | ⬜ pending |
| overall headline (`overall_totals` over `bypassed=0`) | ADR 0014 §1 (D-05) | unit (core) + black-box | `cargo test -p lacon-core overall` + `cargo test -p lacon-cli stats` | core: matched + unmatched + one `bypassed=1` row → totals exclude bypassed. black-box: headline appears FIRST with runs + saved %. | ❌ W0 | ⬜ pending |
| literal-path fallback (no-git / bare / deleted) | ADR 0014 §2c (D-10) | unit | `cargo test -p lacon-cli` (fallback) | Three branches: no `.git` → literal; `core.bare=true` → literal; non-existent path → literal (no panic, no `canonicalize`). | ❌ W0 | ⬜ pending |
| exit-code + empty-DB contract preserved | D-03 | black-box | `cargo test -p lacon-cli stats_empty_db / stats_invalid_since` | EXISTING `stats_empty_db_prints_no_data_yet_and_succeeds` + `stats_invalid_since_errors_nonzero_no_panic` (`cli_stats.rs:174-246`) still pass; update empty-DB header only if relabeled. | ✅ exists (verify) | ⬜ pending |
| relabeled section headers | ADR 0014 §4 (D-15) | black-box | `cargo test -p lacon-cli stats_seeded` | Update the four `contains(...)` header assertions (`cli_stats.rs:166-169`) to new labels — this IS the relabel regression guard. | ✅ exists (edit) | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] Extend inline `#[cfg(test)] mod tests` in `crates/lacon-cli/src/commands/stats.rs` (existing block ~`stats.rs:299-358`) — `humanize_bytes` boundaries, `is_ephemeral` (incl. `/tmpfoo` negative), `resolve_repo_root` (dir / worktree-absolute / submodule-relative), literal-fallback (no-git / bare / deleted).
- [ ] New black-box tests in `crates/lacon-cli/tests/cli_stats.rs` — ephemeral collapse, `.git` rollup, top-N cap + `… more`, `--all` uncap, `--bytes` escape, headline-first. Reuse `init_db`/`insert_invocation`/`lacon` helpers; add a small fixture builder writing `.git` dirs/files under `tempdir()`.
- [ ] New lacon-core test for `overall_totals` / filtered counterpart — alongside existing `query.rs` tests, seeding matched/unmatched/bypassed rows.
- [ ] Edit the four section-header `contains(...)` assertions in `cli_stats.rs:166-169` to the new D-15 labels.
- [ ] Framework install: **none** — Cargo-native test stack already present.

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Final header/column wording reads cleanly to a non-author | ADR 0014 §4 (Claude's Discretion) | Subjective readability is not assertable; the `contains(...)` tests pin the tokens but not overall legibility | After implementation, run `lacon stats` against a seeded DB and eyeball the headline + four sections for clarity. |

*All other phase behaviors have automated verification.*

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
