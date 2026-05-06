# Roadmap: lacon

## Overview

Greenfield Rust project. v1 ships in six phases: build the streaming engine and the production wrapper that runs every filter (Phase 1), persist invocation history in SQLite with the privacy contract intact (Phase 2), wire the Claude Code adapter that rewrites bash commands at the `PreToolUse` boundary including chained-command splitting and TUI bypass (Phase 3), complete the introspection CLI surface backed by tracking data (Phase 4), ship the ten Tier 1 bundled rules with hermetic fixture tests (Phase 5), and pass the v1 ship gate by validating acceptance criteria end-to-end and writing the user-facing documentation (Phase 6). Phases compose strictly: each delivers a verifiable capability the next depends on, with no horizontal layering across the codebase.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3, 4, 5, 6): planned milestone work for v1.
- Decimal phases (e.g., 2.1): reserved for urgent insertions post-planning via `/gsd-insert-phase`. None at creation time.

- [ ] **Phase 1: Engine core & `lacon run` wrapper** - Streaming pipeline + Starlark `post_process` + rule loader + the production wrapper that runs every filter
- [ ] **Phase 2: Local tracking** - SQLite history at `~/.local/share/lacon/history.db` with privacy contract, retention, and the four required views
- [ ] **Phase 3: Claude Code adapter & `lacon init`** - `PreToolUse` hook with chained-command splitting, TUI bypass, bypass detection, and one-shot project setup
- [ ] **Phase 4: CLI completion (`stats`, `explain`, `doctor`)** - Introspection commands backed by tracking data plus the six-command surface cap
- [ ] **Phase 5: Bundled Tier 1 rules** - Ten YAML rules with success/failure fixtures and integration tests asserting ≥50% reduction with zero error-line drops
- [ ] **Phase 6: v1 ship gate — acceptance & docs** - End-to-end acceptance validation (cold start, hot reload, `pnpm` E2E, `explain` reproducibility, hermetic test coverage) plus README, worked example, and primitive reference

## Phase Details

### Phase 1: Engine core & `lacon run` wrapper
**Goal**: A `lacon` binary that, given a YAML rule, can spawn a subprocess, merge stderr into stdout, run the streaming pipeline (or `on_error` on non-zero exit), enforce the `max_bytes` cap, and write filtered output to its own stdout — everything downstream depends on this working.
**Depends on**: Nothing (first phase). Subsumes Rust workspace scaffolding (`crates/lacon-core`, `crates/lacon-cli`), `Cargo.toml` setup, dependency selection (`regex`, `clap`, `starlark-rust`), and config-loader work needed by `lacon validate`.
**Requirements**: REQ-engine-streaming-primitives, REQ-engine-starlark-postprocess, REQ-engine-rule-loading, REQ-engine-extends, REQ-engine-on-error, REQ-engine-rewrite, REQ-engine-bypass, REQ-engine-max-bytes-cap, REQ-cli-run, REQ-cli-validate
**Success Criteria** (what must be TRUE):
  1. Running `lacon run --rule <id> -- <cmd>` spawns the subprocess, captures merged stdout+stderr line-by-line, applies the rule's pipeline, writes filtered bytes to stdout, and exits with the wrapped subprocess's exit code unchanged.
  2. All ten native primitives (`strip_ansi`, `drop_regex`, `keep_regex`, `replace_regex`, `dedupe`, `collapse_repeated`, `keep_head`, `keep_tail`, `keep_around_match`, `max_bytes`) operate as line-by-line streaming transformers and are individually round-trippable through fixture-based unit tests.
  3. A rule's `on_error` block fully replaces the success pipeline (and optionally `post_process`) when the subprocess exits non-zero, with the success buffer discarded.
  4. `lacon validate <path>` accepts both rule files and `config.yaml` files, dispatches by content (`id`+`match` → rule), and rejects invalid regex / unknown primitive / circular `extends` / missing referenced Starlark file at load time without falling back to defaults.
  5. The `extends` directive prepends the parent's pipeline to the child's, inherits scalar fields the child omits, flattens single-level chains at load time, and rejects cycles.
**Plans**: 7 plans
- [x] 01-01-PLAN.md — Workspace scaffolding & cargo check green
- [x] 01-02-PLAN.md — Pipeline core: Stage enum + 10 native primitives + golden fixtures
- [x] 01-03-PLAN.md — Rule loader: schema + extends flatten + lacon validate dispatch + bundled embedding
- [x] 01-04-PLAN.md — Starlark post_process host (hermetic) + Pipeline integration
- [x] 01-05-PLAN.md — lacon run runtime: subprocess merge, dual-buffer, on_error swap, bypass, signal forwarding
- [ ] 01-06-PLAN.md — CLI surface: clap derive, lacon run + lacon validate wiring, 6-command cap
- [ ] 01-07-PLAN.md — End-to-end integration tests + cold-start probe + benchmark findings

**Implementation-time decisions to settle in this phase** (deferred-to-prototyping per `docs/open-questions.md`):
- **Q-deferred-signal-forwarding**: When Claude Code's Bash tool times out or the user interrupts, what does `lacon run` do? Likely answer: SIGTERM forward + immediate exit for v1, no drain. Settle the first time `lacon run` actually handles a signal in integration testing.
- **Q-deferred-merge-ordering**: stdout/stderr merge implementation guarantee. Likely answer: best-effort line atomicity, no cross-stream order guarantee. Document the chosen guarantee in `docs/architecture.md` once the implementation lands.

### Phase 2: Local tracking
**Goal**: Every `lacon run` invocation persists a row to a SQLite database at `~/.local/share/lacon/history.db` with the v1 privacy contract intact, the four required views queryable, and pruning happening at startup — without breaking the cold-start budget.
**Depends on**: Phase 1
**Requirements**: REQ-tracking-sqlite-location, REQ-tracking-schema, REQ-tracking-raw-outputs-default-off, REQ-tracking-privacy-warning, REQ-tracking-retention-defaults
**Success Criteria** (what must be TRUE):
  1. After running `lacon run -- <cmd>` once, the database file exists at `~/.local/share/lacon/history.db`, its parent directory has `0700` permissions, WAL mode is on, and a row exists in `invocations` with all expected columns populated (timestamps, byte counts, exit code, rule_id/source).
  2. With `store_raw_outputs: false` (default), no rows are written to `raw_outputs`. Flipping the project config to `store_raw_outputs: true` for the first time prints a one-time stderr privacy notice and writes a marker file in the project config dir suppressing future warnings.
  3. The four required views (`v_unmatched_offenders`, `v_filtered_offenders`, `v_bypass_rate`, `v_project_savings`) exist in the schema and return non-error result sets when queried via `sqlite3` after a session of `lacon run` invocations.
  4. Startup pruning deletes `invocations` rows older than 30 days, `raw_outputs` rows older than 3 days, and `suspected_regressions` rows older than 30 days. Append-only numbered migrations are applied at startup; a project `config.yaml` containing a `retention.*` key fails validation with an error pointing at `~/.config/lacon/config.yaml`.
**Plans**: TBD

### Phase 3: Claude Code adapter & `lacon init`
**Goal**: A user can run `lacon init` in a fresh project and have the Claude Code `PreToolUse` hook installed, the `.lacon/` skeleton created, and a CLAUDE.md instruction line added — and from then on every Bash tool invocation that matches a rule is rewritten to `lacon run --rule <id> -- <inner-cmd>` (or whole-chain bypassed when interactive or user-bypassed), reassembled with original operators preserved.
**Depends on**: Phase 1 (engine + `lacon run`), Phase 2 (tracking write-path active for adapter dogfooding)
**Requirements**: REQ-adapter-pretooluse-only, REQ-adapter-bypass-detection, REQ-adapter-chained-commands, REQ-adapter-tui-bypass, REQ-adapter-pipes-passthrough, REQ-cli-init
**Success Criteria** (what must be TRUE):
  1. A user running `lacon init` in a fresh project ends up with `.lacon/`, a `PreToolUse` hook entry in `.claude/settings.json`, a CLAUDE.md instruction line — and re-running `lacon init` in the same project is a safe no-op.
  2. The hook resolves rules per segment, applies `rewrite` (idempotent `add_flags`) to inner argv, and emits `hookSpecificOutput.updatedInput` with matched commands wrapped as `lacon run --rule <id> -- <seg>` while unchanged Bash tool fields (`description`, `timeout`, `run_in_background`) are echoed back unmodified.
  3. The chain splitter correctly handles all 13 scenarios in `docs/specs/chained-commands.md` — single command, two-segment chains per operator, mixed operators, mixed match/unmatched, subshell opacity, command-substitution opacity, quoted-string opacity, pipeline-as-segment, heredoc opacity, `!!` whole-chain bypass, `LACON_DISABLE=1` whole-chain bypass — verified by integration tests.
  4. The TUI heuristic (hardcoded list per `docs/specs/chained-commands.md`) runs per-segment AFTER chain splitting and BEFORE rule resolution; any matching segment causes the entire input to be returned unchanged. Pure-TUI basenames and the conditional patterns (`git rebase -i`, `git commit` w/o `-m`/`-F`, REPLs without positional args, etc.) are all covered by tests.
**Plans**: TBD

**Implementation-time decision to settle in this phase** (deferred-to-prototyping per `docs/open-questions.md`):
- **Q-deferred-init-idempotency**: What happens if `lacon init` runs in a project where the hook is already installed? Likely answer: detect existing block via marker comment (e.g. `// lacon:hook`), replace block contents in place, leave other settings.json keys alone — idempotent re-runs become a no-op when the block matches the current desired state. Settle during the first integration test pass for `lacon init`.

### Phase 4: CLI completion (`stats`, `explain`, `doctor`)
**Goal**: The remaining four CLI commands ship — `lacon stats` summarizes tracking data with filters, `lacon explain <id>` re-runs filtering against stored raw output and shows side-by-side diffs, `lacon doctor` verifies the install/config/rule health of the system — and the binary's command surface is hard-capped at six.
**Depends on**: Phase 2 (tracking data), Phase 3 (adapter + `lacon init`)
**Requirements**: REQ-cli-stats, REQ-cli-explain, REQ-cli-doctor, REQ-cli-surface-cap
**Success Criteria** (what must be TRUE):
  1. `lacon stats` prints top offenders, bypass rates, and unmatched commands derived from the four views, and accepts `--project`, `--since`, and `--rule` filters that narrow the output correctly.
  2. `lacon explain <id>` re-runs the rule's pipeline against the stored raw output for invocation `<id>` and renders a side-by-side diff between raw and filtered, exiting with a clear error message when raw retention was disabled at the time of the original invocation.
  3. `lacon doctor` reports a green status when hooks are installed, `config.yaml` files at every layer parse, every rule loads and validates, and the database directory permissions are `0700`. It surfaces a per-issue actionable error otherwise.
  4. Running `lacon <unknown-subcommand>` returns a non-zero exit code with a clap error pointing at the six legitimate subcommands; the binary has no `purge`, `install`, or `stats --serve` paths in its CLI surface.
**Plans**: TBD

### Phase 5: Bundled Tier 1 rules
**Goal**: Ten Tier 1 YAML rules ship in `bundled-rules/` (`pkg-install`, `cargo-build`, `cargo-test`, `vitest`, `jest`, `pytest`, `tsc`, `eslint`, `git-status`, `docker-build`), each with a success-path fixture and a failure-path fixture under `tests/fixtures/<rule-id>/<scenario>/` and an integration test asserting ≥50% reduction with zero error-line drops.
**Depends on**: Phase 1 (engine + primitives)
**Requirements**: REQ-bundled-rules-tier1, REQ-bundled-rules-format
**Success Criteria** (what must be TRUE):
  1. All ten Tier 1 rule files exist in `bundled-rules/` and load successfully via the resolver — each has a defined `match`, a non-empty `pipeline`, an `on_error` block where appropriate, and uses only the ten native primitives plus optional `post_process`.
  2. Each of the ten rules has at minimum one success-path fixture and one failure-path fixture under `tests/fixtures/<rule-id>/<scenario>/` with `input.txt`, `expected.txt`, and `meta.yaml` (`command`, `tool_version`, `captured_at`, `os`, `notes`).
  3. The fixture-based integration test (`cargo test --test bundled_rules`) walks the fixture tree, asserts byte-exact match of rule output against `expected.txt`, asserts `len(expected)/len(input) <= 0.5` for primary success-path fixtures (skippable via `exempt_from_reduction_check: true`), and asserts the opt-in `must_keep_lines` substring list when present — all without ever installing `pnpm`/`cargo`/`vitest`/etc. in CI.
**Plans**: TBD

### Phase 6: v1 ship gate — acceptance & docs
**Goal**: All v1 acceptance criteria pass end-to-end on macOS and Linux and the user-facing documentation set (README, worked example, primitive reference) ships — this is the gate at which v1 is shippable.
**Depends on**: Phases 1–5
**Requirements**: REQ-acceptance-bundled-reduction, REQ-acceptance-pnpm-end-to-end, REQ-acceptance-cold-start-budget, REQ-acceptance-explain-reproducibility, REQ-acceptance-hot-reload, REQ-acceptance-test-coverage, REQ-docs-readme, REQ-docs-worked-example, REQ-docs-primitive-reference
**Success Criteria** (what must be TRUE):
  1. Cold-start `lacon` invocation is benchmarked at <10ms on the hook hot path on both macOS and Linux, with a reproducible benchmark script checked into the repo.
  2. End-to-end acceptance test passes: `lacon init` followed by a `pnpm install` in a brand-new project produces a filtered tool result reaching the assistant with no manual config — and editing a rule file mid-session takes effect on the next invocation (hot reload, no daemon, no restart).
  3. `lacon explain <id>` reproducibly re-derives the filtered output for any tracked invocation that has stored raw output, byte-for-byte matching what was originally emitted to stdout.
  4. CI is hermetic — it never installs `pnpm`/`cargo`/`vitest`/etc. — and the test suite covers each native primitive, the chained-command splitter (13 scenarios), and every bundled rule via fixture-based integration tests.
  5. README (install + quickstart), worked example (writing a project-specific filter rule), and primitive reference (one example per primitive) ship in the repo and link from the project root.
**Plans**: TBD

## Progress

**Execution Order:**
Phases execute in numeric order: 1 → 2 → 3 → 4 → 5 → 6. Phase 5 (bundled rules) only requires Phase 1 strictly; it can run in parallel with Phases 2–4 if multi-stream work becomes useful, but the linear order is the default.

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Engine core & `lacon run` wrapper | 5/7 | In Progress|  |
| 2. Local tracking | 0/TBD | Not started | - |
| 3. Claude Code adapter & `lacon init` | 0/TBD | Not started | - |
| 4. CLI completion (`stats`, `explain`, `doctor`) | 0/TBD | Not started | - |
| 5. Bundled Tier 1 rules | 0/TBD | Not started | - |
| 6. v1 ship gate — acceptance & docs | 0/TBD | Not started | - |
