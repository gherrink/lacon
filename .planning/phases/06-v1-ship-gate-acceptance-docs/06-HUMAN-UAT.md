---
status: complete
phase: 06-v1-ship-gate-acceptance-docs
source: [06-VERIFICATION.md]
started: 2026-05-22T10:12:00Z
updated: 2026-05-22T17:50:00Z
---

## Current Test

[testing complete]

## Tests

### 1. macOS CI lane first run (SC1 macOS cold-start number + macOS lane hermeticity)
expected: Push to `main` (or open a PR) to trigger `.github/workflows/ci.yml`. Both `ubuntu-latest` and `macos-latest` lanes go green; the macOS `cold_start_probe` step emits a per-OS-labeled min-of-N wall-clock table row; no package-manager fetch step (brew/npm/pnpm/pip/apt) fires; the `tracker_open` criterion bench exits 0 on both lanes. Then fill the `_(CI macos-latest)_` cells in `docs/architecture.md` with the reported macOS numbers.
result: pass
evidence: First macos-latest + ubuntu-latest CI run on 2026-05-22. cold_start_probe emitted per-OS tables on both lanes ("(macos, 50 samples per scenario)" / "(linux, ...)"). macOS min/median: --version 1953/2009µs, validate 2094/2172µs, hook passthrough ~11/~11.9ms, hook rewrite ~11/~11.1ms. Hook wall-clock is spawn-dominated soft-report (non-gated per architecture.md footnote). tracker_open hard gate runs before cold-start in ci.yml, so both lanes reaching cold-start proves it exited 0 on both. No package-manager fetch step exists in the workflow (hermetic by construction). docs/architecture.md:197-200 cells filled. User confirmed pass.

## Summary

total: 1
passed: 1
issues: 0
pending: 0
skipped: 0
blocked: 0

## Gaps
