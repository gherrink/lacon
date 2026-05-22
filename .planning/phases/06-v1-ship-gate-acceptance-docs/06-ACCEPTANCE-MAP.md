# Phase 6 Acceptance Traceability Map (D-01 / D-02)

**Built:** 2026-05-22
**Purpose:** Map every Phase-6 v1-acceptance requirement to the existing test(s)
that prove it, the exact `cargo test` / bench command to re-run, and the audited
green/red status. This is an **audit + traceability artifact**, not new test code
(D-01: audit-first, only fill genuine gaps). Where a requirement is closed by a
later plan in this phase, the row cross-references **Plan 02** rather than
duplicating its work here.

Phases 1–5 are complete; the bulk of the acceptance coverage already lives on
disk. This map confirms it green and identifies the few genuine proof/gate gaps
that Plan 01 fills (D-03 explain byte-equality, D-06 hot-reload proof, D-07
pnpm E2E pair).

---

## REQ → Test Traceability

| REQ ID | Success Criterion | Proving test file(s) | Exact command | Status |
|--------|-------------------|----------------------|---------------|--------|
| REQ-acceptance-bundled-reduction | SC1: all 10 bundled rules reduce ≥50% on representative output without dropping errors | `crates/lacon-core/tests/bundled_rules.rs:160-209` (fixture walker) | `cargo test --test bundled_rules` | **green** — already met by Phase 5 (D-02); see "Bundled reduction evidence" below |
| REQ-acceptance-pnpm-end-to-end | SC2 (first half): `lacon init` → `pnpm install` works end-to-end, hook fires, command wrapped, filtered output reaches the assistant | `crates/lacon-cli/tests/pnpm_e2e.rs` — `pnpm_e2e_hermetic` (default lane, stub) + `pnpm_e2e_real` (`#[ignore]`, real pnpm) | hermetic: `cargo test -p lacon-cli --test pnpm_e2e` · real: `cargo test -p lacon-cli --test pnpm_e2e -- --ignored` | **green** (hermetic) — added by Plan 01 Task 3 (D-07); real variant `#[ignore]`d out of CI |
| REQ-acceptance-cold-start-budget | SC1: cold-start < 10ms on the hook hot path (incl. the `lacon run` / `Tracker::open` path) | `benches/cold_start.rs` (probe), `crates/lacon-core/benches/tracker_open.rs` (criterion gate) | `cargo run --release --bin cold_start_probe` · `cargo bench -p lacon-core --bench tracker_open` | **owned by Plan 02** — steady-state vs first-ever `tracker_open` split + benchmark entry point + ubuntu/macos CI lanes (D-04/D-05/D-08/D-09). Not closed by Plan 01. |
| REQ-acceptance-explain-reproducibility | SC3: `lacon explain` re-derives the filtered output byte-for-byte from stored raw bytes | `crates/lacon-cli/tests/cli_explain.rs` — `explain_filtered_column_byte_equals_run_output` (byte-equality) + the 5 existing substring tests | `cargo test -p lacon-cli --test cli_explain` | **green** — byte-equality test added by Plan 01 Task 2 (D-03) atop the 5 existing |
| REQ-acceptance-hot-reload | SC2 (second half): a rule edit takes effect on the next invocation, no daemon / no restart | `crates/lacon-cli/tests/hot_reload.rs` — `rule_edit_takes_effect_on_next_invocation` | `cargo test -p lacon-cli --test hot_reload` | **green** — two-invocation black-box proof added by Plan 01 Task 2 (D-06); no watcher/daemon added |
| REQ-acceptance-test-coverage | SC4: suite covers each native primitive, the chained-command splitter, and every bundled rule; CI hermetic | see three sub-claims below | see three sub-claims below | **green** (primitives / splitter / bundled). CI-hermetic sub-claim **owned by Plan 02** (D-08). |

### REQ-acceptance-test-coverage — three sub-claims (SC4)

| Sub-claim | Proving test file | Exact command | Audited count | Status |
|-----------|-------------------|---------------|---------------|--------|
| Native primitives (all 10) | `crates/lacon-core/tests/primitives.rs` | `cargo test --test primitives` | 10 golden tests (one per primitive: `strip_ansi`, `drop_regex`, `keep_regex`, `replace_regex`, `dedupe`, `collapse_repeated`, `keep_head`, `keep_tail`, `keep_around_match`, `max_bytes`) | **green** — 10 passed |
| Chained-command splitter (13 spec scenarios) | `crates/lacon-adapter-claudecode/tests/chain_split.rs` | `cargo test -p lacon-adapter-claudecode --test chain_split` | 19 test functions covering all 13 `docs/specs/chained-commands.md` scenarios (S1, S2a/b/c, S3–S13, S14/S14b sub-variants) + 2 pathological no-panic tests | **green** — 19 passed |
| Bundled rules (all 10) | `crates/lacon-core/tests/bundled_rules.rs` | `cargo test --test bundled_rules` | fixture walker over 20 scenarios (10 rules × success + failure fixture each) | **green** — walker green, 20 fixtures asserted |
| CI hermetic (no toolchain installs) | `.github/workflows/ci.yml` | (CI build/test/bench, no `--ignored`, no `brew`/`npm`/`pip`/`apt` install) | — | **owned by Plan 02** (D-08): hermetic ubuntu + macos lanes. Not in Plan 01 scope. |

---

## Bundled reduction evidence (D-02)

`REQ-acceptance-bundled-reduction` is treated as **already met** by Phase 5's
`bundled_rules.rs` walker. The walker (per `assert_fixture`,
`crates/lacon-core/tests/bundled_rules.rs:93-144`) enforces three assertions on
every non-exempt success fixture:

1. **Byte-exact output** — `out.join("\n").trim_end_matches('\n')` equals
   `expected.txt` (`bundled_rules.rs:113-116`).
2. **≥50% reduction** — `expected_len as f64 / input_len as f64 <= 0.5`
   (`bundled_rules.rs:128-134`), skipped only when `exempt_from_reduction_check`
   is set (intended for tiny failure-path fixtures where the output IS the signal).
3. **No dropped errors** — every `must_keep_lines` substring survives filtering
   (`bundled_rules.rs:138-143`), proving error lines are not reduced away.

Replay is subprocess-free byte replay via `Runner::filter_bytes`
(`bundled_rules.rs:79-88`), with `meta.exit_code` selecting the ADR-0010 branch
(success / `on_error` / raw passthrough). Phase 6 re-confirms green and
references this; **no new reduction harness is authored** (D-02).

---

## Audit run record (2026-05-22)

The three audited suites were executed at plan-01 execution time and confirmed
green (not assumed):

| Command | Result |
|---------|--------|
| `cargo test --test primitives` | `ok. 10 passed; 0 failed; 0 ignored` |
| `cargo test --test bundled_rules` | `ok. 1 passed; 0 failed; 0 ignored` (walker; 20 fixtures asserted) |
| `cargo test -p lacon-adapter-claudecode --test chain_split` | `ok. 19 passed; 0 failed; 0 ignored` |

---

## Audit findings / genuine gaps (D-01)

Per RESEARCH Open Question 3, the expectation was few-to-zero genuine coverage
gaps. The audit confirms that:

- **No coverage gap in primitives / splitter / bundled rules.** All three
  sub-suites are green with full coverage. The only proof gaps were the three
  Plan-01 additions below — these are *new acceptance proofs*, not patches to
  missing Phase 1–5 coverage.
- **Plan-01 proof additions (D-03/D-06/D-07):** byte-equality for `explain`,
  the hot-reload two-invocation proof, and the hermetic + `#[ignore]` real pnpm
  E2E pair. These exercise acceptance-level guarantees that the existing
  substring/unit tests did not assert byte-for-byte.
- **Count correction (cosmetic, not a gap):** the chain-splitter suite has **19**
  `#[test]` functions, not 20. The "20" figure counted a literal `#[test]` that
  appears inside the file's module doc-comment (`chain_split.rs:2`). All 13 spec
  scenarios remain fully covered; this is a documentation-count correction, not a
  coverage shortfall.
- **Cross-references to Plan 02 (not Plan 01 gaps):** `REQ-acceptance-cold-start-budget`
  (the `tracker_open` steady-state split, benchmark entry point) and the
  CI-hermetic sub-claim of `REQ-acceptance-test-coverage` (`.github/workflows/ci.yml`)
  are explicitly owned by **Plan 02** and are intentionally out of Plan 01 scope.

---

## Scope note

Plan 01 closes SC4 (test-coverage audit + the primitive/splitter/bundled green
confirmation), SC3 (explain byte-equality), SC2 second-half (hot reload), and
SC2 first-half (pnpm end-to-end, hermetic + real). SC1 (cold-start budget /
`tracker_open` resolution) and the CI-hermetic sub-claim are Plan 02's
responsibility and are cross-referenced above.
