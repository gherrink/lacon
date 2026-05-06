# Bundled rules roadmap

The engine ships with a curated set of rules for the most-used commands. This is a living list; it tracks what's planned, what's done, and what's deliberately not on the list.

## Tier 1 — v1 must-have

These are the rules that ship with v1. Each is responsible for at least 50% byte reduction on representative output without dropping errors.

| Rule ID | Targets | Notes |
|---------|---------|-------|
| `pkg-install` | `npm install`, `pnpm install`, `yarn install`, `pnpm i`, `pnpm add` | Most common. Add `--reporter=silent` where supported, drop deprecation warnings, collapse progress. |
| `cargo-build` | `cargo build`, `cargo check` | Drop "Compiling foo v0.x" repeats; preserve warnings/errors with file:line |
| `cargo-test` | `cargo test` | Preserve test summary line; drop per-test PASS lines; preserve FAIL with full output |
| `vitest` | `vitest`, `vitest run`, `pnpm test` (resolution-dependent) | Same shape as `cargo-test` |
| `jest` | `jest`, `npx jest` | Same shape; jest has its own quirks around watch mode |
| `pytest` | `pytest`, `python -m pytest` | Drop dot-progress; preserve failure tracebacks |
| `tsc` | `tsc`, `tsc --noEmit` | Most output IS the signal (errors). Mostly ANSI strip + dedupe + tail. |
| `eslint` | `eslint`, `pnpm lint` | Drop "passing" summaries; preserve warnings/errors with file:line |
| `git-status` | `git status` | Collapse "Untracked files" sections in monorepos with thousands of files |
| `docker-build` | `docker build`, `docker buildx build` | Drop layer cache hits; preserve actual build steps and errors |

## Tier 2 — post-v1

Likely high-value rules that didn't make v1 because of capacity, not interest:

- `webpack`, `vite`, `turbopack` — frontend bundlers
- `next-build`, `remix-build`, `astro-build` — framework-specific
- `make`, `cmake`, `ninja` — C/C++ builds
- `mvn`, `gradle` — JVM
- `composer install` — PHP
- `pip install`, `poetry install`, `uv pip install` — Python
- `bundle install` — Ruby
- `terraform plan`, `terraform apply` — IaC
- `kubectl apply`, `kubectl logs` — Kubernetes
- `git log`, `git diff` (when very large)
- `find`, `rg` (when output is huge)

## Rules deliberately not on the roadmap

- **Interactive commands** (`vitest --watch`, `htop`, REPLs). The bypass-when-tty heuristic should handle these without a dedicated rule.
- **Editor invocations** (`vim`, `nano`). Out of scope.
- **Anything pager-driven** (`less`, `man`). Already filters itself.

## Format expectations for new rules

Every rule, before it lands, should have:

- A YAML rule file in `bundled-rules/`
- A fixture set under `tests/fixtures/<rule-id>/<scenario>/` with `input.txt` (captured raw output), `expected.txt` (filtered output), and `meta.yaml` (provenance), per [testing-rules](testing-rules.md). At minimum: one success-path scenario and one failure-path scenario.
- An integration test asserting expected reduction ratio and zero error-line drops (run automatically via `cargo test --test bundled_rules`)
- A short doc note in this file describing the trade-off being made
