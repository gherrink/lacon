# Phase 5: Bundled Tier 1 rules - Discussion Log (Assumptions Mode)

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions captured in CONTEXT.md — this log preserves the analysis.

**Date:** 2026-05-22
**Phase:** 05-bundled-tier-1-rules
**Mode:** assumptions
**Areas analyzed:** Test-runner mechanism, Fixture authoring, Rule pipeline design & extends, Test target location & assertion style, Match scope for package-manager aliases

## Assumptions Presented

### A. Test-runner mechanism
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Runner reuses `RuleLoader::new(None).resolve` → `Runner::new` → `filter_bytes`, subprocess-free | Confident | `runtime/mod.rs:423-476`, `loader.rs:127-151`, `explain.rs:118-159` |
| Branch (success vs `on_error`) selected from a new `exit_code` field in `meta.yaml` | Likely | `runtime/mod.rs:457-473`, `docs/specs/testing-rules.md:39-45` (no exit_code in spec'd shape) |

### B. Fixture authoring
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| `input.txt` = real captures; `expected.txt` = rule pipeline output; hermetic CI | Confident | `docs/specs/testing-rules.md:7,16-18,88-108` |
| Byte-exact via `out.join("\n")` vs `expected.trim_end_matches('\n')` | Likely | `tests/primitives.rs:42`, `docs/specs/testing-rules.md:53` |
| meta.yaml drives reduction-exempt + must_keep_lines assertions | Confident | `docs/specs/testing-rules.md:54-68` |

### C. Rule pipeline design & extends
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| 4 test-runner rules share a base via `extends: bundled/...` | Likely | `loader.rs:555-620` (cross-bundled extends implemented, NOT fixture-tested) |
| Authors never hand-place `max_bytes` (auto-injected 32768); `keep_regex` OR-merges; `script:` rejected in pipeline | Confident | `loader.rs:674-716`, `loader.rs:775-785` |

### D. Test target location & assertion style
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Test at `crates/lacon-core/tests/bundled_rules.rs`, fixtures via `CARGO_MANIFEST_DIR/../..`, plain `assert_eq!` | Confident | `tests/primitives.rs:16-44`, `Cargo.toml` (no `[[test]]`, insta unused) |

### E. Match scope for package-manager aliases (added during presentation)
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Bundled rules match concrete tools only; `pnpm test`/`pnpm lint` aliases NOT matched in v1 | Likely | `docs/bundled-rules-roadmap.md:9,11,14,17` |
| `pkg-install` may carry a `rewrite`, pending researcher confirming per-PM quiet flags | Likely | `docs/bundled-rules-roadmap.md:11` |

## Corrections Made

No corrections — user selected "Yes, proceed"; all assumptions confirmed and locked as decisions D-01 through D-11.

## External Research

Not performed during discuss (deliberate). Two research items were flagged by the analyzer and carried into CONTEXT.md `<canonical_refs>` as directives for the downstream `gsd-phase-researcher`:
1. Current real output formats of the ten target tools (success + failure) — needed to author captures and regexes.
2. Quiet/silent flag support per package manager (npm/pnpm/yarn) for the `pkg-install` rewrite.

Rationale for deferral: gathering 10 tools' output formats is the phase-researcher's job and produces RESEARCH.md content that CONTEXT.md's decision-oriented format cannot hold; resolving it during discuss would duplicate work without changing any locked decision.
