---
milestone: v1.0
audited: 2026-05-22T19:55:00+02:00
status: gaps_found
scores:
  requirements: 33/36 fully satisfied (1 unsatisfied, 2 partial — single root cause)
  phases: 6/6 phase-verified
  integration: 5/6 seams wired (1 partial)
  flows: 1/1 headline E2E flow broken (at explain capture step)
gaps:
  requirements:
    - id: "REQ-acceptance-explain-reproducibility"
      status: "unsatisfied"
      phase: "Phase 6 (root cause spans Phase 2 + Phase 4)"
      claimed_by_plans: ["06-01-PLAN.md"]
      completed_by_plans: []   # 06-01-SUMMARY frontmatter lists no requirements; acceptance test seeds DB manually
      verification_status: "passed (masked)"
      evidence: >
        Acceptance test `explain_filtered_column_byte_equals_run_output`
        (crates/lacon-cli/tests/cli_explain.rs:217) proves the REPLAY logic by
        seeding `raw_outputs` with a direct `INSERT INTO raw_outputs` SQL statement,
        then running `lacon explain`. It never drives `lacon run`. The full
        acceptance chain (lacon run → capture pre-filter bytes → raw_outputs INSERT
        → lacon explain replay) is not exercised and does not work: run.rs:275
        hard-codes `raw=None` ("v1 default: no raw output bytes captured"), so no
        live invocation ever populates raw_outputs. The criterion the requirement
        states — reproduce filtering "for any tracked invocation that has stored
        raw output" — is vacuous because no real invocation can have stored raw
        output.
    - id: "REQ-cli-explain"
      status: "partial"
      phase: "Phase 4"
      claimed_by_plans: ["04-01-PLAN.md", "04-02-PLAN.md", "04-03-PLAN.md"]
      completed_by_plans: ["04-01-SUMMARY.md", "04-03-SUMMARY.md"]
      verification_status: "passed (read-side only)"
      evidence: >
        `explain.rs` correctly implements the read/replay side (open_readonly →
        fetch_invocation → fetch_raw_output → Runner::filter_bytes → two-column
        diff) and is well tested. But because the capture path is absent, every
        real `lacon explain <id>` returns the "no stored raw output
        (store_raw_outputs was disabled)" error — even when store_raw_outputs:true
        is set. The command is non-functional on real data. Phase 2 SUMMARY
        explicitly deferred raw capture to "Phase 4's lacon explain work"; Phase 4
        built the reader, not the capturer. The handoff fell through the seam.
    - id: "REQ-tracking-raw-outputs-default-off"
      status: "partial"
      phase: "Phase 2"
      claimed_by_plans: ["02-03-PLAN.md", "02-05-PLAN.md"]
      completed_by_plans: ["02-05-SUMMARY.md"]
      verification_status: "passed (default-off half only)"
      evidence: >
        The "off by default" half is fully satisfied and tested. The "opt-in"
        half is non-functional: flipping store_raw_outputs:true fires the privacy
        warning (record.rs:62) but stores nothing, because run.rs always passes
        raw=None and record.rs:81 only inserts when (true, Some(raw)). The spec
        (docs/specs/tracking-data-model.md:92,151) — part of the contract — says
        raw_outputs stores "the original stdout/stderr" when enabled and is
        "mainly useful for recent lacon explain calls." Opting in produces an
        empty table.
  integration:
    - seam: "Seam 6 — end-to-end: lacon run → capture → raw_outputs → lacon explain"
      status: "partial / broken at capture"
      detail: >
        5 of 6 sub-steps wired (init→hook→run→filter→stdout→track all work). The
        capture step is structurally absent: RunOutcome (runtime/mod.rs:71) carries
        only byte COUNTS (raw_byte_counter), never the raw bytes. The internal
        raw_buffer exists for the on_error/post_process path but is never threaded
        onto RunOutcome, so run.rs has nothing to pass to tracker.record().
  flows:
    - flow: "lacon explain <id> on a real invocation"
      breaks_at: "raw capture in lacon run (run.rs:275 raw=None; RunOutcome lacks raw bytes)"
tech_debt:
  - phase: 03-claude-code-adapter-lacon-init
    items:
      - "LACON_TOOL_USE_ID is emitted in every wrapped command (lib.rs:214) but never consumed by run.rs and has no column in InvocationMeta/invocations. Correlation limited to session_id + ts. Documented deferral to v1.5+, harmless (subprocess inherits env)."
  - phase: 02-local-tracking
    items:
      - "Tracker::open first-ever-run cold-start trips the 3700µs criterion gate at ~25020µs on ext4 (migration COMMIT fsync). Accepted & documented: Phase 6 split the gate to steady-state (~210µs, passes); lazy-open keeps --version/validate/doctor off this path; only the first lacon run per machine pays it. Phase-6 follow-up to re-measure on tmpfs was logged but not done."
  - phase: documentation-integrity
    items:
      - "ROADMAP.md is stale for Phase 2: line 14 shows '[ ]' (unchecked) and the progress table (line 140) shows '4/6 / In Progress', but Phase 2 is fully complete (6/6 SUMMARYs, 02-VERIFICATION passed 28/28, all 5 reqs Complete in REQUIREMENTS.md). The ROADMAP was never updated when Phase 2 closed."
      - "7 requirements are absent from every plan's SUMMARY `requirements_completed` frontmatter (REQ-engine-rule-loading, REQ-engine-extends, REQ-engine-rewrite; plus 06-01's four acceptance reqs whose SUMMARY frontmatter is empty). All are backed by explicit VERIFICATION.md evidence, so they are satisfied — but the 3-source traceability is incomplete."
nyquist:
  compliant_phases: [1, 6]
  partial_phases: [3, 4, 5]   # VALIDATION.md exists but status: draft, nyquist_compliant: false
  missing_phases: [2]          # no VALIDATION.md
  overall: "partial — 2 compliant, 3 draft/non-compliant, 1 missing (discovery only)"
---

# Milestone v1.0 — Audit Report

**Audited:** 2026-05-22
**Status:** ⚠ `gaps_found`
**Milestone:** lacon v1.0 — "Reduce the bytes an AI coding assistant ingests from bash output by 30–70% without dropping signal."

All six phases passed their own VERIFICATION.md gates, the workspace builds, and
448 tests pass hermetically. But a cross-phase integration audit surfaces **one
root-cause gap** — the raw-output capture path was never wired through the live
`lacon run` invocation — that breaks the `lacon explain` feature end-to-end and
violates the tracking-data-model spec. Because the spec is part of the v1
contract (per CLAUDE.md) and an acceptance criterion is involved, this forces
`gaps_found` per the FAIL gate.

---

## Headline finding (BLOCKER)

**`lacon explain` is non-functional on real invocations. Raw output is never captured, even when the user opts in.**

| Layer | What's there | What's missing |
|-------|--------------|----------------|
| Phase 2 schema | `raw_outputs` table, FK, retention, privacy warning | — |
| Phase 2 record | `Tracker::record(meta, raw_opt, …)` inserts raw when `(store_raw=true, raw=Some)` | nothing calls it with `Some` |
| Phase 1 runner | counts raw bytes (`raw_byte_counter`); holds `raw_buffer` internally | never exposes raw bytes on `RunOutcome` |
| Phase 1 run.rs | `record_invocation(...)` wired, best-effort | `run.rs:275` hard-codes `raw=None` |
| Phase 4 explain | read/replay logic correct & tested | always hits the "no stored raw output" branch on real data |
| Phase 6 acceptance | byte-equality replay test passes | it **seeds the DB with raw SQL**, bypassing the capture path |

**Root cause:** Phase 2's SUMMARY deferred raw capture to "Phase 4's `lacon explain`
work." Phase 4 implemented the *reader* (`explain.rs`, `Runner::filter_bytes`,
`query::fetch_raw_output`) but not the *capturer*. Phase 6's acceptance test
validated replay against a hand-seeded row, so the missing capture step never
surfaced. The work fell through the seam between three phases — exactly what a
cross-phase audit exists to catch.

**Remediation is modest, not architectural.** The runner already buffers raw
lines (`raw_buffer` in `runtime/mod.rs`, needed by the `on_error`/`post_process`
paths). Closing the gap means: when `store_raw_outputs` is enabled, expose those
bytes on `RunOutcome`, and have `run.rs` pass `Some(RawOutput)` to
`tracker.record()` instead of the hard-coded `None`. Then add an E2E test that
drives `lacon run` (not a seeded INSERT) and asserts `lacon explain` reproduces.

**Spec evidence (contract):**
- `docs/specs/tracking-data-model.md:92` — *"When raw output retention is enabled, [`raw_output_id`] points to the row in `raw_outputs` storing the original stdout/stderr."*
- `docs/specs/tracking-data-model.md:151` — *"`raw_outputs` … Bulky; mainly useful for recent `lacon explain` calls."*
- `docs/backlog.md` defers only redaction, `lacon purge`, and encryption-at-rest — **all of which presuppose capture works.** Capture itself is not a backlog item.

---

## Requirements coverage (3-source cross-reference)

**33/36 fully satisfied · 1 unsatisfied · 2 partial.** The 3 impacted requirements
share the single root cause above.

| Source | Result |
|--------|--------|
| REQUIREMENTS.md traceability | 36/36 mapped, 0 orphans, all marked Complete |
| Phase VERIFICATION.md tables | 36/36 marked SATISFIED (but explain-reproducibility verified via seeded data, not live capture) |
| SUMMARY `requirements_completed` frontmatter | 29/36 enumerated; 7 absent (see below) but VERIFICATION-backed |

**Orphan detection:** none — every REQ-ID appears in at least one VERIFICATION.md.

**Frontmatter traceability gaps (not implementation gaps):** `REQ-engine-rule-loading`,
`REQ-engine-extends`, `REQ-engine-rewrite` (Phase 1) and the four `REQ-acceptance-*`
reqs in plan 06-01 are absent from any SUMMARY `requirements_completed` list. All
are backed by explicit VERIFICATION.md evidence and manual code inspection →
treated as satisfied; the 3-source chain is merely incomplete on the SUMMARY side.

---

## Cross-phase integration (5/6 seams wired)

| # | Seam | Status |
|---|------|--------|
| 1 | Adapter hook → `lacon run` wrapper (`--rule … -- <cmd>` contract) | ✅ WIRED |
| 2 | `lacon run` → Tracker (best-effort, lazy-open invariant holds) | ✅ WIRED (but passes `raw=None`) |
| 3 | Tracker → `stats`/`explain` (read-only views, column match) | ✅ WIRED |
| 4 | Bundled rules → engine (`include_dir` embed, `extends: bundled/test-base`) | ✅ WIRED |
| 5 | `lacon init` → hook → `doctor` (`lacon-claude-hook` fingerprint matches) | ✅ WIRED |
| 6 | E2E: init → hook → run → filter → track → **explain** | ⚠ BROKEN at capture |

---

## Nyquist coverage (discovery only)

| Phase | VALIDATION.md | `nyquist_compliant` | Action |
|-------|---------------|---------------------|--------|
| 1 | exists (`revised`) | true | — compliant |
| 2 | **missing** | — | `/gsd:validate-phase 2` |
| 3 | exists (`draft`) | false | `/gsd:validate-phase 3` |
| 4 | exists (`draft`) | false | `/gsd:validate-phase 4` |
| 5 | exists (`draft`) | false | `/gsd:validate-phase 5` |
| 6 | exists (`ready`) | true | — compliant |

Phases 1 and 6 are Nyquist-compliant. Phases 3–5 have draft VALIDATION.md files
left at `nyquist_compliant: false`; Phase 2 has none. Given the phases all carry
heavy test coverage and passed verification, these are most likely formal
gaps in the validation framework rather than true coverage holes — but
`/gsd:validate-phase <N>` should confirm before archiving.

---

## Tech debt (non-blocking)

1. **`LACON_TOOL_USE_ID` emitted but unused** (Phase 3) — set in every wrapped command, no consumer, no column. Documented v1.5 deferral. Harmless.
2. **First-run cold-start gate trip** (Phase 2/6) — `Tracker::open` ~25ms on first-ever `lacon run` per machine (ext4 migration fsync). Steady-state gate (~210µs) passes; hot path unaffected by lazy-open. Accepted; tmpfs re-measure was logged but not done.
3. **ROADMAP.md stale for Phase 2** — shows `[ ]` / "4/6 / In Progress" though Phase 2 is fully complete and verified. Fix the checkbox and progress table.
4. **Incomplete SUMMARY traceability** — 7 requirements not enumerated in plan frontmatter (see above).

---

## What's solid

- All 10 engine primitives, Starlark `post_process`, rule loader, `extends`, `on_error`, `max_bytes` cap — wired and fixture-tested.
- SQLite tracking: schema, 4 views, WAL, 0700, retention/prune, privacy warning, lazy-open invariant — all verified.
- Claude Code adapter: PreToolUse rewrite, 13-scenario chain splitter, TUI bypass, `lacon init` idempotency — verified, with the CR-01 shell-injection fix in place.
- 6-command CLI surface cap enforced; `stats`/`doctor` functional.
- 10 bundled rules + 20 fixtures, ≥50% reduction asserted, hermetic.
- Hermetic dual-OS CI (ubuntu + macos), both lanes green, cold-start probe emits per-OS tables.
- README + worked example + primitive reference shipped.

The milestone is one focused fix away from a clean pass: capture raw bytes on
opt-in and prove `lacon explain` end-to-end.
