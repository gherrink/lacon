# lacon

## What This Is

`lacon` is a Rust CLI that integrates with coding-assistant hook systems (Claude Code first) to filter and rewrite the bash command output that flows into the model's context window. It replaces verbose, repetitive build/test/install output with the signal an AI coding assistant actually needs — locally, without LLM calls, and without a daemon.

It exists for one user (the developer) and one consumer (the assistant). It is not a shell, not an LLM, not a remote service, and not a general-purpose log filter.

## Core Value

**Reduce the bytes an AI coding assistant ingests from bash output by 30–70% without dropping signal — locally, with sub-10ms cold start, and a YAML rule per command.**

If filtering accuracy or cold-start budget fails, the project fails. Everything else is negotiable.

## Requirements

### Validated

<!-- Shipped and confirmed valuable. -->

(None yet — design phase, no source code yet.)

### Active

v1 scope. See `.planning/REQUIREMENTS.md` for the full list with REQ-IDs grouped by category and mapped to phases.

- 8 engine requirements (streaming primitives, Starlark `post_process`, rule loading, `extends`, `on_error`, `rewrite`, bypass mechanics, `max_bytes` cap)
- 5 Claude Code adapter requirements (PreToolUse-only, bypass detection, chained-command splitting, TUI bypass, pipes passthrough)
- 5 tracking requirements (SQLite location, schema, raw-outputs default-off, privacy warning, retention defaults)
- 7 CLI surface requirements (`init`, `run`, `stats`, `explain`, `doctor`, `validate`, six-command surface cap)
- 2 bundled-rule requirements (Tier 1 ten-rule library, fixture/test format)
- 6 v1 acceptance criteria (≥50% bundled reduction, end-to-end `pnpm install` flow, <10ms cold start, `explain` reproducibility, hot-reload without restart, hermetic test coverage)
- 3 documentation requirements (README + quickstart, worked example, primitive reference)

### Out of Scope

Explicit v1 exclusions from `docs/v1-scope.md` and `docs/backlog.md`:

- **Adapters for Cursor / aider / generic shells** — Claude Code first; other adapters land on top of the assistant-agnostic core engine post-v1.
- **Per-token (vs per-byte) accounting** — tracking schema is forward-compatible via append-only migration; tokenizer choice is v2 design (deferred to backlog).
- **Per-line streaming Starlark** — aggregated `post_process` only (ADR-0008); per-line Starlark would dominate runtime at typical volumes.
- **Filtering inside pipes (`|`)** — pipes are part of a single segment; `lacon` wraps the whole pipe as a unit. Per-pipe filtering is v2.
- **Native Windows support** — WSL is fine; native Win32 deferred. macOS + Linux only in v1.
- **Public rule registry / cross-machine sync** — `lacon install gh:user/repo` is backlog; no telemetry, no remote calls.
- **Granular per-segment TUI bypass in chains** — v1 uses whole-chain bypass when any segment looks interactive; granular bypass is v2 backlog.
- **User-overridable TUI list** — hardcoded in adapter for v1; user override deferred until clear false-positive pattern emerges.
- **Automatic redaction of `raw_outputs`** — best-effort regex creates false-confidence risk; defer to backlog. v1 ships off-by-default + `0700` + opt-in stderr warning.
- **`lacon purge`** — would push CLI past the 6-command surface; manual cleanup via `rm history.db` or `sqlite3` for v1.
- **Daemon / persistent Starlark interpreter** — load-bearing property: no daemon. Reconsider only if real benchmark data justifies in v2.
- **Multi-rule merging across layers / specificity ranking** — first-match-wins is the contract (ADR-0007).

## Context

**Project status (2026-05-06):** Design phase. No source code, no `Cargo.toml`. The 13 ADRs in `docs/decisions/`, 4 specs in `docs/specs/`, and 2 PRDs (`docs/v1-scope.md`, `docs/vision.md`) form the authoritative contract. Treat them as the source of truth — proposed changes that contradict an ADR must surface that explicitly.

**Architecture (`docs/architecture.md`, updated 2026-05-05):** Adapter (per assistant) → `lacon run` wrapper → rule resolver → pipeline runner (streaming) → tracker (SQLite). The Claude Code `PreToolUse` hook does both jobs: applies the rule's `rewrite` block (flag add/remove) and, for matched commands, wraps the command as `lacon run --rule <id> -- <cmd>`. Filtering happens inside `lacon run`, which spawns the subprocess, merges stderr into stdout, and writes filtered bytes to its own stdout — that's what Claude Code captures as the tool result.

**No `PostToolUse` hook in v1.** Empirical probe on 2026-05-05 (recorded in `docs/open-questions.md` resolved log) confirmed `PostToolUse` cannot replace tool output; only `additionalContext` reaches the model, additively. ADR-0013 is the recovery design and the current load-bearing decision.

**Performance contract:** Cold start under 10ms on the hook hot path. The binary is invoked thousands of times per session. Anything that imposes startup cost (lazy_static blowups, large embedded data, eager rule compilation) needs to justify itself against this budget.

**Repo layout (planned):** `crates/lacon-core/`, `crates/lacon-cli/`, `crates/lacon-adapter-claudecode/`, `bundled-rules/`, `tests/{fixtures,integration}/`. None of this exists yet on disk.

**Three deferred-to-prototyping open questions** are tracked in `docs/open-questions.md` and assigned into the relevant phase as implementation-time decisions (not v1 blockers): (1) signal forwarding semantics in `lacon run` on SIGTERM/SIGINT; (2) `lacon init` idempotency strategy when hook block already exists; (3) stdout/stderr merge ordering guarantee (POSIX line-buffered vs strict line atomicity).

## Constraints

Constraints inherit from 4 SPECs and the cross-cutting NFRs derived from ADRs + PRDs. Full list (29 CON-* entries) is in `.planning/intel/constraints.md`.

### Filter rule schema (CON-filter-rule-*, 11 entries)

- **Rule file locations** (CON-filter-rule-file-locations): three layers — `<cwd>/.lacon/rules/*.yaml`, `~/.config/lacon/rules/*.yaml`, bundled (embedded). First-match-wins. No cross-layer merging. `extends` is the only explicit layering primitive.
- **Top-level fields** (CON-filter-rule-top-level): `id` (required, kebab-case, layer-unique), `description`, `extends`, `match`, `bypass_when`, `rewrite`, `pipeline`, `on_error`, `post_process`. `match` and `pipeline` are required unless inherited.
- **Match operators** (CON-filter-rule-match-operators): `command`, `args_prefix`, `args_contain`, `command_regex`, `any` (OR), `all` (AND).
- **Bypass conditions** (CON-filter-rule-bypass-when): `bypass_when` supports `has_flag`, `is_tty`, `env`. Match → rule skipped, raw output passes through.
- **Rewrite block** (CON-filter-rule-rewrite): `add_flags` (idempotent), `remove_flags`, `replace_flags` (map). Adapter applies before wrapping.
- **Native primitives contract** (CON-filter-rule-native-primitives): the ten primitives — `strip_ansi`, `drop_regex`, `keep_regex` (multi-stage OR'd whitelist), `replace_regex`, `dedupe { max_kept: N }`, `collapse_repeated { pattern, max_kept, summary }` with `{count}` placeholder, `keep_head { lines | bytes }`, `keep_tail { lines | bytes }` (bounded ring buffer), `keep_around_match { pattern, before, after }`, `max_bytes: N` (truncation marker `[lacon: truncated, N more bytes dropped]`, must be last stage).
- **Starlark stage** (CON-filter-rule-starlark-stage): `script: { path, function }` with `def process(ctx, lines) -> list[str]`; `ctx` exposes `.exit_code`, `.duration_ms`, `.command`, `.args`, `.project_path`. Slow vs native primitives — place near end.
- **`on_error` semantics** (CON-filter-rule-on-error): fully replaces `pipeline` (and optionally `post_process`) on non-zero exit. No merging.
- **`post_process` stage** (CON-filter-rule-post-process): Starlark function on entire post-pipeline output; equivalent to a final `script:` stage but conventionally placed in `post_process` for clarity.
- **`extends` semantics** (CON-filter-rule-extends-semantics): inherited fields when child omits them; parent `pipeline` PREPENDED; single-level, non-cyclic, flattened at load time.
- **Validation** (CON-filter-rule-validation): invalid regex, unknown primitive, circular `extends`, missing referenced Starlark file → fails to load. `lacon doctor` runs validation against every rule.

### Config schema (CON-config-*, 5 entries)

- **File locations** (CON-config-file-locations): bundled (embedded), `~/.config/lacon/config.yaml` (user), `<cwd>/.lacon/config.yaml` (project). All optional.
- **v1 keys** (CON-config-v1-keys): `retention.invocations_days` (30, USER-ONLY), `retention.raw_outputs_days` (3, USER-ONLY), `defaults.max_bytes` (32768, project-or-user), `store_raw_outputs` (false, project-or-user — project-level opt-in is the documented pattern).
- **Layer merge** (CON-config-layer-merge): per-key DEEP merge across layers (bundled → user → project). Sub-objects (`retention`, `defaults`) merge recursively, not wholesale. Project file using a user-only key fails validation pointing at the user config path.
- **Unknown keys** (CON-config-unknown-keys): unknown top-level or nested keys fail validation. No silent ignores.
- **Validation dispatch** (CON-config-validation-dispatch): `lacon validate <path>` detects file type by content (top-level `id`+`match` → rule; otherwise config). Files that fail validation are rejected at load time. `lacon` does NOT silently fall back to defaults on malformed config.

### Tracking data model (CON-tracking-*, 8 entries)

- **Database location** (CON-tracking-database-location): `~/.local/share/lacon/history.db`, directory permissions enforced at `0700` at DB initialization.
- **`invocations` schema** (CON-tracking-invocations-schema): columns `id`, `ts`, `assistant`, `session_id`, `project_path`, `command_raw`, `command_normalized`, `rule_id`, `rule_source` (`'project'|'user'|'bundled'|NULL`), `exit_code`, `duration_ms`, `raw_stdout_bytes`, `raw_stderr_bytes`, `filtered_bytes`, `bypassed`, `rewritten`, `truncated_by_max_bytes`, `raw_output_id` (FK → raw_outputs ON DELETE SET NULL). Indexes: `ts`, `command_normalized`, `rule_id`, `project_path`.
- **`raw_outputs` schema** (CON-tracking-raw-outputs-schema): `id`, `invocation_id`, `stdout` (BLOB), `stderr` (BLOB), `created_ts`. Index: `created_ts`.
- **`suspected_regressions` schema** (CON-tracking-suspected-regressions-schema): `id`, `invocation_id` (FK → invocations ON DELETE CASCADE), `reason` (e.g. `'rerun_with_verbose'`, `'explain_called_after'`), `detected_ts`. Index: `invocation_id`.
- **Required views** (CON-tracking-views): `v_unmatched_offenders`, `v_filtered_offenders`, `v_bypass_rate` (HAVING COUNT(*) > 5), `v_project_savings`.
- **Retention policy** (CON-tracking-retention-policy): `invocations` 30 days, `raw_outputs` 3 days, `suspected_regressions` 30 days (tied to invocations). Pruning runs at startup.
- **Privacy contract** (CON-tracking-privacy-contract): off by default, `0700` directory, opt-in stderr warning on first off→on transition (suppressed via marker), no automatic redaction in v1, manual cleanup only, no telemetry.
- **Migration policy** (CON-tracking-migration-policy): numbered append-only migrations applied at startup. Down migrations not supported.
- **Tokens not in v1** (CON-tracking-tokens-not-in-v1): byte-named columns are forward-compatible; token columns can be appended later.

### Chained-commands protocol (CON-chained-*, 8 entries)

- **Splitting boundaries** (CON-chained-splitting-boundaries): top-level `&&`, `||`, `;` only. NOT inside quotes, `(...)`, `$(...)`, backticks, `${...}`, heredocs. Pipes (`|`) are NOT chain operators — pipeline is a single segment.
- **Opaque constructs** (CON-chained-opaque-constructs): subshells, command substitution, process substitution, heredoc bodies, quoted strings.
- **Per-segment rule resolution** (CON-chained-rule-resolution-per-segment): each segment resolved independently (first-match-wins, project > user > bundled). Two outcomes per segment: matched (wrapped) or unmatched (passthrough).
- **Rewrite emission** (CON-chained-rewrite-emission): hook reassembles chain joining segments with original operators, preserving order/operator type.
- **Exit-code propagation** (CON-chained-exit-code-propagation): `lacon run` propagates wrapped subprocess's exit code unchanged. Shell semantics work as if `lacon run` weren't present.
- **Whole-command bypass** (CON-chained-bypass-whole-command): `!!` prefix and `LACON_DISABLE=1` bypass at WHOLE-COMMAND granularity, not per segment.
- **Whole-chain TUI bypass** (CON-chained-tui-bypass-whole-chain): `is_tui(command, args) -> bool` runs per-segment AFTER chain splitting and BEFORE rule resolution. Any match → entire input bypassed (v1 conservative).
- **TUI list v1** (CON-chained-tui-list-v1): hardcoded in adapter. Pure-TUI by basename: `vim`, `vi`, `nvim`, `nano`, `emacs`, `less`, `more`, `most`, `man`, `htop`, `top`, `btop`, `screen`, `tmux`, `ssh`, `mosh`, `ipython`, `irb`, `pry`, `redis-cli`, `crontab`, `visudo`. Conditional: `git rebase -i`, `git commit` w/o `-m`/`-F`, `git add -p/-i`, `git checkout -p`, `git stash -p`, `npm/yarn/pnpm init` w/o `-y`, REPLs (`node`, `python`, `python3`, `mysql`, `psql`, `sqlite3`) with no positional argument.
- **Test obligations** (CON-chained-test-obligations, NFR): splitter test matrix enumerated in `docs/specs/chained-commands.md` — 13 scenarios.

### Cross-cutting NFRs (CON-nfr-*, 5 entries)

- **Cold-start budget** (CON-nfr-cold-start-budget): ≤10ms on hook hot path. ADR-0013 tightens this — `lacon run` is now production hot path, invoked thousands of times per session.
- **Streaming memory** (CON-nfr-streaming-memory): bounded by largest stateful primitive (typically `keep_tail N`) plus `max_bytes` cap. Long builds must not OOM.
- **stderr merge** (CON-nfr-stderr-merge): stderr merges into stdout inside `lacon run`. Pipeline operates on single combined stream. Best-effort line atomicity, no cross-stream order guarantee. (Implementation guarantee deferred to prototyping — see Q-deferred-merge-ordering.)
- **TTY detection downstream** (CON-nfr-tty-detection-downstream): tools spawned by `lacon run` see "not a TTY" — most tools emit less noise in non-TTY mode.
- **No network, no daemon** (CON-nfr-no-network-no-daemon): SQLite single-file storage; backup is `cp history.db backup.db`. WAL mode handles concurrent writes safely.
- **Platform support** (CON-nfr-platform-support): macOS + Linux (and WSL by extension). Native Windows deferred.

## Key Decisions

All 13 ADRs are LOCKED (`status: Accepted`) and form an internally consistent additively-related set. Source: `.planning/intel/decisions.md`. Cross-reference graph is acyclic (max DFS depth 2).

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| **ADR-0001** — Use Claude Code hooks | Reject PATH shims and shell function injection; hook-native integration is the legitimate API surface. NARROWED by ADR-0013 to `PreToolUse` only for v1; `PostToolUse` reserved for v1.5 unmatched-command annotation. | LOCKED |
| **ADR-0002** — Rust as primary language | Sub-millisecond cold start, best-in-class regex, mature crates (`regex`, `clap`, `rusqlite`, `starlark-rust`), cross-compilation. | LOCKED |
| **ADR-0003** — Starlark for escape-hatch scripting | Hermetic by design — no I/O, no clock, no network. Embedded via `starlark-rust`. | LOCKED |
| **ADR-0004** — Project > User > Bundled config precedence | First-match-wins; rules from different layers do NOT merge. Layering only via explicit `extends`. | LOCKED |
| **ADR-0005** — Streaming-first output processing | Native primitives are line-by-line streaming transformers; memory bounded by largest stateful primitive plus `max_bytes` cap. ADR-0008 is the explicit aggregated exception. | LOCKED |
| **ADR-0006** — Hybrid command rewriting and output filtering | Rules support both pre-execution `rewrite` and post-execution `pipeline` as first-class mechanisms. Cheapest-tactic-first per command. | LOCKED |
| **ADR-0007** — First-match-wins rule resolution | Resolver walks layers in priority order, returns the first matching rule. Within a layer, lexicographic order of filenames. No specificity ranking. | LOCKED |
| **ADR-0008** — Aggregated `post_process` Starlark, not per-line | Native pipeline does bulk reduction; Starlark gets the small remaining payload. Per-line streaming Starlark is backlogged. | LOCKED |
| **ADR-0009** — Separated `raw_outputs` table | Different retention per table (30 days invocations, 3 days raw outputs); raw storage off by default. | LOCKED |
| **ADR-0010** — `on_error` replaces the pipeline | Fully replaces `pipeline` and (optionally) `post_process` on non-zero exit. No merging. | LOCKED |
| **ADR-0011** — SQLite for local tracking | `~/.local/share/lacon/history.db` in WAL mode. Append-only migrations on startup. No daemon, no network. | LOCKED |
| **ADR-0012** — Append-only inheritance via `extends` | Inherits scalar fields child omits; PREPENDS parent's `pipeline`. No remove/reorder/insert ops. | LOCKED |
| **ADR-0013** — Filter via PreToolUse-rewritten subprocess wrapper | Empirical probe 2026-05-05 confirmed `PostToolUse` cannot replace tool output. `lacon run --rule <id> -- <cmd>` spawns subprocess, merges stderr into stdout, runs pipeline (or `on_error`), writes filtered bytes to its own stdout, exits with subprocess's exit code. ADDITIVE — narrows ADR-0001 only on execution location; no prior ADR is amended. | LOCKED |

---
*Last updated: 2026-05-06 after initial planning ingest from `.planning/intel/` (24 docs synthesized: 13 ADRs, 4 SPECs, 2 PRDs, 5 DOCs; 0 conflicts blocking, 5 INFO).*
