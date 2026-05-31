---
gsd_state_version: 1.0
milestone: v1.0
milestone_name: milestone
status: milestone_complete
last_updated: 2026-05-31T06:42:15.506Z
last_activity: 2026-05-31
progress:
  total_phases: 9
  completed_phases: 9
  total_plans: 42
  completed_plans: 42
  percent: 100
stopped_at: Milestone complete (Phase 09 was final phase)
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-05-06)

**Core value:** Reduce the bytes an AI coding assistant ingests from bash output by 30–70% without dropping signal — locally, with sub-10ms cold start, and a YAML rule per command.
**Current focus:** Milestone complete

## Current Position

Phase: 09
Plan: Not started
Status: Milestone complete
Last activity: 2026-05-31

Progress: [██████████] 100%

## Performance Metrics

**Velocity:**

- Total plans completed: 28
- Average duration: —
- Total execution time: 0 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| 03 | 5 | - | - |
| 04 | 4 | - | - |
| 5 | 9 | - | - |
| 6 | 3 | - | - |
| 07 | 1 | - | - |
| 08 | 3 | - | - |
| 09 | 3 | - | - |

**Recent Trend:**

- Last 5 plans: —
- Trend: —

*Updated after each plan completion.*
| Phase 01-engine-core-lacon-run-wrapper P01 | 11min | 3 tasks | 22 files |
| Phase 01-engine-core-lacon-run-wrapper P03 | 150 | 3 tasks | 23 files |
| Phase 01-engine-core-lacon-run-wrapper P04 | 9min | 2 tasks | 9 files |
| Phase 01-engine-core-lacon-run-wrapper P05 | 3min | 2 tasks | 6 files |
| Phase 01-engine-core-lacon-run-wrapper P07 | 6min | 2 tasks | 9 files |
| Phase 01-engine-core-lacon-run-wrapper P08 | 8min | 3 tasks | 6 files |
| Phase 02-local-tracking P01 | 10min | 2 tasks | 10 files |
| Phase 02-local-tracking P02 | 10min | 2 tasks tasks | 9 files files |
| Phase 02-local-tracking PP03 | 6min | 2 tasks tasks | 3 files files |
| Phase 02-local-tracking P04 | 12min | 3 tasks | 4 files |
| Phase 02-local-tracking PP05 | 12min | 3 tasks | 4 files |
| Phase 02-local-tracking P06 | 24min | 3 tasks | 8 files |
| Phase 03 P01 | 5min | 3 tasks | 6 files |
| Phase 03 P02 | 3min | 2 tasks | 3 files |
| Phase 03 P03 | 3min | 3 tasks | 6 files |
| Phase 03 P04 | 7min | 3 tasks | 5 files |
| Phase 03 P05 | 2min | 2 tasks | 3 files |
| Phase 04 P01 | 9min | 4 tasks | 4 files |
| Phase 04 P02 | 7min | 2 tasks | 2 files |
| Phase 04-cli-completion-stats-explain-doctor P03 | 4min | 3 tasks | 5 files |
| Phase 04-cli-completion-stats-explain-doctor P04 | 4min | 3 tasks | 4 files |
| Phase 07 P01 | 12min | 3 tasks | 4 files |
| Phase 08 P01 | 6min | 2 tasks | 2 files |
| Phase 08 P02 | 8min | 2 tasks | 1 files |
| Phase 08 P03 | 6min | 3 tasks | 4 files |
| Phase 09 P01 | 5min | 2 tasks | 3 files |
| Phase 09 P02 | 8min | 1 tasks | 3 files |
| Phase 09 P03 | 3min | 3 tasks | 7 files |

## Accumulated Context

### Roadmap Evolution

- Phase 7 added: Close gap: capture raw output on opt-in so lacon explain works end-to-end (from v1.0 milestone audit gaps_found)
- Phase 8 added: Redesign lacon stats output for readability (ADR 0014) — read-time presentation layer (project rollup, top-N capping, clarified columns); follows the stats tracking-hygiene fix (test XDG leak + DB purge)
- Phase 9 added: Output-fidelity safety — dedupe/collapse must never substitute or fabricate lines (preserve tabular/repeated-prefix signal), and inline `LACON_DISABLE=1` must guarantee byte-exact passthrough (from v1.0 post-ship validation feedback, 2026-05-31)

### Decisions

Full decision log lives in PROJECT.md "Key Decisions" (13 LOCKED ADRs). Recent decisions affecting current work:

- ADR-0013 (2026-05-05): Filter via `PreToolUse`-rewritten subprocess wrapper. `lacon run` is now production hot path — cold-start budget is load-bearing.
- ADR-0008 (locked): Aggregated `post_process` Starlark, not per-line. Constrains Phase 1 Starlark stage design.
- ADR-0005 (locked): Streaming-first output processing. Native primitives are line-by-line transformers; memory bounded by largest stateful primitive plus `max_bytes` cap.
- PLAN-01 (2026-05-06): `serde_saphyr::Value` does NOT exist in 0.0.26. PLAN-03 must use `TopLevelKeyProbe` with `Option<serde::de::IgnoredAny>` for D-17 content dispatch. Validated by `wave0_smoke.rs::smoke_serde_saphyr_value_dispatch`.
- PLAN-01 (2026-05-06): `starlark` 0.13 compiles under workspace MSRV 1.80 — confirmed by Wave 0 smoke test.
- PLAN-01 (2026-05-06): `signal-hook` declared in `[workspace.dependencies]` AND `lacon-core/Cargo.toml [dependencies]`; PLAN-05 inherits via `{ workspace = true }` without editing either Cargo.toml.
- [Phase ?]: ANSI OSC regex ordering bug fixed
- [Phase ?]: MaxBytes N = current overflowing line bytes only (streaming model; future lines unknown)
- [Phase ?]: Integration test fixture path: CARGO_MANIFEST_DIR + '../..' for workspace-root fixtures
- [Phase 01-engine-core-lacon-run-wrapper]: WAVE-0 FINDING confirmed: serde_saphyr::Value does NOT exist in 0.0.26 — use TopLevelKeyProbe pattern (Option<IgnoredAny> + flatten HashMap) for all YAML dispatch
- [Phase 01-engine-core-lacon-run-wrapper]: StageSpec externally-tagged enum works with serde-saphyr 0.0.26 standard derive — no manual Deserialize impl needed for unit/newtype/struct-valued YAML forms
- [Phase 01-engine-core-lacon-run-wrapper]: rust-embed: relative folder path resolves from CARGO_MANIFEST_DIR without interpolate-folder-path feature (Cargo.toml B1 freeze safe)
- [Phase 01-engine-core-lacon-run-wrapper]: PLAN-04: ctx passed as Starlark dict (SmallMap); scripts use ctx['exit_code'] syntax — Simpler v1 impl vs custom StarlarkValue; attribute-style deferred
- [Phase 01-engine-core-lacon-run-wrapper]: PLAN-04: AstModule::clone() per run() call since eval_module consumes AST — AstModule derives Clone and is Arc-backed in starlark-0.13; cheap
- [Phase 01-engine-core-lacon-run-wrapper]: PLAN-04: load() in .star files rejected at eval time not parse time in starlark-0.13 — Dialect::Standard with no loader set; hermetic by construction
- [Phase 01-engine-core-lacon-run-wrapper]: assert_cmd::cargo::cargo_bin used instead of env!(CARGO_BIN_EXE_*) for external workspace binary lookup
- [Phase 01-engine-core-lacon-run-wrapper]: D-11 resolved: best-effort line atomicity, no cross-stream order guarantee (single os_pipe FIFO)
- [Phase 01-engine-core-lacon-run-wrapper]: D-12 resolved: SIGTERM/SIGINT forwarded via nix::kill; no drain; exit 128+sig
- [Phase 01-engine-core-lacon-run-wrapper]: lacon cold-start: --version median 1154us, validate median 1259us — both well under 10ms Phase 6 budget
- [Phase 01-engine-core-lacon-run-wrapper P08]: SC4 closed — validate_rule() wires flatten_extends_with_lookup + compile_resolved; same-directory parent lookup for standalone file validation; DEFAULT_MAX_BYTES pub const as single source of truth
- [Phase ?]: [Phase 02-local-tracking PLAN-01]: rusqlite 0.39 + bundled feature wired into workspace; lacon-core inherits via workspace=true; ~13s first-cache cargo check wall, fast incremental thereafter
- [Phase ?]: [Phase 02-local-tracking PLAN-01]: D-03 InvocationMeta extension confirmed purely additive (grep -rn returned only def site); 5 fields added (assistant/session_id/project_path/command_normalized/raw_output_id)
- [Phase ?]: [Phase 02-local-tracking PLAN-01]: Tracker struct ships as pub-from-day-one skeleton (one private bool field) so 02-02..02-04 attach methods without API breakage
- [Phase ?]: [Phase 02-local-tracking PLAN-01]: D-18 normalize() is pure free fn (not method); 7 unit + 3 integration fixtures lock contract; pre-existing rustdoc warning in rules/schema.rs:72 logged in deferred-items.md (out of scope)
- [Phase ?]: [Phase 02-local-tracking PLAN-02]: M0001_INITIAL DDL byte-exact per spec; HAVING COUNT(*) > 5 + DROP VIEW IF EXISTS pattern verified by grep
- [Phase ?]: [Phase 02-local-tracking PLAN-02]: libsqlite3-sys 0.37 ships -DSQLITE_DEFAULT_FOREIGN_KEYS=1 — bundled rusqlite 0.39 has fks=ON by default; Plan 04 must still set pragma defensively
- [Phase ?]: [Phase 02-local-tracking PLAN-02]: Plan 02 owns ALL Phase 2 pub mod declarations in tracking/mod.rs (migrations/privacy/health/prune/record); Plans 03/04/05 only overwrite stub files
- [Phase ?]: [Phase 02-local-tracking PLAN-03]: privacy.rs + health.rs OVERWRITE Plan 02 stubs without touching tracking/mod.rs (wave-2 ownership rule); 10 new tests pass (4 privacy unit + 1 health unit + 5 integration); workspace 173 → 183, no regression
- [Phase ?]: [Phase 02-local-tracking PLAN-03]: OpenOptions::create_new(true) is the OS-atomic primitive — no Path::exists() pre-check (TOCTOU); concurrent_calls_at_most_one_creates smoke verifies API contract
- [Phase ?]: [Phase 02-local-tracking PLAN-03]: D-16 warning text is byte-stable; format_warning_byte_exact_template asserts the 4-line template via String concatenation; ~/.local/share/lacon/history.db stays literal even when XDG_DATA_HOME overridden
- [Phase ?]: [Phase 02-local-tracking PLAN-04]: Tracker.conn ships as `pub` (NOT `pub(crate)`) per revision Issue #1; integration tests under crates/lacon-core/tests/ are external to the crate boundary and need to read tracker.conn directly; regression-guard `! grep 'pub(crate) conn: Connection'` locks the contract
- [Phase ?]: [Phase 02-local-tracking PLAN-04]: 3-pragma contract order locked: busy_timeout=200ms (Duration::from_millis(200)) → set_db_config(SQLITE_DBCONFIG_ENABLE_FKEY, true) → pragma_update_and_check(None, journal_mode, WAL); debug_assert_eq verifies WAL accepted, not silently dropped
- [Phase ?]: [Phase 02-local-tracking PLAN-04]: Rule 1 deviation — Plan's negative-side FK test ("fresh conn defaults to OFF") incorrect for our build (libsqlite3-sys 0.37 ships -DSQLITE_DEFAULT_FOREIGN_KEYS=1); reworked to sibling-toggle proof of per-connection independence (same approach as tracking_schema.rs::fk_silent_no_op_without_pragma)
- [Phase ?]: [Phase 02-local-tracking PLAN-04]: prune_if_due uses unchecked_transaction() to operate on &Connection (not &mut), safe under single-threaded-per-process invariant; DELETE order raw_outputs → suspected_regressions → invocations minimizes ON DELETE SET NULL trigger fires
- [Phase ?]: [Phase 02-local-tracking PLAN-05]: capture-before-move pattern locks rule_id+rule_source via .clone() in run_with_rule BEFORE Runner::new moves resolved (RuleSource is Clone NOT Copy at loader.rs:50; Issue #2 fix)
- [Phase ?]: [Phase 02-local-tracking PLAN-05]: record_invocation calls config::load_layered(project, user) and reads cfg.store_raw_outputs+cfg.retention so SC2 (flip project config → marker+warning) is reachable end-to-end via the CLI (Issue #9 fix)
- [Phase ?]: [Phase 02-local-tracking PLAN-05]: end-to-end smoke 'lacon run -- echo hi' against XDG-overridden tempdir DB measures ~40ms wall in debug build with DB creation+migration+prune+INSERT; row written with assistant='claude-code' (default), exit_code=0, raw_output_id=NULL — defaults correct
- [Phase ?]: [Phase 02-local-tracking PLAN-06]: Privacy warning gate widened in Tracker::record (Rule 2 deviation) — fires on cfg.store_raw_outputs alone, not gated on raw_opt.is_some(); SC2 reachable end-to-end via CLI per Issue #9
- [Phase ?]: [Phase 02-local-tracking PLAN-06]: Criterion bench gate at Tracker::open boundary (BUDGET_MICROS=3_700) is REAL (Issue #3 Option A) — gate trips on this hardware (criterion median 25020us vs 3700us target). Dominant cost is ext4 fsync at migration COMMIT. Phase 6 follow-up: re-measure on tmpfs and split first-ever vs steady-state Tracker::open.
- [Phase ?]: [Phase 02-local-tracking PLAN-06]: D-04 lazy-open invariant locked by 5 tests in tracking_coldstart.rs — 3 runtime negative tests + 2 source-grep invariants using env!(CARGO_MANIFEST_DIR) per Issue #7
- [Phase ?]: [Phase 03 PLAN-01]: serde_json pinned 1.0.149 in [workspace.dependencies]; adapter inherits via { workspace = true }; Plan 05 lacon-cli also inherits
- [Phase ?]: [Phase 03 PLAN-01]: adapter dep set locked to lacon-core + serde + serde_json + anyhow (D-02 cold-start); grep gate forbids rusqlite/starlark/os_pipe/regex/etcetera/signal-hook/nix
- [Phase ?]: [Phase 03 PLAN-01]: HookInput omits deny_unknown_fields (CC may add fields); BashToolInput skip_serializing_if=Option::is_none so updatedInput never injects null (D-03 echo-back)
- [Phase ?]: [Phase 03 PLAN-01]: rule matcher promoted to lacon_core::rules::match_argv_via_load_all; empty-argv returns Ok(None); lacon-cli run delegates; cli_run.rs byte-for-byte unchanged
- [Phase ?]: [Phase 03 PLAN-02]: chain splitter is a single-pass 8-field DFA (process_sub_depth wired per RESEARCH:510); | never splits (D-09); trailing_op_span carries leading+op+trailing ws for byte-exact reassembly
- [Phase ?]: [Phase 03 PLAN-02]: heredoc body opaque via real delimiter-line tracking (<<DELIM/<<-DELIM/quoted); S11 fixture passes without the opaque-until-EOL fallback; <<< here-string is 3-byte opaque token
- [Phase ?]: [Phase 03 PLAN-03]: is_tui in adapter (D-15), apply_rewrite in lacon-core (D-19); is_repl conservative (python --version = TUI); quote_for_shell single-quote-wrap survives ONE shell parse (D-22), $(rm -rf /) round-trip guard green
- [Phase ?]: [Phase 03 PLAN-03]: apply_rewrite order remove->replace->add, idempotent apply(apply(x))==apply(x) (T3), argv[0] never touched (T10); add_flags literal-element semantics (T9)
- [Phase 03]: PLAN-04: run_hook composes Plan 1/2/3 — non-Bash guard + detect_bypass(!!/LACON_DISABLE exact-1) + split_chain + is_tui-before-resolve whole-chain bypass + match_argv_via_load_all + apply_rewrite + quote_for_shell + D-26 prefix (ASSISTANT/SESSION/TOOL_USE) + trailing_op_span byte-exact reassembly; all-unmatched short-circuits PassThrough
- [Phase 03]: PLAN-04: pipelined matched segments NOT wrapped (Rule 1) — lacon run has no shell hop so a re-quoted | becomes literal arg; chain::has_top_level_pipe gates byte-exact passthrough per chained-commands.md:17
- [Phase 03]: PLAN-04: hook cold-start passthrough median ~1029us / rewrite ~1146us (Linux) under 2ms/5ms soft targets; probe telemetry-not-gate, Phase 6 owns formal gate
- [Phase 03]: PLAN-04: bin/hook.rs unchanged (Plan 1 Task 3 already shipped JSON emit); ENV_LOCK Mutex serializes LACON_DISABLE unit tests, no serial_test dep
- [Phase ?]: [Phase 03 PLAN-05]: lacon init walks .claude/settings.json via serde_json::Value scrub-then-reinsert (D-12/D-28); command-string fingerprint starts_with('lacon-claude-hook'); idempotent + preserves user hooks/top-level keys; atomic write via tempfile::NamedTempFile::persist (D-13)
- [Phase ?]: [Phase 03 PLAN-05]: CLAUDE.md note via HTML-comment markers <!-- lacon:start/end --> detect-and-replace (D-14); orphan/corrupt marker => append fresh + warn (never destroy user content); non-object settings.json => refuse Ok(1); REQ-cli-init closed, Phase 3 complete
- [Phase ?]: [Phase 04 PLAN-01]: Wave-0 spike confirmed strict SQLITE_OPEN_READ_ONLY reads a WAL history.db on this build (rusqlite 0.39/libsqlite3-sys 0.37, ext4) — open_readonly uses READ_ONLY, D-02 fallback not needed
- [Phase ?]: [Phase 04 PLAN-01]: tracking::query is the read API (D-01) — 4 view readers + 4 D-09 base-table filtered re-queries (params!/?N, no value interpolation T-04-01) + fetch_invocation/fetch_raw_output for explain; lacon-cli keeps rusqlite dev-only
- [Phase 04]: [Phase 04 PLAN-02]: Runner::filter_bytes is the subprocess-free byte-replay entry point (D-04) mirroring runtime/mod.rs:342-359 exit-code branch (ADR-0010 success/on_error/raw-passthrough) so explain (Wave 2) re-derives filtered output from stored bytes without spawning; branch-fidelity tests lock all 3 cases (T-04-04). Sig: filter_bytes(&mut self, merged_bytes: &[u8], exit_code: i32, duration_ms: u64, command_raw: &str, project_path: Option<String>) -> Result<Vec<String>, RuntimeError>
- [Phase 04]: [Phase 04 PLAN-03]: lacon stats reads tracking::query views (unfiltered) and switches to D-09 base-table filtered re-queries when any of --project/--since/--rule set; --since v1 grammar is single-unit Nd/Nh/Nm -> ms cutoff (malformed -> exit 2 no panic); empty-DB checked via db_path.exists() before open_readonly -> 'no data yet' + exit 0 (D-03)
- [Phase 04]: [Phase 04 PLAN-03]: lacon explain replays stored raw bytes (stdout++stderr) through Runner::filter_bytes selecting the ADR-0010 branch from the stored exit code, renders a hand-rolled raw|filtered side-by-side (no diff crate, D-06); NULL raw_output_id errors pointing at store_raw_outputs (SC2); non-numeric id -> exit 2 never panics (T-04-07); insta NOT adopted (rusqlite stays dev-only, D-01); exit codes 0/1/2 (success / op-failure / bad-input)
- [Phase ?]: [Phase 04 PLAN-04]: lacon doctor is a fixed five-check sweep (hook install / config-per-layer / rule sweep / DB dir perms 0700 / read-only tracker health) printing one Pass/Fail/Warn line each; exit 0 iff no check hard-fails (D-07). Fresh machine (no settings.json/no history.db) reads informational [warn] not red and exits 0 (D-03); a positively broken state flips it red.
- [Phase ?]: [Phase 04 PLAN-04]: doctor DB checks use open_readonly ONLY (D-08, T-04-11) and gate on db_path.exists() before opening, so a fresh run never creates history.db (D-04 preserved); doctor.rs has zero Tracker::open refs (grep gate = 0). cli_surface hardened: purge/install/stats --serve each proven non-zero (D-13, REQ-cli-surface-cap).
- [Phase 07]: [Phase 07 PLAN-01]: raw capture field RunOutcome.raw_captured: Option<Vec<u8>> + RunOptions.capture_raw: bool (default false, derive(Default)); capture form is raw_buffer.join("\n").into_bytes() with NO trailing newline (D-05) — exact inverse of the per-line reader build so filter_bytes' split-on-\n re-split round-trips byte-identically (proven by the Task 3 byte-exact E2E)
- [Phase 07]: [Phase 07 PLAN-01]: run.rs:275 hard-coded None gap closed — capture flag set from resolved store_raw_outputs via shared load_cfg/config_paths/user_config_dir helpers (flag in run_with_rule, gate in record_invocation read the SAME value); RawOutput{stdout: captured, stderr: Vec::new()} (D-04 merged stream) passed as Some(&raw); existing double-gate in Tracker::record is the sole persist authority (D-07). Default-off path byte-for-byte unchanged (D-03/D-09), no new clippy warnings.
- [Phase 08]: [Phase 08 PLAN-01]: overall_totals/filtered_overall_totals readers added behind lacon-core::tracking::query (D-02); scalar COALESCE(SUM,0) + query_row over bypassed=0 base table, no v_overall view/migration/field rename (D-01 fence); ?N placeholder binds (T-08-02), no rule_id predicate so headline spans matched+unmatched (D-05)
- [Phase ?]: [Phase 08 PLAN-02]: stats presentation helpers added as private fns in commands/stats.rs (D-04) — humanize_bytes (decimal-SI, D-13), is_ephemeral/ephemeral_prefixes (component-wise Path::starts_with, D-08), resolve_repo_root (.git dir/worktree/submodule via bounded fs reads, no git subprocess, no canonicalize, D-09/D-10), canonical_project_key (precedence ephemeral->repo-root->literal, D-07); #[allow(dead_code)] until 08-03 wires call sites; execute signature unchanged; 8 new inline tests incl. /tmpfoo negative + 3 literal-fallback branches
- [Phase ?]: [Phase 08 PLAN-03]: lacon stats wired to ADR 0014 read-time presentation — headline FIRST (runs, canonical project count = rolled.len() not SQL distinct, raw→kept, saved abs+%, D-05); Rust-side project rollup under canonical_project_key + sort_by_key(Reverse) DESC (D-06); TOP_N=10 cap + '… M more' hint via generic print_capped, --all uncaps (D-11/D-12); render closure humanizes bytes, --bytes exact ints (D-14); relabeled headers/columns kept+'saved %' (D-15); no migration/view/field rename (D-01/D-15 fence); empty-DB→0 + bad --since→2 preserved (D-03)
- [Phase ?]: Phase 09-01: inline LACON_DISABLE=1 env-prefix bypass added to detect_bypass (inline_disable_bypass leading-assignment scan, breaks at command word per D-04, one-layer unquote + exact-1 match). Byte-exact run_bypassed backstop locked in cli_run via assert_cmd.
- [Phase ?]: Phase 09-02: collapse_repeated elision standardized to fixed [lacon: collapsed N lines] marker at both in-run + flush sites (D-07 option a — free-form summary_template no longer emitted, field retained for YAML deserialization); CR-03 guard preserved; survivors proven verbatim (D-09); dedupe unchanged.
- [Phase ?]: Phase 09-03: git-status collapse_repeated REMOVED (D-08) — tab-indented per-file lines survive verbatim; many-untracked + new tabular-signal fixtures exempt_from_reduction_check (Open Q2 / tsc precedent); tabular-signal gates on fabrication CLASS not literal table string (Open Q1); filter-rule-schema.md documents fixed [lacon: collapsed N lines] marker, free-form summary no longer emitted (D-12); tsc dedupe confirmed signal-preserving unchanged (D-10).

### Pending Todos

None yet.

### Blockers/Concerns

None blocking. Three deferred-to-prototyping open questions assigned to phases as implementation-time decisions (not v1 blockers):

- **Phase 1**: Q-deferred-signal-forwarding (SIGTERM behavior in `lacon run`); Q-deferred-merge-ordering (stdout/stderr merge guarantee).
- **Phase 3**: Q-deferred-init-idempotency (`lacon init` re-run handling).

### Quick Tasks Completed

| # | Description | Date | Commit | Directory |
|---|-------------|------|--------|-----------|
| 260522-s33 | Scrub stale phase-stub annotations from CLI help strings | 2026-05-22 | 3d8d849 | [260522-s33-scrub-cli-help-phase-stubs](./quick/260522-s33-scrub-cli-help-phase-stubs/) |
| 260522-tor | Scope-aware `lacon init` (--project/--user) with LACON.md + verified @import reference | 2026-05-22 | 4697099 | [260522-tor-init-choose-project-vs-user-local-scope-](./quick/260522-tor-init-choose-project-vs-user-local-scope-/) |
| 260522-v4a | Scope-aware `lacon doctor` — checks project + user setup (hook + LACON.md + @import) with opt-in posture | 2026-05-22 | 1baa30c | [260522-v4a-doctor-check-both-project-and-user-setup](./quick/260522-v4a-doctor-check-both-project-and-user-setup/) |

### Note on requirement count

`.planning/intel/SYNTHESIS.md` reports "26 distinct REQ-* IDs"; the actual count in `.planning/intel/requirements.md` is 36 distinct REQ-* headings. The 36 figure is authoritative for this roadmap; coverage is 36/36, no orphans. Recorded for transparency.

## Deferred Items

| Category | Item | Status | Deferred At |
|----------|------|--------|-------------|
| *(none)* | | | |

## Session Continuity

Last session: 2026-05-31T06:22:51.387Z
Stopped at: Completed 09-01-PLAN.md
Resume file: None
