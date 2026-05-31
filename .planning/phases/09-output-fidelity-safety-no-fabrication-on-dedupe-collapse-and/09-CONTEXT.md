# Phase 9: Output-fidelity safety — no fabrication on dedupe/collapse and guaranteed LACON_DISABLE bypass - Context

**Gathered:** 2026-05-31 (assumptions mode)
**Status:** Ready for planning

<domain>
## Phase Boundary

lacon must never substitute or fabricate content when filtering, and `LACON_DISABLE=1` must be a hard guarantee of byte-exact passthrough on the Claude Code Bash hot path. Two previously-"complete" requirements are reopened because v1.0 validation (2026-05-31) reproduced two concrete bugs:

1. **Fabrication on collapse** — structurally-similar lines (aligned/tabular output, repeated-prefix loops, grep hits) were replaced by a synthetic line (observed: `table table table`) that blended into real output, so filtered output looked valid but was false.
2. **Inline bypass failure** — `LACON_DISABLE=1 <cmd>` as an inline env prefix did not reliably bypass; only `LACON_DISABLE=1 bash -c '…'` worked sometimes.

**In scope:** fixing `detect_bypass` to honor the inline env prefix; making `dedupe`/`collapse_repeated` fidelity-safe (no line emitted that could be mistaken for real tool output); re-auditing the two bundled rules that use these primitives; any spec change the marker fix requires.

**Out of scope:** new primitives, new bypass syntaxes beyond `LACON_DISABLE`/`!!`, per-segment bypass (whole-command granularity is locked, CON-chained-bypass-whole-command), changes to the other 8 primitives, engine streaming model (ADR-0005 locked).
</domain>

<decisions>
## Implementation Decisions

### Bypass detection — inline `LACON_DISABLE=1` env prefix
- **D-01:** Add a new leading-env-assignment parser to the adapter. `detect_bypass` (`crates/lacon-adapter-claudecode/src/lib.rs:45-49`) must inspect the command **string**, not just the hook process's own env. There is no reusable parser today — the "D-26 env-var prefix" is only the literal prefix the hook *emits* when wrapping, not a parser of incoming assignments.
- **D-02:** On detecting an inline `LACON_DISABLE=1` prefix, the hook returns `PassThrough` **before any wrapping** (`lib.rs:136-138`). The real shell then applies the assignment and the command runs unwrapped → byte-exact output. Bypassing before wrap is the simple, sufficient fix; no engine change is required for the bypass guarantee.
- **D-03:** Parser strips a run of leading `NAME=value` tokens (leading position only — bash treats `NAME=val` as an assignment only before the command word), and bypasses iff some `LACON_DISABLE` assignment among them has the exact value `"1"`, matching the locked `as_deref() == Ok("1")` semantics (`lib.rs:384-394`, `runtime/mod.rs:191`). Other env assignments are ignored for bypass purposes.
- **D-04:** Handle reasonable quoting of the value (`LACON_DISABLE=1`, `LACON_DISABLE="1"`, `LACON_DISABLE='1'` all bypass). Do **not** bypass on non-leading occurrences (e.g. `echo LACON_DISABLE=1` must still filter). Cold-start budget ≤10ms must hold — the parse is cheap leading-token scanning.

### Defense-in-depth inside `lacon run`
- **D-05:** `lacon run` already honors `LACON_DISABLE=1` from its own process env via byte-exact `run_bypassed` (`crates/lacon-core/src/runtime/mod.rs:189-193, 525-567` — `Stdio::inherit()`, no pipeline, no `max_bytes`, `bypassed:true`). This is left as-is; it is the engine-layer backstop. The only gap was the hook, addressed by D-01–D-04.

### Fabrication fix — collapse_repeated / dedupe (HYBRID: remove where it bites + standardize the marker)
- **D-06:** `dedupe` is confirmed fabrication-free — every emission is a verbatim input line (`crates/lacon-core/src/pipeline/stages.rs:256-270`). No change to `dedupe` behavior. The only non-verbatim emissions in the entire stage set are `collapse_repeated`'s summary line and the `max_bytes` truncation marker.
- **D-07:** **Standardize the elision marker.** When `collapse_repeated` removes lines, the emitted line must be a visually-distinct, lacon-namespaced marker that cannot be mistaken for real tool output — modeled on the existing `[lacon: truncated, N more bytes dropped]` marker (`stages.rs:450-457`). A free-form `summary_template` that blends into output (e.g. git-status's tab-indented `"\t… {count} more…"`, `bundled-rules/git-status.yaml:17`) is the failure mode and must not be possible to produce ambiguously. Reinterpret success-criterion #1 as: *never emit a line that could be mistaken for real tool output* — a clearly-marked lacon elision line is permitted; a substituted/blending line is not.
- **D-08:** **Remove `collapse_repeated` where it collapses signal.** Aligned/tabular and repeated-prefix output is signal. Bundled rules must not collapse it. Specifically remove/narrow the `collapse_repeated` stage in `git-status.yaml` so per-file tabular lines survive verbatim. (User's words: "remove collapse_repeated where it may bite us and add a marker.")
- **D-09:** A line is never *substituted* — primitives either keep a line verbatim or drop it, and a drop leaves the D-07 standardized marker. No primitive may emit plausible-but-false text in place of a dropped line.

### Bundled-rule re-audit
- **D-10:** Re-audit scope is exactly two bundled rules: `git-status.yaml` (`collapse_repeated` on `^\t`, max_kept 5 — high risk, the one that bit; apply D-08) and `tsc.yaml` (`dedupe` max_kept 1 — low risk, verbatim-only, likely already compliant; confirm its success fixture preserves signal). No other bundled rule uses `dedupe`/`collapse_repeated`.
- **D-11:** Each affected rule's success-path fixture (`tests/fixtures/<rule-id>/<scenario>/`) is updated/added to prove every surviving line is byte-identical to an input line, and any elision is the D-07 marker — never a substitution. New fixtures use aligned/tabular and repeated-prefix input per success-criteria #1 and #3.

### Spec / schema impact
- **D-12:** The marker standardization touches the user-facing contract in `docs/specs/filter-rule-schema.md:128-137` (the `collapse_repeated` `summary` / `{count}` documentation). Treat as a deliberate, documented change — update the spec to describe the standardized elision-marker behavior. Keep it additive/clarifying where possible (D-07 keeps `collapse_repeated` available; it does not remove the primitive).

### Claude's Discretion
- Exact marker wording/format (within the `[lacon: …]` convention), and whether `summary_template` is retained as an optional suffix inside the marker or dropped in favor of a fixed marker — planner/implementer decides, constrained by D-07 (must be unambiguously a lacon marker) and D-12 (spec updated to match).
- Whether the inline-prefix parser is shared with `argv_for_resolution` or kept standalone — implementer's call, constrained by the ≤10ms budget.

### Folded Todos
- TODO.md (the v1.0 validation repro) is the originating source for this entire phase; its three "what would help" asks map to D-07/D-08/D-09 (fidelity) and D-01–D-05 (bypass). No separate backlog todos matched.
</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

- `docs/specs/filter-rule-schema.md` — the `dedupe` / `collapse_repeated` / `max_bytes` contract (lines ~128-137 for collapse summary + `{count}`). Any change here is a breaking change for users (D-12).
- `docs/decisions/0005-streaming-first.md` — streaming line-by-line transformer model; memory bound. Constrains any fidelity fix.
- `crates/lacon-core/src/pipeline/stages.rs` — `dedupe` (256-270, verbatim), `collapse_repeated` summary emission (276-291 in-run, 428-444 flush), `max_bytes` marker (450-457, the model for D-07).
- `crates/lacon-adapter-claudecode/src/lib.rs` — `detect_bypass` (45-49, the bug), `run_hook` flow + `PassThrough` (136-138), env-prefix emission at ~209-219, exact-`"1"` test (384-394).
- `crates/lacon-adapter-claudecode/src/chain.rs` — chain split + `is_wrap_safe`; relevant because `KEY=value cmd` is currently treated as wrap-safe literal.
- `crates/lacon-core/src/runtime/mod.rs` — `lacon run`; `LACON_DISABLE` honored at 189-193, byte-exact `run_bypassed` at 525-567 (D-05 backstop).
- `bundled-rules/git-status.yaml` (collapse on `^\t`, the rule to fix per D-08) and `bundled-rules/tsc.yaml` (dedupe, to confirm per D-10).
- `tests/fixtures/git-status/` and `tests/fixtures/tsc/` — fixture triples to update per D-11.
- `TODO.md` (repo root) — the live v1.0 repro that originated this phase.
</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `run_bypassed` (`runtime/mod.rs:525-567`) is already a correct byte-exact passthrough — reuse as the engine-layer guarantee; no new bypass machinery needed at the engine.
- The `max_bytes` truncation marker `[lacon: truncated, N more bytes dropped]` (`stages.rs:450-457`) is the template/convention for the D-07 standardized elision marker.
- `detect_bypass`'s existing exact-`"1"` test (`lib.rs:384-394`) defines the value semantics the new prefix parser must match.

### Established Patterns
- Whole-command bypass granularity is locked (CON-chained-bypass-whole-command) — the inline-prefix fix bypasses the *whole* command, never per-segment.
- First-match-wins rule resolution; primitives are streaming line transformers (ADR-0005). The fidelity fix stays within streaming (no buffering/reorder).
- Bundled rules ship with success+failure fixtures asserting ≥50% reduction with zero error-line drops; new/updated fixtures follow that triple structure.

### Integration Points
- Hook hot path: `detect_bypass` → chain-split → TUI-bypass → resolve → wrap. The bypass fix lands at the first step and short-circuits to `PassThrough`.
- Pipeline: `collapse_repeated` stage in `stages.rs`; its emission rules are the fabrication surface. `git-status.yaml` is the bundled rule wiring that stage to real tabular output.
</code_context>

<specifics>
## Specific Ideas

- The observed fabrication token was `table table table` — repeated-prefix/tabular loop output collapsed into a blending synthetic line. New fixtures should reproduce this class (aligned columns, repeated leading tokens, grep hits).
- `!!` is unusable from inside the agent's Bash tool (`command not found: !!`), so `LACON_DISABLE` is the *only* escape hatch the agent has — making the inline prefix reliable is the highest-leverage part of the bypass work.
</specifics>

<deferred>
## Deferred Ideas

- Per-segment bypass granularity in chains — explicitly out (v1 contract is whole-command; v2 backlog).
- Generalizing the elision-marker convention to a user-configurable format — not needed for this phase; fixed/namespaced marker suffices.

### Reviewed Todos (not folded)
None — the only todo (TODO.md) is the phase's originating source and is folded.
</deferred>
