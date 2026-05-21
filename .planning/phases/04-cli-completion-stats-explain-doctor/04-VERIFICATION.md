---
phase: 04-cli-completion-stats-explain-doctor
verified: 2026-05-22T00:45:00Z
status: passed
score: 4/4
overrides_applied: 0
re_verification: null
gaps: []
deferred: []
human_verification: []
---

# Phase 4: CLI Completion (stats / explain / doctor) Verification Report

**Phase Goal:** The remaining CLI commands ship — `lacon stats` summarizes tracking data with filters, `lacon explain <id>` re-runs filtering against stored raw output and shows side-by-side diffs, `lacon doctor` verifies the install/config/rule health of the system — and the binary's command surface is hard-capped at six.
**Verified:** 2026-05-22T00:45:00Z
**Status:** passed
**Re-verification:** No — initial verification

---

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | `lacon stats` prints top offenders, bypass rates, and unmatched commands derived from the four views, and accepts `--project`, `--since`, and `--rule` filters that narrow the output correctly | VERIFIED | `stats.rs` calls `query::unmatched_offenders`, `query::filtered_offenders`, `query::bypass_rate`, `query::project_savings` (unfiltered) or their `filtered_*` counterparts when any filter flag is set. `parse_since` converts `Nd/Nh/Nm` to ms. CLI test `stats_seeded_db_shows_four_sections_and_offender_rows` and filter tests pass. Live run shows all four sections with real data. |
| 2 | `lacon explain <id>` re-runs the rule pipeline against the stored raw output for invocation `<id>` and renders a side-by-side diff between raw and filtered, exiting with a clear error message when raw retention was disabled | VERIFIED | `explain.rs` follows the 6-step D-05 flow: safe `i64` parse (non-numeric -> exit 2), DB check, `open_readonly`, `fetch_invocation`, `raw_output_id` NULL branch prints `store_raw_outputs` message (SC2), BLOB merge, `Runner::filter_bytes` replay, hand-rolled two-column render. Tests `explain_non_numeric_id_errors_no_panic` and `explain_raw_output_id_null_errors_with_store_raw_outputs_hint` pass. |
| 3 | `lacon doctor` reports a green status when hooks are installed, config.yaml files at every layer parse, every rule loads and validates, and the database directory permissions are 0700. It surfaces a per-issue actionable error otherwise. | VERIFIED | `doctor.rs` implements the fixed five-check checklist: `check_hook`, `check_configs`, `check_rules`, `check_db_perms`, `check_tracker_health`. Uses `validate_file`, `RuleLoader::load_all`, `health::health_check`, `open_readonly`. Uses `report(Status::Pass/Fail/Warn, …)` fold. Fresh machine (absent settings.json, absent DB) is `Warn` not `Fail`. Black-box suite `cli_doctor.rs` (5 tests) covers all-green, invalid-config failure, invalid-rule failure, fresh-machine informational, and settings-present-without-hook hard-fail. |
| 4 | Running `lacon <unknown-subcommand>` returns a non-zero exit code with a clap error pointing at the six legitimate subcommands; the binary has no `purge`, `install`, or `stats --serve` paths | VERIFIED | `cli.rs` defines exactly one `CliCommand` enum with six variants (Run/Validate/Init/Stats/Explain/Doctor). Live checks confirm: `lacon flibbertigibbet` exits 2 with "unrecognized subcommand", `lacon purge` exits 2, `lacon install` exits 2, `lacon stats --serve` exits 2. `cli_surface.rs` has 6 tests all passing: exactly-six assertion, unknown-subcommand rejection, purge/install/stats-serve forbidden assertions. |

**Score:** 4/4 truths verified

---

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `crates/lacon-core/src/tracking/query.rs` | Read API: view readers + filtered re-queries + explain lookups | VERIFIED | Exists; 10 `pub fn` functions; all SQL parameterized via `params![]`; no user-value string interpolation into SQL |
| `crates/lacon-core/src/tracking/mod.rs` | `open_readonly` helper, never migrates/prunes/INSERTs | VERIFIED | `open_readonly` at line 156; uses `SQLITE_OPEN_READ_ONLY \| SQLITE_OPEN_NO_MUTEX`; omits `journal_mode=WAL` write; `pub mod query;` declared at line 24 |
| `crates/lacon-core/tests/tracking_query.rs` | 13 integration tests for read API + no-write invariant | VERIFIED | Exists; 13 tests all pass covering unfiltered view reads, `--since` cutoff, `--project` filter, `fetch_invocation` hit/miss, BLOB round-trip |
| `crates/lacon-core/tests/wave0_smoke.rs` | Wave-0 spike proving strict `SQLITE_OPEN_READ_ONLY` works on WAL DB | VERIFIED | `smoke_readonly_open_of_wal_db` passes; empirically confirmed strict READ_ONLY works on this build |
| `crates/lacon-core/src/runtime/mod.rs` | `Runner::filter_bytes` subprocess-free byte-replay | VERIFIED | At line 423; `&mut self` signature; never calls `Runner::run`; mirrors exit-code branch at :342-359 exactly |
| `crates/lacon-core/tests/runtime_filter_bytes.rs` | 4 branch-fidelity tests | VERIFIED | All 4 pass: success path, on_error path, no-on_error passthrough (ADR-0010), fidelity assertion |
| `crates/lacon-cli/src/commands/stats.rs` | stats command: views + filters + empty-DB handling | VERIFIED | `pub fn execute(project, since, rule)` at line 27; uses `tracking::query`/`open_readonly`; no rusqlite import; `parse_since` handles `Nd/Nh/Nm` |
| `crates/lacon-cli/src/commands/explain.rs` | explain command: id parse + row lookup + raw-disabled error + byte-replay + diff render | VERIFIED | `pub fn execute(id)` at line 27; safe `i64` parse; 6-step D-05 flow; `filter_bytes` called; `sanitize_for_display` on right column (WR-01); `exit_code_from_stored` guard (WR-04) |
| `crates/lacon-cli/src/commands/doctor.rs` | doctor: hook/config/rules/perms/health five-check sweep | VERIFIED | `pub fn execute()` at line 78; `HOOK_FINGERPRINT = "lacon-claude-hook"`; uses `validate_file`, `load_all`, `health_check`, `open_readonly`; zero `Tracker::open` references |
| `crates/lacon-cli/src/main.rs` | Args threaded through to execute() (D-12) | VERIFIED | `Stats { project, since, rule } => commands::stats::execute(project, since, rule)?`; `Explain { id } => commands::explain::execute(id)?` |
| `crates/lacon-cli/tests/cli_stats.rs` | Black-box stats coverage | VERIFIED | 5 tests: seeded DB shows four sections, `--since` narrows, `--project` narrows, invalid `--since` errors non-zero, empty-DB shows "no data yet" and exits 0 |
| `crates/lacon-cli/tests/cli_explain.rs` | Black-box explain coverage | VERIFIED | 5 tests: side-by-side rendered, raw_output_id NULL errors with `store_raw_outputs` hint, non-numeric id errors non-zero, unknown id "not found", replay with non-zero exit |
| `crates/lacon-cli/tests/cli_doctor.rs` | Black-box doctor coverage | VERIFIED | 5 tests: all-green, invalid-config failure, invalid-rule failure, fresh-machine informational, settings-present-without-hook hard-fail |
| `crates/lacon-cli/tests/cli_surface.rs` | Six-command cap assertion + forbidden subcommand absence | VERIFIED | 6 tests all pass: exactly-six check, unknown rejection, version flag, purge forbidden, install forbidden, stats-serve forbidden |

---

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| `tracking/mod.rs open_readonly` | `rusqlite Connection` | `SQLITE_OPEN_READ_ONLY \| SQLITE_OPEN_NO_MUTEX` (no CREATE, no migrate, no prune) | VERIFIED | Line 158-161; WAL pragma write deliberately omitted (line 169 comment) |
| `tracking/query.rs` | invocations / raw_outputs / four views | Parameterized `SELECT` over `&Connection` using `params![]` | VERIFIED | All 10 functions verified; `v_unmatched_offenders`, `v_filtered_offenders`, `v_bypass_rate`, `v_project_savings` all queried |
| `stats.rs` | `lacon_core::tracking::query` | `open_readonly` + view readers + filtered re-queries | VERIFIED | Line 23: `use lacon_core::tracking::{self, query}`; calls `query::*` functions; no `rusqlite` import |
| `explain.rs` | `Runner::filter_bytes` | resolve rule + fetch BLOBs + replay | VERIFIED | Lines 141-153; `runner.filter_bytes(&merged, exit_code, duration_ms, …)` called with stored values |
| `main.rs` | `stats::execute / explain::execute` | Threaded clap args (project/since/rule, id) | VERIFIED | Lines 15-19; `Stats { project, since, rule } =>` and `Explain { id } =>` destructured and passed |
| `doctor.rs` | `validate_file / load_all / health_check / open_readonly` | Reused core surfaces; read-only DB open (D-08) | VERIFIED | All 4 surfaces imported and called; `Tracker::open` count = 0 (grep verified) |
| `doctor.rs` | `.claude/settings.json hooks.PreToolUse[]` | JSON walk for `lacon-claude-hook` fingerprint | VERIFIED | Lines 141-148; mirrors `init.rs` walk pattern; `HOOK_FINGERPRINT = "lacon-claude-hook"` |

---

### Data-Flow Trace (Level 4)

| Artifact | Data Variable | Source | Produces Real Data | Status |
|----------|---------------|--------|-------------------|--------|
| `stats.rs` | `unmatched`, `f_offenders`, `bypass`, `savings` | `query::unmatched_offenders` / `filtered_*` reading live DB views via `open_readonly` | Yes — SQL queries against live `history.db` rows | FLOWING |
| `explain.rs` | `row` (InvocationRow), `filtered` (Vec<String>) | `fetch_invocation` -> DB row; `fetch_raw_output` -> stored BLOBs; `Runner::filter_bytes` -> pipeline replay | Yes — real DB lookup + BLOB bytes + pipeline execution | FLOWING |
| `doctor.rs` | `all_ok` bool folded from 5 checks | `validate_file` / `load_all` / `health_check` / filesystem stats | Yes — real filesystem reads and DB probe | FLOWING |

---

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Unknown subcommand exits non-zero with clap error | `lacon flibbertigibbet` | exit 2, "unrecognized subcommand 'flibbertigibbet'" | PASS |
| `lacon purge` exits non-zero | `lacon purge` | exit 2, "unrecognized subcommand 'purge'" | PASS |
| `lacon install` exits non-zero | `lacon install` | exit 2, "unrecognized subcommand 'install'" | PASS |
| `lacon stats --serve` exits non-zero | `lacon stats --serve` | exit 2, "unexpected argument '--serve' found" | PASS |
| `lacon explain abc` exits non-zero cleanly, no panic | `lacon explain abc` | exit 2, "invalid invocation id `abc` (expected a number)" | PASS |
| `lacon stats` shows four sections with real data (or "no data yet" on fresh machine) | `lacon stats` | exit 0, four sections rendered with real tracking data from test runs | PASS |
| Wave-0: strict READ_ONLY open of WAL DB works | `cargo test wave0_smoke smoke_readonly_open_of_wal_db` | 1 passed | PASS |
| filter_bytes three branch cases | `cargo test -p lacon-core --test runtime_filter_bytes` | 4 passed | PASS |
| tracking_query integration tests | `cargo test -p lacon-core --test tracking_query` | 13 passed | PASS |
| Full workspace test suite | `cargo test --workspace` | 444 passed, 0 failed | PASS |

---

### Probe Execution

No `probe-*.sh` scripts declared or expected for this phase. Phase 4 is a Rust implementation phase; behavioral verification performed via `cargo test --workspace` (444 tests, exit 0).

---

### Requirements Coverage

| Requirement | Source Plans | Description | Status | Evidence |
|-------------|-------------|-------------|--------|---------|
| REQ-cli-stats | 04-01, 04-03 | `lacon stats` shows top offenders, bypass rates, unmatched commands; `--project`/`--since`/`--rule` filters | SATISFIED | `stats.rs` implement all four sections; filter logic in `query.rs` D-09 re-queries; 5 black-box tests pass |
| REQ-cli-explain | 04-01, 04-02, 04-03 | `lacon explain <id>` re-runs filtering against stored raw output, shows side-by-side diff | SATISFIED | `explain.rs` D-05 6-step flow; `Runner::filter_bytes` byte-replay; SC2 raw-disabled error path; 5 black-box tests pass |
| REQ-cli-doctor | 04-04 | `lacon doctor` verifies hooks installed, configs valid, rules parse, DB perms 0700 | SATISFIED | `doctor.rs` five-check sweep; `check_hook`/`check_configs`/`check_rules`/`check_db_perms`/`check_tracker_health`; 5 black-box tests pass |
| REQ-cli-surface-cap | 04-04 | v1 ships exactly six CLI commands; no purge/install/stats-serve | SATISFIED | `cli.rs` exactly one `CliCommand` enum with 6 variants; `cli_surface.rs` 6 tests all pass including forbidden-subcommand assertions |

No orphaned requirements for Phase 4 in REQUIREMENTS.md.

---

### Anti-Patterns Found

No debt-marker comments (TBD/FIXME/XXX) found in any Phase 4 modified files. No `return null` / empty stub patterns. No `rusqlite` runtime dependency in `lacon-cli` (stays dev-only under `[dev-dependencies]`). No `Tracker::open` in `doctor.rs`.

**Pre-existing clippy lints (out of scope — documented in `deferred-items.md`):**
4 clippy warnings in Phase 1/2 `lacon-core` files (`pipeline/stages.rs:438/451`, `tracking/record.rs:8`, `tracking/mod.rs:201`) predate Phase 4 and are not in any file this phase modified. Confirmed via git blame in summaries. Tracked for Phase 6 hardening. These are NOT Phase 4 gaps.

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| `pipeline/stages.rs` | 438, 451 | `collapsible_if` clippy lint (pre-existing) | INFO | Pre-dates Phase 4; tracked in deferred-items.md |
| `tracking/record.rs` | 8 | `doc_overindented_list_items` clippy lint (pre-existing) | INFO | Pre-dates Phase 4; tracked in deferred-items.md |
| `tracking/mod.rs` | 201 | `manual_ignore_case_cmp` clippy lint (pre-existing) | INFO | Pre-dates Phase 4; tracked in deferred-items.md |

---

### Code Review Fixes — Confirmed Present

All 5 review fixes mentioned in the phase notes are present and verified:

| Fix | Commit | Status |
|-----|--------|--------|
| WR-01: `sanitize_for_display` on explain filtered column | `ef3fbff` | Present — `sanitize_for_display` called at `explain.rs:223` |
| WR-02: map read-path SQL errors to per-command error channel | `6785bb4` | Present — explicit `match` on `query::*` results with `eprintln!` + `return Ok(1)` |
| WR-03: normalize `stats --project` and hint on zero-row filter | `54a0806` | Present — `normalize_project` function + no-match hint at `stats.rs:215-229` |
| WR-04: guard `i64->i32` exit-code cast in explain replay | `8f1ce8f` | Present — `exit_code_from_stored` at `explain.rs:174-181`; `i32::try_from` not `as i32` |
| WR-05: replace debug_assert in health probe with returned error | `d144832` | Present — `apply_connection_pragmas` returns `TrackingError::WalRejected` instead of panicking |

---

### Human Verification Required

None. All success criteria are verifiable through code inspection, test execution, and behavioral spot-checks.

---

### Gaps Summary

No gaps. All four success criteria are VERIFIED against the actual codebase:

1. `lacon stats` — four-section output, D-09 filtered re-queries, `--project/--since/--rule` flags wired, fresh-machine graceful path, 5 black-box tests passing.
2. `lacon explain <id>` — full 6-step D-05 flow, SC2 raw-disabled error path with `store_raw_outputs` message, safe non-numeric id handling, `Runner::filter_bytes` replay, hand-rolled two-column render, 5 black-box tests passing.
3. `lacon doctor` — fixed five-check sweep, per-issue actionable error lines, fresh-machine as informational, D-08 read-only DB (zero `Tracker::open` references), 5 black-box tests passing.
4. CLI surface cap — exactly 6 subcommands in `CliCommand` enum, `purge`/`install`/`stats --serve` all rejected non-zero, 6 `cli_surface.rs` tests passing.

Full workspace: 444 tests, 0 failures.

---

_Verified: 2026-05-22T00:45:00Z_
_Verifier: Claude (gsd-verifier)_
