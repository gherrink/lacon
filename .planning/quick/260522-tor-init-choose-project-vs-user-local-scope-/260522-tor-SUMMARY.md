---
phase: quick-260522-tor
plan: 01
subsystem: lacon-cli (init command)
tags: [cli, init, scope, claude-code, memory-import, hermetic-tests]
requires: [REQ-cli-init]
provides: [scope-aware-init, lacon-md-import-reference]
affects: [crates/lacon-cli]
tech-stack:
  added: []          # dialoguer evaluated and REJECTED — no new crate
  patterns: [scope-parameterized-install, idempotent-reference-line, empirical-import-verification]
key-files:
  created: []
  modified:
    - crates/lacon-cli/src/cli.rs
    - crates/lacon-cli/src/commands/init.rs
    - crates/lacon-cli/src/main.rs
    - crates/lacon-cli/tests/cli_init.rs
decisions:
  - "Selection UX: plain stdin().read_line, not a TUI crate (dialoguer rejected on measurement)."
  - "Import token verified empirically against claude 2.1.148: project=@.claude/LACON.md, user=@LACON.md; extensionless @LACON does NOT resolve."
  - "No-flag dispatch: prompt on a TTY; default to project scope non-interactively."
metrics:
  duration: ~13m
  completed: 2026-05-22
  tasks: 2
  files: 4
---

# Quick Task 260522-tor: scope-aware `lacon init` Summary

Reworked `lacon init` to install lacon at a chosen **project** and/or **user** scope (`--project` / `--user`, both selectable), replacing the never-reaching embedded `<!-- lacon:start/end -->` CLAUDE.md marker block with a standalone `LACON.md` plus an empirically-verified, idempotent Claude Code `@import` reference line.

## What changed

- **`cli.rs`** — `Init` is now a struct variant carrying `#[arg(long)] user: bool` and `#[arg(long)] project: bool`.
- **`main.rs`** — dispatch passes both flags into `init::execute(user, project)`.
- **`init.rs`** — rewritten:
  - `Scope` enum + `ScopePaths` struct; `resolve_scope_paths()` computes each scope's four paths + import token up front; a single `install_scope(&paths)` runs the three install steps once per selected scope (no duplicated logic).
  - Project scope = cwd-relative (`.lacon/`, `./.claude/settings.json`, `./.claude/LACON.md`, `./CLAUDE.md`). User scope = home-relative via `etcetera::home_dir()` for the `~/.claude/*` triple + `etcetera::choose_base_strategy().config_dir()` for `~/.config/lacon/rules/` (mirrors `loader.rs`).
  - Reused `install_lacon_hook` (scrub-then-reinsert) and `atomic_write_json` (perm-preserving) untouched.
  - New `install_reference_line()` replaces the deleted marker-block fns; idempotent whole-line detection (trailing-whitespace tolerant), append-at-EOF with separation, byte-stable on re-run.
  - Removed `LACON_START`/`LACON_END`, `install_claude_md_block`, `strip_lacon_markers`, `append_fresh_block`, and their unit tests.
- **`cli_init.rs`** — rewritten hermetic, flag-driven contract tests (project, project-missing-CLAUDE.md warn+create, user with config preservation, both scopes, per-scope idempotency, perm preservation).

Marker-block identifier grep across `init.rs` + `cli_init.rs` returns **0**.

## (1) Selection-UX measurement and decision (D-ux)

The interactive scope prompt is only reached on a TTY with no flags. I measured the cost of adding a TUI select crate (`dialoguer 0.11`) before deciding.

| Metric | Baseline (no dialoguer) | dialoguer **declared but unused** | dialoguer **actually linked** |
|---|---|---|---|
| `target/release/lacon` size | 7,690,352 B | 7,690,352 B (identical — dead-stripped) | 7,718,416 B (**+28,064 B / +0.36%**) |
| `lacon --version` startup (min-of-15) | ~4647 µs | ~4715 µs | ~4789 µs (within shell-timing noise; prompt is not on the `lacon run` hot path) |
| Transitive crates pulled in | — | `console`, `shell-words`, `zerocopy`, extra `unicode-width` | same |

Method note: a declared-but-unused dep is dead-stripped by the release linker (0-byte delta), so I forced a real `dialoguer::Select` call site reachable from `main` to get the **true** linked size (+28 KB). The deterministic `tracker_open` gate is unaffected (the prompt never touches `Tracker::open`), and startup deltas are inside shell `date` measurement noise.

**Decision: REJECT dialoguer; use a plain `std::io::stdin().read_line` prompt.** The +28 KB and four new transitive crates buy nothing for a one-of-three (`p`/`u`/`b`) selection a one-line read covers, and CLAUDE.md emphasizes cold-start discipline and a minimal dependency surface. dialoguer was fully reverted from `Cargo.toml` **and** `Cargo.lock` (both grep to 0). The final dialoguer-free release binary is 7,700,432 B — the ~10 KB over baseline is the new `init.rs` logic, not a crate.

## (2) Verified `@import` token per scope (D-import)

A live `claude` 2.1.148 binary was available, so I verified empirically (not just from the spec). Method: append a candidate `@import` line to a CLAUDE.md that `claude -p` is confirmed to load (this trusted repo's root), place a `LACON.md` carrying a unique sentinel at the import's resolution target, then ask `claude -p` to echo the sentinel — the sentinel only appears if the import resolved.

| Import form | In file | Resolves to | Result |
|---|---|---|---|
| `@LACON` (extensionless) | `./CLAUDE.md` | — | **DOES NOT resolve** (token left literal; no LACON.md content loaded — the model even flagged the unresolved `@LACON`) |
| `@LACON.md` (extensioned, same dir) | `~/.claude/CLAUDE.md` | `~/.claude/LACON.md` | **RESOLVES** (sentinel loaded) |
| `@.claude/LACON.md` (relative subpath) | `./CLAUDE.md` (repo root) | `./.claude/LACON.md` | **RESOLVES** (sentinel loaded) |

Control: `claude -p` only loads CLAUDE.md from a trusted project root (a fresh `/tmp` dir loads nothing), which is why the probe was run inside this repo with the candidate import temporarily appended and then restored byte-for-byte (verified via `cmp`).

**Tokens written per scope (the verified resolvable forms):**
- **user** (`~/.claude/CLAUDE.md` → `~/.claude/LACON.md`): `@LACON.md`
- **project** (`./CLAUDE.md` → `./.claude/LACON.md`): `@.claude/LACON.md`

`@LACON` was rejected because it does not resolve. A unit test (`project_and_user_import_tokens_are_the_verified_resolvable_forms`) guards against regressing back to `@LACON`.

## (3) No-flag scope dispatch behavior

- **No flag + stdin is a TTY** → interactive prompt: `Choice [p/u/b] (default p)`. Empty/`p` → project, `u` → user, `b` → both; unrecognized input → `Ok(1)` with a stderr message. (Never reached in tests.)
- **No flag + stdin NOT a TTY** (CI / scripted / hermetic tests) → never blocks; prints a stderr note and defaults to **project** scope.
- Either/both flags → install that/those scope(s) directly, regardless of TTY.

## Deviations from Plan

### Out-of-scope changes reverted (scope discipline)
Running `cargo fmt` (mandated by CLAUDE.md) reformatted 55 unrelated workspace files that were not fmt-clean at baseline. Per the executor scope boundary, all 55 were reverted to baseline; cli.rs/main.rs were restored and only my logical edits re-applied (the pre-existing `Explain {}` and `doctor.rs` rustfmt churn was confirmed already non-conformant at HEAD and left untouched). Only the four task files were committed. Logged here rather than fixed.

Otherwise: plan executed as written.

## Tests / Verification

- `cargo build --workspace` (debug, materializes assert_cmd helper bins) — succeeds.
- `cargo test -p lacon-cli --test cli_init` — **8 passed**.
- `cargo test -p lacon-cli` (full) — 25 unit + all integration suites pass; init unit tests **11 passed** (no marker-block tests remain).
- Marker-block grep across `init.rs` + `cli_init.rs` — **0**.
- `cargo clippy -p lacon-cli --bins` — clean (no lacon-cli warnings).
- `rustfmt --check` on the modified source/test files — clean.
- dialoguer absent from `Cargo.toml` and `Cargo.lock`.

## Self-Check: PASSED

- Files: all 4 modified files present (init.rs, cli.rs, main.rs, cli_init.rs).
- Commits: `4697099` (feat, Task 1), `718a6ee` (test, Task 2) both present in git log.
- No file deletions across either task commit.
