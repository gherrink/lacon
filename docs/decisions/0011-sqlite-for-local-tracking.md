---
status: accepted
schema-version: 2
---

# 0011: SQLite for local tracking

## Context

`lacon` records every invocation for the `stats` and `explain` commands. The data model needs aggregation queries (top offenders by command, bypass rates by rule, byte savings by project), retention policies, and concurrent-write safety (multiple Claude Code sessions can run in parallel).

## Options

- **SQLite via `rusqlite` (chosen).** Real SQL, atomic transactions, and WAL-mode concurrency in a single file.
- **JSONL append-only log.** Simpler — one JSON line per invocation — but every `stats` query needs a full file scan, retention pruning means rewriting the whole file, and concurrent writes need file locking that doesn't compose across machines. Rejected.
- **Embedded key-value store (sled, redb).** No SQL, custom aggregation logic, less mature than SQLite — and aggregation is exactly what we need. Rejected.
- **External database (Postgres, MySQL).** Massive overkill for a single-user local tool and counter to the "no daemon, no network, local-only" commitment. Rejected.
- **No tracking at all.** Loses the entire `stats`/`explain` value proposition. Rejected.

## Decision

SQLite, accessed via the `rusqlite` crate. The database lives at `~/.local/share/lacon/history.db` with WAL mode enabled.

## Consequences

- Real SQL for the `stats` views — aggregation, grouping, and indexing come for free.
- Single-file storage with atomic transactions; backup is `cp history.db backup.db`.
- WAL mode handles concurrent writes from multiple `lacon` processes safely.
- Adds `rusqlite` (~MB of binary size after stripping); acceptable for the functionality.
- Migrations are append-only files run on startup; no schema-management daemon needed.
