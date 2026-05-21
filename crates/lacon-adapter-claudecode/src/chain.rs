//! Top-level bash chain splitter (hand-rolled byte-iterating DFA).
//!
//! Splits a raw command string at top-level `&&` / `||` / `;` operators, never
//! inside opaque constructs (single/double quotes, `(...)` subshells, `$(...)`
//! command substitution, backticks, `<(...)`/`>(...)` process substitution,
//! `<<DELIM` heredoc bodies). Pipes (`|`) are NEVER split operators — a pipeline
//! is one segment (D-09 / REQ-adapter-pipes-passthrough).
//!
//! The authoritative test gate is the 13-scenario matrix in
//! `docs/specs/chained-commands.md:122-138` (mirrored in `tests/chain_split.rs`).
//! The DFA state transition table lives in
//! `.planning/phases/03-claude-code-adapter-lacon-init/03-RESEARCH.md:466-510`.
//!
//! Per D-06 the splitter operates on the raw UTF-8 command string (NOT a
//! pre-tokenized argv) so quote/heredoc state is observable. Per D-07 each
//! [`Segment`] preserves its verbatim byte span plus the original operator span,
//! so joining `segment.text + segment.trailing_op_span` across segments
//! reproduces the original input byte-exact (mitigates T-injection-chain-reassembly).

/// A top-level chain operator joining two segments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainOp {
    /// `&&` — run next on success.
    AndAnd,
    /// `||` — run next on failure.
    OrOr,
    /// `;` — run next regardless.
    Semi,
}

/// One segment of a split command chain.
///
/// `text` is the verbatim byte slice from the original input (preserves spacing
/// and quoting). `trailing_op` is the operator that followed this segment, or
/// `None` for the final segment. `trailing_op_span` is the verbatim operator
/// span INCLUDING surrounding whitespace (e.g. `" && "`, `" ||"`, `";  "`); it
/// is used by Plan 03-04 for byte-exact reassembly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// Verbatim byte span from the original input (preserves spacing & quoting).
    pub text: String,
    /// Operator that followed this segment in the input, or `None` for the last.
    pub trailing_op: Option<ChainOp>,
    /// Original operator span including surrounding whitespace
    /// (e.g. `" && "`, `" ||"`, `";  "`). Used by Plan 03-04 for reassembly.
    pub trailing_op_span: Option<String>,
}

/// Split a raw command string into chain [`Segment`]s.
///
/// Splits ONLY at top-level `&&` / `||` / `;`. See module docs for the opacity
/// rules. Joining each `segment.text` with its `trailing_op_span` reproduces the
/// original input byte-exact.
pub fn split_chain(_input: &str) -> Vec<Segment> {
    todo!("Task 2 implements the DFA")
}
