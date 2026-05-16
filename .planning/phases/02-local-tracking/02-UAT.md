---
status: complete
phase: 02-local-tracking
source: [02-01-SUMMARY.md, 02-02-SUMMARY.md, 02-03-SUMMARY.md, 02-04-SUMMARY.md, 02-05-SUMMARY.md, 02-06-SUMMARY.md]
started: 2026-05-16T05:09:00Z
updated: 2026-05-16T05:22:00Z
---

## Current Test

[testing complete]

## Tests

### 1. Cold Start — fresh `lacon run` creates the SQLite DB
expected: |
  Setup (single shell, isolated DB so we don't touch your real ~/.local/share/lacon):

      export XDG_DATA_HOME=$(mktemp -d)
      echo "DB will land at: $XDG_DATA_HOME/lacon/history.db"

  Run a one-shot:

      cargo run --quiet -p lacon-cli -- run -- echo hello-uat

  Then inspect:

      ls -la "$XDG_DATA_HOME/lacon/"
      sqlite3 "$XDG_DATA_HOME/lacon/history.db" "PRAGMA journal_mode;"
      sqlite3 "$XDG_DATA_HOME/lacon/history.db" "SELECT COUNT(*) FROM invocations;"

  Expected observations:
    - `echo hello-uat` output reaches stdout (you see "hello-uat")
    - `history.db` exists, parent dir `lacon/` shows `drwx------` (0700)
    - `PRAGMA journal_mode` returns `wal`
    - `SELECT COUNT(*) FROM invocations` returns `1`
result: pass

### 2. Privacy default — raw_outputs stays empty
expected: |
  Continuing with the same `$XDG_DATA_HOME` from Test 1 (no project `.lacon/config.yaml`
  flipping the default), run another invocation and confirm `raw_outputs` is still empty:

      cargo run --quiet -p lacon-cli -- run -- echo second-run
      sqlite3 "$XDG_DATA_HOME/lacon/history.db" "SELECT COUNT(*) FROM raw_outputs;"
      sqlite3 "$XDG_DATA_HOME/lacon/history.db" "SELECT raw_output_id FROM invocations;"

  Expected:
    - `SELECT COUNT(*) FROM raw_outputs` returns `0`
    - `raw_output_id` column shows NULL for both rows
result: pass

### 3. Privacy opt-in — first run prints stderr warning, second run is silent
expected: |
  Create a project with `store_raw_outputs: true`:

      mkdir -p /tmp/uat-priv/.lacon
      cat > /tmp/uat-priv/.lacon/config.yaml <<'YAML'
      store_raw_outputs: true
      YAML
      cd /tmp/uat-priv

  Run once (stderr should carry the privacy notice):

      cargo run --quiet --manifest-path /home/maurice/Projects/gherrink-lacon/Cargo.toml \
        -p lacon-cli -- run -- echo first 2>&1 1>/dev/null

  Run again (stderr should be silent for the same notice):

      cargo run --quiet --manifest-path /home/maurice/Projects/gherrink-lacon/Cargo.toml \
        -p lacon-cli -- run -- echo second 2>&1 1>/dev/null

      ls -la /tmp/uat-priv/.lacon/

  Expected:
    - Run 1 stderr contains a message mentioning `store_raw_outputs is enabled`
    - Run 2 stderr does NOT repeat that message
    - `/tmp/uat-priv/.lacon/.store_raw_outputs_acked` exists (zero-byte marker)
result: pass

### 4. Four required views are queryable
expected: |
  Against the DB from Tests 1–2 (`$XDG_DATA_HOME/lacon/history.db`), each of the
  four views returns a non-error result set:

      DB="$XDG_DATA_HOME/lacon/history.db"
      sqlite3 "$DB" "SELECT * FROM v_unmatched_offenders LIMIT 5;"
      sqlite3 "$DB" "SELECT * FROM v_filtered_offenders LIMIT 5;"
      sqlite3 "$DB" "SELECT * FROM v_bypass_rate LIMIT 5;"
      sqlite3 "$DB" "SELECT * FROM v_project_savings LIMIT 5;"

  Expected:
    - Each query exits 0 (no "no such view" / "no such column" error)
    - Output may be empty (small dataset) — empty is success, errors are failure
result: pass

### 5. Project `retention.*` key is rejected by `lacon validate`
expected: |
  Create a project config with a user-only retention key and validate it:

      mkdir -p /tmp/uat-retention/.lacon
      cat > /tmp/uat-retention/.lacon/config.yaml <<'YAML'
      retention:
        invocations_days: 7
      YAML

      cargo run --quiet --manifest-path /home/maurice/Projects/gherrink-lacon/Cargo.toml \
        -p lacon-cli -- validate /tmp/uat-retention/.lacon/config.yaml
      echo "exit=$?"

  Expected:
    - Exit code is non-zero
    - Error message mentions `retention` is user-only and points at
      `~/.config/lacon/config.yaml` (the user config path)
result: pass

### 6. Lazy-open — `lacon --version` does NOT create the DB
expected: |
  With a fresh isolated XDG_DATA_HOME, `lacon --version` and `lacon validate <rule>`
  must NOT create the SQLite DB file (cold-start invariant).

      export XDG_DATA_HOME=$(mktemp -d)
      cargo run --quiet -p lacon-cli -- --version
      ls "$XDG_DATA_HOME/lacon/" 2>&1 || echo "(directory does not exist - good)"

  Expected:
    - `--version` prints e.g. `lacon 0.1.0`
    - `$XDG_DATA_HOME/lacon/` either does not exist OR contains no `history.db`
result: pass

## Summary

total: 6
passed: 6
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps

[none yet]
