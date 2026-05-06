# Backlog

Things deliberately deferred from v1, grouped by category. This is not a promise to build any of them — it's a holding place for ideas worth revisiting once v1 is in real use.

## Adapters

- **Cursor adapter** — when their hook/extension API stabilizes
- **aider adapter** — they're more shell-friendly, possibly easier than Claude Code
- **Generic shell wrapper** — for assistants without hooks; opt-in PATH shim
- **Editor-side adapters** (Continue, etc.)

## Engine features

- **Per-line streaming Starlark** — for filters that genuinely need scripted line decisions, not just post-processing
- **Filter inside pipes** — track `cmd1 | cmd2`, filter the pipeline output as a whole
- **Heredoc / subshell / eval handling** — currently passes through; could be parsed
- **Granular TUI-in-chain bypass** — v1 bypasses the entire chain if any segment is interactive ([chained-commands](specs/chained-commands.md)). v2 candidate: wrap non-TUI segments and pass only the interactive segment through. Gated on tracking data showing the lost filtering opportunity is material.
- **Multi-rule merging** — when more than one rule could apply, merge stages instead of first-match-wins. Probably a bad idea, but worth the option to revisit.
- **Conditional pipeline stages** — `if exit_code == 0: keep_tail 5; else: ...` inline rather than via the `on_error` block
- **Stage-level inheritance operations** — insert/remove/replace specific stages from a parent rule (rather than append-only `extends`)
- **Persistent Starlark interpreter / helper process** — amortize Starlark startup across invocations for rules that use `post_process`. v1 accepts cold-start cost in-process; revisit in v2 if benchmarks show the per-rule tax is material. Crosses the daemon-less line, so requires real data to justify.

## Tracking

- **Per-token accounting** — replace or supplement byte counters with token counts. Useful but not blocking. Schema is already forward-compatible (existing counters are explicitly byte-named, so token columns can be appended via the standard migration path). Tokenizer tradeoff for whoever picks this up:
  - **Anthropic's tokenizer for Claude** — most accurate for the v1 audience. Available via the Messages API `count_tokens` endpoint (online) and via Anthropic's open tokenizer packages (vendorable). Cost is either a network round-trip per invocation (rules out the hot path) or a vendored implementation; either way it's Claude-specific.
  - **tiktoken** — open, well-maintained, ballpark-accurate across many model families. Rust bindings exist (`tiktoken-rs`). Wrong for Claude in the strict sense, but consistent enough for trends.
  - **Heuristic (`bytes / 4`)** — wrong but free and zero-dependency. Trends still work because the error is roughly constant per language.
  Whichever wins, record the tokenizer name + version per row so different rows can use different tokenizers as adapters expand.
- **Session-aware aggregation** — group invocations by Claude Code session so users can see "this session cost X tokens, of which Y were saved"
- **Cost estimation** — multiply tokens by current model pricing for a dollar figure
- **Trend graphs** — token spend over time, per project, per rule

## Sharing & discovery

- **Public rule registry** — `lacon install gh:user/repo` to pull a rule pack
- **Sync between machines** — Git-backed user config, optional encrypted sync
- **Suggestion engine** — based on tracked unmatched commands, suggest "you might want a rule for X"

## UI

- **Web UI for stats** — `lacon stats --serve`
- **TUI dashboard** — live view during a Claude Code session
- **VS Code extension** — surface filter activity in the editor

## Platforms

- **Native Windows** — v1 is macOS + Linux + WSL only
- **Static musl builds** for distroless containers

## Programmatic

- **Library API** — expose the engine as a Rust crate / WASM module so other tools can embed it
- **Plugin protocol** — primitives written in any language, communicating over stdio (slower than Starlark but more flexible)

## Quality-of-life

- **Rule hot-reload notifications** — toast or log line when a rule file is reloaded
- **Filter dry-run mode in CI** — run rules against fixture output, fail if regressions
- **User-facing fixture validation** — `lacon validate <rule.yaml> --fixtures <dir>` for users testing their own project rules. v1 covers bundled rules via Rust integration tests (see [testing-rules](testing-rules.md)); user-facing validation extends the same pattern to project-defined rules.
- **Automated fixture drift detection** — periodic CI job that re-captures all bundled-rule fixtures and opens an issue when committed output diverges from current tool output. v1 relies on developer awareness plus user issue reports.
- **Rule profiler** — measure per-stage time on real output, surface slow stages
- **Redaction patterns** — auto-strip lines matching common secret patterns before storing raw output. Deferred from v1 because regex-based redaction is best-effort: false negatives leak secrets through and false positives drop legitimate output (e.g. PAT-shaped strings in `git log`). The "lacon redacts secrets" claim it implies is the kind of feature description that creates downstream incident reports. v1 stance: off-by-default + 0700 + opt-in warning is the contract; users who opt in own what they store. Revisit when there's user demand and a clear regret pattern from real usage.
- **`lacon purge` command** — `lacon purge raw` (all raw outputs), `lacon purge --since=<date>`, `lacon purge --project=<path>`. Deferred from v1 to keep the CLI surface at six commands. v1 cleanup path is `rm ~/.local/share/lacon/history.db` or direct `sqlite3` DELETE. Promote when adding it doesn't crowd more important commands.
- **Encryption at rest for `raw_outputs`** — likely overkill given off-by-default + 0700 + local-only, but worth revisiting if `lacon` ever runs in shared-tenancy contexts (devcontainers, pair-programming setups, etc.).
