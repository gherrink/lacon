# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project status

**v1.0 is code-complete.** The Rust workspace ships across six phases (engine, SQLite tracking, Claude Code adapter, CLI, ten bundled rules, ship gate). The design is locked down by 14 ADRs in `docs/decisions/`. Treat those ADRs as the source of truth — if a proposed change contradicts one, surface that explicitly rather than silently working around it.

## Workspace layout

A Cargo workspace (`resolver = "2"`, stable toolchain pinned in `rust-toolchain.toml`) with these members:

- **`crates/lacon-core`** — the assistant-agnostic engine (library). `pipeline/` (streaming line primitives + `max_bytes` cap), `rules/` (loader with mtime cache, YAML `schema`, `bundled` rust-embed, `rewrite` flag add/remove, `extends` flatten), `runtime/` (subprocess spawn, stderr→stdout merge, POSIX signal forwarding, `on_error` mode switch), `starlark_host/` (the aggregated `post_process` stage), `tracking/` (SQLite WAL: migrations, prune, privacy, normalize, query, health), `config/`, `validate/`.
- **`crates/lacon-cli`** — the `lacon` binary. One module per subcommand under `commands/` (`init`, `run`, `stats`, `explain`, `doctor`, `validate`).
- **`crates/lacon-adapter-claudecode`** — the `lacon-claude-hook` binary + library. `protocol.rs` (PreToolUse JSON), `chain.rs` (`&&`/`||`/`;` splitting), `tui.rs` (interactive-bypass heuristic), `quote.rs`.
- **`bin/test_emitter`** — a test-only stub binary that parametrically reproduces tool output, replacing real tools (cargo, pnpm, etc.) in hermetic tests.
- **`benches/`** — the `cold_start_probe` wall-clock harness (`cold_start.rs`). The deterministic hard gate lives separately at `crates/lacon-core/benches/tracker_open.rs` (criterion; panics if `Tracker::open` median > 3700µs).
- **`bundled-rules/`** — the ten shipped Tier-1 rules (YAML), embedded into `lacon-core` at build time. **`tests/fixtures/`** — input/expected/meta triples driving fixture tests.

## Build, test, and lint

```sh
cargo build --release        # produces target/release/{lacon, lacon-claude-hook}
cargo test --workspace       # default (non-ignored) suite — what CI gates on
cargo clippy --workspace --all-targets
cargo fmt                    # rustfmt + clippy components are in the pinned toolchain
```

**Before running the test suite from a fresh checkout, build the workspace in debug first:**

```sh
cargo build --workspace && cargo test --workspace
```

This is non-obvious and load-bearing: the `lacon-cli` integration tests resolve cross-package helper binaries (`test_emitter`, `lacon-claude-hook`) via `assert_cmd`'s `cargo_bin`, which falls back to `target/debug/<name>`. `cargo test` builds only the test harnesses, not the top-level debug bins, so a bare `cargo test` on a clean tree panics on unresolved binaries. CI runs the debug build explicitly before the test sweep (`.github/workflows/ci.yml`).

Running a subset:

```sh
cargo test -p lacon-core --test primitives          # one integration-test file
cargo test -p lacon-core dedupe                      # tests matching a substring
```

**Ignored tests** are excluded from the default set (and CI) on purpose — the `#[ignore]` string is the runbook line:

```sh
cargo test --test runtime_signal -- --include-ignored          # sends real SIGTERM
cargo test -p lacon-cli --test pnpm_e2e -- --ignored           # runs a real pnpm install (not hermetic)
```

**Benchmarks / cold-start gates:**

```sh
cargo bench -p lacon-core --bench tracker_open   # deterministic HARD gate
./scripts/bench-cold-start.sh                    # SOFT-reported wall-clock min-of-N
```

CI (`.github/workflows/ci.yml`) is **hermetic by contract** — two OS lanes (ubuntu + macos) using each runner's pre-installed Rust, no toolchain/package-manager installs, no secrets. Do not add fetch steps (Homebrew, npm/pnpm, pip, apt); `rusqlite[bundled]` vendors SQLite so there is no system-library temptation.

## What `lacon` is

A Rust CLI that integrates with coding-assistant hook systems (Claude Code first) to filter and rewrite bash command output before it enters the model's context window. Goal: 30–70% byte reduction on common commands without dropping signal. Local-only, no LLM calls, no network.

The big picture in `docs/architecture.md`:

- **Adapter** (per assistant) → **`lacon run` wrapper** → **Rule resolver** → **Pipeline runner** (streaming) → **Tracker** (SQLite). The core engine is assistant-agnostic; adapters are dumb translators that rewrite commands.
- The Claude Code `PreToolUse` hook does both jobs: applies the rule's `rewrite` block (flag add/remove) and, for matched commands, wraps the result as `lacon run --rule <id> -- <cmd>`. Filtering happens inside `lacon run`, which spawns the subprocess, merges stderr into stdout, and writes filtered bytes to its own stdout — that's what Claude Code captures as the tool result. There is **no `PostToolUse` hook** in v1: empirical testing on 2026-05-05 showed `PostToolUse` cannot replace tool output (only `additionalContext` reaches the model, additively). See [ADR 0013](docs/decisions/0013-filter-via-pretooluse-wrapper.md).
- `on_error` *replaces* the success pipeline on non-zero exit; it does not merge. Implemented as an internal mode of `lacon run`, switched on the subprocess's observed exit code.

## Load-bearing design constraints

These come from ADRs and need to hold across any implementation work:

- **Streaming, not buffered** (ADR 0005). Native primitives are line-by-line transformers. Memory is bounded by the largest stateful primitive (typically `keep_tail N`) plus the `max_bytes` final cap. Primitives that need global reordering (e.g. sort) are out of scope. The Starlark `post_process` stage is the only deliberate exception — it runs on aggregated output (ADR 0008) because per-line Starlark would dominate runtime at typical volumes.
- **Cold start under 10ms** on the hook hot path. The binary is invoked thousands of times per session. Anything that imposes startup cost (lazy_static blowups, large embedded data, eager rule compilation) needs to justify itself against this budget.
- **First-match-wins resolution, project > user > bundled** (ADRs 0004, 0007). No merging across rules or layers. Layering is explicit only via `extends`, which *prepends* the parent's pipeline and inherits scalar fields the child doesn't define (ADR 0012). No insert/remove/reorder operations on inherited stages — if you need that, copy the parent.
- **SQLite with WAL mode** at `~/.local/share/lacon/history.db` (ADR 0011). Two tables: `invocations` (metadata, 30-day default retention) and `raw_outputs` (bulky stdout/stderr blobs, 3-day default retention, **off by default** per ADR 0009). Pruning runs on startup. Migrations are append-only.
- **Starlark, not Lua/WASM/custom DSL** (ADR 0003). Hermetic by design — no I/O, no clock, no network. Embedded via `starlark-rust`.
- **Claude Code hooks, not PATH shims or shell injection** (ADR 0001). Don't add escape paths that mutate the user's shell environment.
- **Bypass mechanics**: `!!` command prefix or `LACON_DISABLE=1` env var skips filtering entirely. High bypass rates are tracked as a smell (`v_bypass_rate` view).

## Specs that are part of the contract

- `docs/specs/filter-rule-schema.md` — YAML rule format. Any change here is a breaking change for users. Lists every native primitive (`strip_ansi`, `drop_regex`, `keep_regex`, `replace_regex`, `dedupe`, `collapse_repeated`, `keep_head`, `keep_tail`, `keep_around_match`, `max_bytes`) and the Starlark `script` / `post_process` shape (`def process(ctx, lines) -> list[str]`).
- `docs/specs/tracking-data-model.md` — full SQLite schema, indexes, views (`v_unmatched_offenders`, `v_filtered_offenders`, `v_bypass_rate`, `v_project_savings`), retention policies, and the `0700` directory permission requirement.
- `docs/specs/chained-commands.md` — splitting rules for `&&` / `||` / `;`, per-segment rule resolution, exit-code propagation, and the v1 whole-chain bypass when any segment looks interactive. Granular per-segment TUI bypass is a v2 backlog item.

## v1 scope boundary (`docs/v1-scope.md`)

In: streaming engine + 10 native primitives + Starlark `post_process`, Claude Code adapter only (`PreToolUse` hook that rewrites matched commands to `lacon run --rule <id> -- <cmd>`), six CLI commands (`init`, `run`, `stats`, `explain`, `doctor`, `validate`) — note `run` is now both the production wrapper and the manual-debug entry, top-level chained-command splitting on `&&` / `||` / `;`, ten bundled rules (Tier 1 in `docs/bundled-rules-roadmap.md`), macOS + Linux.

Out: other adapters, per-line streaming Starlark, filtering inside pipes, native Windows, public rule registry, token-based accounting. Many of these are explicitly listed in `docs/backlog.md` — if a request matches one, point at the backlog rather than building it as a side quest.

## Open questions to be aware of

`docs/open-questions.md` is the project's design-risk log, organized by status: **open** items need a decision before the relevant code lands; **deferred to prototyping** items will be settled during implementation when working code exposes the right answer; **resolved** items document the rationale behind specific design choices. Consult it before making decisions that touch any of those topics; add new risks there rather than amending this section.
