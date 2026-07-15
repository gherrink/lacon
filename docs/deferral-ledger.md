---
schema-version: 2
---

# Deferral Ledger

## Entries

### Additional assistant adapters  {#additional-assistant-adapters}

Deferred from v1 (Claude-Code-only). Candidates: a **Cursor adapter** (when their hook/extension API stabilizes); an **aider adapter** (more shell-friendly, possibly easier than Claude Code); a **generic shell wrapper** (opt-in PATH shim for assistants without hooks); and **editor-side adapters** (Continue, etc.). The core engine is already assistant-agnostic.

<!-- fields -->
- kind: Idea
- trigger: A non-Claude-Code assistant with a stable hook/extension API, or user demand

### Engine features  {#engine-features}

Per-line streaming Starlark (scripted line decisions, not just post-processing); filter inside pipes (`cmd1 | cmd2` as a whole); heredoc/subshell/eval parsing (currently passed through); granular TUI-in-chain bypass (wrap non-TUI segments, pass the interactive one — gated on tracking data showing the lost filtering is material); a user-overridable TUI list; multi-rule merging (probably a bad idea, worth the option to revisit); inline conditional pipeline stages; stage-level inheritance operations (insert/remove/replace vs append-only `extends`); and a persistent Starlark interpreter to amortize startup (crosses the daemon-less line, so needs real data).

<!-- fields -->
- kind: Idea
- trigger: User demand or benchmark data that justifies the added complexity

### Tracking: per-token accounting and analytics  {#tracking-per-token-accounting-and}

Per-token accounting to replace or supplement byte counters. Tokenizer tradeoff for whoever picks it up: **Anthropic's tokenizer** (most accurate for the Claude audience, but network round-trip or vendored, and Claude-specific), **tiktoken** (open, ballpark-accurate across families, `tiktoken-rs` exists), or the **`bytes / 4` heuristic** (wrong but free; trends still work). Record the tokenizer name + version per row. Also: session-aware aggregation ("this session cost X, saved Y"), cost estimation (tokens × pricing), and trend graphs.

<!-- fields -->
- kind: Idea
- trigger: Demand for token or cost reporting; the schema is already forward-compatible (byte-named counters, token columns appendable via the standard migration path)

### Sharing & discovery  {#sharing-discovery}

A public rule registry (`lacon install gh:user/repo`); sync between machines (git-backed user config, optional encrypted); and a suggestion engine that proposes rules based on tracked unmatched commands.

<!-- fields -->
- kind: Idea
- trigger: v1 in real use with a demonstrated rule-sharing need

### UI surfaces  {#ui-surfaces}

A web UI for stats (`lacon stats --serve`), a live TUI dashboard during a session, and a VS Code extension surfacing filter activity.

<!-- fields -->
- kind: Idea
- trigger: Demand for richer stats presentation than the CLI

### Platforms  {#platforms}

Native Windows (v1 is macOS + Linux + WSL only) and static musl builds for distroless containers.

<!-- fields -->
- kind: Idea
- trigger: Demand for native Windows or distroless-container support

### Programmatic embedding  {#programmatic-embedding}

A library API (the engine as a Rust crate / WASM module) and a plugin protocol (primitives in any language over stdio — slower than Starlark but more flexible).

<!-- fields -->
- kind: Idea
- trigger: Another tool wanting to embed the engine

### Quality-of-life  {#quality-of-life}

Rule hot-reload notifications; a filter dry-run mode in CI; user-facing fixture validation (`lacon validate <rule.yaml> --fixtures <dir>`); automated fixture drift detection; a rule profiler (per-stage timing); redaction patterns (deferred — regex redaction is best-effort, false negatives leak and false positives drop signal, and the implied "lacon redacts secrets" claim creates incident risk; v1 stance is off-by-default + 0700 + opt-in warning); a `lacon purge` command (`purge raw` / `--since` / `--project`, deferred to keep the CLI at six commands); and encryption at rest for `raw_outputs` (likely overkill given off-by-default + 0700 + local-only, revisit for shared-tenancy contexts).

<!-- fields -->
- kind: Idea
- trigger: User demand or a clear regret pattern from real usage
