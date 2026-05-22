# Phase 6: v1 ship gate — acceptance & docs - Context

**Gathered:** 2026-05-22 (assumptions mode)
**Status:** Ready for planning

<domain>
## Phase Boundary

Pass the v1 ship gate by **validating** all v1 acceptance criteria end-to-end (cold start, hot reload, `pnpm` E2E, `explain` reproducibility, hermetic test coverage) and **shipping** the user-facing documentation set (README, worked example, primitive reference). This is the gate at which v1 is shippable.

**In scope:** auditing/consolidating existing test coverage into a verifiable acceptance bar, authoring a reproducible cold-start benchmark entry point and recording real numbers, resolving the deferred Phase 2 `tracker_open` fsync regression, proving hot reload with a test, building the `pnpm` end-to-end acceptance test (real + hermetic-stub), standing up hermetic CI, and writing the three docs. Fixing any gap the gate exposes (e.g. a primitive edge or splitter scenario found missing during the audit) is in scope — the gate must actually pass.

**Out of scope:** any new engine primitive, new CLI command, new bundled rule, Tier 2 rules, or any v2 backlog item. Phases 1–5 are COMPLETE — Phase 6 only validates and documents what already ships; it does not add product capability.

Depends on: Phases 1–5 (all complete).
</domain>

<decisions>
## Implementation Decisions

### A. Acceptance coverage — audit, don't re-author

- **D-01:** SC4's test-coverage criterion is satisfied primarily by **auditing existing Phase 1–5 tests**, not writing new ones. The coverage already on disk: all 10 native primitives have byte-exact golden tests (`crates/lacon-core/tests/primitives.rs:46-170`); the chain splitter has 20 `#[test]`s covering the 13 spec scenarios (`crates/lacon-adapter-claudecode/tests/chain_split.rs`); every bundled rule is fixture-walked with ≥50% reduction + zero error-line-drop assertions (`crates/lacon-core/tests/bundled_rules.rs:160-209`). Planner produces a coverage **audit/traceability map** (REQ → existing test) and only fills genuine gaps the audit exposes.
- **D-02:** REQ-acceptance-bundled-reduction is treated as **already met** by Phase 5's `bundled_rules.rs` (it asserts `len(expected)/len(input) <= 0.5` on primary success fixtures plus `must_keep_lines`). Phase 6 re-confirms it runs green and references it in the acceptance map — no new reduction harness.
- **D-03:** REQ-acceptance-explain-reproducibility (SC3) is verified by confirming/strengthening the existing `lacon explain` byte-replay path (`crates/lacon-cli/src/commands/explain.rs`, tests in `crates/lacon-cli/tests/cli_explain.rs`) so that the filtered column re-derived from stored raw bytes is byte-for-byte identical to what `lacon run` originally emitted. This is verification + (if needed) one explicit byte-equality test, not new machinery.

### B. Cold-start benchmark & the deferred tracker_open regression

- **D-04:** The reproducible benchmark is the **already-checked-in `benches/cold_start.rs`** (`cold_start_probe`), wrapped in a single committed entry point (shell script or `Makefile`/`cargo` alias) that records `--version`, `validate`, and **the `lacon run` hook hot path** with per-OS labeling (it already prints `std::env::consts::OS`). Phase 6 does NOT author a new benchmark harness.
- **D-05:** Phase 6 MUST resolve the Phase 2 deferred `tracker_open` regression before closing the cold-start contract. The criterion gate at `crates/lacon-core/benches/tracker_open.rs` (`BUDGET_MICROS=3700`) trips at ~25000µs on ext4, fsync-dominated at migration COMMIT (documented in `.planning/phases/02-local-tracking/02-PHASE-BENCH.md`). Resolution = **split first-ever DB creation (once-per-machine) from steady-state `Tracker::open`** and gate the budget on steady-state, and/or re-measure on tmpfs — so the `lacon run` cold-start number reflects the real hot path, not one-time DB creation. The cold-start gate must exercise the `lacon run` path (which touches `Tracker::open`), not only the lazy-open `--version`/`validate` paths.

### C. Hot reload — prove, don't build

- **D-06:** Hot reload (REQ-acceptance-hot-reload, SC2 second half) is **already satisfied by the no-daemon architecture** (ADR-0013): every `lacon run`/`lacon-claude-hook` is a fresh OS process that re-reads rule files from disk, so a mid-session rule edit takes effect on the next invocation. The in-process mtime cache (`crates/lacon-core/src/rules/loader.rs:87-88, 262-274`) already invalidates on mtime change. Phase 6 ships a **proof test** demonstrating "edit rule file → next invocation reflects the change," NOT a file-watcher or any new cache mechanism (a watcher would contradict the locked no-daemon ADR).

### D. pnpm end-to-end (real test + hermetic stub) — CHOSEN

- **D-07:** REQ-acceptance-pnpm-end-to-end is delivered as **two artifacts**: (1) a **`#[ignore]`-gated real test** that runs an actual `pnpm install` through the full `lacon init` → `PreToolUse` hook rewrite → `lacon run` path, runnable by hand via `cargo test -- --ignored` and documented in a runbook; and (2) a **hermetic CI test** that drives the same init→hook→run pipeline using the existing `test_emitter` stub binary (`bin/test_emitter/src/main.rs`, pattern at `crates/lacon-cli/tests/end_to_end.rs:30-77`) so the default `cargo test` and CI never invoke `pnpm`. This reconciles SC2 (real pnpm passes) with SC4 (CI never installs pnpm).

### E. CI — stand up hermetic GitHub Actions — CHOSEN

- **D-08:** Phase 6 **creates** the CI config (no `.github/` exists yet) as a GitHub Actions workflow that runs `cargo build` + `cargo test` and the cold-start benchmark, **hermetic by construction**: never installs `pnpm`/`vitest`/`cargo`-tools/etc., and the real-pnpm test stays `#[ignore]`d out of the CI lane. Hermeticity is reinforced by existing design — `rusqlite[bundled]` (no system libsqlite3), fixtures are static text, `test_emitter` replaces real toolchains.
- **D-09:** The CI workflow runs **both an `ubuntu-latest` and a `macos-latest` lane**, and the macOS lane is what produces SC1's macOS cold-start number that the developer cannot generate locally (Linux-only dev machine). SC1's "<10ms on both macOS AND Linux" is closed by: Linux measured locally + in CI, macOS measured on the `macos-latest` runner. Researcher should advise on shared-runner measurement noise for a sub-10ms wall-clock budget.

### F. Documentation deliverables

- **D-10:** Three docs ship as Markdown linked from the repo root README's Documentation section:
  - **README** (REQ-docs-readme): rewrite the current 24-line design-status stub (`README.md:1-24`) into install + quickstart.
  - **Worked example** (REQ-docs-worked-example): a new `docs/worked-example.md` walking through writing a project-specific filter rule (extract/polish from the existing `docs/specs/filter-rule-schema.md:213-233` "Worked example" material rather than green-fielding).
  - **Primitive reference** (REQ-docs-primitive-reference): a new **`docs/primitive-reference.md`** (CHOSEN over expanding the schema spec) — one worked input→output example per primitive, all ten primitives covered. Source the canonical behavior from `docs/specs/filter-rule-schema.md:98-152` to avoid drift, but the user-facing reference is its own file.

### Claude's Discretion

- Exact form of the benchmark entry point (shell script vs. `Makefile` target vs. `cargo` alias) — D-04.
- Plan decomposition: e.g. an "acceptance validation" plan (D-01..D-07), a "CI + benchmark" plan (D-04, D-05, D-08, D-09), and a "docs" plan (D-10) — vs. another split. Planner's call, optimizing for the wave-based executor; docs are independent of the acceptance/CI work and can run in parallel.
- Whether the steady-state vs. first-ever `Tracker::open` split (D-05) is a new bench variant, a code change in the open path, or a documented measurement protocol — planner/researcher's call based on what the measurement shows.
- Whether the hot-reload proof (D-06) is a CLI black-box test (two `lacon run`s across an edit) or a loader unit test mutating mtime — both are acceptable.

### Folded Todos

None — `gsd-sdk query todo.match-phase 6` returned 0 matches.
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

- `.planning/REQUIREMENTS.md` — the 9 Phase 6 requirements (acceptance criteria + docs) with their exact wording.
- `.planning/ROADMAP.md` (Phase 6 section) — the 5 success criteria this gate must close.
- `docs/specs/filter-rule-schema.md` — primitive list + examples + worked-example material; the source of truth for the primitive-reference and worked-example docs (D-10).
- `docs/testing-rules.md` — hermetic-CI stance ("CI never installs pnpm/cargo/vitest", static fixtures, no live capture); governs D-07/D-08.
- `docs/architecture.md` — no-daemon / cold-start contract / stderr-merge ordering; governs D-04/D-06.
- `.planning/phases/02-local-tracking/02-PHASE-BENCH.md` — the deferred `tracker_open` fsync regression and its "re-measure on tmpfs, split first-ever vs steady-state" follow-up; the input to D-05.
- `benches/cold_start.rs` and `crates/lacon-core/benches/tracker_open.rs` — the existing bench artifacts D-04/D-05 build on.
- ADRs (LOCKED, `docs/decisions/`): ADR-0013 (PreToolUse subprocess wrapper — no daemon, cold-start load-bearing), ADR-0011 (SQLite WAL, the `Tracker::open` path), ADR-0005 (streaming-first), ADR-0010 (`on_error` replaces — explain branch fidelity).

**Researcher must investigate (carried from the analyzer's "Needs External Research"):**
1. **Trustworthy macOS cold-start measurement on shared CI runners** (D-09): how noisy `macos-latest` GitHub Actions runners are for a sub-10ms wall-clock budget, and how to get a defensible number (e.g. min-of-N, warmup discards, fixed runner image).
2. **Idiomatic Rust pattern for tool-dependent E2E tests** (D-07): confirm `#[ignore]` + `cargo test -- --ignored` runbook is the accepted split vs. `env`-gated skip or a separate non-default target, while keeping default `cargo test` hermetic.
3. **fsync-cost generalization across filesystems** (D-05): whether "first-ever DB creation is once-per-machine so steady-state is the real cold-start" holds across macOS APFS and common Linux filesystems — a measurement decision, not just code reading.
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `benches/cold_start.rs` — self-described Phase 6 acceptance gate; measures `--version`, `validate`, and hook hot-path scenarios; already prints `std::env::consts::OS` for per-OS capture (D-04).
- `crates/lacon-core/benches/tracker_open.rs` — criterion bench with `BUDGET_MICROS=3700`; the gate D-05 must split into first-ever vs steady-state.
- `crates/lacon-core/tests/primitives.rs:46-170` — golden tests for all 10 primitives (SC4 audit input).
- `crates/lacon-adapter-claudecode/tests/chain_split.rs` — 20 tests covering the 13-scenario splitter matrix (SC4 audit input).
- `crates/lacon-core/tests/bundled_rules.rs:160-209` — hermetic fixture walker asserting ≥50% reduction + zero error drops for all 10 rules (D-02, REQ-acceptance-bundled-reduction).
- `crates/lacon-cli/src/commands/explain.rs` + `crates/lacon-cli/tests/cli_explain.rs` — `lacon explain` byte-replay; SC3 verification target (D-03).
- `bin/test_emitter/src/main.rs` + `crates/lacon-cli/tests/end_to_end.rs:30-77` — the subprocess-stub pattern for the hermetic pnpm E2E test (D-07).
- `crates/lacon-core/src/rules/loader.rs:87-88, 262-274` — mtime-keyed rule cache that already invalidates on file change (D-06).
- `crates/lacon-cli/tests/tracking_e2e.rs`, `tracking_coldstart.rs` — existing E2E + cold-start test patterns.

### Established Patterns
- No daemon (ADR-0013): every invocation is a fresh process → hot reload is automatic; the in-process cache never spans two invocations (D-06).
- `rusqlite[bundled]` — no system libsqlite3, justified for "no version skew between macOS/Linux CI lanes" (`.planning/phases/02-local-tracking/02-CONTEXT.md`), reinforcing hermetic CI (D-08).
- Whole suite uses plain `assert_eq!`; `insta` declared but unused — do not introduce it.

### Integration Points
- New CI workflow file under `.github/workflows/` (currently absent) with ubuntu + macos lanes (D-08/D-09).
- New benchmark entry point at repo root or `scripts/` wrapping `cold_start_probe` (D-04).
- New docs: rewrite `README.md`; new `docs/worked-example.md`; new `docs/primitive-reference.md` (D-10).
- New `#[ignore]`d pnpm E2E test + hermetic stub variant under `crates/lacon-cli/tests/` (D-07).
- Possible code/bench change in the `Tracker::open` path or a new bench variant for the steady-state split (D-05).
</code_context>

<specifics>
## Specific Ideas

- The dev machine is **Linux only** — macOS numbers come exclusively from the `macos-latest` CI runner (D-09).
- Cold-start prior art: `lacon --version` median ~1154µs, `validate` ~1259µs (Phase 1) — already well under 10ms for the lazy-open paths; the open question is the `lacon run` path that touches `Tracker::open` (D-05).
- The repo `repository` field points at GitHub (`Cargo.toml:11`), making GitHub Actions the natural CI target (D-08).
- README currently states "in design. No installable artifact yet" — must flip to a real install + quickstart (D-10).
</specifics>

<deferred>
## Deferred Ideas

- Static musl builds / distroless container artifacts — v2 backlog (`docs/backlog.md`), not a v1 ship-gate item.
- `scripts/capture-fixtures.sh` live-recapture helper — referenced in `docs/testing-rules.md` but not required to close Phase 6; create only if cheap.
- Native Windows CI lane — out of v1 scope (macOS + Linux only).
- User-facing fixture validation (`lacon validate --fixtures`) and trend/cost docs — v2 backlog.

### Reviewed Todos (not folded)
None — no pending todos matched this phase.
</deferred>
