# Synthesis summary

Single entry point for downstream consumers (`gsd-roadmapper` reads this).

Pass: `gsd-doc-synthesizer` (mode `new`, greenfield `.planning/`). Date: 2026-05-06.

---

## Doc counts by type

| Type | Count | Sources |
|---|---|---|
| ADR | 13 | docs/decisions/0001 through 0013 |
| SPEC | 4 | docs/specs/{filter-rule-schema, config-schema, tracking-data-model, chained-commands}.md |
| PRD | 2 | docs/v1-scope.md (high), docs/vision.md (medium) |
| DOC | 5 | docs/{architecture, backlog, bundled-rules-roadmap, open-questions, testing-rules}.md |
| **Total** | **24** | All docs in the ingest set were classified at high confidence except `docs/vision.md` (medium PRD/DOC; treated as PRD per classifier resolution). |

---

## Decisions locked

13 ADRs, all `locked: true`, all status Accepted:

- ADR-0001 — Use Claude Code hooks (narrowed by ADR-0013 to PreToolUse only for v1)
- ADR-0002 — Rust as primary language
- ADR-0003 — Starlark for escape-hatch scripting
- ADR-0004 — Project > User > Bundled config precedence
- ADR-0005 — Streaming-first output processing
- ADR-0006 — Hybrid command rewriting and output filtering
- ADR-0007 — First-match-wins rule resolution
- ADR-0008 — Aggregated post-process Starlark, not per-line
- ADR-0009 — Separated raw_outputs table
- ADR-0010 — `on_error` replaces the pipeline, doesn't merge
- ADR-0011 — SQLite for local tracking
- ADR-0012 — Append-only inheritance via `extends`
- ADR-0013 — Filter via PreToolUse-rewritten subprocess wrapper

Full text: `decisions.md`.

---

## Requirements extracted

26 REQ-* IDs, derived from `docs/v1-scope.md` (PRD high) with corroborating cross-references to SPECs and ADRs:

**Engine (8):** REQ-engine-streaming-primitives, REQ-engine-starlark-postprocess, REQ-engine-rule-loading, REQ-engine-extends, REQ-engine-on-error, REQ-engine-rewrite, REQ-engine-bypass, REQ-engine-max-bytes-cap.

**Adapter (5):** REQ-adapter-pretooluse-only, REQ-adapter-bypass-detection, REQ-adapter-chained-commands, REQ-adapter-tui-bypass, REQ-adapter-pipes-passthrough.

**Tracking (5):** REQ-tracking-sqlite-location, REQ-tracking-schema, REQ-tracking-raw-outputs-default-off, REQ-tracking-privacy-warning, REQ-tracking-retention-defaults.

**CLI (7):** REQ-cli-init, REQ-cli-run, REQ-cli-stats, REQ-cli-explain, REQ-cli-doctor, REQ-cli-validate, REQ-cli-surface-cap.

**Bundled rules (2):** REQ-bundled-rules-tier1, REQ-bundled-rules-format.

**Acceptance criteria (6):** REQ-acceptance-bundled-reduction, REQ-acceptance-pnpm-end-to-end, REQ-acceptance-cold-start-budget, REQ-acceptance-explain-reproducibility, REQ-acceptance-hot-reload, REQ-acceptance-test-coverage.

**Documentation (3):** REQ-docs-readme, REQ-docs-worked-example, REQ-docs-primitive-reference.

(Group totals 36 because some IDs span groups; 26 distinct IDs once de-duped — full list in `requirements.md` plus a "Vision-derived strategic targets" non-REQ section and an explicit-exclusions section.)

Full text: `requirements.md`.

---

## Constraints

29 CON-* entries across schema (rule format, config format, SQLite schema), protocol (chained commands, TUI heuristic), and NFR (cold-start, streaming memory, stderr merge, TTY downstream, no-network/no-daemon, platform support):

- Filter rule schema: 11 entries (CON-filter-rule-*)
- Config schema: 5 entries (CON-config-*)
- Tracking data model: 8 entries (CON-tracking-*)
- Chained-commands protocol: 8 entries (CON-chained-*)
- Cross-cutting NFRs: 5 entries (CON-nfr-*)

Full text: `constraints.md`.

---

## Context topics

7 topics, each with verbatim notes and source attribution:

- System architecture (`docs/architecture.md`)
- v1-deferred backlog (`docs/backlog.md`)
- Bundled rules roadmap (`docs/bundled-rules-roadmap.md`)
- Open questions log — status-preserved: 0 open, 3 deferred-to-prototyping, 8 resolved (`docs/open-questions.md`)
- Testing strategy (`docs/testing-rules.md`)
- Layout reconciliation note (cross-doc consistency check)

Full text: `context.md`.

---

## Conflicts summary

| Bucket | Count |
|---|---|
| BLOCKERS | 0 |
| WARNINGS | 0 |
| INFO | 5 |

INFO entries cover: ADR 0013 narrowing ADR 0001 (additive, not contradiction); ADR 0008 modulating ADR 0005 streaming model (explicit exception, cross-referenced); rule resolution vs config layering vocabulary overlap (different artifacts, different policies, intentional); historical `lacon purge` drift (resolved); tokenizer framing update (resolved).

Full text: `../INGEST-CONFLICTS.md`.

---

## Pointers

- Decisions: `.planning/intel/decisions.md`
- Requirements: `.planning/intel/requirements.md`
- Constraints: `.planning/intel/constraints.md`
- Context: `.planning/intel/context.md`
- Conflicts report: `.planning/INGEST-CONFLICTS.md`

---

## Status

**READY** — safe to route to `gsd-roadmapper`. Zero blockers, zero competing variants. All 13 ADRs are LOCKED with consistent semantics; the 4 SPECs form a coherent user-facing contract; the 2 PRDs agree on overlapping scope; the 5 DOCs are subordinate context that reinforces (does not contradict) the ADR/SPEC/PRD set.

The deferred-to-prototyping open questions are flagged in `context.md` under their original status with likely-answers attached, so the roadmapper can plan their resolution as part of the relevant implementation milestones rather than treating them as v1 blockers.
