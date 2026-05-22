#!/usr/bin/env bash
#
# bench-cold-start.sh — the reproducible cold-start benchmark entry point for
# REQ-acceptance-cold-start-budget (Phase 6 ship gate, D-04).
#
# This is a THIN WRAPPER around the existing `cold_start_probe` binary
# (benches/cold_start.rs) — it authors NO new timing harness. It:
#   1. builds BOTH release binaries (`lacon` and `lacon-claude-hook`) — the
#      probe ERRORS without `target/release/lacon` and SKIPS the hook
#      scenarios without `target/release/lacon-claude-hook`;
#   2. runs `cargo run --release --bin cold_start_probe`, which exercises the
#      `lacon run` HOOK HOT PATH (passthrough + rewrite hook scenarios that
#      touch `Tracker::open`), not just the lazy-open `--version`/`validate`
#      paths;
#   3. echoes the probe's per-OS-labeled markdown table to stdout (the probe
#      labels output with `std::env::consts::OS`) so it can be pasted into
#      `docs/architecture.md`'s "Cold-start measurements" table.
#
# The headline statistic is min-of-N (the probe discards 3 warm-ups and reports
# min/median/p95/max). On macOS the min-of-N is reported as a SOFT gate; the
# deterministic HARD regression gate is the in-process `tracker_open` criterion
# bench (`cargo bench -p lacon-core --bench tracker_open`), not this script.
#
# Usage:
#   ./scripts/bench-cold-start.sh
#
# Run from anywhere — the script cd's to the repository root so the probe's
# relative `target/release/...` paths resolve.

set -euo pipefail

# Resolve the repository root (the workspace root that holds target/) so the
# probe's relative binary paths resolve regardless of caller cwd.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
cd "${REPO_ROOT}"

echo "==> Building release binaries (lacon + lacon-claude-hook)..." >&2
# Build the whole workspace in release so BOTH target/release/lacon AND
# target/release/lacon-claude-hook exist for the hook hot-path scenarios.
cargo build --release >&2

echo "==> Running cold_start_probe (lacon run hook hot path)..." >&2
# The probe prints the per-OS-labeled markdown table to stdout. Pass it
# straight through so callers (and CI) can capture/redirect it.
cargo run --release --bin cold_start_probe
