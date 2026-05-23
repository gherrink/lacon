---
phase: 08-redesign-lacon-stats-output-for-readability-adr-0014
plan: 03
subsystem: lacon-cli stats presentation (wiring)
tags: [stats, presentation, headline, rollup, top-n-cap, humanize, relabel, adr-0014]
requires:
  - "08-01 (query::overall_totals / filtered_overall_totals headline reader)"
  - "08-02 (humanize_bytes / canonical_project_key / resolve_repo_root / is_ephemeral helpers)"
provides:
  - "lacon stats --bytes flag (exact integers, D-14)"
  - "lacon stats --all flag (uncap every section, D-12)"
  - "headline-first stats output (runs, canonical project count, raw→kept, saved abs+%, D-05)"
  - "Rust-side project rollup under canonical_project_key + re-sort DESC (D-06)"
  - "top-N=10 cap + '… M more' drill-in hint on all four sections (D-11)"
  - "relabeled task-oriented section headers + columns (D-15)"
affects:
  - "end users of `lacon stats` (the user-visible ADR 0014 deliverable)"
tech-stack:
  added: []
  patterns:
    - "render closure (|n| if bytes { n.to_string() } else { humanize_bytes(n) }) at every byte site"
    - "HashMap<String, accumulator> rollup keyed by canonical_project_key, then sort_by_key(Reverse)"
    - "generic print_capped<T>(rows, all, row_fmt) helper: take(limit) + conditional '… M more'"
    - "per-section reader Err → eprintln + Ok(1) (NOT ?), reused for the headline read"
key-files:
  created: []
  modified:
    - "crates/lacon-cli/src/cli.rs (+ --bytes / --all bool flags on Stats)"
    - "crates/lacon-cli/src/main.rs (5-arg Stats dispatch)"
    - "crates/lacon-cli/src/commands/stats.rs (restructured execute + RolledSaving / rollup_project_savings / print_capped; removed 5 #[allow(dead_code)])"
    - "crates/lacon-cli/tests/cli_stats.rs (+6 black-box tests, 4 relabeled header assertions)"
decisions:
  - "D-05: overall headline printed FIRST over bypassed=0 rows; distinct-projects is rolled.len() (canonical), not OverallTotals.distinct_projects (pre-canonicalization)"
  - "D-06: project rollup re-aggregated Rust-side under canonical_project_key (additive sums exact), re-sorted bytes_saved DESC"
  - "D-11: TOP_N=10 cap + '… M more (use --project / --rule / --since / --all to drill in)' on all four sections"
  - "D-12: --all uncaps every section and suppresses the hint"
  - "D-14: --bytes prints exact integers via a single render closure"
  - "D-15: relabeled headers (Commands with no rule / Rule effectiveness / Bypass rates / Savings by project), columns kept + 'saved %'; NO struct-field or view rename (fence held)"
  - "D-16: targeted cli_stats.rs edits with substring contains() assertions, not golden-file equality"
  - "D-01/D-03: no migration/view/write-path change; empty-DB→exit 0 and bad --since→exit 2 contracts preserved"
metrics:
  duration: "~6 min"
  completed: "2026-05-23"
  tasks: 3
  files: 4
---

# Phase 8 Plan 03: Stats Read-Time Presentation Wiring Summary

Wired the ADR 0014 read-time presentation layer end-to-end — the user-visible
deliverable of the phase. `lacon stats` now prints an overall headline FIRST
(runs, canonical project count, `raw → kept` bytes, `saved` absolute + percent),
re-aggregates the project section in Rust under the canonical key (one
`(ephemeral)` line, worktrees/subdirs collapsed to their repo root) and re-sorts
by bytes saved DESC, caps every section at 10 rows with a `… M more` drill-in
hint, humanizes byte counts (`22.8 KB`) with a `--bytes` escape for scripting,
uncaps every section with `--all`, and uses relabeled task-oriented headers and
columns — all with no migration, no view edit, and no stored-field rename.

## What Was Built

Three tasks (the plan's `tdd="true"` tasks ran in plan order — implementation in
Task 1, black-box tests in Task 2 — because `config.json` has `tdd_mode: false`
and the plan front-loaded the restructure with the verbatim consumed interfaces):

**Task 1 — flags + restructured `execute`** (commit `395c696`):
- `cli.rs`: added `bytes: bool` and `all: bool` `#[arg(long)]` flags to the
  `Stats` variant with `///` help text (the `--help` strings). Copied Init's
  `user: bool` shape.
- `main.rs`: extended the Stats dispatch arm to
  `{ project, since, rule, bytes, all } => execute(project, since, rule, bytes, all)?`.
- `stats.rs::execute`: new 5-arg signature. The prologue (parse_since→cutoff_ms,
  normalize_project→project_ref, the `filtered` bool, `xdg_db_path` +
  `db_path.exists()`→`print_empty`+Ok(0), `open_readonly`) is REUSED UNCHANGED
  (no fresh `Connection` — that would re-introduce the WAL-on-readonly bug).
- **Headline (D-05)**: the project rollup runs FIRST so `rolled.len()` (the
  canonical map length) is the displayed distinct-projects count — Pitfall 7:
  `OverallTotals.distinct_projects` is pre-canonicalization and is deliberately
  NOT shown. The headline reads `overall_totals` vs `filtered_overall_totals` on
  the `filtered` bool with the per-section `Err → eprintln + Ok(1)` posture
  (NOT `?`, T-08-03). `saved %` is `bytes_saved * 100 / raw_total` guarding
  `raw_total == 0` (prints `—`).
- **Byte renderer (D-14)**: a single `render` closure
  (`if bytes { n.to_string() } else { humanize_bytes(n) }`) used at every byte
  site (headline + all four sections).
- **Project rollup (D-06)**: new `RolledSaving` struct + `rollup_project_savings`
  re-aggregate the per-`project_path` `ProjectSaving` rows into a
  `HashMap<String, RolledSaving>` keyed by `canonical_project_key`, summing every
  additive field (exact), collected and re-sorted by `bytes_saved` DESC via
  `sort_by_key(Reverse(..))`.
- **Top-N cap (D-11/D-12)**: a generic `print_capped<T>(rows, all, row_fmt)`
  helper prints up to `TOP_N = 10` rows and, unless `all`, appends
  `… {M} more (use --project / --rule / --since / --all to drill in)` when
  `len > 10`. Applied to all four sections.
- **Relabel (D-15)**: headers → "Commands with no rule" / "Rule effectiveness" /
  "Bypass rates" / "Savings by project"; surviving-bytes column → `kept`;
  effectiveness → `saved %` (`100 - keep_ratio*100`, higher is better) instead
  of the inverted `keep_ratio`. `print_empty`'s four headers relabeled to match
  (keeping the "no data yet" token). The five 08-02 helpers, now wired, had their
  `#[allow(dead_code)]` removed.

**Task 2 — six black-box tests + relabeled assertions** (commit `c59861a`):
Reusing `init_db`/`insert_invocation`/`lacon`, each test follows the
`tempdir()`→`init_db`→N×`insert_invocation`→`lacon(xdg).args([...])`→stdout
substring `contains()` shape (D-16):
- `stats_ephemeral_paths_collapse_to_one_bucket` — ≥3 `temp_dir()`-rooted paths →
  exactly ONE `(ephemeral)` line; individual temp paths absent.
- `stats_git_dir_and_subdir_roll_into_one_repo` — `<scratch>/repo/.git/` + a
  subdir → ONE repo-root line; the subdir path collapses.
- `stats_top_n_caps_project_section_with_more_hint` — 11 distinct projects →
  exactly 10 rows + a `more` hint mentioning `--all`.
- `stats_all_flag_uncaps_and_drops_more_hint` — same 11-project seed + `--all` →
  all 11 rows, no `more` line.
- `stats_bytes_flag_prints_exact_integers` — raw 22_800 → `22.8 KB` by default;
  `--bytes` → `22800` and NOT `KB`.
- `stats_headline_prints_first_with_runs_and_saved` — headline string index
  precedes the first section header, carries a runs count and a `%`; the seeded
  bypassed row is excluded (`2 runs` counted).
- The four section-header assertions in the existing seeded-DB test updated to
  the relabeled strings; empty-DB ("no data yet" + exit 0) and bad-`--since`
  (exit 2) contracts left intact.

**Task 3 — full hermetic gate + rustfmt conformance** (commit `5b93e71`):
Full `cargo build --workspace && cargo test --workspace && cargo clippy
--workspace --all-targets` all green. One clippy `unnecessary_sort_by` on the
rollup re-sort was fixed (→ `sort_by_key(Reverse(..))`). The four phase files
were rustfmt-conformed and confined (the workspace-wide pre-existing fmt drift
remains out of scope / deferred, as 08-02 established; CI does not gate on fmt).

## Verification Results

| Gate | Result |
|------|--------|
| `cargo build --workspace` | pass |
| `cargo test --workspace` | 44 test-result groups, 0 failures |
| `cargo test -p lacon-cli --test cli_stats` | 11 passed (5 prior + 6 new) |
| `cargo test -p lacon-cli --bins` | 35 passed (inline unit tests) |
| `cargo clippy --workspace --all-targets` (phase files) | clean (no warnings in cli.rs/main.rs/stats.rs/cli_stats.rs) |
| `cargo fmt --check` (phase files) | clean |
| `grep overall_totals stats.rs` | headline reads filtered/unfiltered on `filtered`, Err→Ok(1) |
| `grep canonical_project_key stats.rs` | called in `rollup_project_savings` |
| old jargon `filtered_bytes=`/`keep_ratio=` in output strings | 0 |
| migration / view DDL edited | none (D-15 fence held) |
| `total_filtered_bytes`/`avg_keep_ratio` struct-field rename | none (10 refs intact) |
| `git diff --name-only e8f892c..HEAD -- crates/` | exactly the 4 lacon-cli phase files |

Manual eyeball (seeded DB) confirmed: headline first with `4 runs across 2
projects` (rolled-up count, not the 3 raw distinct paths), two `/tmp/scratch*`
paths collapsed to one `(ephemeral)` line re-sorted after `/home/me/proj`,
relabeled headers + `kept`/`saved %` columns, `22.8 KB` default vs `22800`
under `--bytes`.

## Threat Mitigations Applied

- **T-08-02 (SQL injection)** — the headline passes `cutoff_ms: Option<i64>` /
  `project_ref: Option<&str>` to `filtered_overall_totals`, which binds via `?N`
  placeholders (08-01). No value interpolation; same posture as the sections.
- **T-08-03 (DoS / panic on empty filter)** — the headline read maps any reader
  `Err → eprintln + Ok(1)` (no `?`→anyhow leak); 08-01's `COALESCE(SUM,0)`
  returns a zeroed headline on an empty/filtered-empty match. The eyeball's
  view-missing case confirmed the mapped exit-1 path (no panic).
- **T-08-08 (unbounded output)** — `TOP_N = 10` cap + `… M more` hint bounds
  output regardless of history size; `--all` is the explicit opt-in to uncapped.
- **T-08-07 (terminal control bytes in project path)** — ACCEPTED per the plan:
  no regression vs. prior behavior (the pre-change code already printed
  `project_path` verbatim); local single-user trust model.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] `--lib` test target does not exist on a binary crate**
- **Found during:** Task 1 verification.
- **Issue:** the plan's `<verify>` uses `cargo test -p lacon-cli --lib`, but
  `lacon-cli` is a binary crate with no library target (`--lib` errors).
- **Fix:** ran the inline unit tests via `cargo test -p lacon-cli --bins` (the
  bin target carries the `#[cfg(test)]` tests) — same as 08-02's deviation.
- **Files modified:** none (invocation-only). **Commit:** n/a.

**2. [Rule 1 - Clippy] `unnecessary_sort_by` on the rollup re-sort**
- **Found during:** Task 3 clippy.
- **Issue:** `out.sort_by(|a,b| b.bytes_saved.cmp(&a.bytes_saved))` tripped
  `clippy::unnecessary_sort_by`.
- **Fix:** `out.sort_by_key(|r| std::cmp::Reverse(r.bytes_saved))` (clippy-clean
  DESC sort).
- **Files modified:** `crates/lacon-cli/src/commands/stats.rs`.
- **Commit:** `5b93e71`.

**3. [Rule 3 - Blocking] non-ephemeral test fixtures resolved to the lacon repo root**
- **Found during:** Task 2 first run (top-N cap + `--all` tests failed: 0/10 and
  0/11 rows shown).
- **Issue:** `CARGO_TARGET_TMPDIR` lives under the lacon repo (`target/`), which
  has a `.git`. `resolve_repo_root` walked up and collapsed all 11 distinct
  `projNN` paths into the single lacon repo-root key, so none of the seeded
  paths matched. (`/tmp`-rooted `tempdir()` would instead collapse to
  `(ephemeral)` — also wrong for these tests.)
- **Fix:** gave each `projNN` its OWN `.git/` directory so the ancestor walk
  returns each path's own root first → 11 distinct repo-root keys, location-
  independent. The git-rollup test was unaffected (the subdir's first `.git`
  ancestor is `repo/.git`).
- **Files modified:** `crates/lacon-cli/tests/cli_stats.rs`. **Commit:** `c59861a`.

### Scope-boundary note (no code impact)

`cargo fmt --check` fails workspace-wide on PRE-EXISTING drift across ~50 files
this plan never touched (lacon-core, adapter, benches, other tests) — the same
condition 08-02 logged to `deferred-items.md` (CI does not gate on fmt). Task 3
conformed only the four phase files via `rustfmt <files>`. That invocation also
re-ordered `commands/mod.rs`'s `pub mod` lines (reachable from the module tree);
since `mod.rs` is NOT a phase file, that reordering was reverted (`git checkout`)
to keep the diff confined to the four declared files — leaving the final diff at
exactly: `cli.rs`, `main.rs`, `commands/stats.rs`, `tests/cli_stats.rs`.

## Known Stubs

None. The presentation layer is fully wired and proven by 6 black-box tests +
the 17 inline helper unit tests. No TODO/FIXME/placeholder markers in any
modified file.

## TDD Gate Compliance

`tdd_mode: false` in `config.json`; the plan placed the implementation in Task 1
and the black-box tests in Task 2 (the consumed interfaces were given verbatim),
so execution followed plan order rather than a strict RED→GREEN per task. All
new black-box and inline tests pass against the implementation; the assertions
are non-vacuous (the headline test's bypassed row carries distinct bytes that
must be excluded; the ephemeral/cap tests count exact occurrences).

## Self-Check: PASSED

- `crates/lacon-cli/src/cli.rs` — FOUND (modified, `bytes: bool` + `all: bool`)
- `crates/lacon-cli/src/main.rs` — FOUND (modified, 5-arg Stats dispatch)
- `crates/lacon-cli/src/commands/stats.rs` — FOUND (modified, `overall_totals` +
  `canonical_project_key` + `rollup_project_savings` + `print_capped`)
- `crates/lacon-cli/tests/cli_stats.rs` — FOUND (modified, 6 new tests + 4
  relabeled assertions)
- Commit `395c696` (feat 08-03) — FOUND
- Commit `c59861a` (test 08-03) — FOUND
- Commit `5b93e71` (style 08-03) — FOUND
