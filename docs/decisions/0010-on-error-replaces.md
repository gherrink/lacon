---
status: accepted
schema-version: 2
---

# 0010: on_error replaces the pipeline, doesn't merge

## Context

Failed commands need different filtering than successful ones — when a build breaks, the user and the model need context, not summary. The success pipeline is designed to be aggressive ("keep only the last 5 lines"); applying it to a failure would silently truncate stack traces and error messages. The schema needs a way to express "different behavior on non-zero exit."

## Options

- **`on_error` fully replaces (chosen).** A separate block that replaces `pipeline` (and optionally `post_process`) on non-zero exit. No merging.
- **`on_error` appends extra stages.** Easier for the common case, but failure pipelines often need different *earlier* stages too (e.g. removing a `keep_regex` that filters out non-error context). Wrong model. Rejected.
- **Inline conditional stages** (`if exit_code != 0: keep_tail 50; else: keep_tail 5`). More compact but harder to read at a glance, and it scales badly when several stages vary by exit code. Rejected.
- **No special handling.** Authors would pick one pipeline for both cases — but success and failure have genuinely different needs, and pretending otherwise produces worse rules. Rejected.

## Decision

`on_error` is a separate block that fully replaces both `pipeline` and (optionally) `post_process` when the command exits non-zero. No merging.

## Consequences

- Rule authors think about success and failure as two distinct pipelines, which matches reality — they often want very different earlier stages (e.g. disabling `keep_regex` whitelisting on failure).
- Predictable: writing `on_error` means "ignore the success pipeline entirely."
- Slightly more verbose than a "modifier" approach (some duplication of common stages like `strip_ansi`).
- A rule wanting 90% the same pipeline but a different `keep_tail` must repeat the rest — an acceptable cost for clarity.
