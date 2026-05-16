---
phase: 02
slug: local-tracking
status: secured
verified: 2026-05-16
threats_total: 28
threats_closed: 28
threats_open: 0
asvs_level: 2
---

# Phase 02 — Local Tracking — Security

> Per-phase security contract: threat register, accepted risks, and audit trail.
> Verification target: every threat declared in the six PLAN.md `<threat_model>`
> blocks has its mitigation either present in code (mitigate), documented as an
> accepted risk (accept), or transferred to an upstream contract (transfer).

---

## Trust Boundaries (aggregated across Plans 01–06)

| Boundary | Description | Data Crossing |
|----------|-------------|---------------|
| Phase 1 `InvocationMeta` consumers ↔ Phase 2 additive extensions | New fields are additive only; compile gate enforces. | Struct layout |
| Workspace deps ↔ `Cargo.lock` | `rusqlite[bundled]` pulls `libsqlite3-sys` (C FFI). | Supply chain |
| `0001_initial.sql` ↔ `docs/specs/tracking-data-model.md` | Spec is the source of truth. Drift = data-model corruption. | DDL transcription |
| `migrate()` ↔ live user `history.db` | A bad migration could corrupt accumulated history. | Schema state |
| Privacy warning text ↔ user-facing trust signal | Mis-wording or suppression breaks the privacy contract. | Stderr text |
| Marker file ↔ filesystem race state | Two parallel `lacon run` invocations may race to create the marker. | `.store_raw_outputs_acked` |
| User home dir ↔ tracker DB file | DB stores `command_raw`, `project_path`, and (opt-in) raw stdout/stderr. | DB file at `~/.local/share/lacon/history.db` |
| In-process connection ↔ disk WAL files | `history.db-wal`/`-shm` siblings inherit parent dir 0700. | WAL files |
| `now_ms` parameter ↔ trustworthy time source | Production uses `SystemTime`; tests inject fixed values. | i64 ms timestamp |
| `InvocationMeta` assembly ↔ env vars | `LACON_ASSISTANT`/`LACON_SESSION_ID` flow into invocations rows. | Untrusted strings |
| `Tracker::record` ↔ `raw_outputs` BLOB | When opt-in, full subprocess stdout/stderr persist as BLOBs. | Bytes |
| CLI ↔ tracker errors | Best-effort (D-12) — errors never alter exit code. | Stderr log lines |
| `load_layered` ↔ project + user config files | CLI reads BOTH configs on every `run.rs` invocation. | YAML config |
| Test-time `XDG_DATA_HOME` ↔ user home | Tests redirect XDG to tempdirs to avoid polluting real DB. | env var |
| Cold-start budget ↔ tracker init cost | Phase 6 ceiling 10ms; Phase 2 ≤2.5ms — gated by criterion bench. | wall-time µs |
| Lazy-open invariant ↔ future commands | `--version`/`validate`/`doctor` MUST NOT touch the DB. | Source-grep contract |

---

## Threat Register (verification table)

| Threat ID | Category | Component | Disposition | Mitigation Plan (declared) | Evidence (file:line) | Status |
|-----------|----------|-----------|-------------|----------------------------|----------------------|--------|
| T-02-01 | T | `rusqlite[bundled]` supply chain | accept | Pin `0.39` exactly + Cargo.lock checked in. | Accepted Risks Log row R-01 below | closed |
| T-02-02 | I | `InvocationMeta` extra fields | accept | All new fields scalar/Option; no PII unless caller populates `command_raw` with secrets — Phase 1 contract unchanged. | Accepted Risks Log row R-02 | closed |
| T-02-03 | E | `TrackingError #[from] rusqlite::Error` | mitigate | `?`-conversion intentional inside `tracking/`; CLI boundary uses `eprintln!` + swallow per D-12. | `crates/lacon-core/src/error.rs:143-146` (`#[from] source: rusqlite::Error`); CLI swallow at `crates/lacon-cli/src/commands/run.rs:342-345`, `:369-378` | closed |
| T-02-04 | T | DDL drift from spec | mitigate | Grep-locked byte-exact strings: `HAVING COUNT(*) > 5`, `ON DELETE CASCADE`, `ON DELETE SET NULL`. | `crates/lacon-core/src/tracking/migrations/0001_initial.sql:89` (HAVING), `:24` (SET NULL), `:44` (CASCADE); 4 `CREATE VIEW` at `:59,69,81,93` | closed |
| T-02-05 | D | `execute_batch` on bad SQL hangs migration | accept | `BEGIN IMMEDIATE` rolls back; bad SQL fails fast; v1 has one migration. | Accepted Risks Log row R-03; transaction at `crates/lacon-core/src/tracking/migrations.rs:45` | closed |
| T-02-06 | I | `suspected_regressions` FK leakage | accept | Local-only; no network. FK CASCADE is the v1 retention contract. | Accepted Risks Log row R-04; CASCADE at `crates/lacon-core/src/tracking/migrations/0001_initial.sql:44` | closed |
| T-02-07 | T | FK pragma silent failure | mitigate | `fk_silent_no_op_without_pragma` test locks it; pragma set explicitly in code. | Test at `crates/lacon-core/tests/tracking_schema.rs` (`fn fk_silent_no_op_without_pragma`); pragma set at `crates/lacon-core/src/tracking/mod.rs:138` (`SQLITE_DBCONFIG_ENABLE_FKEY`) | closed |
| T-02-08 | I | Marker file leakage | accept | Marker is zero-byte; parent dir is 0700; existence reveals only what user-visible config already exposes. | Accepted Risks Log row R-05; zero-byte create via `OpenOptions::write(true).create_new(true).mode(0o600)` at `crates/lacon-core/src/tracking/privacy.rs:90-97` | closed |
| T-02-09 | T | Warning text drift | mitigate | `format_warning_byte_exact_template` test asserts char-by-char output. | `crates/lacon-core/src/tracking/privacy.rs:127-140` (`fn format_warning_byte_exact_template`) | closed |
| T-02-10 | R | Suppressed warning when stderr write fails | accept | Best-effort by design (D-12); marker created first so the notice is "shown". | Accepted Risks Log row R-06; ignored stderr write at `crates/lacon-core/src/tracking/privacy.rs:76` (`let _ = std::io::stderr().write_all(...)`) | closed |
| T-02-11 | T | TOCTOU race on marker creation | mitigate | `OpenOptions::create_new(true)` is atomic; `concurrent_calls_at_most_one_creates` test verifies. | Code at `crates/lacon-core/src/tracking/privacy.rs:94` (`.create_new(true)`); test at `crates/lacon-core/tests/tracking_privacy.rs` (`fn concurrent_calls_at_most_one_creates`) | closed |
| T-02-12 | I | DB readable by other users | mitigate | `ensure_data_dir` enforces 0700 on every open (idempotent); tests lock the contract; WAL/shm inherit. | `crates/lacon-core/src/tracking/mod.rs:166-189` (`ensure_data_dir` Unix branch, `set_mode(0o700)` at `:183`); tests `open_creates_parent_dir_with_0700` and `open_fixes_pre_existing_0755_to_0700` in `crates/lacon-core/tests/tracking_tracker.rs` | closed |
| T-02-13 | T | FK silent no-op | mitigate | `apply_connection_pragmas` sets `SQLITE_DBCONFIG_ENABLE_FKEY`; `open_fk_pragma_is_per_connection` verifies. | `crates/lacon-core/src/tracking/mod.rs:138`; test `open_fk_pragma_is_per_connection` in `crates/lacon-core/tests/tracking_tracker.rs` | closed |
| T-02-14 | D | `busy_timeout=5000ms` masks contention | mitigate | D-11 sets explicit 200ms; `open_busy_timeout_is_200ms` verifies. | `crates/lacon-core/src/tracking/mod.rs:134` (`busy_timeout(Duration::from_millis(200))`); test `open_busy_timeout_is_200ms` in `crates/lacon-core/tests/tracking_tracker.rs` | closed |
| T-02-15 | T | Corrupted `last_pruned_ts` blocks prune forever | mitigate | `prune_with_corrupted_last_pruned_ts_treats_as_zero` test confirms parse failure → 0 → prune fires. | `crates/lacon-core/src/tracking/prune.rs:52-60` (`.ok().and_then(parse).unwrap_or(0)`); test at `crates/lacon-core/tests/tracking_prune.rs:210` (`fn prune_with_corrupted_last_pruned_ts_treats_as_zero`) | closed |
| T-02-16 | D | Cold-start blowup from open+migrate+prune | mitigate | Prune throttled to once per 24h via `lacon_meta.last_pruned_ts`; Phase 6 measures end-to-end with bench gate. | Throttle gate at `crates/lacon-core/src/tracking/prune.rs:98-100` (`PRUNE_THROTTLE_MS = 86_400_000`); bench at `crates/lacon-core/benches/tracker_open.rs` (BUDGET 3700µs, panic-on-exceed) | closed |
| T-02-17 | I | `LACON_SESSION_ID` leakage to DB | accept | DB is 0700; session_id is metadata user explicitly provides via adapter; no exfil. | Accepted Risks Log row R-07; env-var read at `crates/lacon-cli/src/commands/run.rs:272` | closed |
| T-02-18 | I | `raw_outputs` BLOBs contain secrets | mitigate | Off by default (`Config::default().store_raw_outputs = false`); opt-in requires explicit project config flip; marker + warning trigger awareness. | Default at `crates/lacon-core/src/config/mod.rs` (`Config::default`); CLI gate at `crates/lacon-cli/src/commands/run.rs:338` (`cfg.store_raw_outputs` passed to `Tracker::open`); warning at `crates/lacon-core/src/tracking/record.rs:62-71`; CLI passes `None` for raw bytes at `crates/lacon-cli/src/commands/run.rs:363` (no bytes captured in v1 even when opt-in) | closed |
| T-02-19 | T | Tracker write failure changes wrapper exit | mitigate | `record_invocation` never `?`-propagates; `match e { eprintln!; return; }`; Plan 06 injects tracker failure and asserts exit unchanged. | `crates/lacon-cli/src/commands/run.rs:253-379` (no `?` in `record_invocation`; all error arms `eprintln!` + `return`); test `best_effort_unwritable_data_dir_preserves_exit_zero` at `crates/lacon-cli/tests/tracking_best_effort.rs:20` and `best_effort_subprocess_exit_code_propagates` at `:35` | closed |
| T-02-20 | E | Untrusted argv from `PATH` | accept | Phase 1 T-05-01 mitigation already applies — `Command::new(&argv[0])` does not re-shell-interpret; record() binds argv as data only via rusqlite `params!`. | Accepted Risks Log row R-08; spawn at `crates/lacon-cli/src/commands/run.rs:203` and `crates/lacon-core/src/runtime/mod.rs:189,403`; rusqlite parameterized binding at `crates/lacon-core/src/tracking/record.rs:100-104,138-158` | closed |
| T-02-21 | R | Best-effort tracker hides write failures | accept | Stderr log with `lacon: tracker` prefix is the audit trail; Phase 4 `lacon doctor` calls `health_check`. | Accepted Risks Log row R-09; all six `eprintln!` prefixed `lacon: tracker` in `crates/lacon-cli/src/commands/run.rs:265,330,343,372,375` | closed |
| T-02-22 | T | Future commands accidentally open DB on read paths | mitigate | Grep-based source invariants in `tracking_coldstart.rs` lock the contract: `validate.rs` and `doctor.rs` MUST NOT contain `Tracker::open`. | Tests `validate_rs_does_not_reference_tracker` (`tracking_coldstart.rs:104`) and `doctor_rs_does_not_reference_tracker` (`tracking_coldstart.rs:119`); runtime invariants `version_does_not_open_db` (`:33`), `validate_does_not_open_db` (`:56`), `doctor_does_not_open_db` (`:85`) | closed |
| T-02-23 | D | Cold-start regression > 10ms ceiling | mitigate | Criterion bench panics (cargo bench exits non-zero) if mean > 3700µs. Real benchmark gate, not documentation. | `crates/lacon-core/benches/tracker_open.rs` — `BUDGET_MICROS: u128 = 3_700` (line 21), `assert!(mean_micros < BUDGET_MICROS, …)` (lines 80-82) | closed |
| T-02-24 | T | Tracker failure changes wrapper exit code | mitigate | Best-effort tests assert exit code preservation. | `crates/lacon-cli/tests/tracking_best_effort.rs:20` (`best_effort_unwritable_data_dir_preserves_exit_zero`), `:35` (`best_effort_subprocess_exit_code_propagates`) | closed |
| T-02-25 | I | Test pollution: real user history.db corrupted | mitigate | Every test uses `tempdir()` + `.env("XDG_DATA_HOME", ...)`; per-iteration tempdir in the bench. | All `crates/lacon-cli/tests/tracking_*.rs` use `tempdir()` + `.env("XDG_DATA_HOME", ...)`; bench creates fresh tempdir per iteration in `crates/lacon-core/benches/tracker_open.rs:34` (`bench_function` body) | closed |
| T-02-26 | I | `pub conn` exposes `Connection` across crate boundary | accept | `pub conn` required by external integration tests; documented as test-affordance escape hatch. | Accepted Risks Log row R-10; `pub conn: Connection` at `crates/lacon-core/src/tracking/mod.rs:54` with the field-level docstring at `:48-52` explaining the rationale | closed |
| T-02-27 | I | `load_layered` failures silently fall back to defaults on the run path | accept | Validation errors are surfaced earlier by `lacon validate`; run path can't usefully re-emit without polluting tool output. | Accepted Risks Log row R-11; silent fallback at `crates/lacon-cli/src/commands/run.rs:299-303` (`unwrap_or_else(|_| Config::default())`) | closed |
| T-02-28 | T | Bench gate false negatives on slow CI hardware | accept | 3700µs ceiling has 2.5ms headroom over Phase 1's 1154µs baseline; raise single constant if CI hardware regresses. | Accepted Risks Log row R-12; single tunable constant `BUDGET_MICROS` at `crates/lacon-core/benches/tracker_open.rs:21` | closed |

*Disposition legend:* mitigate (implementation required) · accept (documented risk) · transfer (third-party).

---

## Accepted Risks Log

| Risk ID | Threat Ref | Rationale | Accepted By | Date |
|---------|------------|-----------|-------------|------|
| R-01 | T-02-01 | `rusqlite[bundled]` is the most-audited Rust SQLite binding; pinned to `0.39` exactly via workspace Cargo.lock checked in. Advisory monitoring deferred to Phase 6 cut. | Phase 2 maintainer | 2026-05-16 |
| R-02 | T-02-02 | New `InvocationMeta` fields are scalars and `Option`s. No PII enters unless the caller writes secrets into `command_raw` — that's the Phase 1 contract; nothing in Phase 2 changes it. | Phase 2 maintainer | 2026-05-16 |
| R-03 | T-02-05 | v1 has a single migration `M0001_INITIAL`; bad SQL fails immediately at `execute_batch` and `BEGIN IMMEDIATE` rolls back. The pathological "hang" mode requires SQLite-level pathology, not application logic. | Phase 2 maintainer | 2026-05-16 |
| R-04 | T-02-06 | All data is local; no network surface. FK `ON DELETE CASCADE` from `suspected_regressions` is the v1 retention contract per spec — by design, dependent rows die with their parent. | Phase 2 maintainer | 2026-05-16 |
| R-05 | T-02-08 | Marker is zero-byte (`mode 0o600` on Unix); parent dir is `0700`. Its mere existence reveals only that `store_raw_outputs` has been enabled — already user-visible from the config file. | Phase 2 maintainer | 2026-05-16 |
| R-06 | T-02-10 | If the stderr write fails (closed TTY etc), the marker still exists, so the warning will not repeat. Best-effort posture (D-12) accepts this — warning the user via an unreachable stderr is outside our trust boundary. | Phase 2 maintainer | 2026-05-16 |
| R-07 | T-02-17 | DB is `0700` (per-T-02-12 mitigation). `session_id` is metadata the user explicitly provides via the Phase 3 adapter env contract; no exfiltration surface. | Phase 2 maintainer | 2026-05-16 |
| R-08 | T-02-20 | Phase 1 mitigation T-05-01 already in force — `Command::new(&argv[0])` does not re-shell-interpret. `record()` binds argv as rusqlite parameters via `params!`, never string-concatenated SQL. | Phase 2 maintainer | 2026-05-16 |
| R-09 | T-02-21 | The literal `lacon: tracker` prefix is the audit trail visible inline on stderr. Phase 4 `lacon doctor` will expose `health_check` for structured introspection — surface defined in `tracking::health::health_check`. | Phase 2 maintainer | 2026-05-16 |
| R-10 | T-02-26 | `Tracker.conn` is `pub` (not `pub(crate)`) because integration tests live in `crates/lacon-core/tests/` (external to the crate boundary) and need to read pragma state directly. Public API surface contract: `Tracker` is the entry; `conn` is documented as a test escape hatch (`crates/lacon-core/src/tracking/mod.rs:48-52`). | Phase 2 maintainer | 2026-05-16 |
| R-11 | T-02-27 | Validation errors are surfaced by `lacon validate`. Re-emitting them on every `lacon run` would pollute the assistant's tool output (D-02 — filtered bytes are the model-visible result). Silent default fallback preserves the wrapper's stdout contract. | Phase 2 maintainer | 2026-05-16 |
| R-12 | T-02-28 | The 3700µs ceiling has 2.5ms headroom over Phase 1's measured 1154µs `--version` baseline; CI hardware is consistent enough at the 5ms class. If a CI regression appears, raise the single `BUDGET_MICROS` constant. | Phase 2 maintainer | 2026-05-16 |

---

## Unregistered Flags

No `## Threat Flags` sections were present in any of the six SUMMARY.md files
(02-01 through 02-06). No unregistered attack surface to log.

---

## Security Audit Trail

| Audit Date | Threats Total | Closed | Open | Run By |
|------------|---------------|--------|------|--------|
| 2026-05-16 | 28 | 28 | 0 | gsd-security-auditor |

### Verification Method Summary
- **`mitigate` threats (16):** Grepped for the mitigation pattern in the files
  cited by each plan's mitigation column. Every mitigation grep returned at
  least one match at the expected location, and the named "lock-in" tests
  (`open_creates_parent_dir_with_0700`, `open_fk_pragma_is_per_connection`,
  `open_busy_timeout_is_200ms`, `prune_with_corrupted_last_pruned_ts_treats_as_zero`,
  `fk_silent_no_op_without_pragma`, `format_warning_byte_exact_template`,
  `concurrent_calls_at_most_one_creates`, `validate_rs_does_not_reference_tracker`,
  `doctor_rs_does_not_reference_tracker`, `best_effort_unwritable_data_dir_preserves_exit_zero`)
  are all present in the integration test files.
- **`accept` threats (12):** All twelve have explicit entries in the Accepted
  Risks Log above (R-01 through R-12).
- **`transfer` threats:** None declared in this phase.

### Cross-cutting Privacy Contract Verification
The project-level privacy contract was independently verified by inspection:
1. **`raw_outputs` OFF by default** — `Config::default().store_raw_outputs == false`
   in `crates/lacon-core/src/config/mod.rs`; CLI passes `None` for raw bytes
   into `Tracker::record` even when the flag flips
   (`crates/lacon-cli/src/commands/run.rs:363`).
2. **0700 DB dir** — Unconditionally enforced in
   `crates/lacon-core/src/tracking/mod.rs:182-187` (`set_mode(0o700)` whenever
   the directory exists at any other mode).
3. **Best-effort write (D-12)** — `record_invocation` does not propagate
   `TrackingError` via `?`; every error path logs `lacon: tracker …` to stderr
   and `return`s (`crates/lacon-cli/src/commands/run.rs:253-379`).
4. **One-time first-time-on warning + atomic marker (D-14/D-15/D-16)** —
   `warn_once_if_needed` uses `OpenOptions::create_new(true)` with byte-stable
   template (`crates/lacon-core/src/tracking/privacy.rs:63-120`); test
   `format_warning_byte_exact_template` locks the bytes; concurrency-safe.
5. **Hermetic (no network, no LLM calls)** — `tracking/` module only imports
   `rusqlite`, `std::fs`, `std::path`, `std::time`. No outbound calls.
6. **`retention.*` user-only** — Project configs containing `retention.*` are
   rejected with `ValidationError::UserOnlyKeyInProject` at
   `crates/lacon-core/src/config/mod.rs:193-201` (verified inline by Phase 2's
   load_layered hook).

---

## Sign-Off

- [x] All threats have a disposition (mitigate / accept / transfer)
- [x] All `mitigate` threats have evidence (file:line) of the mitigation in code
- [x] All `accept` threats are documented in the Accepted Risks Log
- [x] No `transfer` threats in this phase
- [x] No unregistered flags from SUMMARY.md `## Threat Flags`
- [x] `threats_open: 0` confirmed
- [x] `status: secured` set in frontmatter

**Approval:** verified 2026-05-16 — gsd-security-auditor (28/28 closed)
