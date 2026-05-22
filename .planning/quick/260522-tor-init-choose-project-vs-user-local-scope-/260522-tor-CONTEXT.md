# Quick Task 260522-tor: scope-aware `lacon init` — Context

**Gathered:** 2026-05-22
**Status:** Ready for planning — decisions below are LOCKED (resolved with the user before planning).

<domain>
## Task Boundary

Rework `lacon init` (crates/lacon-cli/src/commands/init.rs) so it asks **where** to install
lacon and installs into the chosen scope. Two scopes:

**Project scope** (cwd-relative — the current behavior's location):
- `.lacon/` rules skeleton (with `.gitkeep`) — unchanged from today.
- Hook entry in `./.claude/settings.json` (PreToolUse Bash) — unchanged mechanics.
- Instructions written to `./.claude/LACON.md` (NEW file).
- `@LACON` reference in `./CLAUDE.md` (repo root).
- If `./CLAUDE.md` does NOT exist: print a warning that this is potentially not a Claude
  Code setup, and ask whether to create `./CLAUDE.md` (containing the reference).

**User / local scope** (home-relative):
- Hook entry in `~/.claude/settings.json` (same scrub-then-reinsert mechanics; MUST preserve
  the user's existing settings — this file already exists and is real).
- Instructions written to `~/.claude/LACON.md` (NEW file).
- `@LACON` reference in `~/.claude/CLAUDE.md`.
- User rules skeleton created at `~/.config/lacon/rules/` (the XDG path the loader already
  reads — resolve via `etcetera`, mirroring `loader.rs` / `doctor.rs`).
</domain>

<decisions>
## Implementation Decisions (LOCKED)

### Naming
- The instructions file is `LACON.md` and the reference token is `@LACON` — NOT `LACONE`.
  Rationale: the user's existing `~/.claude/CLAUDE.md` already contains `@LACON`, and the
  project/binary is named `lacon`. "LACONE" in the original request was a typo.

### Instructions delivery — REPLACE the embedded marker block
- Today instructions live INSIDE CLAUDE.md inside `<!-- lacon:start -->…<!-- lacon:end -->`
  markers. That approach is being REPLACED. Instructions now live in a standalone `LACON.md`
  file; CLAUDE.md only carries the `@LACON` reference line.
- No backward-compatibility / migration path is required: the project was never shipped, so
  no user has the old embedded block in the wild. The old marker-block code paths
  (`install_claude_md_block`, `strip_lacon_markers`, `append_fresh_block`, the
  `LACON_START`/`LACON_END` consts and their orphan-recovery logic) and their tests can be
  removed and replaced with the new reference-line logic.
- The `@LACON` reference itself must still be installed idempotently (a re-run must not append
  the line twice). Use a stable, detectable marker for the reference line so re-runs are
  byte-stable — and verify the actual Claude Code `@`-import path that resolves `LACON.md`
  (e.g. whether it must be `@LACON`, `@LACON.md`, or `@.claude/LACON.md` from the referencing
  file's directory). Make the written reference a WORKING import, and keep `@LACON` only if
  Claude Code actually resolves it to `LACON.md`; otherwise write the correct resolvable path.

### Selection UX
- Always add `--user` and `--project` flags (both may be passed → install both scopes).
- Decide TUI vs stdin EMPIRICALLY: evaluate adding a TUI select crate (e.g. `dialoguer`) and
  measure the cold-start / build-cost impact on the `lacon` binary's hot path (`lacon run` is
  invoked many times per session; the deterministic gate is
  `crates/lacon-core/benches/tracker_open.rs` and the soft probe is
  `scripts/bench-cold-start.sh`). If the regression is within budget, use the TUI; otherwise
  fall back to a plain stdin prompt (no new crate). Document the measurement and the choice
  in the SUMMARY.
- When neither `--user` nor `--project` is passed AND stdin is a TTY → prompt for the scope.
  When neither flag is passed and stdin is NOT a TTY (CI / scripted / hermetic tests) → do
  NOT block on a prompt; pick a deterministic, documented default (project) or require a flag.
  Tests must remain hermetic and non-interactive (drive via flags).

### Existing-config handling
- `~/.claude/settings.json` already exists and holds real user config — the user-scope hook
  install MUST preserve unrelated config (reuse the existing serde_json scrub-then-reinsert +
  atomic-write + permission-preserving logic).
- The CLAUDE.md-missing case applies to project scope (warn + offer to create). For user scope
  `~/.claude/CLAUDE.md` already exists.

### Claude's Discretion
- Exact phrasing of the instructions written to `LACON.md` (must still mention the `!!` bypass
  prefix and `LACON_DISABLE=1`, preserving the user-trust property the old block guaranteed).
- How to factor the shared install logic so project and user scopes share code (parameterize
  paths rather than duplicate).
- Exact wording of warnings/prompts.
</decisions>

<specifics>
## Specific Ideas

- Refactor `execute()` to take a resolved set of target paths per scope, then run the same
  three install steps (rules skeleton, settings hook, instructions+reference) against each
  selected scope.
- Reuse `install_lacon_hook`, `atomic_write_json` as-is (they are path-agnostic once given the
  settings path).
- For project scope, keep creating `.lacon/.gitkeep`. For user scope, create
  `~/.config/lacon/rules/` (resolved via etcetera, same as loader.rs:111-113).
</specifics>

<canonical_refs>
## Canonical References

- `crates/lacon-cli/src/commands/init.rs` — current implementation (to be reworked).
- `crates/lacon-cli/tests/cli_init.rs` — current contract tests (to be reworked).
- `crates/lacon-core/src/rules/loader.rs:108-117` — user rules dir resolution via etcetera.
- `crates/lacon-cli/src/commands/doctor.rs:364-369` — etcetera config-dir helper pattern.
- `CLAUDE.md` (repo root) "Load-bearing design constraints" — cold-start budget on the hook
  hot path; hermetic CI (adding a crates.io dep is allowed; no system-lib/fetch steps).
- ADR 0001 (Claude Code hooks, not PATH shims) — do not add shell-env escape hatches.
</canonical_refs>
