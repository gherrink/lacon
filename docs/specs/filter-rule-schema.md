# Filter rule schema

Reference for the YAML format that defines filter rules. The implementation must match this document; any change here is a breaking change for users.

## File location

Rules can live in three places, in priority order:

| Priority | Path |
|----------|------|
| 1 (highest) | `<cwd>/.lacon/rules/*.yaml` |
| 2 | `~/.config/lacon/rules/*.yaml` |
| 3 (lowest) | bundled (embedded in the binary) |

Resolution is first-match-wins. There is no merging across layers; if you want to layer onto a bundled rule, use `extends`.

## Top-level structure

```yaml
id: pnpm-install
description: pnpm/npm/yarn install
extends: bundled/pkg-install      # optional

match: { ... }                    # required (unless inherited)
bypass_when: { ... }              # optional
rewrite: { ... }                  # optional
pipeline: [ ... ]                 # required (unless inherited)
on_error: { ... }                 # optional
post_process: { ... }             # optional
```

## Fields

### `id`

Required string. Stable identifier used in tracking, in `extends` references, and in CLI output. Must be unique within a layer. Convention: kebab-case.

### `description`

Optional string. Shown in `lacon doctor` and `lacon stats` for human readability.

### `extends`

Optional string. References another rule by ID, optionally prefixed with `bundled/`, `user/`, or `project/`. The parent's fields are inherited where this rule doesn't specify them; the parent's pipeline stages are *prepended* to this rule's pipeline. See [Inheritance semantics](#inheritance-semantics) below.

### `match`

Required (unless inherited). Pattern matcher that determines whether this rule applies to a given command.

```yaml
match:
  any:                            # OR semantics
    - { command: pnpm, args_prefix: [install] }
    - { command: pnpm, args_prefix: [i] }
  # OR alternative:
  command_regex: '^(pnpm|npm)\s+(install|i)'
```

Match operators:

- `command`: exact match against argv[0] basename (so `/usr/local/bin/pnpm` matches `command: pnpm`)
- `args_prefix`: argv[1..N] must start with these tokens
- `args_contain`: argv[1..] must include these tokens (any position)
- `command_regex`: regex against the full normalized command line
- `any`: list of sub-matches, OR semantics
- `all`: list of sub-matches, AND semantics (rare)

### `bypass_when`

Optional. If matched, rule is skipped entirely (raw output passes through).

```yaml
bypass_when:
  any:
    - has_flag: ['--verbose', '-v', '--debug']
    - is_tty: true
    - env: { LACON_DEBUG_RULE: '1' }
```

### `rewrite`

Optional. Modifies the command before execution. Only applied if the adapter supports pre-execution modification (Claude Code does via `PreToolUse`).

```yaml
rewrite:
  add_flags: ['--reporter=silent']
  remove_flags: ['--verbose', '-v']
  replace_flags:
    '--progress': '--no-progress'
```

`add_flags` is idempotent — won't add a flag that's already present.

### `pipeline`

Required (unless inherited). Ordered list of stages applied to streamed output. Each stage has the form `<primitive>: <args>` or just `<primitive>` (no args).

#### Native primitives

**`strip_ansi`** — no args. Removes ANSI color and control sequences.

**`drop_regex: <pattern>`** — drops any line matching the regex.

```yaml
- drop_regex: '^npm warn deprecated'
```

**`keep_regex: <pattern>`** — whitelist mode. If any `keep_regex` is present, only matching lines are kept. Multiple `keep_regex` stages are OR'd.

```yaml
- keep_regex: '(error|ERROR|FAIL)'
```

**`replace_regex: { pattern, replacement }`** — substitutes matched text.

```yaml
- replace_regex:
    pattern: '\b/Users/[^/]+/'
    replacement: '~/'
```

**`dedupe`** — collapses consecutive duplicate lines. Optional arg: `max_kept` (default 1).

```yaml
- dedupe: { max_kept: 1 }
```

**`collapse_repeated: { pattern, max_kept, summary }`** — collapses consecutive lines that all match `pattern` into `max_kept` examples plus a summary line.

```yaml
- collapse_repeated:
    pattern: '^Progress: \d+%'
    max_kept: 1
    summary: '… {count} progress lines'
```

The placeholder `{count}` in `summary` is replaced with the number of dropped lines.

**`keep_head: { lines: N }`** or **`keep_head: { bytes: N }`** — keeps only the first N lines / bytes.

**`keep_tail: { lines: N }`** or **`keep_tail: { bytes: N }`** — keeps only the last N lines / bytes. Implemented as a bounded ring buffer.

**`keep_around_match: { pattern, before, after }`** — for each line matching `pattern`, keep `before` preceding and `after` following lines (grep -B/-A semantics).

```yaml
- keep_around_match:
    pattern: '^FAIL '
    before: 0
    after: 20
```

**`max_bytes: N`** — hard cap on total output size. If exceeded, output is truncated with a `[lacon: truncated, N more bytes dropped]` marker. Should always be the last stage.

#### Starlark stage

**`script: { path, function }`** — runs a Starlark function on the aggregated output.

```yaml
- script:
    path: scripts/jest_summary.star
    function: process
```

The function signature is:

```python
def process(ctx, lines):
    """
    ctx: object with .exit_code, .duration_ms, .command, .args, .project_path
    lines: list[str] — the output so far, after preceding stages
    return: list[str] — the output to pass to subsequent stages
    """
    ...
```

Starlark stages are slow relative to native primitives. Use sparingly. For complex per-rule logic, prefer placing the Starlark stage near the end of the pipeline so it operates on already-reduced output.

### `on_error`

Optional. Replaces `pipeline` (and optionally `post_process`) entirely when the command exits non-zero. Does not merge.

```yaml
on_error:
  pipeline:
    - strip_ansi
    - keep_regex: '(ERR_|error|FAIL)'
    - keep_tail: { lines: 50 }
    - max_bytes: 8192
```

The motivation: failed commands need different filtering than successful ones — when something breaks, you want context, not summary.

### `post_process`

Optional. A Starlark function that runs once on the entire post-pipeline output. Equivalent to a final `script:` stage, but conventionally placed here for clarity.

```yaml
post_process:
  path: scripts/pnpm_install.star
  function: postprocess
```

## Inheritance semantics

When a rule has `extends: <other-rule-id>`:

1. Fields not defined on this rule are inherited from the parent (`description`, `match`, `bypass_when`, `rewrite`, `on_error`, `post_process`)
2. The parent's `pipeline` stages are **prepended** to this rule's `pipeline`
3. Inheritance is single-level and non-cyclic; `extends` chains are flattened at load time

If you need finer control (insert a stage, remove a stage), copy the parent rule and edit it. The simple model is the contract.

## Worked example

```yaml
# .lacon/rules/our-monorepo-pnpm.yaml
id: our-monorepo-pnpm
description: pnpm install in our monorepo (verbose lockfile output we want to strip)
extends: bundled/pkg-install

# pnpm install in this repo emits 50+ "Lockfile is up to date" lines we don't need
pipeline:
  - drop_regex: '^Lockfile is up to date'
  - drop_regex: '^Already up to date'
```

This rule:

- Inherits `match`, `rewrite`, `on_error` from `bundled/pkg-install`
- Runs the bundled pipeline first, then the two extra `drop_regex` stages
- Wins resolution against `bundled/pkg-install` because project rules outrank bundled

## Validation

`lacon validate <path>` parses, type-checks, and dry-runs a rule against optional fixture files. A rule with an invalid regex, an unknown primitive, a circular `extends`, or a missing referenced Starlark file fails to load.

`lacon doctor` runs validation against every rule on the system and reports broken ones.
