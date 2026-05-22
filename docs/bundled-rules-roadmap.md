# Bundled rules roadmap

The engine ships with a curated set of rules for the most-used commands. This is a living list; it tracks what's planned, what's done, and what's deliberately not on the list.

## Tier 1 — v1 must-have

These are the rules that ship with v1. Each is responsible for at least 50% byte reduction on representative output without dropping errors.

| Rule ID | Targets | Notes |
|---------|---------|-------|
| `pkg-install` | `npm install`, `pnpm install`, `yarn install`, `pnpm i`, `pnpm add` | Most common. **No `rewrite` block** — reduction is pipeline-side only (drop deprecation warnings, collapse progress). A `--silent` rewrite is forbidden: on npm and pnpm it deletes the error (e.g. the E404) on a failed install, destroying the `on_error` signal (D-11). |
| `cargo-build` | `cargo build`, `cargo check` | Drop "Compiling foo v0.x" repeats; preserve warnings/errors with file:line |
| `cargo-test` | `cargo test` | Preserve test summary line; drop per-test PASS lines; preserve FAIL with full output |
| `vitest` | `vitest`, `vitest run`, `pnpm test` (resolution-dependent) | Same shape as `cargo-test` |
| `jest` | `jest`, `npx jest` | Same shape; jest has its own quirks around watch mode |
| `pytest` | `pytest`, `python -m pytest` | Drop dot-progress; preserve failure tracebacks |
| `tsc` | `tsc`, `tsc --noEmit` | Most output IS the signal (errors). Mostly ANSI strip + dedupe + tail. |
| `eslint` | `eslint`, `pnpm lint` | Drop "passing" summaries; preserve warnings/errors with file:line |
| `git-status` | `git status` | Collapse "Untracked files" sections in monorepos with thousands of files |
| `docker-build` | `docker build`, `docker buildx build` | Drop layer cache hits; preserve actual build steps and errors |

## Tier 1 — implementation notes

The per-rule reduction trade-off each rule makes (signal kept vs. noise dropped). Success-path filtering is the reductive `pipeline`; failure-path filtering is the context-preserving `on_error` block (replaces, never merges — [ADR-0010](decisions/0010-on-error-replaces-pipeline.md)). Authors omit `max_bytes` (the loader auto-injects the 32 KiB cap).

- **`pkg-install`** — drop deprecation/progress noise (`npm warn deprecated`, pnpm `Progress:`/update box, yarn `[N/4]` steps, funding blurb); keep the `added N packages` / vulnerability summary. **No `rewrite` block** per D-11: a `--silent` (or pnpm silent-reporter) flag deletes the error on a failed install (npm/pnpm emit zero bytes on exit 1), so reduction is pipeline-side only and `on_error` preserves the error block.
- **`cargo-build`** — drop the `Compiling`/`Updating`/`Locking`/`Finished` status repeats (the bulk on multi-dep builds); preserve `warning:`/`error[E…]:` diagnostic blocks with their `file:line` (`-->`) context. Failure exits `101`.
- **`cargo-test`** — drop per-test `... ok` PASS lines and the `Compiling`/`Finished`/`Running` framing; keep the `test result:` summary. On failure preserve `... FAILED`, panic/`assertion` detail, the `failures:` list and `error: test failed`.
- **`vitest`** — strip ANSI first (heavily colorized), drop per-file `✓` PASS lines and `Duration`/setup-timing lines; keep the `Test Files`/`Tests` summary. On failure keep `❯`/`×`/`→` failure detail. Trade-off: match on surrounding ASCII (`failed`, `passed`, `(tests)`) rather than the multibyte glyphs.
- **`jest`** — drop per-suite `PASS` lines, `Snapshots:`/`Time:` framing; keep the `Test Suites:`/`Tests:` summary. On failure keep `FAIL`, the `●` failure header, `Expected:`/`Received:` and the `at … (file:line:col)` location. jest writes to stderr, captured by the merged stream.
- **`pytest`** — drop the `PASSED` per-test lines and the session header (`platform`/`cachedir`/`rootdir`/`plugins`/`collecting`); keep the final `=== N passed ===` banner. On failure preserve the `=== FAILURES ===` block, `E ` assertion lines, `>` source lines and `file:line: Error`. Capture `-v` output as the primary fixture (default dot-progress is already tiny).
- **`tsc`** — the output IS the signal, so reduction comes from `strip_ansi` + `dedupe` + `keep_tail`, not from dropping lines; keep every `file(line,col): error TS…:` and the `Found N errors` summary. tsc emits nothing on success, so the success fixture is reduction-exempt and the failure fixture is primary.
- **`eslint`** — strip ANSI (stylish formatter colorizes); keep the file-path header plus the `line:col error/warning rule` detail and the `✖ N problems` summary; drop pure-pass output (eslint emits nothing on a clean run, so its clean-success fixture is reduction-exempt). Trade-off: dropping pure-warning files is acceptable signal-loss on the success path but never on `on_error`.
- **`git-status`** — collapse the long tab-indented `Untracked files` block (`collapse_repeated` on `^\t` with a `… N more` summary) — the bulk in monorepos; keep `On branch`, the `Changes`/`modified:` lines. Failure (`fatal: not a git repository`) is rare and small → reduction-exempt, keep `fatal:`.
- **`docker-build`** — drop `CACHED`/`DONE`/`sha256:` progress (the sha/byte lines vary run-to-run and would make `expected.txt` brittle); keep the `[k/m]` build-step headers and RUN echo output. On failure preserve the `#N ERROR:` line, the `------` framed context, the `>>>` Dockerfile excerpt and `ERROR: failed to build`. Capture BuildKit (default) output; record `tool_version` in `meta.yaml`.

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
