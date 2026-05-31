---
phase: 09-output-fidelity-safety-no-fabrication-on-dedupe-collapse-and
reviewed: 2026-05-31T06:26:04Z
depth: standard
files_reviewed: 8
files_reviewed_list:
  - crates/lacon-adapter-claudecode/src/lib.rs
  - crates/lacon-adapter-claudecode/tests/hook_e2e.rs
  - crates/lacon-cli/tests/cli_run.rs
  - crates/lacon-core/src/pipeline/stages.rs
  - bundled-rules/git-status.yaml
  - docs/specs/filter-rule-schema.md
  - tests/fixtures/git-status/many-untracked/meta.yaml
  - tests/fixtures/git-status/tabular-signal/meta.yaml
findings:
  critical: 1
  warning: 4
  info: 3
  total: 8
status: issues_found
---

# Phase 9: Code Review Report

**Reviewed:** 2026-05-31T06:26:04Z
**Depth:** standard
**Files Reviewed:** 8
**Status:** issues_found

## Summary

Reviewed the Phase 9 output-fidelity changes: the inline `LACON_DISABLE=1`
env-prefix bypass parser in the Claude Code adapter, the standardized
`[lacon: collapsed N lines]` marker in `collapse_repeated`, the removal of the
`collapse_repeated` stage from `git-status.yaml`, and the spec/fixture updates.

The inline bypass parser is mostly correct and the safe-direction analysis holds
(the failure mode that matters — accidentally dropping filtering on a NON-bypass
command — is well-guarded by the leading-token scan that stops at the command
word). The `collapse_repeated` marker change is correct and the fabrication
concern is genuinely resolved: the marker is a fixed lacon-namespaced literal
that cannot inherit the elided lines' formatting.

The one blocker is a **spec/implementation contradiction**: the spec now
explicitly promises that the `summary` key is optional and that rules omitting it
"continue to parse," but `CollapseArgs.summary` is a required field with
`deny_unknown_fields`, so any rule that drops the key per the spec's own advice
fails to load. This is a correctness defect against the published contract, which
the spec itself declares "a breaking change for users."

The warnings center on a leading-assignment scan that diverges from POSIX shell
semantics in ways that mostly fail safe but include at least one case that fails
toward NOT bypassing when the user intended to bypass, plus dead/misleading state
fields carried in the `Stage` enum.

## Critical Issues

### CR-01: Spec promises `summary` is optional but the loader requires it — rules following the spec's own guidance fail to load

**File:** `docs/specs/filter-rule-schema.md:140`, `crates/lacon-core/src/rules/schema.rs:238-245`
**Issue:**
The Phase 9 spec change states (filter-rule-schema.md:140):

> The `summary` key is still accepted by the YAML loader for backward
> compatibility (rules carrying it continue to parse), but its value is ignored
> at emission time. **Rules should drop the key**; relying on a custom summary
> string is unsupported.

This tells users two things: (1) old rules carrying `summary` keep parsing, and
(2) new rules should drop the key. But the loader struct is:

```rust
#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
pub struct CollapseArgs {
    pub pattern: String,
    pub max_kept: usize,
    pub summary: String,   // no #[serde(default)] — REQUIRED
}
```

`summary` has no `#[serde(default)]`, so a `collapse_repeated` stage **without**
`summary:` fails to deserialize with a "missing field `summary`" error. A user who
follows the spec's explicit instruction to "drop the key" gets a rule that no
longer loads — the exact opposite of the documented contract. The spec header
declares any schema change here "a breaking change for users," so shipping a spec
that contradicts the loader is a real defect, not a doc nit.

The first clause ("rules carrying it continue to parse") is the only part that
holds today; the "should drop the key" guidance is actively wrong.

**Fix:** Make `summary` optional so the spec's promise is true. Since the value
is no longer emitted, it can default to empty:

```rust
#[serde(default)]
pub summary: String,
```

Then `summary_template: summary` at `loader.rs:749` simply carries an empty
(unused) string, which is fine because `stages.rs` no longer reads
`summary_template`. Alternatively, change `summary: String` to
`summary: Option<String>` and drop the field name from the destructure at
`loader.rs:745`. Either way, add a loader test that a `collapse_repeated` stage
with NO `summary:` key parses successfully — there is currently no such test, so
the contradiction is invisible to CI.

## Warnings

### WR-01: `dead_code` / misleading state fields retained in `Stage` after the marker change

**File:** `crates/lacon-core/src/pipeline/stages.rs:97-106, 273-279`
**Issue:**
`Stage::CollapseRepeated.summary_template` is now write-only — the loader
populates it (`loader.rs:749`) but `step()`/`flush()` never read it (the field is
destructured as `summary_template: _` at line 276). The doc comment at lines
95-99 still describes a `{count}` placeholder template and says "`{count}` ... is
replaced with the total number of dropped lines," which is no longer true — the
template is never substituted or emitted. A future maintainer reading this
comment will believe the template is live behavior.

This is dead state threaded through the whole construction path purely for YAML
deserialization, and the stale doc actively misleads. Note the related
`Dedupe.kept_so_far` field is also documented as "unused — kept for interface
compat" (line 82), suggesting a pattern of carrying dead fields.

**Fix:** Either drop `summary_template` from the `Stage` variant entirely (keep
the `summary` only on the deserialization-layer `CollapseArgs`, discard it in the
loader when constructing the `Stage`), or, if it must stay for some deserialize
symmetry, update the doc comment at lines 95-99 to state plainly that
`summary_template` is retained only for backward-compatible parsing and is never
emitted (D-09), mirroring the inline comment already at lines 292-296.

### WR-02: Inline bypass scan diverges from POSIX assignment semantics — `export`-style and tab/newline-separated forms mis-handled

**File:** `crates/lacon-adapter-claudecode/src/lib.rs:78-90`
**Issue:**
`inline_disable_bypass` splits on `split_whitespace()` and treats each leading
`NAME=value` token as an assignment, stopping at the first non-assignment token.
Two real shells-vs-scanner divergences:

1. **`split_whitespace` collapses newlines/tabs into token boundaries**, so a
   command like `"FOO=bar\nLACON_DISABLE=1 rm -rf /"` would be scanned as if
   `LACON_DISABLE=1` were a leading assignment prefix and bypass — but in a real
   shell the newline terminates the first simple command (`FOO=bar` alone, a
   pure-assignment statement), and `LACON_DISABLE=1 rm -rf /` is a *separate*
   command. The scanner bypasses filtering on the entire multi-line input when
   the shell would treat it as two commands. Because the safe direction is
   bypass, this is not a security hole, but it silently drops filtering on a
   second command the user did not prefix — a fidelity regression for chained /
   multi-line input. (Compare: `chain.rs` deliberately tracks newlines and
   heredocs; `detect_bypass` runs *before* `split_chain`, so it never gets that
   precision.)

2. The doc comment (lines 64-68) claims POSIX "command word" detection, but a
   leading `export LACON_DISABLE=1 ...` would stop at `export` (not an
   assignment) and NOT bypass, even though `export` makes it an assignment in
   intent. This is the safe direction (no bypass) so it is acceptable, but the
   "only usable escape hatch" framing (lines 40-42) means an agent that writes
   `export LACON_DISABLE=1; cmd` silently gets filtering instead of the bypass it
   asked for — a usability failure for the one escape hatch the design says
   agents must rely on.

**Fix:** At minimum, scope the scan to the first line to match shell statement
semantics — split on the first `\n`/`\r`/`;`/`&`/`|` boundary before scanning
leading assignments (or reuse the existing `chain.rs` segmentation and only scan
the first segment's leading tokens). Document explicitly that `export`-prefixed
and other builtin-prefixed forms are NOT recognized escape hatches, so the
"only usable escape hatch" claim does not overpromise.

### WR-03: `unquote_one_layer` accepts mismatched-but-same-delimiter degenerate values and a lone-quote pair

**File:** `crates/lacon-adapter-claudecode/src/lib.rs:113-123`
**Issue:**
`unquote_one_layer` strips one layer iff the first and last bytes are both `"` or
both `'`. For the bypass decision this is only ever compared against the literal
`"1"`, so the blast radius is tiny, but two edge cases are worth noting:

1. A value of exactly `"\"\""` (the two-byte string `""`, i.e. an empty
   double-quoted value) unquotes to the empty string — correct, no bypass. Fine.
2. The function operates on raw bytes and indexes `value[1..value.len()-1]`,
   which is safe for ASCII quotes but assumes the value is well-formed UTF-8 at a
   char boundary. Since `value` came from a `&str` and the stripped bytes are
   ASCII quotes, the slice is always on a char boundary — so no panic — but the
   reasoning is implicit. More importantly, the function does NOT validate that
   the quote characters are *balanced as a pair* vs. coincidentally matching: a
   value like `"1` (leading quote, no trailing) is correctly left as-is, but a
   value like `'a'b'c'` (first and last both `'`) is unquoted to `a'b'c`, which a
   real shell would parse very differently. None of these reach the `== "1"`
   comparison as a bypass, so the failure direction is safe (no bypass), but the
   "matches a single shell parse pass" claim in the doc (line 112) overstates the
   fidelity.

**Fix:** No behavior change is strictly required since every divergence fails
toward "not equal to `1`" (no bypass). Tighten the doc comment to say the helper
recognizes ONLY a fully-surrounding matched-delimiter pair and makes no claim of
shell-accurate quote parsing. If stricter matching is wanted later, reject values
containing an interior unescaped quote of the same kind.

### WR-04: `collapse_repeated` marker can still be spoofed by upstream tool output (no escaping of literal `[lacon: ...]` lines)

**File:** `crates/lacon-core/src/pipeline/stages.rs:298, 447`
**Issue:**
The phase goal is "no fabrication" — the marker must be distinguishable from real
tool output. The marker is now a fixed `[lacon: collapsed N lines]` literal, which
solves the *blending* problem (it no longer inherits tab-indent formatting).
However, nothing prevents a tool from itself emitting a line that is
byte-identical to the lacon marker — e.g. a build tool that prints
`[lacon: collapsed 5 lines]` for unrelated reasons, or an adversarial repo whose
filenames/log lines reproduce the marker. Because the marker carries no sentinel
that distinguishes lacon-authored lines from passthrough lines, a downstream
consumer (or the model) cannot tell a real elision marker from a spoofed one. The
spec (filter-rule-schema.md:138) asserts "a collapsed run is never mistaken for,
or substituted by, a plausible-but-fabricated tool line" — that holds for the
*substitution* direction but not the *spoofing* direction.

This is the same latent issue as the `[lacon: truncated, ...]` marker and is
arguably pre-existing / out of the Phase 9 diff's literal scope, but the phase's
explicit charter is output-fidelity-safety against fabrication, so it is worth
recording rather than silently accepting.

**Fix:** Out of v1 scope to fully solve (would need a structural channel, e.g.
emitting markers on a side band or escaping passthrough lines that collide with
the `[lacon:` prefix). At minimum, document in the spec that the textual marker is
advisory and can be coincidentally reproduced by tool output, so consumers do not
treat its presence as a trusted lacon-only signal. Track as a backlog item if a
trusted channel is needed.

## Info

### IN-01: `Dedupe.kept_so_far` is dead state explicitly labeled "unused"

**File:** `crates/lacon-core/src/pipeline/stages.rs:82-88, 256`
**Issue:** `kept_so_far` on `Stage::Dedupe` is documented "unused — kept for
interface compat" and destructured as `kept_so_far: _` in `step()`. It is dead
state on the enum and on every construction site. Not introduced by Phase 9, but
adjacent to the `summary_template` dead-field issue (WR-01) and worth cleaning up
together.
**Fix:** Remove the field from the `Dedupe` variant and its construction in
`loader.rs` (lines ~736-742) unless an external serialization contract requires
it.

### IN-02: `docs/primitive-reference.md` still documents the old free-form summary behavior

**File:** `docs/primitive-reference.md:210-243`
**Issue:** `primitive-reference.md` still describes `collapse_repeated` as
emitting "the `summary` template's `{count}` placeholder ... replaced" and shows
`matching lines are replaced by the summary '… 199 progress lines'`. After the
Phase 9 change the emitted line is the fixed `[lacon: collapsed N lines]` marker,
not the summary template. This doc was not in the review file set but is a sibling
of the spec that Phase 9 updated, and it now contradicts the shipped behavior.
**Fix:** Update `primitive-reference.md` to match `filter-rule-schema.md:128-140`
(fixed marker, `summary` deprecated/ignored).

### IN-03: No test asserts the exact full marker string for `collapse_repeated`

**File:** `crates/lacon-core/src/pipeline/stages.rs:600-713`
**Issue:** The unit tests assert the marker via `assert_eq!` against
`"[lacon: collapsed 3 lines]"` / `"[lacon: collapsed 2 lines]"`, which is good.
But unlike `MaxBytes` (which has a dedicated `max_bytes_truncation_marker_format`
test pinning prefix/suffix), there is no singular "marker format" test naming the
contract string for `collapse_repeated`. Given the marker is now a stability
contract (spec line 136 pins the exact form), a dedicated format-pinning test
would guard against a future whitespace/pluralization drift (e.g. "1 lines").
**Fix:** Add a test pinning the exact byte string for the N=1 case
(`[lacon: collapsed 1 lines]` — note the non-pluralized "lines" is itself a minor
fidelity nit worth a decision) and document whether singular/plural is
intentional.

---

_Reviewed: 2026-05-31T06:26:04Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
