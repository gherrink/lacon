---
schema-version: 1
---

# Roadmap

## Milestones

### Tier 1 — v1 bundled rule library (shipped)  {#tier-1-v1-bundled-rule}

#### Proves

The engine ships a curated set of rules for the most-used commands, each responsible for at least 50% byte reduction on representative output without dropping errors — success-path reduction via the `pipeline`, failure-path context preserved via `on_error` (replaces, never merges — ADR 0010). Authors omit `max_bytes`; the loader auto-injects the 32 KiB cap.

#### Decomposition

The ten rules and their signal/noise trade-offs:

- **`pkg-install`** (`npm`/`pnpm`/`yarn install`, `pnpm i`/`add`) — drop deprecation/progress noise, keep the `added N packages`/vulnerability summary. **No `rewrite` block**: a `--silent`/silent-reporter flag deletes the error on a failed install (npm/pnpm emit zero bytes on exit 1), so reduction is pipeline-side only and `on_error` preserves the error block.
- **`cargo-build`** (`cargo build`/`check`) — drop `Compiling`/`Updating`/`Finished` repeats; preserve `warning:`/`error[E…]:` blocks with `file:line`. Failure exits `101`.
- **`cargo-test`** — drop per-test `... ok` and framing; keep `test result:`; on failure preserve `FAILED`, panic/assertion detail, and the `failures:` list.
- **`vitest`** / **`jest`** — strip ANSI, drop per-file/suite PASS lines and timing framing, keep the summary; on failure keep the failure detail (`❯`/`×` or `●`, `Expected:`/`Received:`, `at … (file:line)`). Match surrounding ASCII, not multibyte glyphs.
- **`pytest`** — drop `PASSED` lines and the session header; keep the `=== N passed ===` banner; on failure preserve the `=== FAILURES ===` block and assertion lines.
- **`tsc`** — output IS the signal, so reduction is `strip_ansi` + `dedupe` + `keep_tail`; keep every `file(line,col): error TS…` and `Found N errors`. Success emits nothing (success fixture reduction-exempt).
- **`eslint`** — strip ANSI, keep file header + `line:col error/warning rule` + `✖ N problems`; clean success emits nothing (reduction-exempt).
- **`git-status`** — collapse the long tab-indented `Untracked files` block; keep `On branch` and `Changes`/`modified:`.
- **`docker-build`** — drop `CACHED`/`DONE`/`sha256:` progress; keep `[k/m]` step headers and RUN output; on failure preserve `#N ERROR:`, framed context, and `ERROR: failed to build`.

Format expectations for any new rule before it lands: a YAML file in `bundled-rules/`; a fixture set under `tests/fixtures/<rule-id>/<scenario>/` (`input.txt`, `expected.txt`, `meta.yaml`) with at least one success and one failure scenario; an integration test asserting the reduction ratio and zero error-line drops (`cargo test --test bundled_rules`); and a trade-off note.

Deliberately not on the roadmap: interactive commands (`vitest --watch`, `htop`, REPLs — handled by the TUI-bypass heuristic), editor invocations (`vim`, `nano`), and pager-driven output (`less`, `man`, which filter themselves).

### Tier 2 — post-v1 rule expansion  {#tier-2-post-v1-rule}

#### Proves

Coverage extends to the next tranche of high-value commands that missed v1 for capacity, not interest.

#### Decomposition

Candidate rules: `webpack`/`vite`/`turbopack` (bundlers); `next-build`/`remix-build`/`astro-build` (frameworks); `make`/`cmake`/`ninja` (C/C++); `mvn`/`gradle` (JVM); `composer install` (PHP); `pip install`/`poetry install`/`uv pip install` (Python); `bundle install` (Ruby); `terraform plan`/`apply` (IaC); `kubectl apply`/`logs` (Kubernetes); large `git log`/`git diff`; and huge `find`/`rg` output.
