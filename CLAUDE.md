# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project status

**Design phase. No code yet.** There is no `Cargo.toml`, no source tree, no build, lint, or test commands. The repository contains only the README, LICENSE, and `docs/`. Do not invent build commands or pretend a tool chain exists — when implementation starts, the planned crate layout is in `docs/architecture.md` (`crates/lacon-core`, `crates/lacon-cli`, `crates/lacon-adapter-claudecode`, `bundled-rules/`, `tests/`).

The design is intentionally locked down ahead of implementation via 13 ADRs in `docs/decisions/`. Treat those ADRs as the source of truth — if a proposed change contradicts one, surface that explicitly rather than silently working around it.

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
