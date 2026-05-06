# Constraints (synthesized from SPEC-class docs)

Four SPEC-class documents are in the ingest set. Each forms a piece of the user-facing contract — changes are breaking. None contradict any LOCKED ADR.

- `docs/specs/filter-rule-schema.md` — YAML rule format
- `docs/specs/config-schema.md` — YAML configuration schema (engine + tracking behaviour)
- `docs/specs/tracking-data-model.md` — SQLite schema, indexes, views, retention, privacy
- `docs/specs/chained-commands.md` — `&&` / `||` / `;` chain handling

Constraints are grouped by spec.

---

## Filter rule schema constraints

### CON-filter-rule-file-locations

- **source:** docs/specs/filter-rule-schema.md
- **type:** schema
- **content:** Rules live in three layers in priority order: (1) `<cwd>/.lacon/rules/*.yaml`, (2) `~/.config/lacon/rules/*.yaml`, (3) bundled (embedded in binary). First-match-wins; no merging across layers. Use `extends` to layer onto bundled rules.

### CON-filter-rule-top-level

- **source:** docs/specs/filter-rule-schema.md
- **type:** schema
- **content:** Top-level rule fields: `id` (required string, kebab-case, unique within layer), `description` (optional), `extends` (optional, string referencing another rule by ID with optional `bundled/`/`user/`/`project/` prefix), `match` (required unless inherited), `bypass_when` (optional), `rewrite` (optional), `pipeline` (required unless inherited), `on_error` (optional), `post_process` (optional).

### CON-filter-rule-match-operators

- **source:** docs/specs/filter-rule-schema.md
- **type:** schema
- **content:** Match operators: `command` (exact match against argv[0] basename), `args_prefix` (argv[1..N] starts with these tokens), `args_contain` (argv[1..] includes tokens), `command_regex` (regex against full normalized command line), `any` (OR), `all` (AND).

### CON-filter-rule-bypass-when

- **source:** docs/specs/filter-rule-schema.md
- **type:** schema
- **content:** `bypass_when` block (rule-level). Sub-conditions: `has_flag`, `is_tty`, `env`. If matched, rule is skipped entirely (raw output passes through).

### CON-filter-rule-rewrite

- **source:** docs/specs/filter-rule-schema.md
- **type:** schema
- **content:** `rewrite` block has `add_flags` (idempotent — won't add a flag already present), `remove_flags`, `replace_flags` (map). Only applied if the adapter supports pre-execution modification.

### CON-filter-rule-native-primitives

- **source:** docs/specs/filter-rule-schema.md
- **type:** schema
- **content:** Native primitive contract:
  - `strip_ansi` — no args; removes ANSI color and control sequences.
  - `drop_regex: <pattern>` — drops lines matching regex.
  - `keep_regex: <pattern>` — whitelist mode; multiple `keep_regex` stages OR'd; if any present, only matching lines kept.
  - `replace_regex: { pattern, replacement }` — regex substitution.
  - `dedupe: { max_kept: N }` (default 1) — collapses consecutive duplicate lines.
  - `collapse_repeated: { pattern, max_kept, summary }` — collapses consecutive matching lines into max_kept examples + summary; `{count}` placeholder in summary.
  - `keep_head: { lines: N }` or `keep_head: { bytes: N }` — keeps first N.
  - `keep_tail: { lines: N }` or `keep_tail: { bytes: N }` — keeps last N (bounded ring buffer).
  - `keep_around_match: { pattern, before, after }` — grep -B/-A semantics.
  - `max_bytes: N` — hard cap, truncation marker `[lacon: truncated, N more bytes dropped]`. Should always be the last stage.

### CON-filter-rule-starlark-stage

- **source:** docs/specs/filter-rule-schema.md
- **type:** schema
- **content:** `script: { path, function }` runs Starlark on aggregated output. Function signature: `def process(ctx, lines) -> list[str]` where `ctx` exposes `.exit_code`, `.duration_ms`, `.command`, `.args`, `.project_path`. Slow relative to native primitives — place near end of pipeline.

### CON-filter-rule-on-error

- **source:** docs/specs/filter-rule-schema.md
- **type:** schema
- **content:** `on_error` is a separate block that fully replaces `pipeline` (and optionally `post_process`) when the command exits non-zero. Does NOT merge.

### CON-filter-rule-post-process

- **source:** docs/specs/filter-rule-schema.md
- **type:** schema
- **content:** `post_process` is a Starlark function run once on entire post-pipeline output. Equivalent to a final `script:` stage; conventionally placed in `post_process` for clarity.

### CON-filter-rule-extends-semantics

- **source:** docs/specs/filter-rule-schema.md
- **type:** schema
- **content:** `extends` semantics: (1) fields not defined on this rule are inherited from parent (`description`, `match`, `bypass_when`, `rewrite`, `on_error`, `post_process`); (2) parent's `pipeline` stages are PREPENDED to this rule's pipeline; (3) inheritance is single-level and non-cyclic — chains flattened at load time.

### CON-filter-rule-validation

- **source:** docs/specs/filter-rule-schema.md
- **type:** schema
- **content:** `lacon validate <path>` parses, type-checks, and dry-runs a rule against optional fixture files. A rule with invalid regex, unknown primitive, circular `extends`, or missing referenced Starlark file fails to load. `lacon doctor` runs validation against every rule on the system.

---

## Config schema constraints

### CON-config-file-locations

- **source:** docs/specs/config-schema.md
- **type:** schema
- **content:** Three config layers: bundled (embedded), user (`~/.config/lacon/config.yaml`), project (`<cwd>/.lacon/config.yaml`). All three optional; missing files fall back to lower layer. Bundled defaults always available.

### CON-config-v1-keys

- **source:** docs/specs/config-schema.md
- **type:** schema
- **content:** v1 config keys (all optional):
  - `retention.invocations_days` (integer, default 30) — also governs `suspected_regressions`. **USER-ONLY.**
  - `retention.raw_outputs_days` (integer, default 3). **USER-ONLY.**
  - `defaults.max_bytes` (integer, default 32768) — fallback final-stage cap. **PROJECT-OR-USER.**
  - `store_raw_outputs` (boolean, default false) — opt-in storage for `raw_outputs` table. **PROJECT-OR-USER** (project-level opt-in is the documented pattern).

### CON-config-layer-merge

- **source:** docs/specs/config-schema.md
- **type:** schema
- **content:** Per-key deep merge across config layers (bundled → user → project). Each layer overrides scalar keys present in lower layers; sub-objects (`retention`, `defaults`) merge recursively rather than wholesale. Per-key scope rules: `retention.*` user-only; `defaults.max_bytes` and `store_raw_outputs` project-or-user. A project config including a user-only key fails validation with an error pointing to `~/.config/lacon/config.yaml`.

### CON-config-unknown-keys

- **source:** docs/specs/config-schema.md
- **type:** schema
- **content:** Unknown top-level or nested keys fail validation. Same posture as filter-rule-schema. No silent ignores.

### CON-config-validation-dispatch

- **source:** docs/specs/config-schema.md
- **type:** schema
- **content:** `lacon validate <path>` accepts both rule files and config files. Dispatcher detects file type by content: top-level `id` + `match` → rule file; otherwise config file. Files that fail validation are rejected at load time; the previously-validated layer is used in its place. `lacon` does NOT silently fall back to defaults when a config file is malformed.

---

## Tracking data model constraints

### CON-tracking-database-location

- **source:** docs/specs/tracking-data-model.md
- **type:** schema
- **content:** Database lives at `~/.local/share/lacon/history.db`. Directory permissions enforced at `0700` at DB initialization.

### CON-tracking-invocations-schema

- **source:** docs/specs/tracking-data-model.md
- **type:** schema
- **content:** `invocations` table columns: `id`, `ts`, `assistant`, `session_id`, `project_path`, `command_raw`, `command_normalized`, `rule_id`, `rule_source` (`'project'|'user'|'bundled'|NULL`), `exit_code`, `duration_ms`, `raw_stdout_bytes`, `raw_stderr_bytes`, `filtered_bytes`, `bypassed`, `rewritten`, `truncated_by_max_bytes`, `raw_output_id` (FK → raw_outputs ON DELETE SET NULL). Indexes on `ts`, `command_normalized`, `rule_id`, `project_path`.

### CON-tracking-raw-outputs-schema

- **source:** docs/specs/tracking-data-model.md
- **type:** schema
- **content:** `raw_outputs` table columns: `id`, `invocation_id`, `stdout` (BLOB), `stderr` (BLOB), `created_ts`. Index on `created_ts`.

### CON-tracking-suspected-regressions-schema

- **source:** docs/specs/tracking-data-model.md
- **type:** schema
- **content:** `suspected_regressions` table: `id`, `invocation_id` (FK → invocations ON DELETE CASCADE), `reason` (e.g. `'rerun_with_verbose'`, `'explain_called_after'`), `detected_ts`. Index on `invocation_id`.

### CON-tracking-views

- **source:** docs/specs/tracking-data-model.md
- **type:** schema
- **content:** Four required views ship in v1:
  - `v_unmatched_offenders` — top offenders by raw bytes, no rule matched (candidates for new rules).
  - `v_filtered_offenders` — top offenders by filtered bytes, rule matched (existing rules leaving tokens on the table).
  - `v_bypass_rate` — `HAVING COUNT(*) > 5` — rules the agent keeps overriding (smell signal).
  - `v_project_savings` — per-project savings summary.

### CON-tracking-retention-policy

- **source:** docs/specs/tracking-data-model.md, docs/specs/config-schema.md
- **type:** schema
- **content:** Retention defaults: `invocations` 30 days, `raw_outputs` 3 days, `suspected_regressions` 30 days (tied to `invocations`). Pruning runs on `lacon` startup as `DELETE FROM ... WHERE created_ts < ?`.

### CON-tracking-privacy-contract

- **source:** docs/specs/tracking-data-model.md
- **type:** schema
- **content:** v1 privacy contract for `raw_outputs`:
  - **Off by default.** Project-level opt-in via `store_raw_outputs: true`.
  - **Directory permissions: `0700`.** Enforced at init.
  - **Opt-in warning:** first off → on transition prints a one-time stderr notice; suppressed via marker in project config dir.
  - **No automatic redaction.** Bytes stored as captured. Pattern-based secret redaction is backlog-only — false-confidence risk excluded from v1.
  - **Manual cleanup.** v1 ships no `lacon purge` command. Users clear via `rm ~/.local/share/lacon/history.db` or `sqlite3` DELETE.
  - **No telemetry, no remote sync, no network access.**

### CON-tracking-migration-policy

- **source:** docs/specs/tracking-data-model.md
- **type:** schema
- **content:** Schema changes ship as numbered migrations applied automatically at startup. Migrations are append-only — never edit a migration after release. Down migrations are not supported.

### CON-tracking-tokens-not-in-v1

- **source:** docs/specs/tracking-data-model.md
- **type:** schema
- **content:** Token counts are NOT in v1 schema (deferred to backlog). Existing counters are explicitly byte-named (`raw_stdout_bytes`, `raw_stderr_bytes`, `filtered_bytes`) so token columns can be appended via standard append-only migration. Cost estimates and cross-machine sync state likewise out of scope.

---

## Chained-commands protocol constraints

### CON-chained-splitting-boundaries

- **source:** docs/specs/chained-commands.md
- **type:** protocol
- **content:** Chains split at TOP-LEVEL `&&`, `||`, `;`. Top-level means: not inside quotes, not inside `(...)` subshells, not inside `$(...)` or backtick command substitution, not inside `${...}` parameter expansion, not inside heredoc bodies. Pipes (`|`) are NOT chain operators — a pipeline is a single segment. Filtering inside pipes is out of v1 scope.

### CON-chained-opaque-constructs

- **source:** docs/specs/chained-commands.md
- **type:** protocol
- **content:** Constructs treated as part of whatever segment contains them (not split): subshells `(cmd1 && cmd2)`, command substitution `$(cmd1 && cmd2)`/`` `cmd1 && cmd2` ``, process substitution `<(...)`/`>(...)`, heredoc bodies, quoted strings containing chain operators.

### CON-chained-rule-resolution-per-segment

- **source:** docs/specs/chained-commands.md
- **type:** protocol
- **content:** Each segment is resolved against the rule registry independently (first-match-wins, project > user > bundled). No merging. No cross-segment rule effects. Two outcomes per segment: matched (wrapped as `lacon run --rule <id> -- <seg>`) or unmatched (passed through unchanged).

### CON-chained-rewrite-emission

- **source:** docs/specs/chained-commands.md
- **type:** protocol
- **content:** Hook reassembles the chain by joining segments with the original operators, preserving order and operator type. Example: `pnpm install && pnpm test || echo failed` → `lacon run --rule pkg-install -- pnpm install && lacon run --rule vitest -- pnpm test || echo failed`.

### CON-chained-exit-code-propagation

- **source:** docs/specs/chained-commands.md
- **type:** protocol
- **content:** `lacon run` propagates its wrapped subprocess's exit code unchanged. Shell `&&`/`||`/`;` semantics work exactly as if `lacon run` weren't present. Filtering one segment cannot change whether or how the next segment runs — filtering changes what the model sees, not what the shell sees.

### CON-chained-bypass-whole-command

- **source:** docs/specs/chained-commands.md
- **type:** protocol
- **content:** `!!` prefix and `LACON_DISABLE=1` env var bypass at WHOLE-COMMAND granularity, not per segment. Whole rewrite is skipped; original command returned unchanged.

### CON-chained-tui-bypass-whole-chain

- **source:** docs/specs/chained-commands.md
- **type:** protocol
- **content:** TUI heuristic `is_tui(command, args) -> bool` runs in adapter, per-segment, AFTER chain splitting and BEFORE rule resolution. If any segment returns true, the ENTIRE INPUT is bypassed (single commands treated as 1-segment chain). Granular per-segment TUI bypass is backlog v2.

### CON-chained-tui-list-v1

- **source:** docs/specs/chained-commands.md
- **type:** protocol
- **content:** v1 hardcoded TUI list lives in adapter code (NOT user config). User-overridable list is backlog. Pure TUI by `argv[0]` basename: `vim`, `vi`, `nvim`, `nano`, `emacs`, `less`, `more`, `most`, `man`, `htop`, `top`, `btop`, `screen`, `tmux`, `ssh`, `mosh`, `ipython`, `irb`, `pry`, `redis-cli`, `crontab`, `visudo`. Conditional patterns:
  - `git rebase` interactive when `-i` or `--interactive` present
  - `git commit` interactive when none of `-m` / `--message` / `--message=…` / `-F` / `--file` present
  - `git add` interactive when `-p` / `--patch` / `-i` / `--interactive` present
  - `git checkout` interactive when `-p` / `--patch` present
  - `git stash` interactive when `-p` / `--patch` present
  - `npm init`, `yarn init`, `pnpm init` interactive when neither `-y` nor `--yes` present
  - `node`, `python`, `python3` interactive when no positional argument (REPL)
  - `mysql`, `psql`, `sqlite3` interactive when no positional argument

### CON-chained-test-obligations

- **source:** docs/specs/chained-commands.md
- **type:** nfr
- **content:** Splitter must have tests covering: single command no chain; two-segment chain per operator (`&&`, `||`, `;`); mixed-operator chain; per-segment differing rule matches; one segment unmatched; one segment interactive (whole-chain bypass); chain inside subshell `(a && b)` (single segment); chain inside command substitution `echo $(a && b)` (single segment); chain operator inside quoted string `echo "a && b"` (single segment); pipeline as a segment `a | b && c` (splits to `[a | b, c]`); heredoc body containing chain operators (opaque); whole-chain bypass via `!!`; whole-chain bypass via `LACON_DISABLE=1`.

---

## Cross-cutting NFRs (sourced from ADRs and PRDs, surfaced here for the implementation contract)

### CON-nfr-cold-start-budget

- **source:** docs/decisions/0002-rust-as-primary-language.md, docs/decisions/0013-filter-via-pretooluse-wrapper.md, docs/v1-scope.md
- **type:** nfr
- **content:** Cold-start binary invocation must be ≤10ms on the hook hot path. ADR 0013 tightens this — `lacon run` is now a production code path, invoked thousands of times per session.

### CON-nfr-streaming-memory

- **source:** docs/decisions/0005-streaming-first.md
- **type:** nfr
- **content:** Memory bounded by largest stateful primitive (typically `keep_tail N`) plus `max_bytes` cap. No relationship to total command output size. Long builds must not OOM the hook process or parent assistant.

### CON-nfr-stderr-merge

- **source:** docs/decisions/0013-filter-via-pretooluse-wrapper.md
- **type:** nfr
- **content:** stderr merges into stdout inside `lacon run`. Pipeline operates on a single combined stream. Stream separation is lost; ordering may differ from raw terminal interleaving. Best-effort line atomicity, no cross-stream order guarantee (per `docs/open-questions.md` deferred-to-prototyping resolution).

### CON-nfr-tty-detection-downstream

- **source:** docs/decisions/0013-filter-via-pretooluse-wrapper.md
- **type:** nfr
- **content:** Tools spawned by `lacon run` see "not a TTY" because the wrapper does not allocate a PTY. Most tools emit less noise in non-TTY mode; some change semantics (`git status` short form, `ls` non-columnar) — generally aligned with what we want.

### CON-nfr-no-network-no-daemon

- **source:** docs/decisions/0011-sqlite-for-tracking.md, docs/vision.md
- **type:** nfr
- **content:** No daemon, no network, local-only. SQLite single-file storage; backup is `cp history.db backup.db`. WAL mode handles concurrent writes from multiple `lacon` processes safely.

### CON-nfr-platform-support

- **source:** docs/v1-scope.md
- **type:** nfr
- **content:** v1 supports macOS + Linux (and WSL by extension). Native Windows is deferred.
