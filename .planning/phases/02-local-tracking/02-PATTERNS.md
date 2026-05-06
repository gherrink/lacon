# Phase 2: Local tracking — Pattern Map

**Mapped:** 2026-05-06
**Files analyzed:** 14 (7 new, 7 modified)
**Analogs found:** 12 / 14 (2 have no exact analog — see "No Analog Found")

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `crates/lacon-core/src/tracking/mod.rs` (new) | module facade + `Tracker` struct | request-response (sync write) | `crates/lacon-core/src/rules/loader.rs` (struct + lazy hot path) | role-match |
| `crates/lacon-core/src/tracking/migrations.rs` (new) | const-SQL DDL embed + apply | one-shot DDL transaction | `crates/lacon-core/src/rules/bundled.rs` (compile-time embed) | partial (different mechanism, same posture) |
| `crates/lacon-core/src/tracking/migrations/0001_initial.sql` (new) | SQL DDL fixture | DDL | none in repo | NONE — see external reference |
| `crates/lacon-core/src/tracking/normalize.rs` (new) | pure `fn normalize(argv) -> String` | transform | `crates/lacon-core/src/rules/loader.rs::strip_layer_prefix` (same shape: pure str fn) | role-match |
| `crates/lacon-core/src/tracking/health.rs` (new) | health-check probe | request-response | `crates/lacon-core/src/runtime/mod.rs::install_signal_forwarder` (small focused helper module) | partial |
| `crates/lacon-core/src/tracking/privacy.rs` (new) | filesystem marker + warning | file-I/O | `crates/lacon-core/src/rules/loader.rs::resolve_script_path` (path validation + fs probe) | partial |
| `crates/lacon-core/src/error.rs` (modified) | add `TrackingError` enum | — | existing `RuntimeError` / `ValidationError` enums in same file | exact |
| `crates/lacon-core/src/lib.rs` (modified) | add `pub mod tracking;` | — | existing `pub mod runtime;` declarations | exact |
| `crates/lacon-core/src/runtime/mod.rs` (modified) | extend `InvocationMeta` fields | — | self (D-03 EXTENDS the existing struct) | exact |
| `crates/lacon-cli/src/commands/run.rs` (modified) | wire tracker post-`Runner::run` | request-response | self (`run_with_rule` and `run_unmatched`) | exact |
| `crates/lacon-core/Cargo.toml` (modified) | add `rusqlite = { workspace = true }` | manifest | self (existing `regex`, `serde-saphyr` deps) | exact |
| `Cargo.toml` (modified) | add `rusqlite = "0.39"` to workspace.dependencies | manifest | self (existing `regex = "1"` etc.) | exact |
| `crates/lacon-core/tests/tracking_tracker.rs` (new) | open + write golden path | integration | `crates/lacon-core/tests/rules_loader.rs` | exact |
| `crates/lacon-core/tests/tracking_schema.rs` (new) | DDL introspection + FK | integration | `crates/lacon-core/tests/rules_loader.rs` (tempdir + setup helper) | role-match |
| `crates/lacon-core/tests/tracking_views.rs` (new) | views queryable smoke | integration | `crates/lacon-core/tests/rules_loader.rs` | role-match |
| `crates/lacon-core/tests/tracking_prune.rs` (new) | clock-injected prune | integration | `crates/lacon-core/tests/rules_loader.rs` | role-match |
| `crates/lacon-core/tests/tracking_privacy.rs` (new) | marker + warning byte-exact | integration | `crates/lacon-core/tests/rules_loader.rs` | role-match |
| `crates/lacon-cli/tests/tracking_e2e.rs` (new) | full CLI lap | integration (CLI) | `crates/lacon-cli/tests/end_to_end.rs` | exact |
| `crates/lacon-cli/tests/tracking_coldstart.rs` (new) | assert no DB on read paths | integration (CLI, negative) | `crates/lacon-cli/tests/cli_run.rs` (assert_cmd skeleton) | role-match |

## Pattern Assignments

### `crates/lacon-core/src/tracking/mod.rs` (Tracker struct, lazy hot path)

**Analog:** `crates/lacon-core/src/rules/loader.rs` — same lazy-on-hot-path posture (D-14), same XDG resolution via etcetera, same in-process struct with read-mostly state.

**Module-doc + import block** (from `loader.rs:1-42`):
```rust
//! RuleLoader — lazy-resolve hot path (D-14), eager path for validate/doctor.

use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use etcetera::BaseStrategy;

use crate::error::ValidationError;
```
Mirror in `tracking/mod.rs`: replace `regex` import block with `rusqlite::{Connection, OpenFlags}`; replace `ValidationError` import with `TrackingError` (defined in `crates/lacon-core/src/error.rs`).

**XDG resolution pattern** (from `loader.rs:110-121`):
```rust
pub fn new(project_dir: Option<PathBuf>) -> Self {
    let user_dir = etcetera::choose_base_strategy()
        .ok()
        .map(|s| s.config_dir().join("lacon").join("rules"));
    // ...
}
```
For Phase 2, swap `s.config_dir()` → `s.data_dir()` and `"rules"` → just `"history.db"` — i.e., `s.data_dir().join("lacon").join("history.db")`. RESEARCH §"Filesystem & Permissions" verifies `etcetera::choose_base_strategy()` returns Xdg on macOS too.

**Lazy-hot-path posture** (from `loader.rs:123-151`): the `resolve()` shape (only-do-work-on-write-path, return early on layer miss) maps directly to `Tracker::open` returning early when the DB exists vs creating fresh. The "Walks layers in priority order" comment style is the precedent for documenting `Tracker::open`'s ordered PRAGMA sequence (RESEARCH §"Connection open + PRAGMA sequence" lines 130-156).

**Struct definition** (from `loader.rs:90-103`):
```rust
pub struct RuleLoader {
    project_dir: Option<PathBuf>,
    user_dir: Option<PathBuf>,
    cache: HashMap<CacheKey, CachedRule>,
    pub defaults_max_bytes: usize,
}
```
Mirror in `tracking/mod.rs`:
```rust
pub struct Tracker {
    conn: rusqlite::Connection,
    cfg_store_raw_outputs: bool,
}
```
Per RESEARCH §"`Tracker::open` flow (sketch)" lines 245-272.

---

### `crates/lacon-core/src/error.rs` (add `TrackingError`)

**Analog:** the **same file** — `RuntimeError` (lines 87-126) and `ValidationError` (lines 16-81) are the existing precedents.

**`thiserror`-derived enum pattern** (from `error.rs:87-126`):
```rust
#[derive(thiserror::Error, Debug)]
pub enum RuntimeError {
    #[error("starlark parse error in {path}: {message}")]
    StarlarkParseError { path: PathBuf, message: String },
    // ...
    #[error("io error in runtime: {source}")]
    IoError { source: std::io::Error },
    #[error("argv was empty")]
    EmptyArgv,
}
```

**`TrackingError` to add** (per RESEARCH §"Error mapping" lines 222-239 — copy verbatim into `error.rs` after the existing enums):
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

**Test pattern** (from `error.rs:144-205`): per-variant `Display` byte-exact assertions. New tests should follow:
```rust
#[test]
fn error_display_format_byte_exact() {
    let err = ValidationError::InvalidRegex { /* ... */ };
    let s = format!("{err}");
    assert_eq!(s, ".lacon/rules/my-rule.yaml:7: InvalidRegex: unclosed character class");
}
```

---

### `crates/lacon-core/src/tracking/normalize.rs` (pure transform fn)

**Analog:** `crates/lacon-core/src/rules/loader.rs::strip_layer_prefix` (lines 436-443) — pure free function, single string in/out, no error type, lives next to its module.

**Pattern** (from `loader.rs:436-443`):
```rust
/// Strip optional `bundled/`, `user/`, `project/` prefix from an extends ID.
fn strip_layer_prefix(id: &str) -> &str {
    for prefix in &["bundled/", "user/", "project/"] {
        if let Some(bare) = id.strip_prefix(prefix) {
            return bare;
        }
    }
    id
}
```

**Mirror for `normalize.rs`** (per CONTEXT D-18 + RESEARCH §"Open Risks" item 4 — fixture cases):
```rust
/// Normalize argv into a stable key for tracking aggregation.
///
/// Conservative v1 algorithm (D-18): basename(argv[0]) plus argv[1] when present
/// AND argv[1] does not start with `-`. The exact normalization is implementation-
/// defined per spec ("may improve over time").
///
/// Examples:
///   ["pnpm","install","--frozen-lockfile"] → "pnpm install"
///   ["/usr/local/bin/pnpm","install"]      → "pnpm install"
///   ["cargo","-V"]                         → "cargo"
pub fn normalize(argv: &[String]) -> String { /* ... */ }
```
Test fixture table per RESEARCH item 4 — enumerate 4-6 cases.

---

### `crates/lacon-core/src/tracking/migrations.rs` (const-SQL embed)

**Analog (closest):** `crates/lacon-core/src/rules/bundled.rs` (lines 1-39) — compile-time asset embedding via `rust-embed`. **NOT a perfect match** because migrations aren't iterated; CONTEXT D-08 explicitly picks inline `const` via `include_str!` instead of `rust-embed`.

**Pattern from `bundled.rs:14-23`** (compile-time path resolution):
```rust
/// rust-embed handle for the bundled-rules/ directory.
///
/// The path `../../bundled-rules/` is relative to `crates/lacon-core/Cargo.toml`
/// (i.e., relative to `$CARGO_MANIFEST_DIR`), resolving to `<workspace>/bundled-rules/`.
#[derive(RustEmbed)]
#[folder = "../../bundled-rules/"]
pub struct BundledRules;
```

**Mirror for `migrations.rs`** (per RESEARCH §"`migrate()` pattern" lines 277-296 — `include_str!` is RELATIVE to the source file, see Pitfall 6):
```rust
const M0001_INITIAL: &str = include_str!("migrations/0001_initial.sql");
const TARGET_VERSION: i32 = 1;

pub fn migrate(conn: &mut rusqlite::Connection) -> Result<(), TrackingError> {
    let current: i32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if current >= TARGET_VERSION { return Ok(()); }

    let tx = conn.transaction_with_behavior(
        rusqlite::TransactionBehavior::Immediate,
    )?;
    if current < 1 {
        tx.execute_batch(M0001_INITIAL)?;
    }
    tx.pragma_update(None, "user_version", TARGET_VERSION)?;
    tx.commit()?;
    Ok(())
}
```

**`include_str!` path semantics** documented in `bundled.rs:11-13`:
```rust
//! # Path resolution
//! rust-embed resolves relative paths from `$CARGO_MANIFEST_DIR` at compile time.
```
The same comment style applies to `include_str!("migrations/0001_initial.sql")` from `crates/lacon-core/src/tracking/migrations.rs` → resolves to `crates/lacon-core/src/tracking/migrations/0001_initial.sql` (Pitfall 6 in RESEARCH).

---

### `crates/lacon-core/src/tracking/privacy.rs` (marker file + warning)

**Analog (partial):** `crates/lacon-core/src/rules/loader.rs::resolve_script_path` (lines 700-744) — same shape: a path-validation helper that returns `Result<_, ValidationError>`, with explicit pre-checks before any I/O.

**Pattern from `loader.rs:700-744`** (path validation discipline + structured error mapping):
```rust
fn resolve_script_path(script_path: &Path, rule_path: &Path) -> Result<PathBuf, ValidationError> {
    if script_path.is_absolute() { return Err(/* ... */); }
    if script_path.components().any(|c| matches!(c, Component::ParentDir)) { /* ... */ }
    let rule_dir = rule_path.parent().unwrap_or(Path::new("."));
    let resolved = rule_dir.join(script_path);
    if !resolved.exists() { return Err(/* ... */); }
    Ok(resolved)
}
```

**Mirror for `privacy.rs::warn_once_if_needed`** (per RESEARCH §"Privacy Marker File Semantics" lines 494-528):
```rust
pub(crate) fn warn_once_if_needed(
    config_path: &Path,
    marker_path: &Path,
) -> Result<(), TrackingError> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;

    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)        // atomic; AlreadyExists if file exists
        .mode(0o600)
        .open(marker_path)
    {
        Ok(_) => {
            let warning = format!(
                "lacon: store_raw_outputs is enabled.\n\
                 lacon: raw stdout/stderr will be retained at ~/.local/share/lacon/history.db\n\
                 lacon: for up to 3 days. Disable in {} or run `rm` on the DB.\n\
                 lacon: this notice is shown once per project (marker: {}).\n",
                config_path.display(),
                marker_path.display(),
            );
            let _ = std::io::stderr().write_all(warning.as_bytes());
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(TrackingError::Marker { path: marker_path.to_owned(), source: e }),
    }
}
```

**Warning text is byte-stable** per CONTEXT D-16; the `<config-path>` and `<marker-path>` are the only interpolated parts. Tests must assert byte-exact prefix on the literal lines.

---

### `crates/lacon-core/src/tracking/health.rs` (no-op write/read probe)

**Analog (partial):** `crates/lacon-core/src/runtime/mod.rs::install_signal_forwarder` (lines 434-481) — small focused helper that takes a connection-like resource and returns a structured result. Health is even simpler.

**Pattern shape** (from CONTEXT D-13):
```rust
/// No-op write/read probe for `lacon doctor` (Phase 4). Phase 2 defines it
/// only — never called by Phase 2 code.
pub fn health_check(conn: &rusqlite::Connection) -> Result<HealthReport, TrackingError> {
    // SELECT 1 round-trip — confirms DB is reachable.
    let one: i32 = conn.query_row("SELECT 1", [], |r| r.get(0))?;
    debug_assert_eq!(one, 1);
    Ok(HealthReport { /* ... */ })
}
```
**No tighter analog exists** — `runtime::ScriptCtx` builder (line 309) is the closest "build-and-return-a-struct" pattern but it's a different domain. Document this as a Phase 4 surface; ship a single passing unit test in Phase 2.

---

### `crates/lacon-core/src/runtime/mod.rs` (extend `InvocationMeta`)

**Analog:** the existing struct itself (lines 89-113). Per CONTEXT D-03: EXTEND, do not redefine.

**Existing struct** (lines 89-113):
```rust
#[derive(Debug, Clone)]
pub struct InvocationMeta {
    pub ts_unix_ms: u64,
    pub rule_id: Option<String>,
    pub rule_source: Option<crate::rules::RuleSource>,
    pub command_raw: String,
    pub argv: Vec<String>,
    pub exit_code: i32,
    pub duration_ms: u64,
    pub byte_counts: ByteCounts,
    pub bypassed: bool,
    pub rewritten: bool,
    pub truncated_by_max_bytes: bool,
}
```

**Phase 2 additions** (per CONTEXT D-03 + RESEARCH §"INSERT pattern with `params!`" lines 167-196 column list):
```rust
    // Phase 2 additions:
    /// Assistant identifier (D-17 env: LACON_ASSISTANT, default "claude-code").
    pub assistant: String,
    /// Optional session id (D-17 env: LACON_SESSION_ID, default None → SQL NULL).
    pub session_id: Option<String>,
    /// Project root (typically std::env::current_dir() at the call site).
    pub project_path: Option<PathBuf>,
    /// Normalized command key for aggregation; produced by tracking::normalize::normalize().
    pub command_normalized: String,
    /// FK into raw_outputs.id; populated only when raw retention is active.
    pub raw_output_id: Option<i64>,
```

**Existing comment style to preserve** (line 87-89):
```rust
/// Phase-2 tracker metadata. Defined here so Phase 2 can add the SQLite write
/// alongside this struct without refactoring Phase 1 code.
```
This was already pre-staged — Phase 2 just fills in the additional fields and updates the doc comment to reflect the now-active state.

---

### `crates/lacon-cli/src/commands/run.rs` (wire-up site)

**Analog:** the **same file** — `run_with_rule` (lines 152-174) and `run_unmatched` (lines 176-200) are the two entry points where Phase 2 inserts the post-`Runner::run` tracker call.

**Existing `run_with_rule`** (lines 152-174):
```rust
fn run_with_rule<W: Write>(
    resolved: ResolvedRule,
    argv: Vec<String>,
    project_path: Option<PathBuf>,
    sink: &mut W,
) -> anyhow::Result<i32> {
    let options = RunOptions {
        project_path,
        extra_env: Default::default(),
    };
    let mut runner = Runner::new(resolved, options);
    match runner.run(&argv, sink) {
        Ok(outcome) => Ok(outcome.exit_code),
        Err(RuntimeError::SpawnFailed { program, source }) => {
            eprintln!("lacon run: failed to spawn `{}`: {}", program, source);
            Ok(127)
        }
        Err(e) => {
            eprintln!("lacon run: {}", e);
            Ok(1)
        }
    }
}
```

**Phase 2 insertion shape** (between `runner.run()` and `Ok(outcome.exit_code)` — per CONTEXT D-02 "AFTER `Runner::run` returns and BEFORE process exit"):
```rust
match runner.run(&argv, sink) {
    Ok(outcome) => {
        // Phase 2: best-effort tracker write. Bytes already on stdout (D-12).
        if let Err(e) = record_invocation(/* db_path, retention, store_raw_outputs,
                                             outcome, &resolved, &argv, project_path */) {
            eprintln!("lacon: tracker write failed: {e}");  // D-12
        }
        Ok(outcome.exit_code)
    }
    // ... existing error arms unchanged
}
```

**Existing error-arm pattern** (lines 165-172) is the precedent for the `eprintln!("lacon: tracker ...")` D-12 best-effort log.

**Project path source** (line 22):
```rust
let project_path = std::env::current_dir().ok();
```
This is already the right value for `InvocationMeta.project_path` per CONTEXT D-17.

**Env-var read precedent** (`runtime/mod.rs:157`):
```rust
if std::env::var("LACON_DISABLE").as_deref() == Ok("1") {
```
Mirror for `LACON_ASSISTANT` / `LACON_SESSION_ID` per CONTEXT D-17 — same `std::env::var().ok()` shape, but no string equality comparison; just unwrap-or-default.

---

### `crates/lacon-core/Cargo.toml` (add rusqlite)

**Analog:** the same file — every existing dep follows the workspace-inheritance pattern.

**Existing pattern** (lines 8-32 in `crates/lacon-core/Cargo.toml`):
```toml
[dependencies]
regex = { workspace = true }
serde-saphyr = { workspace = true }
etcetera = { workspace = true }
# ...
```

**Phase 2 addition** (per CONTEXT D-07):
```toml
# Used by Phase 2 (tracking: SQLite for invocations + raw_outputs + suspected_regressions)
rusqlite = { workspace = true }
```

---

### `Cargo.toml` (root, workspace.dependencies)

**Analog:** the same file (lines 13-32).

**Existing pattern** (lines 13-32):
```toml
[workspace.dependencies]
regex = "1"
serde = { version = "1", features = ["derive"] }
serde-saphyr = "0.0.26"
# ...
etcetera = "0.11"
rust-embed = "8"
```

**Phase 2 addition** (per RESEARCH §"Cargo wiring" lines 110-118 + CONTEXT D-07):
```toml
rusqlite = { version = "0.39", features = ["bundled"] }
```

---

### `crates/lacon-core/tests/tracking_*.rs` (5 new integration tests)

**Analog:** `crates/lacon-core/tests/rules_loader.rs` — same crate's existing integration test file.

**Header + helper pattern** (from `rules_loader.rs:1-28`):
```rust
//! Integration tests for RuleLoader: resolve, mtime cache, layer fallback, error cases.

use std::path::PathBuf;

use lacon_core::error::ValidationError;
use lacon_core::rules::loader::RuleLoader;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("rules")
}

fn setup_project_with_rules(files: &[(&str, &str)]) -> tempfile::TempDir {
    let tmp = tempfile::TempDir::new().unwrap();
    let rules_dir = tmp.path().join(".lacon").join("rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    for (name, content) in files {
        std::fs::write(rules_dir.join(name), content).unwrap();
    }
    tmp
}
```

**Mirror for `tracking_tracker.rs`** (per RESEARCH §"Wave 0 Gaps"):
```rust
//! Integration tests for Tracker: open + write golden path; XDG-on-macOS smoke.

use lacon_core::error::TrackingError;
use lacon_core::tracking::Tracker;

fn setup_tempdir_db() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("history.db");
    (tmp, db_path)
}
```
**Key difference vs `rules_loader.rs`:** RESEARCH Pitfall 4 prefers passing `db_path: &Path` directly into `Tracker::open` rather than env-var-overriding XDG_DATA_HOME — same pattern as `RuleLoader::new(project_dir: Option<PathBuf>)` at `loader.rs:110`.

**Test naming convention** (from `rules_loader.rs:32-43`):
```rust
#[test]
fn resolve_valid_simple() { /* ... */ }
```
Phase 2 uses snake_case names mapping directly to the RESEARCH "Phase Requirements → Test Map" table (e.g., `migration_creates_all_objects`, `fk_cascade_on_invocation_delete`, `prune_throttled_within_24h`).

---

### `crates/lacon-cli/tests/tracking_e2e.rs` (full CLI lap)

**Analog:** `crates/lacon-cli/tests/end_to_end.rs` — same crate, same pattern.

**Header + emitter resolution** (from `end_to_end.rs:1-32`):
```rust
//! Workspace-level end-to-end integration tests for Phase 1.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::PathBuf;
use tempfile::tempdir;

fn write_rule(dir: &std::path::Path, content: &str) {
    let rules_dir = dir.join(".lacon").join("rules");
    fs::create_dir_all(&rules_dir).unwrap();
    fs::write(rules_dir.join("test.yaml"), content).unwrap();
}

fn test_emitter_path() -> PathBuf {
    assert_cmd::cargo::cargo_bin("test_emitter")
}
```

**Test body shape** (from `end_to_end.rs:34-78`):
```rust
#[test]
fn end_to_end_strip_ansi_and_drop_stderr() {
    let dir = tempdir().unwrap();
    let emitter_path = test_emitter_path();
    let emitter_name = emitter_path.file_name().unwrap().to_str().unwrap();

    write_rule(dir.path(), &format!(/* ... */));

    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .args(["run", "--rule", /* ... */])
        .assert()
        .success()
        .stdout(predicate::str::contains("line 1"));
}
```

**Phase 2 extensions** (per RESEARCH §"Wave 0 Gaps" + Pitfall 4 — env-var route used HERE because the binary owns the etcetera call, not the test):
```rust
Command::cargo_bin("lacon")
    .unwrap()
    .current_dir(dir.path())
    .env("XDG_DATA_HOME",   tmp_data.path())
    .env("XDG_CONFIG_HOME", tmp_config.path())
    .args(["run", "--rule", "e2e-tracking", "--", emitter_path.to_str().unwrap()])
    .assert()
    .success();

// Then assert the DB file landed where expected and contains a row.
let db_path = tmp_data.path().join("lacon").join("history.db");
assert!(db_path.exists(), "tracker DB created at XDG_DATA_HOME/lacon/history.db");
```

---

### `crates/lacon-cli/tests/tracking_coldstart.rs` (negative test)

**Analog:** `crates/lacon-cli/tests/cli_run.rs` (lines 1-13 helper, 15-44 assert_cmd shape).

**Pattern** (from `cli_run.rs:14-44`):
```rust
#[test]
fn run_with_rule_filters_output() {
    let dir = tempdir().unwrap();
    Command::cargo_bin("lacon")
        .unwrap()
        .current_dir(dir.path())
        .args(["run", /* ... */])
        .assert()
        .success();
}
```

**Phase 2 negative-test shape** (per RESEARCH §"Wave 0 Gaps" — assert DB is NOT created):
```rust
#[test]
fn version_does_not_open_db() {
    let tmp_data = tempdir().unwrap();
    Command::cargo_bin("lacon")
        .unwrap()
        .env("XDG_DATA_HOME", tmp_data.path())
        .args(["--version"])
        .assert()
        .success();

    let db_path = tmp_data.path().join("lacon").join("history.db");
    assert!(!db_path.exists(), "--version must NOT create the tracker DB");
}
```

---

## Shared Patterns

### `thiserror`-derived error enums in `lacon-core`
**Source:** `crates/lacon-core/src/error.rs:15-126`
**Apply to:** all new tracking modules (return `Result<_, TrackingError>` from public fns)
```rust
#[derive(thiserror::Error, Debug)]
pub enum SomeError {
    #[error("category: descriptive message with {context}")]
    Variant { context: String, source: std::io::Error },
}
```
Phase 1 D-03 contract: `thiserror` inside crates, `anyhow` at the CLI boundary. The CLI `run.rs` already uses `anyhow::Result<i32>` (line 16); Phase 2 tracker errors get `eprintln!` then swallowed (D-12), never bubbling up via `?`.

### Lazy-on-the-hot-path
**Source:** `crates/lacon-core/src/rules/loader.rs:123-151` (`resolve` does no work for layers that can't match; eager `load_all` is a separate function for `validate`)
**Apply to:** `Tracker::open` lives in `crates/lacon-cli/src/commands/run.rs` and is reachable from no other code path. Per CONTEXT D-04: `--version`, `validate`, `doctor` MUST NOT call `Tracker::open`.

### XDG path resolution via `etcetera`
**Source:** `crates/lacon-core/src/rules/loader.rs:111-113`
```rust
let user_dir = etcetera::choose_base_strategy()
    .ok()
    .map(|s| s.config_dir().join("lacon").join("rules"));
```
**Apply to:** `Tracker::open` call site in `crates/lacon-cli/src/commands/run.rs` — use `s.data_dir()` (NOT `config_dir()`), join `"lacon"`, then `"history.db"`. RESEARCH §"Filesystem & Permissions" confirms macOS returns Xdg too — no platform branch needed.

### Module facade re-exports
**Source:** `crates/lacon-core/src/rules/mod.rs:3-12`
```rust
pub mod bundled;
pub mod loader;
pub mod schema;

pub use loader::{RuleLoader, ResolvedRule, RuleSource};
pub use schema::{ /* ... */ };
```
**Apply to:** `crates/lacon-core/src/tracking/mod.rs` should re-export `Tracker`, `TrackingError` (from `error.rs`), and the `RawOutput` type so call sites use `lacon_core::tracking::Tracker` rather than `lacon_core::tracking::tracker::Tracker`.

### `lib.rs` module declaration
**Source:** `crates/lacon-core/src/lib.rs:14-20`
```rust
pub mod error;
pub mod config;
pub mod rules;
pub mod pipeline;
pub mod starlark_host;
pub mod runtime;
pub mod validate;
```
**Apply to:** add `pub mod tracking;` to `crates/lacon-core/src/lib.rs` at the same indentation/style. Update the module-map doc comment block (lines 5-12) to document `tracking` (PLAN-08 / Phase 2).

### Tempdir-based integration test scaffolding
**Source:** `crates/lacon-core/tests/rules_loader.rs:11-28`
**Apply to:** all new `tracking_*.rs` test files — accept a `db_path: &Path` directly into `Tracker::open` (Pitfall 4 mitigation), build it from `TempDir::new()` per test for cargo-test parallel safety.

### CLI integration test scaffolding (assert_cmd)
**Source:** `crates/lacon-cli/tests/end_to_end.rs:13-32`, `cli_run.rs:1-13`
**Apply to:** `tracking_e2e.rs` and `tracking_coldstart.rs` — `Command::cargo_bin("lacon")`, `tempdir()`, `.env("XDG_DATA_HOME", tmp.path())` for binary-side XDG override.

### Best-effort error logging at CLI boundary (D-12)
**Source:** `crates/lacon-cli/src/commands/run.rs:165-172`
```rust
Err(RuntimeError::SpawnFailed { program, source }) => {
    eprintln!("lacon run: failed to spawn `{}`: {}", program, source);
    Ok(127)
}
Err(e) => {
    eprintln!("lacon run: {}", e);
    Ok(1)
}
```
**Apply to:** Phase 2 tracker error handling per CONTEXT D-12 — `eprintln!("lacon: tracker {phase} failed: {e}")` with no exit-code change.

---

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `crates/lacon-core/src/tracking/migrations/0001_initial.sql` | DDL fixture | one-shot DDL | First SQL file in the repo; no in-tree precedent. Use the byte-exact DDL skeleton from RESEARCH §"M0001 SQL skeleton" (lines 302-405) — already specified to byte-exactness against `docs/specs/tracking-data-model.md:14-141`. |
| `crates/lacon-core/src/tracking/health.rs` (`health_check` function) | probe | request-response | No existing "no-op DB round-trip" pattern. Closest external reference: the standard `SELECT 1` health-probe idiom; rusqlite docs (Context7 `/websites/rs_rusqlite_0_39_0_rusqlite`, RESEARCH "Sources" line 721) cover `query_row` + `pragma_query_value` shapes used inline. Keep the function < 20 lines and unit-test it directly. |

For the SQL file: planner should reference `docs/specs/tracking-data-model.md:14-141` as the authoritative source. RESEARCH already transcribed it byte-exact (lines 302-405) including the `DROP VIEW IF EXISTS` pattern (D-09), the `HAVING COUNT(*) > 5` clause on `v_bypass_rate`, and the `INSERT INTO lacon_meta (key, value) VALUES ('last_pruned_ts', '0')` seed. **Order in the file matters:** `invocations` first, then `raw_outputs` (forward-FK from invocations is fine in SQLite — RESEARCH line 407 cites sqlite.org/foreignkeys.html §1).

---

## Metadata

**Analog search scope:**
- `crates/lacon-core/src/{rules,runtime,config,validate}/` — sibling-module precedent for `tracking/`
- `crates/lacon-core/src/error.rs` — `thiserror` enum precedent
- `crates/lacon-core/src/rules/{loader.rs,bundled.rs}` — XDG, lazy hot path, compile-time embedding
- `crates/lacon-cli/src/commands/run.rs` — tracker call-site
- `crates/lacon-{core,cli}/tests/*.rs` — test scaffolding

**Files scanned:** 18 source files across both crates + workspace + crate Cargo.toml manifests.

**Pattern extraction date:** 2026-05-06

**Key project-conventions confirmed (from `CLAUDE.md`):**
- Streaming, not buffered (does not apply to tracker write — single sync INSERT)
- Cold start under 10ms (load-bearing for Phase 2)
- `thiserror` inside crates, `anyhow` at CLI boundary
- No async runtime — `rusqlite` sync is consistent
- Migrations are append-only (Phase 2 ships M0001; future migrations append to dispatch table)
- Bundled assets via `rust-embed` OR inline `const` — Phase 2 picks `include_str!` per CONTEXT D-08

## PATTERN MAPPING COMPLETE
