# Phase 9: Output-fidelity safety — no fabrication on dedupe/collapse and guaranteed LACON_DISABLE bypass - Research

**Researched:** 2026-05-31
**Domain:** Rust streaming-pipeline primitives (lacon-core) + Claude Code PreToolUse adapter (lacon-adapter-claudecode)
**Confidence:** HIGH (all claims verified against source in this session; no external libraries involved)

## Summary

This phase reopens two "complete" v1.0 requirements after live validation reproduced two concrete bugs: (1) `collapse_repeated` emits a non-verbatim summary line that, in the field, looked like fabricated tool output (`table table table`), and (2) the Claude Code hook's `detect_bypass` only reads the hook *process's own* environment, so an inline `LACON_DISABLE=1 <cmd>` prefix on the command string never bypasses.

Both fixes are small, local, and fully constrained by the locked D-01..D-12 decisions. The bypass fix is a new leading-`NAME=value` parser in `detect_bypass` (adapter, `lib.rs:45-49`) that short-circuits to `PassThrough` *before* chain-split/wrap — no engine change needed. The fabrication fix is hybrid: (a) remove/narrow `collapse_repeated` from `git-status.yaml` so tabular file lines survive verbatim (D-08), and (b) standardize `collapse_repeated`'s remaining elision emission to an unambiguous `[lacon: …]`-namespaced marker modeled on the existing `[lacon: truncated, N more bytes dropped]` marker (D-07). `dedupe` is already verbatim-only and needs no behavior change (D-06).

A research finding the planner must weigh: the literal git-status summary template (`"\t… {count} more changed/untracked files"`) is **not itself** the `table table table` string the user observed — that came from a *loop printing per-item rows* (`name crates:… npm:…`), which does not match `git-status`. The phase's success criteria are still met by the hybrid fix (the marker convention plus removing collapse where it bites), but the planner should not assume the exact `table table table` bytes will be reproducible from `git-status` alone. See Pitfall 1 and Open Question 1.

**Primary recommendation:** Implement the inline-env-prefix parser in `detect_bypass` (whole-command bypass before wrap), narrow `git-status.yaml` to stop collapsing tabular file lines, standardize the `collapse_repeated` elision marker to `[lacon: collapsed N <noun>]` form, update `docs/specs/filter-rule-schema.md`, and add: a hook-e2e byte-exact passthrough test for the inline prefix, plus no-fabrication fixtures (tabular + repeated-prefix) proving every survivor is byte-identical to an input line.

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| REQ-engine-bypass | Byte-exact passthrough guarantee | Already satisfied at the engine layer by `run_bypassed` (`runtime/mod.rs:525-567`, `Stdio::inherit()`, no pipeline, `bypassed:true`). The gap is purely the hook; D-05 confirms engine backstop is correct as-is. |
| REQ-adapter-bypass-detection | Reliable inline `LACON_DISABLE=1` env-prefix detection in the PreToolUse hook | `detect_bypass` (`lib.rs:45-49`) only reads `std::env::var("LACON_DISABLE")`. New leading-`NAME=value` parser required (D-01..D-04). Short-circuit to `PassThrough` at `lib.rs:136-138` before chain-split. |
| REQ-engine-streaming-primitives | `dedupe`/`collapse_repeated` must never substitute or fabricate — elide explicitly or preserve | `dedupe` verbatim-only (`stages.rs:256-270`, D-06, no change). `collapse_repeated` summary is the only fabrication surface (`stages.rs:288-296` in-run, `432-444` flush) → standardize marker (D-07) + remove from git-status (D-08). |
</phase_requirements>

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** Add a new leading-env-assignment parser to the adapter. `detect_bypass` (`lib.rs:45-49`) must inspect the command **string**, not just the hook process's own env. No reusable parser exists today.
- **D-02:** On detecting an inline `LACON_DISABLE=1` prefix, the hook returns `PassThrough` **before any wrapping** (`lib.rs:136-138`). The real shell applies the assignment; the command runs unwrapped → byte-exact. No engine change required for the bypass guarantee.
- **D-03:** Parser strips a run of leading `NAME=value` tokens (leading position only — bash treats `NAME=val` as an assignment only before the command word), and bypasses iff some `LACON_DISABLE` assignment among them has the exact value `"1"`, matching the locked `as_deref() == Ok("1")` semantics (`lib.rs:384-394`, `runtime/mod.rs:191`). Other env assignments are ignored for bypass purposes.
- **D-04:** Handle reasonable quoting of the value (`LACON_DISABLE=1`, `LACON_DISABLE="1"`, `LACON_DISABLE='1'` all bypass). Do **not** bypass on non-leading occurrences (e.g. `echo LACON_DISABLE=1` must still filter). Cold-start budget ≤10ms must hold.
- **D-05:** `lacon run` already honors `LACON_DISABLE=1` from its own process env via byte-exact `run_bypassed` (`runtime/mod.rs:189-193, 525-567`). Left as-is; engine-layer backstop. The only gap was the hook.
- **D-06:** `dedupe` is confirmed fabrication-free — every emission is a verbatim input line (`stages.rs:256-270`). No change to `dedupe`. The only non-verbatim emissions in the stage set are `collapse_repeated`'s summary line and the `max_bytes` truncation marker.
- **D-07:** **Standardize the elision marker.** When `collapse_repeated` removes lines, the emitted line must be a visually-distinct, lacon-namespaced marker that cannot be mistaken for real tool output — modeled on `[lacon: truncated, N more bytes dropped]` (`stages.rs:450-457`). A free-form `summary_template` that blends into output (e.g. git-status's tab-indented `"\t… {count} more…"`) is the failure mode. Reinterpret success-criterion #1 as: *never emit a line that could be mistaken for real tool output* — a clearly-marked lacon elision line is permitted; a substituted/blending line is not.
- **D-08:** **Remove `collapse_repeated` where it collapses signal.** Aligned/tabular and repeated-prefix output is signal. Remove/narrow the `collapse_repeated` stage in `git-status.yaml` so per-file tabular lines survive verbatim.
- **D-09:** A line is never *substituted* — primitives either keep a line verbatim or drop it, and a drop leaves the D-07 standardized marker. No primitive may emit plausible-but-false text in place of a dropped line.
- **D-10:** Re-audit scope is exactly two bundled rules: `git-status.yaml` (`collapse_repeated` on `^\t`, max_kept 5 — apply D-08) and `tsc.yaml` (`dedupe` max_kept 1 — verbatim-only, confirm its success fixture preserves signal). No other bundled rule uses these primitives.
- **D-11:** Each affected rule's success-path fixture (`tests/fixtures/<rule-id>/<scenario>/`) is updated/added to prove every surviving line is byte-identical to an input line, and any elision is the D-07 marker — never a substitution. New fixtures use aligned/tabular and repeated-prefix input per success-criteria #1 and #3.
- **D-12:** The marker standardization touches `docs/specs/filter-rule-schema.md:128-137` (the `collapse_repeated` `summary` / `{count}` documentation). Treat as a deliberate, documented change — update the spec to describe the standardized elision-marker behavior. Keep it additive/clarifying where possible (D-07 keeps `collapse_repeated` available).

### Claude's Discretion
- Exact marker wording/format (within the `[lacon: …]` convention), and whether `summary_template` is retained as an optional suffix inside the marker or dropped in favor of a fixed marker — constrained by D-07 (must be unambiguously a lacon marker) and D-12 (spec updated to match).
- Whether the inline-prefix parser is shared with `argv_for_resolution` or kept standalone — constrained by the ≤10ms budget.

### Deferred Ideas (OUT OF SCOPE)
- Per-segment bypass granularity in chains — explicitly out (v1 contract is whole-command; v2 backlog).
- Generalizing the elision-marker convention to a user-configurable format — not needed for this phase; fixed/namespaced marker suffices.
</user_constraints>

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Inline `LACON_DISABLE=1` prefix detection | Adapter (`lacon-adapter-claudecode`) | — | The hook is the only place that sees the raw command string before it reaches the shell; it must decide bypass before wrapping. Engine never sees the un-wrapped command. |
| Byte-exact passthrough execution | Engine (`lacon-core/runtime`) | — | `run_bypassed` already inherits stdio and runs no pipeline. Defense-in-depth backstop (D-05); unchanged. |
| No-fabrication on collapse/dedupe | Engine (`lacon-core/pipeline/stages.rs`) | — | Primitives own line emission; the fabrication surface is `CollapseRepeated`'s summary. |
| Rule-level decision *not to collapse signal* | Rule config (`bundled-rules/git-status.yaml`) | Engine | Whether tabular output is collapsed is a per-rule wiring decision, not a primitive behavior. D-08 fix lives in YAML. |
| Marker contract documentation | Spec (`docs/specs/filter-rule-schema.md`) | — | The marker is user-facing contract; spec is the source of truth (CLAUDE.md). |

## Standard Stack

No external libraries are introduced by this phase. All work is within the existing Rust workspace using already-present crates (`regex`, `smallvec`, `serde`, `serde_saphyr` for fixtures, `assert_cmd` for e2e). No `npm install` / `cargo add` step. **Package Legitimacy Audit is N/A — zero new dependencies.**

### Existing components touched
| Component | Path | Role in this phase |
|-----------|------|--------------------|
| `detect_bypass` | `crates/lacon-adapter-claudecode/src/lib.rs:45-49` | Add inline-prefix parser (D-01..D-04) |
| `run_hook` bypass step | `crates/lacon-adapter-claudecode/src/lib.rs:135-138` | Short-circuit point (D-02) |
| `CollapseRepeated` step/flush | `crates/lacon-core/src/pipeline/stages.rs:272-297, 432-444` | Standardize marker (D-07) |
| `Dedupe` | `crates/lacon-core/src/pipeline/stages.rs:256-270` | Verify only — no change (D-06) |
| `git-status.yaml` | `bundled-rules/git-status.yaml:14-17` | Remove/narrow collapse (D-08) |
| `tsc.yaml` | `bundled-rules/tsc.yaml:17` | Confirm dedupe fixture preserves signal (D-10) |
| filter-rule-schema spec | `docs/specs/filter-rule-schema.md:128-137` | Update marker docs (D-12) |
| Fixtures | `tests/fixtures/git-status/`, `tests/fixtures/tsc/` | Update/add no-fabrication fixtures (D-11) |

## Architecture Patterns

### Data flow (the two hot paths this phase touches)

```
Claude Code Bash tool
   │  PreToolUse JSON {tool_input.command}
   ▼
[run_hook]  lib.rs:125
   │
   ├─ 0. non-Bash guard → PassThrough
   │
   ├─ 1. detect_bypass(command)  ◄── PHASE 9 BYPASS FIX HERE (lib.rs:45-49, 136-138)
   │        • "!!" prefix  → bypass
   │        • process env LACON_DISABLE=1 → bypass            (existing)
   │        • NEW: leading NAME=value scan; LACON_DISABLE="1" → bypass  (D-01..D-04)
   │        → PassThrough  (shell runs cmd unwrapped → byte-exact)
   │
   ├─ 2. split_chain  (chain.rs)
   ├─ 3. per-segment TUI check → whole-chain PassThrough
   └─ 4. per-segment resolve → is_wrap_safe → rewrite → quote → wrap as
            "LACON_ASSISTANT=… lacon run --rule <id> -- <argv>"
                 │
                 ▼
         [lacon run]  runtime/mod.rs:178
                 │
                 ├─ LACON_DISABLE=1 in OWN env → run_bypassed (Stdio::inherit, no pipeline)  ◄── D-05 backstop
                 └─ else → os_pipe merge → Pipeline (stages) → sink
                                                  │
                                                  ▼
                              [CollapseRepeated.step/flush]  ◄── PHASE 9 FABRICATION FIX (stages.rs:272-297, 432-444)
                                 emits verbatim survivors + ONE standardized [lacon: …] marker
```

### Pattern 1: Leading-assignment scan (POSIX assignment-position rule)
**What:** Bash treats `NAME=value` tokens as environment assignments **only** when they appear before the command word. After the first non-assignment token, `NAME=value` is just an argument. The parser must scan leading whitespace-delimited tokens, accept those matching `^[A-Za-z_][A-Za-z0-9_]*=…`, and stop at the first token that is not an assignment (that token is the command word).
**When to use:** In `detect_bypass`, after the `!!` check and before/instead-of the process-env check (or in addition to it).
**Example (illustrative — implementer's exact form is discretion per D-04):**
```rust
// Source: derived from D-03/D-04 + existing exact-"1" semantics (lib.rs:384-394).
// Scan leading NAME=value assignments; bypass iff a LACON_DISABLE assignment
// among them has value exactly "1" (after stripping one layer of '..' or "..").
fn inline_disable_bypass(command: &str) -> bool {
    for tok in command.split_whitespace() {
        // First non-assignment token is the command word → stop.
        let Some((name, value)) = split_leading_assignment(tok) else { break };
        if name == "LACON_DISABLE" && unquote_scalar(value) == "1" {
            return true;
        }
    }
    false
}
```
Note the value-unquoting must accept `1`, `"1"`, `'1'` (D-04). A token like `echo` (no `=`) ends the scan; `echo LACON_DISABLE=1` therefore never bypasses (the first token `echo` is not an assignment → loop breaks immediately). `split_whitespace` is acceptable here because a *leading* assignment value containing whitespace would have to be quoted, and the hot path only needs a cheap, allocation-light leading scan to stay within the ≤10ms budget.

### Pattern 2: Standardized lacon elision marker
**What:** When `CollapseRepeated` drops lines, emit a single line that is unambiguously a lacon marker, modeled on `[lacon: truncated, N more bytes dropped]`. Recommended shape: `[lacon: collapsed {count} <noun>]` (e.g. `[lacon: collapsed 118 lines]`). The leading `[lacon:` token is the same namespace already used by the two existing markers, so a reader (human or model) can recognize it as tool-injected, not tool-emitted.
**When to use:** Both the in-run summary path (`stages.rs:288-296`) and the flush path (`stages.rs:432-444`).
**Example:**
```rust
// Source: stages.rs:450-457 (existing max_bytes marker) — adopt the same [lacon: …] convention.
let marker = format!("[lacon: collapsed {} lines]", dropped);
out.push(Cow::Owned(marker));
```
Discretion (D-07): the implementer may retain `summary_template` as an *optional* suffix *inside* the marker (e.g. `[lacon: collapsed {count} lines — {summary}]`) or drop the free-form template entirely in favor of a fixed marker. Either is acceptable as long as the result cannot be mistaken for real tool output and the spec (D-12) is updated to match.

### Anti-Patterns to Avoid
- **Free-form `summary_template` that mimics tool output:** the current git-status template `"\t… {count} more changed/untracked files"` is tab-indented exactly like the file lines it replaces — it *blends in*. This is the D-07 failure mode. Never let a marker inherit the formatting of the lines it elides.
- **Bypassing after wrap:** if you only bypass inside `lacon run`, the hook has already rewritten the command and stripped the inline assignment into a quoted arg. Bypass must happen in the hook, before wrap (D-02).
- **Per-segment bypass:** out of scope (CON-chained-bypass-whole-command). The inline-prefix bypass is whole-command.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Byte-exact passthrough execution | A new bypass execution path | Existing `run_bypassed` (`runtime/mod.rs:525-567`) | Already correct (`Stdio::inherit()`, no pipeline, `bypassed:true`). D-05 confirms it's the backstop. |
| Shell quoting in the wrap path | Custom escaping | Existing `quote_for_shell` (`quote.rs`) | This phase does not touch the wrap path. |
| Full bash assignment grammar | A POSIX-complete parser | A cheap leading-token scan (Pattern 1) | The hot path needs ≤10ms; only leading assignments matter for bypass (D-03). Full grammar is over-engineering. |
| Chain splitting / wrap-safety | New parsing | Existing `split_chain` / `is_wrap_safe` (`chain.rs`) | The bypass short-circuits *before* these run; no change needed. |

**Key insight:** Almost everything this phase needs already exists. The bypass fix is one new helper function plus a call-site; the fabrication fix is a one-line marker change in two code paths plus a YAML edit. Resist building new machinery.

## Runtime State Inventory

This phase has **no rename/refactor/migration** dimension and no stored-state component. Verified:
- **Stored data:** None — no DB schema, key, or collection names change. The SQLite tracker is untouched.
- **Live service config:** None — no external service config.
- **OS-registered state:** None.
- **Secrets/env vars:** `LACON_DISABLE` and `LACON_ASSISTANT`/`LACON_SESSION_ID`/`LACON_TOOL_USE_ID` are read/emitted by name; this phase *adds* parsing of `LACON_DISABLE` from the command string but does not rename any var. No env-var name changes.
- **Build artifacts:** None — no package rename. (Bundled rules are rust-embed'd into `lacon-core` at build time, so a `git-status.yaml` edit requires a rebuild before the bundled-rules fixture test sees it — this is normal `cargo build` behavior, not stale-artifact risk.)

## Common Pitfalls

### Pitfall 1: Assuming the `table table table` string is reproducible from git-status
**What goes wrong:** The phase narrative ties the observed `table table table` fabrication to `collapse_repeated`. But the git-status summary template is `"\t… {count} more changed/untracked files"` — running it produces `\t… 118 more changed/untracked files` (see the current `tests/fixtures/git-status/many-untracked/expected.txt`), **not** `table table table`. The user's repro (TODO.md) was a *loop printing per-item rows* (`name crates:… npm:… github:…`) — a command that does **not** match the `git-status` rule at all.
**Why it happens:** The `table table table` artifact most plausibly came from a *different* rule (or a `collapse_repeated`/`replace_regex` interaction on the user's own command), or from output that matched some rule's `collapse_repeated` whose template repeated a captured word. The exact originating rule was not captured in the repro.
**How to avoid:** Treat success-criterion #1 per its D-07 reinterpretation: *no line may be mistaken for real tool output*. Build no-fabrication fixtures that reproduce the *class* (aligned columns, repeated-prefix loops, grep hits) and assert every survivor is byte-identical to an input line and every elision is the `[lacon: …]` marker. Do **not** block the phase on reproducing the exact `table table table` bytes. See Open Question 1.

### Pitfall 2: Inline prefix bypass leaking into the filtered path via the wrap
**What goes wrong:** If bypass is checked too late, `run_hook` wraps the command and the inline `LACON_DISABLE=1` becomes a quoted literal argument or an env on the *wrapper* — and the wrapper's `lacon run` may or may not honor it depending on how the shell applies it. Result: inconsistent bypass (exactly the reported symptom).
**Why it happens:** The current `detect_bypass` only reads the hook's own env; the inline prefix is invisible to it, so the command proceeds to wrap.
**How to avoid:** Short-circuit in `detect_bypass` at `lib.rs:136-138`, returning `PassThrough` before `split_chain`. The unmodified command then reaches the real shell, which applies the assignment and runs the command unwrapped → byte-exact (D-02).

### Pitfall 3: `split_whitespace` mis-handling a quoted assignment value
**What goes wrong:** `LACON_DISABLE='1'` survives `split_whitespace` as one token `LACON_DISABLE='1'`; but `LACON_DISABLE='a b'` would split. For *bypass* purposes only the exact value `1` matters, and `1` never contains whitespace, so this is benign — but the unquoting step must strip the surrounding quotes before the `== "1"` compare (D-04).
**Why it happens:** Naive `value == "1"` fails for `'1'` / `"1"`.
**How to avoid:** Unquote one balanced layer of `'...'` or `"..."` before comparing. Add unit tests for all three forms plus the negative `echo LACON_DISABLE=1`.

### Pitfall 4: Forgetting the flush path when changing the marker
**What goes wrong:** `CollapseRepeated` emits the summary in **two** places — mid-stream on a non-matching line (`stages.rs:288-296`) and at end-of-stream in `flush()` (`stages.rs:432-444`). Changing only one leaves an inconsistent marker.
**How to avoid:** Change both. Both currently use `summary_template.replace("{count}", …)`; both must produce the new `[lacon: …]` marker. Preserve the CR-03 guard (`flush` only emits when `dropped > 0`).

### Pitfall 5: Bundled-rule edit not reflected without rebuild
**What goes wrong:** `git-status.yaml` is embedded via rust-embed at build time. Editing the YAML and running `cargo test` without a prior `cargo build` of the workspace can run stale embedded bytes (and the CLAUDE.md note already warns a bare `cargo test` panics on unresolved helper bins).
**How to avoid:** Follow the documented `cargo build --workspace && cargo test --workspace` order. The bundled-rules fixture test (`bundled_rules.rs`) replays through `RuleLoader::new(None)` which reads the embedded rule, so the rebuild is mandatory.

## Code Examples

### Existing verbatim-only dedupe (D-06 — confirm, do not change)
```rust
// Source: crates/lacon-core/src/pipeline/stages.rs:256-270 (verified this session)
Stage::Dedupe { last, max_kept, repeat_count, kept_so_far: _ } => {
    let is_dup = last.as_deref() == Some(&line);
    if is_dup {
        if *repeat_count < *max_kept { *repeat_count += 1; out.push(line); }
        // else: consecutive duplicate beyond max_kept — drop
    } else {
        *last = Some(line.clone().into_owned());
        *repeat_count = 1;
        out.push(line);   // every emission is a verbatim input line
    }
}
```

### Existing collapse_repeated summary (the fabrication surface — to be standardized)
```rust
// Source: crates/lacon-core/src/pipeline/stages.rs:288-296 (in-run) + 432-444 (flush)
if *kept_so_far > 0 || *dropped > 0 {
    let summary = summary_template.replace("{count}", &dropped.to_string());
    out.push(Cow::Owned(summary));   // ← non-verbatim; D-07 makes this an unambiguous [lacon: …] marker
    *kept_so_far = 0;
    *dropped = 0;
}
```

### Existing byte-exact engine bypass (D-05 — backstop, unchanged)
```rust
// Source: crates/lacon-core/src/runtime/mod.rs:189-193 + 525-567
if std::env::var("LACON_DISABLE").as_deref() == Ok("1") {
    return self.run_bypassed(argv, sink, started);  // Stdio::inherit(), no pipeline, bypassed:true
}
```

### Existing hook e2e bypass test (the template for the new inline-prefix test)
```rust
// Source: crates/lacon-adapter-claudecode/tests/hook_e2e.rs:188-201
// Today only tests process-env LACON_DISABLE. The new test asserts an INLINE
// prefix in tool_input.command bypasses (empty stdout = pass-through).
#[test]
fn bypass_via_lacon_disable_env_emits_empty_stdout() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE);
    let payload = bash_payload(&dir.path().to_string_lossy(), "echo hi");
    let output = run_hook_with_input_and_env(&payload, &[("LACON_DISABLE", "1")]);
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
}
```

## Elision-Marker Convention (D-07 / D-12 reconciliation)

There are currently **three** non-verbatim emission points in the codebase (the marker convention should be consistent across all of them):

| Marker | Source | Form today | In scope for D-07? |
|--------|--------|-----------|--------------------|
| Byte-cap truncation | `stages.rs:450-457` | `[lacon: truncated, N more bytes dropped]` | No (already namespaced; it is the *model*) |
| Per-line cap | `runtime/mod.rs:288` | `<line> [lacon: line truncated]` (suffix) | No (already namespaced) |
| **Collapse summary** | `stages.rs:288-296, 432-444` | free-form `summary_template` (e.g. `\t… {count} more…`) | **YES — this is the fix** |

**Recommended standardized form:** `[lacon: collapsed {count} lines]` (or `{noun}` if a per-rule noun is retained as a suffix inside the brackets). It matches the existing `[lacon: <verb>, …]` shape and is impossible to confuse with real tool output. The exact wording is D-07 discretion.

**Spec change (D-12):** `docs/specs/filter-rule-schema.md:128-137` currently documents `collapse_repeated: { pattern, max_kept, summary }` with `{count}` substitution into a free-form `summary`. Update to describe that the elided lines are replaced by a standardized `[lacon: …]` marker. If `summary`/`summary_template` is retained as an optional suffix, document that it is rendered *inside* the marker; if dropped, document its removal. Keep additive where possible (the primitive itself survives — D-07).

## Bundled-Rule Re-Audit (D-08, D-10, D-11)

### git-status.yaml (HIGH risk — the rule to fix)
Current pipeline (`bundled-rules/git-status.yaml:6-18`):
```yaml
pipeline:
  - strip_ansi
  - drop_regex: '^\s*\(use '
  - collapse_repeated:
      pattern: '^\t'
      max_kept: 5
      summary: "\t… {count} more changed/untracked files"   # ← collapses tabular file lines (signal)
```
Current fixture (`tests/fixtures/git-status/many-untracked/expected.txt`) keeps only 5 file lines and emits `\t… 118 more changed/untracked files`. Per D-08, the tab-indented per-file lines are **signal** and must survive verbatim. The fix removes/narrows `collapse_repeated`. Options for the planner (D-08 says "remove/narrow"):
- **Remove the stage entirely:** all file lines survive verbatim. *Risk:* this fixture is the rule's only non-exempt success fixture, and `bundled_rules.rs` enforces `expected/input ≤ 0.5` reduction (`bundled_rules.rs:120-135`). With the stage removed, output ≈ input (the only reduction is the one `(use …)` drop_regex line) → **the ≥50% reduction assertion fails.** The planner must either (a) set `exempt_from_reduction_check: true` on the regenerated fixture (justified: tabular file lists are signal, like `tsc`), or (b) keep a *narrowed* collapse that only collapses provably-noise lines, or (c) replace the primary fixture with one whose reduction comes from a non-signal source.
- **Narrow the stage:** keep `collapse_repeated` but change emission to the standardized marker so any collapse is unambiguous. Still risks the reduction floor depending on input.

**Recommendation:** Remove the signal-collapsing stage and set `exempt_from_reduction_check: true` with a `must_keep_lines` proving the file lines survive (mirrors how `tsc` handles "the output IS the signal"). This is the cleanest expression of D-08. The planner should make this an explicit decision and document it in the fixture `meta.yaml` `notes`.

### tsc.yaml (LOW risk — confirm only)
`bundled-rules/tsc.yaml:17` uses `dedupe: { max_kept: 1 }`, which is verbatim-only (D-06). The `tsc/type-errors` fixture is already `exempt_from_reduction_check: true` with `must_keep_lines: ["error TS"]`, and its input has no consecutive duplicates so dedupe drops nothing (`expected.txt == input.txt`). **No change required**; D-10/D-11 only ask to confirm the success fixture preserves signal — it does (verified this session).

### Fixtures requiring regeneration/addition
| Fixture | Action | Why |
|---------|--------|-----|
| `tests/fixtures/git-status/many-untracked/{input,expected,meta}.txt/yaml` | **Regenerate** | After D-08, expected must show all file lines verbatim (or a `[lacon: …]` marker if a narrowed collapse is kept). Likely set `exempt_from_reduction_check: true`. |
| `tests/fixtures/git-status/<new tabular scenario>/` | **Add (recommended)** | A scenario proving every survivor is byte-identical to an input line per success-criterion #1/#3. |
| `tests/fixtures/git-status/not-a-repo/` | Verify unchanged | `on_error` path; not affected by D-08. |
| `tests/fixtures/tsc/type-errors/` | Verify unchanged | Already compliant (D-10). |
| New repeated-prefix / grep-hit fixture (collapse_repeated unit-level) | **Add** | The `primitives/` tree or a `stages.rs` unit test should prove `collapse_repeated` emits the `[lacon: …]` marker and only verbatim survivors. |

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust built-in `#[test]` + `assert_cmd` (e2e) + data-driven fixture walker (`bundled_rules.rs`) |
| Config file | none — `cargo test` driven; fixtures live at workspace-root `tests/fixtures/` |
| Quick run command | `cargo test -p lacon-core --test bundled_rules` / `cargo test -p lacon-adapter-claudecode --test hook_e2e` |
| Full suite command | `cargo build --workspace && cargo test --workspace` (build first — CLAUDE.md load-bearing note) |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| REQ-adapter-bypass-detection | Inline `LACON_DISABLE=1 <cmd>` → byte-exact passthrough (empty stdout, no rewrite) | e2e (assert_cmd) | `cargo test -p lacon-adapter-claudecode --test hook_e2e` | ❌ Wave 0 — add `inline_lacon_disable_prefix_passes_through` |
| REQ-adapter-bypass-detection | `LACON_DISABLE="1"` / `'1'` quoting variants bypass; `echo LACON_DISABLE=1` does NOT | unit | `cargo test -p lacon-adapter-claudecode detect_bypass` | ❌ Wave 0 — extend `detect_bypass_*` unit tests in `lib.rs` |
| REQ-adapter-bypass-detection | Inline prefix in a chain bypasses the whole command | e2e | `cargo test -p lacon-adapter-claudecode --test hook_e2e` | ❌ Wave 0 |
| REQ-engine-bypass | `lacon run` with own-env `LACON_DISABLE=1` is byte-exact | integration (existing) | `cargo test -p lacon-cli` | ✅ (D-05 backstop already tested) |
| REQ-engine-streaming-primitives | `collapse_repeated` emits only verbatim survivors + one `[lacon: …]` marker (tabular + repeated-prefix) | unit | `cargo test -p lacon-core collapse_repeated` | ⚠️ exists for old template — update assertions in `stages.rs` tests |
| REQ-engine-streaming-primitives | `dedupe` survivors byte-identical to input (regression) | unit (existing) | `cargo test -p lacon-core dedupe` | ✅ |
| REQ-engine-streaming-primitives (#1/#3) | git-status success fixture: every survivor byte-identical to an input line | fixture | `cargo test -p lacon-core --test bundled_rules` | ⚠️ regenerate `git-status/many-untracked` + add tabular scenario |

### Sampling Rate
- **Per task commit:** the targeted command for the layer touched (`cargo test -p lacon-adapter-claudecode --test hook_e2e` for bypass; `cargo test -p lacon-core collapse_repeated` / `--test bundled_rules` for fabrication).
- **Per wave merge:** `cargo test --workspace` (after `cargo build --workspace`).
- **Phase gate:** Full suite green + `cargo clippy --workspace --all-targets` + `cargo fmt --check` before `/gsd-verify-work`.

### Wave 0 Gaps
- [ ] `crates/lacon-adapter-claudecode/tests/hook_e2e.rs` — new test: `inline_lacon_disable_prefix_passes_through` (asserts empty stdout / no rewrite for `LACON_DISABLE=1 echo hi`, and a quoted-variant case). A *true* byte-exact assertion against the unwrapped command's stdout (success-criterion #2) requires spawning the real command; the existing e2e harness asserts pass-through (empty stdout = hook did not rewrite), which is the hook-level proof. If a literal byte-exact stdout comparison is desired, add a `lacon-cli` integration test that runs `lacon run` is NOT involved (bypass means no wrap) — instead assert the hook returns no rewrite and document that the shell then runs the raw command.
- [ ] `crates/lacon-adapter-claudecode/src/lib.rs` — extend `detect_bypass` unit tests: quoting variants, leading-position-only, negative `echo LACON_DISABLE=1`.
- [ ] `crates/lacon-core/src/pipeline/stages.rs` — update `collapse_repeated_*` unit tests to assert the new `[lacon: …]` marker; add a repeated-prefix/tabular case proving verbatim survivors.
- [ ] `tests/fixtures/git-status/many-untracked/` — regenerate `expected.txt` + `meta.yaml` (likely `exempt_from_reduction_check: true`); optionally add a second tabular scenario.
- [ ] No framework install needed — all harnesses exist.

## State of the Art

Not applicable — this is a local Rust workspace with locked ADRs and no external-ecosystem dependency. No "current vs old approach" axis beyond the in-repo decisions captured in D-01..D-12.

## ADR Conflict Analysis (focus item 6)

| ADR | Relevance | Conflict? |
|-----|-----------|-----------|
| 0005 streaming-first | The fabrication fix stays line-by-line (verbatim emit or drop-with-marker); no buffering/reorder introduced. | **No conflict.** Marker emission is already streaming. |
| 0006 hybrid rewrite-and-filter | Bypass-before-wrap is consistent with the adapter's rewrite role. | **No conflict.** |
| 0007 first-match-wins | Unaffected — no resolution change. | No conflict. |
| 0010 on-error-replaces-pipeline | git-status `on_error` branch (`keep_regex '^fatal:'`) is untouched by D-08 (which targets the success pipeline's collapse stage). | No conflict. |
| 0012 append-only inheritance | git-status does not use `extends`; editing its pipeline in place is fine. | No conflict. |
| 0013 filter-via-pretooluse-wrapper | The bypass short-circuits *before* the wrapper is constructed — fully within ADR-0013's model (the hook decides whether to wrap). Cold-start ≤10ms budget preserved (cheap leading scan). | **No conflict** — but the ≤10ms budget is a hard constraint the parser must respect (D-04). |
| CON-chained-bypass-whole-command | Inline-prefix bypass is whole-command (returns `PassThrough` for the entire input). | **Aligned** — per-segment bypass is explicitly deferred. |

**Spec contract (not an ADR but contract-level per CLAUDE.md):** `docs/specs/filter-rule-schema.md` *is* changed by D-12. This is a deliberate, documented breaking change to the `collapse_repeated` `summary` semantics. The planner must include the spec update as a task and note it as a user-facing contract change. No ADR forbids this; ADRs are silent on the marker wording.

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | The observed `table table table` came from a non-git-status command (a per-item loop), not from the git-status `collapse_repeated` template | Pitfall 1, Open Q1 | LOW — the hybrid fix still satisfies the success criteria; only affects whether the exact string is reproducible in a fixture. |
| A2 | Removing the git-status collapse stage will breach the `bundled_rules.rs` ≥50% reduction floor, requiring `exempt_from_reduction_check: true` | Bundled-Rule Re-Audit | LOW-MEDIUM — verified the assertion exists (`bundled_rules.rs:120-135`) and current reduction depends on the collapse; the planner must make the exempt-vs-narrow call explicitly. |
| A3 | A cheap `split_whitespace`-based leading scan stays within the ≤10ms cold-start budget | Pattern 1, D-04 | LOW — the scan is O(prefix length), allocation-light; the existing hot path already does comparable string work. |

**Note:** A1 is the one item the planner should surface for confirmation if reproducing the exact `table table table` bytes is considered a hard acceptance requirement. Per the D-07 reinterpretation of success-criterion #1, it is not — the class-based fixtures suffice.

## Open Questions

1. **Is reproducing the literal `table table table` string a phase acceptance requirement?**
   - What we know: it was observed live; the git-status template does not produce it; the originating command (per TODO.md) was a per-item loop, not `git status`.
   - What's unclear: which rule (if any) produced it, or whether it was a user-command artifact.
   - Recommendation: do NOT gate on the exact string. Gate on the D-07 reinterpretation (no line mistakable for real output) using tabular + repeated-prefix + grep-hit fixtures. Flag to user if they want the exact repro chased.

2. **git-status reduction floor: exempt vs. narrow (D-08 "remove/narrow")?**
   - What we know: removing collapse makes output ≈ input → breaches the ≥50% assertion.
   - What's unclear: user preference between (a) exempt the fixture (tabular = signal, like tsc) or (b) keep a narrowed collapse with the standardized marker.
   - Recommendation: exempt (cleanest D-08 expression). Planner should decide explicitly and record in `meta.yaml` notes.

3. **Byte-exact assertion strength for the bypass e2e (success-criterion #2 wording).**
   - What we know: when the hook bypasses, it returns `PassThrough` (empty stdout) and the *shell* runs the unwrapped command — the hook never produces the command's stdout, so a "stdout equals unwrapped command's stdout" assertion is not a hook-level test.
   - Recommendation: assert at the hook layer that no rewrite occurs (empty stdout). If a literal byte-exact-passthrough assertion is wanted, add a separate `lacon run` engine test proving `run_bypassed` is byte-exact (the D-05 backstop), and treat the two together as the criterion-#2 proof.

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain (pinned) | all builds/tests | ✓ (assumed — workspace builds) | per `rust-toolchain.toml` | — |
| `/bin/sh` | `quote.rs` round-trip tests (not touched this phase) | ✓ (CLAUDE.md: present on all v1 targets) | — | — |
| git | NOT required for fixtures — replay is subprocess-free (`bundled_rules.rs` D-01) | n/a | — | fixtures are static `input.txt` |

No external dependency blocks this phase. Fixtures are static text replayed in-process; no real `git`/`tsc` invocation is needed for the tests.

## Sources

### Primary (HIGH confidence — read this session)
- `crates/lacon-adapter-claudecode/src/lib.rs` — `detect_bypass` (45-49), `run_hook` bypass step (135-138), wrap form (213-219), exact-"1" unit tests (384-394)
- `crates/lacon-adapter-claudecode/src/chain.rs` — `split_chain`, `is_wrap_safe` (accepts `KEY=value` prefix as wrap-safe, line 700 test)
- `crates/lacon-adapter-claudecode/src/quote.rs` — `quote_for_shell` (not modified this phase)
- `crates/lacon-adapter-claudecode/tests/hook_e2e.rs` — full e2e structure, existing bypass test (188-201)
- `crates/lacon-core/src/pipeline/stages.rs` — `Dedupe` (256-270), `CollapseRepeated` (272-297 step, 432-444 flush), `MaxBytes` marker (450-457)
- `crates/lacon-core/src/runtime/mod.rs` — engine bypass (189-193), `run_bypassed` (525-567), per-line truncation marker (288)
- `crates/lacon-core/tests/bundled_rules.rs` — fixture walker, 3-assertion contract, reduction floor (120-135)
- `bundled-rules/git-status.yaml`, `bundled-rules/tsc.yaml` — the two rules in scope
- `tests/fixtures/git-status/many-untracked/`, `tests/fixtures/tsc/type-errors/` — current fixture triples
- `crates/lacon-core/src/rules/loader.rs` — `DEFAULT_MAX_BYTES = 32_768`, max_bytes auto-injection
- `docs/specs/filter-rule-schema.md:128-137` — `collapse_repeated` contract
- `docs/decisions/*` — ADR list (0001-0014); 0005, 0010, 0012, 0013 reviewed for conflict
- `09-CONTEXT.md`, `TODO.md` — locked decisions + originating repro

### Secondary / Tertiary
- None — no web research performed; local-only Rust workspace, codebase evidence sufficient.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — zero new dependencies; all touchpoints read in source.
- Architecture / data flow: HIGH — traced both hot paths through actual code.
- Bypass fix: HIGH — `detect_bypass` and `is_wrap_safe` confirmed; only ambiguity is parser-sharing (D-04 discretion).
- Fabrication fix: HIGH on mechanism (collapse summary is the surface); MEDIUM on the exact `table table table` provenance (A1 — flagged).
- Bundled-rule re-audit: HIGH — both rules + fixtures + the reduction-floor interaction verified.
- Pitfalls: HIGH — derived from read source, not assumed.

**Research date:** 2026-05-31
**Valid until:** stable indefinitely (internal code, locked ADRs) — re-verify only if `stages.rs`, `lib.rs`, `chain.rs`, or the two bundled rules change before planning.
