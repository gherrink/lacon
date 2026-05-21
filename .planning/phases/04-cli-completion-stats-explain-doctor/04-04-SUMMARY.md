---
phase: 04-cli-completion-stats-explain-doctor
plan: 04
subsystem: cli
tags: [doctor, health-check, sqlite, read-only, cli-surface-cap, clap, assert_cmd]

# Dependency graph
requires:
  - phase: 04-01
    provides: "tracking::open_readonly(&Path) read-only DB open + tracking::health::health_check probe"
  - phase: 03
    provides: "lacon init writes the lacon-claude-hook PreToolUse(Bash) fingerprint that doctor reads (A4 contract)"
  - phase: 01
    provides: "lacon_core::validate::validate_file + RuleLoader::load_all (config/rule sweep surfaces)"
provides:
  - "lacon doctor — fixed five-check health sweep (hook install, config-per-layer, rule sweep, DB dir perms 0700, tracker health) with per-issue actionable errors and exit-0-iff-all-pass (REQ-cli-doctor)"
  - "Locked six-command surface cap with proven absence of forbidden v2 subcommands purge/install/stats --serve (REQ-cli-surface-cap)"
affects: [phase-05-bundled-rules, phase-06-hardening]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "doctor as pure composition: every check reuses an existing verified core surface (validate_file / load_all / health_check / open_readonly); zero new construction"
    - "Three-state checklist (Pass / Fail / Warn): only Fail flips the overall result red; Warn is fresh-machine informational (D-03) and stays green"
    - "DB-touching checks gate on db_path.exists() BEFORE open_readonly (which never CREATEs), so a fresh run never creates history.db — D-04 cold-start invariant preserved"

key-files:
  created:
    - crates/lacon-cli/tests/cli_doctor.rs
  modified:
    - crates/lacon-cli/src/commands/doctor.rs
    - crates/lacon-cli/tests/cli_surface.rs
    - crates/lacon-cli/tests/tracking_coldstart.rs

key-decisions:
  - "Fresh-but-no-hook exit semantics: a brand-new project (no .claude/settings.json, no history.db) is NOT an error — hook/db-perms/tracker render as [warn] and doctor exits 0. A POSITIVELY broken state flips it red: settings.json present-but-missing-hook, an invalid config/rule, or a non-0700 DB dir."
  - "Doctor's tracker check uses open_readonly ONLY (D-08); doctor.rs contains zero `Tracker::open` references (grep gate = 0, T-04-11 mitigated). Doc comments were reworded to avoid the literal token so the source-grep gate stays at 0."
  - "stats --serve forbidden via clap unknown-arg rejection (stats is a real subcommand; --serve is not one of its args) rather than a separate subcommand — no web-UI/daemon path exists (CON-nfr-no-network, D-13)."

patterns-established:
  - "Pattern: checklist report() helper returns bool (false only on Fail) and the caller folds with `all_ok &= ...` — single source of truth for the exit code"
  - "Pattern: black-box doctor fixtures seed a real WAL history.db via `lacon run` (test_emitter + project rule) so the perms + health checks validate a real 0700 dir, not a hand-rolled DB"

requirements-completed: [REQ-cli-doctor, REQ-cli-surface-cap]

# Metrics
duration: 4min
completed: 2026-05-22
---

# Phase 4 Plan 04: doctor Health Sweep & Six-Command Surface Cap Summary

**`lacon doctor` runs a fixed five-check sweep (hook install, config-per-layer, rule sweep, DB dir perms 0700, read-only tracker health) printing one pass/fail/warn line each and exiting 0 iff all pass — fresh machines read informational, not red — plus a hardened six-command surface cap that proves `purge`/`install`/`stats --serve` are absent.**

## Performance

- **Duration:** ~4 min
- **Started:** 2026-05-21T22:20:29Z
- **Completed:** 2026-05-21T22:24:41Z
- **Tasks:** 3
- **Files modified:** 4 (1 created, 3 modified)

## Accomplishments

- **doctor five-check sweep (D-07):** Replaced the Phase 1 stub with the fixed checklist — hook install, config-per-layer validity, rule sweep, DB dir perms (0700), and tracker health — each printing one `[ ok ]` / `[fail]` / `[warn]` line. Exits 0 only when no check hard-fails; exits 1 with per-issue actionable error lines naming the offending path otherwise.
- **Read-only DB posture (D-08, T-04-11):** The perms + health checks open the DB read-only via `open_readonly` and never migrate/prune/INSERT. `doctor.rs` contains **zero** `Tracker::open` references (grep gate verified = 0).
- **Fresh-machine handling (D-03):** No `.claude/settings.json` and no `history.db` (brand-new clone) → those checks read as informational `[warn]` lines and doctor exits 0, not red. A positively broken state (settings present but hook missing, invalid config/rule, wrong perms) is what flips it red.
- **Six-command cap hardened (D-13, REQ-cli-surface-cap):** Extended `cli_surface.rs` without weakening the existing assertions — `lacon purge`, `lacon install`, and `lacon stats --serve` each now proven to exit non-zero. The pre-existing exactly-six and unknown-subcommand assertions remain intact.
- **Black-box coverage:** 5 `cli_doctor.rs` cases (all-green with a real seeded WAL DB, invalid-config failure, invalid-rule failure, fresh-machine informational, settings-present-without-hook hard-fail) lock the contract.

## Task Commits

Each task was committed atomically:

1. **Task 1: doctor five-check health sweep (TDD)** - `c8c56a8` (feat) — RED proved by the Phase 1 stub's exit-2; GREEN proved by `cli_doctor` + smoke run
2. **Task 2: cli_doctor.rs black-box tests + cold-start stale-assert fix** - `690409b` (test)
3. **Task 3: harden cli_surface.rs (six-command cap + forbidden v2 subcommands)** - `00673fe` (test)

_Note: Task 1 is a `tdd="true"` task. Its RED phase was the pre-existing stub returning exit 2 (verified before the rewrite); the GREEN proof — the `cli_doctor.rs` integration suite — ships in Task 2's commit because doctor's behavior is inherently filesystem/CLI-driven (no pure-unit RED). The implementation and its black-box proof are therefore in adjacent commits, both green._

## Files Created/Modified

- `crates/lacon-cli/src/commands/doctor.rs` (modified) — the five-check sweep: `execute()` folds `check_hook` / `check_configs` / `check_rules` / `check_db_perms` / `check_tracker_health` via `all_ok &= ...`; a `report(Status, label, detail)` helper renders Pass/Fail/Warn and returns the fold bit
- `crates/lacon-cli/tests/cli_doctor.rs` (created) — 5 black-box assert_cmd cases with tempdir cwd + XDG isolation
- `crates/lacon-cli/tests/cli_surface.rs` (modified) — added 3 forbidden-subcommand assertions; existing six-command + unknown-subcommand cap untouched
- `crates/lacon-cli/tests/tracking_coldstart.rs` (modified) — updated the stale `doctor_does_not_open_db` assertion (stub exit-2 → implemented exit-0) while preserving + re-asserting the load-bearing "never creates history.db" invariant

## Decisions Made

- **Fresh-but-no-hook = exit 0 (informational), not red.** A new project that has not run `lacon init` is the normal first-run state, so the missing hook + absent DB are `[warn]`, and doctor exits 0. Only a positively broken state hard-fails. Documented in the `cli_doctor::doctor_fresh_machine_is_informational_not_red` test docstring and the `..._settings_present_without_hook_is_a_hard_fail` companion that proves the red path.
- **`stats --serve` rejected as an unknown clap arg, not a missing subcommand.** `stats` is a real subcommand whose arg set is `--project/--since/--rule`; `--serve` therefore fails clap parsing non-zero. This is the right shape because no web-UI/daemon surface exists at all (CON-nfr-no-network, D-13).
- **Doc comments reworded to keep the `Tracker::open` grep gate at 0.** The plan's threat-mitigation gate (`grep -c 'Tracker::open\b'` must be 0, T-04-11) is a literal source grep. Negative-mention doc comments ("never call `Tracker::open`") tripped it, so they were reworded to "never the write/migrate path" — same intent, gate green.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Stale `doctor_does_not_open_db` cold-start assertion**
- **Found during:** Task 2 (running the full `lacon-cli` suite after adding `cli_doctor.rs`)
- **Issue:** `crates/lacon-cli/tests/tracking_coldstart.rs::doctor_does_not_open_db` was written against the Phase 1 stub and asserted `cmd.assert().code(2)` (the "not yet implemented" exit). Now that doctor is implemented, it exits 0 on a fresh machine, so the test failed.
- **Fix:** Updated the assertion to `cmd.assert().success()` and added a comment explaining the Phase 4 contract change. Crucially **preserved** the load-bearing invariant: the test still asserts `!history.db.exists()` after a fresh-machine doctor run — doctor opens read-only and gates on `db_path.exists()` first, so it never creates the DB (D-04/D-08).
- **Files modified:** `crates/lacon-cli/tests/tracking_coldstart.rs`
- **Verification:** `cargo test -p lacon-cli --test tracking_coldstart` → 5/5 pass; the companion source-grep `doctor_rs_does_not_reference_tracker` (forbids `Tracker::open`) also passes.
- **Committed in:** `690409b` (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug — stale test vs. implemented behavior).
**Impact on plan:** Necessary to keep the suite green after doctor moved from stub to implemented; the cold-start invariant it guards is unchanged and re-asserted. No scope creep — only the obsolete stub-era exit code was corrected.

## Issues Encountered

- **`lacon-cli` is a binary-only crate** (no lib target), so `cargo clippy -p lacon-cli --lib` errors. Used `--bins --tests` for the clippy gate instead. All four plan-owned files are clippy-clean (0 hits).
- **4 pre-existing `lacon-core` clippy lints** (`stages.rs:438/451`, `record.rs:8`, `mod.rs:201`) keep the workspace `-D warnings` gate red. These predate this plan, are in files it does not own, and are documented + re-confirmed in `deferred-items.md` (Phase 6 hardening item). Not fixed here per the scope boundary — touching other phases' code from this plan would be scope creep.

## Threat Surface

No new security surface beyond the plan's `<threat_model>`. The three boundaries it identified (settings.json JSON parse, config/rule YAML validators, read-only history.db probe) are all mitigated as planned: every parse goes through `serde_json` / `validate_file` / `load_all` returning `Result` mapped to a printed line + non-zero exit (no raw `?` panic, T-04-10); the health probe uses `open_readonly` only (T-04-11, grep gate = 0); doctor prints only offending paths + structured error messages, never raw file contents (T-04-13).

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- Phase 4 (CLI completion) is now functionally complete across all four plans: 04-01 (read layer), 04-02 (`filter_bytes`), 04-03 (`stats` + `explain`), 04-04 (`doctor` + surface cap). All six subcommands (`run`, `validate`, `init`, `stats`, `explain`, `doctor`) are implemented and the surface cap is locked.
- Phase 5 (bundled rules) and Phase 6 (hardening) can rely on `lacon doctor` as the install/config/rule/perms/health self-check.
- **Concern (carried forward):** the 4 pre-existing `lacon-core` clippy lints keep the workspace `-D warnings` gate red until a later plan/phase clears them (tracked in `deferred-items.md`). Recommended owner: Phase 6.

## Self-Check: PASSED

- Files verified present: `doctor.rs`, `cli_doctor.rs`, `cli_surface.rs`, `tracking_coldstart.rs`, `04-04-SUMMARY.md`
- Commits verified present: `c8c56a8`, `690409b`, `00673fe`

---
*Phase: 04-cli-completion-stats-explain-doctor*
*Completed: 2026-05-22*
