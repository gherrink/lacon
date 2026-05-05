# 0011: SQLite for local tracking

**Status:** Accepted

## Context

`lacon` records every invocation for the `stats` and `explain` commands. The data model needs aggregation queries (top offenders by command, bypass rates by rule, byte savings by project), retention policies, and concurrent-write safety (multiple Claude Code sessions can run in parallel).

## Decision

SQLite, accessed via the `rusqlite` crate. Database lives at `~/.local/share/lacon/history.db` with WAL mode enabled.

## Consequences

- Real SQL for the `stats` views — aggregation, grouping, indexing all come for free
- Single-file storage with atomic transactions; backup is `cp history.db backup.db`
- WAL mode handles concurrent writes from multiple `lacon` processes safely
- Adds `rusqlite` (~MB of binary size after stripping); acceptable for the functionality
- Migrations are append-only files run on startup; no schema management daemon needed

## Alternatives considered

**JSONL append-only log.** Simpler — every invocation appends one line of JSON to a file. Rejected because every `stats` query requires a full file scan, retention pruning means rewriting the whole file, and concurrent writes need file locking that doesn't compose well across machines.

**Embedded key-value store (sled, redb).** No SQL, custom aggregation logic, less mature than SQLite. Rejected: aggregation is exactly what we need, and SQLite is the most battle-tested embedded database in existence.

**External database (Postgres, MySQL).** Massive overkill for a single-user local tool, plus runs counter to the "no daemon, no network, local-only" architectural commitment.

**No tracking at all.** Loses the entire `stats` and `explain` value proposition. Trivially rejected.
