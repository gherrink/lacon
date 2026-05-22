---
phase: 05-bundled-tier-1-rules
reviewed: 2026-05-22T05:45:00Z
depth: standard
files_reviewed: 14
files_reviewed_list:
  - crates/lacon-core/src/rules/loader.rs
  - crates/lacon-core/src/rules/bundled.rs
  - crates/lacon-core/tests/bundled_rules.rs
  - bundled-rules/test-base.yaml
  - bundled-rules/cargo-test.yaml
  - bundled-rules/cargo-build.yaml
  - bundled-rules/pkg-install.yaml
  - bundled-rules/git-status.yaml
  - bundled-rules/docker-build.yaml
  - bundled-rules/tsc.yaml
  - bundled-rules/eslint.yaml
  - bundled-rules/pytest.yaml
  - bundled-rules/vitest.yaml
  - bundled-rules/jest.yaml
findings:
  critical: 0
  warning: 4
  info: 6
  total: 10
status: issues_found
---

# Phase 5: Code Review Report

**Reviewed:** 2026-05-22T05:45:00Z
**Depth:** standard
**Files Reviewed:** 14 (3 Rust + 11 YAML)
**Status:** issues_found

## Summary

Phase 5 ships the bundled Tier-1 rule set (10 active rules + 1 inert parent), the
loader change that wires bundled→bundled `extends` into the eager `load_all` path,
and the fixture-walking integration test.

The implementation is **substantially correct and well-engineered**. I verified
the following against the live code, not just by reading:

- **All 67 regex patterns compile** under the real `regex` 1.x crate (62 via
  `Regex::new`, 5 `keep_regex` via `RegexSet::new`). None use look-around or
  backreferences. Confirmed by compiling a temporary harness against the crate.
- **The loader fix is correct.** `load_all()` now resolves bundled→bundled
  `extends` via `find_in_bundled` and produces all 11 rules with zero errors.
  The lazy `resolve()` path and the eager `load_all()` path produce **identical**
  flattened pipelines (cargo-test = 11 success stages on both paths). Cycle
  detection is sound: a self- or mutual-extends in the bundled set seeds a fresh
  `visited` set per rule and recurses parent-first, so a cycle hits
  `CircularExtends` rather than looping — verified by reading the recursion in
  `flatten_extends_with_lookup` (loader.rs:515-562).
- **Exit-code routing in the test mirrors the runner.** `replay()` →
  `filter_bytes` routes `0`→success, nonzero+on_error→on_error, nonzero+none→raw
  passthrough (runtime/mod.rs:457-473). Every bundled rule defines `on_error`, so
  no rule hits the raw-passthrough branch.
- **Division-by-zero is handled.** The reduction check guards `input_len > 0` with
  an actionable message before dividing (bundled_rules.rs:123-127); empty-input
  fixtures (tsc/clean, eslint/clean) are correctly `exempt_from_reduction_check`.
- **All 20 shipped fixtures pass** and error signal survives every `on_error`
  pipeline (E404, panic/assertion, docker ERROR frame, fatal:, etc.).
- **Matching is correct**, including the negative case (`pnpm test` does NOT match
  `pkg-install`) and `python -m pytest`.

The findings below are edge-case behavior risks and documentation/accuracy
defects, not correctness bugs in the happy path. No Critical issues.

## Warnings

### WR-01: `keep_around_match` → `keep_tail` ordering can drop the FIRST error when a failure block exceeds the tail cap

**File:** `bundled-rules/test-base.yaml:14-19`, `bundled-rules/cargo-build.yaml:20-24`, `bundled-rules/docker-build.yaml:22-29`, `bundled-rules/pkg-install.yaml:34-38`
**Issue:** Several `on_error` pipelines run `keep_around_match` (or `keep_regex`)
*before* `keep_tail: { lines: N }`. Because `keep_tail` keeps the **last** N lines
of whatever the prior stage emitted, when a match-heavy failure produces more than
N kept lines, the tail cap **discards the head — including the first/root error**.

Verified directly: feeding test-base's `on_error` an input with an early
`error: FIRST FAILURE at top` followed by 80 more `error:` lines yields 60 output
lines whose first line is `error: failure number 21` — `FIRST FAILURE` is gone.
For most real failures the block is < N lines so this never triggers, but the
common "first error is the root cause" heuristic is exactly what gets truncated in
the pathological case.

**Fix:** For error pipelines, prefer `keep_head` over (or in addition to)
`keep_tail` so the first error survives, or raise the cap, or drop the trailing
`keep_tail` and rely on the auto-injected `max_bytes` ceiling (which truncates the
tail, preserving the head):
```yaml
on_error:
  pipeline:
    - strip_ansi
    - keep_around_match: { pattern: '(?i)(FAILED|panicked|error|assertion)', before: 1, after: 6 }
    # keep_tail drops the head; on error the head is usually the root cause.
    # Option A: cap from the top instead.
    - keep_head: { lines: 60 }
```
At minimum, document this as intended behavior in each rule's comment so a future
author does not assume the first error is always preserved.

### WR-02: `test-base` is matchable as a real command, contradicting its "INERT in command resolution" claim

**File:** `bundled-rules/test-base.yaml:8-11`
**Issue:** The description states test-base "is INERT in command resolution: its
`match` is a sentinel that no real command produces, so it is only ever reached
via `extends`, never matched directly." This is **false as written**:
`match_argv_via_load_all` returns `test-base` for argv
`["__lacon_test_base_never_matches__"]`, and `resolve("test-base")` succeeds.
The rule is reachable both by ID and by (an admittedly unguessable) command line.
The risk is low — no human runs that command — but the inertness is a comment, not
an enforced property, so a future refactor that loosens the regex or a copy-paste
of this pattern could silently expose a parentless rule.

**Fix:** Either (a) downgrade the claim to "no real-world command matches this
sentinel" (accurate), or (b) make inertness structural by giving the loader a way
to mark parent-only rules as non-matchable (e.g. omit `match` entirely and have
`extends`-only children inherit nothing matchable). Option (a) is sufficient for
v1; the wording is the defect.

### WR-03: `tsc` `args_prefix: []` matches `tsc` with ANY arguments, not just bare/`--noEmit`

**File:** `bundled-rules/tsc.yaml:4-7`
**Issue:** The comment says the rule matches "concrete tsc invocations (bare `tsc`
and `tsc --noEmit`)." But `args_prefix: []` is an empty prefix, which matches
**every** `tsc` invocation regardless of args (the prefix loop in
loader.rs:404-413 is a no-op for an empty list). Verified: `tsc -p tsconfig.json`
matches. This makes the second `any` alternative (`args_prefix: ['--noEmit']`)
**dead/redundant** — the empty-prefix branch already swallows everything.

This is arguably the *desired* breadth (you probably want to filter all `tsc`
runs), but the rule's stated intent ("concrete invocations only") is the opposite
of its behavior, and the redundant alternative is misleading. If breadth is
intended, the rule contradicts the D-10 "match concrete invocations only" rationale
cited in `pkg-install.yaml:3` for the sibling rules.
**Fix:** Decide and align comment with behavior:
```yaml
# Intent: filter ALL tsc invocations.
match:
  command: tsc          # matches tsc with any args; no args_prefix needed
```
or, to genuinely restrict to bare + `--noEmit`, the matcher needs an exact-args
operator that the schema does not currently provide — in which case file a backlog
item rather than relying on `args_prefix: []`.

### WR-04: `pkg-install` success-path blacklist can drop meaningful peer-dependency / lockfile signal

**File:** `bundled-rules/pkg-install.yaml:25-27`
**Issue:** On the **success** path (exit 0), `^info `, `^warning `, and `^success `
drop any line with those prefixes. The trailing space avoids most false positives
(`info:`/`warning:` with a colon survive), but yarn-classic emits actionable
content as `warning <pkg> has unmet peer dependency ...` and
`warning <pkg> > <dep>: deprecated`, which would be **silently dropped** even
though they are exactly the kind of signal a developer wants. Confirmed:
`warning unused variable x` → DROP. Because this is the success path, these lines
do NOT get rescued by `on_error`.

This is the documented blacklist tradeoff (the rule comment defends the
no-`--silent` decision), but dropping peer-dependency warnings is a real
signal-loss risk for an install rule whose whole job is to preserve install
problems.
**Fix:** Narrow the patterns to the known-noise shapes rather than the bare
keyword, e.g. `^warning Ignored build scripts` / `^warning .* has unmet peer`
should be a *keep*, not a drop; or anchor to the specific noisy prefixes yarn
emits (`^warning package-lock.json found`). If broad dropping is intentional,
add a comment noting peer-dep warnings are sacrificed.

## Info

### IN-01: `cargo-build` / `pkg-install` / `docker-build` / `eslint` `on_error` use a bare/over-broad match anchor

**File:** `bundled-rules/cargo-build.yaml:23`, `bundled-rules/docker-build.yaml:28`, `bundled-rules/pkg-install.yaml:37`
**Issue:** `keep_around_match: { pattern: '^error', ... }` (cargo-build) and
`pattern: 'ERROR'` (docker-build, unanchored) match broadly. `'ERROR'` unanchored
will trigger on any line *containing* ERROR (e.g. `#5 12.3 Resolved ERROR_CODES
table`), keeping 15 lines of context around incidental matches and inflating
output. This degrades reduction quality on failure but never drops signal, so it
is Info, not Warning.
**Fix:** Anchor where the tool's framing allows, e.g. docker BuildKit error
headers are `^#\d+ ERROR:` and the final line is `^ERROR: failed`:
```yaml
- keep_around_match: { pattern: '^(#\d+ )?ERROR', before: 2, after: 15 }
```

### IN-02: `pkg-install` `^warning ` / `^info ` / `^success ` collide with non-install tools sharing the manager (low risk)

**File:** `bundled-rules/pkg-install.yaml:25-27`
**Issue:** Companion to WR-04. The bare-keyword drops are install-formatter
specific (yarn-classic style) but the rule also matches `npm install` /
`pnpm install`, which do not emit these exact prefixes — so the patterns are inert
for some matched managers and aggressive for others. Not incorrect, just uneven
coverage across the managers the single rule claims to serve.
**Fix:** None required for v1; note in the rule comment which manager each pattern
targets.

### IN-03: Reduction check trims trailing newlines on both sides, masking trailing-blank-line differences

**File:** `crates/lacon-core/tests/bundled_rules.rs:110-116`
**Issue:** The byte-exact assertion does `actual.trim_end_matches('\n')` vs
`expected.trim_end_matches('\n')`. This is the documented D-04 idiom and is
correct for the single-trailing-newline cosmetic case, but it also masks a real
difference where the pipeline emits (or fails to emit) **multiple** trailing blank
lines vs expected. A rule that accidentally appended two trailing blanks would
still pass. Low risk for the current fixtures.
**Fix:** Acceptable for v1; if tightening later, compare with a single normalized
trailing newline rather than stripping all of them.

### IN-04: `cargo-test` success path lists `Running`/`Updating`/`Locking`/`Downloading`/`Downloaded` drops that `cargo-build` omits

**File:** `bundled-rules/cargo-test.yaml:15-23` vs `bundled-rules/cargo-build.yaml:13-16`
**Issue:** The two cargo rules drop overlapping-but-different scaffolding sets
(cargo-test drops `Running`/`Downloading`/etc.; cargo-build does not). This is
likely intentional (build emits fewer of those), but the divergence is undocumented
and invites drift — a maintainer fixing one will not know to fix the other.
**Fix:** Add a one-line comment in each pointing at the other, or factor the shared
cargo-scaffolding drops into a `cargo-base` parent (mirroring the `test-base`
pattern) so the two stay in sync.

### IN-05: `dedupe` in `tsc` only collapses *consecutive* duplicates — non-adjacent repeats survive

**File:** `bundled-rules/tsc.yaml:14,19`
**Issue:** The comment says reduction comes from "collapsing accidental
duplicates." `Stage::Dedupe` (stages.rs:256-270) only collapses **consecutive**
identical lines; tsc's repeated `error TSxxxx` lines for the same root cause are
typically interleaved with file/context lines, so `dedupe` will rarely fire. The
rule is harmless (it falls back to `keep_tail: 100` + the auto `max_bytes`), but
the comment overstates `dedupe`'s contribution.
**Fix:** Documentation only — clarify that `dedupe` catches exact back-to-back
repeats and the real cap is `keep_tail`/`max_bytes`.

### IN-06: Unused `FixtureMeta` fields (`tool_version`, `os`, `notes`) carry `#[allow(dead_code)]`

**File:** `crates/lacon-core/tests/bundled_rules.rs:48-62`
**Issue:** Three deserialized fields are read by serde but never used by the test
logic, suppressed with `#[allow(dead_code)]`. This is deliberate (provenance kept
in `meta.yaml` for humans) and the struct comment explains the NO-`deny_unknown_fields`
choice, so it is acceptable. Flagged only for completeness.
**Fix:** None required. Optionally drop the fields from the struct and rely on the
already-permissive deserializer (no `deny_unknown_fields`) to ignore them, removing
the `#[allow]` attributes.

---

_Reviewed: 2026-05-22T05:45:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
