# 0010: on_error replaces the pipeline, doesn't merge

**Status:** Accepted

## Context

Failed commands need different filtering than successful ones — when a build breaks, the user (and the model) need context, not summary. The success pipeline is designed to be aggressive ("keep only the last 5 lines"); applying the same pipeline to a failure mode would silently truncate stack traces and error messages.

The schema needs a way to express "different behavior on non-zero exit." Three options: a separate full pipeline (`on_error: { pipeline: [...] }`), additional stages appended to the success pipeline (`on_error: [...]` as extra stages), or conditional inline stages (`if exit_code != 0: ...`).

## Decision

`on_error` is a separate block that fully replaces both `pipeline` and (optionally) `post_process` when the command exits non-zero. No merging.

## Consequences

- Rule authors think about success and failure as two distinct pipelines, which matches reality — they often want very different earlier stages (e.g. disable `keep_regex` whitelisting on failure)
- Predictable: if you write `on_error`, you're saying "ignore the success pipeline entirely"
- Slightly more verbose to write than a "modifier" approach (some duplication of common stages like `strip_ansi`)
- If a rule wants 90% the same pipeline but with different `keep_tail` values, it has to repeat the rest. Acceptable cost for clarity.

## Alternatives considered

**`on_error` appends extra stages.** Easier to write the common case, but failure pipelines often need different *earlier* stages too — e.g. removing a `keep_regex` that filters out non-error context. Rejected as the wrong model.

**Inline conditional stages.** `if exit_code != 0: keep_tail 50; else: keep_tail 5`. More compact but harder to read at a glance, and it scales badly when more than one stage needs to vary by exit code.

**No special handling at all.** Rule authors would pick a pipeline that works for both cases. Rejected: the success and failure cases have genuinely different needs and pretending otherwise produces worse rules.
