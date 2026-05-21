//! End-to-end tests for the `lacon-claude-hook` binary.
//!
//! Spawn the compiled binary with `assert_cmd`, feed a `PreToolUse` JSON payload
//! on stdin, and assert on stdout/exit. These lock the orchestration matrix that
//! Plan 03-04 wired together: pass-through, `!!`/`LACON_DISABLE` bypass, single +
//! chain rewrite, whole-chain TUI bypass, the D-03 echo-back contract, the D-26
//! env-var prefix (Phase 2 tracker integration), pipe-passthrough, and the
//! non-Bash defensive guard.
//!
//! Each test uses its own `tempdir()` for `.lacon/rules/` and sets the payload's
//! `cwd` to that dir so the hook's `RuleLoader::new(Some(cwd))` finds the rule —
//! avoiding cross-test contamination. Shape assertions parse stdout as
//! `serde_json::Value` (not fragile substring matching on raw JSON).

use std::fs;
use std::path::Path;
use std::process::Output;

use assert_cmd::Command;
use serde_json::Value;

/// Spawn the hook binary, write `input_json` to stdin, capture output.
fn run_hook_with_input(input_json: &str) -> Output {
    Command::cargo_bin("lacon-claude-hook")
        .unwrap()
        .write_stdin(input_json)
        .output()
        .expect("hook binary runs")
}

/// Same, but with extra environment variables (for the `LACON_DISABLE` test).
fn run_hook_with_input_and_env(input_json: &str, env: &[(&str, &str)]) -> Output {
    let mut cmd = Command::cargo_bin("lacon-claude-hook").unwrap();
    for (k, v) in env {
        cmd.env(k, v);
    }
    cmd.write_stdin(input_json).output().expect("hook binary runs")
}

/// Write a project rule to `<dir>/.lacon/rules/test.yaml`
/// (mirrors `crates/lacon-cli/tests/cli_run.rs:9-13`).
fn write_rule(dir: &Path, rule_yaml: &str) {
    let rules_dir = dir.join(".lacon").join("rules");
    fs::create_dir_all(&rules_dir).unwrap();
    fs::write(rules_dir.join("test.yaml"), rule_yaml).unwrap();
}

/// Build a Bash `PreToolUse` payload pointing `cwd` at `cwd`.
fn bash_payload(cwd: &str, command: &str) -> String {
    serde_json::json!({
        "session_id": "s1",
        "transcript_path": "/t",
        "cwd": cwd,
        "permission_mode": "default",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": command },
        "tool_use_id": "u1"
    })
    .to_string()
}

/// Parse stdout bytes into the rewrite-response `updatedInput.command` string.
fn updated_command(output: &Output) -> String {
    let value: Value = serde_json::from_slice(&output.stdout)
        .expect("stdout is valid JSON on the rewrite path");
    value["hookSpecificOutput"]["updatedInput"]["command"]
        .as_str()
        .expect("updatedInput.command is a string")
        .to_owned()
}

const ECHO_RULE: &str = r#"
id: echo-rule
match: { command: echo }
pipeline:
  - strip_ansi
"#;

const LS_RULE: &str = r#"
id: ls-rule
match: { command: ls }
pipeline:
  - strip_ansi
"#;

/// REQ-adapter-pretooluse-only (base behavior): an unmatched command produces an
/// empty stdout + exit 0 (the cheapest pass-through hot path).
#[test]
fn pass_through_unmatched_command_exits_zero_empty_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let payload = bash_payload(&dir.path().to_string_lossy(), "echo hello");
    let output = run_hook_with_input(&payload);
    assert!(output.status.success(), "exit 0 expected");
    assert!(output.stdout.is_empty(), "empty stdout expected, got {:?}", output.stdout);
}

/// REQ-adapter-pretooluse-only, T-03-04-01 (T-hook-output-shape): a matched
/// command emits the locked `hookSpecificOutput` JSON shape.
#[test]
fn matched_single_command_emits_rewrite_json() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE);
    let payload = bash_payload(&dir.path().to_string_lossy(), "echo hi");
    let output = run_hook_with_input(&payload);
    assert!(output.status.success());

    let value: Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(
        value["hookSpecificOutput"]["hookEventName"].as_str(),
        Some("PreToolUse")
    );
    assert_eq!(
        value["hookSpecificOutput"]["permissionDecision"].as_str(),
        Some("allow")
    );
    let cmd = value["hookSpecificOutput"]["updatedInput"]["command"]
        .as_str()
        .unwrap();
    assert!(
        cmd.contains("lacon run --rule echo-rule"),
        "expected wrapped command, got: {cmd}"
    );
}

/// D-03 echo-back property, T-03-04-05 (T-hook-output-shape): description /
/// timeout / run_in_background round-trip unchanged in updatedInput.
#[test]
fn description_and_timeout_echoed_back_in_updated_input() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE);
    let payload = serde_json::json!({
        "session_id": "s1",
        "transcript_path": "/t",
        "cwd": dir.path().to_string_lossy(),
        "permission_mode": "default",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {
            "command": "echo hi",
            "description": "say hi",
            "timeout": 30000,
            "run_in_background": false
        },
        "tool_use_id": "u1"
    })
    .to_string();

    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    let ui = &value["hookSpecificOutput"]["updatedInput"];
    assert_eq!(ui["description"].as_str(), Some("say hi"));
    assert_eq!(ui["timeout"].as_u64(), Some(30000));
    assert_eq!(ui["run_in_background"].as_bool(), Some(false));
}

/// REQ-adapter-chained-commands, T-03-04-04 (byte-exact reassembly): one matched
/// segment is wrapped while the unmatched second segment + operator survive
/// byte-exact.
#[test]
fn chain_with_one_matched_one_unmatched_emits_chain_rewrite() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE);
    let payload = bash_payload(&dir.path().to_string_lossy(), "echo hi && ls -la");
    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    let cmd = updated_command(&output);
    assert!(
        cmd.contains("lacon run --rule echo-rule -- echo hi"),
        "first segment wrapped: {cmd}"
    );
    assert!(cmd.contains(" && ls -la"), "second segment + operator preserved: {cmd}");
}

/// REQ-adapter-bypass-detection, T-03-04-02 (T-bypass-failsafe): the `!!` prefix
/// produces empty stdout + exit 0.
#[test]
fn bypass_via_bang_prefix_emits_empty_stdout() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE); // would match, but bypass wins
    let payload = bash_payload(&dir.path().to_string_lossy(), "!! echo hi");
    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    assert!(output.stdout.is_empty(), "empty stdout expected on !! bypass");
}

/// REQ-adapter-bypass-detection, T-03-04-02 (T-bypass-failsafe): LACON_DISABLE=1
/// produces empty stdout + exit 0 even when a rule would match.
#[test]
fn bypass_via_lacon_disable_env_emits_empty_stdout() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE);
    let payload = bash_payload(&dir.path().to_string_lossy(), "echo hi");
    let output = run_hook_with_input_and_env(&payload, &[("LACON_DISABLE", "1")]);
    assert!(output.status.success());
    assert!(
        output.stdout.is_empty(),
        "empty stdout expected on LACON_DISABLE=1 bypass"
    );
}

/// REQ-adapter-tui-bypass, T-03-04-03 (T-tui-whole-chain): a TUI segment anywhere
/// in the chain forces the WHOLE chain to bypass — the matched `ls` segment is
/// NOT wrapped because `vim` triggers the bypass (D-15).
#[test]
fn tui_segment_in_chain_triggers_whole_chain_bypass() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), LS_RULE);
    let payload = bash_payload(&dir.path().to_string_lossy(), "vim file && ls -la");
    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    assert!(
        output.stdout.is_empty(),
        "empty stdout expected — vim forces whole-chain bypass"
    );
}

/// D-26 + Phase 2 integration, T-03-04-07: the wrapped command carries the
/// LACON_ASSISTANT=claude-code and LACON_SESSION_ID=<id> prefix that the Phase 2
/// tracker (run.rs:270-272) consumes.
#[test]
fn rewritten_command_contains_lacon_assistant_and_session_id() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE);
    let payload = serde_json::json!({
        "session_id": "session-abc-123",
        "transcript_path": "/t",
        "cwd": dir.path().to_string_lossy(),
        "permission_mode": "default",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
        "tool_use_id": "u1"
    })
    .to_string();

    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    let cmd = updated_command(&output);
    assert!(cmd.contains("LACON_ASSISTANT=claude-code"), "assistant prefix: {cmd}");
    assert!(
        cmd.contains("LACON_SESSION_ID=session-abc-123"),
        "session id prefix: {cmd}"
    );
}

/// REQ-adapter-pipes-passthrough at the orchestration boundary: a `|` is NEVER a
/// split operator — `echo hi | grep h && ls -la` splits into 2 segments, the
/// first (a pipeline) preserved byte-exact (NOT wrapped — re-quoting `|` would
/// break the pipe; pipes are out of v1 filter scope per chained-commands.md:17),
/// the second (`ls`) wrapped. The chain reassembles byte-exact.
#[test]
fn pipe_in_segment_preserved_not_split() {
    let dir = tempfile::tempdir().unwrap();
    // Rule for `ls` so the SECOND segment matches and the chain rewrites,
    // letting us assert the first (pipe) segment travels through verbatim.
    write_rule(dir.path(), LS_RULE);
    let payload = bash_payload(&dir.path().to_string_lossy(), "echo hi | grep h && ls -la");
    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    let cmd = updated_command(&output);
    // First segment preserved byte-exact (pipe intact, NOT split, NOT wrapped).
    assert!(
        cmd.contains("echo hi | grep h"),
        "pipe segment preserved verbatim: {cmd}"
    );
    // The pipe segment is NOT wrapped (no `lacon run` prefix on it).
    assert!(
        cmd.starts_with("echo hi | grep h && "),
        "pipe segment is unwrapped and leads the chain: {cmd}"
    );
    // Second segment wrapped + operator preserved.
    assert!(
        cmd.contains("lacon run --rule ls-rule -- ls -la"),
        "ls segment wrapped: {cmd}"
    );
}

/// CR-01 regression: a matched segment containing a redirection MUST pass
/// through byte-exact (NOT wrapped). Wrapping would re-quote `>` as a literal
/// argument, dropping the redirect and silently losing the user's output file.
#[test]
fn redirection_segment_passes_through_unwrapped() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE); // would match `echo`, but redirect blocks wrap
    let payload = bash_payload(&dir.path().to_string_lossy(), "echo hi > out.txt");
    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    // No rewrite happens (nothing wrappable) → cheapest pass-through, empty stdout.
    assert!(
        output.stdout.is_empty(),
        "redirect segment must not be wrapped; expected pass-through, got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// CR-02 regression: a matched segment containing command substitution MUST pass
/// through byte-exact. Wrapping would single-quote `$(whoami)`, neutralizing the
/// substitution the user intended to execute.
#[test]
fn command_substitution_segment_passes_through_unwrapped() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE);
    let payload = bash_payload(&dir.path().to_string_lossy(), "echo $(whoami)");
    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    assert!(
        output.stdout.is_empty(),
        "command-substitution segment must not be wrapped, got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// CR-03 regression: a matched segment containing a shell comment MUST pass
/// through byte-exact. Wrapping would turn `# do thing` into literal arguments,
/// changing the program's output.
#[test]
fn comment_segment_passes_through_unwrapped() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE);
    let payload = bash_payload(&dir.path().to_string_lossy(), "echo hi # do thing");
    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    assert!(
        output.stdout.is_empty(),
        "comment segment must not be wrapped, got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// CR-04 regression: `${...}` parameter expansion with a chain operator inside
/// the braces must NOT mis-split into a broken two-command chain. The whole
/// command stays one segment and (being unwrappable) passes through byte-exact.
#[test]
fn param_expansion_with_chain_op_not_missplit() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE);
    let payload = bash_payload(&dir.path().to_string_lossy(), "echo ${x:-a && b}");
    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    // Single unwrappable segment → pass-through. Crucially: stdout must NOT
    // contain a `b}` second top-level command (the old mis-split bug).
    assert!(
        output.stdout.is_empty(),
        "param-expansion command must not be mis-split/wrapped, got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// WR-02 regression: backslash-escaped whitespace forms one argument in bash;
/// the segment must pass through byte-exact rather than be re-tokenized into two.
#[test]
fn escaped_whitespace_segment_passes_through_unwrapped() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE);
    let payload = bash_payload(&dir.path().to_string_lossy(), "echo a\\ b");
    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    assert!(
        output.stdout.is_empty(),
        "escaped-whitespace segment must not be wrapped, got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// CR-01 (iteration 2) regression: a matched segment whose only special content
/// is a BARE variable expansion (`$HOME`, `$1`) MUST pass through byte-exact.
/// Wrapping single-quotes `$HOME` → `'$HOME'`, so `lacon run` (no shell hop)
/// would print the literal token instead of the home dir / positional arg the
/// user intended. (iter-1 wrongly wrapped these.)
#[test]
fn bare_variable_expansion_segment_passes_through_unwrapped() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE); // matches `echo`, but `$VAR` blocks wrap
    for command in ["echo $HOME", "echo $1"] {
        let payload = bash_payload(&dir.path().to_string_lossy(), command);
        let output = run_hook_with_input(&payload);
        assert!(output.status.success());
        assert!(
            output.stdout.is_empty(),
            "variable-expansion segment {command:?} must not be wrapped, got {:?}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

/// CR-01 (iteration 2) regression: a matched segment containing an unquoted glob
/// (`*.txt`) MUST pass through byte-exact. Wrapping single-quotes the glob, so
/// `lacon run` would receive the literal `*.txt` instead of bash's pathname
/// expansion.
#[test]
fn glob_segment_passes_through_unwrapped() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE);
    let payload = bash_payload(&dir.path().to_string_lossy(), "echo *.txt");
    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    assert!(
        output.stdout.is_empty(),
        "glob segment must not be wrapped, got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// CR-01 (iteration 2) regression: a matched segment containing a leading tilde
/// (`~`) MUST pass through byte-exact. Wrapping single-quotes `~`, so `lacon run`
/// would receive the literal `~` instead of bash's home-directory expansion.
#[test]
fn tilde_segment_passes_through_unwrapped() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE);
    let payload = bash_payload(&dir.path().to_string_lossy(), "echo ~");
    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    assert!(
        output.stdout.is_empty(),
        "tilde segment must not be wrapped, got {:?}",
        String::from_utf8_lossy(&output.stdout)
    );
}

/// CR-01 (iteration 4, allowlist root-cause fix): brace expansion `{a,b}` was the
/// case the iteration-3 denylist still missed — `eslint src/{a,b}.js` would be
/// wrapped and silently corrupted into the literal `'src/{a,b}.js'`. Under the
/// `is_wrap_safe` allowlist, `{` / `}` are not safe-literal bytes, so the segment
/// is NOT wrap-safe and passes through byte-exact (cheapest pass-through: empty
/// stdout, no rewrite).
#[test]
fn brace_expansion_segment_passes_through_unwrapped() {
    let dir = tempfile::tempdir().unwrap();
    // Rule matches `eslint`/`cargo` (command basename) but brace expansion blocks
    // the wrap; assert the hook does NOT rewrite (byte-exact pass-through).
    write_rule(
        dir.path(),
        r#"
id: eslint-rule
match: { command: eslint }
pipeline:
  - strip_ansi
"#,
    );
    for command in ["eslint src/{a,b}.js", "eslint {1..10}.js"] {
        let payload = bash_payload(&dir.path().to_string_lossy(), command);
        let output = run_hook_with_input(&payload);
        assert!(output.status.success());
        assert!(
            output.stdout.is_empty(),
            "brace-expansion segment {command:?} must not be wrapped, got {:?}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

/// CR-01 (iteration 4): brace expansion in a chain segment passes through while a
/// sibling plain segment is still wrapped — proving the allowlist gate is
/// per-segment. `cargo build {a,b}` (brace expansion → unsafe) is preserved
/// byte-exact; the trailing `echo done` (plain → wrap-safe) is wrapped.
#[test]
fn brace_expansion_segment_preserved_while_sibling_wrapped() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE);
    let payload = bash_payload(
        &dir.path().to_string_lossy(),
        "cargo build {a,b} && echo done",
    );
    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    let cmd = updated_command(&output);
    // First (brace-expansion) segment preserved byte-exact, NOT wrapped.
    assert!(
        cmd.starts_with("cargo build {a,b} && "),
        "brace-expansion segment preserved verbatim and unwrapped: {cmd}"
    );
    // Second segment wrapped.
    assert!(
        cmd.contains("lacon run --rule echo-rule -- echo done"),
        "second segment wrapped: {cmd}"
    );
}

/// CR-01..CR-04 boundary: an unwrappable segment in a chain is preserved
/// byte-exact while a sibling matched segment is still wrapped — proving the
/// guard is per-segment, not whole-chain.
#[test]
fn unwrappable_segment_preserved_while_sibling_wrapped() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE);
    let payload = bash_payload(
        &dir.path().to_string_lossy(),
        "echo hi > out.txt && echo done",
    );
    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    let cmd = updated_command(&output);
    // First (redirect) segment preserved byte-exact, NOT wrapped.
    assert!(
        cmd.starts_with("echo hi > out.txt && "),
        "redirect segment preserved verbatim and unwrapped: {cmd}"
    );
    // Second segment wrapped.
    assert!(
        cmd.contains("lacon run --rule echo-rule -- echo done"),
        "second segment wrapped: {cmd}"
    );
}

/// RESEARCH Q4 RESOLVED 2026-05-16: a non-Bash tool name passes through (defensive
/// guard for a matcher-widened settings.json). HookInput does not
/// `deny_unknown_fields`, so a different `tool_input` shape still parses as long
/// as `command` is present.
#[test]
fn non_bash_tool_passes_through() {
    let dir = tempfile::tempdir().unwrap();
    // tool_input still needs a `command` field for the typed BashToolInput to
    // deserialize; the hook returns PassThrough before ever reading it.
    let payload = serde_json::json!({
        "session_id": "s1",
        "transcript_path": "/t",
        "cwd": dir.path().to_string_lossy(),
        "permission_mode": "default",
        "hook_event_name": "PreToolUse",
        "tool_name": "Write",
        "tool_input": { "command": "echo hi", "file_path": "/tmp/x", "content": "hi" },
        "tool_use_id": "u1"
    })
    .to_string();

    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    assert!(
        output.stdout.is_empty(),
        "empty stdout expected for non-Bash tool"
    );
}

/// RESEARCH Q3 RESOLVED 2026-05-16: the wrapped command carries
/// LACON_TOOL_USE_ID=<id> for Phase 4 cross-correlation (D-26 extended).
#[test]
fn rewritten_command_contains_lacon_tool_use_id() {
    let dir = tempfile::tempdir().unwrap();
    write_rule(dir.path(), ECHO_RULE);
    let payload = serde_json::json!({
        "session_id": "session-abc-123",
        "transcript_path": "/t",
        "cwd": dir.path().to_string_lossy(),
        "permission_mode": "default",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": { "command": "echo hi" },
        "tool_use_id": "toolu_01abc"
    })
    .to_string();

    let output = run_hook_with_input(&payload);
    assert!(output.status.success());
    let cmd = updated_command(&output);
    assert!(
        cmd.contains("LACON_TOOL_USE_ID=toolu_01abc"),
        "tool_use_id prefix: {cmd}"
    );
}
