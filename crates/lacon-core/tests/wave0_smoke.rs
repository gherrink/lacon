//! Wave 0 smoke tests for Phase 1.
//!
//! These tests exist to settle two open questions from RESEARCH.md before
//! PLAN-03 commits to the loader design:
//!
//! 1. Does `serde-saphyr` 0.0.26 expose a `Value` type adequate for the
//!    `lacon validate` content-dispatch check (top-level `id` AND `match`)?
//!    **FINDING:** serde-saphyr 0.0.26 does NOT expose `serde_saphyr::Value`.
//!    It is a pure typed-serde layer with no generic Value enum. The dispatch
//!    path in PLAN-03 must use the fallback: a partial struct with
//!    `Option<serde::de::IgnoredAny>` for the keys of interest (id + match).
//!    This approach is validated in `smoke_serde_saphyr_value_dispatch` below.
//!    See PLAN-03 acceptance — `serde_saphyr::Value` is NOT available.
//!
//! 2. Does `starlark` 0.13 compile under the workspace MSRV of 1.80?
//!    **FINDING:** Yes — verified by the smoke test below.

use serde::Deserialize;

/// Partial top-level-key probe used for D-17 content dispatch.
///
/// PLAN-03 will use this exact pattern in `lacon-core::validate` to decide
/// whether a YAML file is a rule file (has `id` AND `match`) or a config
/// file. `serde::de::IgnoredAny` lets us detect key presence without
/// deserializing the value — zero allocation, no schema coupling.
#[derive(Deserialize)]
struct TopLevelKeyProbe {
    id: Option<serde::de::IgnoredAny>,
    #[serde(rename = "match")]
    match_key: Option<serde::de::IgnoredAny>,
}

#[test]
fn smoke_serde_saphyr_value_dispatch() {
    // Mimics the D-17 `lacon validate` dispatch: parse YAML to a typed
    // probe struct, look for top-level `id` AND `match`. If both present →
    // rule file. Otherwise → config file.
    //
    // NOTE: serde-saphyr 0.0.26 does NOT expose serde_saphyr::Value.
    // The fallback path (TopLevelKeyProbe with IgnoredAny) is validated here.
    // PLAN-03 must use this pattern — not a generic Value type.

    let rule_yaml = r#"
id: example
match:
  command: echo
pipeline:
  - strip_ansi
"#;
    let config_yaml = r#"
defaults:
  max_bytes: 16384
"#;

    // Use typed partial struct with IgnoredAny for content dispatch.
    // This is the D-17 fallback confirmed by PLAN-01 Wave 0.
    let rule_probe: TopLevelKeyProbe = serde_saphyr::from_str(rule_yaml)
        .expect("rule yaml parses with TopLevelKeyProbe");
    let config_probe: TopLevelKeyProbe = serde_saphyr::from_str(config_yaml)
        .expect("config yaml parses with TopLevelKeyProbe");

    assert!(
        rule_probe.id.is_some(),
        "rule YAML must expose top-level `id` via TopLevelKeyProbe"
    );
    assert!(
        rule_probe.match_key.is_some(),
        "rule YAML must expose top-level `match` via TopLevelKeyProbe"
    );
    assert!(
        config_probe.id.is_none(),
        "config YAML must NOT expose top-level `id` (dispatch hinges on this)"
    );
}

#[test]
fn smoke_starlark_module_parses() {
    // Confirms starlark 0.13 compiles + can parse a trivial `process`
    // function body under the workspace MSRV. PLAN-04 builds on this.
    use starlark::syntax::{AstModule, Dialect};

    let src = r#"
def process(ctx, lines):
    return lines
"#;
    let _ast = AstModule::parse("smoke.star", src.to_owned(), &Dialect::Standard)
        .expect("trivial process() parses under starlark 0.13");
}

// ---------------------------------------------------------------------------
// Phase 4 Wave 0: strict read-only open of a WAL `history.db`
// ---------------------------------------------------------------------------
//
// Open Question 1 (04-RESEARCH.md §"Open Questions"): does
// `SQLITE_OPEN_READ_ONLY` succeed against a `history.db` that `Tracker::open`
// has put into WAL mode? Pitfall 1 warns that strict read-only may fail
// because WAL needs shared-memory (`-shm`/`-wal`) coordination that a pure
// read-only handle cannot create. The outcome of this spike GATES Plan 04
// Task 2's open-flag choice:
//   - strict READ_ONLY ok  → open_readonly uses SQLITE_OPEN_READ_ONLY
//   - SQLITE_CANTOPEN/etc. → open_readonly uses the D-02 fallback
//     (SQLITE_OPEN_READ_WRITE without CREATE, still no migrate/no prune)
//
// FINDING (this build: rusqlite 0.39 / libsqlite3-sys 0.37, Linux/ext4,
// 2026-05-22): strict `SQLITE_OPEN_READ_ONLY` SUCCEEDS. After the writer
// Tracker is dropped (WAL checkpointed on close), a fresh read-only handle
// runs `SELECT 1` and `SELECT COUNT(*) FROM v_unmatched_offenders` without
// error. Plan 04 Task 2 therefore uses SQLITE_OPEN_READ_ONLY (the simpler
// path), NOT the fallback.

#[test]
fn smoke_readonly_open_of_wal_db() {
    use lacon_core::config::Retention;
    use lacon_core::runtime::{ByteCounts, InvocationMeta};
    use lacon_core::tracking::Tracker;
    use rusqlite::{Connection, OpenFlags};

    const FIXED_NOW_MS: u64 = 1_700_000_000_000;

    let retention = Retention {
        invocations_days: 30,
        raw_outputs_days: 3,
    };

    // (1) Create a history.db in a tempdir and write at least one row through
    //     the WRITE path (Tracker::open → WAL mode + migrate; Tracker::record
    //     → one invocations row). Mirrors tracking_record.rs seeding.
    let tmp = tempfile::TempDir::new().unwrap();
    let db_path = tmp.path().join("data").join("lacon").join("history.db");

    {
        let tracker = Tracker::open(&db_path, &retention, false, FIXED_NOW_MS)
            .expect("Tracker::open seeds a WAL history.db");

        let meta = InvocationMeta {
            ts_unix_ms: FIXED_NOW_MS,
            rule_id: None, // unmatched → shows in v_unmatched_offenders
            rule_source: None,
            command_raw: "pnpm install".to_string(),
            argv: vec!["pnpm".into(), "install".into()],
            exit_code: 0,
            duration_ms: 42,
            byte_counts: ByteCounts {
                raw_stdout_bytes: 1234,
                raw_stderr_bytes: 0,
                filtered_bytes: 567,
            },
            bypassed: false,
            rewritten: false,
            truncated_by_max_bytes: false,
            assistant: "claude-code".to_string(),
            session_id: Some("sess-xyz".to_string()),
            project_path: Some(std::path::PathBuf::from("/proj")),
            command_normalized: "pnpm install".to_string(),
            raw_output_id: None,
        };
        tracker
            .record(&meta, None, None, None, false, false)
            .expect("seed one invocations row");

        // (2) Drop the Tracker → closes the WAL writer connection.
    }

    // (3) Reopen strictly read-only. We DELIBERATELY do NOT issue
    //     `PRAGMA journal_mode=WAL` here — that is a write and would error on
    //     a read-only handle (Pitfall 1).
    let open_result = Connection::open_with_flags(
        &db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    );

    match open_result {
        Ok(conn) => {
            // (4) Run SELECT 1 and a view query against the read-only handle.
            let one: i64 = conn
                .query_row("SELECT 1", [], |r| r.get(0))
                .expect("SELECT 1 on read-only WAL handle");
            assert_eq!(one, 1);

            let view_count: i64 = conn
                .query_row("SELECT COUNT(*) FROM v_unmatched_offenders", [], |r| {
                    r.get(0)
                })
                .expect("view query on read-only WAL handle");
            assert_eq!(
                view_count, 1,
                "the seeded unmatched (rule_id IS NULL) row appears in v_unmatched_offenders"
            );

            println!(
                "WAVE0 read-only WAL open OUTCOME: strict SQLITE_OPEN_READ_ONLY OK \
                 (SELECT 1 + view query succeeded). Plan 04 Task 2 uses SQLITE_OPEN_READ_ONLY."
            );
        }
        Err(e) => {
            // Document the fallback path explicitly so the gate decision is
            // visible in test output even if this build needs the fallback.
            println!(
                "WAVE0 read-only WAL open OUTCOME: strict SQLITE_OPEN_READ_ONLY FAILED ({e}). \
                 Plan 04 Task 2 must use the D-02 fallback \
                 (SQLITE_OPEN_READ_WRITE without CREATE, no migrate/no prune)."
            );
            // The fallback open (read-write, no CREATE) must still read the DB.
            let conn = Connection::open_with_flags(
                &db_path,
                OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
            )
            .expect("D-02 fallback: read-write (no CREATE) open succeeds");
            let one: i64 = conn
                .query_row("SELECT 1", [], |r| r.get(0))
                .expect("SELECT 1 on fallback handle");
            assert_eq!(one, 1);
        }
    }
}
