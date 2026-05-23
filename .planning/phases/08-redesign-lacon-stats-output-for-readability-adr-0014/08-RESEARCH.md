# Phase 8: Redesign lacon stats output for readability (ADR 0014) - Research

**Researched:** 2026-05-23
**Domain:** Rust CLI presentation refactor (read-time layer over an unchanged SQLite data model)
**Confidence:** HIGH (the domain is settled by ADR 0014 + a thorough CONTEXT.md; all code anchors below were read verbatim this session)

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** Read-time presentation only. The write path still records the literal logical `current_dir()` as `project_path`; the four views (`v_unmatched_offenders`, `v_filtered_offenders`, `v_bypass_rate`, `v_project_savings`) are untouched; **no migration** is added in this phase.
- **D-02:** All new logic lives in `lacon-cli` (`commands/stats.rs` + private helpers). Exactly **one** new SQL aggregate is added behind the `lacon-core::tracking::query` boundary — `query::overall_totals(conn)` plus a `--since`/`--project`-filtered counterpart. `lacon-cli` keeps `rusqlite` dev-only and never inlines a query (prior D-01).
- **D-03:** `stats` stays read-only — opens via `tracking::open_readonly` (prior D-02), gated on `db_path.exists()` so a fresh machine still prints "no data yet" and exits 0 (prior D-03). Exit-code contracts preserved (0 success; 2 on malformed `--since`). New `.git`/temp logic is pure path/file handling with a literal-path fallback on any I/O error.
- **D-04:** Presentation helpers (`humanize_bytes`, project canonicalization + `.git` resolution, top-N capping) are **private `fn`s inside `commands/stats.rs`**, unit-tested via the existing inline `#[cfg(test)] mod tests`. Not a new shared util module, not `lacon-core`. Matches the one-module-per-command convention.
- **D-05:** Print an overall summary line **first**, before the sections: total runs, distinct projects (after canonicalization), `raw → kept` bytes, and `saved` (absolute + percent), computed over `bypassed = 0` rows. Backed by the new `query::overall_totals` reader (and its `--since`/`--project`-filtered counterpart).
- **D-06:** The "Savings by project" section reads the existing per-`project_path` rows and **re-aggregates them in Rust under a canonical key**. Re-aggregation is exact because every project-savings field is an additive sum (runs, raw, filtered, saved). Top-N capping is applied **after** rollup.
- **D-07:** Canonical-key precedence, in order: **(a) ephemeral** → **(b) repo root via `.git` resolution** → **(c) literal fallback**. Ephemeral takes precedence so a throwaway repo created under a temp root still collapses into `(ephemeral)`.
- **D-08:** **Ephemeral detection** uses component-wise `std::path::Path::starts_with` (NOT `str::starts_with`, which would false-match `/tmpfoo`) against a runtime-built prefix set: `/tmp`, `/var/folders`, `/private/var/folders`, `std::env::temp_dir()`, and `$TMPDIR` (when set). Match against the **stored string** — do NOT `canonicalize` (ephemeral paths are frequently already deleted; `current_dir()` stored the logical cwd, so on macOS both `/var/folders` and `/private/var/folders` spellings can appear and both must be matched). All such paths collapse into the single synthetic bucket `(ephemeral)`.
- **D-09:** **`.git` resolution** is a bounded sequence of file reads — no `git` subprocess. Walk the path's ancestors for `.git`:
  - `.git` is a **directory** → that ancestor is the repo root (normal repos + runs from a subdirectory).
  - `.git` is a **file** → parse `gitdir: <path>`: strip the `gitdir: ` prefix and `trim_end()`. **The path may be relative** (git submodules write a relative gitdir; `git worktree` writes absolute) — resolve a relative value against the gitfile's own directory. Then read `<gitdir>/commondir` (conventionally `../..`, relative to that admin gitdir; an absolute value is also legal) to locate the main `.git` directory; the repo root is the **parent of the main `.git`**.
- **D-10:** **Robustness / literal fallback:** a bare repo (`core.bare = true` in `<gitdir>/config`) has no working tree → literal fallback. Any I/O error, missing `.git`, or a recorded directory that no longer exists on disk → literal `project_path`, unchanged. Behavior never regresses below the pre-change exact path. `core.worktree` / `GIT_WORK_TREE` overrides are **not** honored in v1 — parent-of-`.git` is a documented best-effort heuristic.
- **D-11:** Each section prints at most **N = 10** rows, ordered by its primary metric (unmatched: `total_raw_bytes`; filtered: `total_filtered_bytes`; bypass: `bypass_rate`; project: `bytes_saved`/`saved %`), followed by a `… M more` line with a drill-in hint. The **project section re-sorts in Rust after the rollup**; the other sections preserve their existing `ORDER BY … DESC`.
- **D-12:** Ship the **`--all`** flag now (`#[arg(long)]` bool) → prints every row uncapped and suppresses the `… M more` line. The overflow hint lists `--project` / `--rule` / `--since` / `--all`. (`cli_surface.rs` caps *subcommands*, not flags, so this is safe.)
- **D-13:** **Decimal-SI byte humanization:** `KB`/`MB`/`GB` (1000-based), **1 decimal place** above 1 KB, raw integer bytes below 1 KB (e.g. `512 B`). Matches the ADR's literal `22.8 KB` example. Single `humanize_bytes(i64) -> String` helper (none exists today).
- **D-14:** Ship the **`--bytes`** flag now (`#[arg(long)]` bool) → prints exact integer byte counts everywhere a humanized count would appear (scripting escape).
- **D-15:** **Relabel per ADR §4** — replace the "offenders" jargon with task-oriented headers (e.g. "Commands with no rule", "Rule effectiveness"); name the surviving-bytes column `sent`/`kept` (not `filtered_bytes`); show effectiveness as `saved %` (higher is better) instead of the inverted `keep_ratio`. The **stored field names** (`filtered_bytes`, `avg_keep_ratio`) and the **view definitions are NOT renamed** — the change is confined to CLI presentation.
- **D-16:** `cli_stats.rs` gets **targeted edits, not a rewrite** (assertions are substring/`contains`). Update the four section-header `contains(...)` assertions to the new labels; add a test that seeds temp-dir + linked-worktree + multi-path rows to verify the `(ephemeral)` bucket, `.git` rollup, top-N cap + `… M more`, and `--all` uncapping. Add inline unit tests for `humanize_bytes` and the canonicalization helpers. Column-token relabeling is low-risk.

### Claude's Discretion
- Exact final wording of the section headers and the column-header row (within the ADR §4 framing).
- Whether to also include `/dev/shm` and `/run/user/<uid>` (Linux tmpfs) in the ephemeral prefix set — optional; the ADR-listed set is the floor.
- Internal signatures / struct shape for `query::overall_totals` and the canonical-key helper.

### Deferred Ideas (OUT OF SCOPE)
- **`repo_root` column on the write path** — a future append-only migration; not in this phase.
- **`core.worktree` / `GIT_WORK_TREE` honoring** — v1 uses best-effort parent-of-`.git`.
- **`/var/tmp` as ephemeral** — not boot-ephemeral; deliberately not matched.
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| ADR 0014 | Add a read-time presentation layer to `lacon stats`: overall headline, project canonicalization + rollup (`(ephemeral)` bucket, worktree/subdir → repo root via `.git` resolution), top-N capping with `--all` escape, decimal-SI byte humanization with `--bytes` escape, clarified labels. Stored model, four views, write path unchanged — no migration. | Concrete code anchors (every touched function read verbatim below); `overall_totals` reader design (slots behind `tracking::query`, mirrors existing `filtered_project_savings` SQL); Validation Architecture mapping each load-bearing behavior to its smallest test; codebase-specific pitfalls (canonicalization off the write path, component-wise `Path::starts_with`, no `canonicalize()` on deleted paths, no field/view renames, `rusqlite` stays dev-only). |
</phase_requirements>

## Summary

Phase 8 is a contained presentation refactor of one command (`lacon stats`) plus exactly one new read-only aggregate behind the `lacon-core::tracking::query` boundary. The data model, the four SQL views, the write hot path, and the migration set are all out of scope by hard fence (D-01). The CONTEXT.md is unusually complete: 16 locked decisions, git-on-disk format verified against git 2.53.0 (confirmed installed: `git version 2.53.0`), and platform temp-path behavior captured. So this research does **not** re-derive the git/temp domain facts — it concentrates on (1) Validation Architecture (Nyquist is enabled in `.planning/config.json`), (2) verbatim code anchors the planner needs for `<action>`/`<read_first>` fields, (3) the `overall_totals` reader design, and (4) codebase-specific landmines.

The work decomposes cleanly: add two clap bool flags (`--bytes`, `--all`) to the `Stats` variant and thread them through `main.rs` → `stats::execute`; add `query::overall_totals` + `query::filtered_overall_totals` (mirroring the existing `filtered_project_savings` SQL body) returning a new `OverallTotals` struct; add private helpers in `stats.rs` (`humanize_bytes`, `canonical_project_key`, ephemeral detection, `.git` resolution, top-N capping/printing); restructure `execute` to print the headline first, then four relabeled/capped/humanized sections; update the four header `contains(...)` assertions in `cli_stats.rs` and add canonicalization/cap/`--all`/humanize tests.

**Primary recommendation:** Build the canonicalization + `.git`-resolution helpers as pure, total functions that take a `&str`/`&Path` and **never** call `canonicalize()` or spawn a subprocess, with a literal-path fallback on every error branch. Unit-test them inline against on-disk fixtures built in `tempfile::tempdir()`. Add `overall_totals` as a near-clone of `filtered_project_savings`' aggregate (collapse `GROUP BY project_path` to a single aggregate row + `COUNT(DISTINCT project_path)`). Validate behaviors at their boundaries only (Nyquist), not at redundant interior points.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Overall aggregate SQL (`SUM`/`COUNT DISTINCT` over `bypassed=0`) | `lacon-core::tracking::query` (read API) | — | D-01/D-02 boundary: ALL SQL lives in `lacon-core`; `lacon-cli` keeps `rusqlite` dev-only. |
| Per-section view reads (unchanged) | `lacon-core::tracking::query` (existing readers) | — | Already implemented; reused as-is. |
| Project canonicalization + `.git` resolution | `lacon-cli` (`commands/stats.rs` private fns) | filesystem (read-only `.git` file reads) | D-04: stats-local presentation; cold-path FS access that must NEVER be reachable from the write path. |
| Ephemeral-prefix bucketing | `lacon-cli` (`commands/stats.rs` private fn) | `std::env` (`temp_dir`, `$TMPDIR`) | D-08: runtime-built prefix set, component-wise `Path::starts_with`. |
| Rust-side rollup re-aggregation + re-sort | `lacon-cli` (`commands/stats.rs`) | — | D-06/D-11: exact sum-merge under canonical key; DB sort order destroyed by rollup so re-sort in Rust. |
| Byte humanization, label/column rendering, top-N capping | `lacon-cli` (`commands/stats.rs` presentation) | — | D-13/D-15/D-11: pure presentation, no model change. |
| `--bytes` / `--all` flag surface | `lacon-cli` (`cli.rs` `Stats` variant + `main.rs` dispatch) | — | D-12/D-14: bool flags on an existing subcommand; `cli_surface.rs` caps subcommands not flags. |

## Standard Stack

No new external dependencies. Everything needed is already in the workspace.

### Core (already present, reused)
| Item | Where | Purpose | Why Standard |
|------|-------|---------|--------------|
| `clap` (derive) | `lacon-cli` `[dependencies]` | `#[arg(long)] bytes: bool`, `#[arg(long)] all: bool` on `Stats` | Existing CLI surface; `Init` already has `#[arg(long)] user: bool` to copy. |
| `lacon_core::tracking::{open_readonly, Tracker::xdg_db_path}` | `lacon-core` | read-only DB open + path resolve | The exact pattern `stats.rs`/`explain.rs`/`doctor.rs` already use. |
| `lacon_core::tracking::query::*` | `lacon-core` | typed view readers + new `overall_totals` | D-01/D-02 SQL boundary; `rusqlite` stays dev-only in `lacon-cli`. |
| `std::path::Path::{starts_with, ancestors, parent, join}` | std | component-wise prefix match + `.git` ancestor walk | D-08 correctness (avoids `/tmpfoo`); std-only, no crate. |
| `std::env::{temp_dir, var_os("TMPDIR")}` | std | runtime ephemeral prefix set | D-08; portable across Linux/macOS. |
| `std::fs::{read_to_string, metadata}` | std | read `.git` file / detect `.git` dir vs file | D-09 bounded file reads; no `git` subprocess. |
| `tempfile` | `lacon-cli` `[dev-dependencies]` | build `.git` fixtures in tests | Already the test isolation primitive in `cli_stats.rs`. |
| `rusqlite` | `lacon-cli` `[dev-dependencies]` | seed test DB via `SCHEMA_DDL` | Already dev-only; new seeding rows for ephemeral/worktree cases reuse `insert_invocation`. |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled `.git` walk | `gix` / `git2` crate | Adds a heavy dependency to a cold-but-still-startup-sensitive binary, and pulls libgit2/network capability into a local-only tool. ADR 0014 explicitly mandates "a bounded sequence of file reads — no `git` subprocess." REJECTED. |
| Hand-rolled `humanize_bytes` | `humansize` / `bytesize` crate | One ~15-line function with deterministic decimal-SI output is trivial to test and matches the ADR's exact `22.8 KB` example; a crate adds a dependency and a rounding-policy you must still pin. REJECTED (D-13 says "single `humanize_bytes(i64) -> String` helper"). |
| New `overall` SQL view (migration) | append-only migration adding `v_overall` | Violates D-01 (no migration this phase) and ADR 0011 append-only constraint risk; the aggregate is a one-statement read, no view needed. REJECTED. |

**Installation:** None. No `cargo add`. (No `## Package Legitimacy Audit` section — this phase installs zero external packages.)

## Architecture Patterns

### System Architecture Diagram (data flow inside `stats::execute`)

```
                          lacon stats [--project P] [--since S] [--rule R] [--bytes] [--all]
                                              │
                                              ▼
                           parse --since → cutoff_ms (exit 2 on bad value)   ← UNCHANGED
                                              │
                              normalize_project(P) → project_ref             ← UNCHANGED
                                              │
                           xdg_db_path() ; if !exists → print_empty(); exit 0 ← UNCHANGED (relabel headers)
                                              │
                                  open_readonly(db_path)                      ← UNCHANGED
                                              │
        ┌─────────────────────────────────────┼───────────────────────────────────────────┐
        ▼                                       ▼                                            ▼
  NEW: query::overall_totals          existing per-section readers              query::project_savings
  (or filtered_overall_totals)      (unmatched / filtered / bypass)            (or filtered_project_savings)
        │                                       │                                            │
        ▼                                       ▼                                            ▼
  print HEADLINE first             relabel + cap N=10 + humanize           re-aggregate rows in Rust
  (runs, distinct projects,        + "… M more" hint (unless --all)        under canonical_project_key():
   raw→kept, saved abs + %)        sort order preserved from DB              (a) ephemeral → "(ephemeral)"
        (D-05)                            (D-11/D-15)                         (b) .git repo root resolution
                                                                             (c) literal fallback (D-07/D-09/D-10)
                                                                                          │
                                                                             re-SORT by bytes_saved DESC in Rust,
                                                                             cap N=10 + "… M more" + humanize
                                                                                       (D-06/D-11)
                                              │
                                              ▼
                                       Ok(0)   ← exit-code contract preserved (D-03)
```

The `--bytes` flag swaps `humanize_bytes(n)` for `n.to_string()` at every byte render site; `--all` disables both the `N=10` truncation and the `… M more` line.

### Pattern 1: Typed read-API free function over `&Connection` (the `overall_totals` template)
**What:** A free function `pub fn name(conn: &Connection, ...) -> Result<T, TrackingError>` that prepares a static-or-bound SQL string, `query_map`s into a typed struct, and propagates errors with bare `?`. `?` works because `TrackingError` has `#[from] rusqlite::Error` on its `Sqlite` variant (`crates/lacon-core/src/error.rs:142-146`).
**When to use:** The new `overall_totals` / `filtered_overall_totals` readers.
**Example (existing `filtered_project_savings`, the closest template — `query.rs:324-365`):**
```rust
// Source: crates/lacon-core/src/tracking/query.rs (read verbatim)
pub fn filtered_project_savings(
    conn: &Connection,
    since_cutoff_ms: Option<i64>,
    project: Option<&str>,
) -> Result<Vec<ProjectSaving>, TrackingError> {
    let mut sql = String::from(
        "SELECT project_path,
                COUNT(*) AS total_runs,
                SUM(raw_stdout_bytes + raw_stderr_bytes) AS raw_total,
                SUM(filtered_bytes) AS filtered_total,
                SUM(raw_stdout_bytes + raw_stderr_bytes - filtered_bytes) AS bytes_saved
         FROM invocations
         WHERE bypassed = 0",
    );
    let mut binds: Vec<&dyn rusqlite::ToSql> = Vec::new();
    let mut n = 0;
    if let Some(cut) = since_cutoff_ms.as_ref() {
        n += 1;
        sql.push_str(&format!(" AND ts >= ?{n}"));
        binds.push(cut);
    }
    if let Some(p) = project.as_ref() {
        n += 1;
        sql.push_str(&format!(" AND project_path = ?{n}"));
        binds.push(p);
    }
    sql.push_str(" GROUP BY project_path ORDER BY bytes_saved DESC");
    // ... prepare + query_map into ProjectSaving ...
}
```
The new aggregate is this same body with `GROUP BY project_path ORDER BY …` **removed**, the SELECT collapsed to scalars, and `COUNT(DISTINCT project_path)` added (see "overall_totals reader design" below).

### Pattern 2: Per-section error → exit-1 mapping (must be preserved verbatim)
**What:** Every section maps a `query::*` `Err` to `eprintln!("lacon stats: query failed: {e}")` + `return Ok(1)` rather than letting `TrackingError::Sqlite` escape through `?`→anyhow (WR-02). The headline query must follow the same posture.
**Example (`stats.rs:99-105`):**
```rust
let unmatched = match unmatched_res {
    Ok(rows) => rows,
    Err(e) => { eprintln!("lacon stats: query failed: {e}"); return Ok(1); }
};
```

### Pattern 3: Inline `#[cfg(test)] mod tests` with `use super::{...}` (D-04 unit-test home)
**What:** Helpers are private `fn`s; tests import them via `use super::{...}`. `stats.rs` already does this for `normalize_project`/`parse_since` (`stats.rs:299-358`); `explain.rs` does it for `pad_or_truncate`/`sanitize_for_display`/`split_lines`/`exit_code_from_stored` (`explain.rs:305-380`). The new `humanize_bytes` and canonicalization helpers go in the same block.

### Anti-Patterns to Avoid
- **Inlining SQL in `stats.rs`:** violates D-01/D-02. The headline aggregate MUST be a `query::*` function; `rusqlite` stays a `[dev-dependency]` of `lacon-cli` (verified: it is dev-only today).
- **`canonicalize()` anywhere in the canonical-key path:** ephemeral paths are frequently deleted; the write side stored the *logical* cwd. `canonicalize()` would error on deleted dirs and diverge from stored spellings (D-08). Use lexical/string matching only.
- **`str::starts_with` for ephemeral detection:** false-matches `/tmpfoo` against `/tmp` (D-08). Use `Path::starts_with` (component-wise).
- **Renaming stored fields or views:** D-15 is presentation-only. `filtered_bytes`, `avg_keep_ratio`, and the four `v_*` definitions stay byte-identical.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Read-only DB open with safe pragmas | a fresh `Connection::open_with_flags` in `stats.rs` | `tracking::open_readonly` | Already applies the exact `SQLITE_OPEN_READ_ONLY` + busy_timeout + FK pragmas and omits the WAL write (verified `mod.rs:156-172`); inlining would re-introduce the WAL-on-readonly bug it documents. |
| DB path resolution | etcetera call in `stats.rs` | `Tracker::xdg_db_path()` | One source of truth for `<XDG_DATA_HOME>/lacon/history.db` on both Linux and macOS (`mod.rs:121-126`). |
| `--project` path normalization | new parsing | existing `normalize_project` (`stats.rs:243-252`) | Already lexically absolutizes + strips trailing separator without `canonicalize`; reuse unchanged. |
| `.git` discovery | `git2`/`gix`/subprocess | std file reads per D-09 | ADR mandates bounded file reads, no subprocess; local-only tool. |

**Key insight:** Almost every "infrastructure" need in this phase already has a verified helper. The genuinely new code is small: one SQL aggregate, one byte-humanizer, one canonical-key resolver, and the output restructure.

## overall_totals reader design (D-02/D-05)

**Where it slots:** `crates/lacon-core/src/tracking/query.rs`, alongside the existing readers, behind the D-01 boundary. Two functions (unfiltered + filtered), mirroring every other reader pair.

**New typed row struct** (add near the other row structs, `query.rs:36-92`):
```rust
/// Aggregate totals across all `bypassed = 0` invocations, backing the
/// stats headline (D-05). `distinct_projects` counts unique stored
/// `project_path` values PRE-canonicalization (canonicalization is a
/// presentation concern in lacon-cli; SQL has no FS access). The headline's
/// "distinct projects (after canonicalization)" number is computed in
/// stats.rs from the rolled-up project map, NOT from this field — keep both
/// available so the planner can choose.
#[derive(Debug, Clone, PartialEq)]
pub struct OverallTotals {
    pub total_runs: i64,
    pub distinct_projects: i64,
    pub raw_total: i64,
    pub kept_total: i64,   // == SUM(filtered_bytes): bytes that survived to the model
    pub bytes_saved: i64,  // == raw_total - kept_total
}
```

> **Decision note for the planner (D-05 wording):** D-05 says the headline shows "distinct projects (**after canonicalization**)". SQL cannot canonicalize (no FS access). So the *displayed* distinct-projects count should be derived in `stats.rs` from the rolled-up canonical map's length (i.e. `rolled_up.len()`), and `OverallTotals.distinct_projects` (pre-canonicalization, from `COUNT(DISTINCT project_path)`) is informational only. The plan should make this explicit so the headline number matches the project section's row count. This is the one place the headline aggregate and the rollup must agree.

**Unfiltered reader (collapse the `project_savings` aggregate to one row):**
```rust
pub fn overall_totals(conn: &Connection) -> Result<OverallTotals, TrackingError> {
    let mut stmt = conn.prepare(
        "SELECT COUNT(*)                                                AS total_runs,
                COUNT(DISTINCT project_path)                            AS distinct_projects,
                COALESCE(SUM(raw_stdout_bytes + raw_stderr_bytes), 0)   AS raw_total,
                COALESCE(SUM(filtered_bytes), 0)                        AS kept_total,
                COALESCE(SUM(raw_stdout_bytes + raw_stderr_bytes
                             - filtered_bytes), 0)                      AS bytes_saved
         FROM invocations
         WHERE bypassed = 0",
    )?;
    let row = stmt.query_row([], |r| {
        Ok(OverallTotals {
            total_runs: r.get(0)?,
            distinct_projects: r.get(1)?,
            raw_total: r.get(2)?,
            kept_total: r.get(3)?,
            bytes_saved: r.get(4)?,
        })
    })?;
    Ok(row)
}
```

**Filtered counterpart (same `--since`/`--project` bind pattern as `filtered_project_savings`):**
```rust
pub fn filtered_overall_totals(
    conn: &Connection,
    since_cutoff_ms: Option<i64>,
    project: Option<&str>,
) -> Result<OverallTotals, TrackingError> {
    let mut sql = String::from(
        "SELECT COUNT(*),
                COUNT(DISTINCT project_path),
                COALESCE(SUM(raw_stdout_bytes + raw_stderr_bytes), 0),
                COALESCE(SUM(filtered_bytes), 0),
                COALESCE(SUM(raw_stdout_bytes + raw_stderr_bytes - filtered_bytes), 0)
         FROM invocations
         WHERE bypassed = 0",
    );
    let mut binds: Vec<&dyn rusqlite::ToSql> = Vec::new();
    let mut n = 0;
    if let Some(cut) = since_cutoff_ms.as_ref() { n += 1; sql.push_str(&format!(" AND ts >= ?{n}")); binds.push(cut); }
    if let Some(p)  = project.as_ref()          { n += 1; sql.push_str(&format!(" AND project_path = ?{n}")); binds.push(p); }
    let mut stmt = conn.prepare(&sql)?;
    let row = stmt.query_row(binds.as_slice(), |r| {
        Ok(OverallTotals { /* same field reads as above */ })
    })?;
    Ok(row)
}
```

**Three load-bearing details for the planner:**
1. **`COALESCE(SUM(...), 0)`** is required: on an empty (post-filter) result, `SUM` returns `NULL`, and `r.get::<_, i64>()` on a NULL would error. `COUNT(*)` already returns `0`, but `SUM`/`COUNT(DISTINCT)` over zero rows: `COUNT` returns `0`, `SUM` returns NULL. Coalesce all `SUM`s. (The existing per-section readers sidestep this by returning `Vec` — an empty result is zero rows, not a NULL-bearing row. A scalar aggregate is the new case where NULL surfaces.)
2. **`query_row` vs `query_map`:** the aggregate always returns exactly one row (even on empty DB → all-zero row), so use `query_row`, not `query_map().collect()`. This differs from every existing reader and is worth a comment.
3. **`bypassed = 0` is the WHERE floor** for both — matches `v_project_savings` and D-05 ("over `bypassed = 0` rows"). Do NOT add the `rule_id` predicates the offender views use; the headline spans matched + unmatched runs.

**Confidence:** HIGH — derived directly from the verbatim `filtered_project_savings` body (`query.rs:324-365`) and the `v_project_savings` view DDL (`tracking-data-model.md:132-141`); the only new SQL surface is the scalar collapse + `COALESCE`.

## Code Examples

### Two clap bool flags on the existing `Stats` variant (D-12/D-14)
```rust
// Source: crates/lacon-cli/src/cli.rs — extend the Stats variant (copy Init's
// `#[arg(long)] user: bool` shape, lines 40-47).
Stats {
    #[arg(long)]
    project: Option<PathBuf>,
    #[arg(long)]
    since: Option<String>,
    #[arg(long)]
    rule: Option<String>,
    /// Print exact integer byte counts instead of humanized values (scripting).
    #[arg(long)]
    bytes: bool,
    /// Print every row uncapped (suppresses the "… N more" lines).
    #[arg(long)]
    all: bool,
},
```
```rust
// Source: crates/lacon-cli/src/main.rs:15-17 — extend the dispatch arm.
CliCommand::Stats { project, since, rule, bytes, all } => {
    commands::stats::execute(project, since, rule, bytes, all)?
}
```
New signature: `pub fn execute(project: Option<PathBuf>, since: Option<String>, rule: Option<String>, bytes: bool, all: bool) -> anyhow::Result<i32>`.

### humanize_bytes (D-13) — decimal SI, 1 decimal above 1 KB, raw integer below
```rust
// Private fn in commands/stats.rs; unit-tested inline (D-04).
// Decimal SI (1000-based) per D-13 / ADR §4 ("22.8 KB"). Negative inputs are not
// expected (all stored byte counts are >= 0) but are handled defensively.
fn humanize_bytes(n: i64) -> String {
    const UNIT: f64 = 1000.0;
    let neg = n < 0;
    let bytes = n.unsigned_abs() as f64;
    if bytes < UNIT {
        return format!("{n} B"); // raw integer below 1 KB, e.g. "512 B", "0 B"
    }
    let units = ["KB", "MB", "GB", "TB", "PB"];
    let mut value = bytes / UNIT;
    let mut idx = 0;
    while value >= UNIT && idx < units.len() - 1 {
        value /= UNIT;
        idx += 1;
    }
    let sign = if neg { "-" } else { "" };
    format!("{sign}{value:.1} {}", units[idx]) // 1 decimal place, e.g. "22.8 KB"
}
```
> **Boundary the tests must pin (Nyquist):** `999 → "999 B"`, `1000 → "1.0 KB"`, `1024 → "1.0 KB"` (decimal, not binary), `22_800 → "22.8 KB"` (the ADR's literal example), `1_000_000 → "1.0 MB"`, `0 → "0 B"`. The `999`/`1000` pair is the B↔KB threshold; `1024 → 1.0 KB` proves decimal-SI (a binary scheme would print `1.0 KiB`/different value). These six points capture every failure mode of the rounding + threshold logic; interior points (e.g. 5_000) add nothing.

### canonical_project_key — precedence + literal fallback (D-07..D-10)
```rust
// Private fns in commands/stats.rs. PURE: no canonicalize(), no subprocess.
// Every error/None branch falls back to the literal stored path (D-10).
fn canonical_project_key(stored: &str) -> String {
    // (a) ephemeral takes precedence (D-07) — match the STORED string (D-08).
    if is_ephemeral(stored) {
        return "(ephemeral)".to_string();
    }
    // (b) repo root via .git resolution (D-09); None on any miss/IO error.
    if let Some(root) = resolve_repo_root(std::path::Path::new(stored)) {
        return root;
    }
    // (c) literal fallback (D-10) — never regress below the exact path.
    stored.to_string()
}

fn ephemeral_prefixes() -> Vec<std::path::PathBuf> {
    let mut v = vec![
        std::path::PathBuf::from("/tmp"),
        std::path::PathBuf::from("/var/folders"),
        std::path::PathBuf::from("/private/var/folders"),
        std::env::temp_dir(),
    ];
    if let Some(t) = std::env::var_os("TMPDIR") {
        v.push(std::path::PathBuf::from(t));
    }
    v // discretion: optionally add /dev/shm, /run/user/<uid>
}

fn is_ephemeral(stored: &str) -> bool {
    let p = std::path::Path::new(stored);
    ephemeral_prefixes().iter().any(|prefix| p.starts_with(prefix)) // component-wise (D-08)
}
```
`resolve_repo_root` walks `path.ancestors()`, for each ancestor checks `<ancestor>/.git`: if it's a directory → return that ancestor; if it's a file → parse `gitdir:` (relative resolved against the gitfile dir), read `<gitdir>/commondir`, resolve to the main `.git`, return its parent; check `core.bare` in `<gitdir>/config` → `None` (literal fallback). Any `fs` error → `None`. (Domain format details in CONTEXT D-09; verified against git 2.53.0.)

## Common Pitfalls

### Pitfall 1: Canonicalization reachable from the write path
**What goes wrong:** If `.git`-resolution or ephemeral-detection ever runs during `lacon run`, it adds per-invocation filesystem cost to the 10ms cold-start hot path (ADR 0001/0013).
**Why it happens:** A tempting "compute repo_root once and store it" refactor crosses the write/read boundary.
**How to avoid:** Keep all canonicalization as private `fn`s in `commands/stats.rs` (D-04). It must be unreachable from `lacon-core` and from `commands/run.rs`. The `repo_root`-on-write idea is explicitly **deferred** (CONTEXT Deferred Ideas).
**Warning signs:** Any `use` of the canonical-key helper outside `stats.rs`; any new FS read in `run.rs` or `record.rs`.

### Pitfall 2: `str::starts_with` instead of `Path::starts_with`
**What goes wrong:** `"/tmpfoo/x".starts_with("/tmp")` is `true` as strings → a real project under `/tmpfoo` wrongly collapses into `(ephemeral)`.
**How to avoid:** `Path::new(stored).starts_with(prefix)` (component-wise). D-08 calls this out explicitly.
**Warning signs:** A `.starts_with(` on a `&str` in the ephemeral path; a test that only checks `/tmp/x` and never `/tmpfoo/x`.

### Pitfall 3: Calling `canonicalize()` on a possibly-deleted ephemeral/stale path
**What goes wrong:** `std::fs::canonicalize` errors on a non-existent path and resolves symlinks (diverging from the stored logical cwd). Ephemeral dirs are usually already gone.
**How to avoid:** Match against the **stored string**; only do bounded `fs::read_to_string`/`metadata` on `.git` candidates, each with a literal fallback. Never `canonicalize`. (Same rationale `normalize_project` documents at `stats.rs:243-252` for `--project`.)
**Warning signs:** Any `canonicalize` in `stats.rs`; a test that fails when the seeded project dir doesn't exist on disk.

### Pitfall 4: Renaming stored field names or view definitions
**What goes wrong:** Renaming `filtered_bytes`/`avg_keep_ratio` or editing a `v_*` view breaks the append-only contract (ADR 0011) and the data model spec.
**How to avoid:** D-15 is presentation-only. Change only the printed strings in `stats.rs`. Leave `query.rs` struct field names (`total_filtered_bytes`, `avg_keep_ratio`) and all SQL unchanged. The `sent`/`kept`/`saved %` labels are render-time only.
**Warning signs:** A diff touching `migrations/0001_initial.sql`, `tracking-data-model.md` view DDL, or `query.rs` struct field identifiers.

### Pitfall 5: `rusqlite` leaking into `lacon-cli` production deps
**What goes wrong:** Adding the headline aggregate as inline SQL in `stats.rs` would require a runtime `rusqlite` dependency, breaking D-01/D-02.
**How to avoid:** Add the aggregate to `query.rs` (lacon-core). `lacon-cli`'s `rusqlite` stays under `[dev-dependencies]` (verified: it is dev-only today, used only by tests for seeding).
**Warning signs:** `rusqlite` appearing under `[dependencies]` in `crates/lacon-cli/Cargo.toml`; a `use rusqlite` in `stats.rs`.

### Pitfall 6: Scalar aggregate NULL on empty/filtered-empty result
**What goes wrong:** `SUM(...)` over zero rows returns SQL NULL; `r.get::<_, i64>()` on NULL errors, turning an empty-but-valid query into a spurious exit-1.
**How to avoid:** `COALESCE(SUM(...), 0)` in `overall_totals`/`filtered_overall_totals`. The empty-DB path is already short-circuited by `db_path.exists()` (D-03), but a `--since`/`--project` filter that matches nothing on a populated DB still hits this — must be covered.
**Warning signs:** A `--since`/`--project` filter producing a panic or exit-1 instead of a zeroed headline.

### Pitfall 7: Headline distinct-projects count disagreeing with the project section
**What goes wrong:** The SQL `COUNT(DISTINCT project_path)` counts pre-canonicalization paths; the project section shows post-rollup canonical buckets. If the headline prints the SQL count, it can exceed the visible project rows (confusing).
**How to avoid:** Compute the displayed "distinct projects" from the rolled-up canonical map length in `stats.rs` (D-05 says "after canonicalization"). See the overall_totals decision note.
**Warning signs:** Headline says "N projects" but the project section shows fewer rolled-up rows (and `--all` is set so it's not a cap artifact).

## Validation Architecture

> Nyquist validation is **enabled** (`.planning/config.json` → `workflow.nyquist_validation: true`). This section is mandatory. Sampling principle: test each behavior at its **failure-mode boundaries** (thresholds, branch edges, empty/overflow), not at redundant interior points.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` (libtest) + `assert_cmd` (black-box CLI) + `tempfile` + dev-only `rusqlite` for DB seeding |
| Config file | none — Cargo-native; suite gated by CI (`.github/workflows/ci.yml`) after `cargo build --workspace` |
| Quick run command | `cargo test -p lacon-cli stats` (substring-matches both `cli_stats.rs` and the inline `stats.rs` tests) |
| Full suite command | `cargo build --workspace && cargo test --workspace` (the build-first rule is load-bearing per CLAUDE.md — `assert_cmd` resolves `target/debug/lacon`) |

> **Wave-0 build note for the planner:** `cargo test -p lacon-cli` requires `target/debug/lacon` to exist (assert_cmd `cargo_bin` fallback). Plans that run the black-box tests must `cargo build -p lacon-cli` (or `--workspace`) first, or the new `cli_stats.rs` tests panic on unresolved binary — identical to the existing suite's constraint.

### Phase Requirements → Test Map
| Behavior (ADR 0014) | Test Type | Automated Command | Smallest sufficient test (Nyquist) | File Exists? |
|---------------------|-----------|-------------------|-----------------------------------|--------------|
| `(ephemeral)` bucket collapse | unit | `cargo test -p lacon-cli is_ephemeral` (inline) | Seed N rows under `/tmp/a`, `/tmp/b`, `$TMPDIR/c` → assert `canonical_project_key` returns `"(ephemeral)"` for each; AND a black-box test asserting the project section shows ONE `(ephemeral)` line, not N. Boundary: also assert `/tmpfoo/x` is NOT ephemeral (the `Path::starts_with` vs `str::starts_with` failure mode). | ❌ Wave 0 |
| `.git` **directory** rollup (normal repo + subdir) | unit + black-box | `cargo test -p lacon-cli resolve_repo_root` | tempdir `repo/.git/` (dir) + a subdir `repo/sub/`; assert both `repo` and `repo/sub` resolve to `repo`. Two rows seeded with paths `repo` and `repo/sub` roll into one project line. Boundaries: repo root itself, one level of subdir. | ❌ Wave 0 |
| `.git` **file** worktree rollup (absolute gitdir) | unit + black-box | `cargo test -p lacon-cli` (worktree case) | tempdir `repo/.git/` (dir) + `repo/.git/worktrees/wt/commondir` = `../..` + a worktree dir `wt/.git` (file) = `gitdir: <abs>/repo/.git/worktrees/wt`; assert `wt` resolves to `repo`. Black-box: a worktree row + a main-repo row roll into one line. | ❌ Wave 0 |
| relative-gitdir (submodule) resolution | unit | `cargo test -p lacon-cli` (submodule case) | tempdir with a `.git` **file** whose `gitdir:` value is **relative**; assert it resolves against the gitfile's own dir (the branch the worktree-absolute case doesn't exercise). One targeted unit test; this is the relative-vs-absolute branch boundary. | ❌ Wave 0 |
| top-N capping + `… M more` | black-box | `cargo test -p lacon-cli` (cap case) | Seed N=11 distinct non-ephemeral, non-git projects → assert exactly 10 project rows printed + a `… more` line. Boundary: 11 (one over the cap) is the smallest input that proves both the cap AND the overflow line; 10 would prove neither. | ❌ Wave 0 |
| `--all` uncapping | black-box | `cargo test -p lacon-cli` (--all case) | Same 11-project seed + `--all` → assert all 11 rows present AND no `… more` line. (Pairs with the cap test on the same fixture.) | ❌ Wave 0 |
| `--bytes` exact-integer escape | black-box | `cargo test -p lacon-cli` (--bytes case) | Seed a row whose byte total humanizes (e.g. 22_800) → without `--bytes` assert `contains("22.8 KB")`; with `--bytes` assert `contains("22800")` and NOT `contains("KB")`. Boundary: one value that differs between the two render modes. | ❌ Wave 0 |
| `humanize_bytes` decimal-SI boundaries | unit | `cargo test -p lacon-cli humanize` (inline) | The six boundary points: `999→"999 B"`, `1000→"1.0 KB"`, `1024→"1.0 KB"`, `22_800→"22.8 KB"`, `1_000_000→"1.0 MB"`, `0→"0 B"`. Covers B↔KB threshold + decimal-vs-binary + ADR example + 1-decimal rounding. | ❌ Wave 0 |
| overall headline aggregate (`overall_totals` over `bypassed=0`) | unit (lacon-core) + black-box | `cargo test -p lacon-core overall` + `cargo test -p lacon-cli stats` | lacon-core: seed matched + unmatched + one `bypassed=1` row → assert `total_runs`/`raw_total`/`kept_total`/`bytes_saved` exclude the bypassed row. Black-box: assert the headline line appears FIRST (before "Commands with no rule") with runs + saved %. Boundary: the bypassed-exclusion is the one correctness edge. | ❌ Wave 0 |
| literal-path fallback (I/O error / bare repo / deleted dir) | unit | `cargo test -p lacon-cli` (fallback cases) | Three smallest cases: (1) path with no `.git` anywhere → returns the literal path; (2) `.git/config` with `core.bare = true` → literal; (3) a path that does not exist on disk → literal (no panic, no canonicalize). Each is a distinct fallback branch. | ❌ Wave 0 |
| exit-code + empty-DB contract preservation | black-box | `cargo test -p lacon-cli stats_empty_db / stats_invalid_since` (EXISTING) | Already covered by `stats_empty_db_prints_no_data_yet_and_succeeds` and `stats_invalid_since_errors_nonzero_no_panic` (`cli_stats.rs:174-246`). Verify they still pass after the restructure; update the empty-DB header assertion only if `print_empty` headers are relabeled. | ✅ exists (verify) |
| relabeled section headers | black-box | `cargo test -p lacon-cli stats_seeded` (EDIT) | Update the four `contains("Unmatched offenders"/...)` assertions (`cli_stats.rs:166-169`) to the new labels (D-15). Token-only edit; this IS the regression guard for the relabel. | ✅ exists (edit) |

### Sampling Rate
- **Per task commit:** `cargo test -p lacon-cli stats` (inline `stats.rs` unit tests + `cli_stats.rs` black-box) and, for the reader task, `cargo test -p lacon-core query` / `overall`.
- **Per wave merge:** `cargo build --workspace && cargo test --workspace` (full hermetic suite — what CI gates on).
- **Phase gate:** Full suite green + `cargo clippy --workspace --all-targets` + `cargo fmt --check` before `/gsd:verify-work`.

### Wave 0 Gaps
- [ ] Inline `#[cfg(test)] mod tests` additions in `crates/lacon-cli/src/commands/stats.rs` — `humanize_bytes` boundary tests, `is_ephemeral` (incl. `/tmpfoo` negative), `resolve_repo_root` (dir / worktree-absolute / submodule-relative), literal-fallback (no-git / bare / deleted) tests. (No new file; extend the existing block at `stats.rs:299-358`.)
- [ ] New black-box tests in `crates/lacon-cli/tests/cli_stats.rs` — ephemeral collapse, `.git` rollup, top-N cap + `… more`, `--all` uncap, `--bytes` escape, headline-first. Reuse `init_db`/`insert_invocation`/`lacon` helpers; add a small fixture builder that writes `.git` dirs/files under `tempdir()`.
- [ ] New lacon-core test for `overall_totals`/`filtered_overall_totals` — place alongside existing `query.rs` tests (or `tests/` integration test for tracking) seeding matched/unmatched/bypassed rows.
- [ ] Edit the four section-header `contains(...)` assertions in `cli_stats.rs:166-169` to the new D-15 labels.
- [ ] Framework install: none — Cargo-native test stack already present.

## State of the Art

| Old (current `stats.rs`) | New (this phase) | Impact |
|--------------------------|------------------|--------|
| No summary line | Overall headline first (runs, distinct projects, raw→kept, saved abs+%) | D-05 |
| One project row per literal `project_path` | Rust-side rollup under canonical key (`(ephemeral)` / repo root / literal) | D-06..D-10 |
| Every section printed in full | Top-N=10 + `… M more`, `--all` to uncap | D-11/D-12 |
| Raw integer byte counts | `humanize_bytes` decimal-SI, `--bytes` to override | D-13/D-14 |
| "Unmatched/Filtered offenders", `filtered_bytes`, `keep_ratio` | task-oriented headers, `sent`/`kept`, `saved %` | D-15 (presentation only) |

**Not changing (anti-state-of-the-art for this phase):** the four SQL views, stored field names, the write path, the migration set. (D-01.)

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `query_row` on a scalar aggregate with `COALESCE(SUM,0)` returns an all-zero row on an empty result (no NULL panic). | overall_totals design / Pitfall 6 | LOW — standard SQLite semantics; verify with the empty-filter test. If wrong, the filtered-empty case exits 1 instead of printing a zeroed headline. |
| A2 | `TrackingError` has `#[from] rusqlite::Error` on its `Sqlite` variant, so the new readers can use bare `?` like every existing reader. | Pattern 1 | LOW — every reader in `query.rs` already uses bare `?` on `conn.prepare(...)`, which only compiles with that `From`. Confirmed by `error.rs:142-146` (`Sqlite { source }`) + universal `?` use. |
| A3 | `git worktree`/submodule on-disk format (gitfile `gitdir:`, `commondir`) matches what D-09 describes on this machine. | canonical_project_key | LOW — git 2.53.0 confirmed installed (matches CONTEXT's verified version). The `.git`-resolution tests build fixtures directly, so they pin the format regardless. |
| A4 | The displayed "distinct projects" should come from the rolled-up map length, not SQL `COUNT(DISTINCT)`. | overall_totals decision note / Pitfall 7 | MEDIUM — D-05 says "after canonicalization"; a planner could wire the SQL count by mistake, producing a headline that disagrees with the visible project rows. Flag for the plan to make explicit. |

## Open Questions

1. **Final header/column wording** (Claude's Discretion in CONTEXT). The ADR §4 gives examples ("Commands with no rule", "Rule effectiveness", `sent`/`kept`, `saved %`) but not verbatim final strings for all four sections.
   - What we know: framing is fixed; only exact tokens are open.
   - Recommendation: the plan should pick concrete strings and the `cli_stats.rs` header assertions should match them — the test IS the contract for the wording.
2. **Linux tmpfs prefixes** (`/dev/shm`, `/run/user/<uid>`) — discretionary add to the ephemeral set.
   - What we know: the ADR-listed set (`/tmp`, `/var/folders`, `/private/var/folders`, `temp_dir()`, `$TMPDIR`) is the floor.
   - Recommendation: include `/dev/shm`; `/run/user/<uid>` requires the uid — low value, skip in v1 unless trivial.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain (pinned) | build/test | ✓ | per `rust-toolchain.toml` | — |
| `git` | only to build `.git` fixtures by hand in tests (or to spot-verify on-disk format) | ✓ | 2.53.0 | tests can construct `.git` dirs/files with `std::fs` (no `git` binary needed) — the production code never shells out to git (D-09) |
| `rusqlite` (dev) | seed test DB | ✓ | workspace (`[dev-dependencies]`) | — |
| `assert_cmd` / `tempfile` / `predicates` (dev) | black-box CLI tests | ✓ | workspace | — |

**Missing dependencies with no fallback:** none.
**Missing dependencies with fallback:** none required — production code is std-only for the new path logic; tests can build `.git` fixtures with `std::fs` even without the `git` binary.

## Security Domain

> `security_enforcement` is not set in `.planning/config.json` (treated as default). This phase is a read-only, local-only presentation change with a narrow new surface; the relevant controls are minimal but listed for completeness.

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V5 Input Validation | yes | New SQL binds (`--since`/`--project` on `overall_totals`) MUST use `params![]`/`?N` placeholders, never string interpolation (mitigates T-04-01 SQL injection). The static aggregate columns are not user-controlled. |
| V12 Files & Resources | yes | `.git` resolution does bounded `fs::read_to_string`/`metadata` only; never executes a path, never writes, never follows into a subprocess. Cap the ancestor walk (path depth is naturally bounded) and treat every `fs` error as literal fallback (no panic, no info leak). |
| V6 Cryptography | no | none. |
| V2/V3/V4 (auth/session/access) | no | local-only single-user CLI; no auth surface. |

### Known Threat Patterns for this stack
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| SQL injection via `--since`/`--project` reaching the new aggregate | Tampering | bound via `params![]` placeholders (existing `filtered_*` pattern); only static SQL fragments concatenated. |
| Malicious/odd content in a stored `project_path` or `.git` file driving the resolver | Tampering / DoS | bounded file reads, literal fallback on any parse/IO error; no `canonicalize` (avoids symlink-following surprises); output is plain text printed by `stats`, not re-executed. Headline/section bytes are not re-rendered through a terminal-control path here (unlike `explain`, which already sanitizes — not in scope for `stats` numeric/path output, though canonical keys printed verbatim should be considered for control-byte neutralization if a project path could contain ESC bytes — LOW risk, flag for plan discretion). |

## Sources

### Primary (HIGH confidence) — read verbatim this session
- `crates/lacon-cli/src/commands/stats.rs` — the command being redesigned; existing `normalize_project`/`parse_since`/`print_empty` + inline test block.
- `crates/lacon-core/src/tracking/query.rs` — typed readers + the `filtered_project_savings` template for `overall_totals`.
- `crates/lacon-cli/src/commands/explain.rs`, `doctor.rs` — sibling inline-helper + inline-test convention (D-04).
- `crates/lacon-cli/tests/cli_stats.rs` — black-box substring assertions + `SCHEMA_DDL`/`init_db`/`insert_invocation` seeding pattern.
- `crates/lacon-cli/src/cli.rs` (`Stats` variant; `Init`'s `#[arg(long)] user: bool`) + `src/main.rs` (dispatch).
- `crates/lacon-core/src/tracking/mod.rs:121-172` — `Tracker::xdg_db_path` + `open_readonly` (verbatim).
- `crates/lacon-core/src/tracking/health.rs` — `health_check`/`HealthReport`.
- `crates/lacon-core/src/error.rs:131-169` — `TrackingError` variants (`Sqlite { source }`, `#[from]`).
- `crates/lacon-cli/Cargo.toml` — `rusqlite` confirmed `[dev-dependencies]` only.
- `crates/lacon-cli/tests/cli_surface.rs` — D-12 confirmation (cap is on subcommands, not flags).
- `docs/decisions/0014-stats-read-time-presentation.md` — governing ADR.
- `docs/decisions/0011-sqlite-for-local-tracking.md` (referenced) + `docs/specs/tracking-data-model.md` — schema, four view DDLs, append-only/retention.
- `.planning/phases/08-.../08-CONTEXT.md` — 16 locked decisions.
- `.planning/config.json` — `nyquist_validation: true`, `commit_docs: true`.
- `git --version` → 2.53.0 (matches CONTEXT's verified format).

### Secondary (MEDIUM confidence)
- `.planning/REQUIREMENTS.md` / `.planning/ROADMAP.md` — phase scope + REQ mapping (`REQ-cli-stats` complete; this phase = ADR 0014).

### Tertiary (LOW confidence)
- None — all claims verified against the codebase or CONTEXT.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — zero new external deps; every reused helper read verbatim.
- Architecture / code anchors: HIGH — all touched functions and signatures confirmed in source this session.
- `overall_totals` design: HIGH — derived from the verbatim `filtered_project_savings` body + view DDL; only new surface is the scalar collapse + `COALESCE`.
- Validation strategy: HIGH — Nyquist boundary tests mapped to existing framework; framework confirmed present.
- Pitfalls: HIGH — each tied to a specific locked decision and a source line.

**Research date:** 2026-05-23
**Valid until:** 2026-06-22 (stable; the only external dependency is the on-disk `.git` format, pinned by git 2.53.0 and re-pinned by the resolution tests).
