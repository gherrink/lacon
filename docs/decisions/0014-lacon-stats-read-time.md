---
status: accepted
date: 2026-05-23
schema-version: 2
---

# 0014: lacon stats read-time presentation layer

## Context

`lacon stats` reads the four reporting views from ADR 0011 / `tracking-data-model.md` (`v_unmatched_offenders`, `v_filtered_offenders`, `v_bypass_rate`, `v_project_savings`) and prints each in full, one row per line. Specified for the common case (a handful of projects and rules), real usage exposed three problems that make the output unreadable:

1. **The per-project list grows without bound.** `v_project_savings` groups by the literal `current_dir()` captured at invocation, so tools run inside throwaway temp directories produce a unique `project_path` per run — hundreds of one-run rows.
2. **Git worktrees fragment one project into many.** A worktree records its own `project_path`, so one repo worked across N worktrees appears as N unrelated projects; the same happens for runs launched from a subdirectory.
3. **The columns are jargon and partly mislabeled.** "Offenders" is shorthand; `filtered_bytes` counts bytes that *survived* filtering (reads as "removed"); `keep_ratio` is `kept / raw` where lower is better — counterintuitive and inverted vs the "saved" framing. Byte counts are raw integers, nothing is capped, and there is no summary line.

Constraints bound the fix: append-only migrations (ADR 0011) — the four views must not be edited; cold-start ≤ 10 ms on the hook hot path (ADRs 0001/0013) — nothing may be added to the write path that resolves filesystem state per invocation. Canonicalizing a `project_path` to a stable repo key needs filesystem access (reading `.git`), which cannot run in SQL or on the write hot path.

## Options

- **Read-time presentation layer (chosen).** All fixes happen in `lacon-cli` at read time; the stored data model, the four views, and the write path are unchanged, and no migration is introduced.
- **A `repo_root` column via a future append-only migration, computed on the write path.** Would let project identity survive deleted directories and appear in SQL grouping, but pays canonicalization cost on the hot path — violating the cold-start budget. Deferred, not foreclosed.

## Decision

Add a read-time presentation layer to `lacon stats`. The write path still records the literal logical `current_dir()` as `project_path`; the four views are untouched; no migration is introduced. All logic lives in `lacon-cli` (`commands/stats.rs` plus a small helper), with one new aggregate reader behind the `lacon-core::tracking::query` boundary.

1. **Overall headline.** A summary line is printed first: total runs, distinct projects (after canonicalization), `raw → kept` bytes, and `saved` (absolute + percent), over `bypassed = 0` rows. Backed by a new `query::overall_totals(conn)` aggregate (plus a `--since`/`--project`-filtered counterpart).
2. **Project canonicalization + rollup.** The savings-by-project section re-aggregates the per-`project_path` rows in Rust under a canonical key (exact, since every field is an additive sum). The key is, in order: (a) **ephemeral** — a path under a temp root (`temp_dir()`, `/tmp`, `/var/folders`, `/private/var/folders`, `$TMPDIR`) collapses into a single `(ephemeral)` bucket; (b) **repo root via read-time `.git` resolution** — walk ancestors: a `.git` directory means that ancestor is the root; a `.git` *file* (`gitdir:`) means a linked worktree, resolved via `commondir` back to the main repo (no `git` subprocess, only bounded file reads, only in the cold `stats` path); (c) **literal fallback** — no `.git` found (or the dir no longer exists) keeps the literal `project_path`. Ephemeral detection precedes `.git` resolution.
3. **Top-N capping.** Every section prints at most N rows (default 10), ordered by its primary metric, then a `… M more` line with a drill-in hint (`--project` / `--rule` / `--since` / `--all`).
4. **Clarified columns and labels.** Task-oriented headers replace the "offenders" framing (e.g. "Commands with no rule", "Rule effectiveness"); the surviving-bytes column is `sent`/`kept` not `filtered_bytes`; effectiveness is `saved %` (higher is better); bytes are humanized (`22.8 KB`) with a `--bytes` flag for exact integers. Stored field names and view definitions are not renamed — the change is confined to CLI presentation.

## Consequences

- **No schema migration, write path untouched.** The hot-path cold-start budget (ADRs 0001/0013) is unaffected; canonicalization cost is paid only in `stats`.
- **Project grouping reflects current filesystem state, not invocation-time state.** A repo that has moved or been deleted falls back to its literal recorded path — acceptable for a savings report, and the price of keeping resolution off the write path.
- **Rollup is exact for project savings** (sums only). It deliberately does not merge the `AVG`-based `keep_ratio` of `v_filtered_offenders`.
- **`cli_stats.rs` snapshots are regenerated** to the new format; empty-DB and exit-code contracts (exit 2 on bad `--since`) are preserved.
- **`stats` gains a read-only dependency on filesystem layout** (reading `.git`); it opens the DB via `open_readonly` with a literal-path fallback on any I/O error.
- **Generality over the Claude-Code case.** Resolution uses the git worktree mechanism, not the Claude-specific `.claude/worktrees/<id>` path, so worktrees from any tool roll up correctly.
- **Reversibility.** A `repo_root` column can be added later by an append-only migration computed on the write path; this ADR defers that to avoid hot-path cost.
- **Relationship to prior ADRs: additive.** No existing ADR is amended. It builds on ADR 0011 (SQLite tracking / the four views) and respects the cold-start budget from ADRs 0001 and 0013; the `tracking-data-model.md` schema and views are unchanged.
