---
phase: 05-bundled-tier-1-rules
plan: 06
subsystem: testing
tags: [docker, buildkit, bundled-rules, yaml, fixtures, drop_regex, keep_around_match, on_error]

# Dependency graph
requires:
  - phase: 05-bundled-tier-1-rules (plan 05-01)
    provides: bundled_rules.rs fixture-walking test runner + meta.yaml exit_code field
  - phase: 01 (engine)
    provides: RuleLoader::resolve, Runner::filter_bytes, native primitives, on_error branch (ADR-0010), MaxBytes auto-injection
provides:
  - "bundled-rules/docker-build.yaml — docker build / docker buildx build rule"
  - "cached-rebuild success fixture (deterministic ≥50% reduction via dropping BuildKit progress noise)"
  - "build-step-error failure fixture (on_error preserves framed ERROR block + Dockerfile excerpt)"
affects: [05-08-roadmap-notes, phase-06-ship-gate]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Anchored ^#N drop_regex blacklist for BuildKit progress lines"
    - "Dropping run-varying lines (sha256/DONE-timing) to make a derived expected.txt byte-deterministic (Pitfall 6)"
    - "keep_around_match on ERROR (before:2, after:15) to capture a framed multi-line build error"

key-files:
  created:
    - bundled-rules/docker-build.yaml
    - tests/fixtures/docker-build/cached-rebuild/input.txt
    - tests/fixtures/docker-build/cached-rebuild/expected.txt
    - tests/fixtures/docker-build/cached-rebuild/meta.yaml
    - tests/fixtures/docker-build/build-step-error/input.txt
    - tests/fixtures/docker-build/build-step-error/expected.txt
    - tests/fixtures/docker-build/build-step-error/meta.yaml
  modified: []

key-decisions:
  - "Kept the volatile lines IN input.txt (real capture per D-03) and let the rule drop them — determinism comes from the drop, not from hand-scrubbing input"
  - "match.any covers `docker build` and `docker buildx build` (D-10); both emit identical #N BuildKit format"
  - "Failure fixture marked exempt_from_reduction_check (the error IS the signal) with must_keep_lines asserting error survival"

patterns-established:
  - "Pattern 1: BuildKit progress drop via anchored ^#\\d+ <verb> drop_regex stages; step headers (#N [k/m]) and RUN echo (#N <secs>) preserved"
  - "Pattern 2: on_error keep_around_match on ERROR yields the framed ------ block + -------------------- Dockerfile excerpt without look-around (RE2-safe)"

requirements-completed: [REQ-bundled-rules-tier1, REQ-bundled-rules-format]

# Metrics
duration: 12min
completed: 2026-05-22
---

# Phase 5 Plan 6: docker-build Bundled Rule Summary

**docker-build rule that strips BuildKit progress noise (CACHED/DONE/sha256/transferring/exporting incl. run-varying byte-count lines) for a deterministic ≥50% reduction, and preserves the framed RUN-step error via on_error.**

## Performance

- **Duration:** ~12 min
- **Started:** 2026-05-22
- **Completed:** 2026-05-22
- **Tasks:** 2
- **Files created:** 7

## Accomplishments
- `bundled-rules/docker-build.yaml`: matches `docker build` + `docker buildx build` (D-10); success pipeline drops the eight BuildKit progress line classes, keeping `#N [k/m] ...` step headers and `#N <secs> ...` RUN echo output. No hand-placed `max_bytes` (D-07), no Starlark, no look-around regex (RE2-safe).
- `cached-rebuild` success fixture: real Docker 29.5.1 BuildKit rebuild of a 7-step Dockerfile (all layers CACHED). Reduction ratio **0.4603** (≤0.5), byte-stable across re-runs.
- `build-step-error` failure fixture: real `RUN exit 17` failure (exit 1) routed through `on_error`; `keep_around_match` on `ERROR` preserves the `------` framed block, the `> [3/4]` step pointer, the `--------------------` Dockerfile excerpt with `>>>`, and `ERROR: failed to build`.
- `cargo test --test bundled_rules` asserts 2 docker-build fixtures green; full workspace `cargo build` and `lacon validate` both clean.

## Task Commits

1. **Task 1: Author docker-build.yaml** - `de8e2f9` (feat)
2. **Task 2: docker-build fixtures (cached-rebuild + build-step-error)** - `f09fca3` (test)

## Files Created/Modified
- `bundled-rules/docker-build.yaml` - The rule: match.any (docker build / buildx build), success drop_regex pipeline, on_error keep_around_match pipeline
- `tests/fixtures/docker-build/cached-rebuild/{input,expected,meta}.yaml/txt` - Success scenario (exit 0, reduction 0.46, deterministic)
- `tests/fixtures/docker-build/build-step-error/{input,expected,meta}.yaml/txt` - Failure scenario (exit 1, exempt, must_keep_lines on ERROR + exit code: 17)

## Decisions Made
- **Real input, rule-side determinism:** input.txt retains the genuine captured BuildKit output (volatile sha256/DONE-timing lines included) per D-03; the rule's drop_regex stages remove those lines so the derived expected.txt is byte-stable. Verified by re-running the pipeline twice and diffing — identical output.
- **Anchored drop patterns only:** the FROM step header (`#4 [1/7] FROM ...@sha256:<digest>`) is intentionally KEPT — the rule's `^#\d+ sha256:` pattern is anchored to byte-progress lines and does not match the step header. The pinned image digest is content-addressed and stable, so determinism is unaffected.
- **`#N naming to` / `#N unpacking to` survive** the success pipeline (no byte counts / timings on those lines), which is acceptable signal; they are not in any drop pattern and are deterministic.

## Deviations from Plan

None - plan executed exactly as written. All success criteria and acceptance criteria met without auto-fixes.

## Issues Encountered
- A `grep 'sha256'` smoke check produced a false positive on the kept FROM step header line. Confirmed via anchored `grep -E '^#[0-9]+ sha256:'` that no byte-progress line survives and the pipeline is byte-deterministic across re-runs. No code change needed.

## Threat Model Compliance
- **T-5-03 (info disclosure — on_error dropping the build error):** mitigated. `build-step-error/meta.yaml` `must_keep_lines` asserts `ERROR: failed to build` and `exit code: 17` survive filtering; the bundled_rules runner enforces it (green).
- **T-5-02 (ReDoS):** accept — all patterns are anchored RE2 (linear-time); no backtracking possible.
- **T-5-SC (scratch base-image pull):** the alpine:3.20 pull and throwaway Dockerfiles/images were deleted after capture; nothing entered the repo.

## Next Phase Readiness
- docker-build is the 10th/last Tier 1 rule shape (BuildKit streaming + nondeterministic-progress handling); ready for the phase-06 ship gate.
- No blockers. Roadmap note for docker-build (REQ-bundled-rules-format doc note) is handled by the roadmap-notes plan, not this one.

## Self-Check: PASSED

- FOUND: bundled-rules/docker-build.yaml
- FOUND: tests/fixtures/docker-build/cached-rebuild/input.txt
- FOUND: tests/fixtures/docker-build/cached-rebuild/expected.txt
- FOUND: tests/fixtures/docker-build/cached-rebuild/meta.yaml
- FOUND: tests/fixtures/docker-build/build-step-error/input.txt
- FOUND: tests/fixtures/docker-build/build-step-error/expected.txt
- FOUND: tests/fixtures/docker-build/build-step-error/meta.yaml
- FOUND commit: de8e2f9 (Task 1, feat)
- FOUND commit: f09fca3 (Task 2, test)
- cargo build: green; lacon validate: exit 0; cargo test --test bundled_rules: 2 fixtures asserted, 1 passed

---
*Phase: 05-bundled-tier-1-rules*
*Completed: 2026-05-22*
