# 0009: Separated raw_outputs table

**Status:** Accepted

## Context

Two kinds of data go into the tracking database: small invocation metadata (command, exit code, byte counts, duration) and potentially large raw stdout/stderr blobs that back the `lacon explain` command. They have very different size and retention profiles:

- Metadata is a few hundred bytes per row, useful for trend analysis over weeks
- Raw output can be megabytes per row, useful only for the most recent invocations the user might want to explain
- Raw output is also the privacy-sensitive part — it can contain secrets, paths, customer data

## Decision

Store raw output in a separate `raw_outputs` table, referenced from `invocations.raw_output_id`. Apply different retention policies to each table (default: 30 days for invocations, 3 days for raw outputs). When raw output storage is disabled (the default), no row is written to `raw_outputs` and `raw_output_id` is NULL.

## Consequences

- Pruning the bulky storage is independent from pruning the metadata trail
- Trend analysis stays useful for weeks even when raw outputs are aggressively pruned
- When raw storage is off, the `raw_outputs` table stays empty and the join is a NULL — zero overhead
- `lacon explain <id>` requires raw retention to have been enabled at the time of the invocation; users opt in per-project knowing this trade-off
- Slightly more complex schema (two tables, foreign key) and one extra INSERT when raw retention is on

## Alternatives considered

**Single table with stdout/stderr columns.** Simpler schema, one INSERT per invocation. Rejected because retention can't differ between fields in the same row, so we'd either keep megabytes for a month (wasteful) or prune the metadata aggressively (loses trend data).

**File-system blobs in a directory.** Store raw output as files under `~/.local/share/lacon/raw/<id>.txt`. Atomic pruning is harder (need to coordinate file deletion with DB row updates), backup story is more complex, and we lose SQLite's transactional guarantees.
