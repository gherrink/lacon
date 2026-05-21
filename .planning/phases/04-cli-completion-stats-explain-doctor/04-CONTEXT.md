# Phase 4: CLI completion (`stats`, `explain`, `doctor`) - Context

**Gathered:** 2026-05-21 (assumptions mode)
**Status:** Ready for planning

<domain>
## Phase Boundary

The remaining four CLI commands ship and the binary's command surface is hard-capped at six:

- `lacon stats` summarizes tracking data — top offenders, bypass rates, unmatched commands derived from the four views — and accepts `--project`, `--since`, `--rule` filters that narrow the output correctly.
- `lacon explain <id>` re-runs the rule's pipeline against the **stored raw output** for invocation `<id>` and renders a side-by-side raw-vs-filtered view, exiting with a clear error when raw retention was disabled at the time of the original invocation.
- `lacon doctor` reports green when hooks are installed, every layer's `config.yaml` parses, every rule loads and validates, and the DB directory permissions are `0700` — surfacing a per-issue actionable error otherwise.
- `lacon <unknown-subcommand>` returns non-zero with a clap error pointing at the six legitimate subcommands.

Subsumes: a new tracking READ/query surface in `lacon-core` (the write path landed in Phase 2; there is currently zero read API), a byte-replay entry point that runs a pipeline against stored bytes without spawning a subprocess (for `explain`), the doctor health checklist (reusing Phase 2 `health_check`, Phase 3 hook fingerprint, and the Phase 1 validate/loader surface), filling the three stub `execute()` bodies, and threading the already-parsed clap args through `main.rs`.

Out of scope: bundled rule files (Phase 5), end-to-end acceptance + docs (Phase 6), `lacon purge` / `lacon install` / `stats --serve` (v2 backlog — would break the six-command cap), per-token accounting (v2), trend graphs / session rollups / cost estimation (v2), absolute ISO `--since` date parsing (backlog; relative-only in v1).

**Requirements covered:** REQ-cli-stats, REQ-cli-explain, REQ-cli-doctor, REQ-cli-surface-cap.
</domain>

<decisions>
## Implementation Decisions

### A. Tracking read/query API placement

- **D-01:** Add a new read-path module `crates/lacon-core/src/tracking/query.rs` exposing the query surface that `lacon stats` and `lacon explain` call. `lacon-cli` does **NOT** gain a runtime `rusqlite` dependency — it stays a dev-dependency only (`crates/lacon-cli/Cargo.toml:23-30`, added for Phase 2 e2e tests). All SQL lives behind the `lacon-core` `Tracker`/`Connection` boundary, mirroring Phase 2's write path (`record.rs` adds `impl Tracker` methods; the call site at `run.rs:273-280` never inlines SQL). Free functions over `&Connection` are acceptable where a read path doesn't need `Tracker` state — module **location** is the load-bearing decision, not method-vs-free-fn.
- **D-02:** Query commands open the DB **read-only** — add a read-only open helper (e.g. `Tracker::open_readonly` / a free `open_readonly(path)` using `SQLITE_OPEN_READ_ONLY`, **no migrate, no prune**) used by `stats`, `explain`, and `doctor`. Rationale: Phase 2 D-04 frames these commands as non-writing; `Tracker::open` runs migrations + throttled prune (`tracking/mod.rs:104-107`), which are writes. The read-only helper keeps query commands strictly non-mutating. Accepted fallback if the planner finds read-only open impractical: reuse `Tracker::open`, but only on the explicit understanding that "doctor doesn't write" means "doesn't INSERT invocations / doesn't depend on prune side-effects." Either way, **never** write an `invocations` row from a query command.
- **D-03:** Missing-DB is a normal state, not an error. A fresh user who never ran `lacon run` has no `history.db`. `stats` → print a friendly "no data yet" line per section, exit 0. `explain <id>` → clear "no tracked invocations found" error, non-zero exit. `doctor` → report the DB/perms/health checks as "not yet initialized (run a command first)" informational, **not** a hard red failure.

### B. `explain <id>` re-derivation path

- **D-04:** `explain` re-derives filtered output by running the rule's pipeline directly against the **stored** stdout/stderr bytes — it must NOT re-spawn the original command. `Runner::run` (`runtime/mod.rs:189-203`) unconditionally spawns a subprocess, so it cannot be reused. Add a byte-replay entry point — recommended as a `Runner`-side method (e.g. `Runner::filter_bytes(...)`) so the exit-code branch selection and `ScriptCtx` assembly stay colocated with the runtime that authored them (`runtime/mod.rs:342-359`, `:327-333`), rather than duplicated in `lacon-cli`. The replay calls `Pipeline::run_with_post_process` (`pipeline/mod.rs:127-138`, already `pub`, takes `impl Iterator<Item = String>` + `&ScriptCtx`, no subprocess).
- **D-05:** `explain` flow:
  1. Parse `id: String` (clap field, `cli.rs:49`) to `i64` (`invocations.id` is INTEGER) — clap-error or clean message on non-numeric input.
  2. SELECT the invocation row → `rule_id`, `raw_output_id`, `exit_code`, plus `command_raw` / `duration_ms` / `project_path` for `ScriptCtx`.
  3. If `raw_output_id` is NULL → exit with a clear error: raw retention was disabled at the time of this invocation (point at `store_raw_outputs`). This is SC2's required failure path.
  4. Load the stored BLOBs from `raw_outputs`.
  5. Resolve the rule via `RuleLoader::resolve(rule_id)` (`loader.rs:127-151`); `ResolvedRule`'s `success_pipeline` / `on_error_pipeline` / `post_process` / `on_error_post_process` are public fields (`loader.rs:59-77`).
  6. Select the branch by the **stored** `exit_code` (mirror `runtime/mod.rs:342-359`): non-zero → `on_error_pipeline` (+ `on_error_post_process`); zero → `success_pipeline` (+ `post_process`). Required for byte-for-byte reproduction (Phase 6 SC3) of runs that exited through an `on_error` block.
- **D-06:** Side-by-side rendering is **hand-rolled** — a simple two-column raw-vs-filtered renderer, no LCS/Myers diff and no new diff-crate dependency. Consistent with the project's lean-deps posture (CLAUDE.md) and Phase 3's hand-rolled splitter/quote/JSON-walk precedents. SC2's "side-by-side diff" is satisfied by a raw|filtered presentation; byte-for-byte reproducibility (Phase 6) concerns the *filtered output*, not the diff visualization. **Documented escape hatch:** if a true aligned line-diff proves necessary, adopt `similar` (the de-facto Rust diff crate, already transitively in the build via the `insta` dev-dep) as a first-class `lacon-cli` dependency — deferred unless the hand-rolled view under-delivers.

### C. `doctor` checklist, reuse surface, exit semantics

- **D-07:** `lacon doctor` runs a fixed checklist, printing one pass/fail line per item, exit `0` only if all checks pass (non-zero if any fails). Checks:
  1. **Hook install** — read `<cwd>/.claude/settings.json`; walk `hooks.PreToolUse[]` for a Bash matcher whose inner `command` starts with `"lacon-claude-hook"` (the Phase 3 D-12/D-28 fingerprint; reuse the JSON-walk shape from `init.rs:143-145` / the `bash_lacon_commands` test helper at `init.rs:318-329`).
  2. **Config validity per layer** — call `validate::validate_file` (`validate/mod.rs:45`) on each existing `config.yaml`: project (`<cwd>/.lacon/config.yaml`) and user (`~/.config/lacon/config.yaml`). Per `config-schema.md:119` doctor must validate every layer's config.
  3. **Rule sweep** — `RuleLoader::load_all()` (`loader.rs:156`); report any returned validation errors with the offending path.
  4. **DB dir perms** — stat the parent of the resolved DB path via `std::fs::metadata`; assert `0700` (set at `ensure_data_dir`, `tracking/mod.rs:165-190`). If the DB/dir doesn't exist yet → informational per D-03, not a failure.
  5. **Tracker health** — call `tracking::health::health_check(&conn)` (`health.rs`, a read-only `SELECT 1`); Phase 2 D-13 names doctor as its sole intended caller.
- **D-08:** Doctor's DB-touching checks (4, 5) use the D-02 read-only open. Doctor must not migrate, prune, or INSERT.

### D. `stats` filters, output, `--since` parsing, arg threading

- **D-09:** `--project` / `--since` / `--rule` are applied as parameterized re-queries against the base `invocations` table — re-implementing each view's `GROUP BY` / `ORDER BY` body with an added `WHERE` — **not** by filtering the four views directly. Reason: the views carry differing columns and **none expose `ts`** (`tracking-data-model.md:96-141`, byte-exact per Phase 2 D-08): `v_unmatched_offenders` selects only `command_normalized, runs, total_raw_bytes`; `v_bypass_rate` has `rule_id` but no `project_path`; only `v_project_savings` carries `project_path`. The unfiltered sections may still read straight from the views; filtered sections re-query the base table (indexed `idx_inv_ts` / `idx_inv_project` / `idx_inv_rule`).
- **D-10:** `--since` accepts **relative** forms only in v1 (`7d`, `24h`, `30m`-style) → resolved to a cutoff `ts` in unix-ms. Absolute ISO-date parsing is deferred to backlog to avoid pulling a date crate (`chrono`/`time` are not workspace deps). Document ISO `--since` as a backlog enhancement.
- **D-11:** Empty-DB / no-rows is handled gracefully per section ("no data yet"), exit 0 (see D-03). Output is plain-text tables/sections — no color dependency required; keep it readable in a terminal and snapshot-testable (the `insta` dev-dep is available for output snapshot tests).
- **D-12:** `main.rs:15-16` currently calls `stats::execute()` / `explain::execute()` and **discards** the parsed clap args (`{ .. }`). Phase 4 changes those `execute()` signatures to thread `project` / `since` / `rule` (stats) and `id` (explain) through from the already-declared `cli.rs` fields.

### E. Six-command surface cap (REQ-cli-surface-cap)

- **D-13:** The cap is **already satisfied** — `cli.rs:18-54` declares exactly Run / Validate / Init / Stats / Explain / Doctor, and `crates/lacon-cli/tests/cli_surface.rs:6-41` already asserts exactly six subcommands and rejects unknown ones with a non-zero clap error. SC4 needs no new gating code; Phase 4 only keeps this test green (and may add an assertion that `purge` / `install` / `stats --serve` are absent if not already covered). No new subcommand may be introduced by this phase.

### Claude's discretion

- Internal organization of `tracking/query.rs` (one struct of typed result rows per view vs. ad-hoc tuples) — organize for readability.
- Method-vs-free-function for the read API (D-01) — both acceptable.
- Exact `Runner::filter_bytes` signature and where the byte→lines split lives (D-04) — planner's call, as long as exit-code branch + `ScriptCtx` assembly aren't duplicated into `lacon-cli`.
- Exact column widths / section ordering / wording of the `stats` and `doctor` human-readable output.
- Exact relative-duration grammar for `--since` (whether to support combined forms like `1d12h`) — start minimal.
- Whether to capture `LACON_TOOL_USE_ID` correlation for `explain` (Phase 3 D-26 trailing item) — only if a stored tool-use-id makes `explain`'s lookup stronger; otherwise `id` is the `invocations.id` integer.

### Folded todos

None — `gsd-sdk query todo.match-phase 4` returned 0 matches.
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Specs (load-bearing contract)

- `docs/specs/tracking-data-model.md` — the four view DDLs + columns (drives `stats` queries), `raw_outputs` BLOB shape + `exit_code`/`rule_id`/`raw_output_id` columns (drives `explain`), `command_normalized` derivation, retention/`0700`.
- `docs/specs/config-schema.md` — config validation + per-layer merge; "doctor runs config validation on every layer" requirement.
- `docs/specs/filter-rule-schema.md` — pipeline primitives + `post_process` shape (drives `explain`'s byte-replay re-derivation).
- `docs/specs/chained-commands.md` — hook install shape context for the doctor fingerprint check (Phase 3 reference).

### ADRs (LOCKED, all status: Accepted)

- `docs/decisions/0009-separated-raw-outputs.md` — raw retention off-by-default; `explain` errors when `raw_output_id` is NULL.
- `docs/decisions/0010-on-error-replaces-pipeline.md` — `on_error` branch selection in `explain` replay.
- `docs/decisions/0011-sqlite-for-tracking.md` — DB location, WAL, the read surface `stats`/`explain` query.
- `docs/decisions/0013-filter-via-pretooluse-wrapper.md` — cold-start posture; clarifies `stats`/`explain`/`doctor` are NOT on the hook hot path.
- `docs/decisions/0004-config-precedence.md`, `docs/decisions/0007-first-match-wins.md` — loader/resolve semantics doctor's rule sweep relies on.
- `docs/decisions/0005-streaming-output-processing.md` — pipeline streaming model the `explain` replay reuses.

### Architecture and project context

- `docs/architecture.md` — tracker inside `lacon-core`; component boundaries.
- `docs/v1-scope.md` — six-command cap; `lacon purge`/`install` out of scope.
- `docs/open-questions.md` — design-risk log (no open Phase-4-blocking item).
- `.planning/PROJECT.md`, `.planning/REQUIREMENTS.md`, `.planning/ROADMAP.md`.
- `.planning/intel/constraints.md` — CON-tracking-* (views, retention, schema), CON-config-* (validation dispatch), CON-nfr-cold-start-budget.
- `.planning/phases/02-local-tracking/02-CONTEXT.md` — Phase 2 D-04 (lazy/no-write on query paths), D-13 (`health_check`), D-08 (view DDLs), the env-var contract.
- `.planning/phases/03-claude-code-adapter-lacon-init/03-CONTEXT.md` — Phase 3 D-12/D-28 (`lacon-claude-hook` fingerprint doctor reuses), settings.json walk shape.
- `.planning/phases/01-engine-core-lacon-run-wrapper/01-CONTEXT.md` — Phase 1 pipeline/runtime/loader/validate API surface.

### Existing source files Phase 4 directly extends or fills

- `crates/lacon-cli/src/commands/stats.rs`, `explain.rs`, `doctor.rs` — stubs to fill (currently print "not yet implemented", exit 2).
- `crates/lacon-cli/src/cli.rs:18-54` — subcommands already declared (Stats `--project/--since/--rule`, Explain `id`, Doctor).
- `crates/lacon-cli/src/main.rs:15-16` — call sites that currently discard parsed Stats/Explain args (D-12).
- `crates/lacon-cli/Cargo.toml:23-30` — `rusqlite` is dev-only; keep it that way (D-01).
- `crates/lacon-cli/tests/cli_surface.rs:6-41` — six-command-cap test (keep green, D-13).
- `crates/lacon-core/src/tracking/` — `mod.rs` (open, `ensure_data_dir` 0700 at :165-190, prune at :104-107), `health.rs` (`health_check`), `record.rs` (impl-Tracker write-path precedent), `migrations.rs` (view + table DDL), `normalize.rs`. **Add `query.rs` here (D-01).**
- `crates/lacon-core/src/runtime/mod.rs` — `Runner::run` (always spawns, :189-203), exit-code branch (:342-359), `ScriptCtx` (:327-333). Add the byte-replay entry point (D-04).
- `crates/lacon-core/src/pipeline/mod.rs:127-138` — `run_with_post_process` (pub, no subprocess) — the replay target.
- `crates/lacon-core/src/rules/loader.rs` — `resolve` (:127-151), `load_all` (:156), `ResolvedRule` public fields (:59-77).
- `crates/lacon-core/src/validate/mod.rs:45` — `validate_file` (doctor config check).
- `crates/lacon-cli/src/commands/init.rs:143-145, 318-329` — hook fingerprint walk doctor reuses.
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets

- **`tracking::health::health_check(&conn)`** — Phase 2 D-13 built this read-only `SELECT 1` probe specifically for `lacon doctor`. Phase 4 is the caller.
- **`validate::validate_file`** (`validate/mod.rs:45`) — content-dispatching rule/config validator; doctor reuses it per-config-layer.
- **`RuleLoader::load_all` / `resolve` + `ResolvedRule`** (`loader.rs`) — `load_all` is doctor's rule sweep; `resolve(rule_id)` + the public pipeline fields are how `explain` re-derives output.
- **`Pipeline::run_with_post_process`** (`pipeline/mod.rs:127-138`) — pub, subprocess-free; the byte-replay engine for `explain`.
- **Phase 3 hook fingerprint walk** (`init.rs:143-145`, `bash_lacon_commands` at `:318-329`) — doctor's hook-install detection mirrors this.
- **`insta`** is already a workspace dev-dependency — usable for snapshot-testing `stats`/`doctor`/`explain` text output (it bundles `similar` internally, the escape-hatch diff crate for D-06).

### Established Patterns

- **All SQL behind `lacon-core`** — Phase 2 D-01; the write path never inlines SQL at the CLI. Phase 4's read path follows (D-01).
- **`thiserror` inside crates, `anyhow` at the CLI boundary** — Phase 1 D-03.
- **No async runtime; `rusqlite` sync** — Phase 1/2.
- **Lazy / non-write on query paths** — Phase 2 D-04; query commands open read-only (D-02).
- **Hand-rolled over heavy deps** — Phase 3 splitter/quote/JSON-walk precedent; `explain` diff is hand-rolled (D-06).

### Integration Points

- **From Phase 2:** the four views + `raw_outputs` table + `health_check` are the data/entry surface Phase 4 reads. Phase 4 adds the *read* methods Phase 2 deliberately didn't.
- **From Phase 3:** the `lacon-claude-hook` settings.json fingerprint is what doctor's hook check looks for; `lacon init` and `lacon doctor` must agree on the marker.
- **For Phase 6 (acceptance):** REQ-acceptance-explain-reproducibility gates that `explain`'s replay byte-matches the original emitted output — D-04/D-05's exit-code-branch fidelity is what makes this pass.

### Performance contract

`stats` / `explain` / `doctor` are human-invoked interactive commands, **not** on the hook hot path (ADR-0013). The ≤10ms cold-start budget does not gate them — they may open the DB and (for `explain`) run a full pipeline replay. Keep them responsive but correctness/clarity wins over micro-optimization here.
</code_context>

<specifics>
## Specific Ideas

No specific user references — assumptions confirmed as-is on first pass. Approaches above derive from locked ADRs/specs (`tracking-data-model.md`, `config-schema.md`, `filter-rule-schema.md`, ADRs 0009/0010/0011/0013) plus the patterns Phases 1–3 established (all-SQL-in-core, hand-rolled-over-deps, lazy/non-write query paths).
</specifics>

<deferred>
## Deferred Ideas

- **Absolute ISO-date `--since`** — backlog; relative-only in v1 to avoid a date-crate dep (D-10).
- **`similar`/Myers aligned line-diff for `explain`** — escape hatch only (D-06); hand-rolled side-by-side ships in v1.
- **Trend graphs, session-aware rollups, `$/session` cost estimation** — explicit v2 backlog (tracking UI/analytics).
- **`stats --serve` web UI** — v2 backlog; would also break the six-command cap.
- **`lacon purge`** — v2 backlog; manual cleanup (`rm history.db`) is the v1 pattern.
- **`LACON_TOOL_USE_ID` correlation column for `explain`** — Phase 3 D-26 trailing discretion item; adopt only if it strengthens `explain`'s lookup (otherwise `id` = `invocations.id`).

### Reviewed Todos (not folded)

None reviewed — `gsd-sdk query todo.match-phase 4` returned 0 matches.
</deferred>
