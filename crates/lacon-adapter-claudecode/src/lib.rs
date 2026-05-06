//! Claude Code adapter — Phase 1 stub.
//! The real PreToolUse hook + chained-command splitter land in Phase 3
//! (`/gsd-plan-phase 3`). This stub keeps the workspace boundary intact
//! so `lacon-core` does not accidentally take a dependency on adapter
//! internals.

#![allow(dead_code)]

/// Marker zero-sized type confirming the crate compiles. Phase 3
/// replaces with `pub struct Adapter { ... }` and a real hook impl.
pub struct ClaudeCodeAdapterStub;
