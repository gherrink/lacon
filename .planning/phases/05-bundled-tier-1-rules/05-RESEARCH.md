# Phase 5: Bundled Tier 1 rules - Research

**Researched:** 2026-05-22
**Domain:** YAML filter-rule authoring for ten dev-tool command outputs + a fixture-walking integration test
**Confidence:** HIGH (real output captured locally for 6 of 10 tools; engine semantics verified against source)

## Summary

This phase authors ten Tier 1 YAML rules in `bundled-rules/` plus one fixture-walking integration test. The engine, loader, `extends`, primitives, and `Runner::filter_bytes` are all shipped — Phase 5 only *consumes* them. The CONTEXT.md locks 11 decisions (D-01..D-11) covering the test mechanism, fixture format, and match scope; this research does **not** re-litigate them. It fills the two deferred gaps the discuss phase left: (1) the real current output shapes of all ten tools, and (2) the quiet/silent flag verdict for `pkg-install`'s `rewrite` block.

Real output was captured on this machine for **cargo (build/check/test), git status, pytest, docker build, and npm/pnpm/yarn install** — quoted verbatim below so the executor can reuse it as fixture seed material. The four JS-toolchain tools (`tsc`, `eslint`, `vitest`, `jest`) are NOT installed; their formats are documented from authoritative sources and flagged so the executor captures real fixtures via `npx` in a throwaway node project during execution.

**Primary recommendation:** Author each rule with a `drop_regex`/`collapse_repeated` success pipeline plus a context-preserving `on_error` block (`keep_around_match` on error markers + `keep_tail`). Do **not** add a `rewrite` block to `pkg-install` — research proves no universally-safe silent flag exists (`--silent` deletes the error on failure, violating the no-drop contract). Use the D-11 fallback: pipeline-side filtering only.

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Match a command to a rule | Loader (`resolve`) | — | Shipped; rules only declare `match` |
| Reduce success output | Pipeline (success) | — | `drop_regex`/`collapse_repeated`/`keep_regex` stages, streaming |
| Preserve errors on failure | Pipeline (`on_error`) | — | ADR-0010 branch; replaces, never merges |
| Final byte cap | Loader auto-inject | — | `MaxBytes(32768)` injected; authors omit it (D-07) |
| Replay fixtures without subprocess | `Runner::filter_bytes` | RuleLoader | D-01; mirrors `explain.rs:116-159` |
| Walk fixture tree + assert | New test `bundled_rules.rs` | meta.yaml parse | D-09; this phase's only new Rust code |

**Note:** Every rule capability lives in tiers already shipped. The only genuinely new code is the test runner. Rules are pure declarative YAML.

## Standard Stack

This is a Rust-internal phase with **no new external packages**. The "stack" is the existing engine surface plus the captured tool outputs that seed fixtures.

### Core (existing, verified in source)
| Component | Location | Purpose | Why |
|-----------|----------|---------|-----|
| `RuleLoader::new(None)` | `loader.rs:110` | Hermetic bundled-only resolution | `None` project_dir → resolves straight from embedded layer [VERIFIED: source] |
| `RuleLoader::resolve(id)` | `loader.rs:127` | Lazy single-rule-by-id load | Test runner entry [VERIFIED: source] |
| `Runner::filter_bytes(...)` | `runtime/mod.rs:423` | Subprocess-free byte replay, selects ADR-0010 branch from exit_code | THE core reuse [VERIFIED: source] |
| `serde_saphyr 0.0.26` | workspace dep | `meta.yaml` deserialize | Existing YAML parser; mirror `parse_one` at `loader.rs:439` [VERIFIED: source] |
| `regex` crate v1 | `Cargo.toml:14` | All `*_regex` primitives | **No look-around, no backreferences** (RE2-style) [VERIFIED: Cargo.toml + regex crate docs] |
| rust-embed | `bundled.rs` | `#[folder = "../../bundled-rules/"]` | Dropping `<id>.yaml` into `bundled-rules/` auto-embeds it [VERIFIED: source] |

### `filter_bytes` exact signature (verified `runtime/mod.rs:423`)
```rust
pub fn filter_bytes(
    &mut self,
    merged_bytes: &[u8],
    exit_code: i32,
    duration_ms: u64,
    command_raw: &str,
    project_path: Option<String>,
) -> Result<Vec<String>, RuntimeError>
```
Branch logic (verified `runtime/mod.rs:455-470`): `exit_code == 0` → `success_pipeline`; `!= 0` with `on_error_pipeline` present → `on_error_pipeline`; `!= 0` with no `on_error` → **raw passthrough** (returns input lines unfiltered). This is why D-02's `exit_code` field is load-bearing: without it failure fixtures run the success pipeline.

**Installation:** None. No `npm install`/`cargo add`. This phase adds YAML files, fixture text, and one `.rs` test.

## Package Legitimacy Audit

> Not applicable — this phase installs **zero external packages**. The throwaway projects used to *capture* fixtures (npm/pnpm/yarn installs of `lodash`/`request`/etc.) are scratch artifacts, deleted after capture; their dependencies never enter the lacon repo or its `Cargo.toml`. No registry packages are added to the project.

## Architecture Patterns

### System Architecture Diagram (fixture replay path)

```
meta.yaml (command, exit_code, flags)
        │
        ▼
input.txt (real captured stdout+stderr, bytes) ──┐
        │                                         │
        ▼                                         │
RuleLoader::new(None).resolve("<rule-id>")        │
        │  (embedded bundled layer, hermetic)     │
        ▼                                         ▼
Runner::new(resolved, RunOptions::default())
        │
        ▼
runner.filter_bytes(input_bytes, exit_code, 0, command, None)
        │
        ├── exit_code == 0 ─────► success_pipeline ──┐
        ├── exit_code != 0 + on_error ─► on_error ───┤
        └── exit_code != 0 + none ─► raw passthrough ─┤
                                                      ▼
                                          Vec<String> filtered lines
                                                      │
                          ┌───────────────────────────┼───────────────────────────┐
                          ▼                           ▼                           ▼
            byte-exact == expected.txt    len(out)/len(in) <= 0.5    every must_keep_lines present
            (join "\n" vs trim_end '\n')   (primary success only)     (error survival check)
```

### Component Responsibilities
| File | Responsibility |
|------|----------------|
| `bundled-rules/<id>.yaml` | One rule per tool; declarative `match` + `pipeline` + `on_error` |
| `bundled-rules/test-base.yaml` (optional, D-06) | Shared parent for the 4 test-runner rules |
| `tests/fixtures/<id>/<scenario>/{input,expected,meta}` | One success + one failure scenario per rule, minimum |
| `crates/lacon-core/tests/bundled_rules.rs` | Walks fixture tree, replays via `filter_bytes`, asserts |
| `docs/testing-rules.md` | Edited to add `exit_code` to meta.yaml schema (D-02) |
| `docs/bundled-rules-roadmap.md` | Doc note per rule (REQ-bundled-rules-format) |

### Pattern 1: Success pipeline — blacklist drop + collapse
**What:** Strip ANSI, drop known-noise lines, collapse repeated progress.
**When:** Tools that emit signal interleaved with noise (cargo, docker, pnpm).
```yaml
# Source: filter-rule-schema.md + verified StageSpec snake_case keys (schema.rs)
pipeline:
  - strip_ansi
  - drop_regex: '^\s*Compiling '
  - drop_regex: '^\s*Finished '
  # max_bytes auto-injected (D-07) — do NOT hand-place
```

### Pattern 2: on_error — context-preserving
**What:** Keep error markers with surrounding context plus the tail.
**When:** Every rule whose tools have a distinct failure mode (all ten).
```yaml
# Source: filter-rule-schema.md on_error; ADR-0010 (replaces, not merges)
on_error:
  pipeline:
    - strip_ansi
    - keep_around_match: { pattern: '(?i)^error', before: 0, after: 20 }
    - keep_tail: { lines: 40 }
```
Note `keep_around_match` requires **both** `before` and `after` (no defaults; `deny_unknown_fields`) [VERIFIED: schema.rs:262-268].

### Pattern 3: tsc-style — output IS the signal
**What:** Minimal reduction; mostly ANSI strip + dedupe + tail cap.
**When:** Tools whose entire output is errors (tsc). Reduction comes from ANSI/dedup/cap, not dropping lines.
```yaml
pipeline:
  - strip_ansi
  - dedupe: { max_kept: 1 }
  - keep_tail: { lines: 100 }
```

### Anti-Patterns to Avoid
- **Hand-placing `max_bytes`:** loader auto-injects 32768 onto both success and on_error pipelines (D-07). Only add it to *override* the cap. [VERIFIED: loader.rs compile_pipeline]
- **`script:` inside `pipeline:`:** rejected at load with an explicit error (`loader.rs spec_to_stage`). Starlark only via top-level `post_process` (ADR-0008). [VERIFIED: source]
- **Look-around / backreferences in regex:** the `regex` crate rejects `(?=...)`, `(?!...)`, `\1`. Patterns will fail to compile → rule fails validation. [VERIFIED: regex crate is RE2-style]
- **Relying on non-adjacent `keep_regex` to OR:** see Pitfall 1 — only *adjacent* keep_regex stages OR-merge. Non-adjacent ones AND together.
- **Adding a `--silent` rewrite to `pkg-install`:** deletes errors on failure (see D-11 verdict). Forbidden.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Branch select on exit code | Custom if/else in test | `filter_bytes` exit_code arg | Already implements ADR-0010 [VERIFIED] |
| Byte-exact compare | Custom normalizer | `tests/primitives.rs:42` idiom (`join("\n")` vs `trim_end_matches('\n')`) | D-04; whole suite uses it |
| Fixture path | Hardcoded path | `env!("CARGO_MANIFEST_DIR")/../../tests/fixtures/...` | Workspace-root idiom (D-09) |
| meta.yaml parse | hand parse | `#[derive(Deserialize)]` + `serde_saphyr::from_str` | Mirror `parse_one` [VERIFIED] |
| Reduce install noise via rewrite | `--silent` flag injection | Pipeline-side `drop_regex` | `--silent` deletes errors (D-11) |

**Key insight:** Every reduction this phase needs is expressible in the existing ten primitives. There is no scenario in the ten Tier 1 rules that requires `post_process` Starlark — keep all ten rules pure-native for cold-start and simplicity.

## Common Pitfalls

### Pitfall 1: Non-adjacent `keep_regex` stages AND, not OR
**What goes wrong:** Author writes `keep_regex: error` then a `strip_ansi` then `keep_regex: warning`, expecting "keep lines matching error OR warning." Instead they get "keep lines matching error AND (after strip) matching warning" → almost everything drops.
**Why:** `Pipeline::new` OR-merges only *consecutive* `Stage::KeepRegex` runs into one `RegexSet`. A non-KeepRegex stage between them flushes the run, so they become two separate intersecting filters. [VERIFIED: `pipeline/mod.rs:146-178`, tests at `:206-238`]
**How to avoid:** Put all `keep_regex` stages **adjacent** (back-to-back) if OR is intended. Put `strip_ansi` *before* the keep_regex block, never between two keep_regex stages.
**Warning signs:** Output unexpectedly empty or near-empty; reduction ratio ~0.0.

### Pitfall 2: Failure fixture silently runs the success pipeline
**What goes wrong:** A failure fixture's `meta.yaml` omits `exit_code` (or sets 0), so `filter_bytes` runs the success pipeline and `on_error` is never exercised; the test passes against a wrong-but-stable expected.txt.
**Why:** D-02 — the `exit_code` field gates the branch. testing-rules.md's documented schema lacks it.
**How to avoid:** The runner must require `exit_code` for every fixture (or default success=0 / failure-by-convention). Add `exit_code` to the meta.yaml schema in `docs/testing-rules.md` as a deliverable.
**Warning signs:** on_error pipelines show 0% coverage; failure expected.txt looks like success output.

### Pitfall 3: `regex` crate has no look-around — `^(?!...)` patterns won't compile
**What goes wrong:** Author writes `drop_regex: '^(?!error).*'` to "drop everything except errors." The `regex` crate rejects negative look-ahead; the rule fails to load.
**Why:** Rust `regex` is linear-time RE2-style — no `(?=)`, `(?!)`, `(?<=)`, `\1`. [VERIFIED]
**How to avoid:** Use `keep_regex` (whitelist) for "keep only X" instead of negated `drop_regex`. Anchor with `^`/`$`, character classes, and alternation only.
**Warning signs:** `lacon validate` reports InvalidRegex; test panics on `RegexSet::new`.

### Pitfall 4: cross-bundled `extends` is untested at fixture level (D-06)
**What goes wrong:** The 4 test-runner rules share a base via `extends: bundled/test-base`. The resolve path (`loader.rs:573-620` + `merge_rules`) is implemented but has **no fixture-level test** — Phase 5 is the first to exercise it. A bug here breaks 4 of 10 rules at once.
**Why:** `extends` *prepends* the parent pipeline (ADR-0012). If the base has a `keep_regex` and the child adds a `drop_regex`, ordering matters (parent stages run first).
**How to avoid:** Treat the bundled→bundled extends path as an **early spike task** (D-06). Author + test ONE extends-based rule (e.g. cargo-test extending test-base) before the other three. Fallback: copy the shared pipeline into each of the four rules (spec-sanctioned "copy the parent").
**Warning signs:** "could not find parent rule" ParseError; child pipeline missing parent stages; stage order wrong.

### Pitfall 5: Default tool output is sometimes already compact (50% floor at risk)
**What goes wrong:** Modern `npm install` on a clean tree emits ~6 lines; default `pytest` collapses dots to one line; a single-crate `cargo build` has one Compiling line. A "primary success" fixture captured from these won't hit the 50% reduction floor.
**Why:** Tools got quieter; the chatty cases are deprecation warnings, multi-dep builds, verbose mode, and many-file scenarios.
**How to avoid:** Capture the *representative chatty* success case for the primary fixture: cargo with multiple deps; pytest with `-v` (per-test lines); pnpm (chatty by default); git status `-uall` with many untracked files; npm with deprecated deps. Mark genuinely-small fixtures `exempt_from_reduction_check: true` (D-05) and pick a different scenario as "primary."
**Warning signs:** reduction assertion fails with ratio just above 0.5.

### Pitfall 6: docker BuildKit progress lines carry byte counts that vary run-to-run
**What goes wrong:** `#5 sha256:... 1.05MB / 3.63MB 2.0s` lines differ every capture (timings, byte progress), making byte-exact expected.txt brittle if not dropped.
**Why:** BuildKit streams live progress.
**How to avoid:** Drop all `^#\d+ sha256:` and `^#\d+ .* DONE \d` / transfer lines in the success pipeline; keep `^#\d+ \[\d+/\d+\]` step headers and any `ERROR`/`CACHED`-context. Capture with a deterministic Dockerfile.
**Warning signs:** expected.txt regeneration produces different bytes each run.

## Code Examples

### Test runner skeleton (D-01, D-04, D-09)
```rust
// Source: synthesized from explain.rs:116-159 + primitives.rs:16-44 (verified call sites)
use lacon_core::rules::loader::RuleLoader;
use lacon_core::runtime::{Runner, RunOptions};

fn replay(rule_id: &str, input: &[u8], exit_code: i32, command: &str) -> Vec<String> {
    let mut loader = RuleLoader::new(None); // None → hermetic bundled-only (D-01)
    let resolved = loader.resolve(rule_id).expect("resolve");
    let mut runner = Runner::new(resolved, RunOptions::default());
    runner
        .filter_bytes(input, exit_code, 0, command, None)
        .expect("filter_bytes")
}

// Per-fixture: byte-exact (D-04)
let actual = replay(id, &input_bytes, meta.exit_code, &meta.command).join("\n");
let expected = std::fs::read_to_string(expected_path)?
    .trim_end_matches('\n')
    .to_owned();
assert_eq!(actual, expected);
```

### meta.yaml deserialize struct (D-02 adds exit_code)
```rust
// Source: mirror parse_one at loader.rs:439 (serde_saphyr 0.0.26)
#[derive(serde::Deserialize)]
struct FixtureMeta {
    command: String,
    exit_code: i32,                       // D-02 NEW FIELD
    #[serde(default)] tool_version: Option<String>,
    #[serde(default)] exempt_from_reduction_check: bool,  // D-05
    #[serde(default)] must_keep_lines: Vec<String>,       // D-05
    #[serde(default)] os: Option<String>,
    #[serde(default)] notes: Option<String>,
}
```

---

## Research Directive 1: Real output formats of the ten tools

Legend: 🟢 = real output captured on this machine (quote verbatim into fixtures, lightly trim); 🟡 = format from authoritative docs, **executor must capture real fixture via npx during execution**.

### 1. `pkg-install` (npm / pnpm / yarn) 🟢

**npm install — chatty success (deprecated deps), exit 0** [VERIFIED: captured `npm 11.12.1`]:
```
npm warn deprecated uuid@3.4.0: uuid@10 and below is no longer supported.  For ESM codebases, update to uuid@latest.  For CommonJS codebases, use uuid@11 (but be aware this version will likely be deprecated in 2028).
npm warn deprecated har-validator@5.1.5: this library is no longer supported
npm warn deprecated request@2.88.2: request has been deprecated, see https://github.com/request/request/issues/3142

added 48 packages, and audited 49 packages in 1s

3 packages are looking for funding
  run `npm fund` for details

5 vulnerabilities (3 moderate, 2 critical)

Some issues need review, and may require choosing
a different dependency.

Run `npm audit` for details.
```
- **DROP:** `^npm warn deprecated ` (deprecation noise), `^\d+ packages are looking for funding`, `^  run .npm fund`, the multi-line vulnerability blurb (`^Some issues need review`, `^a different dependency`, `^Run .npm audit`).
- **KEEP:** `^added \d+ packages` (the result summary), `^\d+ vulnerabilities` (security signal).

**npm install — failure, exit 1** [VERIFIED: captured]:
```
npm error code E404
npm error 404 Not Found - GET https://registry.npmjs.org/this-package-truly-does-not-exist-lacon-xyz - Not found
npm error 404
npm error 404  The requested resource 'this-package-truly-does-not-exist-lacon-xyz@^1.0.0' could not be found or you do not have permission to access it.
...
npm error A complete log of this run can be found in: /home/.../debug-0.log
```
- on_error: KEEP `^npm error` lines. Prefix is `npm error` (npm 7+; was `npm ERR!` in npm 6 and earlier — note for older fixtures).

**pnpm install — chatty success, exit 0** [VERIFIED: captured `pnpm 11.1.2`]:
```
[WARN] deprecated request@2.88.2: request has been deprecated, see https://github.com/request/request/issues/3142
Progress: resolved 1, reused 0, downloaded 0, added 0

   ╭──────────────────────────────────────────────╮
   │      Update available! 11.1.2 → 11.2.2.      │
   │   To update, run: corepack use pnpm@11.2.2   │
   ╰──────────────────────────────────────────────╯

Progress: resolved 48, reused 0, downloaded 48, added 48, done
[WARN] 2 deprecated subdependencies found: har-validator@5.1.5, uuid@3.4.0
Packages: +48
++++++++++++++++++++++++++++++++++++++++++++++++
dependencies:
+ lodash 4.18.1
+ request 2.88.2 deprecated

Done in 3s using pnpm v11.1.2
```
- **DROP:** `^Progress: resolved`, the update-available box (`^\s*[╭│╰]`), `^\[WARN\]`, `^Packages: \+`, `^\++$` (the plus-bar), the `^\+ ` package list.
- **KEEP:** `^Done in ` (summary), and on failure the error block.

**yarn 1 install — success, exit 0** [VERIFIED: captured `yarn 1.22.22` via Corepack]:
```
yarn install v1.22.22
warning package.json: No license field
info No lockfile found.
warning request@2.88.2: request has been deprecated, ...
[1/4] Resolving packages...
[2/4] Fetching packages...
[3/4] Linking dependencies...
[4/4] Building fresh packages...
success Saved lockfile.
Done in 1.34s.
```
- **DROP:** `^\[[0-9]/4\]` step lines, `^info `, `^warning ` (license/deprecation noise), `^success ` (yarn 1) — keep `^Done in`.
- **NOTE:** yarn 1.x and yarn 2+/Berry differ wildly in output. The fixture must record `tool_version`. yarn 2+ uses `➤ YN0000:` prefixed lines instead. Capture both if the executor has access; v1 fixture from yarn 1 is acceptable since Corepack defaults to 1.22.22 here.

### 2. `cargo-build` (cargo build / cargo check) 🟢

**Success with warning, multi-dep, exit 0** [VERIFIED: captured `cargo 1.95.0`]:
```
    Updating crates.io index
     Locking 9 packages to latest compatible versions
   Compiling serde_core v1.0.228
   Compiling anyhow v1.0.102
   Compiling serde v1.0.228
   Compiling itoa v1.0.18
   Compiling demo v0.1.0 (/tmp/lacon-cargo-XXXX/demo)
warning: unused variable: `unused`
 --> src/main.rs:2:9
  |
2 |     let unused = 42;
  |         ^^^^^^ help: if this is intentional, prefix it with an underscore: `_unused`
  |
  = note: `#[warn(unused_variables)]` (part of `#[warn(unused)]`) on by default

warning: `demo` (bin "demo") generated 1 warning (run `cargo fix --bin "demo" -p demo` to apply 1 suggestion)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.81s
```
- **DROP:** `^\s*Compiling ` (the repeats — the roadmap's headline target), `^\s*Updating `, `^\s*Locking `, `^\s*Finished `.
- **KEEP:** `^warning:` and the full diagnostic block under it (the `-->`, `|`, `= note:` lines give file:line context). To keep the block, prefer `keep_around_match` on `^warning:`/`^error` OR simply drop only the Compiling/Updating/Locking/Finished lines and let everything else through (blacklist approach — recommended here since warning blocks have variable shape).
- **Reduction note:** with N deps, dropping N `Compiling` + 3 status lines is the bulk of the reduction. Capture ≥4 deps for the primary success fixture (Pitfall 5).

**Failure, exit 101** [VERIFIED: captured]:
```
   Compiling demo v0.1.0 (/tmp/lacon-cargo-XXXX/demo)
error[E0308]: mismatched types
 --> src/main.rs:5:18
  |
5 |     let x: i32 = "not a number";
  |            ---   ^^^^^^^^^^^^^^ expected `i32`, found `&str`
  |            |
  |            expected due to this

For more information about this error, try `rustc --explain E0308`.
error: could not compile `demo` (bin "demo") due to 1 previous error
```
- Exit code: cargo build failure is **101** (verified: `cargo test` failure prints `error: test failed`; build prints `error: could not compile`). Record actual exit_code in meta.yaml.
- on_error: KEEP `^error\[E\d+\]:`, `^error:`, and the `-->` / `|` context (use `keep_around_match` on `^error`). DROP the lone `Compiling` line.
- Error prefix forms: `error[E0308]:` (coded), `error:` (uncoded), `warning:`.

### 3. `cargo-test` (cargo test) 🟢

**Success, exit 0** [VERIFIED: captured]:
```
   Compiling demo v0.1.0 (...)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.10s
     Running unittests src/main.rs (target/debug/deps/demo-5879940de8c512d7)

running 4 tests
test tests::test_four ... ok
test tests::test_one ... ok
test tests::test_three ... ok
test tests::test_two ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```
- **DROP:** `^test .+ \.\.\. ok$` (per-test PASS lines — the headline target), `^\s*Compiling `, `^\s*Finished `, `^\s*Running `, `^running \d+ tests$`.
- **KEEP:** `^test result:` (the summary line).

**Failure, exit 101** [VERIFIED: captured]:
```
running 4 tests
test tests::test_four ... FAILED
test tests::test_one ... ok
test tests::test_three ... ok
test tests::test_two ... FAILED

failures:

---- tests::test_four stdout ----

thread 'tests::test_four' (22402) panicked at src/main.rs:11:30:
assertion `left == right` failed
  left: 0
 right: 1
note: run with `RUST_BACKTRACE=1` ...

---- tests::test_two stdout ----
...
failures:
    tests::test_four
    tests::test_two

test result: FAILED. 2 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

error: test failed, to rerun pass `--bin demo`
```
- on_error: KEEP `\.\.\. FAILED$`, the `---- ... stdout ----` blocks, `panicked at`, `assertion`, `left:`/`right:`, the `failures:` list, `^test result: FAILED`, `^error: test failed`. DROP `\.\.\. ok$` and Compiling/Finished/Running. `keep_regex` whitelist of `(FAILED|panicked|assertion|left:|right:|test result:|^error:)` is a clean fit here.

### 4–6. Test-runner rules sharing a base (D-06): cargo-test + vitest + jest + pytest

These four share the shape "drop per-test PASS lines, keep the summary + failures." cargo-test (above) and pytest (below) are 🟢 captured; vitest and jest are 🟡 docs-only.

#### pytest 🟢

**Verbose success (`-v`), exit 0** [VERIFIED: captured `pytest 9.0.2`]:
```
============================= test session starts ==============================
platform linux -- Python 3.14.4, pytest-9.0.2, pluggy-1.6.0 -- /usr/bin/python3
cachedir: .pytest_cache
rootdir: /tmp/lacon-pytest-XXXX
plugins: typeguard-4.4.4
collecting ... collected 8 items

test_demo.py::test_a PASSED                                              [ 12%]
test_demo.py::test_b PASSED                                              [ 25%]
...
test_demo.py::test_h PASSED                                              [100%]

============================== 8 passed in 0.01s ===============================
```
- **DROP:** `PASSED` per-test lines (`PASSED\s+\[`), the header block (`^platform `, `^cachedir:`, `^rootdir:`, `^plugins:`, `^collecting`, `^collected`), the `=== test session starts ===` banner.
- **KEEP:** `^=+ \d+ passed.* =+$` (the final summary banner).
- **NOTE:** *default* (non-verbose) pytest collapses to one dot-line `test_demo.py ........  [100%]` — already tiny. For the primary success fixture, capture `-v` output (Pitfall 5) so the per-test-line drop yields ≥50%.

**Failure, exit 1** [VERIFIED: captured]:
```
test_fail.py .FF                                                         [100%]

=================================== FAILURES ===================================
___________________________________ test_bad ___________________________________

>   def test_bad(): assert compute(3) == 7
E   assert 6 == 7
E    +  where 6 = compute(3)

test_fail.py:5: AssertionError
___________________________________ test_err ___________________________________
    def test_err():
        d = {}
>       return d["missing"]
E       KeyError: 'missing'

test_fail.py:8: KeyError
=========================== short test summary info ============================
FAILED test_fail.py::test_bad - assert 6 == 7
FAILED test_fail.py::test_err - KeyError: 'missing'
========================= 2 failed, 1 passed in 0.01s ==========================
```
- on_error: KEEP the `=== FAILURES ===` block, `^E ` lines (assertion/traceback details — the signal), `^>` lines, `:\d+: \w+Error$` (file:line: ExceptionType), `^FAILED `, the final `=== N failed ===` banner. DROP the `=== test session starts ===` header. `keep_around_match` on `^FAILED|^E |^>` + `keep_tail` works; or whitelist `keep_regex`.

#### vitest 🟡 (NOT installed — executor captures via `npx vitest run`)

[CITED: vitest.dev docs + community examples] Default reporter output shapes:
```
 ✓ src/sum.test.ts (3 tests) 4ms
 ❯ src/bad.test.ts (2 tests | 1 failed) 6ms
   × should add  6ms
     → expected 5 to be 6

 Test Files  1 failed | 1 passed (2)
      Tests  1 failed | 5 passed (6)
   Duration  412ms
```
- **DROP (success):** `^ ✓ ` per-file PASS lines (with leading green check), `Duration`, `Start at`, `Transform`/`Setup`/`Collect`/`Environment`/`Prepare` timing lines.
- **KEEP:** `^ Test Files `, `^      Tests ` summary lines.
- on_error: KEEP `^ ❯ `, `^   × `, `^     → ` (the failure detail), the `FAIL`-marked file lines, `Test Files` summary.
- ⚠️ **Executor MUST capture real output** — vitest output varies by version/reporter/TTY. `npx -p vitest@latest vitest run` in a throwaway project. Strip ANSI first (vitest is heavily colorized; `strip_ansi` is mandatory before any regex). Note the `✓`/`×`/`❯`/`→` are multibyte UTF-8 glyphs — patterns should match on the surrounding ASCII (`(tests)`, `failed`, `passed`) rather than the glyphs where possible.

#### jest 🟡 (NOT installed — executor captures via `npx jest`)

[CITED: jestjs.io docs + community examples] Default output (note: jest writes most to **stderr**, merged here):
```
PASS src/sum.test.js
FAIL src/bad.test.js
  ✕ adds 1 + 2 (3 ms)

  ● adds 1 + 2

    expect(received).toBe(expected)
    Expected: 3
    Received: 4

      at Object.<anonymous> (src/bad.test.js:4:19)

Test Suites: 1 failed, 1 passed, 2 total
Tests:       1 failed, 1 passed, 2 total
Snapshots:   0 total
Time:        1.234 s
```
- **DROP (success):** `^PASS ` per-suite lines, `^Snapshots:`, `^Time:`, `^Ran all test suites`.
- **KEEP:** `^Test Suites:`, `^Tests:` summary.
- on_error: KEEP `^FAIL `, `^\s*● ` (failure header), `^\s*✕ `, `Expected:`/`Received:`, `at .* \(.*:\d+:\d+\)` (the file:line), `^Test Suites:` summary.
- ⚠️ **Executor MUST capture real output.** jest emits to stderr — the lacon merge captures it (filter_bytes takes merged bytes). `npx -p jest jest` in a throwaway project with a babel/ts setup, or `npx jest --no-watchman`. Note `--watch` mode is interactive (TUI-bypassed upstream); fixtures are for non-watch CI runs.

**D-06 shared base recommendation:** the four rules' success pipelines all reduce to "strip_ansi → drop per-test-pass line → keep summary." But the *per-test-pass regex differs per tool* (`\.\.\. ok$` vs `PASSED` vs `^ ✓ ` vs `^PASS `). So a shared base can only safely contain `strip_ansi` + a generic `keep_tail` cap; the tool-specific drop/keep must live in each child. Given how little is genuinely shared, **the copy-the-parent fallback (D-06) is likely cleaner than `extends`** — but still author ONE extends-based rule as the spike to verify the bundled→bundled path (Pitfall 4) before deciding. If the spike is clean and the shared base is just `strip_ansi`, extends is fine.

### 7. `tsc` (tsc / tsc --noEmit) 🟡 (NOT installed — executor captures via `npx tsc`)

[CITED: typescriptlang.org docs] Error format is stable and well-documented:
```
src/index.ts(12,5): error TS2304: Cannot find name 'foo'.
src/index.ts(20,10): error TS2322: Type 'string' is not assignable to type 'number'.

Found 2 errors in the same file, starting at: src/index.ts:12
```
- tsc on **success emits nothing** (exit 0, zero output). So the success fixture is near-empty → mark `exempt_from_reduction_check: true` OR use a "compiles with no errors but pretty/incremental output" scenario. Realistically the *interesting* tsc fixture is the failure path.
- on_error / failure (exit 1/2): the output IS the signal (roadmap note). Pipeline = `strip_ansi` + `dedupe` + `keep_tail`. KEEP everything matching `\(\d+,\d+\): error TS\d+:` (file(line,col): error TSxxxx:) and the `Found N errors` summary.
- ⚠️ **Executor MUST capture real output.** `npx -p typescript tsc --noEmit` against a file with deliberate type errors. With `--pretty` (default in TTY) tsc adds ANSI + multi-line carets → `strip_ansi` mandatory; with `--pretty false` it's the one-line form above. Capture the `--pretty false` form for a stable fixture, OR capture pretty + strip_ansi. Record which in meta.yaml notes.

### 8. `eslint` (eslint) 🟡 (NOT installed — executor captures via `npx eslint`)

[CITED: eslint.org docs, default "stylish" formatter]:
```
/path/to/file.js
  1:7   error    'x' is assigned a value but never used  no-unused-vars
  2:1   warning  Unexpected console statement             no-console

✖ 2 problems (1 error, 1 warning)
  1 error and 0 warnings potentially fixable with the `--fix` option.
```
- eslint on a **clean run emits nothing** (exit 0) → near-empty success fixture; mark exempt or capture a "with warnings, exit 0" scenario (warnings alone don't fail eslint unless `--max-warnings`).
- **DROP (success-with-warnings):** the `✖ N problems` summary line and the `potentially fixable` line are arguably keepable; the noise is bulk warning lines. Roadmap says "drop passing summaries" — but eslint has no per-file PASS lines, so reduction comes from `keep_around_match` on `error` (drop pure-warning files) OR `keep_regex` on ` error `. Be careful: keeping only `error` drops warnings, which may be acceptable signal-loss for success path but NOT for on_error.
- on_error (exit 1, has errors): KEEP file-path header lines (`^/`), ` error `/` warning ` detail lines, the `✖ N problems` summary.
- ⚠️ **Executor MUST capture real output.** `npx -p eslint eslint .` against a file with a `no-unused-vars` violation. The file-path-then-indented-detail two-level format means `keep_around_match` on the detail line (after:0, before:1) preserves the path header. `strip_ansi` mandatory (stylish formatter colorizes).

### 9. `git-status` (git status) 🟢

**Default (collapses dirs), exit 0** [VERIFIED: captured `git 2.53.0`]:
```
On branch main
Changes not staged for commit:
  (use "git add <file>..." to update what will be committed)
  (use "git restore <file>..." to discard changes in working directory)
	modified:   .planning/config.json

Untracked files:
  (use "git add <file>..." to include in what will be committed)
	.claude/worktrees/

no changes added to commit (use "git add" and/or "git commit -a")
```
**`-uall` (the chatty monorepo scenario — 176 lines), exit 0** [VERIFIED: captured]:
```
On branch main

No commits yet

Untracked files:
  (use "git add <file>..." to include in what will be committed)
	.gitignore
	Cargo.lock
	src/gen/mod1.rs
	src/gen/mod10.rs
	... (170+ more lines)
	target/debug/.fingerprint/anyhow-.../lib-anyhow.json
nothing added to commit but untracked files present (use "git add" to track)
```
- **Reduction strategy:** the long `^\t` (tab-indented) file lines under "Untracked files:" are the bulk. Use `collapse_repeated` on `^\t` with `max_kept: 5` + `summary: '\t… {count} more untracked files'`. KEEP `^On branch`, `^Changes`, `^modified:`/`^\tmodified:`, the hint lines optionally dropped.
- **DROP candidates (success):** the `(use "git ..." ...)` hint lines (`^\s*\(use `).
- on_error: `git status` rarely fails (exit nonzero only outside a repo: `fatal: not a git repository`). Failure fixture: KEEP `^fatal:`. Likely `exempt_from_reduction_check: true` (small).
- **NOTE:** default `git status` (no `-uall`) already collapses directories — capture the `-uall` form OR a real many-modified-files repo for the primary success fixture (D-10 matches concrete `git status`; the user may run `-uall` themselves). The rule's `match` is `git status` (any args); the collapse_repeated on `^\t` handles both forms.

### 10. `docker-build` (docker build / docker buildx build) 🟢

**First build, success, exit 0** [VERIFIED: captured `Docker 29.5.1`, BuildKit]:
```
#0 building with "default" instance using docker driver
#1 [internal] load build definition from Dockerfile
#1 transferring dockerfile: 266B done
#1 DONE 0.0s
#2 [internal] load metadata for docker.io/library/alpine:3.20
#2 DONE 4.3s
#5 [1/6] FROM docker.io/library/alpine:3.20@sha256:d9e8...
#5 sha256:25f1d6...471 1.05MB / 3.63MB 2.0s
#5 extracting sha256:25f1...471 0.1s done
#5 DONE 2.7s
#6 [2/6] RUN echo "step one" && echo "more output line 1" && echo "more output line 2"
#6 0.129 step one
#6 DONE 0.2s
#11 exporting to image
#11 exporting manifest sha256:e420...9 done
```
**Rebuild, CACHED layers, exit 0** [VERIFIED: captured]:
```
#6 [4/6] RUN echo "step two done"
#6 CACHED
#7 [5/6] COPY Dockerfile /tmp/Dockerfile
#7 CACHED
...
#11 exporting to image
#11 exporting layers done
```
- **DROP (success):** `^#\d+ CACHED$` (layer cache hits — the headline target), `^#\d+ DONE \d` (timing), `^#\d+ sha256:` (byte-progress, run-to-run nondeterministic — Pitfall 6), `^#\d+ transferring `, `^#\d+ extracting `, `^#\d+ resolve `, `^#0 building with`, `^#\d+ exporting `.
- **KEEP:** `^#\d+ \[\d+/\d+\] ` (the build step headers — what's actually running), `^#\d+ \[internal\]` optionally dropped, and RUN command echo output (`^#\d+ \d+\.\d+ `).

**Failure, exit 1** [VERIFIED: captured]:
```
#5 [2/4] RUN echo "this works"
#5 0.105 this works
#5 DONE 0.1s
#6 [3/4] RUN exit 17
#6 ERROR: process "/bin/sh -c exit 17" did not complete successfully: exit code: 17
------
 > [3/4] RUN exit 17:
------
Dockerfile.fail:3
--------------------
   1 |     FROM alpine:3.20
   3 | >>> RUN exit 17
--------------------
ERROR: failed to build: failed to solve: process "/bin/sh -c exit 17" did not complete successfully: exit code: 17
```
- on_error: KEEP `^#\d+ ERROR:`, the `------` framed error context, `^ > \[`, the `--------------------` Dockerfile excerpt with `>>>`, and `^ERROR: failed to build`. DROP CACHED/DONE/sha256 lines. `keep_around_match` on `ERROR` (before:2, after:15) captures the framed block.
- **NOTE:** Legacy (non-BuildKit) docker output (`Step 1/6 : FROM ...`, `---> Using cache`, `Successfully built`) is a *different* format. Modern Docker defaults to BuildKit. Capture BuildKit (default) for the v1 fixture; record `tool_version` and `DOCKER_BUILDKIT` state in meta.yaml notes. `docker buildx build` produces the same `#N` format.

---

## Research Directive 2: pkg-install rewrite (D-11 verdict)

**VERDICT: Do NOT add a `rewrite` block to `pkg-install`. Use pipeline-side filtering only (D-11 fallback wins).**

### Evidence (all empirically verified on this machine)

| Manager | Silent flag | Accepted? | Success behavior | **Failure behavior** |
|---------|-------------|-----------|------------------|----------------------|
| npm 11.12.1 | `--silent` / `-s` | ✓ (maps to loglevel silent) | 0 bytes output | **0 bytes — E404 error DELETED, exit 1** |
| pnpm 11.1.2 | `--silent` / `-s` / `--reporter=silent` | ✓ (documented alias) | 0 bytes output | **0 bytes — error DELETED, exit 1** |
| yarn 1.22.22 | `--silent` / `-s` | ✓ | keeps `warning` lines, drops `[N/4]`/`success` | keeps warnings (less aggressive) |

**Cross-manager flag collision (the killer):**
- `npm install --reporter=silent` → `npm warn Unknown cli config "--reporter". This will stop working in the next major version of npm.` [VERIFIED] — pnpm's flag breaks npm (warns now, hard-fails in next major).
- `yarn install --reporter=silent` → yarn 1 silently ignores it (no effect).
- `pnpm install --silent` → works (documented alias).

So `--silent` is the *only* flag accepted by all three — **but on npm and pnpm it suppresses error output entirely on failure** (0 bytes, verified exit 1 with empty output). lacon's `on_error` pipeline would have nothing to filter; the error is destroyed *before lacon sees it*. This directly violates the core "never drop errors" contract (REQ-bundled-rules-tier1, REQ-acceptance-bundled-reduction).

### Why pipeline-side filtering is strictly better here
- Default install output already contains the errors (npm: 536 bytes on failure, exit 1, full E404 preserved [VERIFIED]).
- lacon's `on_error` branch can preserve those errors; a `--silent` rewrite cannot un-delete them.
- The success-path noise (`npm warn deprecated`, pnpm `Progress:`/box, yarn `[N/4]`) is fully removable with `drop_regex` — no rewrite needed.

### Recommended `pkg-install` design (no rewrite)
```yaml
id: pkg-install
description: npm / pnpm / yarn install
match:
  any:
    - { command: npm,  args_prefix: [install] }
    - { command: pnpm, args_prefix: [install] }
    - { command: pnpm, args_prefix: [i] }
    - { command: pnpm, args_prefix: [add] }
    - { command: yarn, args_prefix: [install] }
pipeline:
  - strip_ansi
  - drop_regex: '^npm warn deprecated '
  - drop_regex: '^\[WARN\] '                       # pnpm
  - drop_regex: '^Progress: '                       # pnpm
  - drop_regex: '^\s*[╭│╰]'                          # pnpm update box
  - drop_regex: '^Packages: \+'                      # pnpm
  - drop_regex: '^\++$'                              # pnpm progress bar
  - drop_regex: '^\[[0-9]/4\] '                      # yarn 1 steps
  - drop_regex: '^info '                             # yarn 1
  - drop_regex: '^\d+ packages are looking for funding'
  - drop_regex: '^  run `npm fund`'
# NO rewrite block (D-11)
on_error:
  pipeline:
    - strip_ansi
    - keep_around_match: { pattern: '(?i)(error|ERR_|fatal)', before: 1, after: 10 }
    - keep_tail: { lines: 40 }
```
(`match` per D-10 uses `command` + `args_prefix`; `pnpm i`/`pnpm add` are explicit per CONTEXT. `yarn` 2+ output differs — note in fixture meta.)

## Runtime State Inventory

> N/A — greenfield rule authoring. No rename/refactor/migration. No stored data, no live service config, no OS-registered state to update. The only "state" is checked-in fixture text files (static, no migration).

## Common Pitfalls

(See the Common Pitfalls section above — six pitfalls documented: keep_regex adjacency, missing exit_code, no-look-around regex, untested cross-bundled extends, already-compact default output, docker progress nondeterminism.)

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| `npm ERR!` error prefix | `npm error` | npm 7 (2020) | Match `^npm error`, not `^npm ERR!` (note for old fixtures) |
| Legacy docker `Step N/M`, `---> Using cache`, `Successfully built` | BuildKit `#N [k/m]`, `#N CACHED`, `#N DONE` | BuildKit default ~Docker 23 (2023) | Match the `#N` format; capture BuildKit fixtures |
| npm verbose per-package progress | quiet clean-install summary | npm ~8+ | Default clean install is already compact — capture chatty (deprecated-deps) case |
| yarn 1 `[N/4]`/`success`/`Done` | yarn 2+/Berry `➤ YN0000:` | yarn 2 (2020) | Record `tool_version`; Corepack here defaults to yarn 1.22.22 |
| pytest default dot-progress | unchanged; `-v` gives per-test lines | — | Capture `-v` for the reductive primary fixture |

**Deprecated/outdated:** none in the engine — Phase 1-4 primitives are current and shipped.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | vitest default-reporter line shapes (`✓`/`❯`/`×`/`→`, `Test Files`/`Tests` summary) | Tool 4-6 vitest | Wrong regex → vitest rule fails on real output. **Mitigated:** executor MUST capture real fixture via `npx vitest run`. |
| A2 | jest default output shapes (`PASS`/`FAIL`/`●`/`Test Suites:`, stderr stream) | Tool 4-6 jest | Same — executor MUST capture via `npx jest`. |
| A3 | tsc `file(line,col): error TSxxxx:` format + empty-on-success | Tool 7 tsc | Stable per docs, but `--pretty` adds ANSI; executor captures `npx tsc --noEmit`. |
| A4 | eslint stylish formatter (path header + indented `line:col error/warning rule` + `✖ N problems`) | Tool 8 eslint | Formatter-dependent; executor captures `npx eslint .` with stylish (default). |
| A5 | yarn 2+/Berry uses `➤ YN0000:` (only yarn 1 captured) | Tool 1 / SOTA | v1 fixture from yarn 1 is acceptable (Corepack default); yarn 2 is a separate scenario if needed. |
| A6 | cargo build/test failure exit code is 101 | Tools 2-3 | Record actual observed exit_code in meta.yaml; runner uses meta value, not an assumed constant. |

**These six are the only `[ASSUMED]`/`[CITED]` items.** Everything tagged 🟢 (cargo, git, pytest, docker, npm/pnpm/yarn) is real captured output and is `[VERIFIED]`.

## Open Questions

1. **Shared test-base via `extends` vs copy-the-parent (D-06)**
   - What we know: bundled→bundled extends is implemented (`loader.rs:573-620`) but untested at fixture level; prepends parent pipeline.
   - What's unclear: how much is genuinely shareable — analysis shows only `strip_ansi` + a `keep_tail` cap are common; the per-test-pass drop regex differs per tool.
   - Recommendation: spike ONE extends-based rule first (Pitfall 4). If the shared base is just `strip_ansi`, the value of `extends` is marginal — copy-the-parent (D-06 fallback) is likely cleaner and lower-risk. Planner's call; either satisfies the requirements.

2. **Primary-success scenario selection per rule (50% floor)**
   - What we know: several tools' default success output is already compact (Pitfall 5).
   - Recommendation: for each rule, pick the *representative chatty* success case as "primary" (multi-dep cargo, `-v` pytest, deprecated-deps npm, chatty pnpm, `-uall` git, multi-step+cache docker). Mark genuinely-small fixtures `exempt_from_reduction_check: true`.

3. **tsc/eslint near-empty success path**
   - What we know: both emit nothing on a clean run (exit 0).
   - Recommendation: make the *failure* path the primary fixture for these two; success fixture can be tiny + `exempt_from_reduction_check: true`, OR a "warnings present, exit 0" eslint scenario.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| cargo | cargo-build, cargo-test fixtures + the test runner itself | ✓ | 1.95.0 | — |
| git | git-status fixture | ✓ | 2.53.0 | — |
| pytest | pytest fixture | ✓ | 9.0.2 | — |
| docker | docker-build fixture | ✓ | 29.5.1 (BuildKit) | — |
| npm | pkg-install fixture | ✓ | 11.12.1 | — |
| pnpm | pkg-install fixture | ✓ | 11.1.2 | — |
| yarn | pkg-install fixture | ✓ | 1.22.22 (Corepack) | — |
| npx / node | vitest/jest/tsc/eslint capture | ✓ | node 24.15.0, npx 11.12.1 | Capture via `npx -p <pkg>` in throwaway project |
| tsc | tsc fixture | ✗ | — | `npx -p typescript tsc` |
| eslint | eslint fixture | ✗ | — | `npx -p eslint eslint` |
| vitest | vitest fixture | ✗ | — | `npx -p vitest vitest run` |
| jest | jest fixture | ✗ | — | `npx -p jest jest` |

**Missing dependencies with no fallback:** none — all four missing JS tools are reachable via `npx` (node + npx present).
**Missing dependencies with fallback:** tsc/eslint/vitest/jest → capture via `npx` during execution. **CI never installs any of these** (hermetic, REQ-acceptance-test-coverage); fixtures are static text. The `npx` capture happens at *fixture authoring time on the dev machine*, not in CI.

## Validation Architecture

> `workflow.nyquist_validation` was not found explicitly false; treating as enabled.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + cargo test harness (no external test crate; `insta` declared but unused — do NOT introduce, D-09) |
| Config file | none — cargo auto-discovers `crates/lacon-core/tests/*.rs` |
| Quick run command | `cargo test --test bundled_rules` |
| Full suite command | `cargo test` (workspace) |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| REQ-bundled-rules-tier1 | 10 rules each ≥50% reduction on primary success, zero error drops | integration (fixture-walk) | `cargo test --test bundled_rules` | ❌ Wave 0 (new file) |
| REQ-bundled-rules-format | each rule has YAML + fixtures + test + roadmap note | integration + manual doc check | `cargo test --test bundled_rules` | ❌ Wave 0 |

### Sampling Rate
- **Per task commit:** `cargo test --test bundled_rules` (fast — subprocess-free byte replay, no tool spawns)
- **Per wave merge:** `cargo test` (full workspace, ensures no regression in Phase 1-4 suites)
- **Phase gate:** full suite green before `/gsd:verify-work`

### Wave 0 Gaps
- [ ] `crates/lacon-core/tests/bundled_rules.rs` — the fixture-walking runner (D-01/D-04/D-05/D-09); does not exist yet
- [ ] `tests/fixtures/<rule-id>/<scenario>/` trees — 10 rules × ≥2 scenarios; none exist yet (`tests/fixtures/` has only `primitives/`)
- [ ] `bundled-rules/*.yaml` — 10 rule files (+ optional `test-base.yaml`); dir has only `.gitkeep`
- [ ] `docs/testing-rules.md` — add `exit_code` to meta.yaml schema (D-02)
- [ ] Framework install: none — cargo harness already present

## Security Domain

> `security_enforcement` not set false in config; included per policy. This phase has a **minimal** security surface — it authors declarative YAML and reads static fixture files, no network, no auth, no untrusted input at runtime.

### Applicable ASVS Categories
| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | yes (low) | Rule YAML validated at load (`lacon validate`); regex compiled via `regex` crate (no ReDoS — linear-time RE2) |
| V6 Cryptography | no | — |
| V12 Files & Resources | yes (low) | Fixtures read from workspace-root path; script paths already path-traversal-guarded (`resolve_script` T-04-03) — but this phase adds no `script:` |

### Known Threat Patterns for this stack
| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Catastrophic-backtracking regex (ReDoS) in a rule | DoS | Rust `regex` crate is linear-time by construction — no backtracking; ReDoS impossible [VERIFIED: regex crate guarantee] |
| `--silent` rewrite suppressing security signal (vuln/error) | Info disclosure / signal loss | D-11 verdict: no rewrite; `on_error` preserves errors |
| Fixture path traversal | Tampering | Path is `env!(CARGO_MANIFEST_DIR)/../../tests/fixtures/<id>/<scenario>` — fixed prefix, no user input |

## Project Constraints (from CLAUDE.md + ADRs)

These have the authority of locked decisions; rules must not contradict them:
- **Streaming-first (ADR-0005):** all chosen primitives are line-by-line transformers. No primitive needing global reorder (none of the ten rules need sort). ✓
- **First-match-wins, project > user > bundled (ADR-0004/0007):** bundled rules are lowest priority; a user/project rule overrides. No merging. ✓
- **`on_error` replaces, never merges (ADR-0010):** every failure pipeline is self-contained. ✓
- **`extends` prepends parent pipeline, inherits omitted scalars (ADR-0012):** affects D-06 base design — parent stages run first. ✓
- **Starlark only via top-level `post_process` (ADR-0008):** `script:` in `pipeline:` is rejected at load. None of the ten rules need Starlark. ✓
- **Cold start under 10ms:** rules are pure-native (no Starlark), keeping resolution cheap. ✓
- **No new engine primitive / CLI command (CONTEXT in-scope boundary):** Phase 5 consumes the existing surface only. ✓

## Sources

### Primary (HIGH confidence — real captured output / verified source)
- `cargo 1.95.0`, `git 2.53.0`, `pytest 9.0.2`, `docker 29.5.1`, `npm 11.12.1`, `pnpm 11.1.2`, `yarn 1.22.22` — output captured live on this machine 2026-05-22
- `crates/lacon-core/src/rules/loader.rs` — resolve/merge_rules/compile_pipeline/spec_to_stage (verified)
- `crates/lacon-core/src/runtime/mod.rs:423-470` — filter_bytes signature + ADR-0010 branch (verified)
- `crates/lacon-core/src/pipeline/mod.rs:146-238` — KeepRegex adjacency OR-merge (verified)
- `crates/lacon-core/src/rules/schema.rs` — StageSpec snake_case YAML keys, arg fields (verified)
- `crates/lacon-cli/src/commands/explain.rs:116-159` — the resolve→Runner→filter_bytes call site (verified)
- `crates/lacon-core/tests/primitives.rs:16-44` — fixture path + byte-compare idiom (verified)
- `docs/specs/filter-rule-schema.md`, `docs/testing-rules.md`, `docs/specs/config-schema.md`, `docs/bundled-rules-roadmap.md` (read)

### Secondary (MEDIUM — official docs, not locally captured)
- typescriptlang.org — tsc error format `file(line,col): error TSxxxx:`
- eslint.org — stylish formatter output
- vitest.dev / jestjs.io — default reporter shapes

### Tertiary (LOW — none)
- All web/doc claims for the four uninstalled tools are flagged in the Assumptions Log with a mandatory "executor captures real fixture" instruction.

## Metadata

**Confidence breakdown:**
- Tool output formats (cargo/git/pytest/docker/npm/pnpm/yarn): HIGH — real captures quoted verbatim
- Tool output formats (tsc/eslint/vitest/jest): MEDIUM — docs-based, flagged for execution-time capture via npx
- pkg-install rewrite verdict: HIGH — empirically proven `--silent` deletes errors on failure
- Engine semantics (primitives, extends, filter_bytes, keep_regex adjacency): HIGH — verified against source
- Test-runner design: HIGH — mirrors verified explain.rs + primitives.rs idioms

**Research date:** 2026-05-22
**Valid until:** 2026-06-21 (30 days for engine semantics; tool output formats drift faster — re-capture if a fixture goes red, per testing-rules.md regeneration recipe)
