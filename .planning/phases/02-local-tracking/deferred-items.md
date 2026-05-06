# Phase 2 — Deferred Items

Out-of-scope discoveries logged during Phase 2 execution per executor scope-boundary rule.

## Pre-existing rustdoc warning (Plan 02-01 discovery)

- **File:** `crates/lacon-core/src/rules/schema.rs:72`
- **Warning:** `unresolved link to '0'` — the doc comment `Exact match against argv[0] basename.` is parsed by rustdoc as an intra-doc link `[0]`.
- **Scope:** Pre-existing Phase 1 leftover. Not caused by Phase 2 work; not in any Phase 2 plan's `files_modified`.
- **Suggested fix:** escape as `argv\[0\]` or use ``argv[0]`` (backticks) in the doc comment. Touch in a future Phase 1 cleanup pass; do NOT fix here.
- **Discovered by:** Plan 02-01 acceptance-criterion verification (`cargo doc -p lacon-core --no-deps --document-private-items`).
- **Discovered date:** 2026-05-06.
