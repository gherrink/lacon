# Requirements (synthesized from PRD-class docs)

Two PRD-class documents are in the ingest set:

- `docs/v1-scope.md` (PRD, high confidence) — defines the v1 release scope, in-scope features, acceptance criteria, explicit exclusions, and coverage boundary.
- `docs/vision.md` (PRD, medium confidence) — strategic problem/approach/non-goals; lacks user stories and per-requirement acceptance criteria.

Requirements are derived from `docs/v1-scope.md`'s "In scope" + "Acceptance criteria" sections (concrete, testable). `docs/vision.md` contributes target outcomes and architectural commitments rather than itemized requirements; those are folded into the Vision-derived block at the end.

No competing acceptance variants between the two PRDs were detected. They overlap (both name <10 ms cold start, local-only, Claude Code first), and on overlapping points they agree byte-for-byte.

---

## Engine requirements

### REQ-engine-streaming-primitives

- **source:** docs/v1-scope.md
- **description:** v1 ships a streaming output processor with the native primitives `strip_ansi`, `drop_regex`, `keep_regex`, `replace_regex`, `dedupe`, `collapse_repeated`, `keep_head`, `keep_tail`, `keep_around_match`, `max_bytes`.
- **acceptance:** All ten primitives implemented as line-by-line streaming transformers (per ADR 0005 / SPEC filter-rule-schema). Memory bounded by largest stateful primitive plus `max_bytes` cap.

### REQ-engine-starlark-postprocess

- **source:** docs/v1-scope.md
- **description:** Starlark escape hatch ships as a `post_process` step (aggregated, not per-line). Cold-start cost paid per invocation; no shared process / IPC.
- **acceptance:** `script` and `post_process` stages execute Starlark function `process(ctx, lines) -> list[str]` on aggregated output (per ADR 0008 / SPEC filter-rule-schema).

### REQ-engine-rule-loading

- **source:** docs/v1-scope.md
- **description:** Rule loading from `bundled/`, `~/.config/lacon/rules/`, `<project>/.lacon/rules/` with project > user > bundled precedence and first-match-wins resolution (per ADRs 0004, 0007).
- **acceptance:** Resolver walks layers in priority order, returns first matching rule. No cross-layer merging.

### REQ-engine-extends

- **source:** docs/v1-scope.md
- **description:** `extends` inheritance is append-only: parent pipeline prepends, scalar fields inherited only when child omits them (per ADR 0012).
- **acceptance:** No remove/reorder/insert operations exposed in v1.

### REQ-engine-on-error

- **source:** docs/v1-scope.md
- **description:** `on_error` block fully replaces the success pipeline when the wrapped command exits non-zero (per ADR 0010). Implemented inside `lacon run` (per ADR 0013).
- **acceptance:** `on_error` swap occurs based on observed subprocess exit code; success buffer is discarded.

### REQ-engine-rewrite

- **source:** docs/v1-scope.md
- **description:** Pre-execution command rewriting via `rewrite.add_flags` / `remove_flags` / `replace_flags` (per ADR 0006 / SPEC filter-rule-schema).
- **acceptance:** `add_flags` is idempotent (won't duplicate). Adapter applies the rewrite block to inner argv before wrapping.

### REQ-engine-bypass

- **source:** docs/v1-scope.md, docs/specs/chained-commands.md
- **description:** Bypass mechanics: `!!` command prefix and `LACON_DISABLE=1` env var skip filtering entirely.
- **acceptance:** Bypass is whole-command granularity (NOT per-segment in chains). Hook returns the original command unchanged.

### REQ-engine-max-bytes-cap

- **source:** docs/v1-scope.md
- **description:** Hard `max_bytes` cap as final-stage safety net. Default 32768 bytes from `defaults.max_bytes` config when a rule omits its own `max_bytes` primitive.
- **acceptance:** Engine never returns more than `max_bytes` from a pipeline; truncation marker appended on overflow.

---

## Claude Code adapter requirements

### REQ-adapter-pretooluse-only

- **source:** docs/v1-scope.md
- **description:** Adapter installs ONLY a `PreToolUse` hook for the Bash tool (per ADR 0013). No `PostToolUse` hook in v1.
- **acceptance:** Hook resolves rule, applies `rewrite` block to inner argv, wraps matched commands as `lacon run --rule <id> -- <inner-cmd>` via `hookSpecificOutput.updatedInput`. Unmatched commands returned unchanged.

### REQ-adapter-bypass-detection

- **source:** docs/v1-scope.md
- **description:** Hook detects `!!` prefix and `LACON_DISABLE=1` env var; on detection bypasses by returning the original command unchanged.

### REQ-adapter-chained-commands

- **source:** docs/v1-scope.md, docs/specs/chained-commands.md
- **description:** Adapter splits chained commands at top-level `&&`, `||`, `;` (NOT at `|`, NOT inside quotes/subshells/command-substitution/heredocs) and wraps each matched segment independently with its own `lacon run --rule <id> --` prefix. Unmatched segments pass through unchanged. Original operators preserved.
- **acceptance:** Splitter test obligations enumerated in `docs/specs/chained-commands.md` (single command, two-segment chain per operator, mixed operators, mixed match/unmatched, subshell opacity, command-substitution opacity, quoted-string opacity, pipeline-as-segment, heredoc opacity, `!!` whole-chain bypass, `LACON_DISABLE=1` whole-chain bypass).

### REQ-adapter-tui-bypass

- **source:** docs/v1-scope.md, docs/specs/chained-commands.md
- **description:** TUI heuristic `is_tui(command, args) -> bool` runs per-segment **after** chain splitting and **before** rule resolution. If any segment matches, the **entire chain** is bypassed (v1 conservative rule). Hardcoded list lives in adapter code, not user config.
- **acceptance:** v1 list per `docs/specs/chained-commands.md`: pure-TUI by basename (`vim`, `vi`, `nvim`, `nano`, `emacs`, `less`, `more`, `most`, `man`, `htop`, `top`, `btop`, `screen`, `tmux`, `ssh`, `mosh`, `ipython`, `irb`, `pry`, `redis-cli`, `crontab`, `visudo`); conditional patterns for `git rebase -i`, `git commit` w/o `-m`/`-F`, `git add -p/-i`, `git checkout -p`, `git stash -p`, `npm/yarn/pnpm init` w/o `-y`, REPLs (`node`, `python`, `python3`, `mysql`, `psql`, `sqlite3`) with no positional argument.

### REQ-adapter-pipes-passthrough

- **source:** docs/v1-scope.md
- **description:** Pipes (`|`) and subshells: matched argv is wrapped as a unit (the user's pipe is preserved inside the `--` boundary). Filtering inside pipes is OUT OF SCOPE for v1.

---

## Tracking requirements

### REQ-tracking-sqlite-location

- **source:** docs/v1-scope.md
- **description:** SQLite database at `~/.local/share/lacon/history.db` with WAL mode and `0700` directory permissions (per ADR 0011 / SPEC tracking-data-model).
- **acceptance:** Schema migrations applied at startup (append-only). Pruning runs at startup.

### REQ-tracking-schema

- **source:** docs/v1-scope.md, docs/specs/tracking-data-model.md
- **description:** Three tables: `invocations` (metadata), `raw_outputs` (bulk blobs, FK from invocations), `suspected_regressions` (cascade FK to invocations). Indexes and views as enumerated in `docs/specs/tracking-data-model.md`.
- **acceptance:** Views ship: `v_unmatched_offenders`, `v_filtered_offenders`, `v_bypass_rate`, `v_project_savings`.

### REQ-tracking-raw-outputs-default-off

- **source:** docs/v1-scope.md
- **description:** `raw_outputs` storage is OFF by default. Opt-in per project via `store_raw_outputs: true` in `.lacon/config.yaml` (per ADR 0009 / SPEC tracking-data-model).

### REQ-tracking-privacy-warning

- **source:** docs/v1-scope.md, docs/specs/tracking-data-model.md
- **description:** First-time enablement of `raw_outputs` (off → on transition) prints a one-time stderr privacy notice. Suppressed on subsequent invocations via a marker in the project config dir.
- **acceptance:** No automatic redaction in v1 (deferred to backlog).

### REQ-tracking-retention-defaults

- **source:** docs/v1-scope.md, docs/specs/config-schema.md
- **description:** Default retention: 30 days for `invocations` and `suspected_regressions`, 3 days for `raw_outputs`. Configurable in user config; `retention.*` keys are USER-ONLY (project files including a `retention` block fail validation).

---

## CLI surface requirements

### REQ-cli-init

- **source:** docs/v1-scope.md
- **description:** `lacon init` sets up `.lacon/` in the current project, configures the Claude Code `PreToolUse` hook, adds a tiny CLAUDE.md instruction line.

### REQ-cli-run

- **source:** docs/v1-scope.md, docs/decisions/0013-filter-via-pretooluse-wrapper.md
- **description:** `lacon run [--rule <id>] -- <cmd> [args...]` is the production wrapper invoked by the `PreToolUse` rewrite. Spawns the subprocess, merges stdout+stderr, filters, propagates the subprocess's exit code. Without `--rule`, runs the resolver inline against `<cmd>` for manual testing.

### REQ-cli-stats

- **source:** docs/v1-scope.md
- **description:** `lacon stats` shows top offenders, bypass rates, unmatched commands; supports `--project`, `--since`, `--rule` filters.

### REQ-cli-explain

- **source:** docs/v1-scope.md
- **description:** `lacon explain <id>` re-runs filtering against stored raw output, shows side-by-side diff. Requires raw retention to have been enabled at the time of the invocation.

### REQ-cli-doctor

- **source:** docs/v1-scope.md
- **description:** `lacon doctor` verifies hooks are installed, config files are valid, rules parse. Runs config validation on every layer's `config.yaml` in addition to its rule sweep.

### REQ-cli-validate

- **source:** docs/v1-scope.md, docs/specs/config-schema.md
- **description:** `lacon validate <path>` lints a rule file or a `config.yaml` without running it. Dispatcher detects file type by content (`id` + `match` → rule; otherwise config). Files that fail validation are rejected at load time; `lacon` does NOT silently fall back to defaults on malformed config.

### REQ-cli-surface-cap

- **source:** docs/v1-scope.md
- **description:** v1 ships exactly six CLI commands (above). No `lacon purge`, no `lacon install`, no `lacon stats --serve` — those are backlog.

---

## Bundled rule library requirements

### REQ-bundled-rules-tier1

- **source:** docs/v1-scope.md, docs/bundled-rules-roadmap.md
- **description:** v1 ships ten Tier 1 bundled rules: `pkg-install`, `cargo-build`, `cargo-test`, `vitest`, `jest`, `pytest`, `tsc`, `eslint`, `git-status`, `docker-build`.
- **acceptance:** Each rule reduces its target commands by **at least 50%** on representative output WITHOUT dropping errors. Each has at minimum one success-path fixture and one failure-path fixture under `tests/fixtures/<rule-id>/<scenario>/` per `docs/testing-rules.md`.

### REQ-bundled-rules-format

- **source:** docs/bundled-rules-roadmap.md, docs/testing-rules.md
- **description:** Every bundled rule lands with: a YAML rule file in `bundled-rules/`, a fixture set under `tests/fixtures/<rule-id>/<scenario>/` (`input.txt`, `expected.txt`, `meta.yaml`), an integration test asserting reduction ratio and zero error-line drops, and a doc note in `bundled-rules-roadmap.md`.

---

## Acceptance criteria (v1 ship gate)

From `docs/v1-scope.md` "Acceptance criteria":

### REQ-acceptance-bundled-reduction

- **source:** docs/v1-scope.md
- **description:** All ten bundled rules reduce their target commands by at least 50% on representative output without dropping errors.

### REQ-acceptance-pnpm-end-to-end

- **source:** docs/v1-scope.md
- **description:** `lacon init` followed by a `pnpm install` in any new project works end-to-end with no manual config.

### REQ-acceptance-cold-start-budget

- **source:** docs/v1-scope.md, docs/vision.md
- **description:** Cold-start binary invocation is under **10ms** (measured on the hook hot path).

### REQ-acceptance-explain-reproducibility

- **source:** docs/v1-scope.md
- **description:** `lacon explain` correctly reproduces the filtering decision for any tracked invocation.

### REQ-acceptance-hot-reload

- **source:** docs/v1-scope.md
- **description:** Rule files can be edited and changes take effect on the next invocation (no daemon, no restart).

### REQ-acceptance-test-coverage

- **source:** docs/v1-scope.md, docs/testing-rules.md
- **description:** Test suite covers each native primitive, the chained-command splitter, and every bundled rule via fixture-based integration tests. CI is hermetic — never installs `pnpm`, `cargo`, etc.

---

## Documentation requirements

### REQ-docs-readme

- **source:** docs/v1-scope.md
- **description:** README with install + quickstart.

### REQ-docs-worked-example

- **source:** docs/v1-scope.md
- **description:** A worked example: writing a project-specific filter rule.

### REQ-docs-primitive-reference

- **source:** docs/v1-scope.md
- **description:** Reference for every primitive.

---

## Coverage boundary (declarative, surfaced as constraints/non-requirements)

From `docs/v1-scope.md` "Coverage boundary":

- **Invisible to lacon (fundamental):** subprocesses spawned by non-Bash tools or MCP servers — only `PreToolUse(Bash)` fires.
- **Invisible by design:** output redirected to files/sockets/non-stdout sinks; backgrounded long-running processes whose continuing output isn't captured; direct `/dev/tty` writes.
- **Intentionally out of scope:** the user's own terminal sessions outside Claude Code.
- **In scope (clarification):** ANSI escape sequences and cursor-control codes that flow through stdout/stderr ARE filterable via `strip_ansi` — not a coverage gap.

---

## Vision-derived strategic targets (not testable at REQ granularity)

From `docs/vision.md`:

- **Outcome target:** 30–70% reduction in bash output bytes on common commands without measurable loss in assistant quality.
- **Outcome target:** Negligible runtime overhead (<10ms per command on the hook hot path) — agrees with REQ-acceptance-cold-start-budget.
- **Outcome target:** Project rules can be added in a single YAML file with no code changes — agrees with the rule schema and v1 scope.
- **Outcome target:** Trust property — user can always see what was filtered (via `lacon explain`) and bypass when needed.
- **Non-goal:** Not an LLM. No model calls. No embeddings.
- **Non-goal:** Not a shell. Doesn't replace bash, doesn't intercept interactive sessions.
- **Non-goal:** Not a remote service. All processing and storage local. No telemetry.
- **Non-goal:** Not a general-purpose log filter. Optimized for command output in coding-assistant contexts.
- **Architectural commitment:** Local-only by default, streaming over buffered, fast startup, cross-assistant ready (core engine assistant-agnostic).

---

## Explicit exclusions (from v1-scope, surface to roadmapper as "deferred")

From `docs/v1-scope.md` "Explicitly out of scope for v1":

- Adapters for Cursor, aider, or any non-Claude-Code assistant
- Per-token (vs per-byte) accounting
- Per-line streaming Starlark
- Filtering inside pipes
- Native Windows support (WSL is fine; native is deferred)
- Sharing rules across machines / a public rule registry

Full backlog in `docs/backlog.md`.
