# Phase 4: CLI completion (`stats`, `explain`, `doctor`) - Research

**Researched:** 2026-05-21
**Domain:** Rust CLI command implementation against an existing SQLite tracking layer + rule engine (no new external libraries)
**Confidence:** HIGH

## Summary

Phase 4 fills the three remaining stub CLI commands (`stats`, `explain`, `doctor`) and hard-confirms the six-command surface. This is **not** a greenfield phase: every API the three commands need already exists in `lacon-core` (loader, pipeline, validate, health, tracking writer) or in `lacon-cli` (init hook fingerprint, clap surface). The work is plumbing — adding a *read* surface to the tracking layer, a *byte-replay* entry point to the runtime, and three `execute()` bodies — using libraries already in the workspace. No new runtime dependency is warranted (the diff renderer is hand-rolled per D-06).

I verified all 13 locked CONTEXT decisions against the live codebase. **All 13 hold** — file paths, line numbers, signatures, and the four view DDLs match what CONTEXT.md asserts. The single most load-bearing finding for the planner: **none of the four views expose a `ts`, and only one (`v_project_savings`) exposes `project_path`** — so the `--project/--since/--rule` filters MUST re-query the base `invocations` table, exactly as D-09 specifies. The base table has the indexes (`idx_inv_ts`, `idx_inv_project`, `idx_inv_rule`) to make those filtered queries cheap.

**Primary recommendation:** Implement against the existing in-repo APIs verbatim (signatures documented below). Add exactly two new pieces to `lacon-core`: a read-only DB open helper (D-02) and a `Runner` byte-replay method (D-04). Everything else is composition. Use `insta` (already a workspace dev-dep) for snapshot-testing the human-readable output. Watch the one real technical risk: **read-only SQLite open of a WAL database** (see Pitfall 1).

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| `stats` aggregation queries | lacon-core (tracking read API) | lacon-cli (formatting) | D-01 — all SQL behind the core boundary; CLI keeps `rusqlite` dev-only |
| `stats` filter resolution (`--project/--since/--rule`) | lacon-core (query.rs) | — | Parameterized base-table re-queries; SQL never leaves core |
| `explain` byte-replay | lacon-core (runtime + pipeline) | lacon-cli (diff render) | D-04 — exit-code branch + ScriptCtx assembly stay in the runtime that authored them |
| `explain` raw-vs-filtered diff render | lacon-cli | — | D-06 — hand-rolled presentation, pure formatting, no core concern |
| `doctor` hook-install check | lacon-cli | — | Reads `<cwd>/.claude/settings.json`; mirrors init.rs JSON-walk |
| `doctor` config/rule validation | lacon-core (validate + loader) | lacon-cli (orchestration) | Reuses `validate_file` / `load_all` |
| `doctor` DB perms + health | lacon-cli (fs::metadata) + lacon-core (health_check) | — | Perms via std::fs; health via core read-only probe |
| Six-command cap | lacon-cli (clap) | — | Already enforced by `cli.rs` + `cli_surface.rs` test |

## Standard Stack

This phase introduces **no new external crates**. Everything is already in `Cargo.toml [workspace.dependencies]` and verified present.

### Core
| Library | Version (workspace) | Purpose | Why Standard |
|---------|---------------------|---------|--------------|
| `rusqlite` | `0.39` (`bundled`) | All SQLite access (read path) | Already the project's DB layer (ADR-0011); `bundled` ships SQLite, no system dep `[VERIFIED: Cargo.toml, in-repo use]` |
| `clap` | `4` (`derive`) | CLI surface (already declares all 6 cmds) | Existing parser; no change needed `[VERIFIED: cli.rs]` |
| `anyhow` | `1` | CLI-boundary error type | Phase 1 D-03 established pattern `[VERIFIED: main.rs, run.rs]` |
| `thiserror` | `2` | Typed errors inside `lacon-core` | Phase 1 D-03 `[VERIFIED: error.rs use]` |
| `etcetera` | `0.11` | XDG path resolution (DB path, config dirs) | Already used by `Tracker::xdg_db_path`, `RuleLoader` `[VERIFIED: tracking/mod.rs:120-125]` |

### Supporting (dev-only — testing)
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| `insta` | `1` | Snapshot tests for `stats`/`doctor`/`explain` text output | D-11 — output is plain-text and snapshot-testable; `insta` already a workspace dev-dep but **not yet used anywhere** `[VERIFIED: grep found zero `insta::` uses]` |
| `assert_cmd` | `2` | Black-box CLI invocation tests | Already used by `cli_surface.rs`, `cli_run.rs` `[VERIFIED]` |
| `predicates` | `3` | Assertions on CLI stdout/exit | Pairs with `assert_cmd` `[VERIFIED: cli_surface.rs]` |
| `tempfile` | `3` | Isolated temp dirs/DBs in tests | Already used across tracking tests `[VERIFIED]` |
| `rusqlite` (dev) | `0.39` | Seed test DBs directly in `lacon-cli` tests | **Already a dev-dependency** in `crates/lacon-cli/Cargo.toml:28-30` (Phase 2 e2e). Keep dev-only — D-01 forbids a runtime dep `[VERIFIED: Cargo.toml]` |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| Hand-rolled side-by-side diff (D-06) | `similar` crate (de-facto Rust diff, bundled transitively via `insta`) | `similar` gives true LCS/Myers alignment but adds a first-class `lacon-cli` dependency. CONTEXT D-06 makes it an **escape hatch only** — adopt solely if the hand-rolled raw\|filtered view under-delivers. Not needed for SC2. |
| Relative `--since` parser (D-10) | `chrono` / `time` for absolute ISO dates | Neither date crate is a workspace dep; pulling one in for v1 is out of scope. Relative-only (`7d`/`24h`/`30m`) is a tiny hand-rolled parser. ISO `--since` is explicitly backlog. |
| Adding `rusqlite` to lacon-cli runtime deps | Read API in `lacon-core::tracking::query` | D-01 locks SQL behind the core boundary. A runtime `rusqlite` in lacon-cli is a regression the plan must NOT introduce. |

**Installation:** None. No `cargo add`. If `insta` snapshot tests are added, the dependency line already exists in `[workspace.dependencies]`; member crates inherit via `insta = { workspace = true }` under `[dev-dependencies]` (lacon-cli does not yet list it — adding the dev-dependency line is the only Cargo.toml change this phase may need).

**Version verification:** All versions read directly from the in-repo `Cargo.toml [workspace.dependencies]` block on 2026-05-21. No registry lookups required — nothing new is being installed.

## Package Legitimacy Audit

> This phase installs **no new external packages**. All dependencies are already vendored in the workspace `Cargo.toml` and in active use. slopcheck/registry verification is N/A because there is nothing new to verify.

| Package | Registry | Disposition |
|---------|----------|-------------|
| (none new) | — | No installs this phase |

**Packages removed due to slopcheck [SLOP] verdict:** none
**Packages flagged as suspicious [SUS]:** none

*If a planner later decides to promote `similar` from escape-hatch to first-class (D-06), THAT install must run the legitimacy gate — `similar` is a real, widely-used crate (it backs `insta`) but the gate still applies to any new direct dependency.*

## Architecture Patterns

### System Architecture Diagram

```
                         lacon <subcommand>  (clap parse, main.rs)
                                   │
        ┌──────────────────────────┼───────────────────────────┐
        ▼                          ▼                            ▼
   ┌─────────┐               ┌──────────┐                ┌──────────┐
   │  stats  │               │ explain  │                │  doctor  │
   └────┬────┘               └────┬─────┘                └────┬─────┘
        │ project/since/rule       │ id:i64                    │ fixed checklist
        ▼                          ▼                           ▼
  ┌───────────────┐    ┌──────────────────────┐   ┌─────────────────────────┐
  │ tracking::    │    │ SELECT invocation row │   │ 1 hook: read .claude/    │
  │ query (NEW)   │    │ → rule_id, raw_out_id,│   │   settings.json walk     │
  │ open_readonly │    │   exit_code, ctx flds │   │ 2 config: validate_file  │
  │ (D-02, NEW)   │    │ raw_output_id NULL?   │   │   per layer (proj+user)  │
  └──────┬────────┘    │  → SC2 error path     │   │ 3 rules: load_all() sweep│
         │             └──────────┬───────────┘   │ 4 perms: fs::metadata 0700│
   ┌─────┴──────┐                 │ load BLOBs     │ 5 health: health_check    │
   │ unfiltered │                 ▼ from raw_outputs│   (read-only open, D-08) │
   │  → 4 views │    ┌──────────────────────────┐  └────────────┬────────────┘
   │ filtered   │    │ RuleLoader::resolve(id)   │               │
   │  → base    │    │  → ResolvedRule           │               ▼
   │  invocations│   │ exit_code branch (mirror  │        one pass/fail line
   │  WHERE …    │   │  runtime/mod.rs:342-359)  │        per check; exit 0 iff
   └─────┬──────┘    │ Pipeline::                │        all pass
         │           │  run_with_post_process    │
         ▼           │  (no subprocess) (D-04)   │
   plain-text        └──────────┬───────────────┘
   sections                     ▼
   exit 0             raw | filtered (hand-rolled
                      two-column render, D-06)
```

The read path NEVER spawns a subprocess and NEVER writes to the DB (no migrate, no prune, no INSERT). That is the load-bearing invariant for all three commands.

### Recommended Project Structure
```
crates/lacon-core/src/tracking/
├── mod.rs          # ADD: open_readonly helper (D-02) next to Tracker::open
├── query.rs        # NEW (D-01): read API — typed result rows, free fns over &Connection
├── health.rs       # REUSE: health_check(&Connection) — doctor calls this
└── record.rs       # untouched (write path; read precedent for "all SQL in core")

crates/lacon-core/src/runtime/
└── mod.rs          # ADD: Runner::filter_bytes (D-04) — byte-replay, no spawn

crates/lacon-cli/src/commands/
├── stats.rs        # FILL stub → execute(project, since, rule)
├── explain.rs      # FILL stub → execute(id)
└── doctor.rs       # FILL stub → execute()

crates/lacon-cli/src/main.rs   # thread Stats/Explain args (currently `{ .. }`) — D-12
```

### Pattern 1: Read-only DB open (D-02)
**What:** A query-command DB open that applies WAL/pragmas but does NOT migrate or prune.
**When to use:** `stats`, `explain`, `doctor` — every Phase 4 DB touch.
**Verified context:** `Tracker::open` (tracking/mod.rs:81-113) unconditionally runs `migrations::migrate(&mut conn)` then `prune::prune_if_due(&conn, …)` — both are **writes**. A query command must not run them. `apply_connection_pragmas` (tracking/mod.rs:131-160) is `pub(crate)` and can be reused inside core for the new helper.
```rust
// Source: derived from in-repo tracking/mod.rs:81-160 (verified signatures)
// Add to lacon-core::tracking — free fn or Tracker assoc fn (D-01 leaves this to planner).
pub fn open_readonly(db_path: &Path) -> Result<Connection, TrackingError> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    // NOTE: do NOT call pragma_update journal_mode=WAL on a read-only handle
    // (it would attempt a write). See Pitfall 1. busy_timeout + foreign_keys are safe.
    Ok(conn)
}
```

### Pattern 2: Byte-replay for `explain` (D-04, D-05)
**What:** Re-derive filtered output from stored stdout/stderr bytes without spawning.
**Verified context:** `Pipeline::run_with_post_process` (pipeline/mod.rs:127-138) is **`pub`**, takes `lines: impl Iterator<Item = String>`, `post_process: Option<&StarlarkScript>`, `ctx: &ScriptCtx`, returns `Result<Vec<String>, RuntimeError>` — subprocess-free. The exit-code branch the runtime uses lives at runtime/mod.rs:342-359; `ScriptCtx` is built at runtime/mod.rs:327-333 with fields `{ exit_code, duration_ms, command, args, project_path }`. `ResolvedRule` exposes `success_pipeline`, `on_error_pipeline: Option<Pipeline>`, `post_process: Option<StarlarkScript>`, `on_error_post_process: Option<StarlarkScript>` as **public fields** (loader.rs:59-77).
```rust
// Source: composed from verified runtime/mod.rs:327-359 + pipeline/mod.rs:127-138
// Add Runner::filter_bytes so exit-code branch + ScriptCtx assembly stay in core.
// Merge stored stdout+stderr into the same line stream the live runner produced
// (v1 uses a single merged pipe — see runtime/mod.rs:62-66 ByteCounts comment).
let lines = merged_bytes.split(|&b| b == b'\n')
    .map(|l| String::from_utf8_lossy(l).into_owned());
let ctx = ScriptCtx { exit_code: stored_exit, duration_ms: stored_dur,
                      command, args, project_path };
let filtered = if stored_exit == 0 {
    resolved.success_pipeline.run_with_post_process(
        lines, resolved.post_process.as_ref(), &ctx)?
} else if let Some(ref mut on_err) = resolved.on_error_pipeline {
    on_err.run_with_post_process(
        lines, resolved.on_error_post_process.as_ref(), &ctx)?
} else {
    lines.collect() // ADR-0010: no on_error block → raw passthrough (mirror :354-358)
};
```
**Anti-pattern avoided:** Do NOT call `Runner::run` — it always spawns (runtime/mod.rs:162-203). Confirmed: there is no existing `filter_bytes`/replay entry point; it must be added.

### Pattern 3: Filtered stats re-query the base table, NOT the views (D-09)
**What:** `--project/--since/--rule` applied as parameterized `WHERE` clauses on `invocations`, re-implementing each view's GROUP BY/ORDER BY body.
**Verified context (the four view DDLs, byte-exact from `docs/specs/tracking-data-model.md:96-142` AND `migrations/0001_initial.sql:58-102`):**

| View | Columns it exposes | Has `ts`? | Has `project_path`? | Has `rule_id`? |
|------|-------------------|-----------|---------------------|----------------|
| `v_unmatched_offenders` | `command_normalized, runs, total_raw_bytes` | ✗ | ✗ | ✗ (filters `rule_id IS NULL`) |
| `v_filtered_offenders` | `command_normalized, rule_id, runs, total_filtered_bytes, avg_keep_ratio` | ✗ | ✗ | ✓ |
| `v_bypass_rate` | `rule_id, total, bypassed, bypass_rate` (+ `HAVING COUNT(*) > 5`) | ✗ | ✗ | ✓ |
| `v_project_savings` | `project_path, total_runs, raw_total, filtered_total, bytes_saved` | ✗ | ✓ | ✗ |

Because **no view carries `ts`** and only `v_project_savings` carries `project_path`, any filter on `--since` (a `ts` cutoff) or `--project` cannot be expressed against the views. The base `invocations` table has all columns plus indexes `idx_inv_ts`, `idx_inv_project`, `idx_inv_rule`, `idx_inv_cmd` (0001_initial.sql:27-30), so filtered re-queries are cheap. **Unfiltered** sections may still read straight from the views.

### Pattern 4: Doctor checklist (D-07) — reuse, don't reinvent
Each check maps to an existing verified API:
1. **Hook install** — read `<cwd>/.claude/settings.json`, walk `hooks.PreToolUse[]` for a Bash matcher whose inner `command` starts with `"lacon-claude-hook"`. Reuse the exact walk shape from `init.rs` (the scrub loop at the verified `pretool_arr.iter_mut()` block uses `cmd.starts_with("lacon-claude-hook")`; the test helper `bash_lacon_commands` shows the read traversal). The fingerprint string `"lacon-claude-hook"` is the Phase 3 contract — init writes it, doctor reads it; they must agree.
2. **Config per layer** — `validate::validate_file(path) -> Vec<ValidationError>` (validate/mod.rs:45) on each existing `config.yaml`: project `<cwd>/.lacon/config.yaml` and user `~/.config/lacon/config.yaml`. Empty `Vec` = pass. Required by config-schema.md:119 ("doctor runs config validation on every layer's config.yaml").
3. **Rule sweep** — `RuleLoader::load_all(&mut self) -> Result<Vec<ResolvedRule>, Vec<ValidationError>>` (loader.rs:156). `Err(vec)` lists every offending rule with its path.
4. **DB dir perms** — `std::fs::metadata(parent_of_db).permissions().mode() & 0o777 == 0o700`. The dir is created `0700` by `ensure_data_dir` (tracking/mod.rs:165-190). If the DB/dir doesn't exist yet → informational, not a failure (D-03).
5. **Tracker health** — `tracking::health::health_check(&Connection) -> Result<HealthReport, TrackingError>` (health.rs:24) — a `SELECT 1` round-trip. Phase 2 D-13 names doctor as its sole caller. Open the connection read-only (D-08).

### Anti-Patterns to Avoid
- **Calling `Tracker::open` for query commands** — it migrates + prunes (writes). Use the read-only helper (D-02).
- **Filtering the four views for `--since`/`--project`** — impossible (columns absent). Re-query the base table (D-09).
- **Re-spawning the command in `explain`** — replay stored bytes (D-04).
- **Inlining SQL in lacon-cli** — D-01 violation; all SQL lives in `tracking::query`.
- **Treating missing `history.db` as an error** — it's the normal fresh-user state (D-03).
- **Adding a 7th subcommand or a runtime `rusqlite` dep to lacon-cli** — breaks SC4 / D-01.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Re-deriving filtered output | A second pipeline driver in lacon-cli | `Pipeline::run_with_post_process` (pub, verified) | Byte-for-byte fidelity with the live runner; Phase 6 reproducibility (SC3) depends on it |
| Exit-code branch selection in explain | Copy of the if/else into lacon-cli | `Runner::filter_bytes` in core (mirror :342-359) | D-04 keeps branch + ScriptCtx colocated with the runtime; avoids drift |
| Config validation | A YAML re-parser in doctor | `validate::validate_file` | Same validator `lacon validate` uses; one source of truth |
| Rule load errors | A rule directory walker | `RuleLoader::load_all` | Already handles 3-layer walk, extends flatten, regex compile |
| DB health probe | Custom `SELECT` | `health_check` | Built in Phase 2 specifically for doctor (D-13) |
| Hook detection | A new settings.json schema | Reuse init.rs `lacon-claude-hook` fingerprint walk | init and doctor must agree on the marker |
| Diff alignment (only if needed) | A custom LCS | `similar` (escape hatch, D-06) | Only if the hand-rolled raw\|filtered view under-delivers |

**Key insight:** Phase 4 is composition, not construction. The risk is *re-implementing* something that already exists slightly differently and introducing drift (especially the exit-code branch). Every "build" instinct here has an existing core API.

## Runtime State Inventory

> This is a code-completion phase (filling stubs), not a rename/refactor/migration. **Section omitted** — no stored data, live-service config, OS-registered state, secrets, or build artifacts are renamed or migrated. The only persistent state touched is the existing `history.db`, and it is touched **read-only**.

## Common Pitfalls

### Pitfall 1: Read-only SQLite open of a WAL database
**What goes wrong:** Opening `history.db` with `SQLITE_OPEN_READ_ONLY` can fail or behave surprisingly when the database is in WAL mode (it is — `Tracker::open` sets `journal_mode=WAL`, tracking/mod.rs:153-157). A pure read-only handle cannot create or write the `-shm`/`-wal` sidecar files, and SQLite historically requires write access to those to read a WAL database; also, **never** issue `PRAGMA journal_mode=WAL` on a read-only handle (it's a write and will error).
**Why it happens:** WAL needs shared-memory coordination; a strict `SQLITE_OPEN_READ_ONLY` open assumes it may not touch the filesystem beyond the main db file.
**How to avoid:**
- Do NOT re-run `journal_mode=WAL` in the read-only helper (the existing `apply_connection_pragmas` does this — so the helper must NOT call it; only `busy_timeout` and `foreign_keys` are safe, and `foreign_keys` is irrelevant for pure reads).
- Confirm during Wave 0 whether a strict read-only open succeeds on a WAL db created by `Tracker::open`. If it fails on the target platform, the documented D-02 fallback applies: reuse a read-write open (`SQLITE_OPEN_READ_WRITE`, no CREATE) but **without** calling `migrate` or `prune` — i.e. "doesn't write an `invocations` row, doesn't depend on prune side-effects." Either way the invariant ("query commands never INSERT") holds.
**Warning signs:** `SQLITE_CANTOPEN` / `SQLITE_READONLY` on open or first query against a freshly-written WAL db. `[ASSUMED]` that strict read-only may fail on WAL — this is a known SQLite behavior pattern but **must be verified in Wave 0** against this exact build (libsqlite3-sys 0.37 / rusqlite 0.39); do not assume either outcome.

### Pitfall 2: `--since` cutoff arithmetic in unix-ms
**What goes wrong:** `invocations.ts` is unix epoch **milliseconds** (tracking-data-model.md:17, written as `now_ms` in run.rs). A relative `--since 7d` must be resolved to `now_ms - 7*86400*1000`, not seconds.
**How to avoid:** Compute cutoff in ms; compare `ts >= cutoff`. Unit-test the parser with `7d`/`24h`/`30m` (D-10 grammar is minimal — start there; combined forms like `1d12h` are discretion).
**Warning signs:** Off-by-1000 results (filtering returns everything or nothing).

### Pitfall 3: `explain <id>` argument parsing
**What goes wrong:** clap declares `Explain { id: String }` (cli.rs:49) — `id` is a **String**, but `invocations.id` is INTEGER. Non-numeric input must produce a clean message, not a panic.
**How to avoid:** Parse `id.parse::<i64>()` early; on `Err`, print a clear "invalid invocation id" message and exit non-zero (D-05 step 1).
**Warning signs:** `unwrap()` on parse; cryptic error on `lacon explain abc`.

### Pitfall 4: Missing-DB / empty-DB handling differs per command
**What goes wrong:** Treating "no `history.db`" uniformly. D-03 specifies three different behaviors: `stats` → friendly "no data yet" per section, exit 0; `explain <id>` → clear "no tracked invocations found" error, non-zero; `doctor` → "not yet initialized" informational, NOT a red failure.
**How to avoid:** Check DB existence before opening; branch per command. Don't let a `CANTOPEN` bubble up as a generic error.
**Warning signs:** A fresh user (never ran `lacon run`) sees a stack-trace-like error from `stats` or a red `doctor`.

### Pitfall 5: `explain` raw-retention-disabled path (SC2)
**What goes wrong:** Forgetting that `raw_output_id` is NULL by default (ADR-0009, raw_outputs off by default). `explain` cannot replay without stored bytes.
**How to avoid:** After SELECTing the invocation row, if `raw_output_id IS NULL` → exit with a clear error pointing at `store_raw_outputs` (D-05 step 3). This is SC2's **required** failure path — it's a feature, not an edge case.
**Warning signs:** Null-deref / empty replay when raw retention was off.

### Pitfall 6: `explain` exit-code branch must mirror the runtime exactly
**What goes wrong:** Replaying always through `success_pipeline`, ignoring the stored `exit_code`. A run that exited non-zero went through `on_error_pipeline` originally (ADR-0010); replaying it through the success pipeline produces different output → Phase 6 reproducibility (SC3) fails.
**How to avoid:** Select branch by the **stored** `exit_code` (D-05 step 6), mirroring runtime/mod.rs:342-359 — including the "no on_error block → raw passthrough" case. This is exactly why D-04 puts `filter_bytes` in core.
**Warning signs:** `explain` output for a failed run differs from what the user originally saw.

### Pitfall 7: Spec path is `docs/specs/` (plural)
**What goes wrong:** CONTEXT.md `<canonical_refs>` lists `docs/spec/tracking-data-model.md` (singular) in one heading line. The actual directory is `docs/specs/` (plural) — verified. All four spec files live there.
**How to avoid:** Read `docs/specs/tracking-data-model.md`. Minor, but a downstream agent following the singular path verbatim would 404.

## Code Examples

### Verified: the four view columns available to `stats` (unfiltered sections)
```sql
-- Source: docs/specs/tracking-data-model.md:96-142 == migrations/0001_initial.sql:58-102 (byte-exact)
-- Unfiltered stats may read these directly:
SELECT command_normalized, runs, total_raw_bytes FROM v_unmatched_offenders;          -- top unmatched offenders
SELECT command_normalized, rule_id, runs, total_filtered_bytes, avg_keep_ratio
  FROM v_filtered_offenders;                                                            -- filtered offenders
SELECT rule_id, total, bypassed, bypass_rate FROM v_bypass_rate;                       -- bypass smell (HAVING COUNT(*)>5)
SELECT project_path, total_runs, raw_total, filtered_total, bytes_saved
  FROM v_project_savings;                                                               -- per-project savings
```

### Verified: filtered re-query against the base table (D-09)
```sql
-- Source: derived from view bodies + invocations schema (0001_initial.sql:6-30)
-- Example: filtered "unmatched offenders" with --since + --project applied.
SELECT command_normalized,
       COUNT(*) AS runs,
       SUM(raw_stdout_bytes + raw_stderr_bytes) AS total_raw_bytes
FROM invocations
WHERE rule_id IS NULL AND bypassed = 0
  AND ts >= ?1            -- --since cutoff (unix ms); uses idx_inv_ts
  AND project_path = ?2   -- --project; uses idx_inv_project
GROUP BY command_normalized
ORDER BY total_raw_bytes DESC;
-- --rule narrows the rule_id-bearing views similarly (uses idx_inv_rule).
```

### Verified: explain reads the stored BLOBs
```sql
-- Source: raw_outputs schema (0001_initial.sql:32-38) + record.rs INSERT (verified)
SELECT i.rule_id, i.exit_code, i.command_raw, i.duration_ms, i.project_path, i.raw_output_id
FROM invocations i WHERE i.id = ?1;
-- if raw_output_id NOT NULL:
SELECT stdout, stderr FROM raw_outputs WHERE id = ?1;   -- stdout/stderr are BLOB
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `stats`/`explain`/`doctor` print "not yet implemented" exit 2 | Full implementations | Phase 4 (this) | The three stub bodies are replaced `[VERIFIED: stats.rs/explain.rs/doctor.rs all stubs]` |
| No read API in tracking layer | `tracking::query` module (D-01) | Phase 4 | Phase 2 deliberately shipped write-only; Phase 4 adds reads |
| Runtime only spawns | + byte-replay (`Runner::filter_bytes`) | Phase 4 | Enables `explain` and Phase 6 reproducibility |

**Deprecated/outdated:** Nothing. This phase only adds; it changes no existing contract. The `stats::execute()`/`explain::execute()` signatures change (D-12) to accept their already-parsed clap args — that's an internal CLI signature, not a public contract.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | Strict `SQLITE_OPEN_READ_ONLY` open *may* fail on the WAL-mode `history.db` on this build | Pitfall 1, D-02 | If it actually works fine, the read-only helper is simpler than feared (good). If it fails and isn't caught in Wave 0, all three commands break at runtime on a real WAL db. **Must verify in Wave 0.** |
| A2 | A relative-only `--since` parser is sufficient for v1 (no date crate) | D-10, Pitfall 2 | If users need ISO dates, they're blocked — but D-10 explicitly defers ISO to backlog, so this is a confirmed scope decision, not an open risk. |
| A3 | `insta` is the right tool for output snapshot tests | Validation Architecture | Low risk — it's already a workspace dev-dep and standard for Rust text-output snapshots. If undesired, plain `assert_eq!` on captured stdout works. |
| A4 | The `lacon-claude-hook` fingerprint string is stable between Phase 3 (init) and Phase 4 (doctor) | Pattern 4 | If Phase 3 ever changed the marker, doctor's check would false-negative. Verified identical in init.rs today; downstream must keep them in sync. |

**Note:** This table is short because nearly everything was *verified* against the live codebase rather than assumed. A1 is the one finding that genuinely needs a runtime check before the plan commits to the strict read-only open.

## Open Questions

1. **Does strict read-only open work on the WAL `history.db`?**
   - What we know: `Tracker::open` sets `journal_mode=WAL`; the read-only helper must not re-set it.
   - What's unclear: whether `SQLITE_OPEN_READ_ONLY` alone can read a WAL db created by another process on ext4/macOS with this rusqlite build.
   - Recommendation: Wave 0 smoke test — create a DB via `Tracker::open`, write a row, then open read-only and `SELECT 1` + a view query. If it fails, use the D-02 fallback (read-write open, no migrate/no prune). Decide before writing `query.rs`.

2. **`LACON_TOOL_USE_ID` correlation for `explain` (Phase 3 D-26 trailing item)** — discretion (CONTEXT). 
   - Recommendation: skip unless a stored tool-use-id demonstrably strengthens `explain`'s lookup. Default: `id` = `invocations.id` integer. No schema column exists for it today (verified — `invocations` has no tool_use_id column), so adopting it would require a migration, which is out of scope unless justified.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | Build/test | ✓ | rustc 1.95.0 / cargo 1.95.0 (MSRV 1.80) | — |
| `rustfmt`, `clippy` | Lint | ✓ | via rust-toolchain.toml (stable) | — |
| SQLite (runtime) | tracking read path | ✓ (bundled) | via `rusqlite` `bundled` feature — no system dep | — |
| `sqlite3` CLI | Test fixtures (optional) | ✓ | 3.46.1 | rusqlite dev-dep can seed DBs in-process |
| `cargo-nextest` | Faster test runs (optional) | ✗ | — | plain `cargo test` works |

**Missing dependencies with no fallback:** none — every requirement is satisfied.
**Missing dependencies with fallback:** `cargo-nextest` absent → use `cargo test` (the existing test suite uses it).

## Validation Architecture

> `workflow.nyquist_validation` is `true` in `.planning/config.json` — section included.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + `assert_cmd`/`predicates` (black-box CLI) + `insta` (snapshot, dev-dep, not yet used) |
| Config file | none — standard `cargo test` (no nextest config) |
| Quick run command | `cargo test -p lacon-cli stats explain doctor surface` (filter by name) |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| REQ-cli-stats | top offenders/bypass/unmatched from 4 views | integration | `cargo test -p lacon-cli cli_stats` | ❌ Wave 0 — `tests/cli_stats.rs` |
| REQ-cli-stats | `--project/--since/--rule` narrow correctly (base-table re-query) | integration | `cargo test -p lacon-cli cli_stats::filters` | ❌ Wave 0 |
| REQ-cli-stats | empty-DB → "no data yet" exit 0 (D-03) | integration | `cargo test -p lacon-cli cli_stats::empty_db` | ❌ Wave 0 |
| REQ-cli-explain | replay stored bytes → raw\|filtered side-by-side | integration | `cargo test -p lacon-cli cli_explain` | ❌ Wave 0 — `tests/cli_explain.rs` |
| REQ-cli-explain | `raw_output_id IS NULL` → clear error, non-zero (SC2) | integration | `cargo test -p lacon-cli cli_explain::raw_disabled` | ❌ Wave 0 |
| REQ-cli-explain | exit-code branch fidelity (on_error vs success) | unit (core) | `cargo test -p lacon-core filter_bytes` | ❌ Wave 0 — runtime test |
| REQ-cli-doctor | all-green when hooks/configs/rules/perms/health OK | integration | `cargo test -p lacon-cli cli_doctor::all_green` | ❌ Wave 0 — `tests/cli_doctor.rs` |
| REQ-cli-doctor | per-issue actionable error otherwise; exit non-zero | integration | `cargo test -p lacon-cli cli_doctor::failures` | ❌ Wave 0 |
| REQ-cli-doctor | not-yet-initialized DB → informational, not red (D-03) | integration | `cargo test -p lacon-cli cli_doctor::fresh` | ❌ Wave 0 |
| REQ-cli-surface-cap | exactly 6 subcommands; unknown rejected non-zero | integration | `cargo test -p lacon-cli --test cli_surface` | ✅ `tests/cli_surface.rs` (keep green) |
| (core) | `open_readonly` does not migrate/prune/INSERT | unit (core) | `cargo test -p lacon-core open_readonly` | ❌ Wave 0 |
| (core) | `tracking::query` returns typed rows for each view | unit (core) | `cargo test -p lacon-core tracking_query` | ❌ Wave 0 — `tests/tracking_query.rs` |

### Sampling Rate
- **Per task commit:** `cargo test -p lacon-cli <command>` + `cargo test -p lacon-core <new-module>` (< 30s targeted)
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** `cargo test --workspace` green + `cargo clippy --workspace -- -D warnings` before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/lacon-cli/tests/cli_stats.rs` — covers REQ-cli-stats (sections + filters + empty-DB)
- [ ] `crates/lacon-cli/tests/cli_explain.rs` — covers REQ-cli-explain (replay + raw-disabled error + numeric-id parse)
- [ ] `crates/lacon-cli/tests/cli_doctor.rs` — covers REQ-cli-doctor (all-green + per-issue failures + fresh-DB informational)
- [ ] `crates/lacon-core/tests/tracking_query.rs` — covers the new read API + `open_readonly` invariants (no write)
- [ ] Core runtime test for `Runner::filter_bytes` — exit-code branch fidelity (success vs on_error vs no-on_error passthrough)
- [ ] **Wave 0 spike:** verify strict read-only open works on a WAL `history.db` (Open Question 1) — gates the D-02 implementation choice
- [ ] Shared test fixture/helper to seed a `history.db` via the dev-only `rusqlite` (rows across rules/projects/exit-codes/bypass) for stats + explain tests
- [ ] Add `insta = { workspace = true }` to `crates/lacon-cli/[dev-dependencies]` IF snapshot tests are adopted (currently absent there)

## Security Domain

> `security_enforcement` is not set in `.planning/config.json` → treated as enabled. This phase has a narrow, mostly-internal surface (local CLI reading a local SQLite DB the same user wrote), but the relevant controls are real.

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | Local single-user CLI; no auth surface |
| V3 Session Management | no | No sessions |
| V4 Access Control | yes (filesystem) | DB dir is `0700` (verified `ensure_data_dir`); doctor *checks* this perm. Read-only DB open enforces no-write at the SQLite layer. |
| V5 Input Validation | yes | `explain <id>` must validate `id.parse::<i64>()`; `--since` parser must reject malformed input cleanly; all SQL uses **parameterized** `?N` placeholders (record.rs precedent) — never string interpolation |
| V6 Cryptography | no | No crypto in scope; raw_outputs encryption-at-rest is explicit v2 backlog |
| V7 Error Handling/Logging | yes | Best-effort/clear errors per D-03; never leak panics; `explain` raw-disabled is a controlled error not a crash |

### Known Threat Patterns for Rust + rusqlite + local-CLI

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| SQL injection via `--project`/`--rule`/`id` | Tampering | Parameterized queries (`?N`), as already done in record.rs INSERT — extend to all read queries in `query.rs` |
| Path traversal via `--project` filter | Tampering | `--project` is matched as a stored `project_path` value (a `WHERE` parameter), not a filesystem path that gets opened — no traversal risk; do not `fs::open` user-supplied filter values |
| Reading another user's data | Info disclosure | DB is under the invoking user's XDG data dir with `0700` dir perms; no cross-user access by design (V4) |
| Privacy leak via `explain` showing stored raw output | Info disclosure | Raw output is off-by-default (ADR-0009); `explain` only shows what the user opted to store; raw_outputs pruned at 3 days. `explain` errors (not silently fabricates) when retention was off |
| Write to DB from a "read" command | Tampering | `open_readonly` (D-02) + invariant "query commands never INSERT/migrate/prune" — enforced by a core unit test (Wave 0) |
| Panic on malformed input (`lacon explain abc`) | DoS (local) | Validate-and-message, never `unwrap()` on user input (V5/V7) |

## Project Constraints (from CLAUDE.md)

The planner must not recommend approaches that contradict these in-repo directives:

- **No invented build commands beyond the real toolchain.** Real commands: `cargo build`, `cargo test`, `cargo clippy`, `cargo fmt`. (CLAUDE.md "Project status").
- **ADRs are source of truth (13 LOCKED).** Phase 4 relies on ADR-0009 (raw off-by-default → explain error path), ADR-0010 (on_error replaces pipeline → explain branch), ADR-0011 (SQLite/WAL/location → read path), ADR-0013 (cold-start posture clarifies stats/explain/doctor are NOT on the hook hot path). Surface any contradiction rather than working around it.
- **All SQL behind the lacon-core boundary** (architecture.md; D-01). lacon-cli keeps `rusqlite` dev-only.
- **Six-command surface is a contract** (v1-scope.md; REQ-cli-surface-cap). No `purge`/`install`/`stats --serve`.
- **`thiserror` inside crates, `anyhow` at the CLI boundary** (Phase 1 D-03).
- **No async runtime; rusqlite is synchronous** (Phase 1/2).
- **Migrations are append-only; pruning runs on startup** — but query commands must NOT trigger either (D-02/D-08).
- **Cold start < 10ms is load-bearing for `lacon run`** but explicitly does NOT gate stats/explain/doctor (ADR-0013; CONTEXT performance contract). Correctness/clarity wins for these human-invoked commands.

## Sources

### Primary (HIGH confidence — verified in this session against the live codebase)
- `crates/lacon-cli/src/cli.rs` — 6-command surface, Stats/Explain/Doctor arg declarations (D-12, D-13)
- `crates/lacon-cli/src/main.rs` — dispatch discarding Stats/Explain args via `{ .. }` (D-12)
- `crates/lacon-cli/src/commands/{stats,explain,doctor}.rs` — confirmed all three are stubs (exit 2)
- `crates/lacon-cli/src/commands/run.rs` — write-path call site; tracker open/record pattern to mirror read-only
- `crates/lacon-cli/src/commands/init.rs` — `lacon-claude-hook` fingerprint walk (doctor reuse)
- `crates/lacon-cli/tests/cli_surface.rs` — 6-command + unknown-rejection test (SC4, keep green)
- `crates/lacon-cli/Cargo.toml` — `rusqlite` is dev-only (D-01); `insta` not yet listed here
- `crates/lacon-core/src/tracking/mod.rs` — `Tracker::open` (migrate+prune=writes), `apply_connection_pragmas` (pub(crate)), `xdg_db_path`, `ensure_data_dir` 0700 (D-02, D-07.4)
- `crates/lacon-core/src/tracking/health.rs` — `health_check(&Connection)` signature (D-07.5)
- `crates/lacon-core/src/tracking/record.rs` — write-path precedent, 17-col parameterized INSERT, raw_outputs columns (explain BLOB read)
- `crates/lacon-core/src/tracking/migrations/0001_initial.sql` — byte-exact view DDLs + indexes (D-09)
- `crates/lacon-core/src/runtime/mod.rs` — `Runner::run` always spawns; exit-code branch :342-359; `ScriptCtx` fields (D-04, D-05)
- `crates/lacon-core/src/pipeline/mod.rs` — `run_with_post_process` pub, subprocess-free (D-04)
- `crates/lacon-core/src/rules/loader.rs` — `resolve`, `load_all`, `ResolvedRule` public fields (D-05, D-07)
- `crates/lacon-core/src/rules/schema.rs` — `RuleFile`/`OnErrorSpec`/`ScriptSpec` shape (explain replay)
- `crates/lacon-core/src/validate/mod.rs` — `validate_file(path) -> Vec<ValidationError>` (D-07.2)
- `docs/specs/tracking-data-model.md` — schema, four views, retention, 0700, ts-in-ms
- `docs/specs/config-schema.md:119` — "doctor runs config validation on every layer"
- `Cargo.toml` (workspace) — dependency versions; `rust-toolchain.toml` — stable/MSRV 1.80

### Secondary (MEDIUM confidence)
- ADRs 0009/0010/0011/0013 (referenced via CONTEXT canonical_refs; consistent with verified code behavior)

### Tertiary (LOW confidence — flagged for validation)
- SQLite read-only-open-of-WAL behavior on this exact build (A1 / Open Question 1) — **must be Wave-0 verified**, not assumed

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — no new deps; all versions read from in-repo Cargo.toml
- Architecture: HIGH — every API signature verified by reading the source
- View definitions / stats filter design (D-09): HIGH — DDL confirmed byte-exact in two places (spec + migration)
- explain replay path (D-04/D-05): HIGH — `run_with_post_process` and exit-code branch read directly
- Pitfalls: MEDIUM-HIGH — most verified; Pitfall 1 (WAL read-only) is the one genuine unknown, correctly flagged LOW and routed to Wave 0

**Research date:** 2026-05-21
**Valid until:** 2026-06-20 (30 days — stable design-locked project, no fast-moving external deps)
