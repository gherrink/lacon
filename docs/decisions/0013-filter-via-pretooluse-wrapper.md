# 0013: Filter via PreToolUse-rewritten subprocess wrapper

**Status:** Accepted (2026-05-05)

## Context

ADR 0001 chose Claude Code's `PreToolUse` and `PostToolUse` hooks as the integration mechanism. The architecture in `docs/architecture.md` and the v1 scope in `docs/v1-scope.md` both assumed `PostToolUse` could replace the bytes the model receives as the tool result — via a `hookSpecificOutput.updatedToolOutput` field that earlier research suggested existed.

An empirical probe against live Claude Code on 2026-05-05 (see `docs/open-questions.md` → "Claude Code hook mechanics — resolved") confirmed: there is no such field. A `PostToolUse` hook returning `updatedToolOutput` has no effect; the model receives the raw stdout. Only `additionalContext` reaches the model, and it arrives as a `<system-reminder>` appended *after* the raw output, not as a replacement.

This invalidates the central design assumption of post-execution byte reduction via `PostToolUse`. Four alternatives were evaluated (see `open-questions.md`): rewrite-to-wrapper (this ADR), `additionalContext`-only annotation, flags-only, and waiting for a future `updatedToolOutput`.

## Decision

Filtering happens through a subprocess wrapper invoked by a `PreToolUse` hook that rewrites matched commands. The wrapper spawns the original command, merges stderr into stdout, runs the pipeline (success or `on_error` depending on the subprocess's exit code), writes filtered bytes to its own stdout — which becomes Claude Code's tool result — and exits with the subprocess's exit code.

Concretely:

1. **`lacon run --rule <id> -- <cmd> [args...]`** is the wrapper invocation. It spawns `<cmd> [args...]`, reads merged stdout+stderr line-by-line, runs the rule's pipeline (or `on_error` pipeline if the subprocess exits non-zero), writes filtered bytes to its own stdout, writes the tracking row to SQLite, and exits with the subprocess's exit code. `lacon run` without `--rule` runs the rule resolver inline against the inner command — useful for manual testing and for the same code path the hook uses.
2. **The Claude Code adapter installs only a `PreToolUse` hook for the Bash tool.** When a rule matches the command, the hook returns `hookSpecificOutput.updatedInput` with the command rewritten from `<cmd> [args...]` to `lacon run --rule <id> -- <rewritten-cmd> [rewritten-args...]`, where `<rewritten-cmd>` is the result of applying the rule's `rewrite` block (flag additions/removals) to the inner argv. Unmatched commands are returned unchanged.
3. **stderr merges into stdout** inside `lacon run`. The pipeline operates on a single combined stream. Stream separation is lost; we accept this for v1 in exchange for avoiding process-substitution fragility.
4. **Exit codes propagate naturally.** `lacon run` waits on the subprocess and exits with its exit code. Claude Code's `PostToolUseFailure` event still fires correctly on non-zero exit, since the rewritten command's exit code is the original's.
5. **`on_error` is implemented inside `lacon run`.** When the subprocess exits non-zero, the wrapper switches to the rule's `on_error` pipeline before flushing buffered output. The semantics defined in ADR 0010 (replaces, doesn't merge) are unchanged; only the implementation location moves from "hypothetical second hook on `PostToolUseFailure`" to "internal mode of the wrapper."
6. **`PostToolUse` is not installed in v1.** Reserved for v1.5: a hybrid path (fork 5 in `open-questions.md`) attaches `additionalContext` annotations to *unmatched* commands ("lacon could have stripped ~3 kB if it had a rule for this") for the unmatched-offenders feedback loop and `lacon stats`.

## Consequences

- **The streaming pipeline, rule schema, primitives, Starlark stage, and bundled-rules roadmap survive unchanged.** They execute inside `lacon run` instead of inside a hook responder. ADRs 0003, 0004, 0005, 0007, 0008, 0010, 0011, 0012 are unaffected by execution location.
- **`lacon run` becomes a production code path**, not just a manual testing entry. The cold-start budget (≤10 ms, see ADR 0001 context) tightens correspondingly: the binary now runs on every matched command's hot path, not only as a post-execution analyzer.
- **Chained commands** (top-level `&&`, `||`, `;` — already in v1 scope) are split before wrapping; each matched segment is wrapped independently with its own `lacon run --rule <id> --` prefix. Unmatched segments pass through untouched.
- **User-authored pipes** (e.g. `cmd | grep foo`): the matched argv is wrapped as a unit. Whether the pipe is included in the wrapped argv depends on how the rule matcher resolves it — that detail belongs in `filter-rule-schema.md`, not here.
- **TTY detection downstream**: tools spawned by `lacon run` see "not a TTY" because the wrapper does not allocate a PTY. Most tools emit *less* noise in non-TTY mode (no progress bars, no color), which compounds with our filtering. A few change semantics (`git status` short form, `ls` non-columnar) — generally aligned with what we want anyway.
- **stdout/stderr ordering** under merge may differ from raw terminal interleaving. Acceptable v1 trade-off per the stderr-merge decision.
- **Tracking**: `lacon run` writes the invocation row to SQLite at end-of-pipeline. Schema (`tracking-data-model.md`) unchanged; only the writer moves.
- **`!!` prefix and `LACON_DISABLE=1` bypass** are still honored at the `PreToolUse` hook layer — when bypassed, the hook returns the original command unchanged, so `lacon run` is never invoked.
- **Reversibility.** If Claude Code later adds an `updatedToolOutput` field (or equivalent), this design can be revisited by switching the adapter from "rewrite to wrapper" to "respond from PostToolUse." The engine itself doesn't change.

## Relationship to prior ADRs

This ADR is additive. No existing ADR is amended.

- **0001 (use Claude Code hooks):** still accepted. The integration narrows to `PreToolUse` only for filtering; `PostToolUse` is reserved for v1.5 telemetry.
- **0005 (streaming-first):** still accepted. The streaming pipeline runs inside `lacon run`.
- **0006 (hybrid rewrite + filter):** still accepted. The `rewrite` block applies to the inner argv before wrapping; the `pipeline` block applies inside `lacon run`. Both mechanisms remain first-class.
- **0008 (aggregated Starlark):** still accepted; the Starlark stage runs inside `lacon run`.
- **0010 (on_error replaces pipeline):** still accepted. Implementation moves from "second hook on `PostToolUseFailure`" (which was speculative — no such mapping was ever locked in) to "internal mode of `lacon run` switched on the subprocess's observed exit code." Semantics unchanged.
- **0011 (SQLite for tracking):** still accepted. `lacon run` is the writer.

## Alternatives considered

**Literal pipe form: `bash -o pipefail -c '<cmd> 2>&1 | lacon filter --rule X'`.** The most direct interpretation of "rewrite to pipe." Rejected because `lacon filter` running as a pipe consumer cannot see the upstream's exit code in time to choose between the success and `on_error` pipelines — by the time `${PIPESTATUS[0]}` is available, `lacon filter` has already read EOF and exited. Workarounds (two-stage rewrites, exit-code handshakes via temp files, second `lacon postprocess` invocation after the pipe) were uglier than just having `lacon` parent the subprocess. The wrapper achieves the same data flow ("filter on the producer side, before bytes reach Claude Code") with cleaner exit-code semantics.

**`additionalContext`-only annotation.** A `PostToolUse` hook reads the raw output and attaches a savings estimate or summary via `additionalContext`. Rejected as the primary mechanism: the model still reads the unfiltered output, so the byte-reduction goal is not achieved. Useful as a v1.5 supplement (fork 5) for unmatched-command feedback.

**Flags-only.** Restrict the project to `PreToolUse` `rewrite` (add quiet flags). Rejected: drops the streaming pipeline, the Starlark stage, and the regex primitives — most of `filter-rule-schema.md` becomes unimplementable. Roughly half the v1 bundled rules survive, and the project loses meaningful differentiation over a few CLAUDE.md instructions.

**Wait for `updatedToolOutput`.** File a feature request and pause development. Rejected: open-ended; depends on an external roadmap; rewrite-to-wrapper is implementable today and reversible if the field later ships.

**`decision: "block"` + `reason` hack.** Use `PostToolUse`'s documented block mechanism to suppress the raw output and put filtered output in `reason`. Rejected: `block` semantically marks the call as prevented and `reason` is presented as a denial reason — the model would treat the command as failed. Misuses the field's contract.
