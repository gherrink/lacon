---
phase: quick-260522-v4a
plan: 01
subsystem: cli
tags: [doctor, cli, scope-aware, opt-in-posture, hermetic-tests, etcetera, claude-code-hook]

# Dependency graph
requires:
  - phase: quick-260522-tor
    provides: "two-scope `lacon init` (project = cwd-relative; user = ~/.claude + ~/.config/lacon/rules) + the empirically verified @import tokens (@.claude/LACON.md, @LACON.md)"
provides:
  - "scope-aware `lacon doctor`: verifies hook + LACON.md + scope-correct @import reference for BOTH project and user scope, grouped by scope"
  - "opt-in posture: configured+complete=ok; configured+broken=fail/exit1; not-configured-while-other-configured=neutral; neither-configured=warn/exit0; present-file IO/parse error=fail"
  - "HOME-isolated doctor contract tests (run_doctor overrides HOME + XDG so user-scope checks never touch the real ~/.claude)"
affects: [cli-doctor, init, future-adapters]

# Tech tracking
tech-stack:
  added: []  # no new dependencies — reuses etcetera + serde_json already in lacon-cli
  patterns:
    - "Single shared HOOK_FINGERPRINT PreToolUse(Bash) walk (settings_has_hook) reused by both scopes and the cheap configured-probe"
    - "Cross-scope posture computed BEFORE rendering: both scopes' configured-state known up front so a not-configured scope renders neutral vs warn correctly"
    - "Status::Info neutral glyph ([ -- ]) that never flips all_ok — distinct from Warn"
    - "Hermetic CLI tests: every command that can read/write ~/.claude overrides HOME (+ XDG); project-only init writes solely to the tempdir cwd"

key-files:
  created: []
  modified:
    - crates/lacon-cli/src/commands/doctor.rs
    - crates/lacon-cli/tests/cli_doctor.rs
    - crates/lacon-cli/src/cli.rs

key-decisions:
  - "Old doctor_settings_present_without_hook_is_a_hard_fail is REVISED: a bare Claude settings.json without the lacon hook is no longer 'positively broken' — it just means that scope is not configured (warn-when-neither / neutral-when-other-configured)"
  - "user_claude_dir() (etcetera::home_dir().join('.claude')) is a NEW helper, deliberately distinct from user_config_dir() (XDG ~/.config/lacon) — conflating the two was the bug being avoided"
  - "Reference check is a whole-line substring scan (trim_end()) mirroring init's install_reference_line — doctor never shells out to claude"

patterns-established:
  - "Per-scope check via one parameterized code path (check_scope over ScopeTargets) — project and user share one walk, zero duplication"
  - "Configured predicate = settings.json present AND parses AND carries the fingerprint; absent/unparseable => not-configured (caller decides posture); present-but-unreadable on a configured scope => [fail]"

requirements-completed: [REQ-cli-doctor]

# Metrics
duration: 8min
completed: 2026-05-22
---

# Quick Task 260522-v4a: scope-aware `lacon doctor` Summary

**`lacon doctor` now verifies the FULL setup (hook + LACON.md + scope-correct `@import` reference) at BOTH project and user scope, grouped by scope, with the locked opt-in posture and fully HOME-isolated tests.**

## Performance

- **Duration:** ~8 min (verification + summary; the two task commits were already present on the worktree branch base)
- **Started:** 2026-05-22T22:30:00Z (approx)
- **Completed:** 2026-05-22
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments

- `doctor.rs` reworked from the old project-cwd-only `check_hook` into a scope-aware, three-check-per-scope sweep (`check_scope`) driven through ONE shared `HOOK_FINGERPRINT` PreToolUse(Bash) walk (`settings_has_hook`) for both project and user.
- Added a `user_claude_dir()` helper (`etcetera::home_dir()?.join(".claude")`, reads `$HOME`), deliberately distinct from the pre-existing `user_config_dir()` (XDG `~/.config/lacon`).
- Added a neutral `Status::Info` (`[ -- ]`) variant that renders informationally and never flips `all_ok` — used for a not-configured scope when the OTHER scope IS configured.
- `execute()` computes `project_configured` / `user_configured` up front (cheap hook probe) so the cross-scope posture is known before rendering, then emits "Project setup:" and "User setup:" groups followed by the unchanged global checks (config / rules / db-perms / tracker).
- `cli_doctor.rs` made hermetic: `run_doctor` now takes an explicit `home: &Path` and sets `.env("HOME", ...)`; new posture/scope test cases (user-scope-complete, configured-broken-instructions, configured-broken-reference, one-scope-only neutrality both directions, neither-configured warn). 10/10 doctor tests pass.
- `cli.rs` Doctor help reworded to mention both scopes (hook + LACON.md + `@import` reference per scope) plus configs/rules/DB health.

## Task Commits

Each task was committed atomically (code changes only; both commits were already on this worktree's branch base):

1. **Task 1: Rework doctor.rs into scope-aware three-check-per-scope with the locked opt-in posture** — `1baa30c` (feat)
2. **Task 2: Rework cli_doctor.rs for HOME isolation + new posture + scope coverage; update cli.rs Doctor help** — `c233ed7` (test)

_The orchestrator commits the docs artifacts (this SUMMARY, STATE.md) separately._

## Files Created/Modified

- `crates/lacon-cli/src/commands/doctor.rs` — scope-aware three-check-per-scope sweep; `ScopeTargets`, `check_scope`, `check_reference`, `scope_hook_present`, shared `settings_has_hook` walk, `user_claude_dir()` helper, `Status::Info` neutral variant; module + check rustdoc describing both scopes and the opt-in posture.
- `crates/lacon-cli/tests/cli_doctor.rs` — `run_doctor(proj, home, xdg)` with `.env("HOME", ...)`; `init_user_scope` / `seed_history_db` helpers (HOME-isolated); `PROJECT_IMPORT` / `USER_IMPORT` consts; new posture + scope test cases.
- `crates/lacon-cli/src/cli.rs` — Doctor variant help text reworded for both scopes.

## INTENTIONAL Behavior Change (required call-out)

**The old `doctor_settings_present_without_hook_is_a_hard_fail` behavior is REVISED.**

- **Old model (single-scope, cwd-only):** a `<cwd>/.claude/settings.json` that parses but lacks the lacon hook was treated as **positively broken** → `[fail]` → exit 1. The assumption was "there is exactly one place lacon can live, and the absence of the hook there is a problem."
- **New model (two opt-in scopes):** a scope is **configured** iff its `settings.json` carries the lacon hook. A bare Claude `settings.json` without a lacon hook simply means *that scope is not configured* — it is no longer "positively broken." Posture now:
  - if the **other** scope IS configured → the un-configured scope renders **neutrally** (`[ -- ]`, informational, does NOT affect exit). Installing only one scope is a legitimate complete setup — the user's explicit requirement that the other scope must NOT be flagged as a warning.
  - if **neither** scope is configured (fresh machine) → a single `[warn] hook` posture line with the `run \`lacon init\`` hint, **exit 0** — NOT a hard fail.
- **Rationale:** with two opt-in install scopes, "settings.json present without a lacon hook" is ambiguous (the user may have configured the *other* scope, or none yet) and is not actionable-broken on its own. The only states that flip exit to 1 are *positively broken* ones: a **configured** scope missing a completeness sub-check (LACON.md or the `@import` reference), an IO/parse error on a **present** file, or an invalid config/rule/wrong DB perms.
- The corresponding test was renamed to `doctor_neither_scope_configured_warns_not_fails` and now asserts `.success()` + `[warn] hook` + `run \`lacon init\`` + the absence of `[fail] hook`.

## Final grouped doctor output shape

Project-only configured (user shown neutrally), the representative shape:

```
Project setup:
[ ok ] hook: project: lacon-claude-hook installed
[ ok ] instructions: project: <cwd>/.claude/LACON.md present
[ ok ] reference: project: <cwd>/CLAUDE.md imports @.claude/LACON.md

User setup:
[ -- ] hook: user setup: not configured (optional)

[ ok ] config: no config.yaml present (defaults in effect)
[ ok ] rules: N rule(s) parse cleanly
[warn] db-perms: DB not yet initialized (run a command first)
[warn] tracker: history.db not yet initialized (run a command first)

doctor: all checks passed
```

Posture matrix (locked CONTEXT decisions):

| Scope state | Rendering | Exit effect |
|---|---|---|
| configured + complete | `[ ok ]` hook / instructions / reference | none (green) |
| configured + broken (LACON.md or @import missing) | `[fail]` on the missing sub-check | flips to exit 1 |
| not-configured WHILE other configured | `[ -- ] hook: <scope> setup: not configured (optional)` (neutral) | none |
| neither configured (fresh machine) | `[warn] hook: not installed (run \`lacon init\`)` per scope group | none (exit 0) |
| present-file IO/parse error (settings.json or CLAUDE.md) | `[fail]` | flips to exit 1 |

Greppable labels (`hook`, `rules`, `db-perms`, `tracker`) are preserved; per-scope lines all contain `hook`.

## Decisions Made

- Kept the two task commits exactly as authored on the worktree branch — they implement the plan precisely (scope-aware `check_scope`, shared `settings_has_hook` walk, `user_claude_dir()` distinct from `user_config_dir()`, `Status::Info`, HOME-isolated tests, both-scope Doctor help). No re-implementation was warranted; verification confirmed correctness.

## Deviations from Plan

None — plan executed exactly as written. The two tasks were already implemented and committed atomically on the worktree branch (`1baa30c` Task 1 / `c233ed7` Task 2); execution consisted of full verification against the plan's done-criteria, success-criteria, and threat-mitigations, all of which passed.

## Issues Encountered

- **Out-of-scope clippy warnings (NOT fixed):** `cargo clippy -p lacon-cli --all-targets` surfaces 5 pre-existing warnings in files this task does not touch (`lacon-core/src/pipeline/stages.rs`, `lacon-core/src/tracking/{record,mod}.rs`, `lacon-cli/tests/tracking_e2e.rs`). Zero warnings are attributable to `doctor.rs` / `cli_doctor.rs` / `cli.rs`. Logged to `deferred-items.md`; left untouched per the scope boundary.
- **Pre-existing project-wide `cargo fmt` drift (NOT reformatted):** `cargo fmt -p lacon-cli --check` flags ~34 locations across nearly every lacon-cli file (run.rs, stats.rs, main.rs, explain.rs, the test files, and the `Explain` variant at `cli.rs:58`). Per the constraint (and prior quick task 260522-tor's precedent), this pre-existing drift was deliberately NOT reformatted — doing so would touch dozens of unrelated files. The three TOUCHED files contribute ZERO new drift: `doctor.rs` and `cli_doctor.rs` are fully fmt-clean, and the `cli.rs` Doctor doc-comment (this task's only `cli.rs` change) is fmt-clean (the `cli.rs:58` drift is the unrelated `Explain` variant).
- **Self-inflicted test-init artifacts (cleaned up):** while capturing the rendered doctor output, a `lacon init --project` invocation accidentally ran against the worktree root, appending `@.claude/LACON.md` to the tracked `CLAUDE.md` and creating untracked `.claude/LACON.md`, `.claude/settings.json`, `.lacon/`. All were reverted/removed by exact path (`git checkout -- CLAUDE.md`; `rm -f` the specific artifacts) — never `git clean`. Worktree is clean except this SUMMARY, `deferred-items.md`, and STATE.md.

## Threat-model verification

| Threat ID | Status | Evidence |
|---|---|---|
| T-v4a-01 (malformed/unparseable settings.json) | mitigated | present-but-unparseable on a configured scope → `[fail]` via `check_scope`'s early returns; absent/unparseable at probe → not-configured (no panic, no raw `?` to user) |
| T-v4a-02 (test reading the developer's real ~/.claude) | mitigated | `run_doctor`, `seed_history_db`, and `init_user_scope` all set `.env("HOME", ...)`; the only HOME-less invocations are `lacon init --project` which write solely to the tempdir cwd. Full suite green with no real-home reads. |
| T-v4a-03 (unreadable/missing CLAUDE.md at a configured scope) | mitigated | `check_reference`: NotFound while configured → `[fail]`; present-but-unreadable → `[fail]` |
| T-v4a-SC (npm/pip/cargo installs) | accepted | no new dependencies (reuses etcetera + serde_json); no install tasks |

## Verification

- `cargo build --workspace` — clean (up to date).
- `cargo test -p lacon-cli --test cli_doctor` — **10 passed, 0 failed**.
- `cargo test -p lacon-cli` (full non-ignored suite) — all green: cli_doctor 10, cli_init 8, pnpm_e2e 1 passed + 1 ignored (real install correctly excluded → hermeticity intact), plus all other suites.
- `cargo clippy -p lacon-cli --all-targets` — zero warnings on touched files (pre-existing warnings out of scope, logged to deferred-items.md).
- Grep confirms: `doctor.rs` has `fn user_claude_dir`, a single `settings_has_hook` walk (reused, not duplicated), greppable labels intact (8×`hook`, 2×`rules`, 7×`db-perms`, 5×`tracker`); `cli_doctor.rs` `run_doctor` contains `.env("HOME"`.

## Next Phase Readiness

- `lacon doctor` and `lacon init` are now consistent: doctor verifies exactly what init writes, at both scopes, with no leftover single-scope assumptions in `doctor.rs` / `cli.rs` / `cli_doctor.rs`.
- Audit note (no change required): `docs/v1-scope.md:46` / `docs/architecture.md:66` doctor wording is generic and not materially wrong — left as-is per the plan. `pnpm_e2e.rs` stays hermetic (bare/`--project` init writes only to cwd; `pnpm_e2e_hermetic` never calls init).

## Self-Check: PASSED

- FOUND: crates/lacon-cli/src/commands/doctor.rs
- FOUND: crates/lacon-cli/tests/cli_doctor.rs
- FOUND: crates/lacon-cli/src/cli.rs
- FOUND: .planning/quick/260522-v4a-doctor-check-both-project-and-user-setup/260522-v4a-SUMMARY.md
- FOUND commit: 1baa30c (Task 1, feat)
- FOUND commit: c233ed7 (Task 2, test)

---
*Phase: quick-260522-v4a*
*Completed: 2026-05-22*
