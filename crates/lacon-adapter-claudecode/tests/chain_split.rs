//! Chain-splitter test gate — the 13-scenario matrix from
//! `docs/specs/chained-commands.md:122-138` (one `#[test]` per scenario, S1..S13)
//! plus two pathological-input throughput tests.
//!
//! Each scenario asserts: segment count, each `segment.text` byte-exact, each
//! `trailing_op` variant, and that joining `segment.text + trailing_op_span`
//! across all segments reproduces the original input byte-exact (the reassembly
//! invariant Plan 03-04 relies on — mitigates T-injection-chain-reassembly).
//!
//! Scenario→DFA-transition map: 03-RESEARCH.md:516-532. S11 heredoc fixture per
//! 03-RESEARCH.md:534. S6/S12/S13 are orchestration concerns (TUI/`!!`/
//! `LACON_DISABLE` bypass) handled in Plan 03-04 — here we assert ONLY that the
//! splitter produces the expected byte-level segment count regardless of bypass.

use lacon_adapter_claudecode::chain::{ChainOp, Segment};

/// Run the splitter under test.
fn split(s: &str) -> Vec<Segment> {
    lacon_adapter_claudecode::chain::split_chain(s)
}

/// Reassemble segments into the original input: join each segment's verbatim
/// text with its trailing operator span. Must equal the original byte-exact.
fn reassemble(segments: &[Segment]) -> String {
    let mut out = String::new();
    for seg in segments {
        out.push_str(&seg.text);
        if let Some(span) = &seg.trailing_op_span {
            out.push_str(span);
        }
    }
    out
}

/// Assert the reassembly invariant for a given original input.
fn assert_reassembles(input: &str, segments: &[Segment]) {
    assert_eq!(
        reassemble(segments),
        input,
        "reassembly must reproduce the original input byte-exact"
    );
}

// ── S1: Single command, no chain ─────────────────────────────────────────────
#[test]
fn s1_single_command_no_chain() {
    let input = "pnpm test";
    let segs = split(input);
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].text, "pnpm test");
    assert_eq!(segs[0].trailing_op, None);
    assert_eq!(segs[0].trailing_op_span, None);
    assert_reassembles(input, &segs);
}

// ── S2a: Two-segment `&&` ─────────────────────────────────────────────────────
#[test]
fn s2a_two_segment_andand() {
    let input = "a && b";
    let segs = split(input);
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].text, "a");
    assert_eq!(segs[0].trailing_op, Some(ChainOp::AndAnd));
    assert_eq!(segs[1].text, "b");
    assert_eq!(segs[1].trailing_op, None);
    // Byte-exact reassembly: the " && " span (4 bytes incl. spaces) reproduces the input.
    assert_eq!(
        format!(
            "{}{}{}",
            segs[0].text,
            segs[0].trailing_op_span.clone().unwrap_or_default(),
            segs[1].text
        ),
        "a && b"
    );
    assert_reassembles(input, &segs);
}

// ── S2b: Two-segment `||` ─────────────────────────────────────────────────────
#[test]
fn s2b_two_segment_oror() {
    let input = "a || b";
    let segs = split(input);
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].text, "a");
    assert_eq!(segs[0].trailing_op, Some(ChainOp::OrOr));
    assert_eq!(segs[1].text, "b");
    assert_eq!(segs[1].trailing_op, None);
    assert_reassembles(input, &segs);
}

// ── S2c: Two-segment `;` ──────────────────────────────────────────────────────
#[test]
fn s2c_two_segment_semi() {
    let input = "a ; b";
    let segs = split(input);
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].text, "a");
    assert_eq!(segs[0].trailing_op, Some(ChainOp::Semi));
    assert_eq!(segs[1].text, "b");
    assert_eq!(segs[1].trailing_op, None);
    assert_reassembles(input, &segs);
}

// ── S3: Mixed operators ───────────────────────────────────────────────────────
#[test]
fn s3_mixed_operators() {
    let input = "a && b || c ; d";
    let segs = split(input);
    assert_eq!(segs.len(), 4);
    let ops: Vec<Option<ChainOp>> = segs.iter().map(|s| s.trailing_op.clone()).collect();
    assert_eq!(
        ops,
        vec![
            Some(ChainOp::AndAnd),
            Some(ChainOp::OrOr),
            Some(ChainOp::Semi),
            None,
        ]
    );
    assert_eq!(segs[0].text, "a");
    assert_eq!(segs[1].text, "b");
    assert_eq!(segs[2].text, "c");
    assert_eq!(segs[3].text, "d");
    assert_reassembles(input, &segs);
}

// ── S4: Per-segment differing rule (split only; resolution is Plan 03-04) ──────
#[test]
fn s4_per_segment_differing_rule() {
    let input = "pnpm install && pnpm test";
    let segs = split(input);
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].text, "pnpm install");
    assert_eq!(segs[0].trailing_op, Some(ChainOp::AndAnd));
    assert_eq!(segs[1].text, "pnpm test");
    assert_reassembles(input, &segs);
}

// ── S5: One segment unmatched (split only) ────────────────────────────────────
#[test]
fn s5_one_segment_unmatched() {
    let input = "pnpm install && echo done";
    let segs = split(input);
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].text, "pnpm install");
    assert_eq!(segs[1].text, "echo done");
    assert_reassembles(input, &segs);
}

// ── S6: One segment interactive — TUI bypass is Plan 03-04; splitter still splits ─
#[test]
fn s6_one_segment_interactive_whole_chain_bypass() {
    let input = "vim file && echo done";
    let segs = split(input);
    // Splitter just splits; the whole-chain bypass decision lives in orchestration.
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].text, "vim file");
    assert_eq!(segs[1].text, "echo done");
    assert_reassembles(input, &segs);
}

// ── S7: Subshell — single segment (`&&` suppressed inside `(...)`) ─────────────
#[test]
fn s7_subshell_single_segment() {
    let input = "(a && b)";
    let segs = split(input);
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].text, "(a && b)");
    assert_eq!(segs[0].trailing_op, None);
    assert_reassembles(input, &segs);
}

// ── S8: Command substitution — single segment (`&&` suppressed inside `$(...)`) ─
#[test]
fn s8_command_substitution_single_segment() {
    let input = "echo $(a && b)";
    let segs = split(input);
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].text, "echo $(a && b)");
    assert_eq!(segs[0].trailing_op, None);
    assert_reassembles(input, &segs);
}

// ── S9: Chain op inside quoted string — single segment ────────────────────────
#[test]
fn s9_chain_op_in_quoted_string_single_segment() {
    let input = "echo \"a && b\"";
    let segs = split(input);
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].text, "echo \"a && b\"");
    assert_eq!(segs[0].trailing_op, None);
    assert_reassembles(input, &segs);
}

// ── S10: Pipeline as a segment — `|` is NOT a chain op (REQ-pipes-passthrough) ─
#[test]
fn s10_pipeline_as_segment() {
    let input = "a | b && c";
    let segs = split(input);
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].text, "a | b");
    assert!(
        segs[0].text.contains('|'),
        "the pipe must remain inside the first segment"
    );
    assert_eq!(segs[0].trailing_op, Some(ChainOp::AndAnd));
    assert_eq!(segs[1].text, "c");
    assert_reassembles(input, &segs);
}

// ── S11: Heredoc body opaque — chain ops in the body do NOT split ─────────────
#[test]
fn s11_heredoc_body_opaque() {
    // Concrete fixture per 03-RESEARCH.md:534 — assert whole input preserved.
    let input = "cat <<EOF\na && b\nEOF";
    let segs = split(input);
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].text, "cat <<EOF\na && b\nEOF");
    assert_eq!(segs[0].trailing_op, None);
    assert_reassembles(input, &segs);
}

// ── S12: `!!` bypass — splitter has no special handling (bypass is Plan 03-04) ─
#[test]
fn s12_bang_bang_whole_chain_bypass() {
    // The splitter does NOT interpret `!!`; it simply splits the bytes it sees.
    // Orchestration (Plan 03-04) detects `!!` and pass-throughs BEFORE splitting.
    let input = "!! pnpm test";
    let segs = split(input);
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].text, "!! pnpm test");
    assert_eq!(segs[0].trailing_op, None);
    assert_reassembles(input, &segs);
}

// ── S13: LACON_DISABLE bypass — splitter doesn't see env; splits `a && b` ─────
#[test]
fn s13_lacon_disable_whole_chain_bypass() {
    // The splitter has no env awareness; LACON_DISABLE enforcement is Plan 03-04.
    let input = "a && b";
    let segs = split(input);
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].text, "a");
    assert_eq!(segs[1].text, "b");
    assert_reassembles(input, &segs);
}

// ── S14: `${...}` parameter expansion is opaque — chain op inside braces does ──
//        NOT split (CR-04 regression; docs/specs/chained-commands.md:15).
#[test]
fn s14_param_expansion_default_value_single_segment() {
    let input = "echo ${x:-a && b}";
    let segs = split(input);
    assert_eq!(
        segs.len(),
        1,
        "a `&&` inside ${{...}} must not split the command"
    );
    assert_eq!(segs[0].text, "echo ${x:-a && b}");
    assert_eq!(segs[0].trailing_op, None);
    assert_eq!(segs[0].trailing_op_span, None);
    assert_reassembles(input, &segs);
}

// ── S14b: `${...}` closes, then a real top-level `&&` still splits ────────────
#[test]
fn s14b_param_expansion_then_real_chain_op_splits() {
    let input = "echo ${x:-a} && b";
    let segs = split(input);
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].text, "echo ${x:-a}");
    assert_eq!(segs[0].trailing_op, Some(ChainOp::AndAnd));
    assert_eq!(segs[1].text, "b");
    assert_reassembles(input, &segs);
}

// ── Pathological inputs — linear-time, no panic ───────────────────────────────
#[test]
fn pathological_nested_subshells_no_panic() {
    let input = "((((true))))";
    let segs = split(input);
    assert_eq!(segs.len(), 1);
    assert_eq!(segs[0].text, "((((true))))");
    assert_reassembles(input, &segs);
}

#[test]
fn pathological_chain_in_cmd_sub() {
    let input = "echo $(echo $(echo $(echo hi))) && true";
    let segs = split(input);
    assert_eq!(segs.len(), 2);
    assert_eq!(segs[0].text, "echo $(echo $(echo $(echo hi)))");
    assert_eq!(segs[0].trailing_op, Some(ChainOp::AndAnd));
    assert_eq!(segs[1].text, "true");
    assert_reassembles(input, &segs);
}
