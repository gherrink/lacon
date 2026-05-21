//! Claude Code `PreToolUse(Bash)` hook stdin/stdout protocol (D-03).
//!
//! Verified against `code.claude.com/docs/en/hooks` (2026-05-16). The stdin
//! payload is parsed into typed structs so `serde_json` errors loudly on schema
//! drift (missing required field → deserialize error → hook exits non-zero →
//! Claude Code surfaces the failure). The rewrite-path response shape is locked
//! by [`build_rewrite_response`].
//!
//! # Echo-back contract (D-03)
//! `updatedInput` REPLACES the entire `tool_input` object — any field present on
//! the input (`description`, `timeout`, `run_in_background`) MUST be carried
//! through unchanged, and any field ABSENT on the input MUST stay absent (no
//! injected `null`s). [`BashToolInput`]'s `skip_serializing_if = "Option::is_none"`
//! enforces the latter; the round-trip tests below lock both directions.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `PreToolUse` hook stdin payload.
///
/// `#[serde(deny_unknown_fields)]` is deliberately NOT used — Claude Code may add
/// fields over time, and an unknown field must not break the hook (T-03-01-01).
/// Required fields are strictly typed, so a missing one is a hard parse error.
#[derive(Deserialize, Debug, Clone)]
pub struct HookInput {
    pub session_id: String,
    pub transcript_path: String,
    pub cwd: String,
    pub permission_mode: String,
    /// Always `"PreToolUse"` for our hook.
    pub hook_event_name: String,
    /// We only handle `"Bash"`.
    pub tool_name: String,
    /// Bash-tool-specific input fields.
    pub tool_input: BashToolInput,
    pub tool_use_id: String,
}

/// Bash-tool input fields.
///
/// Optional fields use `skip_serializing_if = "Option::is_none"` so the
/// echo-back into `updatedInput` never emits `"description": null` etc. when the
/// source omitted the field (T-03-01-02 — Claude Code's schema treats explicit
/// null differently from a missing key).
#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct BashToolInput {
    pub command: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_in_background: Option<bool>,
}

/// Build the rewrite-path response value (D-03).
///
/// Locks the required output shape: `hookEventName: "PreToolUse"` +
/// `permissionDecision: "allow"` + `updatedInput` (the full echoed-back, possibly
/// command-rewritten, `BashToolInput`). The caller serializes this with
/// `serde_json::to_writer(stdout, &value)`.
pub fn build_rewrite_response(updated_input: &BashToolInput) -> Value {
    serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "allow",
            "updatedInput": updated_input,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_PAYLOAD: &str = r#"{
        "session_id": "sess-1",
        "transcript_path": "/tmp/transcript.jsonl",
        "cwd": "/home/user/project",
        "permission_mode": "default",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {
            "command": "pnpm install",
            "description": "install deps",
            "timeout": 120000,
            "run_in_background": false
        },
        "tool_use_id": "toolu_abc"
    }"#;

    const MINIMAL_PAYLOAD: &str = r#"{
        "session_id": "sess-2",
        "transcript_path": "/tmp/t.jsonl",
        "cwd": "/c",
        "permission_mode": "default",
        "hook_event_name": "PreToolUse",
        "tool_name": "Bash",
        "tool_input": {
            "command": "echo hi"
        },
        "tool_use_id": "toolu_def"
    }"#;

    /// A full payload (all optional fields set) re-serializes its `tool_input`
    /// with every field preserved and equal to the input — the echo-back contract.
    #[test]
    fn deserialize_full_payload_round_trips() {
        let input: HookInput =
            serde_json::from_str(FULL_PAYLOAD).expect("full payload parses");
        assert_eq!(input.tool_input.command, "pnpm install");
        assert_eq!(input.tool_input.description.as_deref(), Some("install deps"));
        assert_eq!(input.tool_input.timeout, Some(120000));
        assert_eq!(input.tool_input.run_in_background, Some(false));

        let reser: Value = serde_json::to_value(&input.tool_input)
            .expect("tool_input re-serializes");
        let obj = reser.as_object().expect("tool_input is an object");
        assert_eq!(obj.get("command").and_then(Value::as_str), Some("pnpm install"));
        assert_eq!(obj.get("description").and_then(Value::as_str), Some("install deps"));
        assert_eq!(obj.get("timeout").and_then(Value::as_u64), Some(120000));
        assert_eq!(obj.get("run_in_background").and_then(Value::as_bool), Some(false));
        // No field lost.
        assert_eq!(obj.len(), 4, "all four tool_input fields present: {obj:?}");
    }

    /// A minimal payload (no optional fields) re-serializes WITHOUT the optional
    /// keys — proves `skip_serializing_if = "Option::is_none"` (no injected null).
    #[test]
    fn deserialize_minimal_payload_skips_optionals() {
        let input: HookInput =
            serde_json::from_str(MINIMAL_PAYLOAD).expect("minimal payload parses");
        assert_eq!(input.tool_input.command, "echo hi");
        assert!(input.tool_input.description.is_none());
        assert!(input.tool_input.timeout.is_none());
        assert!(input.tool_input.run_in_background.is_none());

        let reser: Value = serde_json::to_value(&input.tool_input)
            .expect("tool_input re-serializes");
        let obj = reser.as_object().expect("tool_input is an object");
        assert!(obj.contains_key("command"), "command present");
        assert!(!obj.contains_key("description"), "description must be ABSENT, not null");
        assert!(!obj.contains_key("timeout"), "timeout must be ABSENT, not null");
        assert!(!obj.contains_key("run_in_background"), "run_in_background must be ABSENT, not null");
        assert_eq!(obj.len(), 1, "only command present: {obj:?}");
    }

    /// The rewrite response carries the D-03-required fields with the exact values.
    #[test]
    fn build_rewrite_response_has_required_fields() {
        let updated = BashToolInput {
            command: "lacon run --rule pkg-install -- pnpm install".to_owned(),
            description: None,
            timeout: None,
            run_in_background: None,
        };
        let resp = build_rewrite_response(&updated);
        let hso = resp
            .get("hookSpecificOutput")
            .expect("hookSpecificOutput present");
        assert_eq!(
            hso.get("hookEventName").and_then(Value::as_str),
            Some("PreToolUse"),
            "hookEventName must be PreToolUse"
        );
        assert_eq!(
            hso.get("permissionDecision").and_then(Value::as_str),
            Some("allow"),
            "permissionDecision must be allow"
        );
        let updated_input = hso.get("updatedInput").expect("updatedInput present");
        assert_eq!(
            updated_input.get("command").and_then(Value::as_str),
            Some("lacon run --rule pkg-install -- pnpm install"),
            "updatedInput.command present"
        );
    }
}
