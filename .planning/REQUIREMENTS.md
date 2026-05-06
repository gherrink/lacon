# Requirements: lacon

**Defined:** 2026-05-06
**Core Value:** Reduce the bytes an AI coding assistant ingests from bash output by 30–70% without dropping signal — locally, with sub-10ms cold start, and a YAML rule per command.

## v1 Requirements

Requirements for the v1 release. Each maps to exactly one phase in `.planning/ROADMAP.md`. IDs preserved verbatim from `.planning/intel/requirements.md`.

### Engine

- [x] **REQ-engine-streaming-primitives**: v1 ships a streaming output processor implementing the ten native primitives — `strip_ansi`, `drop_regex`, `keep_regex`, `replace_regex`, `dedupe`, `collapse_repeated`, `keep_head`, `keep_tail`, `keep_around_match`, `max_bytes` — as line-by-line streaming transformers. Memory bounded by largest stateful primitive plus the `max_bytes` cap. (ADR-0005, SPEC filter-rule-schema.)
- [x] **REQ-engine-starlark-postprocess**: Starlark escape hatch ships as a `post_process` step running on aggregated post-pipeline output. Function signature `def process(ctx, lines) -> list[str]`. Cold-start cost paid per invocation; no shared process / IPC. (ADR-0008, SPEC filter-rule-schema.)
- [x] **REQ-engine-rule-loading**: Rule loading from `bundled/`, `~/.config/lacon/rules/`, `<project>/.lacon/rules/` with project > user > bundled precedence and first-match-wins resolution. Resolver walks layers in priority order, returns the first matching rule; no cross-layer merging. (ADRs 0004, 0007.)
- [x] **REQ-engine-extends**: `extends` inheritance is append-only — parent `pipeline` PREPENDED to child's; scalar fields inherited only when child omits them. No remove/reorder/insert operations exposed in v1. (ADR-0012.)
- [x] **REQ-engine-on-error**: `on_error` block fully replaces the success pipeline (and optionally `post_process`) when the wrapped command exits non-zero. Implemented inside `lacon run` via observed subprocess exit code; success buffer is discarded on swap. (ADR-0010, ADR-0013.)
- [x] **REQ-engine-rewrite**: Pre-execution command rewriting via `rewrite.add_flags` / `remove_flags` / `replace_flags`. `add_flags` is idempotent (won't duplicate). Adapter applies the rewrite block to inner argv before wrapping. (ADR-0006.)
- [x] **REQ-engine-bypass**: Bypass mechanics — `!!` command prefix and `LACON_DISABLE=1` env var skip filtering entirely. Bypass is whole-command granularity (NOT per-segment in chains); hook returns the original command unchanged.
- [x] **REQ-engine-max-bytes-cap**: Hard `max_bytes` cap as final-stage safety net. Default 32768 bytes from `defaults.max_bytes` config when a rule omits its own `max_bytes` primitive. Engine never returns more than `max_bytes` from a pipeline; truncation marker `[lacon: truncated, N more bytes dropped]` appended on overflow.

### Claude Code adapter

- [ ] **REQ-adapter-pretooluse-only**: Adapter installs ONLY a `PreToolUse` hook for the Bash tool. No `PostToolUse` hook in v1. Hook resolves rule, applies `rewrite` block to inner argv, wraps matched commands as `lacon run --rule <id> -- <inner-cmd>` via `hookSpecificOutput.updatedInput`. Unmatched commands returned unchanged. (ADR-0013.)
- [ ] **REQ-adapter-bypass-detection**: Hook detects `!!` prefix and `LACON_DISABLE=1` env var; on detection bypasses by returning the original command unchanged.
- [ ] **REQ-adapter-chained-commands**: Adapter splits chained commands at top-level `&&`, `||`, `;` (NOT at `|`, NOT inside quotes/subshells/command-substitution/heredocs) and wraps each matched segment independently. Unmatched segments pass through unchanged. Original operators preserved when reassembling. Splitter must satisfy the 13-scenario test matrix in `docs/specs/chained-commands.md`.
- [ ] **REQ-adapter-tui-bypass**: TUI heuristic `is_tui(command, args) -> bool` runs per-segment AFTER chain splitting and BEFORE rule resolution. If any segment matches, the entire chain is bypassed (v1 conservative). Hardcoded list lives in adapter code (not user config). v1 list per `docs/specs/chained-commands.md`.
- [ ] **REQ-adapter-pipes-passthrough**: Pipes (`|`) and subshells: matched argv is wrapped as a unit, preserving the user's pipe inside the `--` boundary. Filtering inside pipes is OUT OF SCOPE for v1.

### Tracking

- [ ] **REQ-tracking-sqlite-location**: SQLite database at `~/.local/share/lacon/history.db` with WAL mode and `0700` directory permissions. Schema migrations applied at startup (append-only). Pruning runs at startup. (ADR-0011, SPEC tracking-data-model.)
- [x] **REQ-tracking-schema**: Three tables — `invocations` (metadata), `raw_outputs` (bulk blobs, FK from invocations), `suspected_regressions` (cascade FK to invocations). Indexes and views ship as enumerated in `docs/specs/tracking-data-model.md`. Required views: `v_unmatched_offenders`, `v_filtered_offenders`, `v_bypass_rate`, `v_project_savings`.
- [ ] **REQ-tracking-raw-outputs-default-off**: `raw_outputs` storage is OFF by default. Opt-in per project via `store_raw_outputs: true` in `.lacon/config.yaml`. (ADR-0009, SPEC tracking-data-model.)
- [ ] **REQ-tracking-privacy-warning**: First-time enablement of `raw_outputs` (off → on transition) prints a one-time stderr privacy notice. Suppressed on subsequent invocations via a marker in the project config dir. No automatic redaction in v1 (deferred to backlog).
- [ ] **REQ-tracking-retention-defaults**: Default retention — 30 days for `invocations` and `suspected_regressions`, 3 days for `raw_outputs`. Configurable in user config; `retention.*` keys are USER-ONLY (project files including a `retention` block fail validation).

### CLI surface

- [ ] **REQ-cli-init**: `lacon init` sets up `.lacon/` in the current project, configures the Claude Code `PreToolUse` hook in `.claude/settings.json`, adds a tiny CLAUDE.md instruction line.
- [x] **REQ-cli-run**: `lacon run [--rule <id>] -- <cmd> [args...]` is the production wrapper invoked by the `PreToolUse` rewrite. Spawns the subprocess, merges stdout+stderr, filters, propagates the subprocess's exit code. Without `--rule`, runs the resolver inline against `<cmd>` for manual testing. (ADR-0013.)
- [ ] **REQ-cli-stats**: `lacon stats` shows top offenders, bypass rates, unmatched commands; supports `--project`, `--since`, `--rule` filters.
- [ ] **REQ-cli-explain**: `lacon explain <id>` re-runs filtering against stored raw output and shows side-by-side diff. Requires raw retention to have been enabled at the time of the invocation.
- [ ] **REQ-cli-doctor**: `lacon doctor` verifies hooks are installed, config files are valid, rules parse. Runs config validation on every layer's `config.yaml` in addition to its rule sweep.
- [x] **REQ-cli-validate**: `lacon validate <path>` lints a rule file or a `config.yaml` without running it. Dispatcher detects file type by content (`id` + `match` → rule; otherwise config). Files that fail validation are rejected at load time; `lacon` does NOT silently fall back to defaults on malformed config.
- [ ] **REQ-cli-surface-cap**: v1 ships exactly six CLI commands (`init`, `run`, `stats`, `explain`, `doctor`, `validate`). No `lacon purge`, no `lacon install`, no `lacon stats --serve` — those are backlog. CLI parser rejects any seventh subcommand attempt.

### Bundled rule library

- [ ] **REQ-bundled-rules-tier1**: v1 ships ten Tier 1 bundled rules — `pkg-install`, `cargo-build`, `cargo-test`, `vitest`, `jest`, `pytest`, `tsc`, `eslint`, `git-status`, `docker-build`. Each rule reduces its target commands by **at least 50%** on representative output WITHOUT dropping errors. Each ships at minimum one success-path fixture and one failure-path fixture under `tests/fixtures/<rule-id>/<scenario>/`.
- [ ] **REQ-bundled-rules-format**: Every bundled rule lands with: a YAML rule file in `bundled-rules/`, a fixture set under `tests/fixtures/<rule-id>/<scenario>/` (`input.txt`, `expected.txt`, `meta.yaml`), an integration test asserting reduction ratio and zero error-line drops, and a doc note in `bundled-rules-roadmap.md`.

### Acceptance criteria (v1 ship gate)

- [ ] **REQ-acceptance-bundled-reduction**: All ten bundled rules reduce their target commands by at least 50% on representative output without dropping errors.
- [ ] **REQ-acceptance-pnpm-end-to-end**: `lacon init` followed by a `pnpm install` in any new project works end-to-end with no manual config — hook fires, command is wrapped, filtered output reaches the assistant.
- [ ] **REQ-acceptance-cold-start-budget**: Cold-start binary invocation is under **10ms** (measured on the hook hot path). ADR-0013 tightens this — `lacon run` is now a production code path, invoked thousands of times per session.
- [ ] **REQ-acceptance-explain-reproducibility**: `lacon explain` correctly reproduces the filtering decision for any tracked invocation that has stored raw output.
- [ ] **REQ-acceptance-hot-reload**: Rule files can be edited and changes take effect on the next invocation (no daemon, no restart). Resolver invalidates compiled regex cache on file mtime change.
- [ ] **REQ-acceptance-test-coverage**: Test suite covers each native primitive, the chained-command splitter, and every bundled rule via fixture-based integration tests. CI is hermetic — never installs `pnpm`, `cargo`, etc.

### Documentation

- [ ] **REQ-docs-readme**: README with install + quickstart.
- [ ] **REQ-docs-worked-example**: A worked example: writing a project-specific filter rule.
- [ ] **REQ-docs-primitive-reference**: Reference for every primitive with at least one example each.

## Vision-derived strategic targets

From `docs/vision.md`. These are not testable at REQ granularity but constrain how acceptance criteria are interpreted.

- **Outcome target:** 30–70% reduction in bash output bytes on common commands without measurable loss in assistant quality. (Operationalized in v1 as REQ-acceptance-bundled-reduction's "≥50% per rule, no error drops" floor.)
- **Outcome target:** Negligible runtime overhead (<10ms per command on the hook hot path). (Same target as REQ-acceptance-cold-start-budget.)
- **Outcome target:** Project rules can be added in a single YAML file with no code changes. (Implied by SPEC filter-rule-schema and `lacon validate` accepting third-party rule files.)
- **Outcome target — trust property:** User can always see what was filtered (via `lacon explain`) and bypass when needed (via `!!` or `LACON_DISABLE=1`).
- **Non-goal:** Not an LLM. No model calls. No embeddings.
- **Non-goal:** Not a shell. Doesn't replace bash, doesn't intercept interactive sessions.
- **Non-goal:** Not a remote service. All processing and storage local. No telemetry.
- **Non-goal:** Not a general-purpose log filter. Optimized for command output in coding-assistant contexts.
- **Architectural commitment:** Local-only by default, streaming over buffered, fast startup, cross-assistant ready (core engine assistant-agnostic).

## v2 Requirements

Deferred to future release. Tracked but not in current v1 roadmap. Source: `docs/backlog.md`.

### Adapters

- **Adapter — Cursor**: Cursor IDE adapter on top of the assistant-agnostic core.
- **Adapter — aider**: aider adapter.
- **Adapter — generic shell wrapper**: opt-in PATH shim for non-hook-native assistants.
- **Adapter — editor-side (Continue, etc.)**: editor-resident integrations.

### Engine features

- **Engine — per-line streaming Starlark**: per-line evaluator (gated on benchmark data justifying the cost).
- **Engine — filter inside pipes**: pipe-aware splitting so each pipe stage can have its own rule.
- **Engine — heredoc/subshell/eval handling**: filtering inside opaque constructs.
- **Engine — granular per-segment TUI bypass**: bypass only the interactive segment in a chain (gated on tracking data showing whole-chain bypass leaves savings on the table).
- **Engine — user-overridable TUI list**: `~/.config/lacon/tui-commands.yaml` (deferred until clear false-positive pattern emerges).
- **Engine — multi-rule merging**: probably-bad idea, kept for option.
- **Engine — conditional pipeline stages inline**: stage-level guards.
- **Engine — stage-level inheritance operations**: insert/remove/reorder on inherited stages.
- **Engine — persistent Starlark interpreter / helper process**: gated on benchmark data.

### Tracking

- **Tracking — per-token accounting**: tokenizer choice (Anthropic Messages API `count_tokens` vs vendored vs heuristic). Schema is forward-compatible via append-only migration.
- **Tracking — session-aware aggregation**: rollups per Claude Code session.
- **Tracking — cost estimation**: $/session derived from token counts.
- **Tracking — trend graphs**: stats over time.

### Sharing & discovery

- **Sharing — public rule registry**: `lacon install gh:user/repo`.
- **Sharing — cross-machine sync**: optional sync of user rules.
- **Sharing — suggestion engine**: surface candidate-new-rule offenders from `v_unmatched_offenders`.

### UI

- **UI — web UI**: `lacon stats --serve`.
- **UI — TUI dashboard**.
- **UI — VS Code extension**.

### Platforms

- **Platform — native Windows**.
- **Platform — static musl builds for distroless containers**.

### Programmatic

- **Programmatic — library API**: Rust crate or WASM.
- **Programmatic — plugin protocol over stdio**.

### Quality of life

- **QoL — rule hot-reload notifications**.
- **QoL — filter dry-run mode in CI**.
- **QoL — user-facing fixture validation**: `lacon validate <rule.yaml> --fixtures <dir>`.
- **QoL — automated fixture drift detection**.
- **QoL — rule profiler**.
- **QoL — redaction patterns** (deferred for false-confidence risk).
- **QoL — `lacon purge` command** (deferred to keep CLI surface at six commands).
- **QoL — encryption at rest for `raw_outputs`**.

## Out of Scope

Explicit v1 exclusions from `docs/v1-scope.md` "Explicitly out of scope" and "Coverage boundary" sections.

| Feature | Reason |
|---------|--------|
| Adapters for Cursor / aider / non-Claude-Code assistants | Claude Code first; core engine is assistant-agnostic so other adapters land later without breaking v1. |
| Per-token (vs per-byte) accounting | Tokenizer choice is v2 design; tracking columns are byte-named and forward-compatible via append-only migration. |
| Per-line streaming Starlark | Per-line Starlark would dominate runtime; aggregated `post_process` (ADR-0008) is the only Starlark surface in v1. |
| Filtering inside pipes (`\|`) | Pipes are part of a single segment; the user's pipe is preserved inside `--` boundary. Per-pipe filtering requires a different splitter. |
| Native Windows support | macOS + Linux only in v1; WSL is fine. Native Win32 hook integration is deferred. |
| Public rule registry / cross-machine sync | No telemetry, no remote calls in v1. `lacon install gh:...` is backlog. |
| Granular per-segment TUI bypass in chains | v1 uses whole-chain bypass when any segment looks interactive. Granular bypass gated on tracking data showing it's needed. |
| User-overridable TUI list | Hardcoded in adapter; user override deferred until clear false-positive pattern. |
| Automatic redaction of `raw_outputs` | Best-effort regex creates false-confidence risk. v1 ships off-by-default + `0700` + opt-in stderr warning. |
| `lacon purge` subcommand | Would push CLI past the six-command surface (REQ-cli-surface-cap). Manual cleanup via `rm history.db` or `sqlite3 DELETE`. |
| Daemon / persistent Starlark interpreter | Load-bearing property: no daemon. Reconsider only if real benchmark data justifies it. |
| Multi-rule merging across layers / specificity ranking | First-match-wins is the contract (ADR-0007). |
| Subprocess output from non-Bash MCP tools | Fundamental limitation: only `PreToolUse(Bash)` fires. Out of `lacon`'s reach. |
| Output redirected to files / sockets / `/dev/tty` | Invisible by design; never reaches assistant context window so nothing for `lacon` to do. |
| User's own terminal sessions outside Claude Code | Intentionally out of scope; `lacon` is not a shell. |

## Traceability

Phase mappings populated during ROADMAP creation. 36/36 v1 requirements mapped, no orphans.

| Requirement | Phase | Status |
|-------------|-------|--------|
| REQ-engine-streaming-primitives | Phase 1 | Complete |
| REQ-engine-starlark-postprocess | Phase 1 | Complete |
| REQ-engine-rule-loading | Phase 1 | Complete |
| REQ-engine-extends | Phase 1 | Complete |
| REQ-engine-on-error | Phase 1 | Complete |
| REQ-engine-rewrite | Phase 1 | Complete |
| REQ-engine-bypass | Phase 1 | Complete |
| REQ-engine-max-bytes-cap | Phase 1 | Complete |
| REQ-cli-run | Phase 1 | Complete |
| REQ-cli-validate | Phase 1 | Complete |
| REQ-tracking-sqlite-location | Phase 2 | Pending |
| REQ-tracking-schema | Phase 2 | Complete |
| REQ-tracking-raw-outputs-default-off | Phase 2 | Pending |
| REQ-tracking-privacy-warning | Phase 2 | Pending |
| REQ-tracking-retention-defaults | Phase 2 | Pending |
| REQ-adapter-pretooluse-only | Phase 3 | Pending |
| REQ-adapter-bypass-detection | Phase 3 | Pending |
| REQ-adapter-chained-commands | Phase 3 | Pending |
| REQ-adapter-tui-bypass | Phase 3 | Pending |
| REQ-adapter-pipes-passthrough | Phase 3 | Pending |
| REQ-cli-init | Phase 3 | Pending |
| REQ-cli-stats | Phase 4 | Pending |
| REQ-cli-explain | Phase 4 | Pending |
| REQ-cli-doctor | Phase 4 | Pending |
| REQ-cli-surface-cap | Phase 4 | Pending |
| REQ-bundled-rules-tier1 | Phase 5 | Pending |
| REQ-bundled-rules-format | Phase 5 | Pending |
| REQ-acceptance-bundled-reduction | Phase 6 | Pending |
| REQ-acceptance-pnpm-end-to-end | Phase 6 | Pending |
| REQ-acceptance-cold-start-budget | Phase 6 | Pending |
| REQ-acceptance-explain-reproducibility | Phase 6 | Pending |
| REQ-acceptance-hot-reload | Phase 6 | Pending |
| REQ-acceptance-test-coverage | Phase 6 | Pending |
| REQ-docs-readme | Phase 6 | Pending |
| REQ-docs-worked-example | Phase 6 | Pending |
| REQ-docs-primitive-reference | Phase 6 | Pending |

**Coverage:**
- v1 requirements: 36 total
- Mapped to phases: 36
- Unmapped: 0 ✓

Note: `.planning/intel/SYNTHESIS.md` reports "26 distinct REQ-* IDs once de-duped" but the intel/requirements.md file contains 36 distinct REQ-* headings, all with unique kebab-case identifiers. The 36 figure is what's authoritative for this roadmap; the synthesis comment is treated as an INFO-level discrepancy with no impact on coverage (every REQ-ID in intel maps cleanly to one phase).

---
*Requirements defined: 2026-05-06*
*Last updated: 2026-05-06 after roadmap creation.*
