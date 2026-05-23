//! Phase 4 Plan 03 — black-box CLI tests for `lacon stats`.
//!
//! Isolation: each test points the binary at a tempdir via `XDG_DATA_HOME`
//! (so it reads `<tempdir>/lacon/history.db`) and seeds that DB directly with
//! the dev-only `rusqlite` dependency (Cargo.toml `[dev-dependencies]`). No
//! runtime `rusqlite` dep is introduced — all production SQL lives in
//! `lacon-core::tracking::query`.
//!
//! Coverage:
//!   (a) seeded DB → the four section headers + at least one offender row
//!   (b) `--since`/`--project`/`--rule` narrow the output (a row visible without
//!       the filter is absent with a narrowing filter)
//!   (c) empty DB (no history.db) → "no data yet" present, exit success (D-03)

use std::path::Path;

use assert_cmd::Command;
use rusqlite::Connection;
use tempfile::tempdir;

/// The byte-exact schema DDL (subset sufficient for the read path: the two
/// base tables + the four views). Mirrors
/// `crates/lacon-core/src/tracking/migrations/0001_initial.sql`. Seeding tests
/// via the dev-only rusqlite is the established lacon-cli pattern (tracking_e2e.rs).
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
";

fn db_path_under(xdg_data_home: &Path) -> std::path::PathBuf {
    xdg_data_home.join("lacon").join("history.db")
}

/// Create the schema in a fresh history.db under `<xdg>/lacon/`.
fn init_db(xdg_data_home: &Path) -> Connection {
    let db = db_path_under(xdg_data_home);
    std::fs::create_dir_all(db.parent().unwrap()).unwrap();
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(SCHEMA_DDL).unwrap();
    conn
}

/// Insert one invocation row. `rule_id = None` → unmatched offender.
#[allow(clippy::too_many_arguments)]
fn insert_invocation(
    conn: &Connection,
    ts: i64,
    project_path: &str,
    command_normalized: &str,
    rule_id: Option<&str>,
    exit_code: i64,
    raw_bytes: i64,
    filtered_bytes: i64,
    bypassed: i64,
) {
    conn.execute(
        "INSERT INTO invocations
          (ts, assistant, project_path, command_raw, command_normalized, rule_id,
           rule_source, exit_code, duration_ms, raw_stdout_bytes, raw_stderr_bytes,
           filtered_bytes, bypassed, rewritten, truncated_by_max_bytes, raw_output_id)
         VALUES (?1,'claude-code',?2,?3,?4,?5,?6,?7,5,?8,0,?9,?10,0,0,NULL)",
        rusqlite::params![
            ts,
            project_path,
            command_normalized,
            command_normalized,
            rule_id,
            rule_id.map(|_| "project"),
            exit_code,
            raw_bytes,
            filtered_bytes,
            bypassed,
        ],
    )
    .unwrap();
}

fn lacon(xdg: &Path) -> Command {
    let mut cmd = Command::cargo_bin("lacon").unwrap();
    cmd.env("XDG_DATA_HOME", xdg)
        .env("XDG_CONFIG_HOME", xdg.join("config"));
    cmd
}

#[test]
fn stats_seeded_db_shows_four_sections_and_offender_rows() {
    let xdg = tempdir().unwrap();
    let now_ms = 1_700_000_000_000_i64;
    let conn = init_db(xdg.path());
    // Unmatched offender (no rule).
    insert_invocation(&conn, now_ms, "/p/a", "make", None, 0, 5000, 5000, 0);
    // Filtered offender (matched rule).
    insert_invocation(&conn, now_ms, "/p/a", "cargo", Some("cargo-rule"), 0, 8000, 1200, 0);

    let assert = lacon(xdg.path()).arg("stats").assert().success();
    let out = assert.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);

    // D-15: section headers use the relabeled task-oriented strings.
    assert!(stdout.contains("Commands with no rule"), "missing section header; got:\n{stdout}");
    assert!(stdout.contains("Rule effectiveness"), "missing section header; got:\n{stdout}");
    assert!(stdout.contains("Bypass rates"), "missing section header; got:\n{stdout}");
    assert!(stdout.contains("Savings by project"), "missing section header; got:\n{stdout}");
    assert!(stdout.contains("make"), "expected unmatched offender row; got:\n{stdout}");
    assert!(stdout.contains("cargo"), "expected filtered offender row; got:\n{stdout}");
}

#[test]
fn stats_empty_db_prints_no_data_yet_and_succeeds() {
    let xdg = tempdir().unwrap();
    // No history.db created at all → fresh-machine state (D-03).
    let assert = lacon(xdg.path()).arg("stats").assert().success();
    let out = assert.get_output();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.to_lowercase().contains("no data yet"),
        "expected 'no data yet' on a fresh machine; got:\n{stdout}"
    );
}

#[test]
fn stats_project_filter_narrows_output() {
    let xdg = tempdir().unwrap();
    let now_ms = 1_700_000_000_000_i64;
    let conn = init_db(xdg.path());
    // Two unmatched offenders in different projects.
    insert_invocation(&conn, now_ms, "/p/keep", "ninja", None, 0, 4000, 4000, 0);
    insert_invocation(&conn, now_ms, "/p/drop", "scons", None, 0, 4000, 4000, 0);

    // Without filter: both visible.
    let all = lacon(xdg.path()).arg("stats").assert().success();
    let all_out = String::from_utf8_lossy(&all.get_output().stdout).to_string();
    assert!(all_out.contains("ninja") && all_out.contains("scons"), "both present unfiltered; got:\n{all_out}");

    // With --project /p/keep: only ninja's project remains.
    let filtered = lacon(xdg.path())
        .args(["stats", "--project", "/p/keep"])
        .assert()
        .success();
    let f_out = String::from_utf8_lossy(&filtered.get_output().stdout).to_string();
    assert!(f_out.contains("ninja"), "kept project's command present; got:\n{f_out}");
    assert!(!f_out.contains("scons"), "other project's command must be filtered out; got:\n{f_out}");
}

#[test]
fn stats_since_filter_narrows_output() {
    let xdg = tempdir().unwrap();
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    let conn = init_db(xdg.path());
    // One recent, one ancient (older than 7 days).
    insert_invocation(&conn, now_ms, "/p/x", "recentcmd", None, 0, 3000, 3000, 0);
    let ten_days_ago = now_ms - 10 * 86_400_000;
    insert_invocation(&conn, ten_days_ago, "/p/x", "ancientcmd", None, 0, 3000, 3000, 0);

    let filtered = lacon(xdg.path())
        .args(["stats", "--since", "7d"])
        .assert()
        .success();
    let f_out = String::from_utf8_lossy(&filtered.get_output().stdout).to_string();
    assert!(f_out.contains("recentcmd"), "recent command within window; got:\n{f_out}");
    assert!(!f_out.contains("ancientcmd"), "command older than 7d must be excluded; got:\n{f_out}");
}

#[test]
fn stats_invalid_since_errors_nonzero_no_panic() {
    let xdg = tempdir().unwrap();
    init_db(xdg.path());
    let assert = lacon(xdg.path())
        .args(["stats", "--since", "7x"])
        .assert()
        .failure();
    let stderr = String::from_utf8_lossy(&assert.get_output().stderr).to_string();
    assert!(
        stderr.to_lowercase().contains("since"),
        "expected a clear --since error; got:\n{stderr}"
    );
}

// ─── Phase 8 Plan 03 — ADR 0014 read-time presentation black-box tests ───────
//
// These follow the D-16 convention: targeted substring `contains(...)` checks on
// the captured stdout, NOT golden-file equality. Fixtures are seeded directly via
// the dev-only rusqlite (`insert_invocation`) and `.git` dirs are built with
// `std::fs` (no `git` binary). Non-ephemeral project paths live under
// `CARGO_TARGET_TMPDIR` (a per-test scratch dir under the workspace `target/`,
// which is NOT under any ephemeral prefix like `/tmp`), so the canonical-key
// rollup keeps them as repo-root / literal keys instead of collapsing to
// `(ephemeral)`.

/// A unique, non-ephemeral scratch directory under `CARGO_TARGET_TMPDIR`. Used
/// to seed project paths that must NOT collapse into the `(ephemeral)` bucket
/// (the OS tempdir `/tmp` IS ephemeral, so those fixtures cannot live there).
fn non_ephemeral_scratch(tag: &str) -> std::path::PathBuf {
    // Nanosecond suffix keeps parallel test runs from colliding.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("{tag}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Count non-overlapping occurrences of `needle` in `hay`.
fn count_occurrences(hay: &str, needle: &str) -> usize {
    hay.matches(needle).count()
}

// D-08: ≥3 distinct temp-rooted project paths must collapse into ONE
// `(ephemeral)` line, not three. We seed paths under `std::env::temp_dir()` so
// they hit the SAME ephemeral prefix set the binary builds.
#[test]
fn stats_ephemeral_paths_collapse_to_one_bucket() {
    let xdg = tempdir().unwrap();
    let now_ms = 1_700_000_000_000_i64;
    let conn = init_db(xdg.path());

    let tmp = std::env::temp_dir();
    let p1 = tmp.join("lacon-eph-a").to_string_lossy().into_owned();
    let p2 = tmp.join("lacon-eph-b").to_string_lossy().into_owned();
    let p3 = tmp.join("lacon-eph-c").to_string_lossy().into_owned();
    insert_invocation(&conn, now_ms, &p1, "cmd1", None, 0, 5000, 2000, 0);
    insert_invocation(&conn, now_ms, &p2, "cmd2", None, 0, 6000, 2000, 0);
    insert_invocation(&conn, now_ms, &p3, "cmd3", None, 0, 7000, 2000, 0);

    let assert = lacon(xdg.path()).arg("stats").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    assert_eq!(
        count_occurrences(&stdout, "(ephemeral)"),
        1,
        "≥3 temp-rooted projects must collapse to ONE (ephemeral) line; got:\n{stdout}"
    );
    // The individual temp paths must not appear in the project section.
    assert!(!stdout.contains("lacon-eph-a"), "individual temp path leaked; got:\n{stdout}");
    assert!(!stdout.contains("lacon-eph-b"), "individual temp path leaked; got:\n{stdout}");
    assert!(!stdout.contains("lacon-eph-c"), "individual temp path leaked; got:\n{stdout}");
}

// D-06/D-09: a repo (dir with a `.git/` subdir) and a subdir of it roll up into
// ONE line keyed at the repo root — both stored paths collapse.
#[test]
fn stats_git_dir_and_subdir_roll_into_one_repo() {
    let xdg = tempdir().unwrap();
    let now_ms = 1_700_000_000_000_i64;
    let conn = init_db(xdg.path());

    // Non-ephemeral repo fixture (under target/, not /tmp).
    let scratch = non_ephemeral_scratch("git-rollup");
    let repo = scratch.join("repo");
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    let sub = repo.join("sub");
    std::fs::create_dir_all(&sub).unwrap();

    let repo_str = repo.to_string_lossy().into_owned();
    let sub_str = sub.to_string_lossy().into_owned();
    insert_invocation(&conn, now_ms, &repo_str, "build", None, 0, 5000, 2000, 0);
    insert_invocation(&conn, now_ms, &sub_str, "test", None, 0, 6000, 2000, 0);

    let assert = lacon(xdg.path()).arg("stats").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    // The repo-root key appears exactly once in "Savings by project"; the subdir
    // path is NOT shown as its own line.
    assert_eq!(
        count_occurrences(&stdout, &repo_str),
        1,
        "repo + subdir must roll up to ONE repo-root line; got:\n{stdout}"
    );
    assert!(
        !stdout.contains(&sub_str),
        "the subdir path must collapse into the repo-root line; got:\n{stdout}"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

// D-11: 11 distinct non-ephemeral project paths → exactly 10 project rows + a
// "… more" hint mentioning --all. Each path is given its OWN `.git/` directory so
// it resolves to a DISTINCT repo-root key (the ancestor walk returns the first
// `.git` found), making the rollup keep 11 separate buckets regardless of where
// `CARGO_TARGET_TMPDIR` lives (which is itself inside the lacon repo).
#[test]
fn stats_top_n_caps_project_section_with_more_hint() {
    let xdg = tempdir().unwrap();
    let now_ms = 1_700_000_000_000_i64;
    let conn = init_db(xdg.path());

    // Non-ephemeral base; each projNN carries its own `.git` → distinct key.
    let scratch = non_ephemeral_scratch("topn-cap");
    let mut paths = Vec::new();
    for i in 0..11 {
        let p = scratch.join(format!("proj{i:02}"));
        std::fs::create_dir_all(p.join(".git")).unwrap();
        let s = p.to_string_lossy().into_owned();
        // Distinct byte_saved per row so ordering is stable.
        insert_invocation(&conn, now_ms, &s, "cmd", None, 0, 10_000 - i as i64, 1000, 0);
        paths.push(s);
    }

    let assert = lacon(xdg.path()).arg("stats").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    // Exactly 10 of the 11 seeded project paths are printed.
    let shown = paths.iter().filter(|p| stdout.contains(p.as_str())).count();
    assert_eq!(shown, 10, "exactly 10 project rows expected; got {shown}:\n{stdout}");
    assert!(stdout.contains("more"), "expected a '… more' hint; got:\n{stdout}");
    assert!(stdout.contains("--all"), "the hint must mention --all; got:\n{stdout}");

    let _ = std::fs::remove_dir_all(&scratch);
}

// D-12: --all uncaps the project section (all 11 rows) and drops the "… more"
// line, on the SAME 11-project seed.
#[test]
fn stats_all_flag_uncaps_and_drops_more_hint() {
    let xdg = tempdir().unwrap();
    let now_ms = 1_700_000_000_000_i64;
    let conn = init_db(xdg.path());

    // Each projNN carries its own `.git` → 11 distinct repo-root keys (see the
    // top-N test for why CARGO_TARGET_TMPDIR's in-repo location forces this).
    let scratch = non_ephemeral_scratch("all-uncap");
    let mut paths = Vec::new();
    for i in 0..11 {
        let p = scratch.join(format!("proj{i:02}"));
        std::fs::create_dir_all(p.join(".git")).unwrap();
        let s = p.to_string_lossy().into_owned();
        insert_invocation(&conn, now_ms, &s, "cmd", None, 0, 10_000 - i as i64, 1000, 0);
        paths.push(s);
    }

    let assert = lacon(xdg.path()).args(["stats", "--all"]).assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    let shown = paths.iter().filter(|p| stdout.contains(p.as_str())).count();
    assert_eq!(shown, 11, "--all must print all 11 project rows; got {shown}:\n{stdout}");
    assert!(
        !stdout.contains("more (use --project"),
        "--all must drop the '… more' hint; got:\n{stdout}"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

// D-14: a 22_800-byte total humanizes to "22.8 KB" by default; --bytes prints
// the exact integer "22800" and no "KB".
#[test]
fn stats_bytes_flag_prints_exact_integers() {
    let xdg = tempdir().unwrap();
    let now_ms = 1_700_000_000_000_i64;
    let conn = init_db(xdg.path());

    // raw 22_800, filtered 0 → saved 22_800 (humanizes to "22.8 KB").
    insert_invocation(&conn, now_ms, "/p/h", "humanize", None, 0, 22_800, 0, 0);

    let default = lacon(xdg.path()).arg("stats").assert().success();
    let d_out = String::from_utf8_lossy(&default.get_output().stdout).to_string();
    assert!(d_out.contains("22.8 KB"), "default run must humanize to 22.8 KB; got:\n{d_out}");

    let bytes = lacon(xdg.path()).args(["stats", "--bytes"]).assert().success();
    let b_out = String::from_utf8_lossy(&bytes.get_output().stdout).to_string();
    assert!(b_out.contains("22800"), "--bytes must print the exact integer; got:\n{b_out}");
    assert!(!b_out.contains("KB"), "--bytes must NOT humanize; got:\n{b_out}");
}

// D-05: the overall headline is printed BEFORE the first section header, carries
// a runs count and a saved percent, and excludes bypassed rows from its totals.
#[test]
fn stats_headline_prints_first_with_runs_and_saved() {
    let xdg = tempdir().unwrap();
    let now_ms = 1_700_000_000_000_i64;
    let conn = init_db(xdg.path());

    // Two counted rows + one bypassed row that must NOT enter the headline.
    insert_invocation(&conn, now_ms, "/p/a", "make", None, 0, 5000, 2000, 0);
    insert_invocation(&conn, now_ms, "/p/b", "cargo", Some("cargo-rule"), 0, 8000, 1200, 0);
    insert_invocation(&conn, now_ms, "/p/c", "skip", None, 0, 9999, 9999, 1);

    let assert = lacon(xdg.path()).arg("stats").assert().success();
    let stdout = String::from_utf8_lossy(&assert.get_output().stdout).to_string();

    // The headline precedes the first section header (string-index comparison).
    let headline_idx = stdout.find("Overall:").expect("a headline line must be printed");
    let first_section_idx = stdout
        .find("Commands with no rule")
        .expect("the first section header must be present");
    assert!(
        headline_idx < first_section_idx,
        "headline must appear BEFORE the first section header; got:\n{stdout}"
    );

    // Grab just the headline line and assert it carries runs + a saved percent.
    let headline = stdout.lines().find(|l| l.contains("Overall:")).unwrap();
    assert!(headline.contains("runs"), "headline must carry a runs count; got:\n{headline}");
    assert!(headline.contains('%'), "headline must carry a saved percent; got:\n{headline}");
    // The bypassed row's 9999 raw bytes are excluded: only 2 runs counted.
    assert!(headline.contains("2 runs"), "bypassed row must be excluded; got:\n{headline}");
}
