# Context (synthesized from DOC-class supporting files)

Five DOC-class supporting files are in the ingest set:

- `docs/architecture.md` — high-level architectural explainer
- `docs/backlog.md` — v1-deferred ideas
- `docs/bundled-rules-roadmap.md` — Tier 1 v1 rules + Tier 2 backlog + deliberate exclusions
- `docs/open-questions.md` — design-risk log (open / deferred / resolved)
- `docs/testing-rules.md` — fixture-based hermetic CI strategy

DOC content is NOT decisional — it explains, expands, and tracks. Anything in a DOC that contradicts an ADR is automatically subordinate. Everything ingested below is consistent with the LOCKED ADR set as of 2026-05-06.

---

## Topic: System architecture (verbatim notes)

**source:** docs/architecture.md

- Updated 2026-05-05 per ADR 0013. Filtering happens inside a subprocess wrapper (`lacon run`) invoked by a `PreToolUse`-rewritten command. Original "hook responds with filtered bytes" flow was abandoned after empirical testing showed `PostToolUse` cannot replace tool output.
- Internal pipeline, primitives, Starlark stage, tracker, and rule schema unchanged — only execution location moved.
- Component contract:
  - **Adapter** (Claude Code-specific): handles hook contract translation. Resolves rule, applies rewrite block, wraps matched commands as `lacon run --rule <id> -- <inner-cmd>` via `hookSpecificOutput.updatedInput`. `updatedInput` REPLACES the entire input object — unchanged fields (`description`, `timeout`, `run_in_background`) must be echoed back.
  - **Rule resolver** (core): caches compiled regexes; invalidates on rule file mtime change.
  - **Pipeline runner** (core): streams stdout/stderr line-by-line through native primitives. Maintains a bounded ring buffer for `keep_tail`. On non-zero exit, swaps in `on_error` pipeline.
  - **Tracker** (core): cheap synchronous SQLite write on the hot path (single INSERT). Optional `raw_outputs` storage off by default.
  - **Wrapper** (`lacon run`): spawns subprocess, reads merged stdout+stderr line-by-line, runs pipeline, writes filtered bytes to its own stdout, writes tracking row, exits with subprocess's exit code.
- Repo file layout:
  ```
  lacon/
  ├── README.md
  ├── docs/
  ├── crates/
  │   ├── lacon-core/
  │   ├── lacon-cli/
  │   └── lacon-adapter-claudecode/
  ├── bundled-rules/
  └── tests/
      ├── fixtures/
      └── integration/
  ```
- Lifecycle of an invocation (worked example with `pnpm install --frozen-lockfile`) is enumerated step-by-step. Hook fires → adapter rewrites → shell exec → `lacon run` spawns subprocess → pipeline runs → exit code dispatch (success vs `on_error`) → tracking write → filtered bytes flushed to stdout → Claude Code captures.

---

## Topic: v1-deferred backlog (verbatim categories)

**source:** docs/backlog.md

This is a holding place, NOT a build promise.

- **Adapters:** Cursor; aider; generic shell wrapper (opt-in PATH shim); editor-side adapters (Continue, etc.).
- **Engine features:** per-line streaming Starlark; filter inside pipes; heredoc/subshell/eval handling; granular TUI-in-chain bypass (gated on tracking data); user-overridable TUI list (`~/.config/lacon/tui-commands.yaml` or similar) — deferred until clear false-positive pattern; multi-rule merging (probably bad idea, kept for option); conditional pipeline stages inline; stage-level inheritance operations; persistent Starlark interpreter / helper process (gated on benchmark data).
- **Tracking:** per-token accounting (Anthropic tokenizer vs tiktoken vs heuristic — schema is forward-compatible via append-only migration); session-aware aggregation; cost estimation; trend graphs.
- **Sharing & discovery:** public rule registry (`lacon install gh:user/repo`); cross-machine sync; suggestion engine.
- **UI:** web UI (`lacon stats --serve`); TUI dashboard; VS Code extension.
- **Platforms:** native Windows; static musl builds for distroless containers.
- **Programmatic:** library API (Rust crate / WASM); plugin protocol over stdio.
- **Quality-of-life:** rule hot-reload notifications; filter dry-run mode in CI; user-facing fixture validation (`lacon validate <rule.yaml> --fixtures <dir>`); automated fixture drift detection; rule profiler; redaction patterns (deferred for false-confidence risk); `lacon purge` command (deferred to keep CLI surface at six commands); encryption at rest for `raw_outputs`.

---

## Topic: Bundled rules roadmap

**source:** docs/bundled-rules-roadmap.md

### Tier 1 — v1 must-have (10 rules)

| Rule ID | Targets | Notes |
|---|---|---|
| `pkg-install` | `npm/pnpm/yarn install`, `pnpm i`, `pnpm add` | Add `--reporter=silent` where supported, drop deprecation warnings, collapse progress |
| `cargo-build` | `cargo build`, `cargo check` | Drop "Compiling foo v0.x" repeats; preserve warnings/errors with file:line |
| `cargo-test` | `cargo test` | Preserve test summary line; drop per-test PASS lines; preserve FAIL with full output |
| `vitest` | `vitest`, `vitest run`, `pnpm test` | Same shape as cargo-test |
| `jest` | `jest`, `npx jest` | Watch mode quirks |
| `pytest` | `pytest`, `python -m pytest` | Drop dot-progress; preserve failure tracebacks |
| `tsc` | `tsc`, `tsc --noEmit` | Most output IS the signal; ANSI strip + dedupe + tail |
| `eslint` | `eslint`, `pnpm lint` | Drop "passing" summaries; preserve warnings/errors with file:line |
| `git-status` | `git status` | Collapse "Untracked files" sections in monorepos |
| `docker-build` | `docker build`, `docker buildx build` | Drop layer cache hits; preserve actual build steps and errors |

### Tier 2 — post-v1

`webpack`, `vite`, `turbopack`; `next-build`, `remix-build`, `astro-build`; `make`, `cmake`, `ninja`; `mvn`, `gradle`; `composer install`; `pip install`, `poetry install`, `uv pip install`; `bundle install`; `terraform plan/apply`; `kubectl apply/logs`; `git log`, `git diff` (large output); `find`, `rg` (huge output).

### Deliberately not on roadmap

- Interactive commands (handled by TUI bypass heuristic).
- Editor invocations (out of scope).
- Pager-driven (already self-filtering).

### Format expectations

Every rule lands with: YAML rule file in `bundled-rules/`, fixture set under `tests/fixtures/<rule-id>/<scenario>/` (`input.txt`, `expected.txt`, `meta.yaml`) — at minimum one success-path + one failure-path scenario, integration test asserting reduction ratio + zero error-line drops, doc note in roadmap.

---

## Topic: Open questions log (status-preserved)

**source:** docs/open-questions.md

Three sections: **Open**, **Deferred to prototyping**, **Resolved**. Status preserved verbatim for the roadmapper.

### Open

*None currently.* (verbatim from source as of 2026-05-06)

### Deferred to prototyping (have likely-answers, not commitments)

#### Q-deferred-signal-forwarding

- **status:** deferred-to-prototyping
- **question:** When Claude Code's Bash tool times out (default 2 min) or the user interrupts, what does `lacon run` do? Forward SIGINT/SIGTERM and drain pipeline for partial result, or just die?
- **likely answer:** SIGTERM forward + immediate exit for v1, no drain. Revisit if user reports indicate partial-results-on-timeout is meaningful in practice.

#### Q-deferred-init-idempotency

- **status:** deferred-to-prototyping
- **question:** What happens if `lacon init` runs in a project where the hook is already installed in `.claude/settings.json`? Overwrite, detect-and-skip, append? Same on upgrades?
- **likely answer:** Detect existing block via marker comment (e.g. `// lacon:hook`), replace block contents in place, leave other settings.json keys alone. Idempotent re-runs become a no-op when the block matches the current desired state. Settle during the first integration test pass.

#### Q-deferred-merge-ordering

- **status:** deferred-to-prototyping
- **question:** ADR 0013 says ordering "may differ from raw terminal interleaving" without specifying the implementation guarantee. POSIX line-buffered merge has known race conditions; lossless merging with strict line atomicity requires either a pty or careful select/epoll bookkeeping.
- **likely answer:** "Best-effort line atomicity, no cross-stream order guarantee" once the implementation chooses an approach. Most rules don't depend on cross-stream order — they filter by content. Document the guarantee in `architecture.md` or `chained-commands.md` once chosen.

### Resolved (rationale log; surfaced as INFO in conflict report)

#### R-resolved-claude-code-hook-mechanics

- **status:** resolved by ADR 0013
- **finding:** Empirical probe 2026-05-05 verified: `PostToolUse` cannot replace tool output; there is no `updatedToolOutput` field. `additionalContext` is delivered AFTER raw output as a `<system-reminder>` — additive, not replacement. `PreToolUse` rewrites via `hookSpecificOutput.updatedInput` (replaces entire input object). `PostToolUseFailure` is a real, distinct event. Bash `tool_response` is structured `{stdout, stderr, interrupted, isImage}`. Hook output capped at 10,000 chars. Hook config in `.claude/settings.json` (project) and `~/.claude/settings.json` (user). Default hook timeout 600s.
- **resolution:** ADR 0013 — `PreToolUse`-rewritten subprocess wrapper. `additionalContext` reserved for v1.5 unmatched-command annotation.

#### R-resolved-starlark-performance

- **status:** resolved 2026-05-06
- **resolution:** No daemon in v1, regardless of benchmark outcome. Daemon-less is a load-bearing property, not a preference. `post_process` is opt-in per rule; the cost is fairly exposed to rule authors. ADR 0008 keeps it on aggregated output (worst-case multiplier is matched-commands-per-session, not lines × commands). In-process levers remain (lazy interpreter init, bytecode caching). Benchmarking is post-prototype tuning. Persistent helper process reconsidered in v2 only if real data justifies.

#### R-resolved-chained-command-behavior

- **status:** resolved 2026-05-06 → spec at docs/specs/chained-commands.md
- **summary:** "Second command depends on first command's output" is a non-issue (lacon only filters its own stdout, exit codes propagate unchanged). Per-segment rule resolution with first-match-wins + project>user>bundled. TUI-in-chain bypasses the entire chain (v1 conservative); granular per-segment bypass is backlog. User-driven bypass remains whole-command. Splitting boundary: top-level operators only (quotes/subshells/command-substitution/heredocs are opaque).

#### R-resolved-coverage-boundary

- **status:** resolved 2026-05-06 → docs/v1-scope.md#coverage-boundary
- **summary:** Three categories — fundamental limitation (non-Bash MCP subprocesses invisible); by design (redirected/backgrounded/`/dev/tty` invisible to model and lacon both); out of scope (user's own terminal sessions). ANSI-via-stdout is filterable via `strip_ansi` (not a coverage gap).

#### R-resolved-tokenizer-choice

- **status:** resolved-as-deferred 2026-05-06
- **summary:** Existing tracking columns are explicitly byte-named, so token columns can be appended via standard append-only migration (per ADR 0011). Tokenizer choice itself is v2 design; lives in backlog → Per-token accounting. One factual update: Anthropic's tokenizer is no longer closed — reachable via Messages API `count_tokens` and via vendorable open packages. Trade is "online API vs vendored vs heuristic."

#### R-resolved-privacy-raw-outputs

- **status:** resolved 2026-05-06 → docs/specs/tracking-data-model.md#privacy
- **summary:** v1 contract = off by default + `0700` + opt-in stderr warning on first off→on transition. No automatic redaction (best-effort regex creates false-confidence risk). No `lacon purge` command in v1 (would push CLI past 6-command boundary). Encryption at rest is backlog material. **Side-effect:** spec previously documented `lacon purge` subcommands as if they shipped in v1; spec was corrected to match the 6-command surface.

#### R-resolved-config-schema

- **status:** resolved 2026-05-06 → docs/specs/config-schema.md
- **summary:** Five effective keys. `retention.invocations_days` (30) and `retention.raw_outputs_days` (3) are user-only. `defaults.max_bytes` (32768) and `store_raw_outputs` (false) are project-or-user. Per-key deep merge. Project file using user-only key fails validation with clear error. Skipped from v1: `hook.timeout_seconds` (Claude Code 600s default hardcoded), `database_path` (no override), log-level/verbosity (auto-detect).

#### R-resolved-tui-heuristic

- **status:** resolved 2026-05-06 → docs/specs/chained-commands.md#interactive-tui-commands--v1
- **summary:** `is_tui(command, args) -> bool` in adapter, called per-segment AFTER chain splitting and BEFORE rule resolution. Whole-input bypass on any match. Hardcoded list lives in adapter code. User-overridable list deferred to backlog.

#### R-resolved-rule-testing-strategy

- **status:** resolved 2026-05-06 → docs/testing-rules.md
- **summary:** Fixture-based hermetic CI. `bundled-rules/<rule-id>/fixtures/<scenario>/` with `input.txt`/`expected.txt`/`meta.yaml`. Per-fixture assertions: byte-exact output match; ≥50% reduction (skippable via `meta.yaml` flag for edge cases); opt-in `must_keep_lines` substring list. Regeneration is developer-local manual step (`scripts/capture-fixtures.sh` helper). Periodic re-capture is on the developer, not CI. **Deferred:** user-facing `lacon validate --fixtures` for project rules; automated CI drift detection.

---

## Topic: Testing strategy (verbatim notes)

**source:** docs/testing-rules.md

- **Strategy:** fixture-based, hermetic CI. Captured representative output checked into repo. Test runner asserts rule pipeline transforms captured input into expected output, byte-for-byte. CI never installs `pnpm`, `cargo`, `vitest`, etc.
- **Trade-offs accepted:**
  - Drift over time — captured output is a snapshot from one tool version on one machine; fixtures must be regenerated periodically.
  - Coverage bounded by fixture set — mitigated by capturing multiple scenarios per rule (success, failure, edge cases).
- **Rejected:** live capture in CI (slow, fragile); pure synthetic fixtures (cleaner-than-reality).
- **Layout:** `bundled-rules/<rule-id>.yaml` + `tests/fixtures/<rule-id>/<scenario>/{input.txt,expected.txt,meta.yaml}`. Note: this layout differs from `bundled-rules-roadmap.md` Tier 1 doc which references `tests/fixtures/<rule-id>/<scenario>/` paths but co-locates fixtures with rules in the embedded path narrative — see Topic: layout reconciliation.
- **`meta.yaml`:** keys are `command`, `tool_version`, `captured_at`, `os`, `notes`. `os` is informational, not a test selector.
- **Per-fixture assertions:** (1) byte-exact match of rule output against `expected.txt` (no whitespace leniency); (2) reduction threshold `len(expected)/len(input) <= 0.5` for primary success-path fixtures (skippable via `exempt_from_reduction_check: true`); (3) `must_keep_lines` substring list check (opt-in).
- **Runner:** single Rust integration test file walks `tests/fixtures/` tree. `cargo test --test bundled_rules`. `insta` is fine for snapshot ergonomics; plain `assert_eq!` works too.
- **Regeneration workflow:** capture stdout+stderr merged → re-run rule → update `meta.yaml` → verify test passes → commit fixture + rule changes together. Helper script `scripts/capture-fixtures.sh` for batch re-capture.
- **Out of scope for v1:** user-facing fixture validation; automated drift detection. Both backlog.

---

## Topic: Layout reconciliation note (DOC-only, INFO-grade)

`docs/architecture.md` says fixtures live under `tests/fixtures/`. `docs/bundled-rules-roadmap.md` says fixtures live under `tests/fixtures/<rule-id>/<scenario>/`. `docs/testing-rules.md` shows the layout as `tests/fixtures/<rule-id>/<scenario>/` with rule files separately at `bundled-rules/<rule-id>.yaml` (rule and fixtures NOT co-located). The three are consistent — fixtures in `tests/fixtures/`, rules in `bundled-rules/`. No conflict.
