//! Integration tests for tracking::privacy: marker creation, idempotent re-call,
//! parent-dir-missing error path, race-free atomic create.
//!
//! Public API tested via `lacon_core::tracking::privacy` since the helpers
//! are `pub fn` (not `pub use` re-exported) — a future API stabilization
//! pass may move them to `tracking::*`, but for v1 they live under the
//! sub-module path.

use std::path::PathBuf;

use lacon_core::error::TrackingError;
use lacon_core::tracking::privacy::{warn_once_if_needed, MARKER_FILENAME};

fn setup_tempdir() -> tempfile::TempDir {
    tempfile::TempDir::new().unwrap()
}

#[test]
fn warning_creates_marker_first_time() {
    let tmp = setup_tempdir();
    let cfg = tmp.path().join("config.yaml");
    let marker = tmp.path().join(MARKER_FILENAME);

    // First call: marker absent → create + warn.
    warn_once_if_needed(&cfg, &marker).expect("first call succeeds");
    assert!(marker.exists(), "marker file created on first call");
}

#[test]
fn warning_prints_once_then_marker_silent_on_second_call() {
    let tmp = setup_tempdir();
    let cfg = tmp.path().join("config.yaml");
    let marker = tmp.path().join(MARKER_FILENAME);

    warn_once_if_needed(&cfg, &marker).expect("first call ok");
    // Second call: marker present → AlreadyExists → silent Ok.
    warn_once_if_needed(&cfg, &marker).expect("second call ok (silent)");
    assert!(marker.exists());
}

#[test]
fn pre_existing_marker_is_silent_ok() {
    let tmp = setup_tempdir();
    let cfg = tmp.path().join("config.yaml");
    let marker = tmp.path().join(MARKER_FILENAME);

    // Pre-create marker as if a previous invocation wrote it.
    std::fs::write(&marker, b"").unwrap();

    warn_once_if_needed(&cfg, &marker).expect("pre-existing marker → Ok");
}

#[test]
fn missing_parent_dir_yields_marker_error() {
    let tmp = setup_tempdir();
    // Construct a marker path under a nonexistent subdir.
    let cfg = tmp.path().join("nonexistent_dir/config.yaml");
    let marker = tmp.path().join("nonexistent_dir/").join(MARKER_FILENAME);

    let err = warn_once_if_needed(&cfg, &marker)
        .expect_err("missing parent dir should fail");
    match err {
        TrackingError::Marker { path, source: _ } => {
            assert_eq!(path, PathBuf::from(&marker), "error carries the marker path");
        }
        other => panic!("expected TrackingError::Marker, got {:?}", other),
    }
}

#[test]
fn concurrent_calls_at_most_one_creates() {
    // Smoke test for RESEARCH §"Privacy Marker File Semantics" race claim.
    // Spawns two threads that both attempt warn_once_if_needed; both must
    // return Ok (one creates the file, the other observes AlreadyExists),
    // and the marker must exist at the end.
    let tmp = setup_tempdir();
    let cfg = tmp.path().join("config.yaml");
    let marker = tmp.path().join(MARKER_FILENAME);

    let cfg_a = cfg.clone();
    let marker_a = marker.clone();
    let cfg_b = cfg.clone();
    let marker_b = marker.clone();

    let h_a = std::thread::spawn(move || warn_once_if_needed(&cfg_a, &marker_a));
    let h_b = std::thread::spawn(move || warn_once_if_needed(&cfg_b, &marker_b));

    let r_a = h_a.join().unwrap();
    let r_b = h_b.join().unwrap();
    assert!(r_a.is_ok(), "thread A: {:?}", r_a);
    assert!(r_b.is_ok(), "thread B: {:?}", r_b);
    assert!(marker.exists());
}
