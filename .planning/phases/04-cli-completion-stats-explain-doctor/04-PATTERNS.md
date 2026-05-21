# Phase 4: CLI completion (`stats`, `explain`, `doctor`) - Pattern Map

**Mapped:** 2026-05-21
**Files analyzed:** 8 (3 new/extended in lacon-core, 3 stub fills in lacon-cli, 1 main.rs edit, 1 Cargo.toml edit) + 5 new test files
**Analogs found:** 8 / 8 (every file has a strong in-repo analog; nothing greenfield)

> **Posture for the planner:** Phase 4 is *composition*, not construction. Every behaviour has an existing analog in this workspace. The risk is re-implementing something slightly differently and drifting from the live runner (especially the exit-code branch). When in doubt, mirror the analog line-for-line.
>
> **ADR / constraint reminders that gate these patterns:**
> - All SQL stays behind the `lacon-core` boundary; `lacon-cli` keeps `rusqlite` **dev-only** (D-01, architecture.md). Do NOT add a runtime `rusqlite` dep to `lacon-cli`.
> - Query commands NEVER write: no migrate, no prune, no INSERT (D-02, ADR-0011). The read-only open helper is the enforcement point.
> - `thiserror` inside `lacon-core`, `anyhow` at the CLI boundary (Phase 1 D-03). Commands return `anyhow::Result<i32>` and exit with the returned code.
> - `explain` exit-code branch must mirror `runtime/mod.rs:342-359` byte-for-byte (D-04/D-05, ADR-0010) — Phase 6 reproducibility depends on it.
> - Six-command cap is a contract (REQ-cli-surface-cap, v1-scope.md). No 7th subcommand.

---

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/lacon-core/src/tracking/query.rs` (NEW) | service (read API) | CRUD (read) | `crates/lacon-core/src/tracking/record.rs` (write path) | role-match (read vs write, same `impl Tracker`/`&Connection` SQL boundary) |
| `crates/lacon-core/src/tracking/mod.rs` (EXTEND: `open_readonly`) | service / factory | request-response | `Tracker::open` @ `tracking/mod.rs:81-113` + `apply_connection_pragmas` @ `:131-160` | exact (sibling constructor) |
| `crates/lacon-core/src/runtime/mod.rs` (EXTEND: `filter_bytes`) | service | transform / batch | `Runner::run` exit-code branch @ `runtime/mod.rs:326-359` | exact (same exit-code branch, no spawn) |
| `crates/lacon-cli/src/commands/stats.rs` (FILL stub) | controller (command) | request-response (read) | `crates/lacon-cli/src/commands/validate.rs` (returns `Ok(i32)`, prints per-line) | role-match |
| `crates/lacon-cli/src/commands/explain.rs` (FILL stub) | controller (command) | request-response → transform | `crates/lacon-cli/src/commands/run.rs` (resolve rule + drive runtime + print) | role-match |
| `crates/lacon-cli/src/commands/doctor.rs` (FILL stub) | controller (command) | request-response (checklist) | `crates/lacon-cli/src/commands/init.rs` (JSON walk + per-step pass/fail + `Ok(1)`) | role-match |
| `crates/lacon-cli/src/main.rs` (EDIT: thread args) | route / dispatch | request-response | existing `main.rs:12-13` arms that already destructure (`Run`, `Validate`) | exact |
| `crates/lacon-cli/Cargo.toml` (EDIT: add `insta` dev-dep IF snapshots adopted) | config | — | workspace `[dev-dependencies]` block in `crates/lacon-cli/Cargo.toml:23-30` | exact |
| `crates/lacon-core/tests/tracking_query.rs` (NEW test) | test | CRUD | `crates/lacon-core/tests/tracking_record.rs` | exact |
| `crates/lacon-cli/tests/cli_stats.rs` / `cli_explain.rs` / `cli_doctor.rs` (NEW tests) | test | request-response | `crates/lacon-cli/tests/cli_surface.rs` (assert_cmd black-box) | exact |

---

## Pattern Assignments

### `crates/lacon-core/src/tracking/mod.rs` — ADD `open_readonly` (D-02)

**Analog:** `Tracker::open` @ `tracking/mod.rs:81-113` + `apply_connection_pragmas` @ `tracking/mod.rs:131-160`.

The new read-only opener is the **same shape as `Tracker::open` minus steps 4 (migrate) and 5 (prune)**, and it must NOT re-issue `journal_mode=WAL` (that pragma is a write — Pitfall 1). `apply_connection_pragmas` is `pub(crate)` so a new helper inside the `tracking` module can selectively reuse the safe pragmas (`busy_timeout`, `foreign_keys`) but must skip the WAL line.

**Open flags pattern to copy** (from `tracking/mod.rs:93-98`):
```rust
let mut conn = Connection::open_with_flags(
    db_path,
    OpenFlags::SQLITE_OPEN_READ_WRITE      // ← read-only variant swaps to SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_CREATE     // ← OMIT for read-only (do not create)
        | OpenFlags::SQLITE_OPEN_NO_MUTEX,
)?;
```

**WAL-pragma hazard to avoid** (`tracking/mod.rs:153-157`) — this block must NOT run on a read-only handle:
```rust
let mode: String = conn
    .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;  // ← WRITE; errors read-only
if mode.to_ascii_lowercase() != "wal" { return Err(TrackingError::WalRejected { actual_mode: mode }); }
```

**Wave-0 gate (Open Question 1 / Pitfall 1):** before committing to strict `SQLITE_OPEN_READ_ONLY`, smoke-test it against a WAL `history.db` created by `Tracker::open`. If it fails, the D-02 fallback is `SQLITE_OPEN_READ_WRITE` (no `CREATE`, no migrate, no prune) — the invariant "query commands never INSERT" still holds. The existing `crates/lacon-core/tests/wave0_smoke.rs` is the home for this spike.

**Error type:** return `Result<Connection, TrackingError>` (the existing tracking error enum already carries `Sqlite` via `#[from] rusqlite::Error` @ `error.rs:142-146`).

---

### `crates/lacon-core/src/tracking/query.rs` (NEW, D-01) — read API

**Analog:** `crates/lacon-core/src/tracking/record.rs` (the write-path sibling — *the* precedent for "all SQL behind the core boundary").

**Module-doc + SQL-boundary pattern** (mirror `record.rs:1-23`): a `//!` header explaining the read path, `use rusqlite::...`, errors as `TrackingError`. Free functions over `&Connection` are acceptable here (D-01 leaves method-vs-free-fn to discretion); `record.rs` uses `impl Tracker` because it needs `self.cfg_store_raw_outputs`, which a read path does not.

**Prepared-statement + parameterized-query pattern to copy** (`record.rs:113-121, 138-158`) — every read query MUST use `?N` placeholders, never string interpolation (security V5 / SQL-injection mitigation):
```rust
let mut stmt = self.conn.prepare_cached("INSERT INTO invocations ( ... ) VALUES (?1,?2,...)")?;
let id = stmt.insert(params![ /* positional values */ ])?;
```
For reads, the analog idiom is `conn.query_row(SQL, params![...], |r| r.get(N))` (used throughout: `prune.rs:52-57`, `health.rs:25`, and the test-side `tracking_record.rs:264-281`) and `stmt.query_map(...)` for multi-row result sets.

**Unfiltered sections read the four views directly** (DDL @ `tracking/migrations/0001_initial.sql:58-102`):
```sql
SELECT command_normalized, runs, total_raw_bytes FROM v_unmatched_offenders;
SELECT command_normalized, rule_id, runs, total_filtered_bytes, avg_keep_ratio FROM v_filtered_offenders;
SELECT rule_id, total, bypassed, bypass_rate FROM v_bypass_rate;
SELECT project_path, total_runs, raw_total, filtered_total, bytes_saved FROM v_project_savings;
```

**Filtered sections (`--project/--since/--rule`) re-query the BASE table, NOT the views (D-09).** Critical fact verified against `0001_initial.sql`: **no view exposes `ts`; only `v_project_savings` exposes `project_path`.** So filters cannot be expressed against views. Re-implement each view's `GROUP BY`/`ORDER BY` body against `invocations` with added `WHERE`. Indexes `idx_inv_ts`, `idx_inv_project`, `idx_inv_rule`, `idx_inv_cmd` (`0001_initial.sql:27-30`) make this cheap. Example (filtered "unmatched offenders"):
```sql
SELECT command_normalized, COUNT(*) AS runs,
       SUM(raw_stdout_bytes + raw_stderr_bytes) AS total_raw_bytes
FROM invocations
WHERE rule_id IS NULL AND bypassed = 0
  AND ts >= ?1            -- --since cutoff (unix MS — Pitfall 2)
  AND project_path = ?2   -- --project
GROUP BY command_normalized
ORDER BY total_raw_bytes DESC;
```

**explain's row lookup** (BLOB columns are `stdout`/`stderr` per `0001_initial.sql:32-38`):
```sql
SELECT rule_id, exit_code, command_raw, duration_ms, project_path, raw_output_id
FROM invocations WHERE id = ?1;
-- if raw_output_id NOT NULL:
SELECT stdout, stderr FROM raw_outputs WHERE id = ?1;   -- both BLOB
```

**Result-row organization (discretion, D-01):** prefer one small typed struct per view for readability (matches the project's `HealthReport`/`RawOutput` struct style in `health.rs:11-14` and `tracking/mod.rs:39-43`) over ad-hoc tuples.

---

### `crates/lacon-core/src/runtime/mod.rs` — ADD `Runner::filter_bytes` (D-04, D-05)

**Analog:** the exit-code branch + `ScriptCtx` assembly inside `Runner::run` @ `runtime/mod.rs:326-359`. This is the load-bearing fidelity point — `explain`'s replay MUST select the same branch the live runner did.

**ScriptCtx assembly to mirror** (`runtime/mod.rs:327-333`) — for replay, fields come from the **stored** row, not a live subprocess (`ScriptCtx` def @ `starlark_host/mod.rs:52-59`):
```rust
let ctx = ScriptCtx {
    exit_code,                                  // ← stored exit_code from invocations row
    duration_ms: started.elapsed()...,          // ← stored duration_ms from row
    command: argv[0].clone(),                    // ← derive from stored command_raw
    args: argv[1..].to_vec(),
    project_path: self.options.project_path...,  // ← stored project_path
};
```

**Exit-code branch to mirror EXACTLY** (`runtime/mod.rs:342-359`) — including the "no `on_error` block → raw passthrough" arm (ADR-0010):
```rust
let filtered = if exit_code == 0 {
    self.resolved.success_pipeline.run_with_post_process(
        raw_buffer.into_iter(), self.resolved.post_process.as_ref(), &ctx)?
} else if let Some(ref mut on_err) = self.resolved.on_error_pipeline {
    on_err.run_with_post_process(
        raw_buffer.into_iter(), self.resolved.on_error_post_process.as_ref(), &ctx)?
} else {
    raw_buffer   // ← ADR-0010: no on_error → raw passthrough; replay MUST preserve this
};
```

**Replay target (subprocess-free, already `pub`):** `Pipeline::run_with_post_process` @ `pipeline/mod.rs:127-138` — takes `lines: impl Iterator<Item = String>`, `post_process: Option<&StarlarkScript>`, `ctx: &ScriptCtx`, returns `Result<Vec<String>, RuntimeError>`. Do NOT call `Runner::run` (`runtime/mod.rs:162-203`) — it unconditionally spawns.

**Byte→line split:** mirror the runtime's own UTF-8-lossy approach (`runtime/mod.rs:265-270`: `String::from_utf8_lossy`) so replay matches the live reader. v1 merges stdout+stderr into one stream (`ByteCounts` comment @ `runtime/mod.rs:60-66`); the replay should feed the merged stored bytes through one line iterator.

**`ResolvedRule` public fields used by replay** (`loader.rs:59-77`): `success_pipeline`, `on_error_pipeline: Option<Pipeline>`, `post_process: Option<StarlarkScript>`, `on_error_post_process: Option<StarlarkScript>` — all `pub`, no accessor needed.

**Signature (discretion, D-04):** keep the exit-code branch + ScriptCtx assembly in this method so they are NOT duplicated into `lacon-cli`. Returns `Result<Vec<String>, RuntimeError>`.

---

### `crates/lacon-cli/src/commands/stats.rs` (FILL stub, D-09/D-10/D-11/D-12)

**Analog:** `crates/lacon-cli/src/commands/validate.rs` (the simplest "return `Ok(i32)`, print per-line" command) for the control-flow skeleton; `run.rs:239-258` for the DB-path-resolve + open pattern.

**Signature change (D-12):** `execute()` → `execute(project: Option<PathBuf>, since: Option<String>, rule: Option<String>)`. The clap fields already exist (`cli.rs:38-45`).

**Command skeleton to copy** (`validate.rs:15-29` shape — exists-check, work, per-line print, return code):
```rust
pub fn execute(/* threaded args */) -> anyhow::Result<i32> {
    // 1. resolve db_path via tracking::Tracker::xdg_db_path()  (run.rs:239-245)
    // 2. if !db_path.exists() → print "no data yet" per section, Ok(0)  (D-03)
    // 3. open read-only (D-02), call tracking::query fns, print plain-text tables
    Ok(0)
}
```

**DB-path resolution to copy** (`run.rs:239-245`):
```rust
let db_path = match tracking::Tracker::xdg_db_path() {
    Some(p) => p,
    None => { eprintln!("..."); return Ok(/* graceful */); }
};
```

**Missing-DB handling (D-03):** `stats` → friendly "no data yet" per section, `Ok(0)`. Check `db_path.exists()` BEFORE opening (Pitfall 4) — never let `SQLITE_CANTOPEN` bubble up.

**`--since` parser (D-10, Pitfall 2):** relative-only (`7d`/`24h`/`30m`) → cutoff in unix **milliseconds** (`now_ms - n*unit_ms`). `ts` is unix-MS (`tracking-data-model.md`, written as `now_ms` @ `run.rs:163-164`). Hand-rolled (no date crate). Unit-test the parser. The "now_ms" idiom to reuse (`run.rs:163-164`):
```rust
let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;
```

**Output:** plain-text, snapshot-testable (D-11). No color dep.

---

### `crates/lacon-cli/src/commands/explain.rs` (FILL stub, D-04/D-05/D-06)

**Analog:** `crates/lacon-cli/src/commands/run.rs` (resolve a rule via `RuleLoader`, drive the runtime, print) — explain is "run.rs but against stored bytes instead of a subprocess."

**Signature change (D-12):** `execute()` → `execute(id: String)`.

**id parse (D-05 step 1, Pitfall 3):** `id` is a clap `String` (`cli.rs:49`) but `invocations.id` is INTEGER. Parse `id.parse::<i64>()` early; on `Err`, print a clear message and return non-zero. Never `unwrap()` on user input (security V5/V7).

**Flow (D-05):**
1. parse `id` → `i64`.
2. open read-only (D-02); if no DB / no row → "no tracked invocations found", non-zero (D-03 / Pitfall 4).
3. `tracking::query` SELECT the invocation row → `rule_id, raw_output_id, exit_code, command_raw, duration_ms, project_path`.
4. if `raw_output_id IS NULL` → clear error pointing at `store_raw_outputs`, non-zero (D-05 step 3 / Pitfall 5 — **SC2's required failure path**, not an edge case).
5. load BLOBs from `raw_outputs`.
6. resolve rule via `RuleLoader::resolve(rule_id)` (`loader.rs:127-151`).
7. call `Runner::filter_bytes(...)` (the new core method) — exit-code branch lives in core (D-04).

**Rule-resolve pattern to copy** (`run.rs:26-35`):
```rust
let mut loader = RuleLoader::new(project_path.clone());
let resolved = match loader.resolve(&rule_id) {
    Ok(r) => r,
    Err(e) => { eprintln!("{}", e); return Ok(1); }
};
```

**Diff render (D-06):** hand-rolled two-column `raw | filtered` renderer. No LCS/Myers, no new diff crate. Escape hatch (`similar`, bundled transitively via `insta`) only if the hand-rolled view under-delivers — and any promotion runs the legitimacy gate.

---

### `crates/lacon-cli/src/commands/doctor.rs` (FILL stub, D-07/D-08)

**Analog:** `crates/lacon-cli/src/commands/init.rs` — same `.claude/settings.json` JSON-walk world, same per-step `Ok(1)`-on-failure convention, and doctor's hook check is the **read** half of init's **write**.

**Signature:** stays `execute() -> anyhow::Result<i32>`. Fixed checklist, one pass/fail line per item, `Ok(0)` iff all pass.

**Check 1 — hook install** (reuse the init fingerprint walk, `init.rs:136-147` + test helper `bash_lacon_commands` @ `init.rs:318-329`). Read `<cwd>/.claude/settings.json`, walk `hooks.PreToolUse[]` for a `"Bash"` matcher whose inner `command` `starts_with("lacon-claude-hook")`. The read-traversal idiom to copy (`init.rs:318-328`):
```rust
settings["hooks"]["PreToolUse"].as_array().into_iter().flatten()
    .filter(|g| g["matcher"] == "Bash")
    .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
    .filter_map(|h| h["command"].as_str())
    .filter(|c| c.starts_with("lacon-claude-hook"))
```
The fingerprint string `"lacon-claude-hook"` is the Phase 3 contract — init writes it (`init.rs:145, 166`), doctor reads it; they MUST stay in sync (A4).

**Settings read pattern to copy** (`init.rs:50-66`): read_to_string → `serde_json::from_str::<Value>` → handle `NotFound` as a normal state (informational per D-03, not red).

**Check 2 — config per layer:** `lacon_core::validate::validate_file(path)` (`validate/mod.rs:45`) on each existing `config.yaml`: project `<cwd>/.lacon/config.yaml` and user `~/.config/lacon/config.yaml`. Empty `Vec` = pass. Reuse the per-line print idiom from `validate.rs:24-26`. Resolve the user config dir via `etcetera` exactly as `run.rs:182-187`.

**Check 3 — rule sweep:** `RuleLoader::load_all()` (`loader.rs:156`) → `Err(Vec<ValidationError>)` lists every offending rule with its path. Construct loader as `run.rs:26`.

**Check 4 — DB dir perms:** `std::fs::metadata(parent_of_db).permissions().mode() & 0o777 == 0o700`. Dir is created `0700` by `ensure_data_dir` (`tracking/mod.rs:165-190` — the canonical perm logic). If DB/dir absent → informational, not failure (D-03).

**Check 5 — tracker health:** `lacon_core::tracking::health::health_check(&conn)` (`health.rs:24`) — a `SELECT 1` round-trip. Open the connection via the D-02 read-only helper (D-08). Phase 2 D-13 names doctor as its sole caller (`health.rs:1-3`).

---

### `crates/lacon-cli/src/main.rs` — thread args (D-12)

**Analog:** the arms that already destructure, `main.rs:12-13`:
```rust
CliCommand::Run { rule, argv } => commands::run::execute(rule, argv)?,
CliCommand::Validate { path } => commands::validate::execute(&path)?,
```
Change the two discarding arms (`main.rs:15-16`) from `{ .. }` to:
```rust
CliCommand::Stats { project, since, rule } => commands::stats::execute(project, since, rule)?,
CliCommand::Explain { id } => commands::explain::execute(id)?,
```
`Doctor` (`main.rs:17`) needs no change. The `std::process::exit(exit_code)` tail (`main.rs:19`) is the established exit convention.

---

### `crates/lacon-cli/Cargo.toml` — add `insta` dev-dep IF snapshots adopted (D-11)

**Analog:** the `[dev-dependencies]` block @ `Cargo.toml:23-30`. `insta` is already in `[workspace.dependencies]` (workspace `Cargo.toml`) but NOT yet listed under `lacon-cli`. Add via the workspace-inherit idiom used by every other dev-dep:
```toml
insta = { workspace = true }
```
This is the **only** Cargo.toml change Phase 4 may need. Do NOT add `rusqlite` to `[dependencies]` (D-01) — it stays dev-only (`Cargo.toml:30`).

---

## Shared Patterns

### Read-only DB open (cross-cutting for stats + explain + doctor)
**Source:** `Tracker::open` @ `tracking/mod.rs:81-113`, `apply_connection_pragmas` @ `tracking/mod.rs:131-160`.
**Apply to:** all three commands' DB touches.
**Rule:** open via the new D-02 helper; never call `Tracker::open` from a query command (it migrates + prunes = writes). Never re-issue `journal_mode=WAL` on a read-only handle.

### DB-path resolution + graceful skip
**Source:** `run.rs:239-245`.
```rust
let db_path = match tracking::Tracker::xdg_db_path() { Some(p) => p, None => { /* graceful */ } };
```
**Apply to:** stats, explain, doctor. Followed by `db_path.exists()` check BEFORE opening (per-command branch per D-03 / Pitfall 4).

### Parameterized SQL only
**Source:** `record.rs:113-121, 138-158` (write); `prune.rs:52-57`, `health.rs:25` (read).
**Apply to:** every query in `query.rs`. Use `?N` placeholders + `params![...]`; never interpolate user input (`--project`/`--rule`/`id`). Security V5 / SQL-injection mitigation.

### Error convention — `thiserror` in core, `anyhow` at CLI
**Source:** `error.rs` (`TrackingError`, `ValidationError`, `RuntimeError`); commands return `anyhow::Result<i32>` (`validate.rs:15`, `run.rs:19`, `init.rs:41`).
**Apply to:** `query.rs` returns `Result<_, TrackingError>`; `filter_bytes` returns `Result<_, RuntimeError>`; the three commands map errors to a printed message + an exit code (`Ok(0)`/`Ok(1)`/`Ok(2)`), never propagate a raw `?` to the user as a panic.

### CLI per-line error/section printing + exit codes
**Source:** `validate.rs:21-28` (per-line `eprintln!`, `Ok(0)`/`Ok(1)`); `init.rs:99-102` (success line + `Ok(0)`); `run.rs:21-22` (`Ok(2)` for usage errors).
**Apply to:** stats/explain/doctor human-readable output.

### settings.json JSON-walk (read side)
**Source:** `init.rs:318-329` (`bash_lacon_commands`) — the read traversal; `init.rs:50-66` — the read+parse+NotFound handling.
**Apply to:** doctor check 1. Fingerprint `"lacon-claude-hook"` must match init exactly.

### Test conventions
**Source (core, DB-seeding):** `crates/lacon-core/tests/tracking_record.rs` — `setup_db_path()` tempdir helper (`:19-23`), `Tracker::open` to seed (`:65`), `sample_meta(...)` builder (`:32-55`), `conn.query_row` assertions (`:264-281`), `FIXED_NOW_MS` constant (`:17`). Use this exact shape for `tests/tracking_query.rs` (seed via `Tracker::record`, then assert via `tracking::query`).
**Source (CLI, black-box):** `crates/lacon-cli/tests/cli_surface.rs` — `assert_cmd::Command::cargo_bin("lacon")`, `.assert().success()/.failure()`, `predicates::str::contains`. Use for `cli_stats.rs` / `cli_explain.rs` / `cli_doctor.rs`. Seed test DBs via the dev-only `rusqlite` (`Cargo.toml:30`) into a tempdir; point the binary at it via env (the e2e tests under `crates/lacon-cli/tests/tracking_e2e.rs` already use `assert_cmd` + tempdir + env for DB isolation — reuse that fixture approach). The 6-command-cap test (`cli_surface.rs`) stays green untouched (D-13).
**Source (inline unit tests):** `#[cfg(test)] mod tests` at the bottom of the module (`init.rs:314-474`, `pipeline/mod.rs:188-278`, `validate/mod.rs:194-277`) — use for the `--since` parser and `filter_bytes` branch-fidelity unit tests.
**Snapshots:** `insta` (D-11) for plain-text output — currently unused anywhere; add the dev-dep line to `lacon-cli` if adopted.

---

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| (none) | — | — | Every Phase 4 file has a strong in-repo analog. The only genuinely new code shapes — the read-only opener, the byte-replay method, the hand-rolled `--since` parser, the two-column diff — are each a *minus-some-steps* variant of an existing function (`Tracker::open`, `Runner::run` branch, the `now_ms` math, plain-text printing). RESEARCH.md's "Code Examples" provide the SQL bodies if a planner wants a literal starting point, but the structural patterns above all come from live code. |

> Note: `insta` snapshot tests have **zero existing usages** in the repo (verified in RESEARCH). The dependency exists in `[workspace.dependencies]` but no `insta::` call site exists to copy. If adopted, follow the standard `insta::assert_snapshot!(captured_stdout)` form against `assert_cmd`-captured output; otherwise plain `predicates`/`assert_eq!` on stdout (as `cli_surface.rs` does) is the established fallback.

---

## Metadata

**Analog search scope:** `crates/lacon-core/src/{tracking,runtime,pipeline,rules,validate}`, `crates/lacon-cli/src/{commands,cli.rs,main.rs}`, `crates/lacon-cli/tests/`, `crates/lacon-core/tests/`, `crates/lacon-core/src/tracking/migrations/0001_initial.sql`, workspace + member `Cargo.toml`.
**Files scanned (read in full or targeted):** 18 source/test/manifest/SQL files.
**Skills directory:** none present (`.claude/skills/` and `.agents/skills/` absent).
**Pattern extraction date:** 2026-05-21
