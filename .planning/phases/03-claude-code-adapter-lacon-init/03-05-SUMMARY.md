---
phase: 03-claude-code-adapter-lacon-init
plan: 05
subsystem: cli
tags: [lacon-init, settings-json, claude-md, idempotent, atomic-write, serde_json, tempfile]

# Dependency graph
requires:
  - phase: 03-claude-code-adapter-lacon-init
    plan: 04
    provides: lacon-claude-hook binary (PreToolUse Bash hook) that `lacon init` registers in .claude/settings.json
  - phase: 03-claude-code-adapter-lacon-init
    plan: 01
    provides: serde_json workspace dependency; lacon-claude-hook command-string fingerprint
provides:
  - "lacon init full implementation: .lacon/.gitkeep skeleton + PreToolUse(Bash) lacon-claude-hook entry in .claude/settings.json + CLAUDE.md marker block"
  - "install_lacon_hook: scrub-then-reinsert serde_json::Value walk (D-12, D-28) — idempotent, user-content-preserving"
  - "install_claude_md_block / append_fresh_block: HTML-comment marker detect-and-replace (D-14)"
  - "atomic_write_json: tempfile::NamedTempFile::persist 2-space-indent + trailing-newline writer (D-13)"
  - "4 e2e tests (cli_init.rs) + 7 unit tests locking create/idempotent/preserve/drift contract"
affects:
  - "Phase 4 (lacon doctor): canonical settings.json shape + lacon-claude-hook fingerprint that doctor detects"
  - "Phase 6 (acceptance): REQ-acceptance-pnpm-end-to-end relies on lacon init wiring the hook with no manual config"

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "serde_json::Value walk for clobber-safe JSON config editing (parse-edit-rewrite, preserve unknown keys)"
    - "Command-string fingerprint (starts_with) as the lacon-managed marker — no schema-fragile sibling field (D-12, D-28)"
    - "Scrub-then-reinsert for byte-stable idempotency across re-runs"
    - "Atomic config write via tempfile::NamedTempFile::new_in(parent) + persist (POSIX rename(2))"
    - "HTML-comment markers for markdown-renderer-safe block detection"

key-files:
  created:
    - crates/lacon-cli/tests/cli_init.rs
  modified:
    - crates/lacon-cli/Cargo.toml
    - crates/lacon-cli/src/commands/init.rs
    - Cargo.lock

key-decisions:
  - "Defensive non-object guard: if .claude/settings.json parses to a non-object (bare array/scalar) lacon init refuses (Ok(1)) rather than clobbering — extends D-11 beyond the happy path."
  - "Malformed nested `hooks`/`PreToolUse` (wrong JSON type) are reset to empty object/array inside install_lacon_hook so the walk never panics on user-corrupted files."
  - "Cargo.lock amended into Task 1's commit to keep the lockfile in sync with the serde_json manifest addition (Rule 3 blocking-issue fix)."
  - "(Some,Some) start>=end ordering folded into the orphan-marker arm — any corrupt CLAUDE.md marker state appends fresh + warns, never destroying user content."

requirements-completed: [REQ-cli-init]

# Metrics
duration: 2min
completed: 2026-05-21
---

# Phase 3 Plan 05: `lacon init` Summary

**`lacon init` fully implemented — it creates the `.lacon/.gitkeep` skeleton, installs (or refreshes) the lacon-managed `PreToolUse(Bash)` `lacon-claude-hook` entry inside `.claude/settings.json` via a clobber-safe `serde_json::Value` scrub-then-reinsert walk written atomically through `tempfile::NamedTempFile::persist`, and appends/refreshes a `<!-- lacon:start -->…<!-- lacon:end -->` CLAUDE.md note mentioning `!!` and `LACON_DISABLE=1` — locked by 4 e2e tests (create / idempotent / preserve / drift) and 7 unit tests, with the 6-command CLI surface intact.**

## Performance

- **Duration:** ~2 min
- **Started:** 2026-05-21T19:37:57Z
- **Completed:** 2026-05-21T19:40:00Z
- **Tasks:** 2
- **Files modified:** 3 (1 created, 2 modified; +Cargo.lock)

## Accomplishments

- **`lacon init` implementation (init.rs).** Replaced the 6-line Phase 1 stub with the full `execute() -> anyhow::Result<i32>` flow: (A) `.lacon/` + `.lacon/.gitkeep` skeleton, (B) `.claude/settings.json` read-parse-walk-atomicwrite, (C) `CLAUDE.md` read-replace-write, (D) success print. Returns `Ok(1)` with a `lacon init:`-prefixed stderr message on parse/IO failure.
- **`install_lacon_hook` clobber-safe walk (D-11/D-12/D-28).** Ensures `hooks.PreToolUse[]` exists, scrubs every Bash-matcher inner hook whose `command` starts with `lacon-claude-hook`, drops now-empty Bash groups, then appends exactly one canonical entry. Non-Bash matcher groups, user-authored Bash hooks, and all top-level keys (`model`, `theme`, …) are preserved untouched. Scrub-then-reinsert makes the output byte-stable across runs.
- **`install_claude_md_block` + `append_fresh_block` (D-14).** HTML-comment marker detection: both markers in order → in-place span replacement; orphan/corrupt marker → fresh block at EOF + stderr warning (never destroy user content); no markers → append at EOF with a blank-line separator.
- **`atomic_write_json` (D-13).** `create_dir_all(parent)` (creates `.claude/` if missing) + `NamedTempFile::new_in(parent)` (same-filesystem) + 2-space pretty serialization + trailing newline + `persist` (atomic POSIX rename). A concurrent `claude` startup sees old-or-new, never half-written.
- **11 tests total.** 7 unit tests in `init.rs` (hook walk into empty object, idempotency no-op, user-hook preservation, drift collapse, CLAUDE.md append/idempotency/orphan) + 4 e2e tests in `cli_init.rs` driving the real `lacon` binary in tempdirs.
- **Closed REQ-cli-init.** Phase 3's user-visible opt-in command is complete. All six Phase 3 requirement IDs are now covered (5 by Plans 01–04, REQ-cli-init here).

## Task Commits

Each task was committed atomically:

1. **Task 1: Cargo.toml deps + init.rs implementation** - `a84f5f1` (feat) — includes amended Cargo.lock
2. **Task 2: cli_init.rs e2e suite** - `96411d7` (test)

## Files Created/Modified

- `crates/lacon-cli/Cargo.toml` - Added `serde_json = { workspace = true }` + `tempfile = { workspace = true }` to `[dependencies]`. (`tempfile` remains in `[dev-dependencies]` too — Cargo allows both.)
- `crates/lacon-cli/src/commands/init.rs` - Full implementation: `execute`, `install_lacon_hook`, `install_claude_md_block`, `append_fresh_block`, `atomic_write_json`, `BLOCK_BODY` const + 7 unit tests (359 lines).
- `crates/lacon-cli/tests/cli_init.rs` (created) - 4 e2e tests via `assert_cmd::cargo_bin("lacon")` in isolated tempdirs.
- `Cargo.lock` - `serde_json` added to the `lacon-cli` dependency list (lockfile sync).

## Decisions Made

- **Defensive non-object guard on settings.json.** D-11 describes the happy path (object or missing). I added a guard: if the file parses to a non-object JSON value (bare array/scalar), `lacon init` refuses with `Ok(1)` rather than discarding the user's file. Conservative posture consistent with the plan's clobber-safety threat (T-settings-clobber).
- **Type-resilient nested-path coercion.** Inside `install_lacon_hook`, if `hooks` or `PreToolUse` exist but are the wrong JSON type (user corruption), they are reset to `{}` / `[]` so the walk proceeds without panicking on `.expect(...)`.
- **Corrupt-ordering CLAUDE.md folded into the orphan arm.** `(Some, Some)` with `start >= end` (markers reversed) is treated like the single-orphan case: append fresh + warn, leave existing markers untouched.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Cargo.lock out of sync with manifest dependency addition**
- **Found during:** Task 1 (post-commit deletion/status check)
- **Issue:** Adding `serde_json` to `lacon-cli`'s `[dependencies]` updated `Cargo.lock` (tracked file), but the initial Task 1 commit staged only the manifest + source. A lockfile that lags its manifest is a CI/reproducibility hazard.
- **Fix:** Amended the Task 1 commit to include `Cargo.lock` (single new line: `serde_json` under `lacon-cli`'s deps; `tempfile` was already listed via dev-deps).
- **Files modified:** `Cargo.lock`
- **Commit:** `a84f5f1` (Task 1, amended)

---

**Total deviations:** 1 auto-fixed (1 Rule 3 blocking-issue).
**Impact on plan:** None on scope or behavior — purely keeps the committed lockfile consistent with the manifest. No architectural changes, no checkpoints.

## Issues Encountered

- `lacon-cli` is a binary-only crate (no lib target), so `cargo test -p lacon-cli --lib` errors; unit tests run via `--bin lacon`. Not a defect — noted for future test invocation.
- Pre-existing clippy warning in `crates/lacon-cli/tests/tracking_e2e.rs` (1 warning, unchanged since Phase 2) — out of scope per the SCOPE BOUNDARY rule. `init.rs` and `cli_init.rs` are clippy-clean.
- The `test_emitter` "ignoring invalid dependency … missing a lib target" warning is pre-existing workspace noise, unrelated to this plan.

## Threat Surface Scan

No new security-relevant surface beyond the plan's `<threat_model>`. All six registered threats (T-03-05-01 user-hook preservation, -02 idempotency, -03 atomic write, -04 CLAUDE.md marker detection, -05 corrupt-marker accept, -06 CLI surface cap) are mitigated/accepted as planned and exercised by the test suite. The added non-object/wrong-type guards strengthen T-03-05-01 (clobber-safety) without introducing new surface.

## User Setup Required

None for this plan. End users opt in by running `lacon init` in their project; that command does all the wiring (`.lacon/`, `.claude/settings.json` hook, `CLAUDE.md` note) with no manual config.

## Next Phase Readiness

- **Phase 3 is complete.** All six phase requirement IDs are covered: REQ-adapter-pretooluse-only / -bypass-detection / -chained-commands / -tui-bypass / -pipes-passthrough (Plans 01–04) + REQ-cli-init (this plan). Ready for `/gsd-verify-work 3` (UAT + verification).
- **Phase 4 (`lacon doctor`)** can detect the installed hook via the canonical `.claude/settings.json` shape and the `lacon-claude-hook` command-string fingerprint established here.
- **Phase 6 acceptance** (`lacon init` → `pnpm install` → filtered output) is unblocked: `lacon init` registers the `PreToolUse(Bash)` hook pointing at the Plan 04 binary with zero manual steps.

## TDD Gate Compliance

The plan tasks are marked `tdd="true"`, but `tdd_mode` is `false` in `config.json`, so the plan-level RED→GREEN gate is not enforced for this phase. Tests were authored alongside the implementation (unit tests in Task 1, e2e in Task 2) — appropriate for a config-editing command whose behavior is fully enumerated by the D-11..D-14/D-28 spec and exercised end-to-end before commit. No separate `test(...)`-before-`feat(...)` gate sequence was required; the Task 1 `feat` commit shipped with its 7 unit tests, and Task 2's `test` commit adds the e2e coverage layer.

## Self-Check: PASSED

- FOUND: crates/lacon-cli/src/commands/init.rs
- FOUND: crates/lacon-cli/tests/cli_init.rs
- FOUND: crates/lacon-cli/Cargo.toml
- FOUND commit: a84f5f1 (Task 1)
- FOUND commit: 96411d7 (Task 2)

---
*Phase: 03-claude-code-adapter-lacon-init*
*Completed: 2026-05-21*
