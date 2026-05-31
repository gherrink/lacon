//! Stage enum + 10 native primitive implementations for the lacon streaming pipeline.
//!
//! # Security note (T-02-01 — ReDoS)
//! The `regex` 1.x crate uses a lineartime NFA engine with no backreferences and no
//! exponential backtracking. User-supplied patterns (from rule YAML) cannot cause
//! algorithmic-complexity denial-of-service. Any pattern that fails `Regex::new` or
//! `RegexSet::new` is rejected by PLAN-03's loader with category `InvalidRegex` before
//! a `Stage` is ever constructed.
//!
//! # Memory bounds (T-02-02, T-02-03, CON-nfr-streaming-memory)
//! - `KeepTail::Lines(n)` ring: at most `n` lines in memory at once.
//! - `KeepTail::Bytes(n)` ring: at most `n + 1_line_max` bytes (pops-front before push).
//! - `KeepAroundMatch` context buffer: at most `before` lines.
//! - `Dedupe` / `CollapseRepeated` state: at most one buffered line + counters.
//! - `MaxBytes` is the secondary safety net per D-07; it truncates all subsequent output.
//!
//! # Design decisions (from 01-CONTEXT.md)
//! - D-05: Closed `enum Stage` dispatched via `match`. NO `Box<dyn Stage>`.
//! - D-06: Multiple `keep_regex` stages OR-merged into a single `RegexSet` at load time
//!   (done in `Pipeline::new`, not here).
//! - D-07: `max_bytes` stage is an explicit `Stage::MaxBytes` variant.
//! - D-08: Truncation marker `[lacon: truncated, N more bytes dropped]` is byte-exact.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::sync::OnceLock;

use regex::Regex;
use smallvec::SmallVec;

/// Output type alias for stage step/flush operations.
/// Most stages produce 0 or 1 output lines per input line.
/// `SmallVec<[_; 2]>` avoids heap allocation in the common case.
pub type LineOut<'a> = SmallVec<[Cow<'a, str>; 2]>;

/// Mode for `KeepHead` and `KeepTail` stages.
#[derive(Debug)]
pub enum HeadTailMode {
    /// Keep/tail by line count.
    Lines(usize),
    /// Keep/tail by byte count (including trailing `\n` per line).
    Bytes(usize),
}

/// Closed enum of all 10 native streaming pipeline primitives.
///
/// Each variant carries its own mutable state inline (no heap-allocated trait objects).
/// Per D-05: dispatched via `match` in `step()` and `flush()`.
///
/// # PLAN-03 construction contract
/// PLAN-03 (rule loader) constructs `Stage` values from a parsed `RuleFile.pipeline`.
/// Adjacent `Stage::KeepRegex` variants must be collapsed into one via `Pipeline::new`'s
/// OR-merge (D-06). PLAN-03 emits one `Stage::KeepRegex(RegexSet::new([single_pattern]))`
/// per `keep_regex:` YAML entry; the constructor in `Pipeline::new` merges N of those
/// into a single `RegexSet`.
#[derive(Debug)]
pub enum Stage {
    /// Removes ANSI CSI/SGR and OSC escape sequences from each line.
    StripAnsi,

    /// Drops (removes) any line matching the regex pattern.
    DropRegex(Regex),

    /// Keeps only lines matching any pattern in the set (whitelist mode).
    /// Created by OR-merging all adjacent `keep_regex` stages at load time.
    /// An empty `RegexSet` (no patterns) drops all lines.
    KeepRegex(regex::RegexSet),

    /// Replaces all occurrences of `pattern` in each line with `replacement`.
    ReplaceRegex {
        pattern: Regex,
        replacement: String,
    },

    /// Collapses consecutive identical lines, emitting at most `max_kept` copies.
    /// State resets when a different line arrives.
    ///
    /// # Fields
    /// - `last`: the last line seen (if any)
    /// - `max_kept`: maximum consecutive duplicates to emit (default 1)
    /// - `repeat_count`: how many times `last` has been emitted so far
    /// - `kept_so_far`: alias for repeat_count tracking (unused — kept for interface compat)
    Dedupe {
        last: Option<String>,
        max_kept: usize,
        repeat_count: usize,
        kept_so_far: usize,
    },

    /// Collapses consecutive lines matching `pattern` into `max_kept` examples plus
    /// a summary line. `{count}` in `summary_template` is replaced with the total
    /// number of dropped lines (NOT including the emitted examples).
    ///
    /// # Fields
    /// - `pattern`: lines matching this are collapsed
    /// - `max_kept`: how many examples to emit before suppressing
    /// - `summary_template`: template with `{count}` placeholder
    /// - `kept_so_far`: examples emitted in the current run
    /// - `dropped`: lines suppressed (not emitted) in the current run
    CollapseRepeated {
        pattern: Regex,
        max_kept: usize,
        summary_template: String,
        kept_so_far: usize,
        dropped: usize,
    },

    /// Keeps only the first N lines or bytes of output.
    ///
    /// - `Lines(n)`: emit first `n` lines, drop the rest.
    /// - `Bytes(n)`: emit lines while cumulative bytes (incl. `\n`) ≤ `n`;
    ///   final line is NOT truncated — the line that would overflow is dropped.
    ///
    /// # Fields
    /// - `mode`: `Lines(n)` or `Bytes(n)`
    /// - `lines_remaining`: lines left to emit (used in Lines mode)
    /// - `bytes_remaining`: bytes budget remaining (used in Bytes mode)
    KeepHead {
        mode: HeadTailMode,
        lines_remaining: usize,
        bytes_remaining: usize,
    },

    /// Keeps only the last N lines or bytes of output (ring-buffer semantics).
    ///
    /// - `Lines(n)`: rolling ring of last `n` lines, emitted at flush.
    /// - `Bytes(n)`: ring tracks running byte count; pop_front when total > `n`.
    ///
    /// # PLAN-03 validation note (T-02-02)
    /// PLAN-03 rejects `n == 0` (degenerate). Suggested PLAN-03 upper bounds:
    /// - Bytes mode: 16 MiB (16_777_216)
    /// - Lines mode: 1_000_000
    ///
    /// # Fields
    /// - `mode`: `Lines(n)` or `Bytes(n)`
    /// - `ring`: the current ring buffer of retained lines
    /// - `byte_count`: running byte count for Bytes mode
    KeepTail {
        mode: HeadTailMode,
        ring: VecDeque<String>,
        byte_count: usize,
    },

    /// Grep `-B`/`-A` semantics: for each line matching `pattern`, emit
    /// `before` preceding context lines and `after` following lines.
    ///
    /// # Fields
    /// - `pattern`: the match trigger
    /// - `before`: context window size (preceding lines)
    /// - `after`: lines to emit after each match
    /// - `ctx_buf`: rolling context buffer of size `before`
    /// - `emit_after`: lines remaining to emit in the post-match window
    KeepAroundMatch {
        pattern: Regex,
        before: usize,
        after: usize,
        ctx_buf: VecDeque<String>,
        emit_after: usize,
    },

    /// Hard cap on total output size. Tracks bytes written; once a line would
    /// exceed `cap`, defers the byte-exact truncation marker (D-08) to `flush()`
    /// after accumulating the full drop count across all remaining lines.
    ///
    /// # Truncation marker (D-08, byte-exact)
    /// `[lacon: truncated, N more bytes dropped]`
    /// where `N` = total bytes dropped — the overflowing line plus every subsequent
    /// line plus their `\n` separators.  Emitted at flush() once all lines are seen.
    ///
    /// # Fields
    /// - `cap`: byte cap (inclusive)
    /// - `written`: bytes written so far (each line counted as `len + 1` for `\n`)
    /// - `truncated`: true once the cap has been exceeded (first overflow seen)
    /// - `dropped_bytes`: cumulative bytes of all dropped lines (CR-04)
    MaxBytes {
        cap: usize,
        written: usize,
        truncated: bool,
        /// Cumulative bytes of all lines dropped past the cap.
        /// Includes the `\n` that the runner appends to each line.
        /// Emitted in flush() as the final `N` in the truncation marker.
        dropped_bytes: usize,
    },
}

/// Returns the shared ANSI escape sequence regex, compiled exactly once.
///
/// Pattern covers:
/// - CSI sequences: `ESC [ <param> <intermediate> <final>`
/// - OSC sequences: `ESC ] ... BEL` or `ESC ] ... ESC \`
fn ansi_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        // Order matters: put longer/more-specific alternatives FIRST so they are
        // tried before the short single-character fallbacks.
        //
        // CSI: ESC [ <param bytes 0x30-0x3F>* <intermediate bytes 0x20-0x2F>* <final 0x40-0x7E>
        // OSC: ESC ] <any non-BEL, non-ESC>* (BEL | ESC \)
        // Fe sequences: ESC [@-Z\\-_] (single char — catches ESC M, ESC =, etc.)
        //               NOTE: 0x5D is ']' which falls in [@-_], but we handle
        //               the OSC form specifically above so it must come first.
        Regex::new(
            r"\x1b(?:\[[0-?]*[ -/]*[@-~]|\][^\x07\x1b]*(?:\x07|\x1b\\)|[@-Z\\-_])"
        )
        .expect("ANSI regex is hardcoded and valid")
    })
}

impl Stage {
    /// Process one line through this stage, appending zero or more output lines to `out`.
    ///
    /// `line` is `Cow<'a, str>` so that passthrough stages (e.g., `DropRegex` when the
    /// line is not dropped) avoid cloning the string.
    pub fn step<'a>(&mut self, line: Cow<'a, str>, out: &mut LineOut<'a>) {
        match self {
            // ─── StripAnsi ──────────────────────────────────────────────────────────────
            Stage::StripAnsi => {
                let re = ansi_regex();
                if re.is_match(&line) {
                    // Replace produces an owned String; wrap it in Cow::Owned.
                    let stripped = re.replace_all(&line, "").into_owned();
                    out.push(Cow::Owned(stripped));
                } else {
                    // No ANSI codes — pass through without cloning.
                    out.push(line);
                }
            }

            // ─── DropRegex ──────────────────────────────────────────────────────────────
            Stage::DropRegex(re) => {
                if !re.is_match(&line) {
                    out.push(line);
                }
                // else: line is dropped
            }

            // ─── KeepRegex ──────────────────────────────────────────────────────────────
            Stage::KeepRegex(set) => {
                if set.is_match(&line) {
                    out.push(line);
                }
                // else: line is dropped (whitelist mode; empty set drops everything)
            }

            // ─── ReplaceRegex ───────────────────────────────────────────────────────────
            Stage::ReplaceRegex { pattern, replacement } => {
                if pattern.is_match(&line) {
                    let replaced = pattern.replace_all(&line, replacement.as_str()).into_owned();
                    out.push(Cow::Owned(replaced));
                } else {
                    out.push(line);
                }
            }

            // ─── Dedupe ─────────────────────────────────────────────────────────────────
            Stage::Dedupe { last, max_kept, repeat_count, kept_so_far: _ } => {
                let is_dup = last.as_deref() == Some(&line);
                if is_dup {
                    if *repeat_count < *max_kept {
                        *repeat_count += 1;
                        out.push(line);
                    }
                    // else: consecutive duplicate beyond max_kept — drop
                } else {
                    // Different line: reset state and emit
                    *last = Some(line.clone().into_owned());
                    *repeat_count = 1;
                    out.push(line);
                }
            }

            // ─── CollapseRepeated ───────────────────────────────────────────────────────
            Stage::CollapseRepeated {
                pattern,
                max_kept,
                summary_template: _,
                kept_so_far,
                dropped,
            } => {
                if pattern.is_match(&line) {
                    if *kept_so_far < *max_kept {
                        *kept_so_far += 1;
                        out.push(line);
                    } else {
                        *dropped += 1;
                    }
                } else {
                    // Non-matching line: emit the standardized lacon elision marker
                    // ONLY when lines were actually suppressed, then emit the line.
                    //
                    // D-07 fix: the elision is a fixed `[lacon: …]`-namespaced marker
                    // modeled on the `MaxBytes` marker below — NOT the free-form
                    // `summary_template`, which could inherit the elided lines'
                    // formatting (e.g. a tab-indent) and blend into real tool output.
                    // The `summary_template` field is retained for YAML deserialization
                    // (loader populates it) but is no longer emitted (D-09).
                    if *dropped > 0 {
                        out.push(Cow::Owned(format!("[lacon: collapsed {} lines]", dropped)));
                    }
                    *kept_so_far = 0;
                    *dropped = 0;
                    out.push(line);
                }
            }

            // ─── KeepHead ───────────────────────────────────────────────────────────────
            Stage::KeepHead {
                mode,
                lines_remaining,
                bytes_remaining,
            } => match mode {
                HeadTailMode::Lines(_) => {
                    if *lines_remaining > 0 {
                        *lines_remaining -= 1;
                        out.push(line);
                    }
                    // else: past the head — drop
                }
                HeadTailMode::Bytes(cap) => {
                    // bytes_remaining is initialized to *cap on construction.
                    // We track it in bytes_remaining for generality.
                    let line_bytes = line.len() + 1; // +1 for the \n the runner adds
                    if *bytes_remaining >= line_bytes {
                        *bytes_remaining -= line_bytes;
                        out.push(line);
                    }
                    // else: would overflow byte budget — drop this line and all remaining
                    // (set budget to 0 so subsequent lines are also dropped)
                    else if *bytes_remaining > 0 {
                        // Partial: emit as-is (no truncation at the line level) but
                        // the spec says "emit lines until cumulative emitted bytes > n".
                        // The partial case is: budget > 0 but < line_bytes → drop.
                        *bytes_remaining = 0;
                    }
                    let _ = cap; // suppress unused warning — cap is encoded in bytes_remaining init
                }
            },

            // ─── KeepTail ───────────────────────────────────────────────────────────────
            Stage::KeepTail { mode, ring, byte_count } => match mode {
                HeadTailMode::Lines(n) => {
                    let cap = *n;
                    if ring.len() >= cap {
                        ring.pop_front();
                    }
                    ring.push_back(line.into_owned());
                }
                HeadTailMode::Bytes(cap) => {
                    let line_bytes = line.len() + 1; // +1 for \n
                    // Pop from front until we have room.
                    while *byte_count + line_bytes > *cap && !ring.is_empty() {
                        let removed = ring.pop_front().unwrap();
                        *byte_count -= removed.len() + 1;
                    }
                    *byte_count += line_bytes;
                    ring.push_back(line.into_owned());
                }
            },

            // ─── KeepAroundMatch ────────────────────────────────────────────────────────
            Stage::KeepAroundMatch {
                pattern,
                before,
                after,
                ctx_buf,
                emit_after,
            } => {
                if pattern.is_match(&line) {
                    // Drain the pre-match context buffer to out.
                    for buffered in ctx_buf.drain(..) {
                        out.push(Cow::Owned(buffered));
                    }
                    // Emit the matching line.
                    out.push(line);
                    // Signal that the next `after` lines should be emitted.
                    *emit_after = *after;
                } else if *emit_after > 0 {
                    // We are in the post-match window — emit and decrement.
                    *emit_after -= 1;
                    out.push(line);
                } else {
                    // Neither a match nor in the post-match window.
                    // Add to the context buffer; pop front if it exceeds `before`.
                    if *before > 0 {
                        if ctx_buf.len() >= *before {
                            ctx_buf.pop_front();
                        }
                        ctx_buf.push_back(line.into_owned());
                    }
                    // else: before==0, no context needed; just discard
                }
            }

            // ─── MaxBytes ───────────────────────────────────────────────────────────────
            // CR-04 fix: accumulate all dropped bytes in `dropped_bytes`; emit the
            // truncation marker with the CUMULATIVE count in flush(), not here.
            // This produces a byte-exact N per D-08 (first overflowing line + all
            // subsequent lines, including their implicit \n separators).
            Stage::MaxBytes { cap, written, truncated, dropped_bytes } => {
                if *truncated {
                    // Past the cap — accumulate this line's bytes into the running total.
                    *dropped_bytes += line.len() + 1;
                    return;
                }
                // Account for the `\n` the runner appends when joining output.
                let line_bytes = line.len() + 1;
                if *written + line_bytes > *cap {
                    // First line that overflows: begin accumulation — marker deferred to flush().
                    *dropped_bytes += line_bytes;
                    *truncated = true;
                } else {
                    *written += line_bytes;
                    out.push(line);
                }
            }
        }
    }

    /// Flush any buffered state to `out`.
    ///
    /// Called once at end-of-stream. Stateful stages that hold deferred output
    /// (`KeepTail`, `CollapseRepeated`) drain their buffers here.
    ///
    /// For stateless or head-type stages, this is a no-op.
    pub fn flush<'a>(&mut self, out: &mut LineOut<'a>) {
        match self {
            // ─── KeepTail ───────────────────────────────────────────────────────────────
            Stage::KeepTail { ring, .. } => {
                for line in ring.drain(..) {
                    out.push(Cow::Owned(line));
                }
            }

            // ─── CollapseRepeated ───────────────────────────────────────────────────────
            // Flush final summary ONLY when lines were actually suppressed (dropped > 0).
            // CR-03 fix: the old condition `kept_so_far > 0 || dropped > 0` was wrong —
            // when the stream ends exactly at max_kept examples with nothing suppressed,
            // `kept_so_far > 0` fires and emits "… 0 <noun>" which is spurious noise.
            Stage::CollapseRepeated {
                kept_so_far,
                dropped,
                ..
            } => {
                if *dropped > 0 {
                    // D-07: same standardized `[lacon: …]` marker as the in-run path.
                    // CR-03 guard preserved — emit only when lines were suppressed.
                    out.push(Cow::Owned(format!("[lacon: collapsed {} lines]", dropped)));
                    *kept_so_far = 0;
                    *dropped = 0;
                }
            }

            // ─── MaxBytes ───────────────────────────────────────────────────────────────
            // CR-04 fix: emit the truncation marker here with the full cumulative
            // dropped_bytes count.  Deferred from step() so the count covers ALL
            // lines dropped past the cap (first overflowing line + all subsequent).
            Stage::MaxBytes { truncated, dropped_bytes, .. } => {
                if *truncated {
                    let marker = format!(
                        "[lacon: truncated, {} more bytes dropped]",
                        dropped_bytes
                    );
                    out.push(Cow::Owned(marker));
                }
            }

            // All other stages are stateless (or state was already consumed in step).
            _ => {}
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Inline unit tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use regex::RegexSet;

    // Helper: run a single stage on a list of string literals, return owned strings.
    fn run_stage(stage: &mut Stage, lines: &[&str]) -> Vec<String> {
        let mut result: Vec<String> = Vec::new();
        for &line in lines {
            let mut out: LineOut = SmallVec::new();
            stage.step(Cow::Borrowed(line), &mut out);
            for s in out {
                result.push(s.into_owned());
            }
        }
        // flush
        let mut out: LineOut = SmallVec::new();
        stage.flush(&mut out);
        for s in out {
            result.push(s.into_owned());
        }
        result
    }

    // ── StripAnsi ────────────────────────────────────────────────────────────

    #[test]
    fn strip_ansi_removes_csi() {
        let mut s = Stage::StripAnsi;
        let out = run_stage(&mut s, &["\x1b[31mred\x1b[0m"]);
        assert_eq!(out, vec!["red"]);
    }

    #[test]
    fn strip_ansi_passthrough_plain() {
        let mut s = Stage::StripAnsi;
        let out = run_stage(&mut s, &["plain text"]);
        assert_eq!(out, vec!["plain text"]);
    }

    #[test]
    fn strip_ansi_removes_osc() {
        let mut s = Stage::StripAnsi;
        let out = run_stage(&mut s, &["\x1b]2;title\x07text"]);
        assert_eq!(out, vec!["text"]);
    }

    // ── DropRegex ────────────────────────────────────────────────────────────

    #[test]
    fn drop_regex_drops_matching() {
        let re = Regex::new("warn").unwrap();
        let mut s = Stage::DropRegex(re);
        let out = run_stage(&mut s, &["info ok", "warn deprecated", "error critical"]);
        assert_eq!(out, vec!["info ok", "error critical"]);
    }

    // ── KeepRegex ────────────────────────────────────────────────────────────

    #[test]
    fn keep_regex_or_merge_three_patterns() {
        // D-06: three separate patterns merged into one RegexSet
        let set = RegexSet::new(["error", "FAIL", "panic"]).unwrap();
        let mut s = Stage::KeepRegex(set);
        let out = run_stage(
            &mut s,
            &["error: bad", "warning: ok", "FAIL test1", "info", "panic at"],
        );
        assert_eq!(out, vec!["error: bad", "FAIL test1", "panic at"]);
    }

    #[test]
    fn keep_regex_empty_set_drops_all() {
        let set = RegexSet::new::<&[&str], _>(&[]).unwrap();
        let mut s = Stage::KeepRegex(set);
        let out = run_stage(&mut s, &["a", "b", "c"]);
        assert!(out.is_empty(), "empty RegexSet should drop everything");
    }

    // ── ReplaceRegex ─────────────────────────────────────────────────────────

    #[test]
    fn replace_regex_substitutes_all() {
        // Note: no word boundary (\b) before '/' since '/' is not a word character.
        // The spec example uses `\b/Users/[^/]+/` which matches when /Users/ appears
        // after a word boundary — but at the start of a string, use the simpler form.
        let pattern = Regex::new(r"/Users/[^/]+/").unwrap();
        let mut s = Stage::ReplaceRegex {
            pattern,
            replacement: "~/".to_owned(),
        };
        let out = run_stage(&mut s, &["/Users/alice/proj/src/main.rs"]);
        assert_eq!(out, vec!["~/proj/src/main.rs"]);
    }

    // ── Dedupe ───────────────────────────────────────────────────────────────

    #[test]
    fn dedupe_max_kept_1() {
        let mut s = Stage::Dedupe {
            last: None,
            max_kept: 1,
            repeat_count: 0,
            kept_so_far: 0,
        };
        let out = run_stage(&mut s, &["done", "done", "done", "start", "done", "done"]);
        assert_eq!(out, vec!["done", "start", "done"]);
    }

    #[test]
    fn dedupe_max_kept_2() {
        let mut s = Stage::Dedupe {
            last: None,
            max_kept: 2,
            repeat_count: 0,
            kept_so_far: 0,
        };
        let out = run_stage(&mut s, &["a", "a", "a", "a", "b"]);
        assert_eq!(out, vec!["a", "a", "b"]);
    }

    // ── CollapseRepeated ─────────────────────────────────────────────────────

    #[test]
    fn collapse_repeated_summary_on_non_match() {
        let pattern = Regex::new(r"^Progress: \d+%").unwrap();
        let mut s = Stage::CollapseRepeated {
            pattern,
            max_kept: 1,
            summary_template: "… {count} progress lines".to_owned(),
            kept_so_far: 0,
            dropped: 0,
        };
        // 1 kept, then 3 dropped → summary shows 3
        let out = run_stage(
            &mut s,
            &[
                "Progress: 10%",
                "Progress: 20%",
                "Progress: 30%",
                "Progress: 40%",
                "Done",
            ],
        );
        assert_eq!(
            out,
            vec!["Progress: 10%", "[lacon: collapsed 3 lines]", "Done"]
        );
    }

    #[test]
    fn collapse_repeated_flush_summary_at_eos() {
        let pattern = Regex::new(r"^Progress: \d+%").unwrap();
        let mut s = Stage::CollapseRepeated {
            pattern,
            max_kept: 1,
            summary_template: "… {count} progress lines".to_owned(),
            kept_so_far: 0,
            dropped: 0,
        };
        // Stream ends while still in the repeated run — flush must emit summary
        let out = run_stage(
            &mut s,
            &["Progress: 10%", "Progress: 20%", "Progress: 30%"],
        );
        assert_eq!(out, vec!["Progress: 10%", "[lacon: collapsed 2 lines]"]);
    }

    #[test]
    fn collapse_repeated_no_spurious_summary_when_nothing_dropped() {
        // CR-03 regression: when stream ends exactly at max_kept examples with
        // nothing suppressed, flush must NOT emit a "… 0 …" summary line.
        let pattern = Regex::new(r"^P:").unwrap();
        let mut s = Stage::CollapseRepeated {
            pattern,
            max_kept: 2,
            summary_template: "… {count} lines suppressed".to_owned(),
            kept_so_far: 0,
            dropped: 0,
        };
        // Exactly max_kept=2 matching lines, no non-match to trigger in-step summary,
        // and dropped==0 at flush time — must produce no summary line.
        let out = run_stage(&mut s, &["P: 1", "P: 2"]);
        assert_eq!(
            out,
            vec!["P: 1", "P: 2"],
            "no summary line should be emitted when dropped == 0 at flush"
        );
    }

    #[test]
    fn collapse_repeated_survivors_are_verbatim_input_lines() {
        // D-09 / T-09-05: every non-marker emitted line must be byte-identical to
        // an input line. Feed tab-indented (tabular) repeated-prefix input — the
        // class that previously had a blending tab-indented summary substituted in.
        let pattern = Regex::new(r"^\t").unwrap();
        let mut s = Stage::CollapseRepeated {
            pattern,
            max_kept: 2,
            summary_template: "\t… {count} more changed/untracked files".to_owned(),
            kept_so_far: 0,
            dropped: 0,
        };
        let input = [
            "On branch main",
            "\tmodified:   src/a.rs",
            "\tmodified:   src/b.rs",
            "\tmodified:   src/c.rs",
            "\tmodified:   src/d.rs",
            "nothing to commit",
        ];
        let out = run_stage(&mut s, &input);

        // The elision is the standardized lacon marker — NOT a tab-indented
        // line that blends into the surviving file rows (D-07).
        assert_eq!(
            out,
            vec![
                "On branch main",
                "\tmodified:   src/a.rs",
                "\tmodified:   src/b.rs",
                "[lacon: collapsed 2 lines]",
                "nothing to commit",
            ]
        );

        // Every non-marker survivor is byte-identical to an input line.
        for emitted in &out {
            if emitted.starts_with("[lacon:") {
                continue;
            }
            assert!(
                input.contains(&emitted.as_str()),
                "emitted non-marker line {emitted:?} is not byte-identical to any input line"
            );
        }
    }

    // ── KeepHead ─────────────────────────────────────────────────────────────

    #[test]
    fn keep_head_lines() {
        let mut s = Stage::KeepHead {
            mode: HeadTailMode::Lines(3),
            lines_remaining: 3,
            bytes_remaining: 0,
        };
        let out = run_stage(&mut s, &["a", "b", "c", "d", "e"]);
        assert_eq!(out, vec!["a", "b", "c"]);
    }

    #[test]
    fn keep_head_bytes() {
        // "abc\n" = 4 bytes, "def\n" = 4 bytes → budget 8 fits both; "ghi\n" = 4 → overflow → drop
        let mut s = Stage::KeepHead {
            mode: HeadTailMode::Bytes(8),
            lines_remaining: 0,
            bytes_remaining: 8,
        };
        let out = run_stage(&mut s, &["abc", "def", "ghi"]);
        assert_eq!(out, vec!["abc", "def"]);
    }

    // ── KeepTail ─────────────────────────────────────────────────────────────

    #[test]
    fn keep_tail_lines() {
        let mut s = Stage::KeepTail {
            mode: HeadTailMode::Lines(3),
            ring: VecDeque::new(),
            byte_count: 0,
        };
        let out = run_stage(&mut s, &["a", "b", "c", "d", "e"]);
        assert_eq!(out, vec!["c", "d", "e"]);
    }

    #[test]
    fn keep_tail_bytes() {
        // "abc\n"=4, "def\n"=4, "ghi\n"=4 → ring cap 8 → only last 2 fit
        let mut s = Stage::KeepTail {
            mode: HeadTailMode::Bytes(8),
            ring: VecDeque::new(),
            byte_count: 0,
        };
        let out = run_stage(&mut s, &["abc", "def", "ghi"]);
        assert_eq!(out, vec!["def", "ghi"]);
    }

    // ── KeepAroundMatch ──────────────────────────────────────────────────────

    #[test]
    fn keep_around_match_after_only() {
        let pattern = Regex::new(r"^FAIL").unwrap();
        let mut s = Stage::KeepAroundMatch {
            pattern,
            before: 0,
            after: 2,
            ctx_buf: VecDeque::new(),
            emit_after: 0,
        };
        let out = run_stage(&mut s, &["line1", "FAIL here", "line3", "line4", "line5"]);
        assert_eq!(out, vec!["FAIL here", "line3", "line4"]);
    }

    #[test]
    fn keep_around_match_before_and_after() {
        let pattern = Regex::new(r"^FAIL").unwrap();
        let mut s = Stage::KeepAroundMatch {
            pattern,
            before: 2,
            after: 1,
            ctx_buf: VecDeque::new(),
            emit_after: 0,
        };
        let out = run_stage(
            &mut s,
            &["a", "b", "c", "FAIL here", "d", "e"],
        );
        // before=2 → "b" and "c" emitted; match; after=1 → "d"
        assert_eq!(out, vec!["b", "c", "FAIL here", "d"]);
    }

    #[test]
    fn keep_around_match_overlapping_windows() {
        // Two matches close together — post-match window from first overlaps second match.
        let pattern = Regex::new(r"^FAIL").unwrap();
        let mut s = Stage::KeepAroundMatch {
            pattern,
            before: 0,
            after: 3,
            ctx_buf: VecDeque::new(),
            emit_after: 0,
        };
        let out = run_stage(
            &mut s,
            &["line1", "FAIL1", "line3", "FAIL2", "line5", "line6", "line7"],
        );
        // FAIL1 triggers after=3: line3, FAIL2 (match), line5, line6
        // FAIL2 resets emit_after to 3: line5, line6, line7
        // Unique lines in order: FAIL1, line3, FAIL2, line5, line6, line7
        assert_eq!(
            out,
            vec!["FAIL1", "line3", "FAIL2", "line5", "line6", "line7"]
        );
    }

    // ── MaxBytes ─────────────────────────────────────────────────────────────

    #[test]
    fn max_bytes_exact_boundary() {
        // Each line "abc" = 3 chars + 1 \n = 4 bytes. Cap = 8 → fits exactly 2 lines.
        let mut s = Stage::MaxBytes {
            cap: 8,
            written: 0,
            truncated: false,
            dropped_bytes: 0,
        };
        let out = run_stage(&mut s, &["abc", "abc", "abc"]);
        // First 2 fit (8 bytes); third overflows → marker emitted at flush
        assert_eq!(out.len(), 3, "output: {:?}", out);
        assert_eq!(out[0], "abc");
        assert_eq!(out[1], "abc");
        assert!(out[2].starts_with("[lacon: truncated, "), "marker: {:?}", out[2]);
        assert!(out[2].ends_with(" more bytes dropped]"), "marker: {:?}", out[2]);
    }

    #[test]
    fn max_bytes_truncation_marker_format() {
        // D-08 + T-02-05: assert exact marker format string.
        // CR-04: marker is deferred to flush(), so we must run the full stage.
        let mut s = Stage::MaxBytes {
            cap: 5,
            written: 0,
            truncated: false,
            dropped_bytes: 0,
        };
        let out = run_stage(&mut s, &["hello"]); // 5+1=6 > 5 → overflow; marker at flush
        assert_eq!(out.len(), 1, "expected exactly 1 output line (the marker): {:?}", out);
        let marker = &out[0];
        assert!(
            marker.starts_with("[lacon: truncated, "),
            "marker must start with '[lacon: truncated, ', got: {:?}",
            marker
        );
        assert!(
            marker.ends_with(" more bytes dropped]"),
            "marker must end with ' more bytes dropped]', got: {:?}",
            marker
        );
    }

    #[test]
    fn max_bytes_drops_after_truncation() {
        let mut s = Stage::MaxBytes {
            cap: 5,
            written: 0,
            truncated: false,
            dropped_bytes: 0,
        };
        let out = run_stage(&mut s, &["hello", "world", "extra"]);
        // "hello\n" = 6 > 5 → all 3 lines dropped; marker emitted at flush
        assert_eq!(out.len(), 1, "output: {:?}", out);
        assert!(out[0].starts_with("[lacon: truncated, "));
    }

    #[test]
    fn max_bytes_cumulative_drop_count() {
        // CR-04 regression: marker must report TOTAL bytes dropped across all post-cap lines.
        // "ab\n"=3 bytes, "cd\n"=3 bytes, "ef\n"=3 bytes. Cap = 3 → only first line fits.
        // Dropped: "cd\n"=3 + "ef\n"=3 = 6 bytes total.
        let mut s = Stage::MaxBytes {
            cap: 3,
            written: 0,
            truncated: false,
            dropped_bytes: 0,
        };
        let out = run_stage(&mut s, &["ab", "cd", "ef"]);
        assert_eq!(out.len(), 2, "one pass-through line + marker: {:?}", out);
        assert_eq!(out[0], "ab");
        let marker = &out[1];
        assert!(marker.starts_with("[lacon: truncated, "), "marker: {:?}", marker);
        // The marker should report 6 bytes (cd\n=3 + ef\n=3), not just the first overflow line (3).
        assert!(
            marker.contains("6 more bytes dropped"),
            "marker must report cumulative 6 bytes, got: {:?}",
            marker
        );
    }

    // Helper for single-step without flush
    #[allow(dead_code)]
    fn stage_step_str<'a>(stage: &mut Stage, line: &'a str, out: &mut LineOut<'a>) {
        stage.step(Cow::Borrowed(line), out);
    }
}
