# Phase 8: Redesign lacon stats output for readability (ADR 0014) - Pattern Map

**Mapped:** 2026-05-23
**Files analyzed:** 4 modified files (no new files created)
**Analogs found:** 4 / 4 (all exact — every unit of work has an in-repo template)

## Phase shape (read before using the table)

This is a **read-time presentation refactor**. NO new files are created — all four
units of work **extend existing files**. There is exactly one new code surface in
`lacon-core` (the `overall_totals` reader pair); everything else is new private
helpers, new clap flags, and new tests grafted onto code that already exists. The
"analog" for each unit is therefore a sibling pattern **inside the same file** (or
the same crate), which is the strongest possible match — copy it near-verbatim.

Load-bearing fences (from CONTEXT D-01..D-16, restated so the planner does not
have to cross-reference):
- **All SQL stays behind `lacon-core::tracking::query`.** `rusqlite` is confirmed
  `[dev-dependencies]`-only in `crates/lacon-cli/Cargo.toml:42` — never add it to
  `[dependencies]`, never `use rusqlite` in `stats.rs`.
- **Canonicalization (`.git` resolution, ephemeral detection) must live as private
  `fn`s in `commands/stats.rs` and be unreachable from the write path** (`run.rs`,
  `record.rs`, `lacon-core`). It is stats-only and read-only.
- **No `canonicalize()`** anywhere in the canonical-key path (deleted/ephemeral
  dirs); **`Path::starts_with`, not `str::starts_with`** (the `/tmpfoo` false-match).
- **No field/view renames, no migration.** D-15 is render-string-only.

## File Classification

| Modified File | Role | Data Flow | Closest Analog | Match Quality |
|---------------|------|-----------|----------------|---------------|
| `crates/lacon-cli/src/commands/stats.rs` (private helpers: `humanize_bytes`, `is_ephemeral`/`ephemeral_prefixes`, `resolve_repo_root`/`canonical_project_key`, top-N cap) | command + presentation utility | transform / file-I/O (read-only `.git` reads) | inline siblings `normalize_project`/`parse_since`/`print_empty` in **same file** (`stats.rs:243-297`) + inline `#[cfg(test)] mod tests` (`stats.rs:299-358`); `explain.rs` helper+test convention (`explain.rs:199-380`) | exact (same-file sibling) |
| `crates/lacon-core/src/tracking/query.rs` (new `overall_totals` + `filtered_overall_totals` + `OverallTotals` struct) | service / read API | CRUD (read-only aggregate) | `filtered_project_savings` (`query.rs:324-365`) for the filtered counterpart; `project_savings` (`query.rs:155-172`) + struct `ProjectSaving` (`query.rs:66-74`) for the unfiltered shape | exact (same role + same data flow) |
| `crates/lacon-cli/src/cli.rs` (add `bytes: bool`, `all: bool` to `Stats` variant) + `crates/lacon-cli/src/main.rs` (extend dispatch) | config (CLI surface) | request-response (arg dispatch) | `Init`'s `#[arg(long)] user: bool` (`cli.rs:40-47`); `main.rs:14` `Init` destructure → `commands::init::execute(user, project)` | exact |
| `crates/lacon-cli/tests/cli_stats.rs` (header-assertion edits + new seeded tests; inline unit tests go in `stats.rs`) | test | request-response (black-box) | existing seeded test `stats_seeded_db_shows_four_sections_and_offender_rows` (`cli_stats.rs:152-172`) + helpers `SCHEMA_DDL`/`init_db`/`insert_invocation`/`lacon` (`cli_stats.rs:25-150`) | exact |

---

## Pattern Assignments

### Unit 1 — Private presentation helpers in `commands/stats.rs` (command + transform/file-I/O)

**Analog:** `crates/lacon-cli/src/commands/stats.rs` (its own existing inline helpers)
and `crates/lacon-cli/src/commands/explain.rs` (sibling-command convention).

This unit is **adding private `fn`s + inline tests to a file that already has both**.
Mirror the existing structure exactly: helpers as bare `fn` (no `pub`) defined after
`execute`, doc-commented with the rationale, unit-tested in the inline
`#[cfg(test)] mod tests` block via `use super::{...}`.

**Inline-helper convention** (`stats.rs:243-252`, `normalize_project` — note: bare
`fn`, doc comment states the *why* and the no-`canonicalize` rationale, lexical/string
handling, literal fallback on error — the new `canonical_project_key`/`is_ephemeral`/
`resolve_repo_root` helpers must follow this exact posture):
```rust
/// WR-03: normalize a `--project` argument to line up with the stored
/// `project_path` ... We deliberately do NOT `canonicalize` (resolve symlinks):
/// the write side stores the *logical* cwd, so symlink resolution here would
/// diverge from it.
fn normalize_project(p: &std::path::Path) -> String {
    let abs = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
    let s = abs.to_string_lossy();
    let trimmed = s
        .strip_suffix(std::path::MAIN_SEPARATOR)
        .filter(|t| !t.is_empty())
        .unwrap_or(&s);
    trimmed.to_string()
}
```

**Error-as-fallback convention** (`stats.rs:259-283`, `parse_since` — returns
`Result<_, String>` for caller-mapped messaging; the new helpers instead use the
`Option`/literal-fallback shape from RESEARCH `canonical_project_key`, but the
"every error branch has a graceful non-panicking outcome" discipline is the same).

**Inline test block convention — copy this exact shape** (`stats.rs:299-358`):
```rust
#[cfg(test)]
mod tests {
    use super::{normalize_project, parse_since};
    use std::path::{Path, MAIN_SEPARATOR};

    #[test]
    fn normalize_project_absolute_unchanged() { /* ... */ }

    #[test]
    fn parse_since_rejects_bad_unit() {
        assert!(parse_since("7x").is_err());
        assert!(parse_since("abc").is_err());
    }
}
```
The `use super::{...}` import line is extended to include the new helpers
(`humanize_bytes`, `is_ephemeral`, `resolve_repo_root`, `canonical_project_key`).

**Sibling-command confirmation** (`explain.rs:199-303` are bare `fn`s
`exit_code_from_stored`/`split_lines`/`render_side_by_side`/`sanitize_for_display`/
`pad_or_truncate`; `explain.rs:305-380` is their inline `#[cfg(test)] mod tests` with
`use super::{exit_code_from_stored, pad_or_truncate, sanitize_for_display, split_lines};`).
This confirms the convention is project-wide, not a one-off in `stats.rs`.

**Boundary tests the inline block must pin (from RESEARCH Validation Architecture):**
- `humanize_bytes`: `999→"999 B"`, `1000→"1.0 KB"`, `1024→"1.0 KB"` (proves decimal-SI, not binary), `22_800→"22.8 KB"` (ADR §4 literal), `1_000_000→"1.0 MB"`, `0→"0 B"`.
- `is_ephemeral`: `/tmp/x`→true, `$TMPDIR/c`→true, **`/tmpfoo/x`→false** (the `Path::starts_with` vs `str::starts_with` failure mode — this negative is mandatory).
- `resolve_repo_root` / literal fallback: `.git` dir → ancestor; `.git` file w/ absolute gitdir (worktree); `.git` file w/ relative gitdir (submodule); no-`.git` → literal; `core.bare=true` → literal; non-existent path → literal (no panic, no `canonicalize`). Build fixtures with `tempfile::tempdir()` + `std::fs`.

---

### Unit 2 — New SQL aggregate reader `query::overall_totals` (+ filtered) (service, read-only CRUD)

**Analog:** `crates/lacon-core/src/tracking/query.rs` — the filtered counterpart is a
near-clone of `filtered_project_savings` (`query.rs:324-365`); the row struct mirrors
`ProjectSaving` (`query.rs:66-74`); the unfiltered reader mirrors `project_savings`
(`query.rs:155-172`) but collapses to a single scalar row.

**Row struct convention — mirror `ProjectSaving`** (`query.rs:66-74`; same derives,
same doc-comment-above-struct style):
```rust
/// Row of `v_project_savings`: per-project raw-vs-filtered byte totals.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectSaving {
    pub project_path: Option<String>,
    pub total_runs: i64,
    pub raw_total: i64,
    pub filtered_total: i64,
    pub bytes_saved: i64,
}
```
New `OverallTotals` struct goes near the other row structs (`query.rs:36-92`); shape
specified in RESEARCH lines 213-220 (`total_runs`, `distinct_projects`, `raw_total`,
`kept_total`, `bytes_saved`).

**Filtered-reader convention — copy `filtered_project_savings` body verbatim, then
collapse** (`query.rs:324-365`). This is THE template. Note every load-bearing detail:
the `&Connection` first arg, `Option<i64>`/`Option<&str>` filter args, the
`Vec<&dyn rusqlite::ToSql>` binds vec, the `?{n}` placeholder incrementing (NEVER
string-interpolate filter values — T-04-01), `WHERE bypassed = 0` floor, bare `?`
propagation (works because `TrackingError` has `#[from] rusqlite::Error`):
```rust
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

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(binds.as_slice(), |r| {
            Ok(ProjectSaving {
                project_path: r.get(0)?,
                total_runs: r.get(1)?,
                raw_total: r.get(2)?,
                filtered_total: r.get(3)?,
                bytes_saved: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
```

**Three deltas from this template** (the only new SQL surface — RESEARCH lines 279-282):
1. **Drop `GROUP BY ... ORDER BY ...`**, collapse the SELECT to scalars, add
   `COUNT(DISTINCT project_path)`.
2. **`COALESCE(SUM(...), 0)` every `SUM`** — a scalar aggregate over zero rows
   (e.g. a `--since`/`--project` filter matching nothing on a populated DB) returns
   SQL NULL, and `r.get::<_, i64>()` on NULL errors → spurious exit-1. The existing
   `Vec`-returning readers never hit this (empty result = zero rows, not a NULL row).
   This is the one genuinely-new failure mode; it must be covered by a test.
3. **`query_row`, not `query_map().collect()`** — the aggregate always returns
   exactly one row (all-zeros on empty). Worth a code comment since it differs from
   every existing reader.

The exact target bodies for both `overall_totals` and `filtered_overall_totals` are
written verbatim in 08-RESEARCH.md lines 225-277 — the planner can lift them directly.

**Unfiltered-reader convention — `project_savings`** (`query.rs:155-172`) shows the
no-arg, view-direct shape; `overall_totals(conn)` follows it but reads the base
`invocations` table (no `v_overall` view exists; D-01 forbids adding one) and uses
`query_row`.

---

### Unit 3 — New `--bytes` / `--all` bool CLI flags (config, request-response)

**Analog:** `Init`'s `#[arg(long)] user: bool` (`cli.rs:40-47`) and the `Init`
dispatch arm in `main.rs:14`.

**clap bool-flag convention — copy `Init`'s `user: bool`** (`cli.rs:40-47`):
```rust
Init {
    /// Install into the user (home-relative, global) scope.
    #[arg(long)]
    user: bool,
    /// Install into the project (cwd-relative) scope.
    #[arg(long)]
    project: bool,
},
```
The `Stats` variant to extend (`cli.rs:49-59`) currently has `project`/`since`/`rule`.
Add `bytes: bool` and `all: bool` with `#[arg(long)]` + a `///` doc line each (the
existing `since`/`rule` fields lack doc comments, but `project` and the `Init` flags
have them — match the documented style; the doc string becomes the `--help` text).
RESEARCH lines 289-306 give the exact target variant.

**Dispatch destructure convention — `main.rs:11-20`** (the `match cli.command` block;
each arm destructures the variant's named fields and forwards them positionally to
`commands::<cmd>::execute(...)`):
```rust
let exit_code = match cli.command {
    CliCommand::Run { rule, argv } => commands::run::execute(rule, argv)?,
    CliCommand::Init { user, project } => commands::init::execute(user, project)?,
    CliCommand::Stats { project, since, rule } => {
        commands::stats::execute(project, since, rule)?
    }
    // ...
};
```
The `Stats` arm (`main.rs:15-17`) extends to
`CliCommand::Stats { project, since, rule, bytes, all } => commands::stats::execute(project, since, rule, bytes, all)?`.
The `execute` signature in `stats.rs:27` extends correspondingly to
`pub fn execute(project: Option<PathBuf>, since: Option<String>, rule: Option<String>, bytes: bool, all: bool) -> anyhow::Result<i32>`.
(RESEARCH lines 308-313.)

**Safe per D-12:** `cli_surface.rs` caps the number of *subcommands*, not flags, so
adding two bool flags to an existing subcommand does not trip that guard.

---

### Unit 4 — Black-box test additions in `tests/cli_stats.rs` (test, request-response)

**Analog:** the entire existing `crates/lacon-cli/tests/cli_stats.rs` — its
`SCHEMA_DDL`/`init_db`/`insert_invocation`/`lacon` seeding helpers (`cli_stats.rs:25-150`)
and the seeded test `stats_seeded_db_shows_four_sections_and_offender_rows`
(`cli_stats.rs:152-172`).

**DB-seeding helper pattern — reuse as-is** (`cli_stats.rs:25-150`): the dev-only
`rusqlite` + `SCHEMA_DDL` constant (a byte-exact subset of `0001_initial.sql`: two
base tables + four views), `init_db(xdg)` creating `<xdg>/lacon/history.db`,
`insert_invocation(...)` for one row, and `lacon(xdg)` building an `assert_cmd::Command`
with `XDG_DATA_HOME` pointed at the tempdir. **Do not duplicate these — the new tests
call them.** New `.git`-fixture tests additionally write `.git` dirs/files under
`tempfile::tempdir()` with `std::fs` (no `git` binary needed).

**Full seeded-test convention — copy this shape** (`cli_stats.rs:152-172`):
```rust
#[test]
fn stats_seeded_db_shows_four_sections_and_offender_rows() {
    let xdg = tempdir().unwrap();
    let now_ms = 1_700_000_000_000_i64;
    let conn = init_db(xdg.path());
    insert_invocation(&conn, now_ms, "/p/a", "make", None, 0, 5000, 5000, 0);
    insert_invocation(&conn, now_ms, "/p/a", "cargo", Some("cargo-rule"), 0, 8000, 1200, 0);

    let assert = lacon(xdg.path()).arg("stats").assert().success();
    let out = assert.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(stdout.contains("Unmatched offenders"), "missing section header; got:\n{stdout}");
    assert!(stdout.contains("make"), "expected unmatched offender row; got:\n{stdout}");
}
```
New tests follow this exactly: `tempdir()` → `init_db` → N× `insert_invocation` →
`lacon(xdg.path()).args([...]).assert().success()` → `String::from_utf8_lossy(stdout)`
→ substring `contains(...)` assertions (NOT golden-file equality — that is what
"snapshot" means here; substring is the contract).

**Filter-narrowing convention** (`cli_stats.rs:188-209`,
`stats_project_filter_narrows_output`): seed two distinguishable rows, assert one is
present and the other absent with the filter — the template for the `--bytes` mode
test (assert `contains("22.8 KB")` without the flag; `contains("22800")` and
`!contains("KB")` with `--bytes`) and the `--all` test (11-project seed: capped run
shows 10 rows + `… more`; `--all` shows 11 + no `… more`).

**Targeted edits, not a rewrite (D-16):**
- Update the four section-header `contains(...)` assertions at `cli_stats.rs:166-169`
  (`"Unmatched offenders"`, `"Filtered offenders"`, `"Bypass rates"`,
  `"Per-project savings"`) to the new D-15 labels — these assertions ARE the contract
  for the final wording, so they must match whatever strings `stats.rs` prints.
- The empty-DB test (`cli_stats.rs:174-185`) and invalid-`--since` test
  (`cli_stats.rs:233-246`) are preserved; only update the empty-DB header assertion
  if `print_empty` (`stats.rs:286-297`) gets relabeled. The empty-DB test asserts
  `"no data yet"` (case-insensitive) — keep that token if `print_empty` keeps it.

---

## Shared Patterns

### Per-section error → exit-1 mapping (apply to the new headline read)
**Source:** `crates/lacon-cli/src/commands/stats.rs:99-105`
**Apply to:** the new `overall_totals`/`filtered_overall_totals` call site in `execute`.
Every `query::*` `Err` is mapped to a `lacon stats:` stderr line + `return Ok(1)` —
NOT propagated via `?` (which would surface `TrackingError`'s internal text and bypass
the chosen exit code). The headline must follow the same posture as every section.
```rust
let unmatched = match unmatched_res {
    Ok(rows) => rows,
    Err(e) => {
        eprintln!("lacon stats: query failed: {e}");
        return Ok(1);
    }
};
```

### Read-only DB open + path resolve + empty-DB short-circuit (reuse unchanged)
**Source:** `crates/lacon-cli/src/commands/stats.rs:64-86`
**Apply to:** unchanged — the restructured `execute` keeps this exact prologue. Resolve
`Tracker::xdg_db_path()`, short-circuit to `print_empty()` + `Ok(0)` when
`!db_path.exists()` (so a fresh machine still prints "no data yet"), then
`tracking::open_readonly(&db_path)`. Do NOT inline a fresh `Connection::open_with_flags`
(re-introduces the WAL-on-readonly bug `open_readonly` documents).

### `--since`/`--project` filter resolution (reuse unchanged)
**Source:** `crates/lacon-cli/src/commands/stats.rs:28-62`
**Apply to:** unchanged — `parse_since` → `cutoff_ms`, `normalize_project` →
`project_ref`, `filtered = cutoff_ms.is_some() || ...`. The new headline read picks
`filtered_overall_totals(conn, cutoff_ms, project_ref)` vs `overall_totals(conn)`
on the same `filtered` boolean, exactly as each section does.

### Bound SQL filters via `?N` placeholders (security — V5 / T-04-01)
**Source:** `crates/lacon-core/src/tracking/query.rs:338-349` (the binds-vec pattern,
reused in every `filtered_*` reader)
**Apply to:** `filtered_overall_totals` only. Filter values go through
`Vec<&dyn rusqlite::ToSql>` + `?{n}` placeholders; only static SQL fragments are
concatenated. Never string-interpolate `--since`/`--project` into the SQL.

### Inline `#[cfg(test)] mod tests` with `use super::{...}` (D-04 test home)
**Source:** `crates/lacon-cli/src/commands/stats.rs:299-358` and
`crates/lacon-cli/src/commands/explain.rs:305-380`
**Apply to:** all new private helpers in `stats.rs`. Extend the existing block's
`use super::{...}` line; do not create a new test module or a new file.

---

## No Analog Found

None. Every unit of work has an exact in-repo template (most are same-file siblings).
The only genuinely-new code surface is the scalar-collapse SQL in `overall_totals`
(`COALESCE(SUM,0)` + `query_row` over zero rows) — but even that is a three-line delta
from `filtered_project_savings`, and its full target body is already written verbatim
in 08-RESEARCH.md lines 225-277. The planner does not need RESEARCH.md's generic
"Code Examples" as a fallback for any unit; prefer the real-codebase analogs above.

## Metadata

**Analog search scope:** `crates/lacon-cli/src/commands/` (stats.rs, explain.rs),
`crates/lacon-cli/src/` (cli.rs, main.rs), `crates/lacon-core/src/tracking/`
(query.rs), `crates/lacon-cli/tests/` (cli_stats.rs), `crates/lacon-cli/Cargo.toml`.
**Files scanned:** 7 (4 modified targets + 3 confirmatory: explain.rs, main.rs, Cargo.toml).
**rusqlite confirmed dev-only:** `crates/lacon-cli/Cargo.toml:42` is under
`[dev-dependencies]` (line 23), not `[dependencies]` (line 12).
**Pattern extraction date:** 2026-05-23
