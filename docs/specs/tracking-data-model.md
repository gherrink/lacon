# Tracking data model

Reference for the local SQLite database that records every invocation. Lives at `~/.local/share/lacon/history.db` with `0700` permissions on the directory.

## Goals

- Cheap synchronous writes on the hook hot path (single INSERT, sub-millisecond)
- Queryable for the `stats` and `explain` CLI commands
- Privacy-respecting: raw output retention is opt-in and pruned aggressively
- Self-contained: no daemon, no migrations service. Schema migrations applied on `lacon` startup.

## Schema

```sql
CREATE TABLE invocations (
  id                      INTEGER PRIMARY KEY,
  ts                      INTEGER NOT NULL,        -- unix epoch ms
  assistant               TEXT NOT NULL,           -- 'claude-code', etc.
  session_id              TEXT,                    -- from hook context if available
  project_path            TEXT,                    -- cwd at invocation

  command_raw             TEXT NOT NULL,           -- exact command line received
  command_normalized      TEXT NOT NULL,           -- for grouping; e.g. 'pnpm install'
  rule_id                 TEXT,                    -- NULL = no rule matched
  rule_source             TEXT,                    -- 'project' | 'user' | 'bundled' | NULL

  exit_code               INTEGER NOT NULL,
  duration_ms             INTEGER NOT NULL,

  raw_stdout_bytes        INTEGER NOT NULL,
  raw_stderr_bytes        INTEGER NOT NULL,
  filtered_bytes          INTEGER NOT NULL,

  bypassed                INTEGER NOT NULL DEFAULT 0,  -- !! prefix or env var
  rewritten               INTEGER NOT NULL DEFAULT 0,  -- we added flags pre-exec
  truncated_by_max_bytes  INTEGER NOT NULL DEFAULT 0,

  raw_output_id           INTEGER REFERENCES raw_outputs(id) ON DELETE SET NULL
);

CREATE INDEX idx_inv_ts       ON invocations(ts);
CREATE INDEX idx_inv_cmd      ON invocations(command_normalized);
CREATE INDEX idx_inv_rule     ON invocations(rule_id);
CREATE INDEX idx_inv_project  ON invocations(project_path);

CREATE TABLE raw_outputs (
  id              INTEGER PRIMARY KEY,
  invocation_id   INTEGER NOT NULL,
  stdout          BLOB,
  stderr          BLOB,
  created_ts      INTEGER NOT NULL
);

CREATE INDEX idx_raw_created ON raw_outputs(created_ts);

CREATE TABLE suspected_regressions (
  id              INTEGER PRIMARY KEY,
  invocation_id   INTEGER NOT NULL REFERENCES invocations(id) ON DELETE CASCADE,
  reason          TEXT NOT NULL,             -- 'rerun_with_verbose', 'explain_called_after', etc.
  detected_ts     INTEGER NOT NULL
);

CREATE INDEX idx_reg_inv ON suspected_regressions(invocation_id);
```

## Field semantics

### `command_normalized`

The single most important field for `stats`. Without normalization, `pnpm install`, `pnpm install --frozen-lockfile`, and `/usr/local/bin/pnpm install` look like three different commands.

Default normalization: `<basename(argv[0])> <argv[1]>` for known package-manager-style commands; otherwise just the basename. The exact normalization is implementation-defined and may improve over time, so this field is not stable across `lacon` versions.

### `rule_source`

Identifies which layer the rule came from: `project`, `user`, or `bundled`. `NULL` means no rule matched. Used to answer "are project-specific rules pulling their weight" type questions.

### `bypassed`

1 if the user explicitly bypassed filtering for this invocation (`!!` prefix or `LACON_DISABLE=1`). High bypass rate on a rule is a smell.

### `rewritten`

1 if the `rewrite` step modified the command before execution. Useful for distinguishing "tokens saved by output filtering" from "tokens saved by never producing them in the first place."

### `truncated_by_max_bytes`

1 if the final `max_bytes` cap kicked in. High count for a rule means the earlier stages aren't doing enough work and `max_bytes` is load-bearing.

### `raw_output_id`

NULL by default. When raw output retention is enabled, points to the row in `raw_outputs` storing the original stdout/stderr.

## Views

```sql
-- Top offenders by raw bytes, no rule matched (candidates for new rules)
CREATE VIEW v_unmatched_offenders AS
SELECT command_normalized,
       COUNT(*) AS runs,
       SUM(raw_stdout_bytes + raw_stderr_bytes) AS total_raw_bytes
FROM invocations
WHERE rule_id IS NULL AND bypassed = 0
GROUP BY command_normalized
ORDER BY total_raw_bytes DESC;

-- Top offenders by filtered bytes, rule matched (existing rules leaving tokens on the table)
CREATE VIEW v_filtered_offenders AS
SELECT command_normalized, rule_id,
       COUNT(*) AS runs,
       SUM(filtered_bytes) AS total_filtered_bytes,
       AVG(CAST(filtered_bytes AS REAL) /
           NULLIF(raw_stdout_bytes + raw_stderr_bytes, 0)) AS avg_keep_ratio
FROM invocations
WHERE rule_id IS NOT NULL AND bypassed = 0
GROUP BY command_normalized, rule_id
ORDER BY total_filtered_bytes DESC;

-- Smell: rules the agent keeps overriding
CREATE VIEW v_bypass_rate AS
SELECT rule_id,
       COUNT(*) AS total,
       SUM(bypassed) AS bypassed,
       CAST(SUM(bypassed) AS REAL) / COUNT(*) AS bypass_rate
FROM invocations
WHERE rule_id IS NOT NULL
GROUP BY rule_id
HAVING COUNT(*) > 5
ORDER BY bypass_rate DESC;

-- Per-project savings summary
CREATE VIEW v_project_savings AS
SELECT project_path,
       COUNT(*) AS total_runs,
       SUM(raw_stdout_bytes + raw_stderr_bytes) AS raw_total,
       SUM(filtered_bytes) AS filtered_total,
       SUM(raw_stdout_bytes + raw_stderr_bytes - filtered_bytes) AS bytes_saved
FROM invocations
WHERE bypassed = 0
GROUP BY project_path
ORDER BY bytes_saved DESC;
```

## Retention

Two retention policies, both configurable in `~/.config/lacon/config.yaml`:

| Table | Default retention | Reason |
|-------|-------------------|--------|
| `invocations` | 30 days | Cheap to keep; useful for trend analysis |
| `raw_outputs` | 3 days | Bulky; mainly useful for recent `lacon explain` calls |
| `suspected_regressions` | 30 days | Tied to `invocations` |

Pruning runs on `lacon` startup: a single `DELETE FROM ... WHERE created_ts < ?` against each table.

## Privacy

- Database directory permissions: `0700`
- `raw_outputs` storage is **off** by default. Opt-in per project in `.lacon/config.yaml`:

  ```yaml
  store_raw_outputs: true
  ```

- A `lacon purge` command deletes:
  - `lacon purge raw` — all raw outputs
  - `lacon purge --since=2024-01-01` — invocations and outputs older than a date
  - `lacon purge --project=<path>` — everything for a project
- No telemetry, no remote sync, no network access.

## Migration policy

Schema changes ship as numbered migrations applied automatically at startup. Migrations are append-only — never edit a migration after it's released. Down migrations are not supported.

## What's deliberately not in this schema (yet)

- Token counts (we store bytes; tokens require a tokenizer choice — see [open-questions](../open-questions.md))
- Cost estimates (depends on token counts)
- Cross-machine sync state
- User authentication (irrelevant for local-only)
