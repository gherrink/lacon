# Phase 2: Local tracking — Research

**Researched:** 2026-05-06
**Domain:** SQLite (rusqlite) on the cold-start hot path; XDG paths; privacy-marker semantics
**Confidence:** HIGH for rusqlite/SQLite mechanics, etcetera, std::fs primitives; MEDIUM for first-run cost and concrete bench numbers (must be measured during implementation)

## Summary

The Phase 2 design is fully locked (D-01..D-18). What this research adds is the *mechanical surface*: the exact rusqlite calls to use, where each PRAGMA must run, how WAL files behave on `Connection::drop`, how to set `0700` on the data dir without losing the cross-platform abstraction, and how to verify each success criterion with hermetic tests. Several non-obvious load-bearing facts surfaced that the planner should encode as task-level invariants:

1. `PRAGMA foreign_keys=ON` is **per-connection** in SQLite — defaults to OFF — so D-10's `ON DELETE CASCADE` and `ON DELETE SET NULL` are **silently ignored unless every connection sets the pragma** [CITED: sqlite.org/foreignkeys.html]. This is not in CONTEXT.md and is a real landmine.
2. `journal_mode=WAL` is **persistent on the database file** (per sqlite.org/wal.html), but `busy_timeout` is **per-connection**. So the WAL pragma idempotently no-ops after the first invocation; busy_timeout must be set on every connection.
3. `etcetera::choose_base_strategy()` returns the `Xdg` strategy on **both Linux and macOS**, with `data_dir() = ~/.local/share/` on macOS too [CITED: docs.rs/etcetera/0.11.0]. This *satisfies* REQ-tracking-sqlite-location's macOS path requirement. McFly, by contrast, uses Apple-native (`~/Library/Application Support/McFly`) — we deliberately don't.
4. `rusqlite` 0.39.0 ships with bundled SQLite 3.51.3 [CITED: docs.rs/crate/rusqlite/0.39.0]. Confirmed current.
5. SQLite's default busy timeout in `Connection::open` (when no pragma is set) is **5000ms** in newly-created rusqlite connections — D-11's 200ms is an explicit *override*, not an addition.
6. `Connection::drop` runs an automatic checkpoint when it's the last connection, and unlinks `-wal` and `-shm` files cleanly. Don't add manual `wal_checkpoint(PASSIVE)` — it's redundant and burns startup budget.

**Primary recommendation:** Mirror the `RuleLoader` cold-path discipline. `Tracker::open` is one function with a strict, ordered sequence: `open_with_flags` → `busy_timeout` → `foreign_keys=ON` → `journal_mode=WAL` (idempotent on second+ invocation) → `user_version` check → migrate-or-skip → throttled prune → return handle. Single connection per process, dropped at end of `lacon run`. Best-effort write at end of pipeline. Never opened on `--version`/`validate`/`doctor`'s read paths.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| SQLite schema + migrations | `lacon-core::tracking` | — | Hermetic, testable; not CLI-shape |
| Connection open/PRAGMA sequence | `lacon-core::tracking::Tracker` | — | Cold-start discipline lives at the engine boundary |
| Pruning logic + throttle | `lacon-core::tracking::Tracker` | — | Internal concern; throttle key in `lacon_meta` |
| Privacy marker file IO | `lacon-core::tracking::privacy` | — | Filesystem concern, not CLI; reused by future stats path |
| `command_normalized` derivation | `lacon-core::tracking::normalize` | — | Pure fn, unit-testable |
| Tracker call-site (where `Tracker::open` happens) | `lacon-cli::commands::run` | — | After `Runner::run` returns; bytes already on stdout (D-02) |
| `InvocationMeta` assembly (env vars, project_path, ts) | `lacon-cli::commands::run` | `lacon-core::runtime` | Runtime fills its own fields; CLI fills `assistant`/`session_id`/`project_path`/`command_normalized` per D-17 |
| `EngineConfig` consumption (retention, store_raw_outputs) | `lacon-cli::commands::run` | `lacon-core::config` | Config already loaded by Phase 1 logic — Tracker is read-only consumer |
| Error reporting (best-effort log to stderr) | `lacon-cli::commands::run` | — | D-12 best-effort posture; avoids changing exit code |

## Phase Approach

A single sync write happens at end of `lacon run`. Architecture:

```
lacon-cli::commands::run::execute
├── Runner::run → writes filtered bytes to stdout (Phase 1)  ← REQ contract: must reach assistant
└── (post-Runner) Tracker assembly + write          ← Phase 2 NEW
    ├── Tracker::open(db_path)              ← lazy; first call creates dir + DB
    │   ├── ensure_data_dir_with_perms()    ← create_dir_all + chmod 0o700 (Unix only)
    │   ├── Connection::open_with_flags(    ← creates history.db if missing
    │   │     SQLITE_OPEN_READ_WRITE | SQLITE_OPEN_CREATE | SQLITE_OPEN_NO_MUTEX)
    │   ├── conn.busy_timeout(200ms)        ← per-connection (D-11)
    │   ├── conn.set_db_config(ENABLE_FKEY, true) ← per-connection
    │   ├── conn.pragma_update("journal_mode","WAL") ← persistent, idempotent
    │   ├── migrate(&mut conn)              ← user_version → run M0001 in BEGIN/COMMIT
    │   └── prune_throttled(&conn, retention) ← reads lacon_meta.last_pruned_ts
    ├── Tracker::record(&meta, raw_opt)
    │   ├── If store_raw_outputs && raw_opt.some && marker absent: print warning + touch marker
    │   ├── If store_raw_outputs && raw_opt.some: INSERT raw_outputs ... → raw_id
    │   └── INSERT invocations (... raw_output_id=raw_id?)
    └── log + swallow any error (D-12)
```

The connection is dropped at function exit; SQLite auto-checkpoints + unlinks `-wal`/`-shm` files [CITED: sqlite.org/walformat.html "if the last client using the database shuts down cleanly … both the shm file and the wal file are unlinked"]. No manual checkpoint needed.

## Validation Architecture

(Required per Nyquist Dimension 8; `nyquist_validation: true` in config.json.)

### Test Framework
| Property | Value |
|----------|-------|
| Framework | `cargo test` (rustc 1.80+, workspace MSRV); `assert_cmd` 2 + `predicates` 3 + `tempfile` 3 (already workspace-pinned) |
| Config file | `Cargo.toml` per-crate `[[test]]` and `tests/` integration dirs |
| Quick run command | `cargo test -p lacon-core --test tracking_tracker` |
| Full suite command | `cargo test --workspace` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| REQ-tracking-sqlite-location | DB at `XDG_DATA_HOME/lacon/history.db`; dir 0700; `journal_mode=wal`; row in invocations | integration (CLI) | `cargo test -p lacon-cli --test tracking_e2e -- --exact db_created_at_xdg_path` | ❌ Wave 0 |
| REQ-tracking-sqlite-location | macOS `etcetera::choose_base_strategy()` resolves to XDG, not Apple-native | unit | `cargo test -p lacon-core --test tracking_tracker -- xdg_path_macos_too` | ❌ Wave 0 |
| REQ-tracking-schema | All 3 tables, 6 indexes, 4 views exist after `Tracker::open` | unit | `cargo test -p lacon-core --test tracking_schema -- --exact migration_creates_all_objects` | ❌ Wave 0 |
| REQ-tracking-schema | Each view returns rows / non-error against populated DB | unit | `cargo test -p lacon-core --test tracking_views -- views_return_rows` | ❌ Wave 0 |
| REQ-tracking-schema | FK ON DELETE CASCADE actually fires (i.e., `foreign_keys=ON` set) | unit | `cargo test -p lacon-core --test tracking_schema -- fk_cascade_on_invocation_delete` | ❌ Wave 0 |
| REQ-tracking-raw-outputs-default-off | `store_raw_outputs:false` ⇒ no rows in `raw_outputs` | unit | `cargo test -p lacon-core --test tracking_tracker -- raw_outputs_off_no_insert` | ❌ Wave 0 |
| REQ-tracking-privacy-warning | First flip prints exact warning text; touches marker; second flip is silent | unit + CLI integration | `cargo test -p lacon-core --test tracking_privacy -- warning_prints_once_then_marker` and `cargo test -p lacon-cli --test tracking_e2e -- privacy_marker_e2e` | ❌ Wave 0 |
| REQ-tracking-retention-defaults | Rows older than 30/3/30 days deleted on `Tracker::open` (after 24h since last_pruned) | unit | `cargo test -p lacon-core --test tracking_prune -- prune_deletes_old_rows_only` | ❌ Wave 0 |
| REQ-tracking-retention-defaults | Within 24h: prune skipped (last_pruned_ts respected) | unit | `cargo test -p lacon-core --test tracking_prune -- prune_throttled_within_24h` | ❌ Wave 0 |
| REQ-tracking-retention-defaults | Project `retention.*` rejected w/ error pointing at `~/.config/lacon/config.yaml` | unit | (already covered) `cargo test -p lacon-core config::tests::project_retention_rejected` — re-assert in Phase 2 to lock the contract | ✅ exists |
| Cold-start invariant | `lacon --version` does NOT touch the DB filesystem | integration (CLI) | `cargo test -p lacon-cli --test tracking_coldstart -- version_does_not_open_db` | ❌ Wave 0 |
| Cold-start invariant | `lacon validate <path>` does NOT touch the DB | integration (CLI) | `cargo test -p lacon-cli --test tracking_coldstart -- validate_does_not_open_db` | ❌ Wave 0 |
| Best-effort writes (D-12) | Tracker open failure → stderr message + run still exits with subprocess code | integration (CLI) | `cargo test -p lacon-cli --test tracking_e2e -- best_effort_open_failure` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test -p lacon-core --test tracking_tracker --test tracking_schema --test tracking_prune --test tracking_privacy`
- **Per wave merge:** `cargo test --workspace`
- **Phase gate:** Full suite green; Phase 1 cold-start probe re-run with tracker active to confirm headroom (≥7ms) before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `crates/lacon-core/tests/tracking_tracker.rs` — open + write golden path; XDG-on-macOS smoke
- [ ] `crates/lacon-core/tests/tracking_schema.rs` — schema introspection; FK CASCADE/SET NULL fires
- [ ] `crates/lacon-core/tests/tracking_views.rs` — populate fixture rows, query each of the 4 views
- [ ] `crates/lacon-core/tests/tracking_prune.rs` — clock injection (test helper takes `now_ms`); prune-old + prune-throttled paths
- [ ] `crates/lacon-core/tests/tracking_privacy.rs` — marker semantics; warning text byte-exact; `create_new` race posture
- [ ] `crates/lacon-cli/tests/tracking_e2e.rs` — full CLI lap with `XDG_DATA_HOME` and `XDG_CONFIG_HOME` overridden to tempdir; covers SC1/SC2/SC4
- [ ] `crates/lacon-cli/tests/tracking_coldstart.rs` — assert `--version` and `validate` do NOT create `history.db` (negative test: `assert!(!db_path.exists())` after both)
- [ ] `crates/lacon-core/Cargo.toml` add `rusqlite = { workspace = true, features = ["bundled"] }`; workspace add `rusqlite = "0.39"`
- [ ] Test helper crate or module `crates/lacon-core/src/tracking/test_helpers.rs` (under `#[cfg(test)] pub`) for inserting fixture rows with controllable `ts`

## Crate API Notes

### Cargo wiring (D-07)

In `Cargo.toml [workspace.dependencies]`:
```toml
rusqlite = { version = "0.39", features = ["bundled"] }
```

In `crates/lacon-core/Cargo.toml [dependencies]`:
```toml
rusqlite = { workspace = true }
```

Notes:
- `bundled` is the only feature needed for v1. BLOB columns work on the default feature set; the `blob` feature only adds the streaming `Blob` API for incremental I/O — for our small `INSERT/SELECT raw_outputs` pattern, the default `Vec<u8> ↔ ToSql/FromSql` round-trip is fine. **Do not add `blob` unless future `lacon explain` truly needs streaming reads.** [VERIFIED: docs.rs/rusqlite/0.39.0]
- Bundled compile cost: SQLite 3.51.3 amalgamation + `cc`-driven C build adds substantial first-build wall time (typically 30–90s on cold cache). CI cache must include `target/debug/build/libsqlite3-sys-*` to keep iteration loop usable. Subsequent incremental builds are unaffected. [CITED: rusqlite README, lib.rs/crates/rusqlite]
- Binary size: CONTEXT.md cites ~1 MiB. Ballpark consistent with libsqlite3-sys release builds (700KB–1.5MB depending on features).

### Connection open + PRAGMA sequence

```rust
use rusqlite::{Connection, OpenFlags, params};

fn open_connection(db_path: &Path) -> rusqlite::Result<Connection> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;

    // 1. busy_timeout — per-connection. D-11: 200ms.
    //    Internally calls sqlite3_busy_timeout, which installs a busy handler
    //    that retries for up to N ms before returning SQLITE_BUSY.
    conn.busy_timeout(std::time::Duration::from_millis(200))?;

    // 2. foreign_keys — per-connection (defaults OFF). LANDMINE: D-10's
    //    `ON DELETE CASCADE` / `SET NULL` are SILENT NO-OPS without this.
    use rusqlite::config::DbConfig;
    conn.set_db_config(DbConfig::SQLITE_DBCONFIG_ENABLE_FKEY, true)?;

    // 3. journal_mode=WAL — persistent on the file, but cheap to re-set.
    //    Use pragma_update_and_check to verify SQLite accepted "wal".
    let mode: String =
        conn.pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get(0))?;
    debug_assert_eq!(mode.to_ascii_lowercase(), "wal");

    Ok(conn)
}
```

[VERIFIED: rusqlite 0.39 docs via Context7]
[CITED: sqlite.org/wal.html — "WAL journal mode will be set on all connections to the same database file"]
[CITED: sqlite.org/foreignkeys.html — "Foreign key constraints are disabled by default … must be enabled separately for each database connection"]

**Why `SQLITE_OPEN_NO_MUTEX`:** rusqlite types are `!Sync` already; we don't share connections across threads. `NO_MUTEX` skips SQLite's internal serialization mutex. Phase 1 patterns are already single-threaded for tracker writes. [CITED: rusqlite::OpenFlags docs]

### INSERT pattern with `params!`

```rust
let mut stmt = conn.prepare_cached(
    "INSERT INTO invocations (
        ts, assistant, session_id, project_path,
        command_raw, command_normalized, rule_id, rule_source,
        exit_code, duration_ms,
        raw_stdout_bytes, raw_stderr_bytes, filtered_bytes,
        bypassed, rewritten, truncated_by_max_bytes, raw_output_id
    ) VALUES (?1,?2,?3,?4, ?5,?6,?7,?8, ?9,?10, ?11,?12,?13, ?14,?15,?16, ?17)"
)?;

let inv_id = stmt.insert(rusqlite::params![
    meta.ts_unix_ms as i64,
    &meta.assistant,                      // String
    meta.session_id.as_deref(),           // Option<&str>
    meta.project_path.as_ref().and_then(|p| p.to_str()),
    &meta.command_raw,
    &meta.command_normalized,
    meta.rule_id.as_deref(),
    meta.rule_source.as_ref().map(rule_source_str),  // 'project'|'user'|'bundled'|NULL
    meta.exit_code as i64,
    meta.duration_ms as i64,
    meta.byte_counts.raw_stdout_bytes as i64,
    meta.byte_counts.raw_stderr_bytes as i64,
    meta.byte_counts.filtered_bytes as i64,
    meta.bypassed as i64,
    meta.rewritten as i64,
    meta.truncated_by_max_bytes as i64,
    meta.raw_output_id,                   // Option<i64>
])?;
```

`prepare_cached` keeps the compiled `sqlite3_stmt*` per-connection; for our single-INSERT-per-process pattern it's a no-op for *this* invocation but free for future multi-write paths. Per CONTEXT D-99 discretion, positional `?N` reads cleaner than `:name` for this many columns; recommend positional. [VERIFIED: rusqlite 0.39 Context7]

### `pragma_query_value` / `pragma_update`

```rust
let user_version: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
conn.pragma_update(None, "user_version", 1)?;
```

Use this for both reading current `user_version` and stamping after migration. **Do not** use `conn.execute("PRAGMA user_version = 1", [])` — it works, but the typed pragma helpers handle quoting and integer conversion correctly. [VERIFIED: rusqlite 0.39 Context7]

### Transactions

```rust
let tx = conn.transaction()?;       // BEGIN DEFERRED implicit
tx.execute_batch(M0001_INITIAL)?;   // multi-statement DDL string
tx.pragma_update(None, "user_version", 1)?;
tx.commit()?;                       // dropped without commit() → ROLLBACK
```

For the migration, **use `BEGIN IMMEDIATE`** (`Connection::transaction_with_behavior(TransactionBehavior::Immediate)`) — this acquires the write lock up front. Saves the upgrade-from-read-to-write `SQLITE_BUSY` retry path that's a known gotcha [CITED: sqlite.org/forum/info/843e9b7f8f8f3398]. The 200ms busy_timeout still applies.

### Error mapping (`thiserror`)

```rust
#[derive(thiserror::Error, Debug)]
pub enum TrackingError {
    #[error("tracking: failed to create data dir {path}: {source}")]
    CreateDir { path: PathBuf, source: std::io::Error },
    #[error("tracking: failed to set permissions on {path}: {source}")]
    Chmod { path: PathBuf, source: std::io::Error },
    #[error("tracking: sqlite open/migrate failed: {source}")]
    Sqlite { #[from] source: rusqlite::Error },
    #[error("tracking: privacy marker write failed at {path}: {source}")]
    Marker { path: PathBuf, source: std::io::Error },
    #[error("tracking: system time before unix epoch")]
    Clock,
}
```

Mirror Phase 1 patterns: per-error variant, structured `path: PathBuf` for the planner's verifier to grep. `From<rusqlite::Error>` makes the `?` operator clean.

## Migration & WAL Mechanics

### `Tracker::open` flow (sketch)

```rust
pub struct Tracker {
    conn: Connection,
    cfg_store_raw_outputs: bool,
}

impl Tracker {
    pub fn open(
        db_path: &Path,
        retention: &crate::config::Retention,
        cfg_store_raw_outputs: bool,
        now_ms: u64,
    ) -> Result<Self, TrackingError> {
        // 1. Ensure parent dir exists with 0700.
        ensure_data_dir(db_path.parent().expect("db path has parent"))?;

        // 2. Open connection + per-connection PRAGMAs.
        let mut conn = open_connection(db_path)?;  // helper above

        // 3. Migrate (idempotent — uses user_version).
        migrate(&mut conn)?;

        // 4. Throttled prune (24h gate via lacon_meta.last_pruned_ts).
        prune_if_due(&conn, retention, now_ms)?;

        Ok(Tracker { conn, cfg_store_raw_outputs })
    }
}
```

`now_ms` injected for testability — production callsite passes `SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64` (CONTEXT discretion: SystemTime over chrono — confirmed; chrono brings 200KB+ for one timestamp call).

### `migrate()` pattern

```rust
const M0001_INITIAL: &str = include_str!("migrations/0001_initial.sql");
const TARGET_VERSION: i32 = 1;

fn migrate(conn: &mut Connection) -> Result<(), TrackingError> {
    let current: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if current >= TARGET_VERSION { return Ok(()); }

    // BEGIN IMMEDIATE: acquire write lock up front; avoids upgrade-from-read race.
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    if current < 1 {
        tx.execute_batch(M0001_INITIAL)?;   // contains all CREATE TABLE / INDEX / VIEW
    }
    // Future migrations: if current < 2 { tx.execute_batch(M0002_FOO)?; }
    tx.pragma_update(None, "user_version", TARGET_VERSION)?;
    tx.commit()?;
    Ok(())
}
```

`include_str!` embeds the SQL at compile time — zero runtime cost. The single migration string contains **all** DDL: tables, indexes, views, plus an INSERT into `lacon_meta` to seed the throttle key with `0` (so first prune fires).

### M0001 SQL skeleton (DDL byte-exact per `docs/specs/tracking-data-model.md`)

```sql
-- 0001_initial.sql
-- All DDL for v1. View definitions byte-exact per spec lines 96–141.

CREATE TABLE invocations (
  id                      INTEGER PRIMARY KEY,
  ts                      INTEGER NOT NULL,
  assistant               TEXT NOT NULL,
  session_id              TEXT,
  project_path            TEXT,
  command_raw             TEXT NOT NULL,
  command_normalized      TEXT NOT NULL,
  rule_id                 TEXT,
  rule_source             TEXT,
  exit_code               INTEGER NOT NULL,
  duration_ms             INTEGER NOT NULL,
  raw_stdout_bytes        INTEGER NOT NULL,
  raw_stderr_bytes        INTEGER NOT NULL,
  filtered_bytes          INTEGER NOT NULL,
  bypassed                INTEGER NOT NULL DEFAULT 0,
  rewritten               INTEGER NOT NULL DEFAULT 0,
  truncated_by_max_bytes  INTEGER NOT NULL DEFAULT 0,
  raw_output_id           INTEGER REFERENCES raw_outputs(id) ON DELETE SET NULL
);

CREATE INDEX idx_inv_ts       ON invocations(ts);
CREATE INDEX idx_inv_cmd      ON invocations(command_normalized);
CREATE INDEX idx_inv_rule     ON invocations(rule_id);
CREATE INDEX idx_inv_project  ON invocations(project_path);

CREATE TABLE raw_outputs (
  id              INTEGER PRIMARY KEY,
  invocation_id   INTEGER NOT NULL,
  stdout          BLOB,
  stderr          BLOB,
  created_ts      INTEGER NOT NULL
);

CREATE INDEX idx_raw_created ON raw_outputs(created_ts);

CREATE TABLE suspected_regressions (
  id              INTEGER PRIMARY KEY,
  invocation_id   INTEGER NOT NULL REFERENCES invocations(id) ON DELETE CASCADE,
  reason          TEXT NOT NULL,
  detected_ts     INTEGER NOT NULL
);

CREATE INDEX idx_reg_inv ON suspected_regressions(invocation_id);

CREATE TABLE lacon_meta (
  key   TEXT PRIMARY KEY,
  value TEXT
);

INSERT INTO lacon_meta (key, value) VALUES ('last_pruned_ts', '0');

-- Views (D-08 byte-exact). DROP IF EXISTS pattern (D-09) so future migrations
-- can re-create without orphan checks.
DROP VIEW IF EXISTS v_unmatched_offenders;
CREATE VIEW v_unmatched_offenders AS
SELECT command_normalized,
       COUNT(*) AS runs,
       SUM(raw_stdout_bytes + raw_stderr_bytes) AS total_raw_bytes
FROM invocations
WHERE rule_id IS NULL AND bypassed = 0
GROUP BY command_normalized
ORDER BY total_raw_bytes DESC;

DROP VIEW IF EXISTS v_filtered_offenders;
CREATE VIEW v_filtered_offenders AS
SELECT command_normalized, rule_id,
       COUNT(*) AS runs,
       SUM(filtered_bytes) AS total_filtered_bytes,
       AVG(CAST(filtered_bytes AS REAL) /
           NULLIF(raw_stdout_bytes + raw_stderr_bytes, 0)) AS avg_keep_ratio
FROM invocations
WHERE rule_id IS NOT NULL AND bypassed = 0
GROUP BY command_normalized, rule_id
ORDER BY total_filtered_bytes DESC;

DROP VIEW IF EXISTS v_bypass_rate;
CREATE VIEW v_bypass_rate AS
SELECT rule_id,
       COUNT(*) AS total,
       SUM(bypassed) AS bypassed,
       CAST(SUM(bypassed) AS REAL) / COUNT(*) AS bypass_rate
FROM invocations
WHERE rule_id IS NOT NULL
GROUP BY rule_id
HAVING COUNT(*) > 5
ORDER BY bypass_rate DESC;

DROP VIEW IF EXISTS v_project_savings;
CREATE VIEW v_project_savings AS
SELECT project_path,
       COUNT(*) AS total_runs,
       SUM(raw_stdout_bytes + raw_stderr_bytes) AS raw_total,
       SUM(filtered_bytes) AS filtered_total,
       SUM(raw_stdout_bytes + raw_stderr_bytes - filtered_bytes) AS bytes_saved
FROM invocations
WHERE bypassed = 0
GROUP BY project_path
ORDER BY bytes_saved DESC;
```

**Order matters in the file** because `invocations.raw_output_id` references `raw_outputs(id)` — SQLite, unlike MySQL, allows forward references in `CREATE TABLE` only when both tables are created in the same transaction with foreign_keys behavior set; the safe ordering is to **declare `invocations` first** (its FK is "deferred-by-creation-time" in SQLite — the referenced table only needs to exist by the time a row is inserted, not at CREATE time) [CITED: sqlite.org/foreignkeys.html §1].

The `INSERT INTO lacon_meta` inside the migration transaction is fine — the table was just created in the same transaction.

### WAL files on disk

`history.db-wal` and `history.db-shm` appear next to `history.db` once any connection opens in WAL mode and stays open while there are uncommitted frames. Because the parent dir is `0700`, sibling files inherit user-only access automatically — **no explicit per-file chmod needed** [VERIFIED: POSIX directory permission propagation; sqlite.org/walformat.html].

When `Tracker` is dropped at end of `lacon run`, `Connection::drop` runs an automatic checkpoint and unlinks both files cleanly — *iff* the dropped connection is the last open connection [CITED: sqlite.org/walformat.html "if the last client … shuts down cleanly … both the shm file and the wal file are unlinked"]. For us this is always true (one connection per process, dropped at function exit). **Do not add `wal_checkpoint(PASSIVE)`.**

If a sibling `lacon run` from a parallel Claude session is still holding a connection at our drop time, the files persist — that's correct behavior, the sibling will clean them up on its own exit. They re-create on demand on the next open; no corruption risk.

### First-time WAL cost

WAL mode is persistent in the database header — once stamped, every subsequent connection sees `journal_mode=wal` automatically [CITED: sqlite.org/wal.html]. The `pragma_update_and_check` call is therefore:
- **First-ever invocation:** mutates the header from default `delete` to `wal` — single `fsync`, typically <1ms on SSD.
- **Subsequent invocations:** no-op; SQLite returns the existing mode immediately.

Net cold-start overhead from the WAL pragma after first-run is ~µs. The dominant first-time cost is the `cc`-compiled SQLite library load (one-time per binary), the directory `mkdir`, and the schema migration (`execute_batch` of M0001).

## Pruning Throttle Pattern

CONTEXT D-06 specifies `lacon_meta(last_pruned_ts) > 24h ago` as the gate. The mechanism reads the meta row, conditionally runs the three DELETEs and the meta UPDATE, and commits in one transaction.

```rust
const PRUNE_THROTTLE_MS: i64 = 86_400_000;  // 24h

fn prune_if_due(
    conn: &Connection,
    retention: &Retention,
    now_ms: u64,  // injected for tests
) -> Result<(), TrackingError> {
    // Read last_pruned_ts (text column → parse to i64; 0 if absent).
    let last: i64 = conn
        .query_row(
            "SELECT value FROM lacon_meta WHERE key = 'last_pruned_ts'",
            [],
            |r| r.get::<_, String>(0),
        )
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    if (now_ms as i64) - last < PRUNE_THROTTLE_MS {
        return Ok(());
    }

    let inv_cutoff = (now_ms as i64) - (retention.invocations_days as i64) * 86_400_000;
    let raw_cutoff = (now_ms as i64) - (retention.raw_outputs_days as i64) * 86_400_000;
    // suspected_regressions tied to invocations (CON-config-v1-keys: invocations_days
    // also governs suspected_regressions). Use the same cutoff.

    let tx = conn.unchecked_transaction()?;  // we hold &Connection, not &mut
    tx.execute(
        "DELETE FROM raw_outputs WHERE created_ts < ?1",
        params![raw_cutoff],
    )?;
    tx.execute(
        "DELETE FROM suspected_regressions WHERE detected_ts < ?1",
        params![inv_cutoff],
    )?;
    tx.execute(
        "DELETE FROM invocations WHERE ts < ?1",
        params![inv_cutoff],
    )?;
    tx.execute(
        "UPDATE lacon_meta SET value = ?1 WHERE key = 'last_pruned_ts'",
        params![now_ms.to_string()],
    )?;
    tx.commit()?;
    Ok(())
}
```

Note `unchecked_transaction()` — used because we have `&Connection`, not `&mut`. It's safe here because we're single-threaded per process and don't recurse [CITED: rusqlite 0.39 Connection::unchecked_transaction docs].

**Index coverage for the prune queries:**
- `DELETE FROM invocations WHERE ts < ?` → uses `idx_inv_ts` ✓
- `DELETE FROM raw_outputs WHERE created_ts < ?` → uses `idx_raw_created` ✓
- `DELETE FROM suspected_regressions WHERE detected_ts < ?` → **no index on `detected_ts`!** Spec only declares `idx_reg_inv` on `invocation_id`. For v1 with retention deleting cascades from invocations, this is acceptable: most rows in `suspected_regressions` are removed via the FK CASCADE when their parent `invocations` row is deleted. The explicit DELETE here is belt-and-suspenders for orphaned rows. Spec confirms no `idx_reg_detected`; do not add one. (If Phase 4 adds `lacon_meta`-driven detection that creates rows independently of `invocations`, this becomes a real issue — flag for that phase.)

**Order of DELETEs matters:** delete `raw_outputs` first to avoid the `ON DELETE SET NULL` trigger firing for every row about to be deleted anyway. Then `suspected_regressions` (independent), then `invocations` (cascades the rest if any orphans remain). This is a micro-optimization but worth coding correctly the first time.

## Privacy Marker File Semantics

D-14/D-15/D-16 specify the marker file. Implementation:

```rust
fn warn_once_if_needed(
    config_path: &Path,   // resolved at marker-check time; either project or user
    marker_path: &Path,
) -> Result<(), TrackingError> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    // Race-free: create_new(true) returns AlreadyExists if file exists.
    // No TOCTOU between check + create [VERIFIED: doc.rust-lang.org std::fs::OpenOptions].
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)              // belt-and-suspenders; parent dir is 0700 anyway
        .open(marker_path)
    {
        Ok(_) => {
            // We won the race. Print warning.
            let warning = format!(
                "lacon: store_raw_outputs is enabled.\n\
                 lacon: raw stdout/stderr will be retained at ~/.local/share/lacon/history.db\n\
                 lacon: for up to 3 days. Disable in {} or run `rm` on the DB.\n\
                 lacon: this notice is shown once per project (marker: {}).\n",
                config_path.display(),
                marker_path.display(),
            );
            // Stderr write is best-effort; we already created the marker so the
            // warning won't repeat even if this write fails.
            let _ = std::io::stderr().write_all(warning.as_bytes());
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(TrackingError::Marker { path: marker_path.to_owned(), source: e }),
    }
}
```

`create_new(true)` is the atomic primitive [VERIFIED: std::fs::OpenOptions docs]. No `Path::exists()` check — that creates a TOCTOU race when two `lacon run` invocations start simultaneously. With `create_new`, exactly one process gets `Ok`, all others get `AlreadyExists`.

**Marker location resolution (D-14):**
```rust
fn resolve_marker_path(
    project_root: &Path,
    user_config_dir: &Path,
    project_store_raw: bool,    // from .lacon/config.yaml
    user_store_raw: bool,       // from ~/.config/lacon/config.yaml
) -> Option<(PathBuf, PathBuf)>  // (config_path, marker_path)
{
    if project_store_raw {
        let cfg = project_root.join(".lacon").join("config.yaml");
        let marker = project_root.join(".lacon").join(".store_raw_outputs_acked");
        Some((cfg, marker))
    } else if user_store_raw {
        let cfg = user_config_dir.join("config.yaml");
        let marker = user_config_dir.join(".store_raw_outputs_acked");
        Some((cfg, marker))
    } else {
        None  // bundled default false; no marker possible
    }
}
```

Per CONTEXT D-15: "Warning is checked exactly once per invocation, BEFORE the first would-be `raw_outputs` INSERT." So this runs inside `Tracker::record` *only when* the caller passes `Some(raw)` AND `cfg_store_raw_outputs` is true. The check is fast (one `open(O_EXCL)`) and short-circuits cleanly on marker presence.

**Note on D-16 warning text:** the warning interpolates `<config-path>` and `<marker-path>` only — the rest is byte-stable for testing. The hardcoded path `~/.local/share/lacon/history.db` is **literal** in the warning text per CONTEXT D-16; we don't substitute the actual resolved path even if the user has overridden XDG_DATA_HOME. The user-facing text says "where the data WILL go in the documented default location"; the marker path interpolation tells them where the suppression flag lives. This matches D-16 verbatim.

## Filesystem & Permissions

### `etcetera::choose_base_strategy()` confirms XDG on macOS

```rust
use etcetera::BaseStrategy;
let strategy = etcetera::choose_base_strategy()
    .map_err(|e| TrackingError::Generic(e.to_string()))?;
let db_path = strategy.data_dir().join("lacon").join("history.db");
// Linux: ~/.local/share/lacon/history.db
// macOS: ~/.local/share/lacon/history.db (Xdg strategy on macOS too — VERIFIED docs.rs/etcetera/0.11.0)
```

[VERIFIED: docs.rs/etcetera/0.11.0/etcetera/base_strategy/fn.choose_base_strategy.html — "Returns the current OS's default `BaseStrategy`. This uses the `Windows` strategy on Windows, and `Xdg` everywhere else."]

This **satisfies REQ-tracking-sqlite-location's macOS requirement** without a manual override. McFly takes the opposite route (Apple-native on macOS); we deliberately do not. Document this choice in a code comment so future maintainers don't "fix" it.

XDG honors `XDG_DATA_HOME` env var when set, allowing tests to redirect to a tempdir cleanly:
```rust
unsafe { std::env::set_var("XDG_DATA_HOME", tmp.path()); }
let strategy = etcetera::base_strategy::Xdg::new()?;
```
Note: `etcetera::base_strategy::Xdg::new()` reads env vars at construction time, **not** lazily — pattern is "set env, then construct strategy" [VERIFIED: docs.rs/etcetera/0.11.0/etcetera/base_strategy/struct.Xdg.html].

### `0700` parent directory

```rust
fn ensure_data_dir(dir: &Path) -> Result<(), TrackingError> {
    use std::os::unix::fs::PermissionsExt;

    // create_dir_all is race-free against concurrent calls (doc.rust-lang.org).
    std::fs::create_dir_all(dir)
        .map_err(|e| TrackingError::CreateDir { path: dir.to_owned(), source: e })?;

    // Apply 0700 idempotently — runs even when dir already existed,
    // protecting against a previous lacon version that may have created it 0755.
    let mut perms = std::fs::metadata(dir)
        .map_err(|e| TrackingError::Chmod { path: dir.to_owned(), source: e })?
        .permissions();
    if perms.mode() & 0o777 != 0o700 {
        perms.set_mode(0o700);
        std::fs::set_permissions(dir, perms)
            .map_err(|e| TrackingError::Chmod { path: dir.to_owned(), source: e })?;
    }

    Ok(())
}
```

[VERIFIED: doc.rust-lang.org `std::fs::create_dir_all` — "Calling create_dir_all concurrently … is guaranteed not to fail due to a race condition with itself."]

`PermissionsExt` is `std::os::unix::fs` only — wrap the function in `#[cfg(unix)]` and provide a no-op stub for `#[cfg(not(unix))]` to keep compilation green if someone tries `cargo check` on Windows (v1 explicitly excludes Windows, but keeping compilation hermetic prevents accidental local breakage).

### Parent existence

`db_path.parent()` for `~/.local/share/lacon/history.db` returns `~/.local/share/lacon/` — pass that to `ensure_data_dir`. Don't pass the full DB path; that creates a directory *named* `history.db`. Code review and a test should both catch this.

## Pitfalls & Landmines

1. **`PRAGMA foreign_keys=ON` is per-connection and defaults OFF.** Without it, both `ON DELETE CASCADE` (suspected_regressions → invocations) and `ON DELETE SET NULL` (invocations.raw_output_id → raw_outputs) are silent no-ops. Tests will pass for INSERT/SELECT but the v1 retention contract quietly breaks. Mitigation: `set_db_config(SQLITE_DBCONFIG_ENABLE_FKEY, true)` in `open_connection`, plus a unit test that explicitly verifies cascade fires after a parent delete. [CITED: sqlite.org/foreignkeys.html]

2. **Default rusqlite busy_timeout is 5000ms, not 0.** Per rusqlite 0.39 docs, "Newly created connections currently have a default busy timeout of 5000ms." Our D-11 setting of 200ms is therefore an explicit *reduction*. Don't omit it thinking the default is appropriate — 5s would mask real concurrency bugs in tests. [CITED: docs.rs/rusqlite/latest/rusqlite/struct.Connection.html]

3. **`rusqlite[bundled]` first-build wall time.** Compiles SQLite C amalgamation via `cc` — typical 30–90s on cold cache, single-digit seconds on warm. CI runs with `actions/cache` keyed on `Cargo.lock` will reuse `target/debug/build/libsqlite3-sys-*` artifacts; Phase 2 should add a `target/debug/build/libsqlite3-sys-*` cache key to whatever CI exists (or document it for Phase 6 acceptance). Mitigation: not a code change, just an awareness item — don't assume Phase 1's bench/test loop times generalize.

4. **Test isolation via `XDG_DATA_HOME` + `tempfile`.** `cargo test` runs tests in parallel by default. **Every** test that opens the tracker MUST set `XDG_DATA_HOME` (and `XDG_CONFIG_HOME` for marker tests) to a unique tempdir. Use `tempfile::TempDir::new()` and inject the path via env var or — safer — accept `db_path: &Path` directly into `Tracker::open` and route real-vs-test from the call site. The latter avoids env-var stomping entirely and is the recommended pattern. Mitigation: `Tracker::open` accepts the absolute db path; CLI builds it from `etcetera`; tests build it from `TempDir`.

5. **`std::env::set_var` is `unsafe` in 2024 / unsafe-by-default in tests on recent rustc.** Phase 1 already uses it (see config tests). Wrap in `unsafe { ... }` and add `// SAFETY: single-threaded test setup` if rustc warns. Workspace MSRV 1.80 may issue the warning.

6. **`include_str!` paths are relative to the source file.** `include_str!("migrations/0001_initial.sql")` from `crates/lacon-core/src/tracking/mod.rs` resolves to `crates/lacon-core/src/tracking/migrations/0001_initial.sql`. Verify file lives there; CI catches at compile time so this is a foot-cannon, not a runtime hazard.

7. **Cold-start regression on read-only paths.** `lacon --version`, `lacon validate`, and (Phase 4) `lacon doctor` MUST NOT touch the database. Easy to break by accidentally putting `Tracker::open` in a shared init function. Mitigation: explicit assert-not-exists test (`tracking_coldstart.rs`); `Tracker::open` lives in `lacon-cli/src/commands/run.rs` and is called from nowhere else.

8. **Migration-tx + view DROP IF EXISTS interaction with foreign_keys.** The `DROP VIEW IF EXISTS` pattern (D-09) is safe inside a transaction — views don't participate in foreign-key constraints. **But** if a future migration drops/recreates a *table* referenced by FK, that DROP needs `PRAGMA foreign_keys=OFF` around it (per SQLite docs §4.2). Out of v1 scope but flag for migration `0002+`.

9. **`SystemTime::now().duration_since(UNIX_EPOCH)` can fail** on systems with clock skew before 1970. Map to `TrackingError::Clock` and skip the write rather than panic. Won't happen in practice but the `.unwrap()` foot-cannon is worth avoiding.

10. **WAL files persist if a sibling process is open.** Most tests pass through cleanly; flaky test scenarios may see `history.db-wal` and `history.db-shm` lingering. Don't write tests that assert *only* `history.db` exists — assert it exists, ignore siblings.

11. **`HAVING COUNT(*) > 5` makes `v_bypass_rate` empty until 6 invocations.** Tests for SC3 (views queryable) must populate >5 rows per `rule_id` to see rows in `v_bypass_rate`, OR explicitly test that the view is *queryable* (no error) regardless of cardinality. The spec wording is "non-error result sets when queried" — empty is non-error. Use a no-fixture `SELECT COUNT(*) FROM v_bypass_rate` smoke test.

12. **Phase 1's `RuleSource` is in `lacon-core::rules`, not `lacon-core::tracking`.** `InvocationMeta.rule_source: Option<RuleSource>` already exists. Phase 2 needs a `rule_source_str(&RuleSource) -> &'static str` helper to map enum → `'project'/'user'/'bundled'` for the `rule_source` column. Pure function, lives in `tracking::mod.rs`.

13. **`signal_id` (sic — `session_id`) of NULL vs missing column.** rusqlite serializes `Option::None` as SQL NULL via `ToSql` blanket impl. `meta.session_id.as_deref()` (typed `Option<&str>`) is the correct binding. Don't manually `.unwrap_or("")` — empty string would distort `WHERE session_id IS NULL` queries downstream.

14. **`store_raw_outputs` flips ON → OFF doesn't delete existing rows.** That's intentional per spec ("Manual cleanup. v1 ships no `lacon purge` command"). The marker is *enablement* tracking, not a state machine. Mitigation: comment in the privacy module clarifying the flip-off semantics ("user must `rm` history.db or the raw_outputs rows manually").

## Reference Implementations

### atuin (atuinsh/atuin)

- Stack: `sqlx` + SQLite (different from ours; sqlx is async). WAL mode by default. [CITED: github.com/atuinsh/atuin/issues/2356]
- Cold-start posture: documented startup-time issues at large history sizes — fixed by adding indexes. **Lesson:** the four indexes in our schema (idx_inv_ts/cmd/rule/project) are correct *and* load-bearing. Don't trim them under "cold start" pressure.
- WAL on network filesystems known to corrupt — irrelevant for us (local-only) but documents the lower bound of WAL safety.
- Migrations: sqlx-migrate (file-per-version under `migrations/`). We're explicitly *not* using a migration crate (D-08); single inline SQL is the v1 simplification.

### McFly (cantino/mcfly)

- Stack: `rusqlite` (matches ours). Stores at `$XDG_DATA_DIR/mcfly/history.db` on Linux, `~/Library/Application Support/McFly` on macOS. **We deliberately deviate** — REQ-tracking-sqlite-location requires `~/.local/share/lacon/` on **both** platforms, and `etcetera::choose_base_strategy()` cooperates by returning XDG on macOS.
- `MCFLY_HISTORY_LIMIT` pattern (cap rows considered) — irrelevant for our 30-day retention but a good design pattern if `lacon stats` surfaces perf issues in Phase 4.

### `rusqlite_migration` crate

- Decided against (CONTEXT D-08 inline SQL). Worth knowing the API for context: `Migrations::new(vec![M::up("..."), M::up("...")])` + `migrations.to_latest(&mut conn)`. Internally also uses `user_version`. If migrations grow past ~5 entries (v2+), revisit. [CITED: docs.rs/rusqlite_migration]

## Open Risks for Plan-Time Resolution

These don't block planning but the planner should fold them into specific tasks or wave-0 spikes:

1. **First-time migration cost (<50ms target).** Not measured yet. Plan should include a benchmark task ("Bench: cold tracker open against fresh DB; assert <50ms wall on Linux dev box"). If exceeded, the throttle (D-06) and lazy-open (D-04) become *mandatory*, not belt-and-suspenders. Mitigation already in design; just confirm.

2. **`rusqlite[bundled]` cold-start delta vs Phase 1 baseline.** Phase 1 `--version` median was 1154µs. Adding `rusqlite[bundled]` to the *crate graph* (even on paths that don't open a connection) may inflate binary load by 50–200µs. Plan should re-run the cold-start probe in Wave 0 with `lacon-core` linking rusqlite, before any DB-touching code lands, to isolate the link-time cost from open-time cost.

3. **`session_id` env var presence in Phase 2 tests.** Phase 3 (adapter) sets `LACON_SESSION_ID`; Phase 2 tests run *without* it. Confirm `meta.session_id` defaults to `None` cleanly and the column accepts NULL. (This is in spec — `session_id TEXT` is nullable — but worth a test that explicitly inserts NULL.)

4. **Conservative `command_normalized` algorithm wording for the test.** D-18 says "implementation-defined; spec says 'may improve over time'." The Phase 2 unit test should assert *behavior*, not exact strings, for argv inputs the planner picks. A small fixture table — `["pnpm","install","--frozen-lockfile"]` → `"pnpm install"`; `["/usr/local/bin/pnpm","install"]` → `"pnpm install"`; `["cargo","-V"]` → `"cargo"` — is the right shape. Plan should explicitly enumerate the 4–6 fixture cases.

5. **First-run prune semantics.** When `lacon_meta.last_pruned_ts = '0'` (fresh seed), `now_ms - 0 > 86_400_000` is true, so prune fires. On an empty database the DELETEs are no-ops. **But** the M0001 INSERT seeds `last_pruned_ts='0'`, meaning the *very first* `lacon run` after migration runs prune-with-no-rows. That's correct, free, and verifiable — but include a unit test that walks "fresh open → first record → confirm `last_pruned_ts` is now ≈ `now_ms`" so the seed-and-update behavior is locked.

6. **`unchecked_transaction` vs `transaction`.** Sketch above uses `unchecked_transaction()` for the prune step because we hold `&Connection`. Verify this is acceptable in the rusqlite API — alternative is restructuring `Tracker::open` to hold `&mut Connection` throughout. Both work; `&mut self` on the helper functions is idiomatically cleaner. Decide at plan time.

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| REQ-tracking-sqlite-location | DB at `~/.local/share/lacon/history.db`; WAL on; dir 0700 | `etcetera::choose_base_strategy()` returns Xdg on Linux+macOS [CITED: docs.rs/etcetera/0.11.0]; `pragma_update("journal_mode","WAL")` is persistent on the DB file [CITED: sqlite.org/wal.html]; `create_dir_all` is race-free and `set_permissions(0o700)` is the standard idiom [VERIFIED: doc.rust-lang.org] |
| REQ-tracking-schema | 3 tables, 6 indexes, 4 views, FK semantics | DDL byte-exact per `docs/specs/tracking-data-model.md`; FK enforcement requires per-connection `PRAGMA foreign_keys=ON` [CITED: sqlite.org/foreignkeys.html]; `pragma_update_and_check`, `execute_batch`, `transaction_with_behavior` covered by rusqlite 0.39 |
| REQ-tracking-raw-outputs-default-off | No rows in `raw_outputs` when `store_raw_outputs:false` | `Tracker::record` gates the second INSERT on the config flag; default in `EngineConfig` already `false` (`crates/lacon-core/src/config/mod.rs:225`) |
| REQ-tracking-privacy-warning | One-time stderr notice on off→on flip | `OpenOptions::create_new(true)` is atomic [VERIFIED: doc.rust-lang.org]; idempotent on `AlreadyExists`; warning text byte-fixed per D-16; marker path resolution per D-14 |
| REQ-tracking-retention-defaults | Prune at startup; project `retention.*` rejected | DELETE WHERE ts < cutoff covered by indexes (idx_inv_ts, idx_raw_created); throttle via `lacon_meta.last_pruned_ts` per D-06; project-layer rejection already implemented in `parse_partial_from_str`/`retention_precheck` (`crates/lacon-core/src/config/mod.rs:159–204`) |

## Project Constraints (from CLAUDE.md)

The project root `CLAUDE.md` notes "Design phase. No code yet." — this is **stale**; Phase 1 has shipped (see `crates/lacon-core/`, `crates/lacon-cli/`, and `STATE.md` listing 8/8 plans complete). All other directives stand. Specifically applicable to Phase 2:

- **"Streaming, not buffered" (ADR 0005).** Does not apply to tracker writes — tracker is single-INSERT-after-pipeline, not streaming. CONTEXT confirms.
- **"Cold start under 10ms."** Load-bearing for Phase 2. Headroom from Phase 1: ~8.7ms. Tracker open + migrate-skip + INSERT must consume <3ms. Measure don't assume.
- **"First-match-wins, project > user > bundled."** Applies to config layer too. `EngineConfig` already implements per-key deep merge (see `crates/lacon-core/src/config/mod.rs`). Tracker just consumes the resolved config.
- **"SQLite with WAL mode at `~/.local/share/lacon/history.db`."** Phase 2 *implements* this directive; the directive does not constrain the implementation beyond what's already in CONTEXT/specs.
- **"Migrations are append-only."** Reinforces D-08. M0001 is the only v1 migration; future migrations append to a vec/dispatch table; never edited.
- **"`rust-embed` or inline `const` for bundled assets."** CONTEXT D-08 picks inline `const` (via `include_str!`) for migrations. Consistent with project style — matches `rules/bundled.rs`'s rust-embed *but* migrations aren't iterated like rule files, so inline is correct.
- **"`thiserror` inside crates, `anyhow` at the CLI boundary."** `TrackingError` follows `ValidationError`/`RuntimeError` patterns. CLI wrap point is `crates/lacon-cli/src/commands/run.rs`.
- **"No async runtime."** rusqlite is sync — consistent. Reject any temptation to add `tokio-rusqlite` or `sqlx`.
- **Bypass mechanics (`!!`, `LACON_DISABLE=1`).** Phase 2 inherits — `Runner::run` already handles `LACON_DISABLE`; `bypassed: bool` propagates to `InvocationMeta` cleanly. Phase 2 records the bypass; doesn't enforce it.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | First-time WAL header mutation costs <1ms on SSD | Migration & WAL | Documented industry consensus, but unmeasured on our specific FS/kernel combos. If wrong, cold-start budget tightens — mitigation already in place (lazy open). |
| A2 | rusqlite[bundled] adds ~1 MiB to release binary | Crate API Notes | CONTEXT estimate; not re-verified post-3.51.3 amalgamation upgrade. Could be ±50%. Material only for Phase 6 binary-size targets, not v1 functionality. |
| A3 | First-time migration runs in <50ms on fresh DB | Open Risks | Inferred from "small schema + bundled SQLite + SSD." Plan should include the bench task to confirm. |
| A4 | `Connection::drop` always runs auto-checkpoint and unlinks WAL files | Migration & WAL | Verified for the documented "last connection" case; in real-world parallel `lacon run` invocations from sibling Claude sessions this can be false. Tests should not assume sibling files are absent. |
| A5 | macOS `etcetera::choose_base_strategy()` keeps returning Xdg in future versions | Filesystem & Permissions | Public API behavior at 0.11.0; if etcetera ships a major version that switches macOS to Apple-native, our REQ contract breaks. Mitigation: pin etcetera version (workspace already pins `0.11`); add a unit test asserting the macOS path *is* under `.local/share`. |
| A6 | `SQLITE_OPEN_NO_MUTEX` is safe given our single-thread-per-process model | Crate API Notes | rusqlite docs confirm; this matches Phase 1 model (single tracker write per process). If Phase 4 introduces async or shared connections, re-evaluate. |

**These assumptions warrant explicit confirmation with the user before plan-execution if they materially affect the v1 ship gate.** A3 in particular: if first-time migration cost is the wrong assumption, the entire "lazy-open is good enough" stance changes.

## Sources

### Primary (HIGH confidence)
- Context7 `/websites/rs_rusqlite_0_39_0_rusqlite` — `Connection`, `OpenFlags`, `pragma_update`, `pragma_query_value`, `pragma_update_and_check`, `busy_timeout`, `set_db_config`, `transaction_with_behavior`, `prepare_cached`, `params!`, WAL hooks, CheckpointMode
- Context7 `/websites/rs_etcetera_0_11_0_etcetera` — `choose_base_strategy()` returns Xdg on Linux+macOS, returns Windows on Windows
- Context7 `/rusqlite/rusqlite` — bundled feature, transaction patterns, named/positional params
- Context7 `/websites/rs_rusqlite_migration` — user_version migration pattern (decided against the crate, but confirms our manual approach is canonical)
- doc.rust-lang.org — `std::fs::OpenOptions::create_new` (atomic), `std::fs::create_dir_all` (race-free), `std::os::unix::fs::PermissionsExt`
- sqlite.org/wal.html — WAL persistence on the DB file across connections; checkpoint semantics
- sqlite.org/walformat.html — WAL/SHM file lifecycle; auto-cleanup on last close
- sqlite.org/foreignkeys.html — per-connection `PRAGMA foreign_keys=ON` requirement
- `docs/specs/tracking-data-model.md` (in-repo, LOCKED) — schema DDL, view DDL, retention policy
- `docs/decisions/0009-separated-raw-outputs.md`, `0011-sqlite-for-tracking.md`, `0013-filter-via-pretooluse-wrapper.md` (in-repo, LOCKED)
- `crates/lacon-core/src/config/mod.rs`, `crates/lacon-core/src/runtime/mod.rs`, `crates/lacon-core/src/rules/loader.rs`, `crates/lacon-core/src/error.rs` (in-repo) — Phase 1 patterns to mirror

### Secondary (MEDIUM confidence)
- berthub.eu/articles/posts/a-brief-post-on-sqlite3-database-locked-despite-timeout — `BEGIN IMMEDIATE` recommendation for write transactions
- docsaid.org SQLite in Practice — WAL + busy_timeout + short transactions guidance
- github.com/atuinsh/atuin — reference comparison on WAL + sqlite + cold-start at scale

### Tertiary (LOW confidence)
- generalistprogrammer.com tutorials/rusqlite — corroborating bundled feature notes (cross-referenced with primary)

## Metadata

**Confidence breakdown:**
- Standard stack (rusqlite + bundled, etcetera, std::fs): HIGH — Context7 + official docs verified for 0.39.0 and 0.11.0
- Architecture (lazy-open, single-conn-per-process, FK pragma): HIGH — locked by ADRs + verified PRAGMA semantics
- Pitfalls: HIGH for known sqlite gotchas; MEDIUM for measured cold-start delta (must benchmark)
- Validation architecture: HIGH — patterns mirror Phase 1's `assert_cmd` + `tempfile` setup already in `crates/lacon-cli/tests/end_to_end.rs`

**Research date:** 2026-05-06
**Valid until:** 2026-06-06 (30 days — rusqlite/SQLite/etcetera are mature, slow-moving stacks; revisit only if a major version of any ships in that window)

## RESEARCH COMPLETE
