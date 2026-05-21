//! Claude Code adapter library.
//!
//! Speaks Claude Code's `PreToolUse(Bash)` hook protocol (see [`protocol`]) and
//! drives the rewrite pipeline: bypass-detect → chain-split → TUI-bypass →
//! per-segment rule resolve → `rewrite` apply → shell-quote → `lacon run` wrap →
//! reassemble → emit. This Plan (03-01) lands the protocol surface and a
//! pass-through `run_hook` skeleton; Plan 03-04 fills in the orchestration body.
//!
//! `lacon-core` stays assistant-agnostic — the adapter is the only crate that
//! knows Claude Code's wire format (D-01, D-02).

pub mod chain;
pub mod protocol;

// Wave-2 modules (Plan 03-03) land these files; declaring them now would
// be a compile error since the files don't yet exist:
//   pub mod tui;     // is_tui heuristic (Plan 03-03)
//   pub mod quote;   // quote_for_shell (Plan 03-03)

/// The decision `run_hook` returns to the binary entry point.
///
/// - `PassThrough` — emit nothing, exit 0 (D-03 pass-through path): unmatched
///   commands, `!!`/`LACON_DISABLE=1` bypass, whole-chain TUI bypass.
/// - `Rewrite(value)` — emit the `hookSpecificOutput` JSON value on stdout.
pub enum HookOutcome {
    PassThrough,
    Rewrite(serde_json::Value),
}

/// Drive the hook for a parsed stdin payload.
///
/// Plan 03-01 skeleton: returns `PassThrough` unconditionally (no decision logic
/// to attack — T-03-01-04). Plan 03-04 fills in the real orchestration body.
pub fn run_hook(_input: protocol::HookInput) -> anyhow::Result<HookOutcome> {
    Ok(HookOutcome::PassThrough)
}
