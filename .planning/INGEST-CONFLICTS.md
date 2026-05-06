# Conflict Detection Report

Synthesizer pass over 24 classified docs (13 ADRs, 4 SPECs, 2 PRDs, 5 DOCs) on 2026-05-06.

Mode: `new` (greenfield `.planning/`). Precedence: ADR > SPEC > PRD > DOC. No existing locked decisions to compare against.

Cycle detection on the cross-ref graph: clean (max DFS depth observed: 2).

---

### BLOCKERS (0)

No blockers. No LOCKED-vs-LOCKED ADR contradiction was found. ADR 0013 (Filter via PreToolUse-rewritten subprocess wrapper, accepted 2026-05-05) is explicitly additive — its own "Relationship to prior ADRs" section enumerates ADRs 0001, 0005, 0006, 0008, 0010, 0011 and confirms each is unchanged in semantics.

No UNKNOWN-confidence-low classifications. All 24 docs were classified at high confidence except `docs/vision.md` at medium (PRD/DOC was the only ambiguity; the synthesizer treats it as PRD per the classifier's resolution and there are no acceptance criteria to merge).

No cross-ref cycles. The reference graph is hierarchical: ADR 0013 references many predecessors but no predecessor references 0013 in turn; ADRs 0003 and 0005 each reference ADR 0008 in one direction.

---

### WARNINGS (0)

No competing acceptance variants between the two PRD-class docs. `docs/v1-scope.md` and `docs/vision.md` overlap on cold-start budget (<10ms), local-only, Claude Code first, and acceptance criteria framing — and where they overlap they agree. `docs/vision.md` carries strategic targets (30–70% byte reduction, trust property, non-goals) that `docs/v1-scope.md` does not contradict; the synthesizer recorded those under "Vision-derived strategic targets" without merging.

No deferred-to-prototyping question contradicts a locked ADR. The three open items in `docs/open-questions.md` (signal forwarding in `lacon run`, `lacon init` idempotency, stdout/stderr merge ordering) each have a likely-answer and are explicitly deferred to implementation per the doc's own status section.

The "Open" subsection of `docs/open-questions.md` is empty as of 2026-05-06 — the doc itself states *"None currently. New design risks surfaced before or during implementation should be added here."*

---

### INFO (5)

[INFO] ADR 0013 narrows ADR 0001 scope without amending it
  Found: ADR 0001 (docs/decisions/0001-use-claude-code-hooks.md) commits to using both PreToolUse and PostToolUse hooks for Claude Code integration.
  Found: ADR 0013 (docs/decisions/0013-filter-via-pretooluse-wrapper.md, 2026-05-05) installs ONLY a PreToolUse hook in v1; PostToolUse is reserved for v1.5 unmatched-command annotation.
  Resolution: ADR 0013's "Relationship to prior ADRs" block explicitly states ADR 0001 is "still accepted. The integration narrows to PreToolUse only for filtering." This is additive narrowing, not contradiction. Both ADRs remain LOCKED. No precedence conflict. Synthesizer recorded both in decisions.md with the narrowing note attached to ADR 0001.

[INFO] Auto-resolved: ADR 0008 modulates ADR 0005 streaming model
  Found: ADR 0005 (docs/decisions/0005-streaming-first.md) requires native primitives to be streaming line-by-line transformers.
  Found: ADR 0008 (docs/decisions/0008-aggregated-starlark.md) requires Starlark stages to run on aggregated post-pipeline output, not per-line.
  Resolution: ADR 0005 itself names ADR 0008 as the explicit exception ("The Starlark post_process stage is an explicit exception — see ADR 0008 for the reasoning."). Cross-ref relationship is documented and intentional. Not a conflict; recorded in decisions.md.

[INFO] Auto-resolved: rule resolution vs config layering use the same vocabulary for different artifacts
  Found: ADR 0007 (docs/decisions/0007-first-match-wins.md) plus ADR 0004 (docs/decisions/0004-config-precedence.md) require first-match-wins with NO merging across rule layers.
  Found: docs/specs/config-schema.md requires per-key deep merge across config.yaml layers (bundled → user → project), with sub-objects merging recursively rather than wholesale.
  Resolution: Different artifacts. Rules (`rules/*.yaml`) use first-match-wins with `extends` for explicit inheritance. Config files (`config.yaml`) use per-key deep merge because the keys are independent scalars whose merge semantics are well-defined. The two policies coexist intentionally. Not a conflict; both surfaced in constraints.md with their distinct scopes.

[INFO] Resolved historical drift: tracking spec previously listed lacon purge subcommands
  Found: docs/open-questions.md "R-resolved-privacy-raw-outputs" notes that docs/specs/tracking-data-model.md previously documented `lacon purge` as if it shipped in v1, contradicting docs/v1-scope.md's six-command CLI surface.
  Resolution: The spec has been corrected to match the 6-command v1 surface and the manual cleanup path (`rm` on the DB file or direct `sqlite3 DELETE`). Current spec (verified during synthesis) no longer references `lacon purge` as v1. Recorded as historical resolution; no live conflict to surface.

[INFO] Resolved historical drift: tokenizer/token-accounting framing
  Found: docs/open-questions.md "R-resolved-tokenizer-choice" updates the tokenizer trade-off framing — Anthropic's tokenizer is no longer closed; it's reachable via Messages API `count_tokens` endpoint and via vendorable open packages.
  Resolution: Tracking schema is forward-compatible (existing counters are explicitly byte-named per CON-tracking-tokens-not-in-v1). Token columns can be appended via standard append-only migration when v2 picks the tokenizer. No v1 work required; backlog item updated. Recorded as INFO because future implementers reading the v1 specs may otherwise miss the framing update.

---

## Synthesizer notes

- Cycle detection: ran DFS three-color marking on the cross_refs graph. No back-edges. Max depth 2 (ADR 0013 → ADR 0008 via ADR 0005 → ADR 0008).
- Precedence application: all 13 ADRs hold default precedence 1 (LOCKED); 4 SPECs at 2; 2 PRDs at 3; 5 DOCs at 4. No `precedence` field overrides in any classification JSON.
- Open-questions handling: the 3 "Deferred to prototyping" items were treated as informational design-risk log entries (status preserved) and NOT as decisions. They appear in `context.md` under the open-questions topic with their likely-answers attached.
- Vision-doc handling: the medium-confidence PRD classification was honored; strategic targets without testable acceptance criteria (e.g. "30–70% byte reduction without measurable loss in assistant quality") were placed under a "Vision-derived strategic targets" section in `requirements.md` rather than minted as REQ-* IDs.
