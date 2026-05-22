---
phase: 06-v1-ship-gate-acceptance-docs
plan: 03
subsystem: docs
tags: [documentation, readme, filter-rule-schema, primitives, fixtures, quickstart]

# Dependency graph
requires:
  - phase: 01-engine-core-lacon-run-wrapper
    provides: the ten native primitives + golden fixtures (tests/fixtures/primitives) that the primitive reference is verified against
  - phase: 03-claude-code-adapter
    provides: lacon init + lacon-claude-hook PreToolUse wiring described in the README quickstart
  - phase: 05-bundled-rules
    provides: bundled/pkg-install rule that the worked example extends
provides:
  - "README.md rewritten from a design-status stub into install + quickstart, linking the two new docs"
  - "docs/worked-example.md — end-to-end walkthrough for writing a project-specific filter rule (our-monorepo-pnpm extends bundled/pkg-install)"
  - "docs/primitive-reference.md — one fixture-verified input→output example for each of the ten native primitives"
affects: [06-v1-ship-gate-acceptance-docs, ship-gate, SC5]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Doc examples extracted from the canonical schema spec and verified byte-for-byte against golden fixtures (drift-prevention, Pitfall 5)"

key-files:
  created:
    - docs/primitive-reference.md
    - docs/worked-example.md
  modified:
    - README.md

key-decisions:
  - "Primitive-reference examples mirror the exact fixture configs from crates/lacon-core/tests/primitives.rs (keep_around_match after:15, max_bytes cap:200 → 510 bytes dropped, collapse summary '… 199 progress lines'), not the schema spec's illustrative arg values, so the doc is literally the tested behavior."
  - "README documents only the locked six-command surface and v1 platform scope (macOS + Linux); no out-of-v1 promises (no Windows, registry, or purge)."

patterns-established:
  - "Drift-prevention docs: source content from the contract (filter-rule-schema.md) and verify examples against tested golden fixtures rather than green-fielding."

requirements-completed: [REQ-docs-readme, REQ-docs-worked-example, REQ-docs-primitive-reference]

# Metrics
duration: 7min
completed: 2026-05-22
---

# Phase 6 Plan 03: v1 user-facing docs Summary

**Shipped the three v1 docs (SC5): a rewritten install + quickstart README, a project-specific filter-rule worked example extending bundled/pkg-install, and a primitive reference with one fixture-verified example per native primitive — all cross-linked and sourced from the schema contract and golden fixtures.**

## Performance

- **Duration:** ~7 min
- **Started:** 2026-05-22T09:36Z (approx)
- **Completed:** 2026-05-22T09:43Z
- **Tasks:** 3
- **Files modified:** 3 (2 created, 1 rewritten)

## Accomplishments

- `docs/primitive-reference.md`: a section for each of the ten native primitives (strip_ansi, drop_regex, keep_regex, replace_regex, dedupe, collapse_repeated, keep_head, keep_tail, keep_around_match, max_bytes), each with a one-line behavior summary, the YAML config form, and one worked input→output example taken byte-for-byte from `tests/fixtures/primitives/<name>/{input.txt,expected.txt}`. Reproduces the load-bearing semantics exactly: keep_regex whitelist/OR, dedupe max_kept default 1, collapse_repeated `{count}` placeholder, keep_around_match grep -B/-A windowing, and the `[lacon: truncated, N more bytes dropped]` marker as a must-be-last stage.
- `docs/worked-example.md`: an end-to-end walkthrough for writing `.lacon/rules/our-monorepo-pnpm.yaml` (extends `bundled/pkg-install` + two `drop_regex` stages), preserving the three explanatory bullets (inherits match/rewrite/on_error; prepends the parent pipeline; project rules win resolution) consistent with ADR-0012 / ADR-0007, plus closing pointers to `lacon validate` and `lacon explain`. Explicitly notes no remove/reorder/insert ops (out of v1 scope).
- `README.md`: removed the "No installable artifact yet" design stub; added an Install section (`cargo build --release` → `lacon` + `lacon-claude-hook`, macOS + Linux) and a Quickstart (`lacon init` → hook wiring → auto-wrapped `lacon run` → `lacon doctor`), a six-command surface table, and extended the Documentation section with relative links to both new docs while keeping the prior links and License.

## Task Commits

Each task was committed atomically:

1. **Task 1: Author docs/primitive-reference.md (fixture-verified)** - `8edbd01` (docs)
2. **Task 2: Author docs/worked-example.md** - `1c1ce86` (docs)
3. **Task 3: Rewrite README into install + quickstart + links** - `33af93d` (docs)

_Plan metadata (SUMMARY) committed separately in worktree mode._

## Files Created/Modified

- `docs/primitive-reference.md` (created) - one fixture-verified worked example for each of the ten native primitives, with a streaming-model intro (ADR-0005) and a link to the schema spec as the contract.
- `docs/worked-example.md` (created) - project-specific filter-rule walkthrough derived from filter-rule-schema.md:213-233, with the three inheritance/precedence bullets preserved.
- `README.md` (modified) - install + quickstart + Documentation links to the two new docs; design-status stub removed.

## Decisions Made

- **Fixture configs over schema-illustrative args:** where the schema spec's example arg values differ from the actual tested fixture config (e.g. `keep_around_match` uses `after: 20` in the spec illustration but `after: 15` in the fixture/test, and the test caps `max_bytes` at 200 producing a 510-byte-dropped marker), the primitive reference uses the *fixture/test* values so each example is the literal tested behavior. This is the strongest form of the drift-prevention mandate.
- **README scoped to the six-command surface and v1 platforms only** — no Windows / registry / purge promises, per the threat model's T-06-DOC-02 (accept-low) constraint.

## Deviations from Plan

None - plan executed exactly as written. All three task verifications passed on first run, and the overall verification (all ten primitives present, truncation marker present, worked-example with our-monorepo-pnpm, stub removed, both README links present, fixtures cited) passed.

## Issues Encountered

- The project-root `CLAUDE.md` still reads "Design phase. No code yet.", but PROJECT.md, STATE.md, the RESEARCH/PATTERNS docs, and the on-disk Rust workspace (with shipped fixtures and bundled rules) confirm Phases 1–5 are complete. The plan and accumulated state are authoritative for this docs work; the stale CLAUDE.md status line was not in scope for this plan and was left untouched (logged here for transparency — a CLAUDE.md status refresh is a separate concern).

## Threat Flags

None — this plan ships Markdown only: no runtime surface, no network, no dependencies. The only risk (documentation drift, T-06-DOC-01) is mitigated by sourcing from the contract and verifying examples against the golden fixtures.

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- SC5 of the Phase 6 ship gate is satisfied: README (install + quickstart), worked example, and primitive reference all ship and link from the project root.
- Ran fully parallel to Plans 01 and 02 (pure docs, no code dependency). No blockers introduced.

---
*Phase: 06-v1-ship-gate-acceptance-docs*
*Completed: 2026-05-22*
