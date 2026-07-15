---
status: accepted
schema-version: 2
---

# 0009: Separated raw_outputs table

## Context

Two kinds of data go into the tracking database and have very different size and retention profiles: small invocation metadata (command, exit code, byte counts, duration) — a few hundred bytes per row, useful for weeks of trend analysis; and potentially large raw stdout/stderr blobs backing `lacon explain` — megabytes per row, useful only for the most recent invocations, and the privacy-sensitive part (may contain secrets, paths, customer data).

## Options

- **Separate `raw_outputs` table (chosen).** Distinct retention per table; when raw storage is off (default), no row is written and the join is a NULL.
- **Single table with stdout/stderr columns.** Simpler, one INSERT per invocation, but retention can't differ between fields in a row — so either keep megabytes for a month (wasteful) or prune metadata aggressively (loses trend data). Rejected.
- **File-system blobs in a directory.** Store raw output as files under `~/.local/share/lacon/raw/<id>.txt` — but atomic pruning is harder (coordinate file deletion with DB rows), the backup story is more complex, and SQLite's transactional guarantees are lost. Rejected.

## Decision

Store raw output in a separate `raw_outputs` table, referenced from `invocations.raw_output_id`. Apply different retention policies to each table (default: 30 days for invocations, 3 days for raw outputs). When raw-output storage is disabled (the default), no row is written to `raw_outputs` and `raw_output_id` is NULL.

## Consequences

- Pruning the bulky storage is independent from pruning the metadata trail.
- Trend analysis stays useful for weeks even when raw outputs are aggressively pruned.
- When raw storage is off, the `raw_outputs` table stays empty and the join is a NULL — zero overhead.
- `lacon explain <id>` requires raw retention to have been enabled at invocation time; users opt in per-project knowing the trade-off.
- Slightly more complex schema (two tables, a foreign key) and one extra INSERT when raw retention is on.
