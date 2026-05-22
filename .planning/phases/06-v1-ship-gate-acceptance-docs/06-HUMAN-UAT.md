---
status: partial
phase: 06-v1-ship-gate-acceptance-docs
source: [06-VERIFICATION.md]
started: 2026-05-22T10:12:00Z
updated: 2026-05-22T10:12:00Z
---

## Current Test

[awaiting human testing — first macOS CI run]

## Tests

### 1. macOS CI lane first run (SC1 macOS cold-start number + macOS lane hermeticity)
expected: Push to `main` (or open a PR) to trigger `.github/workflows/ci.yml`. Both `ubuntu-latest` and `macos-latest` lanes go green; the macOS `cold_start_probe` step emits a per-OS-labeled min-of-N wall-clock table row; no package-manager fetch step (brew/npm/pnpm/pip/apt) fires; the `tracker_open` criterion bench exits 0 on both lanes. Then fill the `_(CI macos-latest)_` cells in `docs/architecture.md` with the reported macOS numbers.
result: [pending]
why_human: The macOS lane has never executed — the dev machine is Linux-only. macOS wall-clock numbers and macOS-lane runtime behavior can only be confirmed by a real CI run. The macOS cold-start figure is soft-reported by design (D-09); no hard wall-clock assert will fire.

## Summary

total: 1
passed: 0
issues: 0
pending: 1
skipped: 0
blocked: 0

## Gaps
