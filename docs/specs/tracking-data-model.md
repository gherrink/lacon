---
derived-from: prd:v1-scope
schema-version: 1
---

# Tracking data model

## Goal

Reference for the local SQLite database that records every invocation. It lives at `~/.local/share/lacon/history.db` with `0700` permissions on the directory.

## Context

Design goals: cheap synchronous writes on the hook hot path (a single sub-millisecond INSERT); queryable for the `stats` and `explain` CLI commands; privacy-respecting (raw output retention is opt-in and pruned aggressively); self-contained (no daemon, no migrations service - schema migrations are applied on `lacon` startup).

#### Examples

The four reporting views that back `lacon stats`:

```sql
-- Top offenders by raw bytes, no rule matched (candidates for new rules)
CREATE VIEW v_unmatched_offenders AS
SELECT command_normalized, COUNT(*) AS runs,
       SUM(raw_stdout_bytes + raw_stderr_bytes) AS total_raw_bytes
FROM invocations WHERE rule_id IS NULL AND bypassed = 0
GROUP BY command_normalized ORDER BY total_raw_bytes DESC;

-- Top offenders by filtered bytes, rule matched (rules leaving tokens on the table)
CREATE VIEW v_filtered_offenders AS
SELECT command_normalized, rule_id, COUNT(*) AS runs,
       SUM(filtered_bytes) AS total_filtered_bytes,
       AVG(CAST(filtered_bytes AS REAL) /
           NULLIF(raw_stdout_bytes + raw_stderr_bytes, 0)) AS avg_keep_ratio
FROM invocations WHERE rule_id IS NOT NULL AND bypassed = 0
GROUP BY command_normalized, rule_id ORDER BY total_filtered_bytes DESC;

-- Smell: rules the agent keeps overriding
CREATE VIEW v_bypass_rate AS
SELECT rule_id, COUNT(*) AS total, SUM(bypassed) AS bypassed,
       CAST(SUM(bypassed) AS REAL) / COUNT(*) AS bypass_rate
FROM invocations WHERE rule_id IS NOT NULL
GROUP BY rule_id HAVING COUNT(*) > 5 ORDER BY bypass_rate DESC;

-- Per-project savings summary
CREATE VIEW v_project_savings AS
SELECT project_path, COUNT(*) AS total_runs,
       SUM(raw_stdout_bytes + raw_stderr_bytes) AS raw_total,
       SUM(filtered_bytes) AS filtered_total,
       SUM(raw_stdout_bytes + raw_stderr_bytes - filtered_bytes) AS bytes_saved
FROM invocations WHERE bypassed = 0
GROUP BY project_path ORDER BY bytes_saved DESC;
```

Opting into raw-output capture (per project) and clearing retained data manually:

```yaml
# .lacon/config.yaml
store_raw_outputs: true
```

```bash
# remove everything
rm ~/.local/share/lacon/history.db
# or clear only the bulky raw blobs
sqlite3 ~/.local/share/lacon/history.db "DELETE FROM raw_outputs;"
```

## Criteria

### Schema (tables and indexes)  {#schema-tables-and-indexes}

Three tables, per the following DDL:

```sql
CREATE TABLE invocations (
  id                      INTEGER PRIMARY KEY,
  ts                      INTEGER NOT NULL,        -- unix epoch ms
  assistant               TEXT NOT NULL,
  session_id              TEXT,
  project_path            TEXT,
  command_raw             TEXT NOT NULL,
  command_normalized      TEXT NOT NULL,
  rule_id                 TEXT,
  rule_source             TEXT,                    -- 'project'|'user'|'bundled'|NULL
  exit_code               INTEGER NOT NULL,
  duration_ms             INTEGER NOT NULL,
  raw_stdout_bytes        INTEGER NOT NULL,
  raw_stderr_bytes        INTEGER NOT NULL,
  filtered_bytes          INTEGER NOT NULL,
  bypassed                INTEGER NOT NULL DEFAULT 0,
  rewritten               INTEGER NOT NULL DEFAULT 0,
  truncated_by_max_bytes  INTEGER NOT NULL DEFAULT 0,
  raw_output_id           INTEGER REFERENCES raw_outputs(id) ON DELETE SET NULL
);
CREATE INDEX idx_inv_ts      ON invocations(ts);
CREATE INDEX idx_inv_cmd     ON invocations(command_normalized);
CREATE INDEX idx_inv_rule    ON invocations(rule_id);
CREATE INDEX idx_inv_project ON invocations(project_path);

CREATE TABLE raw_outputs (
  id INTEGER PRIMARY KEY, invocation_id INTEGER NOT NULL,
  stdout BLOB, stderr BLOB, created_ts INTEGER NOT NULL
);
CREATE INDEX idx_raw_created ON raw_outputs(created_ts);

CREATE TABLE suspected_regressions (
  id INTEGER PRIMARY KEY,
  invocation_id INTEGER NOT NULL REFERENCES invocations(id) ON DELETE CASCADE,
  reason TEXT NOT NULL, detected_ts INTEGER NOT NULL
);
CREATE INDEX idx_reg_inv ON suspected_regressions(invocation_id);
```

### command_normalized  {#command-normalized}

The single most important field for `stats`. Default normalization is `<basename(argv[0])> <argv[1]>` for known package-manager-style commands, otherwise just the basename. The exact normalization is implementation-defined and may improve over time, so this field is not stable across `lacon` versions.

### rule_source  {#rule-source}

Identifies the layer a rule came from: `project`, `user`, or `bundled`; `NULL` means no rule matched.

### Boolean flag fields  {#boolean-flag-fields}

`bypassed` = 1 when the user explicitly bypassed filtering (`!!` prefix or `LACON_DISABLE=1`) — a high bypass rate on a rule is a smell. `rewritten` = 1 when the `rewrite` step modified the command pre-execution — distinguishes tokens saved by filtering from tokens never produced. `truncated_by_max_bytes` = 1 when the final `max_bytes` cap fired — a high count means earlier stages aren't doing enough.

### raw_output_id  {#raw-output-id}

NULL by default; when raw-output retention is enabled it points to the `raw_outputs` row storing the original stdout/stderr.

### Reporting views  {#reporting-views}

Four views back the `stats` command: `v_unmatched_offenders` (top raw-byte commands with no rule matched, `bypassed = 0`); `v_filtered_offenders` (top filtered-byte commands with a rule, including `AVG` `avg_keep_ratio`); `v_bypass_rate` (per-rule bypass fraction, `HAVING COUNT(*) > 5`); `v_project_savings` (per-`project_path` runs/raw/filtered/bytes_saved, `bypassed = 0`).

### Retention  {#retention}

Default retention: `invocations` 30 days, `raw_outputs` 3 days, `suspected_regressions` 30 days (tied to invocations). Windows are configurable in `~/.config/lacon/config.yaml`. Pruning runs on `lacon` startup as a single `DELETE ... WHERE created_ts < ?` per table.

### Privacy contract  {#privacy-contract}

Raw-output storage is off by default and opt-in per project via `store_raw_outputs: true`. The DB directory and contents are `0700` (user-only), enforced at DB initialization. The first time a project config flips `store_raw_outputs` off→on, `lacon` prints a one-time stderr notice (what is retained, where, how long), suppressed thereafter via a marker in the project config dir. There is no automatic redaction — captured output is stored byte-for-byte and users opting in own what their commands print. v1 ships no `lacon purge` command; users clear data by removing the DB file or via `sqlite3`. No telemetry, no remote sync, no network access.

### Migration policy  {#migration-policy}

Schema changes ship as numbered migrations applied automatically at startup. Migrations are append-only — never edit a released migration; down migrations are not supported.

### Deliberately out of scope  {#deliberately-out-of-scope}

Not in the v1 schema: token counts (require a tokenizer choice — deferred to v2), cost estimates (depend on token counts), cross-machine sync state, and user authentication (irrelevant for a local-only tool).
