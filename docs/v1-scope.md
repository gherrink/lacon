# v1 scope

The first release is the smallest thing that's useful end-to-end for one user on one assistant (Claude Code) on one OS family (macOS + Linux). Anything not on this list defers to the [backlog](backlog.md).

## In scope

### Engine

- Streaming output processor with the native primitives listed in [filter-rule-schema](specs/filter-rule-schema.md): `strip_ansi`, `drop_regex`, `keep_regex`, `replace_regex`, `dedupe`, `collapse_repeated`, `keep_head`, `keep_tail`, `keep_around_match`, `max_bytes`
- Starlark escape hatch via `post_process` step (aggregated, not per-line)
- Rule loading from `bundled/`, `~/.config/lacon/rules/`, `<project>/.lacon/rules/`
- Project > user > bundled precedence, first-match-wins resolution
- `extends` inheritance (append-only)
- `on_error` block that fully replaces the pipeline on non-zero exit
- Pre-execution command rewriting (`rewrite.add_flags`, `remove_flags`, `replace_flags`)
- Bypass via `!!` prefix and `LACON_DISABLE=1` env var
- Hard `max_bytes` cap as final-stage safety net

### Claude Code adapter

- Installs `PreToolUse` and `PostToolUse` hooks for the Bash tool
- Hook reads command + output from Claude Code, returns filtered version
- Detects `!!` prefix and passes through unfiltered
- Handles chained commands via top-level split on `&&`, `||`, `;` (filter each segment with its own rule)
- Pipes (`|`) and subshells pass through unfiltered for v1

### Tracking

- SQLite at `~/.local/share/lacon/history.db`
- Schema as in [tracking-data-model](specs/tracking-data-model.md)
- `raw_outputs` storage **off** by default (opt-in per project)
- Default retention: 30 days for invocations, 3 days for raw outputs

### CLI commands

- `lacon init` — sets up `.lacon/` in the current project, configures Claude Code hooks, adds the tiny CLAUDE.md instruction line
- `lacon run <cmd>` — manual invocation for testing/debugging
- `lacon stats` — show top offenders, bypass rates, unmatched commands; supports `--project`, `--since`, `--rule` filters
- `lacon explain <id>` — re-run filtering against stored raw output, show side-by-side diff
- `lacon doctor` — verify hooks are installed, config files are valid, rules parse
- `lacon validate <rule.yaml>` — lint a rule file without running it

### Bundled rule library

Ten rules covering the highest-impact commands. See Tier 1 in [bundled-rules-roadmap](bundled-rules-roadmap.md):

`pkg-install`, `cargo-build`, `cargo-test`, `vitest`, `jest`, `pytest`, `tsc`, `eslint`, `git-status`, `docker-build`.

### Documentation

- README with install + quickstart
- A worked example: writing a project-specific filter rule
- Reference for every primitive

## Acceptance criteria

v1 ships when:

- [ ] All ten bundled rules reduce their target commands by at least 50% on representative output without dropping errors
- [ ] `lacon init` followed by a `pnpm install` in any new project works end-to-end with no manual config
- [ ] Cold-start binary invocation is under 10ms (measured on the hook hot path)
- [ ] `lacon explain` correctly reproduces the filtering decision for any tracked invocation
- [ ] Rule files can be edited and changes take effect on the next invocation (no daemon, no restart)
- [ ] Test suite covers each native primitive and the chained-command splitter

## Explicitly out of scope for v1

See [backlog](backlog.md) for the full list. The most likely-to-be-asked-about exclusions:

- Adapters for Cursor, aider, or any non-Claude-Code assistant
- Per-token (vs per-byte) accounting
- Per-line streaming Starlark
- Filtering inside pipes
- Native Windows support (WSL is fine; native is deferred)
- Sharing rules across machines / a public rule registry
