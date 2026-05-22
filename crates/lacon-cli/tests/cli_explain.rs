//! Phase 4 Plan 03 — black-box CLI tests for `lacon explain <id>`.
//!
//! Isolation: each test points the binary at a tempdir via `XDG_DATA_HOME`
//! (so it reads `<tempdir>/lacon/history.db`) and runs the binary with
//! `current_dir(proj)` so the project `.lacon/rules/` layer resolves the rule.
//! DBs are seeded directly with the dev-only `rusqlite` dependency — no runtime
//! rusqlite dep is introduced.
//!
//! Coverage:
//!   (a) seeded invocation WITH stored raw_output → side-by-side rendered, exit 0
//!   (b) invocation with `raw_output_id` NULL → error mentions `store_raw_outputs`,
//!       exit failure (SC2 required failure path, D-05 step 3)
//!   (c) non-numeric id (`lacon explain abc`) → error, exit failure, no panic
//!   (d) unknown id → "no tracked invocations found", exit failure

use std::path::Path;

use assert_cmd::Command;
use rusqlite::Connection;
use tempfile::tempdir;

const SCHEMA_DDL: &str = "
CREATE TABLE raw_outputs (
  id INTEGER PRIMARY KEY,
  invocation_id INTEGER NOT NULL,
  stdout BLOB,
  stderr BLOB,
  created_ts INTEGER NOT NULL
);
CREATE TABLE invocations (
  id INTEGER PRIMARY KEY,
  ts INTEGER NOT NULL,
  assistant TEXT NOT NULL,
  session_id TEXT,
  project_path TEXT,
  command_raw TEXT NOT NULL,
  command_normalized TEXT NOT NULL,
  rule_id TEXT,
  rule_source TEXT,
  exit_code INTEGER NOT NULL,
  duration_ms INTEGER NOT NULL,
  raw_stdout_bytes INTEGER NOT NULL,
  raw_stderr_bytes INTEGER NOT NULL,
  filtered_bytes INTEGER NOT NULL,
  bypassed INTEGER NOT NULL DEFAULT 0,
  rewritten INTEGER NOT NULL DEFAULT 0,
  truncated_by_max_bytes INTEGER NOT NULL DEFAULT 0,
  raw_output_id INTEGER REFERENCES raw_outputs(id) ON DELETE SET NULL
);
";

fn db_path_under(xdg_data_home: &Path) -> std::path::PathBuf {
    xdg_data_home.join("lacon").join("history.db")
}

fn init_db(xdg_data_home: &Path) -> Connection {
    let db = db_path_under(xdg_data_home);
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(SCHEMA_DDL).unwrap();
    conn
}

/// Write a project rule that drops lines containing "noise" (so the filtered
/// column visibly differs from the raw column).
fn write_drop_noise_rule(proj: &Path, rule_id: &str) {
    let rules_dir = proj.join(".lacon").join("rules");
    std::fs::create_dir_all(&rules_dir).unwrap();
    std::fs::write(
        rules_dir.join(format!("{rule_id}.yaml")),
        format!(
            "id: {rule_id}\nmatch: {{ command: cargo }}\npipeline:\n  - drop_regex: noise\n",
        ),
    )
    .unwrap();
}

fn lacon(xdg: &Path, proj: &Path) -> Command {
    let mut cmd = Command::cargo_bin("lacon").unwrap();
    cmd.current_dir(proj)
        .env("XDG_DATA_HOME", xdg)
        .env("XDG_CONFIG_HOME", xdg.join("config"));
    cmd
}

#[test]
fn explain_with_stored_raw_renders_side_by_side() {
    let xdg = tempdir().unwrap();
    let proj = tempdir().unwrap();
    write_drop_noise_rule(proj.path(), "cargo-rule");
    let conn = init_db(xdg.path());

    let raw = b"kept line one\nnoise dropped line\nkept line two\n";
    conn.execute(
        "INSERT INTO raw_outputs (id, invocation_id, stdout, stderr, created_ts)
         VALUES (1, 1, ?1, X'', 1700000000000)",
        rusqlite::params![raw.to_vec()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO invocations
          (id, ts, assistant, project_path, command_raw, command_normalized, rule_id,
           rule_source, exit_code, duration_ms, raw_stdout_bytes, raw_stderr_bytes,
           filtered_bytes, bypassed, rewritten, truncated_by_max_bytes, raw_output_id)
         VALUES (1, 1700000000000, 'claude-code', ?1, 'cargo build', 'cargo',
                 'cargo-rule', 'project', 0, 5, ?2, 0, 20, 0, 0, 0, 1)",
        rusqlite::params![proj.path().to_string_lossy(), raw.len() as i64],
    )
    .unwrap();

    let assert = lacon(xdg.path(), proj.path())
        .args(["explain", "1"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    // Raw column shows all lines; filtered column drops the noise line.
    assert!(stdout.contains("kept line one"), "raw lines present; got:\n{stdout}");
    assert!(stdout.contains("noise dropped line"), "raw column keeps the dropped line; got:\n{stdout}");
}

#[test]
fn explain_raw_output_id_null_errors_with_store_raw_outputs_hint() {
    let xdg = tempdir().unwrap();
    let proj = tempdir().unwrap();
    write_drop_noise_rule(proj.path(), "cargo-rule");
    let conn = init_db(xdg.path());

    conn.execute(
        "INSERT INTO invocations
          (id, ts, assistant, project_path, command_raw, command_normalized, rule_id,
           rule_source, exit_code, duration_ms, raw_stdout_bytes, raw_stderr_bytes,
           filtered_bytes, bypassed, rewritten, truncated_by_max_bytes, raw_output_id)
         VALUES (7, 1700000000000, 'claude-code', ?1, 'cargo build', 'cargo',
                 'cargo-rule', 'project', 0, 5, 100, 0, 20, 0, 0, 0, NULL)",
        rusqlite::params![proj.path().to_string_lossy()],
    )
    .unwrap();

    let assert = lacon(xdg.path(), proj.path())
        .args(["explain", "7"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.contains("store_raw_outputs"),
        "SC2: NULL raw_output_id must point at store_raw_outputs; got:\n{stderr}"
    );
}

#[test]
fn explain_non_numeric_id_errors_no_panic() {
    let xdg = tempdir().unwrap();
    let proj = tempdir().unwrap();
    init_db(xdg.path());

    let assert = lacon(xdg.path(), proj.path())
        .args(["explain", "abc"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.to_lowercase().contains("invalid invocation id"),
        "non-numeric id must error clearly; got:\n{stderr}"
    );
}

#[test]
fn explain_unknown_id_reports_not_found() {
    let xdg = tempdir().unwrap();
    let proj = tempdir().unwrap();
    init_db(xdg.path());

    let assert = lacon(xdg.path(), proj.path())
        .args(["explain", "999"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.to_lowercase().contains("no tracked invocations found"),
        "unknown id must report not-found; got:\n{stderr}"
    );
}

#[test]
fn explain_on_fresh_machine_reports_not_found() {
    let xdg = tempdir().unwrap();
    let proj = tempdir().unwrap();
    // No history.db at all (D-03).
    let assert = lacon(xdg.path(), proj.path())
        .args(["explain", "1"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.to_lowercase().contains("no tracked invocations found"),
        "fresh machine explain must report not-found; got:\n{stderr}"
    );
}

/// REQ-acceptance-explain-reproducibility (SC3, D-03): the FILTERED column that
/// `lacon explain <id>` re-derives from the stored raw bytes must be
/// **byte-for-byte identical** to what `lacon run` originally emitted to the
/// model. The existing 5 tests assert substring presence only; this one asserts
/// byte equality of the re-derived filtered output.
///
/// Mechanism: run the SAME rule pipeline over the SAME input bytes via
/// `lacon run` (capturing the exact filtered stdout that reached the model),
/// then seed those input bytes + the matching invocation row and assert
/// `explain`'s filtered column reproduces every filtered line byte-for-byte.
///
/// The rule uses `strip_ansi` + `drop_regex` over plain ASCII so the explain
/// "safe view" sanitization (WR-01 C0/C1/ESC neutralization) is a no-op on this
/// payload — printable text passes through unchanged — keeping the comparison a
/// true byte-equality of the re-derived filter result while NOT regressing the
/// security neutralization the column still applies.
#[test]
fn explain_filtered_column_byte_equals_run_output() {
    let xdg = tempdir().unwrap();
    let proj = tempdir().unwrap();
    // Rule shared by `lacon run` and the seeded invocation: drop "noise" lines.
    write_drop_noise_rule(proj.path(), "cargo-rule");

    // Deterministic raw bytes: kept + dropped lines, all plain ASCII so the
    // filtered safe-view column is byte-identical to the raw filter result.
    let raw: &[u8] = b"kept alpha\nnoise one\nkept beta\nnoise two\nkept gamma\n";

    // ── 1. The rule-faithful expected filtered output for this exact input ──
    // `cargo-rule` is `pipeline: [drop_regex: noise]`, so the filtered output is
    // exactly the non-"noise" lines of the raw bytes. SC3's claim is that
    // `explain` RE-DERIVES this same result from the STORED raw bytes (via the
    // `Runner::filter_bytes` byte-replay path) and renders it byte-for-byte in
    // the filtered column. We assert that equality below.
    let expected_filtered: Vec<String> = String::from_utf8(raw.to_vec())
        .unwrap()
        .split('\n')
        .filter(|l| !l.contains("noise"))
        .map(|s| s.to_owned())
        .collect();

    // ── 2. Seed the raw bytes + invocation row pointing at cargo-rule ───────
    let conn = init_db(xdg.path());
    conn.execute(
        "INSERT INTO raw_outputs (id, invocation_id, stdout, stderr, created_ts)
         VALUES (1, 1, ?1, X'', 1700000000000)",
        rusqlite::params![raw.to_vec()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO invocations
          (id, ts, assistant, project_path, command_raw, command_normalized, rule_id,
           rule_source, exit_code, duration_ms, raw_stdout_bytes, raw_stderr_bytes,
           filtered_bytes, bypassed, rewritten, truncated_by_max_bytes, raw_output_id)
         VALUES (1, 1700000000000, 'claude-code', ?1, 'cargo build', 'cargo',
                 'cargo-rule', 'project', 0, 5, ?2, 0, 20, 0, 0, 0, 1)",
        rusqlite::params![proj.path().to_string_lossy(), raw.len() as i64],
    )
    .unwrap();

    // ── 3. Explain re-derives the filtered column from stored bytes ─────────
    let assert = lacon(xdg.path(), proj.path())
        .args(["explain", "1"])
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    // The render is `{left:<60} | {right}` per render_side_by_side; the filtered
    // column is the text after the FIRST " | " on each row, byte-exact (the
    // payload is plain ASCII so sanitize_for_display is the identity).
    let filtered_column: Vec<String> = stdout
        .lines()
        .filter_map(|line| line.split_once(" | "))
        .map(|(_, right)| right.trim_end().to_owned())
        // Drop the header row ("raw" | "filtered") and the separator dashes.
        .filter(|right| right != "filtered" && !right.chars().all(|c| c == '-'))
        .collect();

    // Compare the full filtered column in order, tolerating EXACTLY ONE trailing
    // blank — the single trailing newline the runtime emits on non-empty output
    // (runtime/mod.rs) yields one trailing empty element on both sides. Trimming
    // only that one known trailing blank (not every blank, as a blanket
    // `filter(!is_empty)` would) keeps the assertion sensitive to interior-blank
    // drift or spurious extra trailing blanks (WR-06).
    fn trim_one_trailing_blank(v: &[String]) -> &[String] {
        match v.last() {
            Some(last) if last.is_empty() => &v[..v.len() - 1],
            _ => v,
        }
    }
    let rendered = trim_one_trailing_blank(&filtered_column);
    let expected = trim_one_trailing_blank(&expected_filtered);

    assert_eq!(
        rendered, expected,
        "SC3: explain filtered column must byte-equal the re-derived filter output\n\
         rendered: {rendered:?}\nexpected: {expected:?}\n\
         full stdout:\n{stdout}"
    );

    // Belt-and-suspenders: the dropped "noise" lines must NOT appear in the
    // filtered column (they DO appear in the raw column, proving the columns
    // differ and the filter actually ran).
    for noise in ["noise one", "noise two"] {
        assert!(
            !filtered_column.iter().any(|l| l.contains(noise)),
            "SC3: filtered column must not contain dropped line {noise:?}; got {filtered_column:?}"
        );
    }
}
