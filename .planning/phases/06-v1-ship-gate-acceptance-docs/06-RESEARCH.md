# Phase 6: v1 ship gate — acceptance & docs - Research

**Researched:** 2026-05-22
**Domain:** Acceptance validation, benchmark methodology (cold start), hermetic CI (GitHub Actions), Rust test-gating conventions, user-facing documentation
**Confidence:** HIGH (codebase verification + official docs); MEDIUM on macOS-runner noise (community-sourced, cross-verified)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01 (audit, don't re-author):** SC4 test-coverage is satisfied primarily by **auditing existing Phase 1–5 tests**, not writing new ones. Coverage on disk: all 10 native primitives have byte-exact golden tests (`crates/lacon-core/tests/primitives.rs:46-170`); the chain splitter has 20 `#[test]`s covering the 13 spec scenarios (`crates/lacon-adapter-claudecode/tests/chain_split.rs`); every bundled rule is fixture-walked with ≥50% reduction + zero error-line-drop assertions (`crates/lacon-core/tests/bundled_rules.rs:160-209`). Planner produces a coverage **audit/traceability map** (REQ → existing test) and only fills genuine gaps the audit exposes.
- **D-02 (bundled reduction already met):** REQ-acceptance-bundled-reduction is treated as **already met** by Phase 5's `bundled_rules.rs` (asserts `len(expected)/len(input) <= 0.5` on primary success fixtures plus `must_keep_lines`). Phase 6 re-confirms green and references it — no new reduction harness.
- **D-03 (explain reproducibility):** REQ-acceptance-explain-reproducibility (SC3) is verified by confirming/strengthening the existing `lacon explain` byte-replay path (`crates/lacon-cli/src/commands/explain.rs`, tests in `crates/lacon-cli/tests/cli_explain.rs`) so the filtered column re-derived from stored raw bytes is byte-for-byte identical to what `lacon run` originally emitted. Verification + (if needed) one explicit byte-equality test, not new machinery.
- **D-04 (benchmark entry point):** The reproducible benchmark is the **already-checked-in `benches/cold_start.rs`** (`cold_start_probe`), wrapped in a single committed entry point (shell script / `Makefile` / `cargo` alias) that records `--version`, `validate`, and **the `lacon run` hook hot path** with per-OS labeling (it already prints `std::env::consts::OS`). Phase 6 does NOT author a new benchmark harness.
- **D-05 (resolve tracker_open regression):** Phase 6 MUST resolve the Phase 2 deferred `tracker_open` regression before closing the cold-start contract. Gate at `crates/lacon-core/benches/tracker_open.rs` (`BUDGET_MICROS=3700`) trips at ~25000µs on ext4, fsync-dominated at migration COMMIT. Resolution = **split first-ever DB creation (once-per-machine) from steady-state `Tracker::open`** and gate the budget on steady-state, and/or re-measure on tmpfs — so the `lacon run` cold-start number reflects the real hot path, not one-time DB creation. The cold-start gate must exercise the `lacon run` path (touches `Tracker::open`), not only the lazy-open `--version`/`validate` paths.
- **D-06 (hot reload — prove, don't build):** Hot reload (SC2 second half) is **already satisfied by the no-daemon architecture** (ADR-0013): every `lacon run`/`lacon-claude-hook` is a fresh OS process re-reading rule files from disk; the in-process mtime cache (`crates/lacon-core/src/rules/loader.rs:87-88, 262-274`) invalidates on mtime change. Phase 6 ships a **proof test**, NOT a file-watcher or new cache mechanism (a watcher would contradict the locked no-daemon ADR).
- **D-07 (pnpm E2E — real test + hermetic stub):** Two artifacts: (1) a **`#[ignore]`-gated real test** running an actual `pnpm install` through `lacon init` → `PreToolUse` hook rewrite → `lacon run`, runnable via `cargo test -- --ignored` and documented in a runbook; and (2) a **hermetic CI test** driving the same init→hook→run pipeline using the existing `test_emitter` stub binary (`bin/test_emitter/src/main.rs`, pattern at `crates/lacon-cli/tests/end_to_end.rs:30-77`) so default `cargo test` and CI never invoke `pnpm`.
- **D-08 (stand up hermetic GitHub Actions):** Phase 6 **creates** the CI config (no `.github/` exists yet) as a GitHub Actions workflow running `cargo build` + `cargo test` + the cold-start benchmark, **hermetic by construction**: never installs `pnpm`/`vitest`/`cargo`-tools/etc., and the real-pnpm test stays `#[ignore]`d out of CI. Reinforced by existing design — `rusqlite[bundled]` (no system libsqlite3), fixtures are static text, `test_emitter` replaces real toolchains.
- **D-09 (ubuntu + macos lanes):** The CI workflow runs **both an `ubuntu-latest` and a `macos-latest` lane**; the macOS lane produces SC1's macOS cold-start number the developer cannot generate locally (Linux-only dev machine). SC1's "<10ms on both" is closed by: Linux measured locally + in CI, macOS measured on the `macos-latest` runner. Researcher to advise on shared-runner measurement noise for a sub-10ms wall-clock budget (see Pitfall 1).
- **D-10 (three docs):** Markdown linked from repo-root README Documentation section:
  - **README** (REQ-docs-readme): rewrite the current 24-line design-status stub (`README.md:1-24`) into install + quickstart.
  - **Worked example** (REQ-docs-worked-example): new `docs/worked-example.md` extracting/polishing the existing `docs/specs/filter-rule-schema.md:213-233` "Worked example" material rather than green-fielding.
  - **Primitive reference** (REQ-docs-primitive-reference): new **`docs/primitive-reference.md`** (CHOSEN over expanding the schema spec) — one worked input→output example per primitive, all ten covered. Source canonical behavior from `docs/specs/filter-rule-schema.md:98-152` to avoid drift.

### Claude's Discretion

- Exact form of the benchmark entry point (shell script vs `Makefile` target vs `cargo` alias) — D-04.
- Plan decomposition (e.g. "acceptance validation" plan D-01..D-07, "CI + benchmark" plan D-04/D-05/D-08/D-09, "docs" plan D-10 — vs another split). Docs are independent of acceptance/CI work and can run in parallel.
- Whether the steady-state vs first-ever `Tracker::open` split (D-05) is a new bench variant, a code change in the open path, or a documented measurement protocol — based on what the measurement shows.
- Whether the hot-reload proof (D-06) is a CLI black-box test (two `lacon run`s across an edit) or a loader unit test mutating mtime — both acceptable.

### Deferred Ideas (OUT OF SCOPE)

- Static musl builds / distroless container artifacts — v2 backlog.
- `scripts/capture-fixtures.sh` live-recapture helper — referenced in `docs/testing-rules.md` but not required; create only if cheap.
- Native Windows CI lane — out of v1 scope (macOS + Linux only).
- User-facing fixture validation (`lacon validate --fixtures`) and trend/cost docs — v2 backlog.
- Any new engine primitive, CLI command, bundled rule, Tier 2 rule, or v2 backlog item. Phases 1–5 are COMPLETE — Phase 6 only validates and documents what already ships.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| REQ-acceptance-bundled-reduction | All 10 bundled rules reduce ≥50% without dropping errors | Already enforced by `bundled_rules.rs:160-209` reduction + `must_keep_lines` assertions across 10 rule fixture trees (D-02). Audit = run `cargo test --test bundled_rules`, map to REQ. |
| REQ-acceptance-pnpm-end-to-end | `lacon init` → `pnpm install` works end-to-end, no manual config | Hermetic stub variant uses `test_emitter` (`end_to_end.rs:30-77` pattern) + `hook_e2e.rs` init→hook→run pipeline; real variant `#[ignore]`-gated per the project's own `runtime_signal.rs:47` convention (D-07). |
| REQ-acceptance-cold-start-budget | Cold-start <10ms on the hook hot path | `cold_start_probe` (`benches/cold_start.rs`) already measures `--version`, `validate`, and hook passthrough/rewrite scenarios with `consts::OS` labeling (D-04). Gap: must exercise the `Tracker::open`-touching `lacon run` path, not just lazy-open paths (D-05). |
| REQ-acceptance-explain-reproducibility | `lacon explain` reproduces filtering byte-for-byte | `explain.rs` already byte-replays via `Runner::filter_bytes` with ADR-0010 exit-code branch fidelity (WR-04 guard at `explain.rs:139,174`); `cli_explain.rs` has 5 tests. Add one explicit byte-equality test if missing (D-03). |
| REQ-acceptance-hot-reload | Rule edits take effect next invocation, no daemon/restart | Automatic via no-daemon (ADR-0013) + mtime cache (`loader.rs:262-274`). Ship a proof test (D-06). |
| REQ-acceptance-test-coverage | Suite covers each primitive, splitter (13), every bundled rule; CI hermetic | All present: `primitives.rs` (10), `chain_split.rs` (20 tests / 13 scenarios), `bundled_rules.rs` (10 rules). CI hermeticity from `rusqlite[bundled]` + static fixtures + `test_emitter` (D-01, D-08). |
| REQ-docs-readme | README install + quickstart | Rewrite `README.md:1-24` stub; install + quickstart + Documentation links (D-10). |
| REQ-docs-worked-example | Worked example: project-specific filter rule | Extract from `filter-rule-schema.md:213-233` into `docs/worked-example.md` (D-10). |
| REQ-docs-primitive-reference | Reference, ≥1 example per primitive | New `docs/primitive-reference.md`, source behavior from `filter-rule-schema.md:98-152` (D-10). |
</phase_requirements>

## Summary

Phase 6 is **validation + documentation, not new product code**. Phases 1–5 are complete and the test scaffolding the ship gate needs is overwhelmingly already on disk: 10 primitive golden tests, a 20-test/13-scenario chain splitter matrix, a fixture walker enforcing ≥50% reduction across all 10 bundled rules, a byte-replay `explain` path with exit-code branch fidelity, and a `cold_start_probe` binary that already measures the hook hot path with per-OS labeling. The phase's job is to (a) assemble those into a verifiable acceptance bar with a REQ→test traceability map, (b) close the three measurement/infrastructure gaps (cold-start gate on the `lacon run` path including the resolved `tracker_open` regression, the hermetic + real pnpm E2E pair, and a hot-reload proof), (c) stand up hermetic CI on ubuntu + macos lanes, and (d) ship three docs.

The three external-research questions all resolve cleanly. **(1) macOS noise:** `macos-latest` is now Apple Silicon (6-core M1, arm64) and substantially less noisy than the old 3-core Intel image, but a sub-10ms wall-clock budget on a shared VM is still borderline — the defensible move is to report the **minimum of N warmed samples** (noise is one-sided and additive; the minimum is the most stable statistic) and treat the gate as a soft regression check with generous headroom rather than a hard <10ms wall-clock assert on macOS CI. **(2) `#[ignore]` for tool-dependent tests:** confirmed idiomatic, and the project *already uses this exact pattern* with an explanatory message at `runtime_signal.rs:47` — make the real-pnpm test follow that house style. **(3) fsync generalization:** the 25ms ext4 figure is first-ever-DB-creation cost dominated by the migration COMMIT fsync; it is **once-per-machine**, not per-invocation. Steady-state `Tracker::open` (existing DB, no migration) is the real cold-start cost. The dev machine's `/tmp` is *already tmpfs*, so the 25ms must have come from a non-tmpfs path; the measurement protocol must explicitly distinguish first-ever vs steady-state and pick the right surface for each platform.

**Primary recommendation:** Treat the phase as three parallel-capable workstreams — (A) acceptance audit + the three proof/gate tests, (B) CI + cold-start benchmark entry point + tracker_open split, (C) the three docs. Make the cold-start gate a *steady-state* measurement on the `lacon run` path; make the macOS CI number a *reported minimum* with documented methodology, not a brittle hard assert; follow the existing `#[ignore = "..."]` convention for the real pnpm test.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Acceptance test audit / traceability | Test harness (existing `tests/` trees) | — | Coverage already lives in per-crate integration tests; the audit is a mapping artifact, not new code (D-01). |
| Cold-start benchmark | `benches/` operator tool (`cold_start_probe`) | CI workflow | Benchmark is a standalone bin (`benches/Cargo.toml`); CI invokes it per-lane for the macOS number (D-04/D-09). |
| `tracker_open` budget gate | `crates/lacon-core/benches/tracker_open.rs` (criterion, `harness=false`) | `Tracker::open` source path | Gate is a microbench; resolution may touch the open path or just the bench's measurement surface (D-05). |
| pnpm E2E (hermetic) | `crates/lacon-cli/tests/` + `bin/test_emitter` | adapter `hook_e2e` pattern | Stub binary replaces the real toolchain; init→hook→run drives the full pipeline (D-07). |
| pnpm E2E (real) | `#[ignore]`-gated test + runbook doc | — | Tool-dependent; excluded from default `cargo test` and CI lane (D-07/D-08). |
| Hot-reload proof | `crates/lacon-core/tests/` (loader) OR `crates/lacon-cli/tests/` (black-box) | — | No-daemon makes this a behavior assertion, not a feature (D-06). |
| Hermetic CI orchestration | `.github/workflows/` (new) | — | New file; orchestrates build/test/bench per OS lane (D-08/D-09). |
| User docs | `README.md`, `docs/*.md` | — | Pure documentation; independent of code (D-10). |

## Standard Stack

This phase introduces **no new runtime dependencies**. All "stack" choices are CI tooling and test-gating conventions that build on what already exists.

### Core (already in the workspace — verified)
| Component | Version | Purpose | Why Standard |
|-----------|---------|---------|--------------|
| `criterion` | 0.5 | `tracker_open` microbench gate | Already wired (`crates/lacon-core/Cargo.toml:41`, `[[bench]] harness=false`) [VERIFIED: codebase grep] |
| `assert_cmd` | 2 | Black-box CLI tests (`Command::cargo_bin`) | Already used across `tests/`; resolves the cargo artifact, not PATH (anti-spoofing, `end_to_end.rs:25-32`) [VERIFIED: codebase grep] |
| `predicates` | 3 | stdout/stderr assertions | Already used in `end_to_end.rs` [VERIFIED: Cargo.toml:32] |
| `tempfile` | 3 | Per-test tempdirs / tempfiles | Used by `cold_start.rs`, `tracker_open.rs`, every E2E test [VERIFIED: codebase grep] |
| `rusqlite` | 0.39 (`bundled`) | SQLite with no system libsqlite3 | The `bundled` feature is **load-bearing for CI hermeticity** — no system SQLite to install, no macOS/Linux version skew (D-08) [VERIFIED: Cargo.toml:27] |
| `test_emitter` | workspace bin | Deterministic stdout/stderr stub | Replaces real toolchains in hermetic E2E (`bin/test_emitter/src/main.rs`) [VERIFIED: codebase read] |

### Supporting (CI tooling — recommendations)
| Component | Version | Purpose | When to Use |
|-----------|---------|---------|-------------|
| `actions/checkout` | v4 | Clone repo in CI | Standard first step [CITED: docs.github.com GitHub-hosted runners] |
| `dtolnay/rust-toolchain` | `stable` (pinned) | Install Rust toolchain in CI | The de-facto community action for installing Rust; pin to `stable` and optionally an explicit version matching MSRV 1.80 [ASSUMED — verify action name before use] |
| `Swatinem/rust-cache` | v2 | Cache `~/.cargo` + `target/` between runs | Cuts CI time; cache is build-artifact only, does not affect hermeticity [ASSUMED — verify action name before use] |

> Note: GitHub-hosted runners ship Rust pre-installed, so an explicit toolchain action is optional. Using one makes the toolchain version deterministic across runner-image updates. Either way, **do not** add steps that `npm install`/`pip install`/`brew install` any rule's target tool — that breaks D-08 hermeticity.

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| `cold_start_probe` bin (D-04) | criterion wall-clock bench of subprocess spawn | criterion is for in-process microbenches; the cold-start budget is *whole-process* startup including dynamic linking — a subprocess-spawning bin (already built) is the correct shape. Keep `cold_start_probe`. |
| `#[ignore]` real-pnpm test (D-07) | env-gated `if std::env::var(...).is_err() { return }` early-return | Env-gated tests report as **"ok" (passed) when skipped**, which is misleading — a skipped test masquerades as a passing one. `#[ignore]` reports as "ignored". The project already chose `#[ignore]` (`runtime_signal.rs:47`). [VERIFIED: rust-lang/rust#68007 + codebase] |
| `#[ignore]` real-pnpm test | separate non-default `[[test]]` target or feature flag | More machinery than needed; `#[ignore]` + a runbook line is the lowest-friction split and matches house style. |
| Hard `<10ms` assert on macOS CI | reported-minimum + soft regression gate | A hard wall-clock assert on a shared VM will flake. Report the number; gate softly (Pitfall 1). |

**Installation:** None. No `cargo add`. This phase adds CI YAML, test files, a benchmark entry-point script, and Markdown docs.

## Package Legitimacy Audit

> No new packages are installed in Phase 6. All Rust dependencies already exist in the locked workspace (`Cargo.toml`), were vetted in Phases 1–5, and are unchanged. slopcheck does not apply (no PyPI/npm/crates install).

The only "external" artifacts are **GitHub Actions** (`actions/checkout`, optionally `dtolnay/rust-toolchain`, `Swatinem/rust-cache`). These are versioned GitHub Actions, not registry packages. Recommendation for the planner:

- Pin each action to a **major version tag** (`@v4`) or, for stricter supply-chain posture, a **full commit SHA**.
- The action *names* above are tagged `[ASSUMED]` — confirm the canonical owner/repo and current major version at plan time (e.g. via the GitHub Marketplace listing) before pinning. `dtolnay/rust-toolchain` and `Swatinem/rust-cache` are the widely-used community standards as of the training cutoff, but verify the slug and that the repo is the authentic one before adding it to a workflow.
- A workflow that uses **only** `actions/checkout` plus the pre-installed runner Rust toolchain is the most conservative, fully-hermetic baseline and avoids third-party actions entirely. Consider this as the default and add caching only if CI time becomes a problem.

**Packages removed due to slopcheck [SLOP] verdict:** none (no package installs).
**Packages flagged as suspicious [SUS]:** none. Action-pinning concern is tracked as A1 in the Assumptions Log.

## Architecture Patterns

### System Architecture Diagram

```
                       PHASE 6 SHIP GATE
                              │
        ┌─────────────────────┼──────────────────────┐
        ▼                     ▼                        ▼
  (A) ACCEPTANCE        (B) CI + BENCH            (C) DOCS
   AUDIT + PROOFS        + tracker_open            (D-10)
        │                     │                        │
   ┌────┴─────┐         ┌─────┴──────┐          ┌──────┴───────┐
   │          │         │            │          │              │
 REQ→test   3 proof   .github/     cold_start  README   worked-example
 traceab.   tests     workflows/   _probe      rewrite  + primitive-ref
 map        │         (yml)        entry pt    │              │
   │        │         │            │           └── link from root README
   │   ┌────┼────┐    │            │
   │   │    │    │    │      ┌─────┴─────┐
   │   pnpm hot- explain  ubuntu     macos
   │   E2E  reload byte-  lane       lane
   │   (stub) proof equal  │           │
   │   +real        test   cargo      cargo build+test
   │   (#ignore)           build+test + cold_start (min-of-N)
   │                       + cold_start  → macOS number (SC1)
   │                       (hermetic:    REPORTED, soft gate
   │                        no pnpm/     (Pitfall 1)
   │                        brew/npm)
   │
   └─► existing tests (run, assert green, map):
        primitives.rs(10) · chain_split.rs(20/13) · bundled_rules.rs(10)
        · cli_explain.rs(5) · hook_e2e.rs(28) · tracking_coldstart.rs(lazy-open invariant)

   tracker_open.rs gate (D-05):  first-ever (incl. migration fsync, ~25ms ext4)
                                 ──split──►  steady-state (existing DB, no migration)
                                 gate budget on STEADY-STATE; measure on tmpfs to
                                 isolate fsync; report both numbers.
```

### Recommended Project Structure (new artifacts only)
```
.github/
└── workflows/
    └── ci.yml              # NEW (D-08): ubuntu + macos lanes; build, test, cold_start
scripts/                    # NEW dir (D-04, if shell-script entry point chosen)
└── bench-cold-start.sh     # wraps `cargo run --release --bin cold_start_probe`
crates/lacon-cli/tests/
├── pnpm_e2e.rs             # NEW (D-07): hermetic stub variant + #[ignore] real variant
└── hot_reload.rs           # NEW (D-06): two-invocation black-box proof (or loader unit test)
crates/lacon-core/benches/
└── tracker_open.rs         # MODIFY (D-05): add steady-state variant; re-target the gate
crates/lacon-cli/tests/cli_explain.rs   # MODIFY (D-03): add byte-equality test if absent
README.md                   # REWRITE (D-10)
docs/
├── worked-example.md       # NEW (D-10)
└── primitive-reference.md  # NEW (D-10)
```

### Pattern 1: Hermetic E2E via `test_emitter` stub
**What:** Drive the real init→hook→run pipeline but substitute a deterministic Rust binary for the external tool, so CI never installs `pnpm`/`cargo`/`vitest`.
**When to use:** The hermetic half of D-07, and any acceptance test that would otherwise need a real toolchain.
**Example:**
```rust
// Source: crates/lacon-cli/tests/end_to_end.rs:30-77 (existing pattern)
fn test_emitter_path() -> std::path::PathBuf {
    // Resolves the cargo-built artifact, NOT a PATH lookup (anti-spoofing, T-07-04).
    assert_cmd::cargo::cargo_bin("test_emitter")
}

#[test]
fn pnpm_e2e_hermetic() {
    let dir = tempdir().unwrap();
    let emitter = test_emitter_path();
    let name = emitter.file_name().unwrap().to_str().unwrap();
    // Write a project rule that MATCHES the stub's invocation name, then run
    // through `lacon run` and assert the filtered result. For the full
    // init→hook→run chain, additionally drive lacon-claude-hook via stdin JSON
    // (see hook_e2e.rs run_hook_with_input) and assert the rewrite wraps the cmd.
    write_rule(dir.path(), &format!("id: pnpm-stub\nmatch: {{ command: {name} }}\npipeline:\n  - strip_ansi\n"));
    Command::cargo_bin("lacon").unwrap()
        .current_dir(dir.path())
        .args(["run", "--rule", "pnpm-stub", "--", emitter.to_str().unwrap(), "--stdout-lines", "3"])
        .assert().success().stdout(predicate::str::contains("line 1"));
}
```

### Pattern 2: Tool-dependent real test gated with `#[ignore]` (house style)
**What:** The real `pnpm install` test, excluded from default `cargo test` and CI.
**When to use:** The real half of D-07.
**Example:**
```rust
// Source: crates/lacon-core/tests/runtime_signal.rs:47 (the project's OWN convention)
#[ignore = "requires pnpm — run via `cargo test --test pnpm_e2e -- --ignored`"]
#[test]
fn pnpm_e2e_real() {
    // 1. `lacon init` in a fresh tempdir project.
    // 2. Drive lacon-claude-hook with a PreToolUse JSON payload for `pnpm install`.
    // 3. Execute the rewritten `lacon run --rule pkg-install -- pnpm install`.
    // 4. Assert filtered output is non-empty and reduced vs raw.
    // Document the exact runbook command in the README/runbook (D-07).
}
```
> The explanatory string in `#[ignore = "..."]` is the runbook line — it prints in test output so a developer sees exactly how to run it. Match this verbatim style.

### Pattern 3: Steady-state vs first-ever `Tracker::open` (D-05)
**What:** Split the criterion bench so the budget gate measures the realistic hot-path cost (existing DB, no migration) instead of once-per-machine DB creation.
**When to use:** D-05 resolution.
**Example (measurement surfaces, not final code — planner decides exact shape):**
```rust
// Current (crates/lacon-core/benches/tracker_open.rs:34-60): fresh tempdir EVERY
// iteration → migration COMMIT fsync in every sample → ~25ms on ext4.
//
// Add a second bench function that creates the DB ONCE (outside the timed loop),
// then times re-opening the EXISTING DB:
fn bench_tracker_open_steady_state(c: &mut Criterion) {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("lacon").join("history.db");
    // One-time creation OUTSIDE the timed section (migration paid once):
    drop(Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS).unwrap());
    c.bench_function("tracker_open_steady_state", |b| {
        b.iter_custom(|iters| {
            let start = Instant::now();
            for _ in 0..iters {
                let t = Tracker::open(&db_path, &default_retention(), false, FIXED_NOW_MS).unwrap();
                drop(t);
            }
            start.elapsed()
        });
    });
    // Gate the BUDGET on this steady-state number. Keep the first-run bench as a
    // REPORTED (non-gating) diagnostic, or re-measure it on tmpfs.
}
```

### Anti-Patterns to Avoid
- **Hard wall-clock `<10ms` assert in macOS CI:** shared VMs jitter; this flakes. Report the minimum; gate softly with headroom (Pitfall 1).
- **`brew install pnpm` / `npm i -g` in the CI workflow:** directly violates D-08 hermeticity. The real-pnpm test is `#[ignore]`d *specifically so CI never needs the tool*.
- **Adding a file-watcher / daemon for hot reload:** contradicts the locked no-daemon ADR-0013. Hot reload is already free; only prove it (D-06).
- **Gating the cold-start budget on first-ever DB creation:** that is once-per-machine cost, not the hot path. Gate on steady-state (D-05).
- **Introducing `insta`:** declared in `Cargo.toml:34` but **unused**; the whole suite uses plain `assert_eq!`. Do not introduce it (CONTEXT "Established Patterns").
- **Re-authoring coverage that already exists:** D-01 is audit-first. Writing a new primitive/splitter/bundled test that duplicates an existing one is wasted effort and out of the audit-don't-re-author mandate.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Skipping tool-dependent tests | Custom env-var skip logic | `#[ignore]` + `cargo test -- --ignored` | Built-in, reports "ignored" not "ok"; house style at `runtime_signal.rs:47` |
| Locating the test stub binary | `which`/PATH lookup | `assert_cmd::cargo::cargo_bin("test_emitter")` | Resolves the cargo artifact; anti-spoofing (`end_to_end.rs:25-32`) |
| SQLite in CI | System libsqlite3 install | `rusqlite` `bundled` feature (already on) | No install, no version skew across OS lanes (D-08) |
| Cold-start measurement | New bespoke harness | Existing `cold_start_probe` bin | D-04 mandates reusing it; already labels per-OS and covers hook scenarios |
| Reduction ratio assertion | New reduction harness | Existing `bundled_rules.rs` walker | D-02 mandates reuse; already asserts `≤0.5` + `must_keep_lines` |
| Hot-reload mechanism | File watcher / daemon | No-daemon process model (already true) | ADR-0013; D-06 says prove not build |
| Side-by-side diff for explain | LCS/Myers/diff crate | Existing hand-rolled renderer | `explain.rs:205` already does it (Phase 4 D-06); SC3 is about byte-fidelity of the *filtered* column, not diff prettiness |

**Key insight:** Phase 6 is almost entirely *reuse and assemble*. The single largest failure mode is building new machinery (a benchmark harness, a reduction harness, a hot-reload watcher, an env-skip helper) where a battle-tested existing artifact or a built-in language feature already covers it.

## Runtime State Inventory

> Phase 6 is validation + documentation. It changes no stored data, registers no OS state, and renames nothing. One narrow runtime-state concern exists — the SQLite DB on the cold-start measurement path — covered below.

| Category | Items Found | Action Required |
|----------|-------------|------------------|
| Stored data | `~/.local/share/lacon/history.db` is created/opened by the `lacon run` cold-start scenario. The cost asymmetry (first-ever creation incl. migration fsync vs steady-state re-open) is the entire substance of D-05. | Measurement protocol must pre-create the DB for steady-state runs and label first-ever runs separately. No schema change, no migration authored. |
| Live service config | None — Phase 6 ships no adapter/service config changes. The CI workflow file is new repo state, not live-service state. | None. |
| OS-registered state | None — no Task Scheduler / launchd / systemd / pm2 involvement. (`lacon init` writes `.claude/settings.json`, but that is Phase 3 and unchanged here; the pnpm E2E test uses tempdirs.) | None. The pnpm E2E tests MUST use tempdir projects so they never mutate the developer's real `.claude/settings.json`. |
| Secrets/env vars | `LACON_DISABLE`, `XDG_DATA_HOME`, `XDG_CONFIG_HOME` are read by existing tests to sandbox state (`tracking_coldstart.rs:37-38`). CI needs no secrets. | Reuse the XDG-redirect sandboxing pattern in new E2E tests so they never touch real user dirs. |
| Build artifacts | `target/release/lacon` and `target/release/lacon-claude-hook` must exist before `cold_start_probe` runs (it checks and errors otherwise — `cold_start.rs:107-110, 138`). | The benchmark entry point (D-04) MUST `cargo build --release` (all bins) first. CI must build release before invoking the probe. |

**Nothing found in OS-registered / live-service / secrets categories beyond the sandboxing note above — verified by grep for scheduler/daemon/launchd patterns and by reading the run/explain/tracking source.**

## Common Pitfalls

### Pitfall 1: Treating the macOS CI cold-start number as a hard <10ms wall-clock gate
**What goes wrong:** A `macos-latest` lane that asserts `median < 10ms` (or worse, `max < 10ms`) flakes intermittently because shared VMs have noisy neighbors, variable CPU, and one-sided positive jitter.
**Why it happens:** Wall-clock measurements on shared cloud VMs are contaminated by OS scheduling, co-tenant load, and (historically) CPU heterogeneity. The noise is **additive and one-sided** — it only ever makes a sample slower, never faster.
**How to avoid:**
- Report the **minimum of N warmed samples**, not mean/median. The minimum is the closest estimate of the "true" cost because noise can only inflate samples (MIT Edelman, "Robust benchmarking in noisy environments"). The existing probe already computes min (`cold_start.rs:82`) and discards 3 warm-up runs (`cold_start.rs:62-64`) — surface the **min** as the headline number.
- Use `macos-latest` (now Apple Silicon M1, 6-core/14GB, arm64 — far more consistent than the retired 3-core Intel image). Community finding: for macro-benchmarks **above ~1ms**, cross-CPU variance is largely negligible — and a 10ms budget with sub-2ms observed Linux numbers leaves enough headroom that even 3–5× macOS jitter stays under budget.
- Gate **softly on macOS**: assert the reported min is under a generous ceiling (the 10ms budget, which Linux clears at ~1–2ms with 5×+ headroom), and/or treat the macOS lane as **report-and-record** (write the number into `docs/architecture.md`'s measurements table) rather than a build-breaking assert. Keep the hard regression gate (the criterion `tracker_open` panic gate) on the deterministic in-process microbench, not on subprocess wall-clock.
- Document the methodology (sample count, warm-up discards, statistic used, runner image) alongside the number so it is *defensible*, per standard benchmark-reporting practice.
**Warning signs:** intermittent red builds only on the macOS lane; numbers that swing 2–10× run-to-run; a "passing" build that occasionally trips on `max`.

### Pitfall 2: Gating the cold-start budget on first-ever DB creation
**What goes wrong:** Wiring the 10ms gate to the `lacon run` path *with a fresh DB* reproduces the Phase 2 ~25ms `tracker_open` failure — the migration COMMIT fsync dominates and the gate trips even though real users only pay that cost once.
**Why it happens:** The first `Tracker::open` on a machine runs migrations and the WAL-mode pragma, both of which fsync. On a non-tmpfs filesystem a single fsync round-trip is routinely 5–25ms (`02-PHASE-BENCH.md:81-85`).
**How to avoid:** Split first-ever from steady-state (D-05, Pattern 3). Gate the budget on steady-state `Tracker::open` (existing DB, no migration). Report first-ever as a separate, non-gating diagnostic, and/or measure it on tmpfs to isolate the fsync component from Rust/SQLite cost. **Confirm the dev-machine observation:** `/tmp` *and* `/dev/shm` are tmpfs on this machine (verified `df -T`), so any 25ms figure came from a non-tmpfs path (likely real `~/.local/share` on the spinning-class allocation noted in the bench). The measurement must therefore be explicit about *which filesystem* each number reflects.
**Warning signs:** the new cold-start gate is green on `--version`/`validate` but red on `lacon run`; the failing number clusters tightly around 5–25ms (fsync signature) rather than scattering.

### Pitfall 3: A non-hermetic CI step sneaks in a toolchain install
**What goes wrong:** Someone adds `brew install pnpm` (or `npm i -g`, `pip install`) to make the "real" E2E pass in CI, silently violating D-08 and SC4.
**Why it happens:** The instinct to make the real test run everywhere conflicts with the hermetic mandate.
**How to avoid:** The real-pnpm test is `#[ignore]`d **precisely so CI never needs the tool**. The default `cargo test` (which CI runs) skips ignored tests automatically. Never add `-- --ignored` to the CI test step. Keep CI to `cargo build`, `cargo test` (no `--ignored`), and the cold-start probe. The `rusqlite[bundled]` feature already removes the one system-library temptation.
**Warning signs:** a `brew`/`apt`/`npm`/`pip` install line in `ci.yml`; the CI test step containing `--ignored`; CI green-time growing because it is downloading a package registry.

### Pitfall 4: pnpm/hot-reload tests mutate real user state
**What goes wrong:** A test runs `lacon init` or `Tracker::open` against the developer's real home dir, writing to `~/.claude/settings.json` or `~/.local/share/lacon/history.db`.
**Why it happens:** Forgetting to redirect HOME/XDG and using a real cwd instead of a tempdir.
**How to avoid:** Reuse the established sandboxing: tempdir projects + `XDG_DATA_HOME`/`XDG_CONFIG_HOME` redirection (`tracking_coldstart.rs:37-38`, `end_to_end.rs` `current_dir(dir.path())`). Every new E2E/proof test must be hermetic in this sense too.
**Warning signs:** test failures that depend on developer-machine state; `.claude/settings.json` showing up modified in `git status` after a test run.

### Pitfall 5: Doc drift between the new docs and the schema spec
**What goes wrong:** `docs/primitive-reference.md` and `docs/worked-example.md` describe behavior that diverges from `docs/specs/filter-rule-schema.md` (the contract) or from actual primitive behavior.
**Why it happens:** Green-fielding new docs instead of extracting from the canonical source.
**How to avoid:** D-10 is explicit — **source from the spec** (`filter-rule-schema.md:98-152` for primitives, `:213-233` for the worked example). Where possible, derive primitive-reference examples from the existing golden fixtures (`tests/fixtures/primitives/<name>/{input,expected}.txt`) so the doc examples are literally the tested behavior. This makes the docs *verifiable* against the fixtures and prevents drift.
**Warning signs:** a doc example whose output would not actually be produced by the primitive; `{count}` / regex-anchoring semantics described differently than the spec.

## Code Examples

### Driving the hook binary via stdin JSON (for the init→hook→run E2E)
```rust
// Source: crates/lacon-adapter-claudecode/tests/hook_e2e.rs (run_hook_with_input) +
//         benches/cold_start.rs:140-174 (PreToolUse payload shape)
let payload = serde_json::json!({
    "session_id": "test", "transcript_path": "/tmp/t.jsonl",
    "cwd": project_dir.to_string_lossy(), "permission_mode": "default",
    "hook_event_name": "PreToolUse", "tool_name": "Bash",
    "tool_input": { "command": "pnpm install" }, "tool_use_id": "tu-1"
}).to_string();
// Spawn lacon-claude-hook, write payload to stdin, read updatedInput JSON from stdout,
// assert it wraps the command as `lacon run --rule pkg-install -- pnpm install`.
```

### Existing reduction + must-keep assertion (the bundled-reduction acceptance, D-02)
```rust
// Source: crates/lacon-core/tests/bundled_rules.rs:160-209 (already enforces bundled reduction)
// The walker reads each tests/fixtures/<rule>/<scenario>/{input,expected,meta}.txt|yaml,
// replays the rule's pipeline (success or on_error per meta.exit_code), asserts byte-exact
// match, asserts len(expected)/len(input) <= 0.5 unless exempt, and checks must_keep_lines.
// Phase 6 action: `cargo test --test bundled_rules` → green → cite in the REQ→test map.
```

### Cold-start probe min/percentile reporting (already in the probe — surface min on macOS)
```rust
// Source: benches/cold_start.rs:62-89
// measure_cold_start discards 3 warm-ups, takes RUNS(=50) samples; run_scenario reports
// min/median/p95/max. For the macOS defensible number, the HEADLINE is `min`.
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `macos-latest` = 3-core Intel runner | `macos-latest` = Apple Silicon M1 (6-core, 14GB, arm64) | M1 runners GA'd 2024; `macos-latest` migrated to Apple Silicon | macOS CI is faster and less noisy than the Phase-2-era assumption; sub-10ms budget is more attainable but still needs min-of-N reporting [CITED: github.blog M1 runner] |
| Env-var early-return to skip tests | `#[ignore]` + `--ignored` | Long-standing Rust idiom | Skipped tests report "ignored" not "ok"; the project already adopted this (`runtime_signal.rs:47`) [VERIFIED: rust-lang/rust#68007] |
| Mean/median for noisy CI benchmarks | Minimum-of-N (noise is one-sided) | Established robust-benchmarking practice (Edelman et al.) | The defensible statistic for a wall-clock budget on shared VMs [CITED: MIT/arXiv robust benchmarking] |

**Deprecated/outdated:**
- The Phase-2 assumption that `tracker_open` cost is representative of every cold start: it is **first-ever-only** cost. Superseded by the steady-state framing (D-05).
- README's "in design. No installable artifact yet" line (`README.md:5`): must flip to install + quickstart (D-10).

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `dtolnay/rust-toolchain` and `Swatinem/rust-cache` are the canonical, authentic community actions | Standard Stack / Legitimacy Audit | Using a typo-squatted action repo in CI is a supply-chain risk. Mitigation: the conservative baseline uses ONLY `actions/checkout` + pre-installed runner Rust; verify any third-party action slug + pin to SHA before adding. |
| A2 | The 25ms `tracker_open` figure came from a non-tmpfs path (`~/.local/share` on spinning-class allocation), not `/tmp` | Pitfall 2 | If the regression reproduces on tmpfs too, the fix is not "measure on tmpfs" but a code change (async write / pre-create at init). Measurement (the D-05 task itself) settles this — re-run `cargo bench -p lacon-core --bench tracker_open` and a tmpfs variant and compare. |
| A3 | macOS M1 runner jitter stays within ~5× of the in-process number, keeping the subprocess cold start under the 10ms budget | Pitfall 1 | If macOS subprocess startup exceeds 10ms, SC1's macOS criterion needs renegotiation (e.g. budget the *steady-state in-process* cost on macOS, report subprocess wall-clock as informational). The macOS CI run is itself the measurement that settles this. |
| A4 | `cargo test` (default, no `--ignored`) skips `#[ignore]`d tests in CI | Pitfall 3 | This is standard Rust behavior; low risk. If wrong, CI would attempt the real pnpm test and fail-to-find-pnpm — caught immediately on first CI run. |
| A5 | Apple Silicon `macos-latest` is arm64 and the release profile builds cleanly there | Pitfall 1 / CI | The codebase uses portable crates (`nix` signal API "identical on macOS" per `architecture.md:174`); `rusqlite[bundled]` compiles on arm64. First macOS CI run validates. |

**These five assumptions are exactly the items a `checkpoint:human-verify` or first-CI-run will settle. None block planning; all are resolved by running the very tasks Phase 6 defines.**

## Open Questions

1. **What is the macOS cold-start number, and does it clear 10ms?**
   - What we know: Linux is ~1–2ms for lazy-open paths; the probe reports min after warm-up; M1 runners are consistent for >1ms macro-benchmarks.
   - What's unclear: the actual macOS subprocess-spawn wall-clock — the dev machine is Linux-only, so this is *only* knowable from the `macos-latest` lane.
   - Recommendation: the first macOS CI run **is** the measurement. Plan a task that runs the probe on macos-latest, records min-of-50 into `docs/architecture.md`, and gates softly (Pitfall 1). Do not pre-commit to a hard macOS assert in the plan.

2. **Does the steady-state `Tracker::open` clear the budget once first-ever creation is excluded?**
   - What we know: first-ever is ~25ms on non-tmpfs (fsync-dominated); steady-state has no migration and no fsync-heavy COMMIT.
   - What's unclear: the exact steady-state number — not yet measured.
   - Recommendation: the D-05 bench split produces it. If steady-state is under ~2.5ms (the original Phase 2 delta target), the contract holds and the gate re-targets cleanly. If not, escalate to the Phase-2-listed options (pre-create DB at `lacon init` time; async write) — but only if measurement shows it is needed.

3. **Is there a genuine coverage gap the audit will expose?**
   - What we know: primitives (10), splitter (13 scenarios / 20 tests), bundled rules (10) all have tests; explain has 5; hook E2E has 28.
   - What's unclear: whether any single REQ has a subtle uncovered edge (e.g. a primitive interaction, a splitter corner) until the traceability map is built.
   - Recommendation: build the REQ→test map first (D-01); only then decide if any gap-filling test is warranted. Expectation: few-to-zero genuine gaps.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain (cargo) | All build/test/bench | ✓ | rust-version 1.80 (workspace MSRV); local 1.94.x per `02-PHASE-BENCH.md:35` | — |
| `criterion` | tracker_open gate | ✓ | 0.5 (`lacon-core/Cargo.toml:41`) | — |
| `assert_cmd`/`predicates`/`tempfile` | E2E tests | ✓ | 2 / 3 / 3 | — |
| tmpfs (`/tmp`, `/dev/shm`) | D-05 tmpfs re-measurement | ✓ | both tmpfs (verified `df -T`) | — (already available) |
| `pnpm` | **real** pnpm E2E only | ✗ (not checked / not required) | — | Hermetic stub variant via `test_emitter` covers CI; real test is `#[ignore]`d (D-07) |
| GitHub Actions (`ubuntu-latest`, `macos-latest`) | CI lanes (D-08/D-09) | ✓ (repo is GitHub: `Cargo.toml:11`) | ubuntu-latest, macos-latest=M1 | — |
| `git`/`gh` | committing CI + docs | ✓ | — | — |

**Missing dependencies with no fallback:** None that block Phase 6. (`pnpm` absence is *by design* — CI must not need it.)
**Missing dependencies with fallback:** `pnpm` → `test_emitter` hermetic stub for the CI-facing acceptance; the real test stays opt-in.

## Validation Architecture

> `workflow.nyquist_validation = true` (config.json) — section included.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + `assert_cmd`/`predicates` (integration), `criterion` (bench) |
| Config file | None (cargo convention); `[[bench]] harness=false` for `tracker_open` (`lacon-core/Cargo.toml:43`) |
| Quick run command | `cargo test --workspace` (skips `#[ignore]`d real-pnpm test by default) |
| Full suite command | `cargo test --workspace` then `cargo test --workspace -- --ignored` (real-pnpm, local only); `cargo bench -p lacon-core --bench tracker_open` (gate); `cargo run --release --bin cold_start_probe` (cold-start) |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| REQ-acceptance-bundled-reduction | 10 rules ≥50% reduction, no error drops | integration (fixture walker) | `cargo test --test bundled_rules` | ✅ `crates/lacon-core/tests/bundled_rules.rs` |
| REQ-acceptance-pnpm-end-to-end (hermetic) | init→hook→run with stub | integration | `cargo test -p lacon-cli --test pnpm_e2e` | ❌ Wave 0 — new file (D-07) |
| REQ-acceptance-pnpm-end-to-end (real) | real pnpm install filtered | integration `#[ignore]` | `cargo test -p lacon-cli --test pnpm_e2e -- --ignored` | ❌ Wave 0 — new file (D-07) |
| REQ-acceptance-cold-start-budget | <10ms hook hot path | bench bin + criterion gate | `cargo run --release --bin cold_start_probe`; `cargo bench -p lacon-core --bench tracker_open` | ✅ probe exists; ⚠️ tracker_open needs steady-state split (D-05) |
| REQ-acceptance-explain-reproducibility | byte-exact filtered replay | integration | `cargo test -p lacon-cli --test cli_explain` | ✅ 5 tests; ⚠️ add explicit byte-equality test if absent (D-03) |
| REQ-acceptance-hot-reload | edit→next invocation reflects change | integration or loader unit | `cargo test -p lacon-cli --test hot_reload` (or loader unit) | ❌ Wave 0 — new file (D-06) |
| REQ-acceptance-test-coverage (primitives) | 10 primitives golden | integration | `cargo test --test primitives` | ✅ `primitives.rs` (10) |
| REQ-acceptance-test-coverage (splitter) | 13 scenarios | integration | `cargo test -p lacon-adapter-claudecode --test chain_split` | ✅ `chain_split.rs` (20 tests / 13 scenarios) |
| REQ-acceptance-test-coverage (CI hermetic) | no toolchain installs | CI assertion | hermetic by construction (no install steps) | ❌ Wave 0 — `.github/workflows/ci.yml` (D-08) |
| REQ-docs-readme / worked-example / primitive-reference | docs ship + link | manual review + (optional) doc-example-vs-fixture check | n/a (markdown) | ❌ Wave 0 — new/rewritten files (D-10) |

### Sampling Rate
- **Per task commit:** `cargo test --workspace` (the relevant crate's tests at minimum).
- **Per wave merge:** `cargo test --workspace` + `cargo bench -p lacon-core --bench tracker_open` (the gate).
- **Phase gate:** full suite green + `cold_start_probe` recorded on both OS lanes + the three docs reviewed, before `/gsd:verify-work`.

### Wave 0 Gaps
- [ ] `crates/lacon-cli/tests/pnpm_e2e.rs` — hermetic stub + `#[ignore]` real (REQ-acceptance-pnpm-end-to-end)
- [ ] `crates/lacon-cli/tests/hot_reload.rs` (or loader unit test) — covers REQ-acceptance-hot-reload
- [ ] `crates/lacon-core/benches/tracker_open.rs` — add steady-state variant; re-target the gate (D-05)
- [ ] `crates/lacon-cli/tests/cli_explain.rs` — add explicit byte-equality test if the 5 existing ones don't already assert byte-for-byte equality vs original run output (D-03)
- [ ] `.github/workflows/ci.yml` — hermetic ubuntu + macos lanes (D-08/D-09)
- [ ] benchmark entry point (`scripts/bench-cold-start.sh` or `Makefile`/cargo alias) — must `cargo build --release` then run the probe (D-04)
- [ ] `README.md` rewrite, `docs/worked-example.md`, `docs/primitive-reference.md` (D-10)

*(No framework install needed — Rust test infra is fully in place.)*

## Security Domain

> `security_enforcement` is not set in config.json → treated as enabled. Phase 6 introduces no auth/session/network surface; the relevant slice is supply-chain (CI actions) and the already-implemented terminal-injection defense in `explain`.

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | No auth surface (local-only CLI). |
| V3 Session Management | no | No sessions. |
| V4 Access Control | no | Local filesystem only; `0700` on the DB dir is Phase 2 (unchanged). |
| V5 Input Validation | yes (existing) | `lacon validate` rejects bad rules at load; `explain` parses ids safely (`explain.rs:29-35`, never `unwrap` on user input). |
| V6 Cryptography | no | No crypto in v1 (redaction/encryption are backlog). |
| V14 Configuration / Supply Chain | yes (new) | Pin GitHub Actions to major-version tags or commit SHAs; prefer the `actions/checkout`-only baseline; verify third-party action slugs before adding (A1). |

### Known Threat Patterns for GitHub Actions CI + terminal output
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Typo-squatted / malicious GitHub Action | Tampering / Elevation | Pin to SHA or major-version tag; verify owner/repo; minimal-action baseline (A1). |
| Untrusted runner secrets exposure | Information Disclosure | Phase 6 needs **no secrets**; do not add `secrets.*` to the workflow. |
| Terminal-control injection via stored raw build logs in `lacon explain` | Tampering (terminal hijack) | Already mitigated: filtered ("safe view") column neutralizes C0/C1/ESC bytes (`explain.rs:235-248`, WR-01). SC3's byte-fidelity contract preserves the RAW column verbatim by design — keep both behaviors intact when adding the D-03 byte-equality test. |
| Test mutating real user state / secrets | Tampering | Sandbox via tempdir + XDG redirection (Pitfall 4). |

## Sources

### Primary (HIGH confidence)
- Codebase (verified via Read/grep): `benches/cold_start.rs`, `crates/lacon-core/benches/tracker_open.rs`, `crates/lacon-cli/src/commands/{run,explain}.rs`, `crates/lacon-core/src/rules/loader.rs`, `crates/lacon-cli/tests/{end_to_end,tracking_coldstart,cli_explain}.rs`, `crates/lacon-core/tests/{primitives,bundled_rules}.rs`, `crates/lacon-adapter-claudecode/tests/{chain_split,hook_e2e}.rs`, `crates/lacon-core/tests/runtime_signal.rs`, `bin/test_emitter/src/main.rs`, `Cargo.toml`, `README.md`
- Project docs: `docs/architecture.md` (cold-start contract, no-daemon, signal/merge decisions), `docs/testing-rules.md` (hermetic CI stance), `docs/specs/filter-rule-schema.md` (primitive + worked-example source), `.planning/phases/02-local-tracking/02-PHASE-BENCH.md` (tracker_open regression)
- CONTEXT.md D-01..D-10 (locked decisions)
- [rust-lang/rust #68007](https://github.com/rust-lang/rust/issues/68007) — `#[ignore]` vs runtime skip reporting semantics
- [The Rust Book — Test Organization](https://doc.rust-lang.org/book/ch11-03-test-organization.html) — `#[ignore]` + `--ignored` convention
- [github.blog — Apple silicon M1 macOS runner](https://github.blog/news-insights/product-news/introducing-the-new-apple-silicon-powered-m1-macos-larger-runner-for-github-actions/) — runner specs (6-core, 14GB, arm64)
- [GitHub-hosted runners reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners) — runner images

### Secondary (MEDIUM confidence)
- [MIT — Robust benchmarking in noisy environments (Edelman)](https://math.mit.edu/~edelman/publications/robust_benchmarking.pdf) / [arXiv 1608.04295](https://arxiv.org/pdf/1608.04295) — minimum-of-N as the robust statistic; one-sided additive noise
- [CodSpeed — Why glibc is faster on some GitHub Actions runners](https://codspeed.io/blog/unrelated-benchmark-regression) — macro-benchmarks >1ms have negligible cross-CPU variance
- [actions/runner-images #1336](https://github.com/actions/runner-images/issues/1336) — macOS runner performance/variance reports

### Tertiary (LOW confidence — flagged for verification)
- `dtolnay/rust-toolchain`, `Swatinem/rust-cache` action slugs (training knowledge; verify before pinning — A1)

## Metadata

**Confidence breakdown:**
- Standard stack / existing coverage audit: HIGH — verified by reading the actual test files and counting tests.
- Architecture / cold-start / tracker_open framing (D-04/D-05): HIGH — bench source + Phase 2 bench report read directly; tmpfs availability verified on the dev machine.
- Pitfalls — `#[ignore]` convention (D-07): HIGH — confirmed idiomatic AND already used in-repo (`runtime_signal.rs:47`).
- Pitfalls — macOS CI noise / measurement strategy (D-09): MEDIUM — runner specs from official sources; noise behavior cross-verified from multiple community + academic sources; the actual macOS number is only knowable from the first CI run (Open Question 1).
- Docs (D-10): HIGH — source material located and line-referenced.
- CI action pinning (D-08): MEDIUM — workflow shape is HIGH; specific third-party action slugs are ASSUMED (A1).

**Research date:** 2026-05-22
**Valid until:** 2026-06-21 (30 days — stable domain; the only fast-moving element is GitHub runner images, which only ever get faster/quieter, making the conclusions conservative)
