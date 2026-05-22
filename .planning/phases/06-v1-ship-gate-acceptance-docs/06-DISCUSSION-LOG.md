# Phase 6: v1 ship gate — acceptance & docs - Discussion Log (Assumptions Mode)

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions captured in CONTEXT.md — this log preserves the analysis.

**Date:** 2026-05-22
**Phase:** 06-v1-ship-gate-acceptance-docs
**Mode:** assumptions
**Areas analyzed:** Acceptance test strategy, Cold-start benchmark, Hot-reload semantics, Documentation deliverables, CI hermeticity

## Assumptions Presented

### Acceptance test strategy
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| SC4 coverage is mostly already met (audit, not re-author) | Confident | `primitives.rs:46-170` (10 primitives), `chain_split.rs` (20 tests/13 scenarios), `bundled_rules.rs:160-209` (10 rules) |
| REQ-acceptance-bundled-reduction already met by Phase 5 | Confident | `bundled_rules.rs` asserts `len(expected)/len(input) <= 0.5` + `must_keep_lines` |
| pnpm E2E reconciled via `#[ignore]` real test + hermetic stub | Likely | `end_to_end.rs:30-77` test_emitter pattern; `docs/testing-rules.md` hermetic stance |

### Cold-start benchmark
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Reuse existing `benches/cold_start.rs`, wrap in entry point | Confident | `benches/cold_start.rs:1-12` self-described Phase 6 gate; prints `env::consts::OS` |
| Must resolve Phase 2 `tracker_open` fsync regression (split first-ever vs steady-state) | Confident | `tracker_open.rs:20-21` BUDGET_MICROS=3700; `02-PHASE-BENCH.md` ~25020µs ext4 fsync |
| macOS half of SC1 needs a CI runner (dev is Linux-only) | Likely | no `.github/` exists; `cold_start.rs:113` per-OS labeling |

### Hot-reload semantics
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Hot reload is automatic via no-daemon; needs proof test not file-watcher | Confident | `loader.rs:87-88,262-274` mtime cache; ADR-0013 no daemon; fresh process per invocation |

### Documentation deliverables
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| README rewrite + docs/ files, sourcing from existing schema spec | Likely | `README.md:1-24` stub; `filter-rule-schema.md:98-152` primitives, `:213-233` worked example |

### CI hermeticity
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| No CI exists; Phase 6 creates hermetic GitHub Actions workflow | Likely | no `.github/`/`.gitlab-ci`/`.circleci`; `rusqlite[bundled]`; `Cargo.toml:11` GitHub repo |

## Corrections Made

No corrections — all four user-facing decisions selected the recommended option:

### macOS gate (SC1)
- **Decision:** GitHub Actions macOS runner (recommended). macOS cold-start number comes from a `macos-latest` CI lane since the dev machine is Linux-only. → D-09

### pnpm E2E (SC2 / SC4 tension)
- **Decision:** `#[ignore]`'d real test + hermetic stub (recommended). Real `pnpm install` test runnable by hand; hermetic CI test via `test_emitter`. → D-07

### CI scope
- **Decision:** Yes — create hermetic GitHub Actions CI (recommended). New workflow, hermetic by construction, real-pnpm test stays `#[ignore]`d. → D-08

### Primitive reference location
- **Decision:** New `docs/primitive-reference.md` (recommended), sourcing canonical behavior from `filter-rule-schema.md` to avoid drift. → D-10

## External Research

Not performed during discussion — three topics deferred to gsd-phase-researcher (recorded in CONTEXT.md canonical_refs): trustworthy macOS cold-start measurement on shared CI runners, idiomatic Rust `#[ignore]` E2E pattern, and fsync-cost generalization across filesystems.
