# Phase 3: Claude Code adapter & `lacon init` - Discussion Log (Assumptions Mode)

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions captured in CONTEXT.md — this log preserves the analysis.

**Date:** 2026-05-16
**Phase:** 03-claude-code-adapter-lacon-init
**Mode:** assumptions
**Areas analyzed:** Adapter binary architecture, Chain splitter, `lacon init` settings.json strategy, TUI heuristic, `rewrite` application + argv quoting, CLAUDE.md instruction, Bypass detection, Env-var contract handoff, Idempotency resolution

## Assumptions Presented

### A. Adapter binary architecture
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Ship `lacon-claude-hook` as a separate `[[bin]]` inside `crates/lacon-adapter-claudecode` — NOT a `lacon hook` subcommand | Confident | `crates/lacon-cli/tests/cli_surface.rs:11` locks 6-command cap (REQ-cli-surface-cap); `bin/test_emitter/Cargo.toml` precedent for `[[bin]]` outside `lacon-cli`; adapter crate currently empty stub |

### B. Chain splitter implementation
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Hand-rolled DFA on raw command string tracking quote/`(...)`/`$(...)`/backtick/heredoc depths; splits only at depth 0 | Likely | `docs/specs/chained-commands.md:15` lists a closed set of opaque constructs suiting a DFA; no shell-tokenize crate in workspace today; 13-scenario test matrix in spec lines 122-138 |
| Output shape `Vec<Segment { text, trailing_op }>` preserving byte-exact original text per segment | Likely | REQ-adapter-chained-commands requires preserved operators; matched-and-rewritten segments produce new shell-quoted output, unchanged segments preserve `text` verbatim |
| Pipes (`\|`) consumed verbatim into the current segment text | Confident | `docs/specs/chained-commands.md:21` "Pipes are not chain operators"; REQ-adapter-pipes-passthrough explicit |
| Alternatives considered: `shell-words` (loses operator info), `conch-parser` (~30K LoC dep on 10ms cold path), tokenize-first (loses byte-exact reassembly) | — | All rejected per evidence above |

### C. `lacon init` settings.json strategy
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Full `serde_json::Value` parse; structural insert into `hooks.PreToolUse[]` with `matcher: "Bash"`, `hooks: [{type: "command", command: "lacon-claude-hook"}]` | Likely (verified by external research) | External research 2026-05-16 against `code.claude.com/docs/en/hooks.md` lines 60-80 confirmed the array-of-matchers shape verbatim |
| Idempotency via command-string prefix fingerprint (`starts_with("lacon-claude-hook")`); NOT via `_lacon_managed: true` sibling | Likely | External research downgraded `_lacon_managed` to Medium confidence — unknown-key tolerance is permissive-but-undocumented; command-string is a natural fingerprint |
| CLAUDE.md handling: append HTML-comment marker block at EOF, detect-and-replace on re-run | Confident | Markdown supports HTML comments verbatim; REQ-cli-init says "tiny instruction line"; user-trust property requires advertising `!!` + `LACON_DISABLE=1` |
| Alternatives considered: `_lacon_managed` sibling (rejected on schema-tolerance grounds), side-file `.claude/.lacon-managed.json` (adds sync surface), full overwrite (destroys user hooks) | — | All rejected per evidence above |

### D. TUI heuristic
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| `is_tui` in `crates/lacon-adapter-claudecode/src/tui.rs`, NOT in `lacon-core` | Confident | `docs/specs/chained-commands.md:104` is explicit: "list lives in adapter code, not in user config" |
| Hardcoded `const PURE_TUI: &[&str]` for the 22 basenames; `match` dispatch for conditional patterns | Confident | Spec table at lines 85-101 is small and closed; `const` table fastest on cold-start path |
| Tests in `tests/chain_split.rs` (13 scenarios) + `tests/tui_heuristic.rs` (per-row coverage) + `tests/hook_e2e.rs` (end-to-end) + CLI's `cli_init.rs` | Confident | Mirrors Phase 1/2 test layout pattern; `assert_cmd::Command::cargo_bin` already used for binary lookup |

### E. `rewrite` block application & argv quoting
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| `apply_rewrite(argv, rewrite) -> Vec<String>` in `lacon-core` (new `crates/lacon-core/src/rules/rewrite.rs`) — idempotent `add_flags`, filter `remove_flags`, remap `replace_flags`, `argv[0]` untouched | Confident | `RewriteSpec` already in `crates/lacon-core/src/rules/schema.rs:119-133`; `docs/specs/filter-rule-schema.md:92` says "`add_flags` is idempotent"; Phase 1's Runner deliberately skipped applying rewrite per Phase 1 D-14 |
| `quote_for_shell(arg) -> Cow<str>` in adapter — single-quote wrap any arg with whitespace/metachars; `'\''` for embedded single quotes | Likely | POSIX-portable; Phase 1's Runner enforces no-re-shell-interpret per `crates/lacon-core/src/runtime/mod.rs:138-141` so only ONE shell-parse hop downstream; edge cases (control chars, locale-specific shells) exist |
| Adapter emits `lacon run --rule <id> -- <quoted argv>`; unchanged segments preserve original `text` byte-exact | Confident | `docs/architecture.md:73` "adapter applies rewrite before wrapping" |
| Alternatives considered: all-in-adapter (duplicates if v2 adapter), `shlex::quote` dep (100 LoC overkill for one function), skip quoting (rejected — produces wrong output for args with whitespace) | — | All rejected per evidence above |

### F. CLAUDE.md instruction line
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Append `<!-- lacon:start --> ... <!-- lacon:end -->` block at EOF (or create CLAUDE.md if missing); single-sentence body mentioning lacon + `!!` + `LACON_DISABLE=1` | Confident | Markdown supports HTML comments; REQ-cli-init says "tiny" line; user-trust property requires advertising the escape hatches |
| Alternatives considered: top-insert (disrupts user's leading instructions), skip-if-absent (loses user-trust property), single-line append no marker (can't reliably detect on re-run) | — | All rejected per evidence above |

### G. Bypass detection
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| `!!` prefix detection on `tool_input.command` (lstrip whitespace, `starts_with("!!")`) → exit 0 empty stdout | Confident | REQ-adapter-bypass-detection explicit; `docs/specs/chained-commands.md:62-66` whole-command bypass |
| `LACON_DISABLE=1` from process env (exact string match) → exit 0 empty stdout; other values (empty, "0", "true") do NOT bypass | Confident | Phase 1 precedent at `crates/lacon-core/src/runtime/mod.rs:157` mirrors this exact behaviour |
| Bypass is whole-command granularity — no chain splitting, no resolution, no rewrites on bypass detect | Confident | `docs/specs/chained-commands.md:62-66` + REQ-engine-bypass |

### H. Env-var contract handoff (Phase 2 integration)
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Adapter prepends `LACON_ASSISTANT=claude-code LACON_SESSION_ID=<id>` to rewritten command (inline env transport, not adapter-process env export) | Confident | Each Claude Code shell invocation is fresh `bash -c`, no inherited adapter env; Phase 2 D-17 mandates this contract |
| `LACON_TOOL_USE_ID=<id>` is Claude's discretion — adds tracker correlation property but not strictly required for Phase 3 | Unclear | Helpful for Phase 4 `lacon explain` correlation; adds JSON-parse cost on every hook invocation (`tool_use_id` from stdin) |

### I. Idempotency resolution (Q-deferred-init-idempotency)
| Assumption | Confidence | Evidence |
|------------|-----------|----------|
| Settled as: detect lacon-managed entries by command-string prefix; strip + re-insert on re-run; user-authored hooks preserved untouched | Confident | `docs/open-questions.md:23-31` deferred-to-prototyping likely answer translated to JSON-valid form per external research |

## External Research

External research agent spawned 2026-05-16 with 4 questions on Claude Code's hook schema. Findings against `code.claude.com/docs/en/hooks.md` and `code.claude.com/docs/en/settings.md`:

- **`.claude/settings.json` `hooks.PreToolUse[]` schema:** array-of-matchers shape with nested array-of-runners. Confirmed verbatim from doc lines 60-80. Source: `https://code.claude.com/docs/en/hooks.md`. High confidence. Confirms Area C/D-11.
- **Hook stdin JSON payload:** top-level `{session_id, transcript_path, cwd, permission_mode, hook_event_name, tool_name, tool_input, tool_use_id}`. Bash fields: `tool_input.{command, description, timeout, run_in_background}`. Source: same doc, "Common input fields" + "PreToolUse > Bash" table at line 1139. High confidence. Confirms Area A/D-03 stdin shape.
- **Hook stdout schema:** `{hookSpecificOutput: {hookEventName: "PreToolUse", permissionDecision: "allow", updatedInput: {<full replacement>}}}`. `updatedInput` REPLACES the entire input object — verbatim quote: "Replaces the entire input object, so include unchanged fields alongside modified ones." Pass-through = `exit 0` with empty stdout. Source: same doc, "PreToolUse decision control" at line 1271. High confidence. Confirms Area A/D-03 stdout shape and elevates ADR-0013's empirical claim to doc-confirmed contract.
- **Unknown-key tolerance:** permissive but undocumented. Recommendation from research: use the `command` string itself as the idempotency fingerprint instead of `_lacon_managed: true`. Medium confidence. Drove the Area C/D-12 decision against the sibling-marker approach.

Research artifacts: response captured inline in this discussion log under "Critical implementation notes" in CONTEXT.md `<specifics>` section. The verbatim doc quote on `updatedInput`'s replacement semantics is recommended for addition to `docs/decisions/0013-filter-via-pretooluse-wrapper.md` (planner's call).

## Corrections Made

No corrections — all assumptions confirmed on first pass via "Yes, proceed".
