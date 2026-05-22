---
phase: quick-260522-tor
plan: 01
type: execute
wave: 1
depends_on: []
files_modified:
  - crates/lacon-cli/src/cli.rs
  - crates/lacon-cli/src/commands/init.rs
  - crates/lacon-cli/src/main.rs
  - crates/lacon-cli/tests/cli_init.rs
autonomous: true
requirements: [REQ-cli-init]
user_setup: []

must_haves:
  truths:
    - "Running `lacon init --project` installs project-scope artifacts (.lacon/.gitkeep, ./.claude/settings.json hook, ./.claude/LACON.md, @LACON-resolving reference line in ./CLAUDE.md)."
    - "Running `lacon init --user` installs user-scope artifacts (~/.claude/settings.json hook with unrelated config preserved, ~/.claude/LACON.md, @LACON-resolving reference in ~/.claude/CLAUDE.md, ~/.config/lacon/rules/ skeleton)."
    - "Passing both --user and --project installs both scopes in one run."
    - "Project scope with a missing ./CLAUDE.md warns it may not be a Claude Code setup and creates ./CLAUDE.md carrying the reference."
    - "Re-running any scope is byte-stable (idempotent): the @LACON reference line is never appended twice, the settings hook collapses to one canonical entry, LACON.md is overwritten with identical content."
    - "The old <!-- lacon:start/end --> marker-block code paths and tests are gone; instructions now live in a standalone LACON.md and CLAUDE.md only carries a resolvable @import reference."
    - "The written CLAUDE.md reference is a WORKING Claude Code @import that resolves to LACON.md (verified empirically; correct form chosen)."
    - "Tests are hermetic and non-interactive: scope is driven by flags, user-scope paths target a tempdir via HOME/XDG_CONFIG_HOME env overrides, no test blocks on a TTY prompt."
    - "The selection UX (TUI crate vs plain stdin) was chosen against a measured cold-start/build impact on the lacon binary, with the measurement documented."
  artifacts:
    - path: "crates/lacon-cli/src/commands/init.rs"
      provides: "Scope-aware init: --user/--project dispatch, per-scope path resolution, shared 3-step install (rules skeleton, settings hook, LACON.md + @import reference)"
      contains: "LACON.md"
    - path: "crates/lacon-cli/src/cli.rs"
      provides: "Init clap variant carrying --user and --project bool flags"
      contains: "Init"
    - path: "crates/lacon-cli/tests/cli_init.rs"
      provides: "Hermetic flag-driven contract tests for both scopes, idempotency, settings preservation, CLAUDE.md-missing warn+create, and @import resolvability"
      contains: "--user"
  key_links:
    - from: "crates/lacon-cli/src/commands/init.rs"
      to: "~/.claude/ (home-relative)"
      via: "etcetera::home_dir() (reads $HOME, overridable in tests)"
      pattern: "home_dir"
    - from: "crates/lacon-cli/src/commands/init.rs"
      to: "~/.config/lacon/rules/ (XDG)"
      via: "etcetera::choose_base_strategy().config_dir() mirroring loader.rs"
      pattern: "choose_base_strategy"
    - from: "crates/lacon-cli/src/commands/init.rs"
      to: "settings.json (both scopes)"
      via: "reused install_lacon_hook + atomic_write_json (path-agnostic)"
      pattern: "install_lacon_hook|atomic_write_json"
---

<objective>
Rework `lacon init` so it asks/accepts WHERE to install lacon — **project** scope (cwd-relative) and/or **user** scope (home-relative) — and installs the chosen scope(s). Replace the embedded `<!-- lacon:start/end -->` CLAUDE.md marker block with a standalone `LACON.md` file plus an idempotent `@LACON` import reference in CLAUDE.md.

Purpose: lacon must be installable both per-project (committed to a repo) and globally for one developer (`~/.claude`), and the instructions must survive in a form Claude Code actually loads. (Per the Claude Code memory spec, block-level HTML comments in CLAUDE.md are STRIPPED before injection — so the old marker-block body never reliably reached the model. A real `@import` to `LACON.md` does.)

Output: a scope-aware `init.rs`, a `--user`/`--project` clap surface, reworked hermetic contract tests, and a measurement-backed selection-UX decision recorded in the SUMMARY.
</objective>

<execution_context>
@$HOME/.claude/get-shit-done/workflows/execute-plan.md
@$HOME/.claude/get-shit-done/templates/summary.md
</execution_context>

<context>
@.planning/quick/260522-tor-init-choose-project-vs-user-local-scope-/260522-tor-CONTEXT.md
@CLAUDE.md

<!-- LOCKED DECISIONS (from CONTEXT.md) — non-negotiable: -->
<!-- D-naming: file is LACON.md, token is @LACON (NOT LACONE). -->
<!-- D-replace: REMOVE marker-block code+tests entirely (never shipped, no migration). -->
<!-- D-flags: --user and --project flags; both may be passed → both scopes. -->
<!-- D-scopes: project = cwd-relative; user = ~/.claude/* + ~/.config/lacon/rules/. -->
<!-- D-missing-claudemd: project scope only — warn + offer to create ./CLAUDE.md. -->
<!-- D-ux: TUI vs stdin chosen EMPIRICALLY by cold-start/build measurement. -->
<!-- D-preserve: reuse serde_json scrub-then-reinsert + atomic_write_json + perm preservation; ~/.claude/settings.json is REAL config. -->
<!-- D-import: verify the @import actually resolves to LACON.md; keep @LACON only if it truly resolves. -->
<!-- D-hermetic: tests flag-driven + non-interactive; user scope writes to a tempdir via HOME/XDG override, never real ~/.claude. -->

<interfaces>
<!-- REUSE AS-IS from current init.rs (path-agnostic once given a settings path): -->
<!--   fn install_lacon_hook(settings: &mut serde_json::Value)  — scrub-then-reinsert, idempotent -->
<!--   fn atomic_write_json(path: &Path, value: &serde_json::Value) -> anyhow::Result<()>  — tempfile persist + Unix perm preservation -->
<!-- REMOVE entirely (marker-block era): -->
<!--   const LACON_START / LACON_END -->
<!--   fn install_claude_md_block / strip_lacon_markers / append_fresh_block -->
<!--   their #[cfg(test)] unit tests -->

<!-- Home / XDG resolution (verified against etcetera 0.11):
       etcetera::home_dir() -> Result<PathBuf, HomeDirError>  wraps std::env::home_dir() => reads $HOME on Unix.
         Use for ~/.claude/{settings.json,LACON.md,CLAUDE.md}: etcetera::home_dir()?.join(".claude").
       etcetera::choose_base_strategy()?.config_dir() honours XDG_CONFIG_HOME on Linux; on macOS uses $HOME/Library/Application Support (apple strategy).
         Use for ~/.config/lacon/rules/, mirroring loader.rs:111-113 and doctor.rs:366-371. -->

<!-- Claude Code @import rules (from https://code.claude.com/docs/en/memory, fetched 2026-05-22):
       - Relative @paths resolve relative to the FILE CONTAINING the import (not cwd, not project root).
         => In ./CLAUDE.md, `@.claude/LACON.md` points at ./.claude/LACON.md.
         => In ~/.claude/CLAUDE.md, `@LACON.md` points at ~/.claude/LACON.md.
       - Docs show extensioned examples (@package.json, @docs/git-instructions.md) AND one extensionless (@README).
         Extensionless resolution (`@LACON` -> LACON.md) is NOT guaranteed by the spec → Task 2 verifies empirically. -->
</interfaces>

# Files to rework (read in full before editing):
@crates/lacon-cli/src/commands/init.rs
@crates/lacon-cli/tests/cli_init.rs
@crates/lacon-cli/src/cli.rs
</context>

<tasks>

<task type="auto">
  <name>Task 1: Add --user/--project flags and scope-aware dispatch + per-scope path resolution</name>
  <files>crates/lacon-cli/src/cli.rs, crates/lacon-cli/src/commands/init.rs, crates/lacon-cli/src/main.rs</files>
  <action>
Change the clap `Init` variant in cli.rs from a unit variant to a struct variant carrying two bool flags (per D-flags): `#[arg(long)] user: bool` and `#[arg(long)] project: bool`. Update the dispatch site in main.rs (and/or wherever `CliCommand::Init` is matched) to pass both flags into `init::execute(user, project)`.

Refactor `init::execute` to take `(user: bool, project: bool)` and resolve a set of target scopes:
- Define a `Scope` notion (project | user). For each selected scope, compute its three base paths up front (parameterize paths, do NOT duplicate the install logic — D's Claude's-Discretion factoring guidance):
  - **Project scope** (cwd-relative): rules skeleton dir = `cwd/.lacon` (+ `.gitkeep`); settings = `cwd/.claude/settings.json`; instructions file = `cwd/.claude/LACON.md`; reference target = `cwd/CLAUDE.md` (repo root); the in-CLAUDE.md import token is resolved by Task 2.
  - **User scope** (home-relative): settings = `etcetera::home_dir()?.join(".claude/settings.json")`; instructions file = `etcetera::home_dir()?.join(".claude/LACON.md")`; reference target = `etcetera::home_dir()?.join(".claude/CLAUDE.md")`; rules skeleton dir = `choose_base_strategy()?.config_dir().join("lacon").join("rules")` (mirror loader.rs:111-113; create the dir with a `.gitkeep` so it survives). Use `etcetera::home_dir()` for the `~/.claude/*` triple (it reads `$HOME`, which the tests override) — do NOT use `config_dir()` for `~/.claude` (that is literally `$HOME/.claude`, not the XDG config dir).
- Scope selection (D-ux selection rules): if `project` set → include project scope; if `user` set → include user scope; if BOTH set → include both. If NEITHER flag set: when `std::io::stdin().is_terminal()` (a TTY) → prompt for the scope (implementation chosen in Task 2); when NOT a TTY (CI / hermetic tests / scripted) → do NOT block; pick the documented deterministic default of **project** scope and proceed. Print which scope(s) were selected.

Implement a single `install_scope(paths)` helper run once per selected scope that performs the three install steps against that scope's paths:
  1. Create the rules skeleton dir + `.gitkeep` (project: `.lacon/.gitkeep`; user: `~/.config/lacon/rules/.gitkeep`).
  2. Read/parse settings.json as `serde_json::Value`, refuse a non-object (keep the existing guard + `Ok(1)` convention), call the REUSED `install_lacon_hook(&mut settings)`, then REUSED `atomic_write_json(path, &settings)`. This preserves unrelated user config and file permissions for `~/.claude/settings.json` (D-preserve). Keep the existing read-error / parse-error → `eprintln!("lacon init: …")` + `Ok(1)` handling.
  3. Write the standalone instructions file (`LACON.md`) with body that MUST mention the `!!` bypass prefix and `LACON_DISABLE=1` (preserving the user-trust property the old block guaranteed — Claude's-Discretion phrasing). Then install the idempotent `@import` reference line into the scope's CLAUDE.md (the detect-and-skip / detect-and-rewrite reference logic is built in Task 2). For PROJECT scope only: if `cwd/CLAUDE.md` does NOT exist, `eprintln!` a warning that this may not be a Claude Code setup and create `cwd/CLAUDE.md` containing the reference (D-missing-claudemd). For USER scope, `~/.claude/CLAUDE.md` is expected to exist; append/refresh the reference line idempotently.

Keep `execute`'s `Ok(0)` success / `Ok(1)` recoverable-error contract. Update the module rustdoc at the top of init.rs to describe scopes + the LACON.md + @import model (delete the stale marker-block prose). REMOVE the marker-block items now (consts `LACON_START`/`LACON_END`, fns `install_claude_md_block`/`strip_lacon_markers`/`append_fresh_block`, and their `#[cfg(test)]` unit tests) — Task 2 supplies the replacement reference-line logic and tests. `install_lacon_hook` and `atomic_write_json` (and their unit tests) stay.
  </action>
  <verify>
    <automated>cargo build -p lacon-cli 2>&1 | tail -5 &amp;&amp; grep -c "install_claude_md_block\|strip_lacon_markers\|LACON_START\|LACON_END" crates/lacon-cli/src/commands/init.rs</automated>
  </verify>
  <done>`cargo build -p lacon-cli` succeeds; `lacon init --user`, `--project`, and both compile and dispatch; the grep count for marker-block identifiers in init.rs is 0; `install_lacon_hook`/`atomic_write_json` remain and still parameterize over a settings path.</done>
</task>

<task type="auto">
  <name>Task 2: Choose selection UX empirically; implement idempotent verified @import reference; rework hermetic tests</name>
  <files>crates/lacon-cli/src/commands/init.rs, crates/lacon-cli/Cargo.toml, crates/lacon-cli/tests/cli_init.rs</files>
  <action>
**(a) Empirical selection-UX decision (D-ux).** Measure the cold-start/build impact of adding a TUI select crate (e.g. `dialoguer`) before deciding. The hot path is `lacon run`; the gates are the deterministic `crates/lacon-core/benches/tracker_open.rs` (criterion, panics if `Tracker::open` median > 3700µs) and the soft probe `scripts/bench-cold-start.sh`. Procedure:
  - Record a baseline: build `cargo build --release -p lacon-cli`, run `scripts/bench-cold-start.sh` and note the `lacon` startup figure; note release binary size of `target/release/lacon`.
  - Temporarily add `dialoguer` to `crates/lacon-cli/Cargo.toml`, rebuild release, re-run the soft probe and re-measure binary size; capture build-time delta. (`lacon init` is NOT on the hot path, but dialoguer pulls transitive deps that link into the single `lacon` binary and inflate cold start / size — that is the regression to measure.)
  - Decision rule: if the cold-start delta keeps `lacon` startup well under the 10ms budget AND binary-size growth is acceptable, KEEP dialoguer and use it for the interactive scope prompt. Otherwise REVERT the Cargo.toml change and implement the interactive prompt as a plain `std::io::stdin().read_line` (no new crate). Either way the prompt is only reached on a TTY with neither flag passed (Task 1), so it is never hit in hermetic tests.
  - Record the measured baseline-vs-dialoguer numbers and the chosen option in the SUMMARY (D-ux requires the measurement be documented). Do NOT leave dialoguer in Cargo.toml if it was rejected.

**(b) Verify the @import actually resolves, then implement the reference line (D-import).** Confirm which import form Claude Code resolves to `LACON.md`. Per the fetched Claude Code memory spec, relative `@paths` resolve relative to the file containing the import, so:
  - In `~/.claude/CLAUDE.md`, the resolvable form for `~/.claude/LACON.md` is `@LACON.md` (or `@~/.claude/LACON.md`).
  - In `./CLAUDE.md` (repo root), `LACON.md` lives at `./.claude/LACON.md`, so the resolvable form is `@.claude/LACON.md`.
  Empirically confirm whether the extensionless `@LACON` resolves (the spec shows `@README` working but is not explicit for arbitrary names). Confirmation method: write a temp `CLAUDE.md` containing the candidate import beside a temp `LACON.md`, and check resolution (e.g. `claude` `/memory` listing or the documented `InstructionsLoaded` hook) IF a `claude` binary is available; if not available in this environment, default to the extensioned, spec-guaranteed form. **Keep `@LACON` only if it genuinely resolves; otherwise write the correct resolvable path per scope** (`@LACON.md` for user `~/.claude/CLAUDE.md`, `@.claude/LACON.md` for project `./CLAUDE.md`). Record the verification outcome and the exact token written per scope in the SUMMARY.

  Implement `install_reference_line(existing_md: &str, import_line: &str) -> String` (replacing the deleted marker-block fns): the import line carries a stable, detectable form so re-runs are byte-stable and never append twice (idempotency). Detect an already-present lacon import line (match the import token, tolerant of trailing whitespace) and leave the file unchanged if present; otherwise append it at EOF with a clean newline boundary (and blank-line separation when the file is non-empty). A second pass over the output must be byte-identical.

**(c) Rework the hermetic contract tests** in `crates/lacon-cli/tests/cli_init.rs` (D-hermetic). DELETE the marker-block tests (`init_in_empty_dir_creates_skeleton`'s marker assertions, `init_orphan_claude_md_marker_recovery_is_idempotent`, and any `<!-- lacon:start -->` assertions). REWRITE/ADD, all flag-driven and non-interactive:
  - **project scope** (`lacon init --project` in a tempdir cwd): asserts `.lacon/.gitkeep`, `.claude/settings.json` carries `lacon-claude-hook` under matcher=Bash, `.claude/LACON.md` exists and contains `!!` + `LACON_DISABLE`, and `CLAUDE.md` contains the resolvable import token (the one chosen in (b)) — NOT any `<!-- lacon` marker.
  - **project CLAUDE.md-missing warn+create**: empty cwd + `--project` → `CLAUDE.md` is created containing the import; stderr carries the "may not be a Claude Code setup" warning (assert via `predicates`).
  - **user scope** (`lacon init --user`): set `.env("HOME", tmp_home)` AND `.env("XDG_CONFIG_HOME", tmp_xdg)` (mirroring the existing tracking tests at tracking_coldstart.rs:37-38) so the binary writes to the tempdir, NEVER the real `~/.claude`. Pre-seed `tmp_home/.claude/settings.json` with real-looking unrelated config (`"model": "..."` + a user Bash hook + an Edit matcher) and `tmp_home/.claude/CLAUDE.md` with user content. Assert: hook added while unrelated config + user hooks preserved; `tmp_home/.claude/LACON.md` written with `!!`/`LACON_DISABLE`; `tmp_home/.claude/CLAUDE.md` gains the resolvable import once and keeps its prior content; `tmp_xdg/lacon/rules/` skeleton exists.
  - **both scopes** (`--user --project`): one invocation installs both; assert one project artifact and one user artifact.
  - **idempotency** (per scope): two runs → settings.json and CLAUDE.md and LACON.md are byte-identical across runs; the import line appears exactly once.
  - KEEP `init_preserves_existing_settings_file_permissions` (still valid; drive it with an explicit scope flag).
  Every test must drive scope via flags so none blocks on a TTY prompt.
  </action>
  <verify>
    <automated>cargo build --workspace &amp;&amp; cargo test -p lacon-cli --test cli_init 2>&1 | tail -15</automated>
  </verify>
  <done>`cargo test -p lacon-cli --test cli_init` passes; user-scope tests write only to the HOME/XDG tempdirs (verified by the test asserting against tempdir paths, not `~`); the chosen import token resolves (or the spec-guaranteed extensioned form is used) and appears exactly once after two runs; the SUMMARY records the cold-start/size measurement, the UX choice, and the verified import token per scope; if dialoguer was rejected it is absent from Cargo.toml.</done>
</task>

</tasks>

<threat_model>
## Trust Boundaries

| Boundary | Description |
|----------|-------------|
| init → `~/.claude/settings.json` | lacon writes into a REAL pre-existing user-config file that may contain unrelated hooks and top-level keys. |
| init → `~/.claude/CLAUDE.md` | lacon appends to a real user memory file; must not duplicate or clobber. |
| test → real `$HOME` | A non-hermetic user-scope test could write to the developer's actual `~/.claude`. |

## STRIDE Threat Register

| Threat ID | Category | Component | Disposition | Mitigation Plan |
|-----------|----------|-----------|-------------|-----------------|
| T-tor-01 | Tampering | `install_scope` settings write | mitigate | Reuse `install_lacon_hook` scrub-then-reinsert + `atomic_write_json` (tempfile persist, Unix perm preservation); refuse non-object settings with `Ok(1)`; covered by user-scope preservation test. |
| T-tor-02 | Tampering | `install_reference_line` on CLAUDE.md | mitigate | Idempotent detect-and-skip on the import token; append-only at EOF; never rewrites user content; byte-stable across runs (idempotency test). |
| T-tor-03 | Information disclosure / Tampering | user-scope tests vs real `$HOME` | mitigate | Tests override `HOME` + `XDG_CONFIG_HOME` to tempdirs (existing pattern at tracking_coldstart.rs:37-38); assertions target tempdir paths only; no real `~/.claude` write. |
| T-tor-04 | Repudiation | broken/unresolvable @import silently shipped | mitigate | Task 2(b) empirically verifies the import resolves to LACON.md; falls back to the spec-guaranteed extensioned form; outcome recorded in SUMMARY. |
| T-tor-SC | Tampering | crates.io install (`dialoguer`, conditional) | mitigate | dialoguer is a widely-used crate; added ONLY if the cold-start/size measurement passes, else reverted. No system-lib/fetch steps (hermetic CI preserved). |
</threat_model>

<verification>
- `cargo build --workspace` succeeds (debug bins materialized for assert_cmd, per CLAUDE.md).
- `cargo test -p lacon-cli --test cli_init` passes — both scopes, both-at-once, idempotency, settings preservation, CLAUDE.md-missing warn+create, perm preservation.
- `cargo test -p lacon-cli` (unit tests in init.rs) passes — `install_lacon_hook` + `atomic_write_json` retained tests green; no marker-block tests remain.
- `grep -c "LACON_START\|LACON_END\|install_claude_md_block\|strip_lacon_markers\|append_fresh_block\|lacon:start\|lacon:end" crates/lacon-cli/src/commands/init.rs crates/lacon-cli/tests/cli_init.rs` returns 0 (marker-block era fully removed).
- `cargo clippy -p lacon-cli --all-targets` clean.
- `cargo fmt --check` clean.
- SUMMARY documents: the cold-start/binary-size measurement, the TUI-vs-stdin choice, and the verified @import token written per scope.
</verification>

<success_criteria>
- `lacon init --project`, `lacon init --user`, and `lacon init --user --project` each install the correct scope artifacts; no-flag non-TTY defaults to project; no-flag TTY prompts.
- Instructions live in a standalone `LACON.md`; CLAUDE.md carries a WORKING, idempotent `@import` reference (verified, correct form per scope); the `<!-- lacon:start/end -->` marker block is gone from source and tests.
- `~/.claude/settings.json` user-scope install preserves unrelated config and permissions.
- Tests are hermetic (HOME/XDG tempdir overrides) and non-interactive (flag-driven).
- The selection-UX choice is backed by a documented measurement.
</success_criteria>

<output>
Create `.planning/quick/260522-tor-init-choose-project-vs-user-local-scope-/260522-tor-SUMMARY.md` when done. The SUMMARY MUST record: (1) the baseline-vs-dialoguer cold-start and binary-size measurement and the resulting TUI-vs-stdin decision; (2) the empirically verified `@import` token written per scope and how it was confirmed to resolve to LACON.md; (3) the final scope-dispatch behavior for the no-flag TTY and no-flag non-TTY cases.
</output>
