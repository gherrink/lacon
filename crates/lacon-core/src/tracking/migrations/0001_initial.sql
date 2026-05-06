-- 0001_initial.sql — Phase 2 / lacon v1.
-- DDL byte-exact per docs/specs/tracking-data-model.md:14-141.
-- Applied inside a single BEGIN IMMEDIATE / COMMIT transaction by tracking::migrations::migrate.
-- D-09: views use the drop-if-exists pattern so future migrations can re-create without orphan checks.

CREATE TABLE invocations (
  id                      INTEGER PRIMARY KEY,
  ts                      INTEGER NOT NULL,
  assistant               TEXT NOT NULL,
  session_id              TEXT,
  project_path            TEXT,
  command_raw             TEXT NOT NULL,
  command_normalized      TEXT NOT NULL,
  rule_id                 TEXT,
  rule_source             TEXT,
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
  reason          TEXT NOT NULL,
  detected_ts     INTEGER NOT NULL
);

CREATE INDEX idx_reg_inv ON suspected_regressions(invocation_id);

CREATE TABLE lacon_meta (
  key   TEXT PRIMARY KEY,
  value TEXT
);

INSERT INTO lacon_meta (key, value) VALUES ('last_pruned_ts', '0');

DROP VIEW IF EXISTS v_unmatched_offenders;
CREATE VIEW v_unmatched_offenders AS
SELECT command_normalized,
       COUNT(*) AS runs,
       SUM(raw_stdout_bytes + raw_stderr_bytes) AS total_raw_bytes
FROM invocations
WHERE rule_id IS NULL AND bypassed = 0
GROUP BY command_normalized
ORDER BY total_raw_bytes DESC;

DROP VIEW IF EXISTS v_filtered_offenders;
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

DROP VIEW IF EXISTS v_bypass_rate;
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

DROP VIEW IF EXISTS v_project_savings;
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
