# Phase 2: Local tracking - Context

**Gathered:** 2026-05-06 (assumptions mode)
**Status:** Ready for planning

<domain>
## Phase Boundary

Every `lacon run` invocation persists a row to a SQLite database at `~/.local/share/lacon/history.db` with the v1 privacy contract intact, the four required views queryable, and pruning happening at startup — without breaking the cold-start budget.

Subsumes: rusqlite dependency add, schema migration mechanism, the three tables (`invocations`, `raw_outputs`, `suspected_regressions`), four views (`v_unmatched_offenders`, `v_filtered_offenders`, `v_bypass_rate`, `v_project_savings`), retention/pruning, the privacy contract (off-by-default `raw_outputs`, one-time off→on warning, marker file), and wiring the tracker into `lacon-cli/src/commands/run.rs` after `Runner::run` returns.

Out of scope: `lacon stats` / `lacon explain` query commands (Phase 4), the Claude Code adapter that supplies `session_id` / `assistant` (Phase 3 — but the env-var contract is defined here), per-token accounting (v2), automatic redaction (v2), `lacon purge` (v2).

**Requirements covered:** REQ-tracking-sqlite-location, REQ-tracking-schema, REQ-tracking-raw-outputs-default-off, REQ-tracking-privacy-warning, REQ-tracking-retention-defaults.
</domain>

<decisions>
## Implementation Decisions

### A. Crate layout & module placement

- **D-01:** Tracker lives in `crates/lacon-core/src/tracking/` as a sibling of `runtime/`, `config/`, `rules/`, `validate/`. Public surface:
  - `Tracker::open(db_path: &Path) -> Result<Tracker, TrackingError>` — opens (or creates) the DB, applies migrations, runs throttled pruning, returns a handle.
  - `Tracker::record(&self, meta: &InvocationMeta, raw: Option<&RawOutput>) -> Result<i64, TrackingError>` — single INSERT into `invocations`; conditional INSERT into `raw_outputs` when `raw` is `Some` AND the project's `store_raw_outputs` is true.
  - `Tracker::prune(&self, retention: &Retention) -> Result<PruneStats, TrackingError>` — internal-but-public-for-tests; called by `open()`.
  - `RawOutput { stdout: Vec<u8>, stderr: Vec<u8> }` — passed through from the runtime when raw retention is active.
- **D-02:** Call-site is `crates/lacon-cli/src/commands/run.rs` — both `run_with_rule` and `run_unmatched`. Tracker open happens lazily AFTER `Runner::run` returns and BEFORE process exit. Filtered bytes already reached stdout by then (per `Runner::run`'s contract), so a tracker failure can never delay or block the assistant's tool result.
- **D-03:** `InvocationMeta` (already defined at `crates/lacon-core/src/runtime/mod.rs:90-113`) is the hand-off struct. Phase 2 EXTENDS it (adds `assistant: String`, `session_id: Option<String>`, `project_path: Option<PathBuf>`, `command_normalized: String`, `raw_output_id: Option<i64>`) rather than redefining a parallel struct. Field additions are additive and do not break Phase 1 callers — Phase 1 currently only constructs `RunOutcome`, not `InvocationMeta`, so the assembly point moves into the CLI command.

### B. Cold-start strategy

- **D-04:** DB connection is opened LAZILY — only on the path that writes (i.e. only inside `crates/lacon-cli/src/commands/run.rs` post-`Runner::run`). `lacon --version`, `lacon validate <path>`, and `lacon doctor` (Phase 4 query mode that doesn't write) MUST NOT open the DB. Cold-start budget headroom from Phase 1 measurements: ~8.7ms (Phase 1 binary `--version` median 1154µs, well under the 10ms ceiling).
- **D-05:** Migrations run via SQLite's `PRAGMA user_version`. On open: read `user_version`, apply each unapplied migration in a single transaction, set the new `user_version`. No external migration crate.
- **D-06:** Pruning is throttled. `lacon_meta(key TEXT PRIMARY KEY, value TEXT)` table holds `last_pruned_ts` (unix ms). On `Tracker::open`, if `now - last_pruned_ts > 86_400_000` (24h), run the three `DELETE FROM ... WHERE ts < ?` statements and update `last_pruned_ts`. Otherwise skip. New users / first-run apply pruning immediately (`last_pruned_ts` absent → treated as 0).
- **D-07:** `rusqlite` added to `[workspace.dependencies]` with `features = ["bundled"]`. Hermetic by construction — no system `libsqlite3-dev` requirement, no version skew between macOS/Linux CI lanes. Trade-off (binary size +~1 MiB) is acceptable per REQ-acceptance-test-coverage hermeticity goal. Re-evaluate `system` feature only if Phase 6 binary-size targets demand it.

### C. Schema migration mechanism

- **D-08:** Migrations are inline `const` SQL strings in `crates/lacon-core/src/tracking/migrations.rs`. Single migration `M0001_INITIAL` for v1 covers:
  - `CREATE TABLE invocations` (full column set per `docs/specs/tracking-data-model.md:14-39`)
  - `CREATE TABLE raw_outputs` (per spec lines 46-52)
  - `CREATE TABLE suspected_regressions` (per spec lines 56-61)
  - The 4 indexes on `invocations` (`idx_inv_ts`, `idx_inv_cmd`, `idx_inv_rule`, `idx_inv_project`)
  - `idx_raw_created` on `raw_outputs(created_ts)`
  - `idx_reg_inv` on `suspected_regressions(invocation_id)`
  - The 4 views (`v_unmatched_offenders`, `v_filtered_offenders`, `v_bypass_rate`, `v_project_savings`) — DDL byte-exact per spec lines 96-141, including `HAVING COUNT(*) > 5` on `v_bypass_rate`.
  - `CREATE TABLE lacon_meta` — for `last_pruned_ts` and any future single-row metadata.
- **D-09:** Migration application is wrapped in a single `BEGIN; ... COMMIT;` transaction. View definitions inside migrations follow the `DROP VIEW IF EXISTS <name>; CREATE VIEW <name> AS ...` pattern so future migrations can safely re-define a view without orphan checks.
- **D-10:** Schema invariants from spec are enforced at migration time:
  - `invocations.raw_output_id` → `raw_outputs(id)` ON DELETE SET NULL
  - `suspected_regressions.invocation_id` → `invocations(id)` ON DELETE CASCADE
  - WAL mode applied via `PRAGMA journal_mode=WAL` AT EACH `Connection::open` (not migration-bound — WAL is a connection-time pragma).
- **D-11:** WAL `busy_timeout` set to **200ms** at every connection open (`PRAGMA busy_timeout = 200`). Justified by ADR-0011's "WAL handles concurrent writes safely" claim — gives a sibling `lacon run` from a parallel Claude session room to commit before we'd hard-fail the tracker write.

### D. Tracker write failure handling

- **D-12:** Tracker writes are BEST-EFFORT. The contract for `lacon run` from ADR-0013 ("filtered bytes reach assistant + propagate subprocess exit code") is preserved unconditionally. Implementation:
  - `Tracker::open` failure → log `lacon: tracker init failed: <err>` to stderr ONCE, then disable the tracker for the remainder of the invocation. `Runner::run` already wrote filtered bytes to stdout before this code path runs.
  - `Tracker::record` failure → log `lacon: tracker write failed: <err>` to stderr; do not change the wrapper's exit code.
  - `Tracker::prune` failure → log `lacon: tracker prune failed: <err>` to stderr; continue.
- **D-13:** Tracker errors are surfaced structurally by `lacon doctor` (Phase 4) — Phase 2 exposes a `Tracker::health_check()` helper that performs a no-op write/read against the DB and returns a structured result. Phase 4 calls it; Phase 2 just defines it.

### E. Privacy marker file & env-var contract for adapter

- **D-14:** Privacy warning marker is a **zero-byte sentinel file**. Locations (resolved at marker check time):
  - Project-layer opt-in (`store_raw_outputs: true` in `<cwd>/.lacon/config.yaml`): `<cwd>/.lacon/.store_raw_outputs_acked`.
  - User-layer opt-in (`store_raw_outputs: true` in `~/.config/lacon/config.yaml` AND project layer absent or unset): `~/.config/lacon/.store_raw_outputs_acked`.
  - Bundled-default opt-in is impossible (default is false), so no third location.
- **D-15:** Warning is checked exactly once per invocation, BEFORE the first would-be `raw_outputs` INSERT. If `store_raw_outputs` resolves to true AND no marker file exists at the resolved location: print the warning to stderr, then `touch` the marker file. Subsequent invocations short-circuit on marker presence.
- **D-16:** Warning text is FIXED (deterministic for testing):
  ```
  lacon: store_raw_outputs is enabled.
  lacon: raw stdout/stderr will be retained at ~/.local/share/lacon/history.db
  lacon: for up to 3 days. Disable in <config-path> or run `rm` on the DB.
  lacon: this notice is shown once per project (marker: <marker-path>).
  ```
  The `<config-path>` and `<marker-path>` are interpolated; everything else is byte-stable. Suppressing the marker file (`rm <marker-path>`) re-triggers the notice — that's the designed undo.
- **D-17:** Phase 1 `InvocationMeta` deliberately omits `assistant` and `session_id`. Phase 2 defines the env-var contract that Phase 3 (the adapter) MUST satisfy:
  - `LACON_ASSISTANT` (default `"claude-code"` if unset) → `invocations.assistant`. Required to be NOT NULL per spec; the default carries it.
  - `LACON_SESSION_ID` (default unset → `NULL`) → `invocations.session_id`. Spec marks the column nullable (`docs/specs/tracking-data-model.md:18`).
  - `project_path` → `std::env::current_dir()` (already used in `crates/lacon-cli/src/commands/run.rs:22`).
  - `command_raw` → `argv.join(" ")` (cosmetic, used for display in `lacon stats` and `lacon explain`).
  - Precedent: `LACON_DISABLE` is the existing env-var control surface (`crates/lacon-core/src/runtime/mod.rs:157`).
- **D-18:** `command_normalized` derivation per spec (`docs/specs/tracking-data-model.md:72`): `<basename(argv[0])> <argv[1]>` for known package-manager-style commands; otherwise just `basename(argv[0])`. Phase 2 implements a conservative initial form: basename + first-arg if first-arg does not start with `-`, else basename only. Lives in `crates/lacon-core/src/tracking/normalize.rs` as a pure `fn normalize(argv: &[String]) -> String`. The exact normalization is implementation-defined (spec says "may improve over time").

### Implementation-time benchmarks for the planner to schedule into Phase 2

These are not gating decisions but measurements to take during Phase 2 work — the planner should fold them into a benchmark plan:

1. **`rusqlite` cold-start on the hot path** (`Connection::open` + `PRAGMA journal_mode=WAL` + `PRAGMA busy_timeout=200` + `user_version` check + a single `INSERT INTO invocations ... VALUES (...)`). Target: ≤2.5ms additional cost on top of Phase 1's ~1.2ms. If exceeded, the lazy-open + 24h prune throttle (D-04, D-06) becomes mandatory rather than belt-and-suspenders.
2. **First-time migration cost** (apply migration `0001` against an empty DB file). Should be one-time per machine; measure to confirm it's <50ms so first-run UX is acceptable.
3. **WAL contention** under simulated concurrent `lacon run` from parallel sessions — 200ms `busy_timeout` (D-11) sized for plausible CC-session concurrency. If the test harness exposes contention, raise to 500ms or surface as v2 backlog.

### Claude's discretion

- Internal layout under `crates/lacon-core/src/tracking/` (`mod.rs`, `migrations.rs`, `normalize.rs`, `health.rs` etc.) — organize for readability without re-litigating crate boundaries.
- Choice of `chrono::Utc::now()` vs `std::time::SystemTime` for unix-ms timestamps — both work; pick whichever has lower cold-start cost (likely `SystemTime`).
- Exact SQL parameter binding style (`?1`/`?2` positional vs `:name` named) — both valid in `rusqlite`; pick whichever reads cleaner at the call site.
- Choice between `rusqlite` `Connection::open_with_flags` (explicit `SQLITE_OPEN_CREATE | SQLITE_OPEN_READWRITE`) and the default `Connection::open` — both create-if-missing; pick by preference.

### Folded todos

None — `gsd-sdk query todo.match-phase 2` returned 0 matches.
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### ADRs (LOCKED, all status: Accepted)

- `docs/decisions/0009-separated-raw-outputs.md` — split table, 30/3-day retention, off-by-default
- `docs/decisions/0011-sqlite-for-tracking.md` — `rusqlite`, WAL mode, append-only migrations on startup, no daemon
- `docs/decisions/0013-filter-via-pretooluse-wrapper.md` — `lacon run` is the production hot path; cold-start is load-bearing

### Specs (load-bearing contract)

- `docs/specs/tracking-data-model.md` — full SQLite schema (3 tables, 6 indexes, 4 views), retention table, privacy contract, migration policy
- `docs/specs/config-schema.md` — USER-ONLY `retention.*`, project-or-user `store_raw_outputs`, layer merge semantics, validation error format

### Architecture and project context

- `docs/architecture.md` — places tracker inside `lacon-core`; component boundaries
- `docs/v1-scope.md` — explicit in-scope/out-of-scope (no `lacon purge`, no automatic redaction in v1)
- `docs/open-questions.md` — Privacy and `raw_outputs` resolution (2026-05-06 entry)
- `.planning/PROJECT.md`, `.planning/REQUIREMENTS.md`, `.planning/ROADMAP.md`
- `.planning/phases/01-engine-core-lacon-run-wrapper/01-CONTEXT.md` — Phase 1 D-01..D-18 (esp. D-03 dep set, D-13 dual-buffer model)
- `.planning/intel/constraints.md` — CON-tracking-* (8 entries), CON-config-* (5 entries), CON-nfr-cold-start-budget

### Phase 1 source files Phase 2 directly extends

- `crates/lacon-core/src/runtime/mod.rs:90-113` — `InvocationMeta` struct (extend, do not redefine)
- `crates/lacon-core/src/runtime/mod.rs:71-85` — `RunOutcome` (consumed by tracker assembly in CLI)
- `crates/lacon-core/src/config/mod.rs` — `EngineConfig`, `Retention`, `retention_precheck` (consumed unchanged)
- `crates/lacon-core/src/rules/loader.rs:32, 109-111` — `etcetera::choose_base_strategy` pattern (mirror for `~/.local/share/lacon/`)
- `crates/lacon-cli/src/commands/run.rs` — wire-up site (post-`Runner::run`, pre-process-exit)
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- **`InvocationMeta` struct** (`crates/lacon-core/src/runtime/mod.rs:90-113`) — Phase 1 pre-staged this for Phase 2. Fields ts_unix_ms, rule_id, rule_source, command_raw, argv, exit_code, duration_ms, byte_counts, bypassed, rewritten, truncated_by_max_bytes already populated by the runtime caller. Phase 2 EXTENDS with `assistant`, `session_id`, `project_path`, `command_normalized`, `raw_output_id`.
- **`EngineConfig` + `Retention`** (`crates/lacon-core/src/config/mod.rs`) — already loaded with the per-key deep merge; `retention_precheck` already enforces USER-ONLY for project layer (T-03-06). Tracker just consumes `cfg.retention` at prune time and `cfg.store_raw_outputs` at write time.
- **`etcetera::choose_base_strategy()`** — used in `crates/lacon-core/src/rules/loader.rs:111` for `~/.config/lacon/`. Same pattern resolves `~/.local/share/lacon/` (data_dir, not config_dir).
- **`thiserror`-derived error enums** — Phase 1 precedent in `crates/lacon-core/src/error.rs`. Phase 2 adds `TrackingError` following the same shape.

### Established Patterns

- **Streaming, no buffering** (ADR-0005) — does not directly apply to tracking, but the tracker write is a single sync INSERT after pipeline completion (NOT per-line).
- **Lazy-resolve-on-the-hot-path** (Phase 1 D-14) — same posture for the tracker: open/migrate/prune only on write paths; never on `--version` / `validate`.
- **`thiserror` inside crates, `anyhow` at the CLI boundary** (Phase 1 D-03) — Phase 2 follows.
- **No async runtime** (Phase 1 D-04) — `rusqlite` is sync; this is consistent. No `tokio-rusqlite`, no `sqlx`.
- **Bundled assets via `rust-embed` or inline `const`** (Phase 1 D-03 / `crates/lacon-core/src/rules/bundled.rs`) — migrations use inline `const` per D-08 since they're not iterated like rule files.

### Integration Points

Phase 2 outputs that downstream phases consume:

- **For Phase 3 (adapter):** the env-var contract (`LACON_ASSISTANT`, `LACON_SESSION_ID`) defined in D-17 is what the Claude Code adapter populates from hook context. Phase 3 owns no schema work.
- **For Phase 4 (`lacon stats` / `explain` / `doctor`):** the four views queried by `lacon stats` are created in migration `0001`. The `Tracker::health_check()` helper (D-13) is what `lacon doctor` calls. `lacon explain` reads from the `raw_outputs` table — Phase 2 defines the storage; Phase 4 defines the read path.
- **For Phase 6 (acceptance):** `REQ-acceptance-cold-start-budget` is verified with the tracker active. The benchmarks in this section gate whether the throttle (D-06) and `bundled` feature (D-07) hold up.

### Performance contract

The cold-start budget is load-bearing (CON-nfr-cold-start-budget, ADR-0013). Phase 1 baseline: `--version` 1154µs, `validate` 1259µs (STATE.md:87). Headroom: ~8.7ms. Tracker open + migrate-skip + INSERT must consume well under 3ms for the 10ms-on-the-hook contract to hold with safety margin. This is measured, not asserted.
</code_context>

<specifics>
## Specific Ideas

No specific user references — assumptions confirmed as-is on first pass. Approaches above are derived from locked ADRs/specs (`docs/specs/tracking-data-model.md`, `docs/specs/config-schema.md`, ADRs 0009/0011/0013) plus the patterns Phase 1 established for the cold-start hot path.
</specifics>

<deferred>
## Deferred Ideas

- **Per-token accounting columns** — explicitly v2 backlog. Schema is forward-compatible: new INTEGER columns can be appended via migration `0002`+ without breaking the v1 reader.
- **Automatic redaction of `raw_outputs`** — explicitly v2 backlog. False-confidence risk per `docs/open-questions.md` "Privacy and `raw_outputs` — resolved".
- **`lacon purge` subcommand** — would push CLI past the 6-command surface (REQ-cli-surface-cap). v2 backlog. Manual cleanup via `rm history.db` is the v1 pattern.
- **Cross-machine sync of `history.db`** — v2 backlog (`rsync`/Drive overlap is user-driven; lacon doesn't sync).
- **`PRAGMA mmap_size` / synchronous=NORMAL tuning** — initial config is `WAL` + default `synchronous=NORMAL` (WAL's default). If Phase 2 benchmarks reveal commit cost dominates, tune in Phase 6.
- **Encrypted `raw_outputs` BLOBs** — v2 backlog. v1 protection is `0700` dir + off-by-default + opt-in warning.
- **Marker-file durability in ephemeral devcontainers/codespaces** — README-level documentation note (v2 polish), not a v1 blocker. The marker re-firing on rebuild is acceptable behaviour for v1.
- **Refinery / sqlx-migrate** — overkill for v1's single migration. Revisit only if migrations grow past ~5 entries.

### Reviewed Todos (not folded)

None reviewed — `gsd-sdk query todo.match-phase 2` returned 0 matches.
</deferred>
