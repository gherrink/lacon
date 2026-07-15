---
derived-from: prd:v1-scope
schema-version: 1
---

# Filter rule schema

## Goal

Reference for the YAML format that defines filter rules. The implementation must match this document; any change here is a breaking change for users.

## Context

Rules live in three places, in priority order: `<cwd>/.lacon/rules/*.yaml` (highest), `~/.config/lacon/rules/*.yaml`, then bundled (embedded in the binary, lowest). Resolution is first-match-wins with no merging across layers; layering onto a bundled rule is done explicitly via `extends`.

## Criteria

### Top-level structure  {#top-level-structure}

A rule is a YAML map with: `id` (required), `description` (optional), `extends` (optional), `match` (required unless inherited), `bypass_when` (optional), `rewrite` (optional), `pipeline` (required unless inherited), `on_error` (optional), `post_process` (optional).

### id  {#id}

Required string, unique within a layer, used in tracking, `extends` references, and CLI output. Convention: kebab-case.

### match operators  {#match-operators}

`match` (required unless inherited) determines applicability. Operators: `command` (exact match against the basename of argv[0]), `args_prefix` (argv[1..] must start with these tokens), `args_contain` (argv[1..] includes these tokens in any position), `command_regex` (regex against the full normalized command line), `any` (list of sub-matches, OR semantics), `all` (list of sub-matches, AND semantics).

### bypass_when  {#bypass-when}

Optional. If it matches, the rule is skipped entirely and raw output passes through. Operators include `has_flag`, `is_tty`, and `env`.

### rewrite  {#rewrite}

Optional; modifies the command before execution, applied only when the adapter supports pre-execution modification (Claude Code via `PreToolUse`). Keys: `add_flags` (idempotent — never adds a flag already present), `remove_flags`, `replace_flags`.

### pipeline structure  {#pipeline-structure}

Required unless inherited. An ordered list of stages applied to streamed output; each stage is `<primitive>: <args>` or a bare `<primitive>`.

### Native primitives  {#native-primitives}

`strip_ansi` (no args); `drop_regex: <pattern>` (drop matching lines); `keep_regex: <pattern>` (whitelist — if any present, only matching lines kept; multiple are OR'd); `replace_regex: {pattern, replacement}`; `dedupe: {max_kept=1}` (collapse consecutive duplicates); `collapse_repeated: {pattern, max_kept}`; `keep_head: {lines|bytes}`; `keep_tail: {lines|bytes}` (bounded ring buffer); `keep_around_match: {pattern, before, after}` (grep -B/-A semantics); `max_bytes: N` (hard total-size cap, should be the last stage, truncates with a `[lacon: truncated, N more bytes dropped]` marker).

### collapse_repeated fidelity contract (Phase 9)  {#collapse-repeated-fidelity-contract-phase}

When a run of matching lines is collapsed, the dropped lines are replaced by exactly one fixed lacon-namespaced marker `[lacon: collapsed N lines]` (N = lines dropped), emitted both mid-stream when a run ends and at end-of-output flush, and only when at least one line was actually dropped. The marker is a fixed line by design so it can never inherit the formatting of the lines it replaces, and every surviving (non-marker) line is byte-identical to an input line — a collapsed run is never substituted by a plausible-but-fabricated tool line. The marker is advisory, not a trusted sentinel: tool output can coincidentally reproduce a byte-identical marker line, so consumers must not treat its presence as a lacon-only signal. The earlier free-form `summary:` template is no longer emitted — the YAML loader still accepts the `summary` key for backward compatibility, but its value is ignored at emission time and rules should drop it.

### Starlark stage  {#starlark-stage}

`script: {path, function}` runs a Starlark function on the aggregated output. Signature `def process(ctx, lines) -> list[str]`, where `ctx` exposes `.exit_code`, `.duration_ms`, `.command`, `.args`, `.project_path` and `lines` is the output after preceding stages. Starlark is slow relative to native primitives — use sparingly and place near the end of the pipeline so it runs on already-reduced output.

### on_error  {#error}

Optional; fully replaces `pipeline` (and optionally `post_process`) when the command exits non-zero. It does not merge. Motivation: failed commands need context, not summary.

### post_process  {#post-process}

Optional Starlark function that runs once on the entire post-pipeline output — equivalent to a final `script:` stage, conventionally placed here for clarity.

### Inheritance semantics  {#inheritance-semantics}

With `extends: <rule-id>`: fields not defined on the child are inherited from the parent (`description`, `match`, `bypass_when`, `rewrite`, `on_error`, `post_process`); the parent's `pipeline` stages are prepended to the child's; inheritance is single-level, non-cyclic, and flattened at load time. Finer control (insert/remove a stage) requires copying the parent rule and editing it.

### Validation  {#validation}

`lacon validate <path>` parses, type-checks, and dry-runs a rule against optional fixture files. A rule with an invalid regex, an unknown primitive, a circular `extends`, or a missing referenced Starlark file fails to load. `lacon doctor` runs validation against every rule on the system and reports broken ones.
