//! Claude Code adapter library.
//!
//! Speaks Claude Code's `PreToolUse(Bash)` hook protocol (see [`protocol`]) and
//! drives the rewrite pipeline: bypass-detect → chain-split → TUI-bypass →
//! per-segment rule resolve → `rewrite` apply → shell-quote → `lacon run` wrap →
//! reassemble → emit. This Plan (03-04) lands the full orchestration body — every
//! algorithm it composes was built in Plans 03-01/02/03; this crate only wires.
//!
//! `lacon-core` stays assistant-agnostic — the adapter is the only crate that
//! knows Claude Code's wire format (D-01, D-02).

use std::path::PathBuf;

use lacon_core::rules::{
    apply_rewrite, match_argv_via_load_all, RuleLoader, RewriteSpec,
};

pub mod chain;
pub mod protocol;
pub mod quote;
pub mod tui;

/// The decision `run_hook` returns to the binary entry point.
///
/// - `PassThrough` — emit nothing, exit 0 (D-03 pass-through path): unmatched
///   commands, `!!`/`LACON_DISABLE=1` bypass, whole-chain TUI bypass.
/// - `Rewrite(value)` — emit the `hookSpecificOutput` JSON value on stdout.
pub enum HookOutcome {
    PassThrough,
    Rewrite(serde_json::Value),
}

/// Detect whether the whole command should bypass filtering entirely (D-23/24/25).
///
/// Two triggers, checked cheaply before any chain split or rule resolution:
/// - **D-23 `!!` prefix:** the command, after LSTRIP of leading whitespace,
///   `starts_with("!!")`.
/// - **D-24 `LACON_DISABLE=1`:** the env var equals the exact string `"1"`.
///   Mirrors the engine precedent at `crates/lacon-core/src/runtime/mod.rs:175`
///   (`as_deref() == Ok("1")`) — other values (empty, `"0"`, `"true"`) do NOT
///   bypass.
///
/// Either trigger means the entire input bypasses (D-25 — whole-command
/// granularity, the cheapest hot path).
fn detect_bypass(command: &str) -> bool {
    if command.trim_start().starts_with("!!") {
        return true;
    }
    std::env::var("LACON_DISABLE").as_deref() == Ok("1")
}

/// Tokenize a chain segment into an argv for rule resolution + TUI check (D-08,
/// revised 2026-05-16 in 03-CONTEXT.md).
///
/// Whitespace-splits the segment while respecting single + double quotes (the
/// quote bytes are dropped from the emitted tokens). Per the 2026-05-16 scope
/// reduction, `$(...)` is NOT opaquely tracked here — it is treated as part of
/// the surrounding token. The chain splitter (`chain.rs`, D-06) retains full
/// `$(...)` opacity for top-level operator detection; only this secondary
/// resolver-input tokenizer is scope-reduced. Promoting to full `$(...)` opacity
/// is deferred to v1.5+ if a real-world rule predicate needs it.
fn argv_for_resolution(text: &str) -> Vec<String> {
    let mut argv: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut has_token = false;

    for c in text.chars() {
        if in_single {
            if c == '\'' {
                in_single = false;
            } else {
                current.push(c);
            }
            continue;
        }
        if in_double {
            if c == '"' {
                in_double = false;
            } else {
                current.push(c);
            }
            continue;
        }
        match c {
            '\'' => {
                in_single = true;
                has_token = true;
            }
            '"' => {
                in_double = true;
                has_token = true;
            }
            c if c.is_whitespace() => {
                if has_token {
                    argv.push(std::mem::take(&mut current));
                    has_token = false;
                }
            }
            other => {
                current.push(other);
                has_token = true;
            }
        }
    }
    if has_token {
        argv.push(current);
    }
    argv
}

/// Drive the hook for a parsed stdin payload (full orchestration).
///
/// Pipeline (each step short-circuits to `PassThrough` on the cheapest path):
/// 0. Non-Bash pass-through (defensive — RESEARCH Q4 RESOLVED 2026-05-16).
/// 1. Bypass-detect `!!` / `LACON_DISABLE=1` (D-23/24/25).
/// 2. Chain split (Plan 02).
/// 3. Per-segment TUI check BEFORE resolve (D-15) — any TUI → whole-chain bypass.
/// 4. Per-segment resolve (one [`RuleLoader`] per invocation, D-14) → rewrite →
///    quote → wrap matched segments in the `lacon` wrapper invocation (D-21 form)
///    with the D-26 env-var prefix.
/// 5. Reassemble byte-exact via `trailing_op_span`; emit a rewrite response only
///    if at least one segment changed, else `PassThrough`.
pub fn run_hook(input: protocol::HookInput) -> anyhow::Result<HookOutcome> {
    // 0. Non-Bash pass-through (defensive guard, RESEARCH Q4 RESOLVED 2026-05-16).
    // The hook is registered under `matcher: "Bash"`, but a user could widen the
    // matcher manually; this guard keeps us from rewriting Write/Edit/Read calls.
    if input.tool_name != "Bash" {
        return Ok(HookOutcome::PassThrough);
    }

    let command = &input.tool_input.command;

    // 1. Bypass-detect (D-23/24/25): cheapest hot path — no split, no resolve.
    if detect_bypass(command) {
        return Ok(HookOutcome::PassThrough);
    }

    // 2. Chain split (Plan 02).
    let segments = crate::chain::split_chain(command);
    if segments.is_empty() {
        return Ok(HookOutcome::PassThrough);
    }

    // 3. Per-segment TUI check BEFORE resolve (D-15). Any TUI segment triggers a
    // whole-chain bypass (CON-chained-tui-bypass-whole-chain).
    for segment in &segments {
        let argv = argv_for_resolution(&segment.text);
        if !argv.is_empty() && crate::tui::is_tui(&argv[0], &argv[1..]) {
            return Ok(HookOutcome::PassThrough);
        }
    }

    // 4. Per-segment resolve + rewrite + wrap. One loader per hook invocation
    // (D-14 cache amortization).
    let mut loader = RuleLoader::new(Some(PathBuf::from(&input.cwd)));
    let default_rewrite = RewriteSpec::default();

    let mut any_rewritten = false;
    let mut rendered: Vec<String> = Vec::with_capacity(segments.len());

    for segment in &segments {
        let argv = argv_for_resolution(&segment.text);
        if argv.is_empty() {
            // Degenerate segment (e.g. trailing operator only) — keep verbatim.
            rendered.push(segment.text.clone());
            continue;
        }

        // ALLOWLIST gate (CR-01 root-cause fix): wrap a matched segment ONLY when
        // it is provably reproducible by tokenize→requote. When `run_hook` wraps a
        // segment it tokenizes it (`argv_for_resolution`), applies the rule's flag
        // rewrite, and re-quotes every token with `quote_for_shell` (single-
        // quoting), then `lacon run` executes `Command::new(argv[0]).args(...)`
        // with NO shell hop. Single-quoting neutralizes EVERY shell expansion, so
        // any segment containing an expansion / redirection / operator / comment —
        // `$VAR`, `$(...)`, backticks, `${...}`, globs `* ? [`, brace expansion
        // `{a,b}`, `~`, redirections `< >`, pipes `|`, `&`, `;`, `#`, escaped
        // whitespace `\ ` — would be silently corrupted if wrapped.
        //
        // `is_wrap_safe` is a positive allowlist: it accepts ONLY whitespace, inert
        // safe-literal bytes, single-quoted spans, and double-quoted *literal*
        // spans (no `$`/backtick/backslash). It subsumes the former separate pipe
        // guard and the `has_unwrappable_construct` denylist (which repeatedly
        // missed cases, e.g. brace expansion). A NOT-wrap-safe segment passes
        // through byte-exact so the shell still sees the real construct
        // (docs/specs/chained-commands.md:17).
        if !crate::chain::is_wrap_safe(&segment.text) {
            rendered.push(segment.text.clone());
            continue;
        }

        match match_argv_via_load_all(&mut loader, &argv) {
            Ok(Some(resolved)) => {
                // Apply the rule's rewrite block (or the no-op default).
                let rewrite = resolved.rule.rewrite.as_ref().unwrap_or(&default_rewrite);
                let rewritten_argv = apply_rewrite(&argv, rewrite);
                let quoted: Vec<String> = rewritten_argv
                    .iter()
                    .map(|a| crate::quote::quote_for_shell(a).into_owned())
                    .collect();

                // Session/tool-use IDs are untrusted fields — quote them too
                // (UUIDs in practice, but defensive against metachars).
                let session = crate::quote::quote_for_shell(&input.session_id);
                let tool_use = crate::quote::quote_for_shell(&input.tool_use_id);

                // D-26 (extended RESEARCH Q3 RESOLVED 2026-05-16) env-var prefix +
                // D-21 wrap form. LACON_ASSISTANT/LACON_SESSION_ID satisfy the
                // Phase 2 tracker contract (run.rs:270-272); LACON_TOOL_USE_ID
                // carries the cross-correlation id for Phase 4.
                let wrapped = format!(
                    "LACON_ASSISTANT=claude-code LACON_SESSION_ID={} LACON_TOOL_USE_ID={} lacon run --rule {} -- {}",
                    session,
                    tool_use,
                    resolved.id,
                    quoted.join(" ")
                );
                rendered.push(wrapped);
                any_rewritten = true;
            }
            Ok(None) => {
                // No matching rule — preserve the segment byte-exact.
                rendered.push(segment.text.clone());
            }
            Err(errors) => {
                // WR-06: a bad rule file must not break the hook (best-effort
                // pass-through is the right safety posture). But Claude Code does
                // NOT surface hook stderr in the normal flow (it captures stdout
                // as the tool result), so a stderr-only message is invisible —
                // a malformed rule would silently disable filtering with no
                // feedback. Make the failure observable by ALSO appending it to a
                // discoverable log file (`<XDG_DATA_HOME>/lacon/hook-errors.log`)
                // that `lacon doctor` / the user can inspect. This is the
                // exceptional path (only fires on a malformed rule), so it does
                // not affect the success hot path or the ≤10ms cold-start budget.
                for e in &errors {
                    eprintln!("lacon-claude-hook: rule load error: {e}");
                }
                log_rule_load_errors(&errors);
                rendered.push(segment.text.clone());
            }
        }
    }

    // 5. Reassemble. If nothing was rewritten, take the cheapest pass-through.
    if !any_rewritten {
        return Ok(HookOutcome::PassThrough);
    }

    let mut command_out = String::with_capacity(command.len());
    for (i, segment) in segments.iter().enumerate() {
        command_out.push_str(&rendered[i]);
        command_out.push_str(segment.trailing_op_span.as_deref().unwrap_or(""));
    }

    // Clone the source tool_input (echoing description/timeout/run_in_background
    // per D-03) and replace only the command with the reassembled chain.
    let mut new_input = input.tool_input.clone();
    new_input.command = command_out;
    let value = crate::protocol::build_rewrite_response(&new_input);
    Ok(HookOutcome::Rewrite(value))
}

/// Append rule-load errors to a discoverable log file so a malformed rule does
/// not degrade filtering *silently* (WR-06). Claude Code does not surface hook
/// stderr, so stderr alone is invisible; this writes to
/// `<XDG_DATA_HOME>/lacon/hook-errors.log` (sibling of the tracker DB) which the
/// user / `lacon doctor` can inspect.
///
/// Fully best-effort: any failure to resolve the path or open/write the file is
/// swallowed (a logging failure must never break the hook). Only called on the
/// exceptional malformed-rule path, so it never touches the success hot path.
fn log_rule_load_errors(errors: &[lacon_core::error::ValidationError]) {
    use std::io::Write;

    let Some(db_path) = lacon_core::tracking::Tracker::xdg_db_path() else {
        return;
    };
    let log_path = db_path.with_file_name("hook-errors.log");
    if let Some(parent) = log_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
    else {
        return;
    };
    for e in errors {
        // Best-effort: ignore individual write failures.
        let _ = writeln!(file, "rule load error: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{BashToolInput, HookInput};
    use std::sync::Mutex;

    /// `LACON_DISABLE` is process-global; cargo runs tests in parallel. Any test
    /// that sets/reads it must hold this lock so a concurrent test does not see a
    /// transient value (flaky-test fix). The guard is held for the whole body and
    /// the var is always removed before release.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn input_with(command: &str, tool_name: &str) -> HookInput {
        HookInput {
            session_id: "s1".to_owned(),
            transcript_path: "/t".to_owned(),
            cwd: "/nonexistent-cwd-for-unit-test".to_owned(),
            permission_mode: "default".to_owned(),
            hook_event_name: "PreToolUse".to_owned(),
            tool_name: tool_name.to_owned(),
            tool_input: BashToolInput {
                command: command.to_owned(),
                description: None,
                timeout: None,
                run_in_background: None,
            },
            tool_use_id: "u1".to_owned(),
        }
    }

    fn is_passthrough(outcome: &HookOutcome) -> bool {
        matches!(outcome, HookOutcome::PassThrough)
    }

    // --- run_hook bypass / pass-through unit tests (no subprocess; that's hook_e2e) ---

    #[test]
    fn empty_command_passes_through() {
        let out = run_hook(input_with("", "Bash")).unwrap();
        assert!(is_passthrough(&out));
    }

    #[test]
    fn unmatched_command_passes_through() {
        // cwd points at a nonexistent dir → load_all finds no project rules.
        let out = run_hook(input_with("echo hi", "Bash")).unwrap();
        assert!(is_passthrough(&out));
    }

    #[test]
    fn bang_bang_prefix_passes_through() {
        // Holds ENV_LOCK: `!!` bypass must fire regardless of LACON_DISABLE state,
        // but a concurrent test setting LACON_DISABLE=1 would mask the cause, so
        // pin a known-clean env for a deterministic assertion.
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        std::env::remove_var("LACON_DISABLE");
        let out = run_hook(input_with("!! pnpm test", "Bash")).unwrap();
        assert!(is_passthrough(&out));
    }

    #[test]
    fn bang_bang_prefix_with_leading_whitespace_passes_through() {
        // LSTRIP then starts_with("!!").
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        std::env::remove_var("LACON_DISABLE");
        let out = run_hook(input_with("   !! pnpm test", "Bash")).unwrap();
        assert!(is_passthrough(&out));
    }

    #[test]
    fn lacon_disable_env_passes_through() {
        // Serialize env mutation: set, run, unset (other tests must not see it).
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        std::env::set_var("LACON_DISABLE", "1");
        let out = run_hook(input_with("echo hi", "Bash")).unwrap();
        std::env::remove_var("LACON_DISABLE");
        assert!(is_passthrough(&out));
    }

    #[test]
    fn non_bash_tool_passes_through() {
        let out = run_hook(input_with("anything", "Write")).unwrap();
        assert!(is_passthrough(&out));
    }

    #[test]
    fn detect_bypass_only_exact_one_disables() {
        // D-24: only the exact string "1" bypasses.
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        for v in ["", "0", "true", "yes", "2"] {
            std::env::set_var("LACON_DISABLE", v);
            assert!(!detect_bypass("echo hi"), "value {v:?} must NOT bypass");
        }
        std::env::set_var("LACON_DISABLE", "1");
        assert!(detect_bypass("echo hi"), "value \"1\" must bypass");
        std::env::remove_var("LACON_DISABLE");
    }

    #[test]
    fn detect_bypass_bang_bang() {
        // `!!` triggers regardless of LACON_DISABLE; pin a clean env so the
        // negative cases ("ls !!", "echo hi") aren't masked by a concurrent test.
        let _g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        std::env::remove_var("LACON_DISABLE");
        assert!(detect_bypass("!! ls"));
        assert!(detect_bypass("  !!ls"));
        assert!(!detect_bypass("ls !!"));
        assert!(!detect_bypass("echo hi"));
    }

    // --- argv_for_resolution tokenizer unit tests (D-08 revised) ---

    #[test]
    fn argv_simple() {
        assert_eq!(argv_for_resolution("echo hi"), vec!["echo", "hi"]);
    }

    #[test]
    fn argv_single_quoted() {
        assert_eq!(
            argv_for_resolution("echo 'hi there'"),
            vec!["echo", "hi there"]
        );
    }

    #[test]
    fn argv_double_quoted() {
        assert_eq!(
            argv_for_resolution("echo \"hi there\""),
            vec!["echo", "hi there"]
        );
    }

    #[test]
    fn argv_empty_input() {
        assert!(argv_for_resolution("").is_empty());
        assert!(argv_for_resolution("   ").is_empty());
    }

    #[test]
    fn argv_extra_whitespace_collapsed() {
        assert_eq!(
            argv_for_resolution("  ls   -la  "),
            vec!["ls", "-la"]
        );
    }

    #[test]
    fn argv_adjacent_quote_glue() {
        // `echo a'b'c` → ["echo", "abc"] (quote bytes dropped, token glued).
        assert_eq!(argv_for_resolution("echo a'b'c"), vec!["echo", "abc"]);
    }
}
