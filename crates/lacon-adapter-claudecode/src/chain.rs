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
    /// `${...}` parameter-expansion brace depth. Per
    /// `docs/specs/chained-commands.md:15` `${...}` is a top-level-suppressing
    /// opaque construct, so a `&&`/`||`/`;` inside the braces (e.g. a `${x:-a &&
    /// b}` default value) must NOT split (CR-04).
    param_expansion_depth: u32,
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
            param_expansion_depth: 0,
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
            && self.param_expansion_depth == 0
            && self.in_heredoc.is_none()
    }

    /// True inside ANY opaque construct (quote / cmd-sub / subshell / backtick /
    /// process-sub / `${...}` / heredoc) — toggles below must respect this.
    fn in_opaque(&self) -> bool {
        self.in_single_quote
            || self.in_double_quote
            || self.subshell_depth > 0
            || self.cmd_sub_depth > 0
            || self.backtick_depth > 0
            || self.process_sub_depth > 0
            || self.param_expansion_depth > 0
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

        // 7. `${` opens parameter expansion (lookahead). Per
        //    `docs/specs/chained-commands.md:15` this is opaque: a `&&`/`||`/`;`
        //    inside `${x:-a && b}` must not split (CR-04). Checked BEFORE `$(`.
        if b == b'$' && i + 1 < n && bytes[i + 1] == b'{' {
            state.param_expansion_depth += 1;
            i += 2;
            continue;
        }

        // 8. `$(` opens command substitution (lookahead).
        if b == b'$' && i + 1 < n && bytes[i + 1] == b'(' {
            state.cmd_sub_depth += 1;
            i += 2;
            continue;
        }

        // 9. Inside a `${...}` expansion, `{` nests deeper and `}` closes one
        //    level; suppress chain operators while depth > 0. A `${` opener is
        //    handled above, so a bare `{` here is a nested brace (e.g. brace
        //    expansion inside the expansion word) — track it to find the match.
        if state.param_expansion_depth > 0 {
            if b == b'{' {
                state.param_expansion_depth += 1;
                i += 1;
                continue;
            }
            if b == b'}' {
                state.param_expansion_depth -= 1;
                i += 1;
                continue;
            }
        }

        // 10. Process substitution `<(` / `>(` (lookahead) opens an opaque region.
        if (b == b'<' || b == b'>') && i + 1 < n && bytes[i + 1] == b'(' {
            state.process_sub_depth += 1;
            i += 2;
            continue;
        }

        // 11. Here-string `<<<` is single-token opaque (NOT a heredoc). Consume the
        //    three bytes; the following word stays in the current segment.
        if b == b'<' && i + 2 < n && bytes[i + 1] == b'<' && bytes[i + 2] == b'<' {
            i += 3;
            continue;
        }

        // 12. Heredoc opener `<<DELIM` / `<<-DELIM` / `<<'DELIM'` / `<<"DELIM"`.
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

        // 13. Subshell `(` — but ONLY when not preceded by `$` (already handled).
        if b == b'(' && !state.in_double_quote {
            state.subshell_depth += 1;
            i += 1;
            continue;
        }

        // 14. Closing `)` — decrement the highest-precedence open depth
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

        // 15. Split operators — only at top level, never inside any opaque region.
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

/// True when a matched chain `segment` can be safely re-tokenized and re-quoted
/// into a `lacon run --rule <id> -- <argv>` wrapper without changing the
/// command's runtime semantics. This is a positive **ALLOWLIST** (CR-01 root-cause
/// fix, iteration 4): the orchestrator wraps a matched segment ONLY when this
/// returns `true`; everything else passes through byte-exact.
///
/// # Why an allowlist, not a denylist
///
/// When `run_hook` wraps a segment it: tokenizes it with `argv_for_resolution`
/// (a whitespace splitter that only models single/double quotes), applies the
/// rule's flag rewrite, and re-quotes every token with `quote_for_shell`
/// (single-quoting). Single-quoting neutralizes EVERY shell expansion, and the
/// downstream Runner executes `Command::new(&argv[0]).args(&argv[1..])` with NO
/// shell hop — so there is no place to faithfully re-emit any expansion bash
/// would have performed.
///
/// The previous `has_unwrappable_construct` predicate was a denylist of
/// dangerous constructs (`$(...)`, backticks, redirections, `${...}`, globs,
/// `~`, `$VAR`, …). A denylist must enumerate every construct that breaks, and it
/// repeatedly missed cases (most recently brace expansion `{a,b}` / `{1..10}`,
/// e.g. `eslint src/{a,b}.js` was wrapped and silently corrupted into the literal
/// `'src/{a,b}.js'`). Inverting to an allowlist means a segment is wrapped ONLY
/// when it is *provably reproducible* by tokenize→requote; any unanticipated byte
/// is treated as unsafe and passed through byte-exact (the fail-safe direction,
/// matching `docs/specs/chained-commands.md:17`).
///
/// # What counts as wrap-safe
///
/// A segment is wrap-safe iff it is composed EXCLUSIVELY of (scanning top level):
/// - whitespace separators (space, tab),
/// - "safe literal" bytes that are inert in the shell AND survive a
///   `quote_for_shell` round-trip: ASCII alphanumerics plus the set
///   `/ . - _ = : @ , + %`,
/// - single-quoted spans `'...'` — always literal/safe (the shell strips the
///   quotes and `argv_for_resolution` reproduces the inner bytes; on requote
///   `quote_for_shell` re-single-quotes them, round-tripping faithfully),
/// - double-quoted spans `"..."` that contain NO `$`, backtick, or backslash — a
///   double-quoted *literal* like `"a b"` is inert (it just suppresses word
///   splitting / globbing on already-inert bytes); `"$HOME"` is NOT, because the
///   `$` still expands inside double quotes.
///
/// ANY other top-level byte makes the segment NOT wrap-safe: `$`, backtick,
/// `* ? [ ] { }`, `~`, `< >`, `|`, `&`, `;`, `( )`, `#`, `!`, `\`, and any
/// control / non-printable byte. An empty or whitespace-only segment is NOT
/// wrappable (nothing to resolve).
///
/// This subsumes the old separate pipe guard (`|` is rejected here) and the old
/// `has_unwrappable_construct` denylist in one positive predicate.
///
/// # Round-trip correctness
///
/// For every segment this accepts, the existing `argv_for_resolution` tokenizer +
/// `quote_for_shell` round-trips faithfully:
/// - bare safe-literal runs tokenize on whitespace into argv tokens of inert
///   bytes; `quote_for_shell` re-emits each token (single-quoting if it contains
///   `= % * ?` etc. — but those can only appear here as `=` / `%`, both inert),
/// - single-quoted spans: `argv_for_resolution` drops the quote bytes and keeps
///   the inner bytes (including whitespace) in one token (`echo 'a b'` →
///   `["echo","a b"]`); `quote_for_shell` re-single-quotes → `'a b'`,
/// - double-quoted literal spans: same — `echo "a b"` tokenizes to
///   `["echo","a b"]` (the quoted whitespace stays in the token, NOT split), and
///   re-quotes to `'a b'`, which the single downstream shell parse turns back
///   into the token `a b`.
///
/// Because a wrap-safe segment contains no expansion, no redirection, and no
/// operator, the requoted argv reproduces the exact same program invocation.
///
/// Allocation-free, single-pass, linear-time — preserves the ≤10ms cold-start
/// budget (ADR-0013).
pub fn is_wrap_safe(segment: &str) -> bool {
    let bytes = segment.as_bytes();
    let n = bytes.len();
    let mut i = 0usize;
    let mut saw_token = false;

    while i < n {
        let b = bytes[i];

        // Whitespace separators are always safe (and tokenize boundaries).
        if b == b' ' || b == b'\t' {
            i += 1;
            continue;
        }

        // A newline at top level would join two commands when the shell re-parses
        // the wrapped form; treat it as unsafe (it is not a plain separator here).
        if b == b'\n' || b == b'\r' {
            return false;
        }

        // Single-quoted span: everything until the closing `'` is literal/safe.
        // An unterminated single quote is malformed → unsafe.
        if b == b'\'' {
            let mut j = i + 1;
            while j < n && bytes[j] != b'\'' {
                j += 1;
            }
            if j >= n {
                return false; // unterminated single quote
            }
            saw_token = true;
            i = j + 1;
            continue;
        }

        // Double-quoted span: safe ONLY if it contains no `$`, backtick, or
        // backslash (all of which keep their special meaning inside double
        // quotes). An unterminated double quote is malformed → unsafe.
        if b == b'"' {
            let mut j = i + 1;
            while j < n && bytes[j] != b'"' {
                let c = bytes[j];
                if c == b'$' || c == b'`' || c == b'\\' {
                    return false; // expansion / escape survives inside "..."
                }
                j += 1;
            }
            if j >= n {
                return false; // unterminated double quote
            }
            saw_token = true;
            i = j + 1;
            continue;
        }

        // Top-level "safe literal" byte: ASCII alphanumeric or one of the inert
        // punctuation bytes that also survive a quote_for_shell round-trip.
        if is_safe_literal_byte(b) {
            saw_token = true;
            i += 1;
            continue;
        }

        // Anything else (`$ \` * ? [ ] { } ~ < > | & ; ( ) # ! \\`, control /
        // non-printable bytes, multi-byte UTF-8 lead/continuation bytes) is not
        // provably reproducible by tokenize→requote → not wrap-safe.
        return false;
    }

    // Reject empty / whitespace-only segments: there is nothing to wrap.
    saw_token
}

/// True for a single byte that is inert in the shell at top level AND survives a
/// `quote_for_shell` round-trip: ASCII alphanumerics plus the curated inert
/// punctuation set `/ . - _ = : @ , + %`. Every other byte is treated as
/// potentially shell-significant and must make the segment fall out of the
/// [`is_wrap_safe`] allowlist. Kept tiny and branch-cheap for the hot path.
#[inline]
fn is_safe_literal_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric()
        || matches!(
            b,
            b'/' | b'.' | b'-' | b'_' | b'=' | b':' | b'@' | b',' | b'+' | b'%'
        )
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

    // NOTE (iteration 4): the former `has_top_level_pipe` predicate (and its five
    // unit tests) were removed when the wrap gate was inverted from a denylist to
    // the `is_wrap_safe` allowlist. The allowlist rejects `|` (it is not a safe
    // literal byte and is not inside a quoted span), so a pipelined segment such
    // as `echo hi | grep h` is no longer wrap-safe and passes through byte-exact —
    // the exact behavior the pipe guard provided. The pipe-passthrough invariant
    // is now exercised by `is_wrap_safe` rejection tests below and by the
    // `pipe_in_segment_preserved_not_split` e2e regression.

    // ── CR-04: `${...}` parameter expansion is opaque in the splitter ──────────

    #[test]
    fn param_expansion_default_value_with_chain_op_is_single_segment() {
        // `echo ${x:-a && b}` is ONE command (a default-value expansion), not a
        // broken two-segment chain. The `&&` inside the braces must not split.
        let input = "echo ${x:-a && b}";
        let segs = split_chain(input);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "echo ${x:-a && b}");
        assert_eq!(segs[0].trailing_op, None);
    }

    #[test]
    fn param_expansion_with_semicolon_is_single_segment() {
        let segs = split_chain("echo ${x:-a; b}");
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "echo ${x:-a; b}");
    }

    #[test]
    fn param_expansion_closes_then_real_chain_op_splits() {
        // After the `}` closes the expansion, a real top-level `&&` still splits.
        let segs = split_chain("echo ${x:-a} && b");
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "echo ${x:-a}");
        assert_eq!(segs[0].trailing_op, Some(ChainOp::AndAnd));
        assert_eq!(segs[1].text, "b");
    }

    // ── CR-01 (iteration 4): is_wrap_safe ALLOWLIST ───────────────────────────
    //
    // The wrap gate is now a positive allowlist: a segment is wrappable ONLY when
    // it is provably reproducible by `argv_for_resolution` → `quote_for_shell`.
    // ACCEPT rows are plain commands composed of safe-literal bytes + quoted
    // *literal* spans; REJECT rows carry any shell expansion / operator / redirect
    // / comment / escape the wrap form cannot reproduce.

    #[test]
    fn wrap_safe_accepts_plain_commands() {
        // The canonical wrappable shapes from real usage.
        assert!(is_wrap_safe("cargo build --release"));
        assert!(is_wrap_safe("pytest -k foo"));
        assert!(is_wrap_safe("eslint ."));
        assert!(is_wrap_safe("npm run test:unit"));
        assert!(is_wrap_safe("cmd --features=a,b,c"));
        assert!(is_wrap_safe("KEY=value cmd"));
        assert!(is_wrap_safe("echo hi"));
        assert!(is_wrap_safe("ls -la /tmp"));
        // Inert punctuation from the safe-literal set: `/ . - _ = : @ , + %`.
        assert!(is_wrap_safe("docker run img:tag"));
        assert!(is_wrap_safe("cmd a@host"));
        assert!(is_wrap_safe("printf 100%"));
        assert!(is_wrap_safe("cc -O2 +x"));
    }

    #[test]
    fn wrap_safe_accepts_quoted_literal_spans() {
        // Single-quoted spans are always literal/safe; argv_for_resolution keeps
        // the inner bytes (incl. whitespace) in one token and quote_for_shell
        // re-single-quotes them faithfully.
        assert!(is_wrap_safe("echo 'literal text'"));
        // Double-quoted *literal* spans (no $ / backtick / backslash) are safe:
        // they only suppress word splitting on already-inert bytes.
        assert!(is_wrap_safe("echo \"literal text\""));
        assert!(is_wrap_safe("git commit -m \"msg\""));
        // A `|` / `*` / `~` inside a quoted span is literal → still wrap-safe.
        assert!(is_wrap_safe("echo 'a | b'"));
        assert!(is_wrap_safe("echo \"a * b\""));
        // Adjacent-quote glue round-trips (argv: ["echo","abc"]).
        assert!(is_wrap_safe("echo a'b'c"));
    }

    #[test]
    fn wrap_safe_rejects_variable_expansion() {
        // Bare / positional / special-param expansion: quote_for_shell would
        // single-quote-neutralize it, so the wrap cannot reproduce the expansion.
        assert!(!is_wrap_safe("echo $HOME"));
        assert!(!is_wrap_safe("echo ${x}"));
        assert!(!is_wrap_safe("echo $1"));
        assert!(!is_wrap_safe("echo $?"));
        assert!(!is_wrap_safe("echo $@"));
        assert!(!is_wrap_safe("cargo build $FLAGS"));
        // `$` expands inside double quotes too — a double-quoted span with `$` is
        // NOT a literal span, so it is rejected.
        assert!(!is_wrap_safe("echo \"$HOME\""));
    }

    #[test]
    fn wrap_safe_rejects_command_and_process_substitution() {
        assert!(!is_wrap_safe("echo $(whoami)"));
        assert!(!is_wrap_safe("echo `id`"));
        assert!(!is_wrap_safe("diff <(a) <(b)"));
    }

    #[test]
    fn wrap_safe_rejects_globs_and_brace_expansion() {
        // The case the denylist kept missing: brace expansion.
        assert!(!is_wrap_safe("ls src/{a,b}.js"));
        assert!(!is_wrap_safe("echo {1..10}"));
        assert!(!is_wrap_safe("eslint src/{a,b}.js"));
        // Pathname globs.
        assert!(!is_wrap_safe("ls *.rs"));
        assert!(!is_wrap_safe("echo *"));
        assert!(!is_wrap_safe("ls file?.txt"));
        assert!(!is_wrap_safe("ls [abc].txt"));
        assert!(!is_wrap_safe("grep foo src/*"));
    }

    #[test]
    fn wrap_safe_rejects_tilde_expansion() {
        assert!(!is_wrap_safe("echo ~"));
        assert!(!is_wrap_safe("ls ~/.config"));
        assert!(!is_wrap_safe("echo ~user"));
    }

    #[test]
    fn wrap_safe_rejects_redirections() {
        assert!(!is_wrap_safe("echo hi > out.txt"));
        assert!(!is_wrap_safe("echo hi >> out.txt"));
        assert!(!is_wrap_safe("cat < in.txt"));
        assert!(!is_wrap_safe("cmd 2> err.log"));
        assert!(!is_wrap_safe("cmd &> all.log"));
        assert!(!is_wrap_safe("cat <<<word"));
    }

    #[test]
    fn wrap_safe_rejects_operators_comments_and_escapes() {
        // Pipe (subsumes the former has_top_level_pipe guard).
        assert!(!is_wrap_safe("a | b"));
        // Background / job-control.
        assert!(!is_wrap_safe("a & b"));
        // Comment in word position.
        assert!(!is_wrap_safe("a # c"));
        assert!(!is_wrap_safe("echo hi # do thing"));
        assert!(!is_wrap_safe("# whole line comment"));
        // Escaped whitespace would re-tokenize into two args.
        assert!(!is_wrap_safe("echo a\\ b"));
        // History expansion / negation.
        assert!(!is_wrap_safe("echo !x"));
        // Semicolon and parens (a wrapped segment never legitimately holds these,
        // but the allowlist must reject them defensively).
        assert!(!is_wrap_safe("a ; b"));
        assert!(!is_wrap_safe("(true)"));
    }

    #[test]
    fn wrap_safe_rejects_empty_and_whitespace_only() {
        // Nothing to wrap.
        assert!(!is_wrap_safe(""));
        assert!(!is_wrap_safe("   "));
        assert!(!is_wrap_safe("\t "));
    }

    #[test]
    fn wrap_safe_rejects_unterminated_and_unsafe_quoted_spans() {
        // Unterminated quotes are malformed → not wrap-safe.
        assert!(!is_wrap_safe("echo 'unterminated"));
        assert!(!is_wrap_safe("echo \"unterminated"));
        // Double-quoted span carrying an escape or backtick is not a literal span.
        assert!(!is_wrap_safe("echo \"a\\nb\""));
        assert!(!is_wrap_safe("echo \"`id`\""));
    }

    #[test]
    fn wrap_safe_rejects_non_ascii_and_control_bytes() {
        // A multi-byte UTF-8 char outside a quoted span is not in the allowlist
        // (conservative: pass through byte-exact rather than risk a mis-quote).
        assert!(!is_wrap_safe("echo café"));
        // But inside a single-quoted span it is literal/safe.
        assert!(is_wrap_safe("echo 'café'"));
        // Embedded newline at top level is unsafe.
        assert!(!is_wrap_safe("echo a\nb"));
    }

    // Retained for documentation: the historical denylist examples now map onto
    // the allowlist verdicts above. A `#` glued mid-token (`a#b`) used to need a
    // word-position carve-out; under the allowlist `#` is simply never a safe
    // literal byte, so `a#b` is uniformly NOT wrap-safe (still the safe
    // direction: pass through byte-exact).
    #[test]
    fn wrap_safe_glued_hash_is_not_wrappable() {
        assert!(!is_wrap_safe("echo a#b"));
    }

    #[test]
    fn wrap_safe_treats_quoted_dangerous_bytes_as_literal() {
        // Inside a single-quoted span every byte is literal and faithfully
        // reproduced by argv_for_resolution → quote_for_shell, so a `>` / `$` / `#`
        // there does NOT make the segment unsafe (the surrounding command is still
        // a plain wrappable invocation).
        assert!(is_wrap_safe("echo '> not a redirect'"));
        assert!(is_wrap_safe("echo '$(not a sub)'"));
        assert!(is_wrap_safe("echo '# not a comment'"));
        // Same for a double-quoted *literal* span (no $ / backtick / backslash).
        assert!(is_wrap_safe("echo \"> not a redirect\""));
        assert!(is_wrap_safe("echo \"# not a comment\""));
    }
}
