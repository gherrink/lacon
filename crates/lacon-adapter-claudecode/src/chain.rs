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

/// Active heredoc body context: the delimiter to match and whether leading
/// tabs are stripped (`<<-DELIM`). While set, all chain operators are opaque.
struct HeredocCtx {
    delimiter: String,
    /// `<<-` strips leading tabs from the terminator line (`<<-DELIM`).
    strip_tabs: bool,
}

/// DFA state for [`split_chain`]. Each opaque construct from
/// `docs/specs/chained-commands.md:19-27` has an explicit state field so that
/// `&&` / `||` / `;` inside it never triggers a split (03-RESEARCH.md:466-510).
struct SplitState {
    in_single_quote: bool,
    in_double_quote: bool,
    subshell_depth: u32,
    cmd_sub_depth: u32,
    backtick_depth: u32,
    process_sub_depth: u32,
    in_heredoc: Option<HeredocCtx>,
    escape_pending: bool,
}

impl SplitState {
    fn new() -> Self {
        SplitState {
            in_single_quote: false,
            in_double_quote: false,
            subshell_depth: 0,
            cmd_sub_depth: 0,
            backtick_depth: 0,
            process_sub_depth: 0,
            in_heredoc: None,
            escape_pending: false,
        }
    }

    /// True when the cursor sits at top level: no active quote, heredoc, depth,
    /// or backtick. Only then may a chain operator split the input.
    fn at_top_level(&self) -> bool {
        !self.in_single_quote
            && !self.in_double_quote
            && self.subshell_depth == 0
            && self.cmd_sub_depth == 0
            && self.backtick_depth == 0
            && self.process_sub_depth == 0
            && self.in_heredoc.is_none()
    }

    /// True inside ANY opaque construct (quote / cmd-sub / subshell / backtick /
    /// process-sub / heredoc) — toggles below must respect this.
    fn in_opaque(&self) -> bool {
        self.in_single_quote
            || self.in_double_quote
            || self.subshell_depth > 0
            || self.cmd_sub_depth > 0
            || self.backtick_depth > 0
            || self.process_sub_depth > 0
            || self.in_heredoc.is_some()
    }
}

/// Split a raw command string into chain [`Segment`]s.
///
/// Splits ONLY at top-level `&&` / `||` / `;`. See module docs for the opacity
/// rules. Joining each `segment.text` with its `trailing_op_span` reproduces the
/// original input byte-exact.
///
/// Single-pass, byte-iterating, linear-time in input length. The only heap
/// allocation beyond the output `Vec<Segment>` is the per-segment `String`s and
/// the (rare) heredoc delimiter capture.
///
/// # Examples
/// ```
/// use lacon_adapter_claudecode::chain::{split_chain, ChainOp};
/// let segs = split_chain("a && b");
/// assert_eq!(segs.len(), 2);
/// assert_eq!(segs[0].trailing_op, Some(ChainOp::AndAnd));
/// // `|` is never a split operator:
/// assert_eq!(split_chain("a | b && c").len(), 2);
/// ```
pub fn split_chain(input: &str) -> Vec<Segment> {
    let bytes = input.as_bytes();
    let n = bytes.len();
    let mut state = SplitState::new();
    let mut segments: Vec<Segment> = Vec::new();

    // `seg_start` is the byte index where the current segment's text begins.
    let mut seg_start = 0usize;
    let mut i = 0usize;

    while i < n {
        let b = bytes[i];

        // 1. Escape: the previous byte was a backslash in an escapable context.
        if state.escape_pending {
            state.escape_pending = false;
            i += 1;
            continue;
        }

        // 2. Heredoc body: opaque until a line equal to the delimiter is found.
        if let Some(ctx) = &state.in_heredoc {
            // A heredoc terminator must sit at the start of a line. Advance to the
            // next newline; on each line start, test for the delimiter.
            if b == b'\n' {
                // Examine the line that follows this newline.
                let line_start = i + 1;
                let mut line_end = line_start;
                while line_end < n && bytes[line_end] != b'\n' {
                    line_end += 1;
                }
                let mut content_start = line_start;
                if ctx.strip_tabs {
                    while content_start < line_end && bytes[content_start] == b'\t' {
                        content_start += 1;
                    }
                }
                let line = &input[content_start..line_end];
                if line == ctx.delimiter {
                    // The delimiter line closes the heredoc. Consume up to and
                    // including the delimiter line; stay opaque for the newline.
                    state.in_heredoc = None;
                    i = line_end;
                    continue;
                }
            }
            i += 1;
            continue;
        }

        // 3. Backslash starts an escape outside single quotes (literal inside
        //    single quotes per 03-RESEARCH.md:504).
        if b == b'\\' && !state.in_single_quote {
            state.escape_pending = true;
            i += 1;
            continue;
        }

        // 4. Single quote: toggles unless inside a double quote / heredoc.
        if b == b'\'' && !state.in_double_quote && state.in_heredoc.is_none() {
            state.in_single_quote = !state.in_single_quote;
            i += 1;
            continue;
        }

        // 5. Double quote: toggles unless inside a single quote / heredoc.
        if b == b'"' && !state.in_single_quote && state.in_heredoc.is_none() {
            state.in_double_quote = !state.in_double_quote;
            i += 1;
            continue;
        }

        // Inside a single quote everything else is literal (no nesting opens).
        if state.in_single_quote {
            i += 1;
            continue;
        }

        // 6. Backtick command substitution: flat toggle (no nesting in bash).
        if b == b'`' {
            state.backtick_depth ^= 1;
            i += 1;
            continue;
        }

        // 7. `$(` opens command substitution (lookahead).
        if b == b'$' && i + 1 < n && bytes[i + 1] == b'(' {
            state.cmd_sub_depth += 1;
            i += 2;
            continue;
        }

        // 8. Process substitution `<(` / `>(` (lookahead) opens an opaque region.
        if (b == b'<' || b == b'>') && i + 1 < n && bytes[i + 1] == b'(' {
            state.process_sub_depth += 1;
            i += 2;
            continue;
        }

        // 9. Here-string `<<<` is single-token opaque (NOT a heredoc). Consume the
        //    three bytes; the following word stays in the current segment.
        if b == b'<' && i + 2 < n && bytes[i + 1] == b'<' && bytes[i + 2] == b'<' {
            i += 3;
            continue;
        }

        // 10. Heredoc opener `<<DELIM` / `<<-DELIM` / `<<'DELIM'` / `<<"DELIM"`.
        if b == b'<' && i + 1 < n && bytes[i + 1] == b'<' && !state.in_double_quote {
            let mut j = i + 2;
            let strip_tabs = j < n && bytes[j] == b'-';
            if strip_tabs {
                j += 1;
            }
            // Skip optional spaces/tabs between `<<` and the delimiter word.
            while j < n && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if let Some((delimiter, after)) = scan_heredoc_delimiter(input, j) {
                state.in_heredoc = Some(HeredocCtx {
                    delimiter,
                    strip_tabs,
                });
                i = after;
                continue;
            }
            // No valid delimiter — treat `<<` as ordinary bytes.
            i += 2;
            continue;
        }

        // 11. Subshell `(` — but ONLY when not preceded by `$` (already handled).
        if b == b'(' && !state.in_double_quote {
            state.subshell_depth += 1;
            i += 1;
            continue;
        }

        // 12. Closing `)` — decrement the highest-precedence open depth
        //     (cmd_sub > process_sub > subshell) per 03-RESEARCH.md:503.
        if b == b')' && !state.in_double_quote {
            if state.cmd_sub_depth > 0 {
                state.cmd_sub_depth -= 1;
            } else if state.process_sub_depth > 0 {
                state.process_sub_depth -= 1;
            } else if state.subshell_depth > 0 {
                state.subshell_depth -= 1;
            }
            i += 1;
            continue;
        }

        // 13. Split operators — only at top level, never inside any opaque region.
        if state.at_top_level() {
            // `&&` (2-byte) — but NOT a single `&` (background) which we leave verbatim.
            if b == b'&' && i + 1 < n && bytes[i + 1] == b'&' {
                push_segment(input, &mut segments, seg_start, i, Some(ChainOp::AndAnd));
                let (span, next) = consume_op_span(input, i, 2);
                set_last_span(&mut segments, span);
                seg_start = next;
                i = next;
                continue;
            }
            // `||` (2-byte) — NOT a single `|` (pipe, D-09) which we leave verbatim.
            if b == b'|' && i + 1 < n && bytes[i + 1] == b'|' {
                push_segment(input, &mut segments, seg_start, i, Some(ChainOp::OrOr));
                let (span, next) = consume_op_span(input, i, 2);
                set_last_span(&mut segments, span);
                seg_start = next;
                i = next;
                continue;
            }
            // `;` (1-byte).
            if b == b';' {
                push_segment(input, &mut segments, seg_start, i, Some(ChainOp::Semi));
                let (span, next) = consume_op_span(input, i, 1);
                set_last_span(&mut segments, span);
                seg_start = next;
                i = next;
                continue;
            }
        }

        // Any other byte (including a single `|` or `&`) is consumed verbatim.
        let _ = state.in_opaque(); // documents intent; opaque bytes fall through here.
        i += 1;
    }

    // Final segment: from seg_start to end-of-input, no trailing operator.
    push_segment(input, &mut segments, seg_start, n, None);

    segments
}

/// True if `segment` contains a top-level single `|` (a pipe) outside any opaque
/// construct (quotes, subshells, `$(...)`, backticks, process-sub, heredoc).
///
/// Used by the hook orchestrator (Plan 03-04): a matched segment that is a
/// pipeline (`echo hi | grep h`) CANNOT be safely wrapped as
/// `lacon run --rule <id> -- <argv>`, because the downstream Runner executes
/// `Command::new(&argv[0]).args(&argv[1..])` with NO shell hop — re-quoting the
/// `|` as a literal argument would destroy the pipeline semantics. Per
/// `docs/specs/chained-commands.md:17` ("filtering inside pipes is explicitly out
/// of scope for v1") the orchestrator treats a pipelined segment as unmatched
/// (byte-exact pass-through) so the shell still sees the real `|`.
///
/// `||` (the OrOr chain op) is NOT a pipe and never reports true here; it is also
/// already a chain operator that `split_chain` would have split on at top level,
/// so a single [`Segment`] never contains a top-level `||`.
pub fn has_top_level_pipe(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    let n = bytes.len();
    let mut state = SplitState::new();
    let mut i = 0usize;

    while i < n {
        let b = bytes[i];

        if state.escape_pending {
            state.escape_pending = false;
            i += 1;
            continue;
        }
        if let Some(ctx) = &state.in_heredoc {
            if b == b'\n' {
                let line_start = i + 1;
                let mut line_end = line_start;
                while line_end < n && bytes[line_end] != b'\n' {
                    line_end += 1;
                }
                let mut content_start = line_start;
                if ctx.strip_tabs {
                    while content_start < line_end && bytes[content_start] == b'\t' {
                        content_start += 1;
                    }
                }
                if segment[content_start..line_end] == ctx.delimiter {
                    state.in_heredoc = None;
                    i = line_end;
                    continue;
                }
            }
            i += 1;
            continue;
        }
        if b == b'\\' && !state.in_single_quote {
            state.escape_pending = true;
            i += 1;
            continue;
        }
        if b == b'\'' && !state.in_double_quote {
            state.in_single_quote = !state.in_single_quote;
            i += 1;
            continue;
        }
        if b == b'"' && !state.in_single_quote {
            state.in_double_quote = !state.in_double_quote;
            i += 1;
            continue;
        }
        if state.in_single_quote {
            i += 1;
            continue;
        }
        if b == b'`' {
            state.backtick_depth ^= 1;
            i += 1;
            continue;
        }
        if b == b'$' && i + 1 < n && bytes[i + 1] == b'(' {
            state.cmd_sub_depth += 1;
            i += 2;
            continue;
        }
        if (b == b'<' || b == b'>') && i + 1 < n && bytes[i + 1] == b'(' {
            state.process_sub_depth += 1;
            i += 2;
            continue;
        }
        if b == b'<' && i + 2 < n && bytes[i + 1] == b'<' && bytes[i + 2] == b'<' {
            i += 3;
            continue;
        }
        if b == b'<' && i + 1 < n && bytes[i + 1] == b'<' && !state.in_double_quote {
            let mut j = i + 2;
            let strip_tabs = j < n && bytes[j] == b'-';
            if strip_tabs {
                j += 1;
            }
            while j < n && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if let Some((delimiter, after)) = scan_heredoc_delimiter(segment, j) {
                state.in_heredoc = Some(HeredocCtx {
                    delimiter,
                    strip_tabs,
                });
                i = after;
                continue;
            }
            i += 2;
            continue;
        }
        if b == b'(' && !state.in_double_quote {
            state.subshell_depth += 1;
            i += 1;
            continue;
        }
        if b == b')' && !state.in_double_quote {
            if state.cmd_sub_depth > 0 {
                state.cmd_sub_depth -= 1;
            } else if state.process_sub_depth > 0 {
                state.process_sub_depth -= 1;
            } else if state.subshell_depth > 0 {
                state.subshell_depth -= 1;
            }
            i += 1;
            continue;
        }
        // Top-level `|` that is NOT part of `||` is a pipe.
        if b == b'|' && state.at_top_level() {
            let is_or_or = i + 1 < n && bytes[i + 1] == b'|';
            let prev_or_or = i > 0 && bytes[i - 1] == b'|';
            if !is_or_or && !prev_or_or {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Scan a heredoc delimiter word starting at byte `start`. Handles bare,
/// single-quoted, and double-quoted delimiters. Returns `(delimiter, next_index)`
/// where `next_index` is the byte just past the delimiter token, or `None` if no
/// valid delimiter is present.
fn scan_heredoc_delimiter(input: &str, start: usize) -> Option<(String, usize)> {
    let bytes = input.as_bytes();
    let n = bytes.len();
    if start >= n {
        return None;
    }
    let quote = bytes[start];
    if quote == b'\'' || quote == b'"' {
        // Quoted delimiter: read until the matching closing quote.
        let mut k = start + 1;
        while k < n && bytes[k] != quote {
            k += 1;
        }
        if k >= n {
            return None; // unterminated quote — not a valid delimiter
        }
        let delim = input[start + 1..k].to_owned();
        if delim.is_empty() {
            return None;
        }
        return Some((delim, k + 1));
    }
    // Bare delimiter: a word of [A-Za-z0-9_] (and a few common chars). Stop at
    // whitespace, operator, or end-of-line.
    let mut k = start;
    while k < n {
        let c = bytes[k];
        let is_word = c.is_ascii_alphanumeric() || c == b'_' || c == b'.' || c == b'-';
        if !is_word {
            break;
        }
        k += 1;
    }
    if k == start {
        return None;
    }
    Some((input[start..k].to_owned(), k))
}

/// Push a segment covering `input[start..end]` with the given trailing operator
/// (span filled in afterwards via [`set_last_span`]).
fn push_segment(
    input: &str,
    segments: &mut Vec<Segment>,
    start: usize,
    end: usize,
    op: Option<ChainOp>,
) {
    segments.push(Segment {
        text: input[start..end].to_owned(),
        trailing_op: op,
        trailing_op_span: None,
    });
}

/// Capture the operator span starting at `op_pos` (length `op_len` bytes)
/// PLUS any trailing whitespace, so reassembly is byte-exact. Returns the span
/// string and the index of the first byte of the next segment.
fn consume_op_span(input: &str, op_pos: usize, op_len: usize) -> (String, usize) {
    let bytes = input.as_bytes();
    let n = bytes.len();
    let mut end = op_pos + op_len;
    while end < n && (bytes[end] == b' ' || bytes[end] == b'\t') {
        end += 1;
    }
    (input[op_pos..end].to_owned(), end)
}

/// Set the `trailing_op_span` of the most recently pushed segment, after first
/// trimming any trailing whitespace from that segment's text into the span so
/// the span captures the FULL operator region (leading + operator + trailing).
fn set_last_span(segments: &mut [Segment], op_and_trailing: String) {
    if let Some(last) = segments.last_mut() {
        // Move trailing whitespace off the segment text and into the span prefix
        // so reassembly (`text + span`) reproduces the original byte-exact.
        let trimmed = last.text.trim_end_matches([' ', '\t']);
        let leading_ws = &last.text[trimmed.len()..];
        let span = format!("{leading_ws}{op_and_trailing}");
        let trimmed_owned = trimmed.to_owned();
        last.text = trimmed_owned;
        last.trailing_op_span = Some(span);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input_yields_one_empty_segment() {
        // An empty command is one (empty) segment with no trailing op — joining
        // text+span still reproduces "".
        let segs = split_chain("");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "");
        assert_eq!(segs[0].trailing_op, None);
        assert_eq!(segs[0].trailing_op_span, None);
    }

    #[test]
    fn single_segment_no_op() {
        let segs = split_chain("echo hi");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "echo hi");
        assert_eq!(segs[0].trailing_op, None);
    }

    #[test]
    fn two_segment_andand_reassembles() {
        let segs = split_chain("a && b");
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].trailing_op, Some(ChainOp::AndAnd));
        let rejoined = format!(
            "{}{}{}",
            segs[0].text,
            segs[0].trailing_op_span.clone().unwrap_or_default(),
            segs[1].text
        );
        assert_eq!(rejoined, "a && b");
    }

    #[test]
    fn single_pipe_is_not_a_split() {
        let segs = split_chain("a | b");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "a | b");
    }

    #[test]
    fn has_top_level_pipe_detects_bare_pipe() {
        assert!(has_top_level_pipe("echo hi | grep h"));
        assert!(has_top_level_pipe("a|b"));
    }

    #[test]
    fn has_top_level_pipe_ignores_no_pipe() {
        assert!(!has_top_level_pipe("echo hi"));
        assert!(!has_top_level_pipe("ls -la"));
    }

    #[test]
    fn has_top_level_pipe_ignores_quoted_pipe() {
        assert!(!has_top_level_pipe("echo 'a | b'"));
        assert!(!has_top_level_pipe("echo \"a | b\""));
    }

    #[test]
    fn has_top_level_pipe_ignores_pipe_in_subshell_and_cmdsub() {
        assert!(!has_top_level_pipe("echo $(a | b)"));
        assert!(!has_top_level_pipe("(a | b)"));
        assert!(!has_top_level_pipe("echo `a | b`"));
    }

    #[test]
    fn has_top_level_pipe_ignores_or_or() {
        // A single segment never holds a top-level `||` (split_chain would have
        // split), but guard the predicate anyway.
        assert!(!has_top_level_pipe("a || b"));
    }
}
