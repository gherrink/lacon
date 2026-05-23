# Phase 8: redesign-lacon-stats-output-for-readability-adr-0014 - Context

**Gathered:** 2026-05-23 (assumptions mode)
**Status:** Ready for planning

<domain>
## Phase Boundary

Make `lacon stats` readable at real-world history sizes by adding a **read-time
presentation layer**, per ADR 0014. In scope: an overall savings headline,
project canonicalization + rollup (a single `(ephemeral)` temp-dir bucket;
worktree/subdir → repo root via read-time `.git` resolution), top-N capping per
section with an `--all` escape, decimal-SI byte humanization with a `--bytes`
escape, and clarified section/column labels.

**Out of scope (hard boundary):** the stored data model, the four SQL views, and
the write hot path are **unchanged** — no schema migration is introduced. All new
behavior is confined to `lacon-cli` plus exactly one new aggregate reader behind
the `lacon-core::tracking::query` boundary. This is a presentation change, not a
data-model change.
</domain>

<decisions>
## Implementation Decisions

### Scope & data model (locked by ADR 0014)
- **D-01:** Read-time presentation only. The write path still records the literal
  logical `current_dir()` as `project_path`; the four views (`v_unmatched_offenders`,
  `v_filtered_offenders`, `v_bypass_rate`, `v_project_savings`) are untouched; **no
  migration** is added in this phase.
- **D-02:** All new logic lives in `lacon-cli` (`commands/stats.rs` + private
  helpers). Exactly **one** new SQL aggregate is added behind the
  `lacon-core::tracking::query` boundary — `query::overall_totals(conn)` plus a
  `--since`/`--project`-filtered counterpart. `lacon-cli` keeps `rusqlite` dev-only
  and never inlines a query (prior D-01).
- **D-03:** `stats` stays read-only — opens via `tracking::open_readonly` (prior
  D-02), gated on `db_path.exists()` so a fresh machine still prints "no data yet"
  and exits 0 (prior D-03). Exit-code contracts preserved (0 success; 2 on
  malformed `--since`). New `.git`/temp logic is pure path/file handling with a
  literal-path fallback on any I/O error.

### Helper code placement
- **D-04:** Presentation helpers (`humanize_bytes`, project canonicalization +
  `.git` resolution, top-N capping) are **private `fn`s inside
  `commands/stats.rs`**, unit-tested via the existing inline `#[cfg(test)] mod
  tests`. Not a new shared util module, not `lacon-core`. Matches the one-module-
  per-command convention (`explain.rs` keeps `render_side_by_side`/`sanitize_for_display`
  inline; `stats.rs` already keeps `normalize_project`/`parse_since` inline).

### Overall headline
- **D-05:** Print an overall summary line **first**, before the sections: total
  runs, distinct projects (after canonicalization), `raw → kept` bytes, and `saved`
  (absolute + percent), computed over `bypassed = 0` rows. Backed by the new
  `query::overall_totals` reader (and its `--since`/`--project`-filtered counterpart).

### Project canonicalization + rollup
- **D-06:** The "Savings by project" section reads the existing per-`project_path`
  rows and **re-aggregates them in Rust under a canonical key**. Re-aggregation is
  exact because every project-savings field is an additive sum (runs, raw, filtered,
  saved). Top-N capping is applied **after** rollup.
- **D-07:** Canonical-key precedence, in order: **(a) ephemeral** → **(b) repo root
  via `.git` resolution** → **(c) literal fallback**. Ephemeral takes precedence so
  a throwaway repo created under a temp root still collapses into `(ephemeral)`.
- **D-08:** **Ephemeral detection** uses component-wise `std::path::Path::starts_with`
  (NOT `str::starts_with`, which would false-match `/tmpfoo`) against a runtime-built
  prefix set: `/tmp`, `/var/folders`, `/private/var/folders`, `std::env::temp_dir()`,
  and `$TMPDIR` (when set). Match against the **stored string** — do NOT
  `canonicalize` (ephemeral paths are frequently already deleted; `current_dir()`
  stored the logical cwd, so on macOS both `/var/folders` and `/private/var/folders`
  spellings can appear and both must be matched). All such paths collapse into the
  single synthetic bucket `(ephemeral)`.
- **D-09:** **`.git` resolution** is a bounded sequence of file reads — no `git`
  subprocess. Walk the path's ancestors for `.git`:
  - `.git` is a **directory** → that ancestor is the repo root (normal repos + runs
    from a subdirectory).
  - `.git` is a **file** → parse `gitdir: <path>`: strip the `gitdir: ` prefix and
    `trim_end()`. **The path may be relative** (git submodules write a relative
    gitdir; `git worktree` writes absolute) — resolve a relative value against the
    gitfile's own directory. Then read `<gitdir>/commondir` (conventionally `../..`,
    relative to that admin gitdir; an absolute value is also legal) to locate the
    main `.git` directory; the repo root is the **parent of the main `.git`**.
- **D-10:** **Robustness / literal fallback:** a bare repo (`core.bare = true` in
  `<gitdir>/config`) has no working tree → literal fallback. Any I/O error, missing
  `.git`, or a recorded directory that no longer exists on disk → literal
  `project_path`, unchanged. Behavior never regresses below the pre-change exact
  path. `core.worktree` / `GIT_WORK_TREE` overrides are **not** honored in v1 —
  parent-of-`.git` is a documented best-effort heuristic.

### Top-N capping & ordering
- **D-11:** Each section prints at most **N = 10** rows, ordered by its primary
  metric (unmatched: `total_raw_bytes`; filtered: `total_filtered_bytes`; bypass:
  `bypass_rate`; project: `bytes_saved`/`saved %`), followed by a `… M more` line
  with a drill-in hint. The **project section re-sorts in Rust after the rollup**
  (canonical-key re-aggregation destroys the DB's `bytes_saved DESC` order); the
  other sections preserve their existing `ORDER BY … DESC`.
- **D-12:** Ship the **`--all`** flag now (`#[arg(long)]` bool) → prints every row
  uncapped and suppresses the `… M more` line. The overflow hint lists
  `--project` / `--rule` / `--since` / `--all`. (`cli_surface.rs` caps *subcommands*,
  not flags, so this is safe.)

### Columns & labels
- **D-13:** **Decimal-SI byte humanization:** `KB`/`MB`/`GB` (1000-based), **1
  decimal place** above 1 KB, raw integer bytes below 1 KB (e.g. `512 B`). Matches
  the ADR's literal `22.8 KB` example. Single `humanize_bytes(i64) -> String`
  helper (none exists today). *(User-confirmed: decimal SI over binary `KiB`.)*
- **D-14:** Ship the **`--bytes`** flag now (`#[arg(long)]` bool) → prints exact
  integer byte counts everywhere a humanized count would appear (scripting escape).
- **D-15:** **Relabel per ADR §4** — replace the "offenders" jargon with task-
  oriented headers (e.g. "Commands with no rule", "Rule effectiveness"); name the
  surviving-bytes column `sent`/`kept` (not `filtered_bytes`); show effectiveness as
  `saved %` (higher is better) instead of the inverted `keep_ratio`. The **stored
  field names** (`filtered_bytes`, `avg_keep_ratio`) and the **view definitions are
  NOT renamed** — the change is confined to CLI presentation.

### Tests
- **D-16:** `cli_stats.rs` gets **targeted edits, not a rewrite** (assertions are
  substring/`contains`, not golden-file equality — that is what "snapshot" means in
  prior D-11). Update the four section-header `contains(...)` assertions to the new
  labels; add a test that seeds temp-dir + linked-worktree + multi-path rows to
  verify the `(ephemeral)` bucket, `.git` rollup, top-N cap + `… M more`, and
  `--all` uncapping. Add inline unit tests for `humanize_bytes` and the
  canonicalization helpers. Column-token relabeling is low-risk (no test asserts
  `filtered_bytes`/`keep_ratio` literals).

### Claude's Discretion
- Exact final wording of the section headers and the column-header row (within the
  ADR §4 framing — the ADR gives examples, not verbatim final strings for all four).
- Whether to also include `/dev/shm` and `/run/user/<uid>` (Linux tmpfs) in the
  ephemeral prefix set — optional; the ADR-listed set is the floor.
- Internal signatures / struct shape for `query::overall_totals` and the canonical-
  key helper.
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

- `docs/decisions/0014-stats-read-time-presentation.md` — the governing ADR; this
  phase is its implementation. Read in full.
- `docs/decisions/0011-sqlite-for-local-tracking.md` — the four views, append-only
  migration constraint, no-daemon/cold-start framing.
- `docs/specs/tracking-data-model.md` — full schema, indexes, the four view
  definitions, retention policies (relabeling must be reflected wherever the `stats`
  output is described; field/view names are NOT renamed).
- `crates/lacon-cli/src/commands/stats.rs` — the command being redesigned (4
  sections; existing `normalize_project`/`parse_since`/`print_empty` helpers).
- `crates/lacon-core/src/tracking/query.rs` — the read API; `query::overall_totals`
  is added here behind the D-01 boundary.
- `crates/lacon-cli/tests/cli_stats.rs` — black-box tests (substring assertions;
  seeds DB via dev-only `rusqlite` + `SCHEMA_DDL`).
- `crates/lacon-cli/src/cli.rs` — clap `Stats { project, since, rule }`; `--bytes`
  and `--all` flags are added here.
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- **`lacon_core::tracking::query`** already exposes typed view readers
  (`project_savings`, `unmatched_offenders`, `filtered_offenders`, `bypass_rate`)
  and base-table filtered re-queries (`filtered_*`, prior D-09). The project rollup
  re-aggregates the existing `ProjectSaving` rows in Rust — no new project SQL needed
  beyond `overall_totals`.
- **`tracking::open_readonly`** (prior D-02) is the read-only WAL open; reuse as-is.
- **`stats.rs` helper + inline-test pattern** is the template for the new helpers
  (`normalize_project`/`parse_since` already there; `explain.rs`/`doctor.rs` confirm
  the convention).
- **CLI bool-flag shape** for `--bytes`/`--all`: copy `init`'s `#[arg(long)] user:
  bool` (cli.rs).

### Established Patterns
- **D-01 SQL boundary:** all SQL behind `lacon-core::tracking::query`; `lacon-cli`
  keeps `rusqlite` dev-only. The new `overall_totals` reader honors this.
- **Cold-start budget is load-bearing** but applies to the *write* hot path
  (`lacon run`); canonicalization is read-only and stats-only, so it is exempt — but
  must never be reachable from the write path (reinforces D-04 placement).
- **Append-only migrations** (ADR 0011): views in migration `0001` are immutable;
  this phase adds none.
- **Test isolation:** CLI tests point the binary at a tempdir via `XDG_DATA_HOME`
  and seed the DB with the dev-only `rusqlite` + `SCHEMA_DDL` constant.

### Integration Points
- `cli.rs` `Stats { … }` variant + `main.rs` dispatch (`commands::stats::execute(...)`)
  — extend the signature with `bytes: bool` and `all: bool`.
- `commands/stats.rs::execute` — restructure output (headline → relabeled, capped,
  humanized sections); add private helpers.
- `query.rs` — add `overall_totals` (+ filtered counterpart) and its typed row.
- `cli_stats.rs` — update header assertions + add canonicalization/cap/`--all` tests.

### Research-firmed facts (git on-disk format, verified against git 2.53)
- `.git` **file** = `gitdir: <path>\n`; path is **absolute for `git worktree`** but
  **relative for submodules** — must handle both (relative → resolve against the
  gitfile dir).
- `commondir` = path **relative to the admin gitdir** (conventionally `../..`);
  absolute is legal. `parent(main .git)` is the working-tree root for the normal
  non-bare case.
- `Path::starts_with` is component-wise (avoids the `/tmpfoo` false positive);
  `current_dir()` stores the **logical** cwd, so macOS may store either
  `/var/folders` or `/private/var/folders`.
</code_context>

<specifics>
## Specific Ideas

- ADR §4 byte example is `22.8 KB` → decimal SI, one decimal place (D-13, user-confirmed).
- Ephemeral bucket label is the literal string `(ephemeral)` (ADR §2a).
- Overflow line shape: `… M more (use --project / --rule / --since / --all to drill in)`.
</specifics>

<deferred>
## Deferred Ideas

- **`repo_root` column on the write path** — a future append-only migration could
  compute repo identity at write time (so project identity survives deleted dirs and
  appears in SQL grouping). The ADR explicitly defers this to avoid hot-path cost;
  not in this phase.
- **`core.worktree` / `GIT_WORK_TREE` honoring** — v1 uses best-effort parent-of-
  `.git`; honoring these overrides is deferred (documented caveat instead).
- **`/var/tmp` as ephemeral** — not boot-ephemeral; deliberately not matched.

### Reviewed Todos (not folded)
None — `todo.match-phase 08` returned zero matches.
</deferred>
