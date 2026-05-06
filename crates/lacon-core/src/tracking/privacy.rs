//! Privacy marker (D-14/D-15) + first-time-on warning (D-16).
//!
//! When `store_raw_outputs` resolves to true AND the marker file does not yet
//! exist at the resolved location, `warn_once_if_needed` prints the byte-stable
//! warning to stderr and atomically creates the marker. Second-and-subsequent
//! invocations short-circuit on marker presence.
//!
//! Race posture: `OpenOptions::create_new(true)` is the atomic primitive
//! [VERIFIED: doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.create_new].
//! No `Path::exists()` check — that creates a TOCTOU race when two `lacon run`
//! invocations start simultaneously. With `create_new`, exactly one process
//! gets `Ok`; all others get `AlreadyExists`.
//!
//! Note on D-16 warning text: the warning interpolates `<config-path>` and
//! `<marker-path>` only. The hardcoded `~/.local/share/lacon/history.db` stays
//! literal per RESEARCH "Note on D-16" — the user-facing text describes the
//! documented default location, not the resolved XDG path.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::TrackingError;

/// Marker filename at both project and user layers.
pub const MARKER_FILENAME: &str = ".store_raw_outputs_acked";

/// Resolve the (config_path, marker_path) tuple to use for the warning,
/// based on which layer turned `store_raw_outputs` ON. Per CONTEXT D-14:
/// project layer wins when both are true; user layer is the fallback.
/// Returns `None` when neither layer enables it (bundled default is false,
/// so no marker is meaningful).
pub fn resolve_marker_path(
    project_root: Option<&Path>,
    user_config_dir: Option<&Path>,
    project_store_raw: bool,
    user_store_raw: bool,
) -> Option<(PathBuf, PathBuf)> {
    if project_store_raw {
        let root = project_root?;
        let cfg = root.join(".lacon").join("config.yaml");
        let marker = root.join(".lacon").join(MARKER_FILENAME);
        Some((cfg, marker))
    } else if user_store_raw {
        let dir = user_config_dir?;
        let cfg = dir.join("config.yaml");
        let marker = dir.join(MARKER_FILENAME);
        Some((cfg, marker))
    } else {
        None
    }
}

/// First-time-on stderr warning. Byte-stable per CONTEXT D-16 except for
/// the `<config-path>` and `<marker-path>` interpolation — everything else
/// is literal. Atomic marker creation via `create_new(true)`.
///
/// Best-effort: if the stderr write fails, the marker is still created
/// (we already won the race) so the notice will not repeat.
///
/// # Errors
/// `TrackingError::Marker` if the marker file cannot be created for any
/// reason OTHER than `AlreadyExists` (which is the silent-success path).
pub fn warn_once_if_needed(
    config_path: &Path,
    marker_path: &Path,
) -> Result<(), TrackingError> {
    // Ensure the parent dir exists; the project case relies on it being there
    // already (the project's .lacon/ dir was created by Plan 06 / lacon init in
    // Phase 3, but config.yaml's existence implies it). For the user-config dir
    // case, etcetera created it. We do NOT mkdir-p here — that's the caller's
    // job; if the parent doesn't exist we surface the io error as TrackingError::Marker.

    match marker_open_create_new(marker_path) {
        Ok(()) => {
            let warning = format_warning(config_path, marker_path);
            let _ = std::io::stderr().write_all(warning.as_bytes());
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(e) => Err(TrackingError::Marker {
            path: marker_path.to_owned(),
            source: e,
        }),
    }
}

/// Atomic marker create. Unix-only mode bits are belt-and-suspenders;
/// the parent dir is 0700 either way.
#[cfg(unix)]
fn marker_open_create_new(marker_path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(marker_path)
        .map(|_| ())
}

#[cfg(not(unix))]
fn marker_open_create_new(marker_path: &Path) -> std::io::Result<()> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(marker_path)
        .map(|_| ())
}

/// Build the byte-stable D-16 warning string.
/// Public-in-crate so tests can assert byte-exact output.
pub(crate) fn format_warning(config_path: &Path, marker_path: &Path) -> String {
    format!(
        "lacon: store_raw_outputs is enabled.\n\
         lacon: raw stdout/stderr will be retained at ~/.local/share/lacon/history.db\n\
         lacon: for up to 3 days. Disable in {} or run `rm` on the DB.\n\
         lacon: this notice is shown once per project (marker: {}).\n",
        config_path.display(),
        marker_path.display(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn format_warning_byte_exact_template() {
        let cfg = PathBuf::from("/proj/.lacon/config.yaml");
        let marker = PathBuf::from("/proj/.lacon/.store_raw_outputs_acked");
        let s = format_warning(&cfg, &marker);
        // Each line literal, in order. Concatenation rather than a single string
        // literal so this test catches reordering as well as edits.
        let expected = String::new()
            + "lacon: store_raw_outputs is enabled.\n"
            + "lacon: raw stdout/stderr will be retained at ~/.local/share/lacon/history.db\n"
            + "lacon: for up to 3 days. Disable in /proj/.lacon/config.yaml or run `rm` on the DB.\n"
            + "lacon: this notice is shown once per project (marker: /proj/.lacon/.store_raw_outputs_acked).\n";
        assert_eq!(s, expected);
    }

    #[test]
    fn resolve_marker_project_wins_over_user() {
        let proj = PathBuf::from("/proj");
        let user = PathBuf::from("/home/u/.config/lacon");
        let r = resolve_marker_path(Some(&proj), Some(&user), true, true);
        let (cfg, marker) = r.expect("project layer present");
        assert_eq!(cfg, PathBuf::from("/proj/.lacon/config.yaml"));
        assert_eq!(marker, PathBuf::from("/proj/.lacon/.store_raw_outputs_acked"));
    }

    #[test]
    fn resolve_marker_falls_back_to_user_when_project_off() {
        let proj = PathBuf::from("/proj");
        let user = PathBuf::from("/home/u/.config/lacon");
        let r = resolve_marker_path(Some(&proj), Some(&user), false, true);
        let (cfg, marker) = r.expect("user layer present");
        assert_eq!(cfg, PathBuf::from("/home/u/.config/lacon/config.yaml"));
        assert_eq!(marker, PathBuf::from("/home/u/.config/lacon/.store_raw_outputs_acked"));
    }

    #[test]
    fn resolve_marker_returns_none_when_both_off() {
        let proj = PathBuf::from("/proj");
        let user = PathBuf::from("/home/u/.config/lacon");
        assert!(resolve_marker_path(Some(&proj), Some(&user), false, false).is_none());
    }
}
