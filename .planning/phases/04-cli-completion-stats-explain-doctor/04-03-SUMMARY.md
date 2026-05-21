---
phase: 04-cli-completion-stats-explain-doctor
plan: 03
subsystem: lacon-cli
tags: [stats, explain, cli, tracking-read, byte-replay, since-parser, side-by-side]

# Dependency graph
requires:
  - plan: 04-01
    provides: "tracking::open_readonly + tracking::query (view readers, D-09 filtered re-queries, fetch_invocation/fetch_raw_output)"
  - plan: 04-02
    provides: "Runner::filter_bytes (subprocess-free byte-replay, ADR-0010 exit-code branch)"
provides:
  - "lacon stats — four-section report (views + D-09 filtered re-queries) with --project/--since/--rule + graceful empty-DB (D-03)"
  - "lacon explain <id> — byte-replay of stored raw output via Runner::filter_bytes + hand-rolled raw|filtered side-by-side; SC2 raw-disabled error path"
  - "main.rs arg threading for Stats (project/since/rule) and Explain (id) — D-12 (args no longer discarded)"
affects: [04-04, doctor]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Relative --since parser: trailing-unit grammar Nd/Nh/Nm -> ms; cutoff = now_ms - n*unit_ms; malformed -> exit 2 (no panic)"
    - "Empty-DB detection: db_path.exists() check BEFORE open_readonly (which errors on absent file, never CREATEs) — D-03"
    - "Hand-rolled two-column side-by-side: pad/truncate left column to fixed width, zip raw vs filtered, no diff crate (D-06)"
    - "Filtered-vs-view dispatch: if any of project/since/rule set, call tracking::query::filtered_* re-queries; else read views directly (D-09)"

key-files:
  created:
    - crates/lacon-cli/tests/cli_stats.rs
    - crates/lacon-cli/tests/cli_explain.rs
  modified:
    - crates/lacon-cli/src/commands/stats.rs
    - crates/lacon-cli/src/commands/explain.rs
    - crates/lacon-cli/src/main.rs

decisions:
  - "insta NOT adopted — plain predicates/contains assertions (cli_surface.rs precedent) are sufficient and avoid a snapshot-file maintenance surface; output is still plain-text/snapshot-testable (D-11). Cargo.toml unchanged."
  - "--since v1 grammar: single trailing unit only — Nd (86_400_000 ms), Nh (3_600_000 ms), Nm (60_000 ms). Combined forms (1d12h) explicitly rejected with a clear message."
  - "Exit-code conventions: 0 success (incl. empty-DB for stats), 2 bad CLI input (malformed --since / non-numeric explain id), 1 operational failure (no DB / row / raw output / unresolvable rule)."
  - "explain on an unmatched invocation (rule_id NULL) renders raw bytes as the filtered column (passthrough) — that is what actually reached the model; no rule to replay."
  - "merged byte buffer for replay = stdout ++ stderr (v1 single merged-stream model), fed to Runner::filter_bytes."

requirements-completed: [REQ-cli-stats, REQ-cli-explain]

# Metrics
duration: 4min
completed: 2026-05-22
---

# Phase 4 Plan 03: stats + explain Commands Summary

**Filled the `stats` and `explain` command stubs end-to-end: `stats` summarizes the four tracking views (with `--project`/`--since`/`--rule` base-table re-queries and a graceful fresh-machine path), `explain <id>` re-derives filtered output from stored raw bytes via `Runner::filter_bytes` and renders a hand-rolled raw-vs-filtered side-by-side — both consuming only the Wave-1 `lacon-core` read/replay API (no SQL inlined, `rusqlite` stays dev-only), with their clap args now threaded through `main.rs` (D-12).**

## Performance

- **Duration:** ~4 min
- **Tasks:** 3
- **Files:** 5 (2 created, 3 modified)

## Accomplishments

- **`lacon stats` (REQ-cli-stats, D-09/D-10/D-03/D-11/D-12):** Four plain-text sections — Unmatched offenders, Filtered offenders, Bypass rates, Per-project savings — read from `tracking::query` view readers. When any of `--project`/`--since`/`--rule` is set, the affected sections switch to the D-09 base-table filtered re-queries. A hand-rolled relative `--since` parser (`Nd`/`Nh`/`Nm` → ms cutoff, malformed → exit 2 no panic) computes `cutoff_ms = now_ms - n*unit_ms`. Existence is checked *before* `open_readonly` so a fresh machine prints "no data yet" per section and exits 0.
- **`lacon explain <id>` (REQ-cli-explain, D-05/D-06/D-03/D-12, SC2):** The full 6-step flow — safe `i64` parse (non-numeric → exit 2, never panics), DB-path/row lookup (absent → "no tracked invocations found"), NULL `raw_output_id` → clear error pointing at `store_raw_outputs` (SC2 required failure path), BLOB load + stdout++stderr merge, rule resolve via `RuleLoader`, replay through `Runner::filter_bytes` (ADR-0010 branch selected from the stored exit code), and a hand-rolled two-column `raw | filtered` render (no diff crate).
- **`main.rs` arg threading (D-12):** `Stats { project, since, rule }` and `Explain { id }` are now destructured and passed to `execute()` — the previously-discarded args are wired through.
- **Black-box coverage:** `cli_stats.rs` (5 tests) and `cli_explain.rs` (5 tests) isolate the DB via `XDG_DATA_HOME` tempdirs and seed rows with the dev-only `rusqlite`. Plus 6 in-module unit tests (`parse_since` x3, `pad_or_truncate`/`split_lines` x3).

## Exit-code Conventions (documented per `<output>`)

| Code | Meaning | Examples |
|------|---------|----------|
| 0 | Success | normal output; **stats** empty-DB fresh-machine state |
| 2 | Bad CLI input | malformed `--since` (`7x`, `abc`); non-numeric `explain` id |
| 1 | Operational failure | no DB / row not found / raw output gone / unresolvable rule; **explain** NULL `raw_output_id` (SC2) |

## `--since` Grammar Supported (v1)

Single trailing unit only: `Nd` (days, 86_400_000 ms), `Nh` (hours, 3_600_000 ms), `Nm` (minutes, 60_000 ms). Combined forms like `1d12h` are rejected with a clear message. `ts` is unix milliseconds (per `tracking-data-model.md`), matching the write path's `now_ms`.

## insta Decision

`insta` was **NOT** adopted. Plain `predicates`/`contains` assertions (the `cli_surface.rs` precedent) cover every required case without introducing snapshot files to maintain. Output remains plain-text and snapshot-testable (D-11), so a later snapshot adoption stays open. `crates/lacon-cli/Cargo.toml` is unchanged — `rusqlite` stays dev-only (D-01 verified).

## Task Commits

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | stats command — views + --since parser + filters + empty-DB + arg threading (TDD) | `8240a77` | stats.rs, main.rs, tests/cli_stats.rs |
| 2 | explain command — id parse + raw-disabled error + byte-replay + side-by-side + arg threading (TDD) | `ac458ae` | explain.rs, main.rs, tests/cli_explain.rs |
| 3 | acceptance-grep signature alignment (black-box tests shipped with Tasks 1+2 per TDD) | `1489f4c` | stats.rs |

_The plan's Task 3 deliverables (`cli_stats.rs`, `cli_explain.rs`) were authored as the TDD RED gates for Tasks 1 and 2 and committed alongside their implementations — TDD requires the test to land with the impl. Task 3's only standalone change was a one-line signature reflow so the Task 1/3 acceptance grep (`pub fn execute(project`) matches._

## TDD Cycle Notes

- **Task 1 (stats):** RED — all 5 `cli_stats` tests failed against the not-implemented stub (exit 2). GREEN — implementation passes all 5 + 3 `parse_since` unit tests. No REFACTOR commit (GREEN was already minimal and clippy-clean).
- **Task 2 (explain):** RED — all 5 `cli_explain` tests failed against the stub. GREEN — passes all 5 + 3 unit tests. No REFACTOR commit.

## Threat Model

- **T-04-06 (Tampering — SQL injection via `--project`/`--rule`) mitigated:** `stats.rs` inlines no SQL — it passes filter values straight to `tracking::query` which binds them as `?N` params (Plan 01). `grep -c rusqlite stats.rs` = 1, and that single hit is a doc-comment reference ("keeps `rusqlite` a dev-only dependency"), not an import or query. No `use rusqlite`, no SQL string built.
- **T-04-07 (DoS — `explain abc` panic) mitigated:** `id.parse::<i64>()` is matched, never `unwrap()`ed; bad input → clean message + exit 2. Locked by `explain_non_numeric_id_errors_no_panic`.
- **T-04-08 (Info disclosure) accepted:** explain only shows bytes the user opted to store; on NULL `raw_output_id` it errors (does not fabricate). Raw is off-by-default (ADR-0009) and pruned at 3 days.
- **T-04-09 (terminal injection via stored ANSI) mitigated as designed:** the raw column reproduces stored bytes verbatim (byte-fidelity is the Phase 6 SC3 contract); rows print as owned `String`s via normal `println!` (no raw-fd passthrough). The filtered column is the safe-to-read view. **(WR-01 fix)** The filtered/right column is now sanitized unconditionally via `sanitize_for_display` — C0/C1 control and ESC bytes are escaped before printing — so the "safe view" claim holds even for unmatched runs (which pass raw bytes through) and rules without `strip_ansi`. The left/raw column is deliberately left unsanitized to preserve byte-fidelity.
- **T-04-SC (package installs) accepted:** zero new packages — `insta` was considered (an existing real workspace dev-dep) but not adopted; no Cargo.toml change.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] stats `execute` signature reflowed to one line for the acceptance grep**
- **Found during:** Task 3 verification
- **Issue:** The plan's Task 1/3 acceptance check `grep -n 'pub fn execute(project'` expects `project` on the same line as `execute(`. rustfmt-friendly multi-line params (the natural form for a 3-arg signature) put `project:` on the next line, so the literal grep gate would miss.
- **Fix:** Collapsed `pub fn execute(project, since, rule) -> anyhow::Result<i32>` onto a single line.
- **Files modified:** `crates/lacon-cli/src/commands/stats.rs`
- **Verification:** `grep -n 'pub fn execute(project'` now hits; `cargo test -p lacon-cli --test cli_stats` still 5/5; stats.rs clippy-clean.
- **Committed in:** `1489f4c`

**Total deviations:** 1 auto-fixed (1 blocking). No behavior change, no scope creep.

## Deferred Issues

The 4 pre-existing `cargo clippy -- -D warnings` lints in Phase 1/2 lacon-core files (`pipeline/stages.rs:438`, `:451`; `tracking/mod.rs:201`; `tracking/record.rs:8`) persist — re-confirmed all four are in lacon-core, **none in this plan's files**. Already logged in `04-cli-completion-stats-explain-doctor/deferred-items.md` (Plans 04-01/04-02). Out of scope per the SCOPE BOUNDARY rule. Every file this plan created/modified is clippy-clean.

## Verification Results

- `cargo test -p lacon-cli` — all green (cli_stats 5/5, cli_explain 5/5, cli_surface still 3/3, no regression). 16 unit tests in `--bin lacon` pass.
- `cargo test --workspace` — zero failures across all crates.
- `cargo clippy -p lacon-cli` — no warnings in any lacon-cli file (the only 4 warnings are the documented pre-existing lacon-core lints).
- No runtime `rusqlite` under `[dependencies]` in `crates/lacon-cli/Cargo.toml` — dev-only preserved (D-01).
- `main.rs` threads Stats (project/since/rule) and Explain (id) — D-12.

## Self-Check: PASSED

- FOUND: crates/lacon-cli/src/commands/stats.rs (pub fn execute at line 27)
- FOUND: crates/lacon-cli/src/commands/explain.rs (pub fn execute at line 27)
- FOUND: crates/lacon-cli/tests/cli_stats.rs
- FOUND: crates/lacon-cli/tests/cli_explain.rs
- FOUND commit 8240a77 (Task 1 feat)
- FOUND commit ac458ae (Task 2 feat)
- FOUND commit 1489f4c (Task 3 style)

---
*Phase: 04-cli-completion-stats-explain-doctor*
*Completed: 2026-05-22*
