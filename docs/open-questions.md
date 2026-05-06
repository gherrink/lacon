# Open questions

Things we don't yet know that could change the design. Each one needs an answer (or a "we accept the unknown") before the relevant part of v1 ships.

## Claude Code hook mechanics — resolved (ADR 0013)

The load-bearing question — *can a hook modify output before the model sees it?* — was resolved by an empirical probe against live Claude Code on 2026-05-05.

**Findings (verified against `code.claude.com/docs/en/hooks` and the probe):**

- `PreToolUse` rewrites commands via `hookSpecificOutput.updatedInput` (replaces the entire input object — unchanged fields must be echoed back).
- `PostToolUseFailure` is a real, distinct event from `PostToolUse`.
- Bash `tool_response` is structured: `{stdout, stderr, interrupted, isImage}`.
- Hook output is capped at 10,000 characters — anything larger is elided to a file and replaced with a preview + path.
- Hook config lives where expected: `.claude/settings.json` (project) and `~/.claude/settings.json` (user). Default hook timeout is 600 s.
- `additionalContext` is delivered to the model as a `<system-reminder>` appended **after** the raw tool output. Additive, not replacement.

**The blocker:** `PostToolUse` **cannot** replace tool output. There is no `updatedToolOutput` field; the probe confirmed that returning one has no effect, and the model receives the raw stdout regardless.

**Resolution:** [ADR 0013](decisions/0013-filter-via-pretooluse-wrapper.md). v1 filters output via a `PreToolUse`-rewritten command that wraps the original in `lacon run --rule <id> -- <cmd>`, so filtering happens inside the subprocess wrapper before Claude Code captures the tool result. The streaming pipeline, rule schema, primitives, and Starlark stage are unchanged — only their execution location moves from "hook responder" to "subprocess wrapper."

`additionalContext` is reserved for v1.5: annotation of unmatched commands ("lacon could have stripped ~3 kB if it had a rule for this").

## Starlark performance at hook scale — resolved (2026-05-06)

Starlark startup overhead is small (<5ms) but it gets invoked on every command Claude Code runs that hits a rule with `post_process`. In a busy session that could be hundreds of times. The original question had two parts: *will it actually be a problem?* (unanswerable without a prototype to benchmark) and *if yes, daemon or accept?* (answerable now, on architectural grounds).

**Resolution:** No daemon in v1, regardless of benchmark outcome. Reasoning:

- Daemon-less is a load-bearing property of the design, not a preference. Re-introducing lifecycle, IPC, and rule-reload concerns to amortize a cost we haven't measured is the wrong trade.
- `post_process` is opt-in per rule. A rule author using it is choosing a heavier primitive; that's a fair cost to expose to rule authors, not something to hide behind a daemon.
- `post_process` runs once on aggregated output (ADR 0008), not per line. Worst-case multiplier is "matched commands per session," not "lines × commands."
- If cold-start turns out to be slow, in-process levers remain available: lazy interpreter init (only when a matched rule has `post_process`), bytecode caching, scoping `post_process` capabilities. Benchmarking is a post-prototype tuning task, not a design blocker.

If v2 benchmarks show in-process optimization isn't enough, a persistent helper process can be reconsidered then — with real data driving the daemon-vs-no-daemon trade.

## Chained command behavior — resolved (2026-05-06)

Full semantics now live in [chained-commands](specs/chained-commands.md). Summary of resolutions:

- **"Second command depends on first command's output"** — non-issue. `lacon run` propagates exit codes unchanged and only filters its own stdout (what Claude Code captures). The shell-level data flow between segments is untouched. Filtering changes what the model sees, not what the next command sees.
- **Per-segment rule semantics** — each segment is resolved independently with first-match-wins and project > user > bundled precedence. No merging, no cross-segment effects. Matched segments are wrapped as `lacon run --rule <id> -- <segment>`, unmatched segments pass through, and the original operators are preserved.
- **TUI-in-chain (v1)** — if any segment matches the TUI heuristic, the **entire chain** is bypassed. Conservative by design; granular per-segment bypass is a [backlog](backlog.md) v2 candidate gated on tracking data showing the lost filtering opportunity is material.

User-driven bypass (`!!`, `LACON_DISABLE=1`) remains whole-command. The splitting boundary (top-level operators only — quotes, subshells, command substitution, heredocs are opaque) is captured in the spec along with the test obligations for the splitter.

## What lives outside hooks — resolved (2026-05-06)

Boundary documented in [v1-scope → Coverage boundary](v1-scope.md#coverage-boundary). The original concerns sort into three categories:

- **Fundamental limitation:** subprocesses from non-Bash tools or MCP servers don't flow through `PreToolUse(Bash)`, so `lacon` can't see them.
- **By design:** redirected output (to files, backgrounded processes, `/dev/tty`) is invisible to both `lacon` and the model — there's nothing to filter because the model isn't seeing it either.
- **Out of scope:** user's own terminal sessions.

Long-running watchers and ANSI/control-sequence output were partially misframed in the original list. Foreground watcher output is filtered up to the tool timeout like any other command; backgrounded output never reaches the model. ANSI escapes that flow through stdout/stderr are filterable via `strip_ansi` — not a coverage gap. README copy can lift from the v1-scope section when written.

## Tokenizer choice — resolved-as-deferred (2026-05-06)

The schema impact concern is settled: existing tracking columns are explicitly byte-named (`raw_stdout_bytes`, `raw_stderr_bytes`, `filtered_bytes`), so adding token columns later is a normal append-only migration ([ADR 0011](decisions/0011-sqlite-for-tracking.md)) with no v1 work required.

The tokenizer choice itself is a v2 design decision and lives under [backlog → Per-token accounting](backlog.md), with the three-option tradeoff (Anthropic's tokenizer, tiktoken, heuristic) captured there for whoever picks it up. One factual update from the original framing: Anthropic's tokenizer is no longer closed — it's reachable via the Messages API `count_tokens` endpoint and via vendorable open packages, so the v2 trade is more "online API vs. vendored vs. heuristic" than "closed vs. open."

## Privacy and `raw_outputs` — resolved (2026-05-06)

v1 contract is now documented in [tracking-data-model → Privacy](specs/tracking-data-model.md#privacy):

- **Off by default + `0700` + opt-in stderr warning** on the first off → on transition. That's the v1 protection.
- **No automatic redaction.** Best-effort regex stripping creates false-confidence risk (false negatives leak, false positives drop legitimate output) and would imply a "lacon redacts secrets" feature claim we can't honor. Deferred to [backlog](backlog.md) gated on real user-regret signal.
- **No `lacon purge` command in v1.** Users clear retained data via `rm` on the DB file or direct `sqlite3 DELETE`. Adding `purge` would push the v1 CLI past its 6-command boundary; deferred to [backlog](backlog.md).
- **Encryption at rest** — already backlog material. v1 stance unchanged.

A side-effect of this resolution: the tracking spec previously documented `lacon purge` subcommands as if they shipped in v1, contradicting `v1-scope.md`. The spec has been corrected to match the 6-command v1 surface and the manual cleanup path.

## Testing strategy for rules — resolved (2026-05-06)

Strategy now lives in [testing-rules](testing-rules.md). Summary:

- **Fixture-based, hermetic CI.** Each bundled rule has captured `input.txt` / `expected.txt` / `meta.yaml` triples under `bundled-rules/<rule-id>/fixtures/<scenario>/`. A single Rust integration test walks the tree and asserts byte-exact rule output against expectations. CI never installs `pnpm`, `cargo`, etc.
- **Per-fixture assertions:** byte-exact output match, ≥50% reduction (skippable for edge-case fixtures via `meta.yaml` flag), and an opt-in `must_keep_lines` list for explicitly preserving error/warning substrings.
- **Regeneration is a developer-local manual step**, helped by `scripts/capture-fixtures.sh`. Procedure documented in the new doc. Periodic re-capture is on the developer, not CI.
- **Deferred to [backlog](backlog.md):** user-facing `lacon validate --fixtures` for project rules, and automated CI drift detection.
