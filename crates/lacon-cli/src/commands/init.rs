//! `lacon init` subcommand: opt a project into lacon (REQ-cli-init).
//!
//! Creates three things in the current working directory, idempotently:
//!
//! 1. **`.lacon/` skeleton** — an empty project rules directory with a
//!    `.gitkeep` so it survives `git clone`. Rule files are created lazily by
//!    the user; we do NOT pre-create `.lacon/rules/` (Phase 1's loader handles
//!    a missing dir identically to an empty one).
//! 2. **`.claude/settings.json` PreToolUse(Bash) hook** — installs (or
//!    refreshes) the lacon-managed `lacon-claude-hook` entry inside
//!    `hooks.PreToolUse[]` using the array-of-matchers shape (D-11). The file is
//!    parsed as a `serde_json::Value`, so unrelated user config (top-level keys
//!    like `model`/`theme`, non-Bash matcher groups, and user-authored Bash
//!    hooks) is preserved untouched (D-28, T-settings-clobber). The walk uses
//!    the command-string itself (`starts_with("lacon-claude-hook")`) as the
//!    lacon-managed fingerprint (D-12) — scrub-then-reinsert guarantees a
//!    byte-stable result across runs (idempotency, D-28). The write is atomic
//!    via `tempfile::NamedTempFile::persist` (POSIX rename(2)) so a concurrent
//!    `claude` startup never observes a half-written file (D-13).
//! 3. **`CLAUDE.md` note** — appends or refreshes a
//!    `<!-- lacon:start -->…<!-- lacon:end -->` HTML-comment block (D-14)
//!    mentioning the `!!` bypass prefix and `LACON_DISABLE=1`. HTML comments
//!    survive every markdown renderer, so detection is a plain string scan.

use std::path::Path;

use serde_json::{json, Value};

const LACON_START: &str = "<!-- lacon:start -->";
const LACON_END: &str = "<!-- lacon:end -->";

/// CLAUDE.md note body (D-14). MUST mention `!!` and `LACON_DISABLE=1` so the
/// user trust property holds. Exact phrasing is the author's discretion.
const BLOCK_BODY: &str = "Bash output is filtered by lacon to reduce token usage. Bypass one command \
with the `!!` prefix (e.g., `!! pnpm test`). Disable filtering entirely with `LACON_DISABLE=1`.";

/// Entry point dispatched from `cli.rs`'s `Init` variant.
///
/// Returns `Ok(0)` on success, `Ok(1)` on a recoverable user/IO error (with a
/// `lacon init:`-prefixed stderr message), mirroring the `validate` convention.
pub fn execute() -> anyhow::Result<i32> {
    let cwd = std::env::current_dir()?;

    // Step A: `.lacon/` skeleton (+ .gitkeep so the dir survives `git clone`).
    let lacon_dir = cwd.join(".lacon");
    std::fs::create_dir_all(&lacon_dir)?;
    std::fs::write(lacon_dir.join(".gitkeep"), b"")?;

    // Step B: `.claude/settings.json` walk — install/refresh the lacon hook.
    let settings_path = cwd.join(".claude").join("settings.json");
    let mut settings = match std::fs::read_to_string(&settings_path) {
        Ok(text) => match serde_json::from_str::<Value>(&text) {
            Ok(value) => value,
            Err(e) => {
                eprintln!(
                    "lacon init: failed to parse .claude/settings.json: {e}"
                );
                return Ok(1);
            }
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(e) => {
            eprintln!("lacon init: failed to read .claude/settings.json: {e}");
            return Ok(1);
        }
    };
    // Defensive: if the file parsed to a non-object (e.g. a bare array or
    // scalar), it is not a valid Claude Code settings file. Refuse rather than
    // clobber.
    if !settings.is_object() {
        eprintln!(
            "lacon init: .claude/settings.json is not a JSON object; \
             refusing to overwrite"
        );
        return Ok(1);
    }
    install_lacon_hook(&mut settings);
    if let Err(e) = atomic_write_json(&settings_path, &settings) {
        eprintln!("lacon init: failed to write .claude/settings.json: {e}");
        return Ok(1);
    }

    // Step C: `CLAUDE.md` walk — append/refresh the marker block.
    let claude_md_path = cwd.join("CLAUDE.md");
    let existing_md = match std::fs::read_to_string(&claude_md_path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            eprintln!("lacon init: failed to read CLAUDE.md: {e}");
            return Ok(1);
        }
    };
    let new_md = install_claude_md_block(&existing_md, BLOCK_BODY);
    if let Err(e) = std::fs::write(&claude_md_path, new_md) {
        eprintln!("lacon init: failed to write CLAUDE.md: {e}");
        return Ok(1);
    }

    println!(
        "lacon init: installed (.lacon/, .claude/settings.json hook, CLAUDE.md note)"
    );
    Ok(0)
}

/// Install (or refresh) the lacon-managed `PreToolUse(Bash)` hook entry inside
/// `settings.hooks.PreToolUse[]` (D-11, D-12, D-28).
///
/// Scrub-then-reinsert: existing lacon entries (fingerprinted by command-string
/// prefix) are stripped, then exactly one canonical entry is appended. This is
/// what makes re-running `lacon init` byte-stable (idempotency) while leaving
/// every user-authored hook and every non-Bash matcher group untouched.
fn install_lacon_hook(settings: &mut Value) {
    // Ensure path: settings.hooks.PreToolUse exists and is an array.
    let hooks = settings
        .as_object_mut()
        .expect("settings is object")
        .entry("hooks")
        .or_insert_with(|| json!({}));
    // If `hooks` exists but is not an object (malformed user file), replace it
    // with a fresh object so the walk can proceed without panicking.
    if !hooks.is_object() {
        *hooks = json!({});
    }
    let pretool = hooks
        .as_object_mut()
        .expect("hooks is object")
        .entry("PreToolUse")
        .or_insert_with(|| json!([]));
    if !pretool.is_array() {
        *pretool = json!([]);
    }
    let pretool_arr = pretool.as_array_mut().expect("PreToolUse is array");

    // Phase 1 (scrub): for each Bash matcher-group, drop inner hooks whose
    // command starts with the lacon fingerprint.
    for group in pretool_arr.iter_mut() {
        if group.get("matcher").and_then(Value::as_str) != Some("Bash") {
            continue;
        }
        let Some(inner) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
            continue;
        };
        inner.retain(|h| {
            let cmd = h.get("command").and_then(Value::as_str).unwrap_or("");
            !cmd.starts_with("lacon-claude-hook")
        });
    }

    // Phase 2 (cleanup): remove now-empty Bash matcher groups so the file stays
    // tidy. Non-Bash groups and non-empty Bash groups are kept.
    pretool_arr.retain(|group| {
        let is_bash = group.get("matcher").and_then(Value::as_str) == Some("Bash");
        if !is_bash {
            return true;
        }
        group
            .get("hooks")
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty())
    });

    // Phase 3 (insert): append the canonical lacon entry.
    pretool_arr.push(json!({
        "matcher": "Bash",
        "hooks": [
            { "type": "command", "command": "lacon-claude-hook" }
        ]
    }));
}

/// Append or refresh the lacon CLAUDE.md note block (D-14).
///
/// - Both markers present and ordered → replace the span between them in place,
///   preserving all surrounding content.
/// - Exactly one marker present, or markers in corrupt order (orphan state) →
///   strip the orphan marker token(s) so the file returns to a clean state,
///   warn on stderr, then append a fresh well-formed block at EOF. Stripping
///   first (WR-04) guarantees convergence: a subsequent run sees exactly one
///   well-formed pair and takes the in-place-replace branch instead of accreting
///   another block (and never clobbers user content sandwiched in between).
/// - Neither marker present → append a fresh block at EOF.
fn install_claude_md_block(existing: &str, block_body: &str) -> String {
    let start_idx = existing.find(LACON_START);
    let end_idx = existing.find(LACON_END);

    match (start_idx, end_idx) {
        (Some(s), Some(e)) if s < e => {
            let end_inclusive = e + LACON_END.len();
            let mut out = String::with_capacity(existing.len());
            out.push_str(&existing[..s]);
            out.push_str(LACON_START);
            out.push('\n');
            out.push_str(block_body);
            out.push('\n');
            out.push_str(LACON_END);
            out.push_str(&existing[end_inclusive..]);
            out
        }
        (Some(_), None) | (None, Some(_)) | (Some(_), Some(_)) => {
            // (Some, Some) with start >= end is also a corrupt ordering; treat
            // it like the orphan-marker case. Strip the orphan marker token(s)
            // first so re-runs converge to a single well-formed block (WR-04).
            eprintln!(
                "lacon init: warning — CLAUDE.md has an unmatched lacon marker; \
                 removing the stray marker and appending a fresh block at EOF"
            );
            let cleaned = strip_lacon_markers(existing);
            append_fresh_block(&cleaned, block_body)
        }
        (None, None) => append_fresh_block(existing, block_body),
    }
}

/// Remove every `<!-- lacon:start -->` / `<!-- lacon:end -->` marker token from
/// `text`, leaving the surrounding (user) content intact. Used by the orphan
/// recovery path (WR-04) to scrub stray markers before appending a fresh block,
/// so repeated `lacon init` runs converge instead of accreting blocks.
///
/// Also drops a marker's own trailing newline (so removing a marker that sat on
/// its own line does not leave a blank line behind), but never touches adjacent
/// user content.
fn strip_lacon_markers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let next_start = rest.find(LACON_START);
        let next_end = rest.find(LACON_END);
        // Find the nearest marker (start or end), whichever comes first.
        let (pos, marker_len) = match (next_start, next_end) {
            (Some(s), Some(e)) if s <= e => (s, LACON_START.len()),
            (Some(_), Some(e)) => (e, LACON_END.len()),
            (Some(s), None) => (s, LACON_START.len()),
            (None, Some(e)) => (e, LACON_END.len()),
            (None, None) => {
                out.push_str(rest);
                break;
            }
        };
        out.push_str(&rest[..pos]);
        let mut after = pos + marker_len;
        // Consume the marker's own trailing newline so a marker that occupied a
        // whole line does not leave an empty line behind.
        if rest.as_bytes().get(after) == Some(&b'\n') {
            after += 1;
        }
        rest = &rest[after..];
    }
    out
}

/// Append a fresh marker block at EOF, ensuring a clean newline boundary and a
/// blank line of visual separation from existing content.
fn append_fresh_block(existing: &str, block_body: &str) -> String {
    let mut out = String::with_capacity(existing.len() + 256);
    out.push_str(existing);
    if !existing.is_empty() && !existing.ends_with('\n') {
        out.push('\n');
    }
    if !existing.is_empty() {
        out.push('\n'); // visual separation
    }
    out.push_str(LACON_START);
    out.push('\n');
    out.push_str(block_body);
    out.push('\n');
    out.push_str(LACON_END);
    out.push('\n');
    out
}

/// Write `value` to `path` atomically (D-13).
///
/// Creates the parent directory (`.claude/`) if missing, serializes with
/// 2-space pretty indent + trailing newline (Claude Code's conventional style),
/// then `persist`es a same-directory tempfile via POSIX rename(2) — atomic on
/// macOS + Linux.
///
/// WR-03: `persist` keeps the *tempfile's* mode (`0600`), not the destination's.
/// To avoid silently narrowing a pre-existing `settings.json`'s permissions
/// (e.g. a group-readable file on a shared box), the original mode is read and
/// re-applied to the tempfile before `persist` when the destination exists.
fn atomic_write_json(path: &Path, value: &Value) -> anyhow::Result<()> {
    use std::io::Write;

    let parent = path
        .parent()
        .expect(".claude/settings.json has a parent directory");
    std::fs::create_dir_all(parent)?;

    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    let bytes = serde_json::to_vec_pretty(value)?;
    tmp.write_all(&bytes)?;
    tmp.write_all(b"\n")?;
    tmp.flush()?;

    // Preserve the destination's existing permissions across the atomic replace
    // (WR-03). Unix-only: v1 targets macOS + Linux, and a Unix mode is the only
    // permission concept the spec cares about. Best-effort — a metadata read
    // failure must not abort the (more important) atomic write.
    #[cfg(unix)]
    {
        if let Ok(meta) = std::fs::metadata(path) {
            let perms = meta.permissions();
            // Apply the original file's mode to the tempfile so `persist` (which
            // keeps the tempfile's perms) lands the file with the original mode.
            let _ = std::fs::set_permissions(tmp.path(), perms);
        }
    }

    tmp.persist(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bash_lacon_commands(settings: &Value) -> Vec<String> {
        settings["hooks"]["PreToolUse"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|g| g["matcher"] == "Bash")
            .flat_map(|g| g["hooks"].as_array().into_iter().flatten())
            .filter_map(|h| h["command"].as_str())
            .filter(|c| c.starts_with("lacon-claude-hook"))
            .map(String::from)
            .collect()
    }

    #[test]
    fn install_hook_into_empty_object_adds_one_entry() {
        let mut settings = json!({});
        install_lacon_hook(&mut settings);
        assert_eq!(bash_lacon_commands(&settings), vec!["lacon-claude-hook"]);
    }

    #[test]
    fn install_hook_is_idempotent() {
        let mut settings = json!({});
        install_lacon_hook(&mut settings);
        let after_one = settings.clone();
        install_lacon_hook(&mut settings);
        assert_eq!(after_one, settings, "second install is a structural no-op");
        assert_eq!(bash_lacon_commands(&settings).len(), 1);
    }

    #[test]
    fn install_hook_preserves_non_bash_and_user_bash_hooks() {
        let mut settings = json!({
            "model": "claude-opus-4",
            "hooks": {
                "PreToolUse": [
                    { "matcher": "Edit", "hooks": [{ "type": "command", "command": "my-edit-hook.sh" }] },
                    { "matcher": "Bash", "hooks": [{ "type": "command", "command": "my-bash-formatter.sh" }] }
                ]
            }
        });
        install_lacon_hook(&mut settings);

        // Top-level key untouched.
        assert_eq!(settings["model"], "claude-opus-4");
        // Edit matcher untouched.
        let pretool = settings["hooks"]["PreToolUse"].as_array().unwrap();
        let edit = pretool.iter().find(|g| g["matcher"] == "Edit").unwrap();
        assert_eq!(edit["hooks"][0]["command"], "my-edit-hook.sh");
        // User's Bash formatter preserved AND lacon hook added.
        let all_bash: Vec<&str> = pretool
            .iter()
            .filter(|g| g["matcher"] == "Bash")
            .flat_map(|g| g["hooks"].as_array().unwrap().iter())
            .filter_map(|h| h["command"].as_str())
            .collect();
        assert!(all_bash.contains(&"my-bash-formatter.sh"));
        assert!(all_bash.contains(&"lacon-claude-hook"));
    }

    #[test]
    fn install_hook_collapses_drifted_lacon_entries_to_one() {
        let mut settings = json!({
            "hooks": {
                "PreToolUse": [
                    { "matcher": "Bash", "hooks": [
                        { "type": "command", "command": "lacon-claude-hook" },
                        { "type": "command", "command": "lacon-claude-hook --debug" }
                    ]}
                ]
            }
        });
        install_lacon_hook(&mut settings);
        assert_eq!(bash_lacon_commands(&settings), vec!["lacon-claude-hook"]);
    }

    #[test]
    fn claude_md_appends_block_to_empty() {
        let out = install_claude_md_block("", BLOCK_BODY);
        assert!(out.contains(LACON_START));
        assert!(out.contains(LACON_END));
        assert!(out.contains("!!"));
        assert!(out.contains("LACON_DISABLE"));
    }

    #[test]
    fn claude_md_is_idempotent() {
        let first = install_claude_md_block("# My Project\n\nSome notes.\n", BLOCK_BODY);
        let second = install_claude_md_block(&first, BLOCK_BODY);
        assert_eq!(first, second, "second pass replaces the block in place");
        // Pre-existing content survives.
        assert!(second.starts_with("# My Project"));
        assert!(second.contains("Some notes."));
    }

    #[test]
    fn claude_md_orphan_marker_strips_stray_and_appends_fresh() {
        let existing = "# Project\n\n<!-- lacon:start -->\nstale\n";
        let out = install_claude_md_block(existing, BLOCK_BODY);
        // User content survives; the orphan START marker is stripped (WR-04) so
        // the file has exactly one well-formed pair afterward.
        assert!(out.contains("# Project"));
        assert!(out.contains("stale"));
        assert_eq!(
            out.matches(LACON_START).count(),
            1,
            "exactly one start marker after recovery, got: {out}"
        );
        assert_eq!(
            out.matches(LACON_END).count(),
            1,
            "exactly one end marker after recovery, got: {out}"
        );
    }

    #[test]
    fn claude_md_orphan_marker_recovery_is_idempotent() {
        // WR-04: the old code accreted a block on every run and could clobber
        // content between the orphan and the appended block. Recovery must now
        // converge to a stable file across repeated runs.
        let existing = "# Project\n\n<!-- lacon:start -->\nstale\n";
        let first = install_claude_md_block(existing, BLOCK_BODY);
        let second = install_claude_md_block(&first, BLOCK_BODY);
        assert_eq!(
            first, second,
            "orphan recovery must converge (idempotent across runs)"
        );
        // And it must stay a single well-formed block, not accrete.
        assert_eq!(second.matches(LACON_START).count(), 1);
        assert_eq!(second.matches(LACON_END).count(), 1);
        // User content preserved.
        assert!(second.contains("# Project"));
        assert!(second.contains("stale"));
    }

    #[test]
    fn claude_md_orphan_end_marker_recovers() {
        // An orphan END marker (the other half) is also stripped + recovered.
        let existing = "# Project\n\n<!-- lacon:end -->\n";
        let out = install_claude_md_block(existing, BLOCK_BODY);
        assert_eq!(out.matches(LACON_START).count(), 1);
        assert_eq!(out.matches(LACON_END).count(), 1);
        let again = install_claude_md_block(&out, BLOCK_BODY);
        assert_eq!(out, again, "end-marker recovery is idempotent too");
    }

    #[test]
    fn strip_lacon_markers_removes_both_kinds() {
        let text = "before\n<!-- lacon:start -->\nmid\n<!-- lacon:end -->\nafter\n";
        let cleaned = strip_lacon_markers(text);
        assert!(!cleaned.contains(LACON_START));
        assert!(!cleaned.contains(LACON_END));
        assert!(cleaned.contains("before"));
        assert!(cleaned.contains("mid"));
        assert!(cleaned.contains("after"));
    }
}
