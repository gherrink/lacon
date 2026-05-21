//! `lacon-claude-hook` — the Claude Code `PreToolUse(Bash)` hook binary (D-01).
//!
//! Ships as a separate binary (not a `lacon hook` subcommand) so the 6-command
//! `lacon` CLI surface stays locked (REQ-cli-surface-cap). Depends only on
//! `lacon-core` + serde/serde_json/anyhow to honour the ≤10ms cold-start budget
//! on the hook hot path (D-02, ADR-0013).
//!
//! Reads a `HookInput` JSON document from stdin, dispatches on `run_hook`, and
//! either exits 0 with empty stdout (pass-through) or writes the
//! `hookSpecificOutput` rewrite JSON to stdout (D-03).

use std::io::{self, Write};

use anyhow::Result;
use lacon_adapter_claudecode::protocol::HookInput;
use lacon_adapter_claudecode::{run_hook, HookOutcome};

fn main() -> Result<()> {
    let input: HookInput = serde_json::from_reader(io::stdin().lock())?;

    match run_hook(input)? {
        // D-03 pass-through path: exit 0, no stdout. Cheapest hot-path branch.
        HookOutcome::PassThrough => Ok(()),
        // D-03 rewrite path: emit the response value, newline-terminated for
        // tooling-friendliness (matches the project's trailing-newline convention).
        HookOutcome::Rewrite(value) => {
            let stdout = io::stdout();
            let mut handle = stdout.lock();
            serde_json::to_writer(&mut handle, &value)?;
            handle.write_all(b"\n")?;
            Ok(())
        }
    }
}
