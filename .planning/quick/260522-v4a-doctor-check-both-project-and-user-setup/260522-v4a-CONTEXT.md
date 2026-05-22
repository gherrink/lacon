# Quick Task 260522-v4a: scope-aware `lacon doctor` — Context

**Gathered:** 2026-05-22
**Status:** Ready for planning — decisions below are LOCKED (resolved with the user before planning).

<domain>
## Task Boundary

Quick task 260522-tor made `lacon init` install at two scopes (project = cwd-relative, user =
`~/.claude` + `~/.config/lacon/rules`). `lacon doctor` still assumes the OLD single-scope model:
its hook check (`check_hook`) only inspects `<cwd>/.claude/settings.json`, and it never checks the
new `LACON.md` instructions file or the `@import` reference line that init now writes per scope.

This task reworks `doctor` to check BOTH scopes' full setup and mention both, and audits/fixes any
remaining single-scope assumptions in the codebase.

### What `init` writes per scope (the setup `doctor` must verify)
- **Project** (cwd-relative): hook in `./.claude/settings.json`; `./.claude/LACON.md`;
  `@.claude/LACON.md` reference in `./CLAUDE.md`; `./.lacon/` rules skeleton.
- **User** (home-relative): hook in `~/.claude/settings.json`; `~/.claude/LACON.md`;
  `@LACON.md` reference in `~/.claude/CLAUDE.md`; `~/.config/lacon/rules/` skeleton.

The verified import tokens (from 260522-tor, confirmed against live `claude 2.1.148`):
project = `@.claude/LACON.md`, user = `@LACON.md`. Extensionless `@LACON` does NOT resolve.
</domain>

<decisions>
## Implementation Decisions (LOCKED)

### Breadth — FULL setup, both scopes
For EACH scope (project + user), doctor checks three things and groups the output by scope:
1. **hook** — the scope's `settings.json` contains the `lacon-claude-hook` PreToolUse(Bash)
   fingerprint (reuse the existing `HOOK_FINGERPRINT` walk).
2. **instructions** — the scope's `LACON.md` exists (project `./.claude/LACON.md`;
   user `~/.claude/LACON.md`).
3. **reference** — the scope's `CLAUDE.md` contains the resolvable `@import` token for that scope
   (project `@.claude/LACON.md` in `./CLAUDE.md`; user `@LACON.md` in `~/.claude/CLAUDE.md`).
   Checking that the token STRING is present is sufficient (the token form was already empirically
   verified to resolve in 260522-tor); do not shell out to `claude`.

Output is grouped, e.g. a "Project setup" group and a "User setup" group, then the existing global
checks (rules / db-perms / tracker) below. Keep the `[ ok ] / [warn] / [fail]` line vocabulary;
labels stay greppable (the existing tests match on substrings like `hook`).

### Posture — the key rule
Define a scope as **configured** iff its `settings.json` contains the lacon hook.
Let `any_configured = project_configured || user_configured`.

- **Configured scope, complete** → `[ ok ]` for hook + instructions + reference.
- **Configured scope, broken** (hook present but `LACON.md` missing OR the `@import` reference
  missing from CLAUDE.md) → `[fail]` for the missing sub-check → flips exit to 1. (A half-installed
  scope is a real, actionable problem.)
- **Not-configured scope WHEN `any_configured` is true** → render NEUTRALLY (informational,
  e.g. `[ -- ] <scope> setup: not configured (optional)`). This is NOT a `[warn]` and NOT a
  `[fail]`; installing only one scope is a legitimate complete setup. **This is the user's
  explicit requirement: when one scope is installed, the other must NOT be flagged as a warning.**
- **Neither scope configured** (fresh machine / lacon not set up at all) → `[warn]` per the
  existing D-03 fresh-machine posture with the `run \`lacon init\`` hint; exit stays 0.
- **IO / parse errors** on a file that IS present (unreadable/unparseable settings.json or
  CLAUDE.md) → `[fail]` regardless (existing T-04-10 error posture).

### Intentional behavior change (call out in SUMMARY)
The old `doctor_settings_present_without_hook_is_a_hard_fail` behavior is REVISED. With two opt-in
scopes, a project that merely has a Claude `settings.json` without a lacon hook is no longer
"positively broken" — under the new model:
  - if the USER scope IS configured → the project scope is shown neutrally (not flagged), and
  - if NEITHER scope is configured → it is a `[warn]` (run `lacon init`), exit 0 (not a hard fail).
Update that test to the new posture. Document the change in the SUMMARY.

### Hermetic tests (CRITICAL)
The user-scope checks read `~/.claude/*` via `etcetera::home_dir()` (reads `$HOME`). The current
`cli_doctor.rs` `run_doctor` helper redirects `XDG_DATA_HOME` + `XDG_CONFIG_HOME` but NOT `HOME`,
so a naive user-scope check would read the developer's REAL `~/.claude`. The `run_doctor` helper
MUST also set `.env("HOME", <tempdir>)` (mirror `cli_init.rs`'s user-scope tests). All doctor tests
must target tempdir HOME/XDG, never the real home.

### Audit — other single-scope assumptions
Sweep for and fix remaining places that assume the old single-scope (project/cwd-only) model:
- `doctor.rs` module rustdoc (lines ~1-30) + `check_hook` rustdoc — describe both scopes + the new
  instructions/reference checks; drop "fixed five-check" framing if the check count changes.
- `cli.rs:64` Doctor help text ("Verify hooks installed, configs valid, rules parse.") — update to
  reflect both scopes / the new checks.
- `pnpm_e2e.rs` — calls bare `lacon init` (now project-default, non-interactive). Verify it still
  passes; if it needs a `--project` flag or HOME isolation to stay hermetic, add it.
- Check `docs/` (architecture.md / specs / any doctor description) for stale single-scope wording;
  update prose only where it materially misdescribes behavior (do not over-edit docs).
The audit is part of the deliverable, but keep fixes proportionate — doctor.rs is the substantive
change; the rest is wording/test hygiene.

### Claude's Discretion
- Exact neutral-line glyph/wording for the not-configured-but-other-installed case.
- Whether to add a small `user_claude_dir()` helper (mirroring init.rs's `~/.claude` resolution)
  vs inline; factor to avoid duplication with the existing `user_config_dir()` (which resolves the
  XDG `~/.config/lacon`, a DIFFERENT path — do not conflate `~/.claude` with the XDG config dir).
- Exact grouping headers / output ordering, as long as both scopes are clearly mentioned.
</decisions>

<specifics>
## Specific Ideas

- Add a `user_claude_dir()` -> `etcetera::home_dir()?.join(".claude")` helper (init.rs already does
  this). Reuse the existing `HOOK_FINGERPRINT` + the PreToolUse(Bash) walk from `check_hook` for
  both scopes — parameterize the settings path rather than duplicating the walk.
- Per-scope check fn signature like `check_scope(label, settings_path, lacon_md_path, claude_md_path,
  import_token) -> (configured: bool, ok: bool, lines...)` so project and user share one code path.
- The reference check is a substring scan for the scope's import token in CLAUDE.md (tolerant of the
  exact line). LACON.md check is `Path::is_file()`.
</specifics>

<canonical_refs>
## Canonical References

- `crates/lacon-cli/src/commands/doctor.rs` — current 5-check sweep (to be reworked).
- `crates/lacon-cli/tests/cli_doctor.rs` — current contract tests (HOME override + posture rework).
- `crates/lacon-cli/src/commands/init.rs` — scope path resolution + import tokens to mirror
  (project `@.claude/LACON.md`, user `@LACON.md`).
- `crates/lacon-cli/tests/cli_init.rs` (lines ~30-31, 166+) — `PROJECT_IMPORT`/`USER_IMPORT`
  constants + the HOME+XDG tempdir isolation pattern to mirror in doctor tests.
- `crates/lacon-cli/src/cli.rs:40` (Init help) and `:64` (Doctor help).
- 260522-tor SUMMARY — the empirically verified import tokens and the reason `@LACON` was rejected.
</canonical_refs>
