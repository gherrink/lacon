# Phase 06 — Deferred Items

Out-of-scope discoveries logged during execution. Not fixed by the plan that
found them (SCOPE BOUNDARY rule: only auto-fix issues DIRECTLY caused by the
current task's changes).

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| test-infra (pre-existing) | `lacon-cli` integration tests fail with `CARGO_BIN_EXE_test_emitter is unset` | Open — blocks `cargo test --workspace` going green | Plan 06-02, Task 3 |

## Detail: `CARGO_BIN_EXE_test_emitter` unset (pre-existing)

**Discovered:** Plan 06-02, Task 3, while validating that the new CI workflow's
`cargo test --workspace` step would pass locally.

**Symptom:** `cargo test --workspace` (and `cargo test -p lacon-cli --test
cli_doctor` / `--test end_to_end` / `--test tracking_e2e` in isolation) panics:

```
`CARGO_BIN_EXE_test_emitter` is unset
help: available binary names are "lacon"
```

**Affected test binaries** (all in `crates/lacon-cli/tests/`):
`cli_doctor.rs`, `end_to_end.rs`, `tracking_e2e.rs` — every test that calls
`assert_cmd::cargo::cargo_bin("test_emitter")`.

**Root cause (pre-existing, NOT caused by this plan):** `assert_cmd` is pinned
to `2.2.1` in `Cargo.lock`. Its `cargo_bin(name)` reads `CARGO_BIN_EXE_<name>`
and panics if unset. Cargo only sets `CARGO_BIN_EXE_test_emitter` when
`test_emitter` is declared as an **artifact dependency**
(`test_emitter = { path = "...", artifact = "bin" }`), not as the plain path
`[dev-dependencies]` it currently uses
(`crates/lacon-cli/Cargo.toml:27`). On rustc 1.95.0 + assert_cmd 2.2.1 the env
var is never populated, so the lookup fails. Unit tests and the
adapter/hook/chain/tui integration tests all pass; only the `lacon-cli` e2e
tests that resolve the `test_emitter` helper binary are affected.

**Why not fixed here:** Out of scope for Plan 06-02. This plan's `files_modified`
are the bench, the cold-start script, `docs/architecture.md`, and the CI
workflow — none touch `lacon-cli` test wiring, `Cargo.toml` dev-deps, or the
`assert_cmd` pin. Fixing it requires either (a) converting `test_emitter` to a
Cargo artifact-dependency (`artifact = "bin"`, possibly needing `-Z bindeps` /
a newer stable), (b) bumping `assert_cmd` to a version whose `cargo_bin`
falls back to a target-dir search, or (c) replacing `cargo_bin("test_emitter")`
with `env!("CARGO_BIN_EXE_test_emitter")` won't help (same env var) — likely a
manifest/dependency change. That is a test-infrastructure change for a
follow-up plan, not a side quest here.

**Impact on this plan:** The CI workflow (`.github/workflows/ci.yml`) is correct
per the Plan 06-02 spec and runs `cargo test --workspace` as the hermetic test
step. Until this pre-existing breakage is fixed, that step will go RED on the
first CI run. SC4's "CI runs the hermetic test suite green" clause is therefore
**blocked by this pre-existing bug**, not by anything Plan 06-02 changed. The
deterministic hard gate (`cargo bench -p lacon-core --bench tracker_open`) and
the cold-start probe step are unaffected.

**Recommended follow-up:** A small test-infra plan to make `test_emitter`
resolvable under the workspace's pinned toolchain/assert_cmd (artifact-dep or
assert_cmd bump), then confirm `cargo test --workspace` is green on both lanes.
