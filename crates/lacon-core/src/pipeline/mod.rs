//! Streaming pipeline runner.
//!
//! # Design (D-05, D-06)
//! A `Pipeline` wraps a `Vec<Stage>` and drives lines through each stage in order.
//! `Pipeline::new` performs the load-time `KeepRegex` OR-merge (D-06): any run of
//! adjacent `Stage::KeepRegex` variants is collapsed into a single `RegexSet` so that
//! all patterns are evaluated in one NFA pass rather than N separate passes.
//!
//! # RegexSet OR-merge contract (D-06)
//! PLAN-03 (rule loader) emits one `Stage::KeepRegex(RegexSet::new([single_pattern]))`
//! per `keep_regex:` YAML entry. `Pipeline::new` merges adjacent runs into a single
//! `RegexSet`. Non-adjacent `KeepRegex` stages are kept separate (they operate on
//! already-filtered output and must each enforce their own whitelist independently).

pub mod stages;

use std::borrow::Cow;

use regex::RegexSet;
use smallvec::SmallVec;

use stages::{LineOut, Stage};

/// A streaming pipeline of native filtering primitives.
///
/// Construct with `Pipeline::new(stages)` and drive with `Pipeline::run(lines_iter)`.
pub struct Pipeline {
    stages: Vec<Stage>,
}

impl Pipeline {
    /// Construct a pipeline from a flat `Vec<Stage>`, performing the load-time
    /// `KeepRegex` OR-merge (D-06) for adjacent `KeepRegex` variants.
    ///
    /// # OR-merge algorithm
    /// Walk `stages` left-to-right, collecting runs of consecutive `KeepRegex` variants.
    /// When a run ends (different stage type or end of list), replace the entire run with
    /// a single `Stage::KeepRegex(RegexSet::new(all_patterns))`.
    ///
    /// A single-element run is kept as-is (the `RegexSet` already holds one pattern).
    /// An empty `stages` input produces an empty pipeline (pass-through).
    ///
    /// # Panics
    /// Does not panic. Pattern validity is enforced by PLAN-03's loader
    /// (`Regex::new` / `RegexSet::new` returns `Err` which the loader surfaces as
    /// `ValidationError::InvalidRegex`). By the time `Pipeline::new` is called, all
    /// patterns are already validated.
    pub fn new(stages: Vec<Stage>) -> Self {
        let merged = merge_keep_regex_stages(stages);
        Self { stages: merged }
    }

    /// Drive an iterator of lines through all pipeline stages and return the filtered output.
    ///
    /// End-of-stream `flush()` is called on every stage after the last line, allowing
    /// stateful stages (`KeepTail`, `CollapseRepeated`) to emit buffered output.
    ///
    /// # Memory model
    /// Two `SmallVec<[Cow<str>; 2]>` buffers are swapped between stages to avoid
    /// per-line heap allocation. Allocation only occurs when a stage produces more than
    /// 2 output lines from a single input line (rare).
    pub fn run<I: Iterator<Item = String>>(&mut self, lines: I) -> Vec<String> {
        let mut output: Vec<String> = Vec::new();

        // Two staging buffers — reused across all lines to minimize allocation.
        let mut buf_a: LineOut = SmallVec::new();
        let mut buf_b: LineOut = SmallVec::new();

        for line in lines {
            // Seed buf_a with the current line.
            buf_a.clear();
            buf_a.push(Cow::Owned(line));

            // Pass buf_a through each stage into buf_b, then swap.
            for stage in &mut self.stages {
                buf_b.clear();
                for item in buf_a.drain(..) {
                    stage.step(item, &mut buf_b);
                }
                std::mem::swap(&mut buf_a, &mut buf_b);
            }

            // buf_a now holds the output of the last stage.
            for item in buf_a.drain(..) {
                output.push(item.into_owned());
            }
        }

        // Flush all stages in order, propagating flushed lines through subsequent stages.
        for stage_idx in 0..self.stages.len() {
            // Flush stage at stage_idx into buf_a.
            buf_a.clear();
            self.stages[stage_idx].flush(&mut buf_a);

            if buf_a.is_empty() {
                continue;
            }

            // Propagate flushed lines through remaining stages (stage_idx+1 .. end).
            for further_idx in (stage_idx + 1)..self.stages.len() {
                buf_b.clear();
                for item in buf_a.drain(..) {
                    self.stages[further_idx].step(item, &mut buf_b);
                }
                std::mem::swap(&mut buf_a, &mut buf_b);
            }

            for item in buf_a.drain(..) {
                output.push(item.into_owned());
            }
        }

        output
    }

    /// Returns the number of stages in the pipeline (after OR-merge).
    pub fn stage_count(&self) -> usize {
        self.stages.len()
    }
}

/// Merge adjacent `Stage::KeepRegex` runs into a single `RegexSet` (D-06).
///
/// This is a standalone function so it can be unit-tested independently.
fn merge_keep_regex_stages(stages: Vec<Stage>) -> Vec<Stage> {
    let mut result: Vec<Stage> = Vec::with_capacity(stages.len());
    let mut pending_patterns: Vec<String> = Vec::new();

    for stage in stages {
        match stage {
            Stage::KeepRegex(set) => {
                // Accumulate all patterns from this RegexSet.
                for pattern in set.patterns() {
                    pending_patterns.push(pattern.to_owned());
                }
            }
            other => {
                // Non-KeepRegex stage: flush any pending patterns first.
                if !pending_patterns.is_empty() {
                    let merged_set = RegexSet::new(&pending_patterns)
                        .expect("patterns were already validated; merge cannot fail");
                    result.push(Stage::KeepRegex(merged_set));
                    pending_patterns.clear();
                }
                result.push(other);
            }
        }
    }

    // Flush any trailing KeepRegex run.
    if !pending_patterns.is_empty() {
        let merged_set = RegexSet::new(&pending_patterns)
            .expect("patterns were already validated; merge cannot fail");
        result.push(Stage::KeepRegex(merged_set));
    }

    result
}

// ═══════════════════════════════════════════════════════════════════════════════
// Inline unit tests
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use stages::{HeadTailMode, Stage};

    fn lines(s: &str) -> impl Iterator<Item = String> + '_ {
        s.lines().map(str::to_owned)
    }

    #[test]
    fn pipeline_empty_passthrough() {
        let mut p = Pipeline::new(vec![]);
        let out = p.run(lines("a\nb\nc"));
        assert_eq!(out, vec!["a", "b", "c"]);
    }

    #[test]
    fn pipeline_stage_count_after_merge() {
        // Three adjacent KeepRegex stages should collapse into 1.
        let stages = vec![
            Stage::KeepRegex(RegexSet::new(["error"]).unwrap()),
            Stage::KeepRegex(RegexSet::new(["FAIL"]).unwrap()),
            Stage::KeepRegex(RegexSet::new(["panic"]).unwrap()),
        ];
        let p = Pipeline::new(stages);
        assert_eq!(p.stage_count(), 1, "3 adjacent KeepRegex → 1 merged stage");
    }

    #[test]
    fn pipeline_or_merge_matches_all_patterns() {
        // After merging, all three patterns should still match.
        let stages = vec![
            Stage::KeepRegex(RegexSet::new(["error"]).unwrap()),
            Stage::KeepRegex(RegexSet::new(["FAIL"]).unwrap()),
            Stage::KeepRegex(RegexSet::new(["panic"]).unwrap()),
        ];
        let mut p = Pipeline::new(stages);
        let out = p.run(lines("ok\nerror: bad\nwarning\nFAIL test\npanic: oops"));
        assert_eq!(out, vec!["error: bad", "FAIL test", "panic: oops"]);
    }

    #[test]
    fn pipeline_non_adjacent_keep_regex_not_merged() {
        // KeepRegex, StripAnsi, KeepRegex → two separate KeepRegex stages.
        let stages = vec![
            Stage::KeepRegex(RegexSet::new(["a"]).unwrap()),
            Stage::StripAnsi,
            Stage::KeepRegex(RegexSet::new(["b"]).unwrap()),
        ];
        let p = Pipeline::new(stages);
        assert_eq!(p.stage_count(), 3, "non-adjacent KeepRegex kept separate");
    }

    #[test]
    fn pipeline_keep_tail_flushed_after_strip_ansi() {
        // KeepTail + StripAnsi in pipeline: flush must propagate through StripAnsi.
        let stages = vec![
            Stage::KeepTail {
                mode: HeadTailMode::Lines(2),
                ring: std::collections::VecDeque::new(),
                byte_count: 0,
            },
            Stage::StripAnsi,
        ];
        let mut p = Pipeline::new(stages);
        let out = p.run(lines("a\nb\n\x1b[31mc\x1b[0m"));
        // KeepTail keeps last 2: "b" and "\x1b[31mc\x1b[0m"
        // StripAnsi strips codes from both during flush propagation
        assert_eq!(out, vec!["b", "c"]);
    }

    #[test]
    fn pipeline_max_bytes_truncation_exact() {
        // "abc\n" = 4 bytes each. cap=8 → 2 lines fit; 3rd triggers truncation.
        let stages = vec![Stage::MaxBytes {
            cap: 8,
            written: 0,
            truncated: false,
        }];
        let mut p = Pipeline::new(stages);
        let out = p.run(lines("abc\nabc\nabc"));
        assert_eq!(out.len(), 3);
        assert_eq!(out[0], "abc");
        assert_eq!(out[1], "abc");
        assert!(
            out[2].contains("[lacon: truncated, "),
            "third element must be truncation marker"
        );
    }
}
