# Open questions

Things we don't yet know that could change the design. Each one needs an answer (or a "we accept the unknown") before the relevant part of v1 ships.

## Claude Code hook mechanics

We've committed to using `PreToolUse` and `PostToolUse` hooks, but several practical questions remain unverified:

- **What exactly does the hook receive?** Documented schema vs. what's actually in the JSON payload. Does it include exit code? Duration? Working directory?
- **Can the hook modify the command before execution?** (Required for the `rewrite` feature.) If not, we need a different rewrite path.
- **Can the hook modify the output before the model sees it?** (Required for filtering.) If the model sees the raw output regardless of what the hook returns, the entire approach collapses.
- **What's the timeout?** If the hook takes too long, does Claude Code abort, return raw output, or hang?
- **Hook execution context.** TTY? stdin? environment variables?

**Action:** Verify against current Claude Code docs and a small experimental hook before locking the v1 design.

## Starlark performance at hook scale

Starlark startup overhead is small (<5ms) but it gets invoked on every command Claude Code runs that hits a rule with `post_process`. In a busy session that could be hundreds of times.

- **Can we keep a Starlark interpreter alive across invocations?** A persistent daemon would amortize startup. But it adds complexity (lifecycle, IPC, restart on rule change) and we've otherwise committed to a daemon-less design.
- **Or do we just measure and accept it?** If 5ms × 200 invocations is 1 second of total session overhead, that's tolerable.

**Action:** Benchmark Starlark cold-start with `starlark-rust` once we have a prototype. Decide based on data.

## Chained command behavior

We split on top-level `&&`, `||`, `;` and filter each segment independently. Edge cases:

- What if the second command's behavior depends on the first command's output (which we just filtered)? Probably fine since the assistant cares about final state, not intermediate, but worth confirming.
- What about `&&` chains where the rule for one segment is "bypass" and another is "filter"? Pretty sure the answer is "respect each rule independently" but the user-facing semantics need to be documented clearly.
- What if a chain contains an interactive command (e.g. `git rebase -i && pnpm test`)? The TUI heuristic only triggers if we know the first command, but the rule may not have run by then.

**Action:** Write down explicit rules for chained-command handling in the spec, then write tests against representative inputs.

## What lives outside hooks

If a Claude Code skill spawns a subprocess that's not via the Bash tool, the hook doesn't fire. Same for:

- Long-running watchers started by the assistant (`pnpm dev`, `cargo watch`)
- Output that bypasses our hook (e.g. tools that write directly to terminal control sequences)
- The user's own terminal sessions (intentionally out of scope, but worth confirming users don't expect us to cover them)

**Action:** Document the boundary clearly in the README so users have correct expectations.

## Tokenizer choice (deferred but worth flagging)

When we eventually move from byte-based to token-based accounting, we need to choose a tokenizer:

- **Anthropic's Claude tokenizer** — most accurate for Claude Code users but the SDK to access it is closed, and we'd be wrong for non-Claude assistants
- **tiktoken (OpenAI)** — open, well-maintained, accurate-ish for many models
- **A heuristic (e.g. bytes / 4)** — wrong but free

This is deferred to backlog but the choice will affect the tracking schema, so worth thinking ahead.

## Privacy and `raw_outputs`

If we let users opt into raw output retention for `lacon explain`, we will store strings that may contain:

- API keys printed by misconfigured tools
- Local file paths that reveal user identity
- Customer data in test fixtures

We've committed to off-by-default and 0700 directory permissions. Other things to consider:

- **Redaction patterns** — drop lines matching common secret patterns before storing. Is the false-negative rate acceptable?
- **A `lacon purge` command** — for deleting all raw outputs, all entries from a date range, etc.
- **Encryption at rest** — probably overkill for v1, but should be a backlog item.

**Action:** Decide whether v1 ships any redaction logic at all, or just relies on off-by-default plus permissions plus a warning in docs.

## Testing strategy for rules

Each bundled rule needs to be tested against representative output. Where do we get that output?

- Capturing real output from `pnpm install` etc. is easy, but it varies by version, OS, lockfile state, registry latency
- Synthetic fixtures are stable but can drift from reality
- Testing in CI requires those tools to be installed in CI

**Action:** Settle on a fixture-based testing strategy; document how to regenerate fixtures when the underlying tool changes its output format.
