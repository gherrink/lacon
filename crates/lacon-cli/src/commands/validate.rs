//! lacon validate subcommand: lint a rule or config file.
//!
//! Per D-17: dispatches by content (top-level `id` AND `match` -> rule;
//! else config). Per D-18: errors print one per line as
//! `<path>:<line>: <Category>: <message>`. Per docs/specs/config-schema.md
//! line 103, the example error is exactly:
//!   `.lacon/config.yaml:1: key `retention` is user-only; ...`
//!
//! Phase 1 scope cap: type-check + extends-flatten + script-file-existence
//! check. The `--fixtures` "dry-run-against-fixtures" mode is a v2 backlog
//! item (QoL — user-facing fixture validation).

use std::path::Path;

pub fn execute(path: &Path) -> anyhow::Result<i32> {
    if !path.exists() {
        eprintln!("{}: file not found", path.display());
        return Ok(1);
    }
    let errors = lacon_core::validate::validate_file(path);
    if errors.is_empty() {
        Ok(0)
    } else {
        for err in errors {
            eprintln!("{}", err);
        }
        Ok(1)
    }
}
