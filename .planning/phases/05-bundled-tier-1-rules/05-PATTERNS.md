# Phase 5: Bundled Tier 1 rules - Pattern Map

**Mapped:** 2026-05-22
**Files analyzed:** 13 file kinds (10 rule YAMLs ×1 + optional test-base.yaml + fixture trees + 1 integration test + 1 doc edit)
**Analogs found:** 13 / 13 (every new file has a strong in-repo analog — no "no analog" cases)

> All file:line references below were re-verified against the current source on 2026-05-22, not just trusted from CONTEXT.md/RESEARCH.md. Two corrections vs. upstream docs are flagged inline (`collapse_repeated` field name; `keep_around_match` having no defaults).

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `bundled-rules/<id>.yaml` ×10 | config (declarative rule) | transform | `crates/lacon-core/tests/fixtures/rules/parent.yaml` (rule w/ pipeline + on_error) | role+flow exact |
| `bundled-rules/test-base.yaml` (optional, D-06) | config (shared parent rule) | transform | `crates/lacon-core/tests/fixtures/rules/parent.yaml` + `child.yaml` (extends pair) | role+flow exact |
| `tests/fixtures/<id>/<scenario>/input.txt` | test (fixture data) | file-I/O | `tests/fixtures/primitives/<name>/input.txt` | role+flow exact |
| `tests/fixtures/<id>/<scenario>/expected.txt` | test (fixture data) | file-I/O | `tests/fixtures/primitives/<name>/expected.txt` | role+flow exact |
| `tests/fixtures/<id>/<scenario>/meta.yaml` | test (fixture metadata) | file-I/O | NEW shape — closest is `parse_one` deserialize idiom (`loader.rs:439`); no existing meta.yaml in repo | flow-match (parse idiom) |
| `crates/lacon-core/tests/bundled_rules.rs` | test (integration, fixture-walk) | transform / request-response | `crates/lacon-core/tests/primitives.rs` (fixture-walk + byte-compare) + `runtime_filter_bytes.rs` (filter_bytes call) + `explain.rs:116-159` (resolve→new→filter_bytes) | composite exact |
| `docs/testing-rules.md` (MODIFIED) | docs | n/a | itself (lines 37-47 `meta.yaml` block) | self |

**No "No Analog Found" section** — every file maps to a concrete in-repo pattern. The only genuinely new artifact shape is `meta.yaml`, and even that reuses the verified `serde_saphyr::from_str` deserialize idiom.

---

## Pattern Assignments

### `bundled-rules/<id>.yaml` ×10 (config, transform)

**Analog:** `crates/lacon-core/tests/fixtures/rules/parent.yaml` (the only in-repo rule that has BOTH a success `pipeline` and an `on_error` block — the exact shape all ten Tier 1 rules need). Secondary: `valid_simple.yaml` (minimal shape), `child.yaml` (extends shape).

**Full analog — `parent.yaml` (top-level field layout + on_error nesting), VERIFIED current:**
```yaml
id: parent
match:
  command: pnpm
  args_prefix: [install]
pipeline:
  - strip_ansi
  - drop_regex: '^Lockfile'
on_error:
  pipeline:
    - keep_regex: '(error|FAIL)'
    - keep_tail:
        lines: 50
    - max_bytes: 8192
```

**Field layout to copy** (top-level keys, verified against `RuleFile` at `crates/lacon-core/src/rules/schema.rs:27-64`):
- `id` (String, required, kebab-case) — must equal the resolution id passed to `resolve(...)`.
- `description` (optional) — shown in `doctor`/`stats`.
- `match` (serde-renamed from `match_spec`; `MatchSpec` at `schema.rs:69-91`): `command`, `args_prefix: [..]`, `args_contain: [..]`, `command_regex`, and `any: [..]` / `all: [..]` for OR/AND nesting. **D-10:** use `command` + `args_prefix`; `pkg-install` uses an `any:` list of per-manager sub-matches.
- `pipeline` (`Vec<StageSpec>`) — the success path.
- `on_error.pipeline` (`OnErrorSpec` → its own `Vec<StageSpec>`) — the ADR-0010 replacement path.
- `extends` (optional) — only on the 4 test-runner rules if D-06 chooses extends.
- `rewrite` — **omit** on `pkg-install` per D-11 research verdict (no universally-safe silent flag).
- `#[serde(deny_unknown_fields)]` is on `RuleFile` and EVERY arg struct — any typo'd key fails load. (`schema.rs:28`)

**Stage spec YAML keys** — VERIFIED against `StageSpec` enum (`schema.rs:177-212`, `rename_all = "snake_case"`) and its arg structs. Use these exact key shapes:

| Primitive | YAML form | Arg struct (verified) |
|-----------|-----------|-----------------------|
| `strip_ansi` | `- strip_ansi` (bare unit variant) | none (`schema.rs:181`) |
| `drop_regex` | `- drop_regex: '<re>'` (string) | newtype String (`schema.rs:184`) |
| `keep_regex` | `- keep_regex: '<re>'` (string) | newtype String; **adjacent keep_regex OR-merge** (`schema.rs:187`) |
| `replace_regex` | `- replace_regex: { pattern: '<re>', replacement: '<s>' }` | `ReplaceRegexArgs` (`schema.rs:215-220`) |
| `dedupe` | `- dedupe` OR `- dedupe: { max_kept: 1 }` | `Option<DedupeArgs>`, `max_kept` default 1 (`schema.rs:223-229`) |
| `collapse_repeated` | `- collapse_repeated: { pattern: '<re>', max_kept: 5, summary: '… {count} more' }` | `CollapseArgs` (`schema.rs:236-245`) — **key is `summary`, NOT `summary_template`** (RESEARCH.md Pattern uses `summary` correctly; the runtime `Stage` field is `summary_template` but the YAML key is `summary`) |
| `keep_head` | `- keep_head: { lines: N }` or `{ bytes: N }` | `HeadTailArgs` (`schema.rs:248-257`) |
| `keep_tail` | `- keep_tail: { lines: N }` or `{ bytes: N }` | `HeadTailArgs` (`schema.rs:248-257`) |
| `keep_around_match` | `- keep_around_match: { pattern: '<re>', before: N, after: N }` | `KeepAroundArgs` (`schema.rs:260-269`) — **`before` AND `after` are BOTH required, no `#[serde(default)]`**; omitting either fails load |
| `max_bytes` | `- max_bytes: N` | newtype usize (`schema.rs:208`) — **omit unless overriding** the 32768 default (D-07) |
| `script` | (forbidden inside `pipeline:`) | rejected at load — Starlark only via top-level `post_process` (ADR-0008) |

**Success pipeline pattern (blacklist drop + collapse)** — RESEARCH Pattern 1, anchored on `valid_simple.yaml:5-8`:
```yaml
pipeline:
  - strip_ansi
  - drop_regex: '^\s*Compiling '
  - drop_regex: '^\s*Finished '
  # NO max_bytes — auto-injected (D-07)
```

**on_error pattern (context-preserving)** — RESEARCH Pattern 2, anchored on `parent.yaml:8-13`:
```yaml
on_error:
  pipeline:
    - strip_ansi
    - keep_around_match: { pattern: '(?i)^error', before: 0, after: 20 }
    - keep_tail: { lines: 40 }
```

**Cross-cutting constraints (apply to ALL ten rules):**
- `regex` crate is RE2-style — **no look-around** (`(?=)`, `(?!)`, `(?<=)`), **no backreferences** (`\1`). Use `keep_regex` whitelist instead of negated `drop_regex`. (RESEARCH Pitfall 3)
- Put all `keep_regex` stages **adjacent** (back-to-back) for OR semantics; a non-keep_regex stage between them turns OR into AND. Put `strip_ansi` *before* any keep_regex block. (RESEARCH Pitfall 1)
- The bundled YAML auto-embeds via rust-embed (`#[folder = "../../bundled-rules/"]`, `crates/lacon-core/src/rules/bundled.rs:21-23`) — dropping `<id>.yaml` into `bundled-rules/` is the entire integration step. `iter_bundled()` filters to `.yaml` only (`bundled.rs:28-32`), so the existing `.gitkeep` is ignored.

---

### `bundled-rules/test-base.yaml` (optional, D-06) (config, shared parent)

**Analog:** the `parent.yaml` + `child.yaml` extends pair (`crates/lacon-core/tests/fixtures/rules/`).

**Child extends shape — `child.yaml` (VERIFIED current):**
```yaml
id: child
extends: parent
pipeline:
  - drop_regex: '^Done'
```

**`extends` semantics to honor** (verified `merge_rules` at `crates/lacon-core/src/rules/loader.rs:555-571`):
- Parent pipeline is **prepended** to the child's: `child.pipeline = [parent_stages, child_stages].concat()` (`loader.rs:564-567`, ADR-0012). Parent stages run FIRST.
- Scalar fields (`description`, `match_spec`, `bypass_when`, `rewrite`, `on_error`, `post_process`) — child wins, else inherited from parent (`loader.rs:557-562`). So if the base defines `on_error`, children that omit it inherit it.
- `extends` ID may carry a `bundled/` prefix, stripped by `strip_layer_prefix` (`loader.rs:573-581`). Both `extends: bundled/test-base` and `extends: test-base` resolve.
- **Bundled→bundled resolution path** (D-06 risk — first phase to exercise it): `try_resolve_from_bundled` (`loader.rs:584-606`) → `find_in_bundled` (`loader.rs:608-620`) → `flatten_extends_with_lookup`. Implemented, NO existing fixture-level test. **Spike ONE extends rule first** (e.g. `cargo-test extends test-base`) before authoring the other three; fallback is copy-the-parent (D-06).
- **MaxBytes injection happens AFTER flatten** (`compile_pipeline:686-695`, "Pitfall 7" comment) — the base must NOT hand-place `max_bytes`, or children inherit a premature cap.

**Caveat from RESEARCH (lines 502, 718-719):** only `strip_ansi` + a generic `keep_tail` cap are genuinely shared across the 4 test runners (the per-test-PASS drop regex differs per tool). If the base reduces to just `strip_ansi`, the copy-the-parent fallback is likely cleaner. The base, if a separate rule, still needs a valid `match` (or be excluded from resolution).

---

### `tests/fixtures/<id>/<scenario>/input.txt` + `expected.txt` (test, file-I/O)

**Analog:** `tests/fixtures/primitives/<name>/{input.txt,expected.txt}` (existing 10 primitive fixture pairs, e.g. `tests/fixtures/primitives/keep_around_match/`).

**Layout to mirror** (sibling tree under workspace-root `tests/fixtures/`, NOT crate-root — confirmed by `primitives.rs:7-8` comment "fixtures live at the WORKSPACE root … shared with PLAN-05/PLAN-07"):
```
tests/fixtures/
  <rule-id>/
    <scenario>/         # slug: clean-install, with-warnings, compile-error …
      input.txt         # real captured merged stdout+stderr (D-03)
      expected.txt      # generated by running the rule's own pipeline (D-03 regen recipe)
      meta.yaml         # NEW — see below
```

**Content rules:**
- `input.txt` is **real captured tool output** (D-03), lightly trimmed/anonymized, never hand-synthesized. RESEARCH lines 240-621 carry verbatim 🟢 captures for 6 tools (cargo, git, pytest, docker, npm/pnpm/yarn) to seed fixtures; the 4 JS tools (tsc/eslint/vitest/jest) are 🟡 — executor MUST capture real output via `npx -p <pkg>` during execution.
- `expected.txt` is **generated, not authored** — run the rule pipeline against `input.txt` (the regeneration recipe at `docs/testing-rules.md:85-110`).
- A single trailing newline is tolerated by the comparison (see runner below) — editors may add one.

---

### `tests/fixtures/<id>/<scenario>/meta.yaml` (test, metadata) — NEW SHAPE

**Analog:** no existing `meta.yaml` in the repo (confirmed: `find tests/fixtures -name meta.yaml` → none). Closest is the documented schema at `docs/testing-rules.md:37-47` plus the `serde_saphyr::from_str` deserialize idiom at `parse_one` (`loader.rs:439-440`).

**Deserialize idiom to mirror** (verified `parse_one`, `crates/lacon-core/src/rules/loader.rs:439-440`):
```rust
serde_saphyr::from_str::<RuleFile>(content).map_err(|e| { /* ... */ })
```
For the test runner, define a small struct + `serde_saphyr::from_str::<FixtureMeta>(&meta_str)`:
```rust
// Mirror parse_one; serde_saphyr 0.0.26 is the workspace YAML parser.
#[derive(serde::Deserialize)]
struct FixtureMeta {
    command: String,
    exit_code: i32,                                       // D-02 NEW FIELD (load-bearing)
    #[serde(default)] tool_version: Option<String>,
    #[serde(default)] exempt_from_reduction_check: bool,  // D-05
    #[serde(default)] must_keep_lines: Vec<String>,       // D-05
    #[serde(default)] os: Option<String>,
    #[serde(default)] notes: Option<String>,
}
```

**meta.yaml content shape** (extends the documented block at `testing-rules.md:39-45` with `exit_code`):
```yaml
command: cargo build              # passed as command_raw to filter_bytes (ScriptCtx)
exit_code: 0                      # D-02 — 0=success pipeline, nonzero=on_error branch
tool_version: "cargo 1.95.0"
os: linux
exempt_from_reduction_check: false   # D-05 — set true on already-small failure fixtures
must_keep_lines:                     # D-05 — optional error-survival assertions
  - "error[E0308]"
notes: "multi-dep build, 5 Compiling lines"
```

**Why `exit_code` is load-bearing (D-02 / RESEARCH Pitfall 2):** `filter_bytes` selects the branch from `exit_code` — `0` → success pipeline, nonzero + `on_error` → on_error pipeline, nonzero + none → raw passthrough (verified `runtime/mod.rs:457-473`). Without it, failure fixtures silently run the success pipeline and never exercise `on_error`. Record the ACTUAL observed exit code (cargo build/test failure = 101, not 1 — RESEARCH A6).

---

### `crates/lacon-core/tests/bundled_rules.rs` (test, integration fixture-walk) — THE only new Rust

This is a **composite** of three verified analogs. Copy each piece from its source.

**Analog A — fixture path + byte-compare idiom:** `crates/lacon-core/tests/primitives.rs:16-44`

Path helper (VERIFIED `primitives.rs:16-26`) — adapt the subdir to `<rule-id>/<scenario>`:
```rust
fn fixture_path(primitive: &str, name: &str) -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");        // crates/lacon-core
    PathBuf::from(manifest_dir)
        .join("../..")                                     // workspace root
        .join("tests/fixtures/primitives")                 // → tests/fixtures/<rule-id>
        .join(primitive)
        .join(name)
}
```

Byte-exact compare normalization (VERIFIED `primitives.rs:38-43`, this is the D-04 idiom):
```rust
let out = pipeline.run(lines.into_iter());
let actual = out.join("\n");
// Trim trailing newline from expected (text editors add one); compare normalised.
let expected_trimmed = expected.trim_end_matches('\n').to_owned();
```
Then `assert_eq!(actual, expected_trimmed, "<msg>")` — plain `assert_eq!`, the whole suite uses it; **do NOT introduce `insta`** (D-09).

**Analog B — the resolve → Runner::new → filter_bytes call sequence:** `crates/lacon-cli/src/commands/explain.rs:116-153` (VERIFIED current line numbers). The runner is a stripped-down version (no DB, no two-column render):
```rust
// explain.rs:118 — loader (use None for hermetic bundled-only, NOT project_path_buf)
let mut loader = RuleLoader::new(None);
// explain.rs:119 — resolve by id
let resolved = loader.resolve(rule_id)?;     // ResolvedRule
// explain.rs:126-130 — options + Runner::new
let mut runner = Runner::new(resolved, RunOptions::default());
// explain.rs:141-147 — the filter_bytes call (5 args)
let lines = runner.filter_bytes(
    &merged,        // &[u8]   — input.txt bytes
    exit_code,      // i32     — from meta.exit_code (D-02)
    duration_ms,    // u64     — pass 0 in the test
    command_raw,    // &str    — from meta.command
    project_path,   // Option<String> — None
)?;
```

**Exact `filter_bytes` signature** (VERIFIED `crates/lacon-core/src/runtime/mod.rs:423-430`):
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
Branch logic (VERIFIED `runtime/mod.rs:457-473`): `exit_code == 0` → `success_pipeline.run_with_post_process`; `!= 0` with `on_error_pipeline` → that; `!= 0` with none → raw `lines` passthrough.

**Analog C — make_rule helper + branch assertions for unit-level tests:** `crates/lacon-core/tests/runtime_filter_bytes.rs:20-40` (the `make_rule(success, on_error) -> ResolvedRule` builder) and `:45-139` (the three branch tests). Use this style for any extends-spike unit test that doesn't need bundled YAML, and as the template for asserting filtered output.

**Synthesized replay helper** (RESEARCH-verified, lines 197-217, against the above):
```rust
use lacon_core::rules::loader::RuleLoader;
use lacon_core::runtime::{Runner, RunOptions};

fn replay(rule_id: &str, input: &[u8], exit_code: i32, command: &str) -> Vec<String> {
    let mut loader = RuleLoader::new(None);            // None → hermetic bundled-only (D-01)
    let resolved = loader.resolve(rule_id).expect("resolve");
    let mut runner = Runner::new(resolved, RunOptions::default());
    runner.filter_bytes(input, exit_code, 0, command, None).expect("filter_bytes")
}
```

**Three per-fixture assertions (D-05), all driven from meta.yaml:**
1. **Byte-exact:** `assert_eq!(replay(...).join("\n"), expected.trim_end_matches('\n'))` (D-04 idiom).
2. **Reduction:** `len(expected) as f64 / len(input) as f64 <= 0.5` on primary success fixtures; skip when `meta.exempt_from_reduction_check`.
3. **must_keep_lines:** every substring in `meta.must_keep_lines` must appear in the joined output (error-survival).

**Test target location (D-09):** `crates/lacon-core/tests/bundled_rules.rs` — cargo auto-discovers `tests/*.rs`; the filename makes `cargo test --test bundled_rules` resolve. (Confirmed: 18 sibling `tests/*.rs` files already auto-discovered, e.g. `primitives.rs`, `runtime_filter_bytes.rs`, `extends_flatten.rs`.)

---

### `docs/testing-rules.md` (MODIFIED, docs)

**Analog:** itself — the existing `meta.yaml` block at `docs/testing-rules.md:37-47`.

**Edit (D-02):** add `exit_code` to the documented `meta.yaml` schema block. Current block (lines 39-45) lacks it:
```yaml
command: pnpm install
tool_version: "pnpm 9.4.0"
captured_at: 2026-04-12
os: linux
notes: clean install on a fresh node_modules; lockfile up to date
```
Add `exit_code: 0` (with a one-line note that nonzero selects the `on_error` branch per ADR-0010, and that cargo build/test failures are 101). The "What each test verifies" section (lines 49-68) already documents `exempt_from_reduction_check` and `must_keep_lines` — no change needed there.

---

## Shared Patterns

### Hermetic rule resolution (subprocess-free)
**Source:** `crates/lacon-cli/src/commands/explain.rs:118-130` + `crates/lacon-core/src/rules/loader.rs:127` (`resolve`) + `:142` (bundled-layer branch)
**Apply to:** the `bundled_rules.rs` runner — every fixture replay.
```rust
let mut loader = RuleLoader::new(None);   // None project_dir → only project(absent)+user+bundled; bundled wins
let resolved = loader.resolve(rule_id)?;  // walks project → user → bundled (loader.rs:129-144)
```
`RuleLoader::new(None)` still consults the user layer via XDG (`loader.rs:111-113`), but in CI there are no user rules, so resolution falls through to the embedded bundled layer (`loader.rs:142`). Hermetic for CI.

### Byte-exact comparison with trailing-newline tolerance (D-04)
**Source:** `crates/lacon-core/tests/primitives.rs:38-43`
**Apply to:** every byte-exact assertion in `bundled_rules.rs`.
```rust
let actual = out.join("\n");
let expected_trimmed = expected.trim_end_matches('\n').to_owned();
assert_eq!(actual, expected_trimmed, "...");
```

### Workspace-root fixture path
**Source:** `crates/lacon-core/tests/primitives.rs:16-26` (`env!("CARGO_MANIFEST_DIR").join("../..").join("tests/fixtures/...")`). Also used at `extends_flatten.rs:12-17` (joins crate-root `tests/fixtures/rules` instead — note: bundled_rules fixtures use the workspace-root form, two levels up, per D-09).
**Apply to:** the `bundled_rules.rs` path helper.

### ScriptCtx / serde_saphyr YAML parse
**Source:** `crates/lacon-core/src/rules/loader.rs:439-440` (`parse_one` → `serde_saphyr::from_str`)
**Apply to:** `FixtureMeta` deserialization in the runner. `serde-saphyr 0.0.26` is the workspace YAML parser (same one the loader uses for rules).

### MaxBytes auto-injection (D-07)
**Source:** `crates/lacon-core/src/rules/loader.rs:686-695` (`compile_pipeline` appends `Stage::MaxBytes { cap: defaults_max_bytes=32768 }` when absent, on BOTH success and on_error pipelines independently — `compile_resolved:633-641`)
**Apply to:** all ten rule YAMLs — authors OMIT `max_bytes` unless overriding the 32768 cap.

### on_error replaces, never merges (ADR-0010)
**Source:** `crates/lacon-core/src/runtime/mod.rs:457-473` (branch) + `parent.yaml:8-13` (YAML shape)
**Apply to:** every rule's failure pipeline — self-contained, no inheritance from the success pipeline at runtime.

---

## Verification Notes (deltas vs. upstream docs)

1. **`collapse_repeated` YAML key is `summary`, not `summary_template`.** The runtime `Stage::CollapseRepeated` field is `summary_template` (seen in `primitives.rs:101`), but the YAML/`CollapseArgs` key is `summary` (`schema.rs:244`). RESEARCH.md Pattern at line 565 correctly uses `summary:` — follow RESEARCH, not the runtime field name.
2. **`keep_around_match` has NO defaults** — both `before` and `after` are plain `usize` with no `#[serde(default)]` (`schema.rs:266,268`), under `deny_unknown_fields`. Omitting either fails load. RESEARCH line 125 flags this correctly.
3. **explain.rs call-site line numbers** verified: `RuleLoader::new` at :118, `resolve` at :119, `Runner::new` at :130, `filter_bytes` at :141 — within the CONTEXT-cited 116-159 range. ✓
4. **`filter_bytes` signature** verified byte-for-byte at `runtime/mod.rs:423-430`. ✓
5. **`resolve` at loader.rs:127, `parse_one` at :439** — both confirmed. ✓
6. **In-repo rule YAML analogs DO exist** (CONTEXT hedged "may be none"): `crates/lacon-core/tests/fixtures/rules/{parent,child,valid_simple}.yaml` are real loadable rules. `bundled-rules/` itself holds only `.gitkeep`. Anchor rule authoring on `parent.yaml` (has on_error) + the schema spec.

## Metadata

**Analog search scope:** `bundled-rules/`, `tests/fixtures/`, `crates/lacon-core/tests/`, `crates/lacon-core/src/rules/`, `crates/lacon-core/src/runtime/`, `crates/lacon-cli/src/commands/`, `docs/specs/`, `docs/testing-rules.md`. Worktree copies under `.claude/worktrees/` excluded.
**Files scanned (read in full or targeted):** 12 (primitives.rs, runtime_filter_bytes.rs, explain.rs §, runtime/mod.rs §, loader.rs §×3, bundled.rs, schema.rs §×2, parent/child/valid_simple.yaml, extends_flatten.rs §, testing-rules.md)
**Pattern extraction date:** 2026-05-22
