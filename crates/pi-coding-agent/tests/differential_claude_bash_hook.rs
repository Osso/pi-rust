//! Differential tests for the bundled `claude-bash-hook` extension.

#[path = "support/oracle.rs"]
mod oracle;

use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use oracle::TsOracle;
use pi_coding_agent::bundled_extensions::claude_bash_hook::{
    ClaudeBashHookOptions, ClaudeBashHookReviewResult, review_tool_with_claude_bash_hook,
};
use pi_coding_agent::core::extensions::types::{ToolCallEvent, ToolName};
use serde_json::{Value, json};

const SOURCE: &str = "packages/coding-agent/extensions/claude-bash-hook/src/index.ts";

fn result_value(result: ClaudeBashHookReviewResult) -> Value {
    match result {
        ClaudeBashHookReviewResult::Allow { updated_input } => match updated_input {
            Some(updated_input) => json!({"action": "allow", "updatedInput": updated_input}),
            None => json!({"action": "allow"}),
        },
        ClaudeBashHookReviewResult::Ask { reason } => json!({"action": "ask", "reason": reason}),
        ClaudeBashHookReviewResult::Deny { reason } => json!({"action": "deny", "reason": reason}),
        ClaudeBashHookReviewResult::Unavailable => json!({"action": "unavailable"}),
    }
}

fn write_script(dir: &std::path::Path, name: &str, output: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\nprintf '%s' '{output}'\n")).expect("script");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

#[tokio::test]
async fn differential_claude_bash_hook() {
    let cases = vec![
        json!({"name": "allow", "toolName": "pyrun_eval", "input": {"code": "print(1 + 1)"}, "output": r#"{"hookSpecificOutput":{"permissionDecision":"allow","permissionDecisionReason":"ok"}}"#}),
        json!({"name": "allow-updated", "toolName": "bash", "input": {"command": "ls"}, "output": r#"{"hookSpecificOutput":{"permissionDecision":"allow","updatedInput":{"command":"ls -la"}}}"#}),
        json!({"name": "deny", "toolName": "bash", "input": {"command": "rm -rf /"}, "output": r#"{"hookSpecificOutput":{"permissionDecision":"deny","permissionDecisionReason":"nope"}}"#}),
        json!({"name": "block-default", "toolName": "bash", "input": {"command": "ls"}, "output": r#"{"hookSpecificOutput":{"permissionDecision":"block","permissionDecisionReason":"   "}}"#}),
        json!({"name": "ask", "toolName": "bash", "input": {"command": "ls"}, "output": r#"{"hookSpecificOutput":{"permissionDecision":"ask","permissionDecisionReason":"confirm?"}}"#}),
        json!({"name": "unknown-decision", "toolName": "bash", "input": {"command": "ls"}, "output": r#"{"hookSpecificOutput":{"permissionDecision":"maybe"}}"#}),
        json!({"name": "malformed", "toolName": "bash", "input": {"command": "ls"}, "output": "not json"}),
        json!({"name": "empty-output", "toolName": "bash", "input": {"command": "ls"}, "output": ""}),
        json!({"name": "non-reviewable-tool", "toolName": "read", "input": {"path": "x"}, "output": r#"{"hookSpecificOutput":{"permissionDecision":"allow"}}"#}),
        json!({"name": "empty-command", "toolName": "bash", "input": {"command": ""}, "output": r#"{"hookSpecificOutput":{"permissionDecision":"allow"}}"#}),
        json!({"name": "missing-code", "toolName": "pyrun_eval", "input": {}, "output": r#"{"hookSpecificOutput":{"permissionDecision":"allow"}}"#}),
    ];

    let mut ts = TsOracle::spawn("claude-bash-hook-oracle.ts", SOURCE);
    let response = ts.request(&json!({"id": 1, "operation": "cases", "cases": cases}));
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    let expected: Vec<Value> = serde_json::from_value(response["output"]["results"].clone()).expect("oracle results");

    let dir = tempfile::tempdir().expect("temp dir");
    let mut actual = Vec::new();
    for (index, case) in cases.iter().enumerate() {
        let script = write_script(
            dir.path(),
            &format!("hook-{index}.sh"),
            case.get("output").and_then(Value::as_str).unwrap_or_default(),
        );
        let event = ToolCallEvent {
            tool_name: ToolName::parse(case["toolName"].as_str().expect("tool name")),
            tool_call_id: "test-call".to_owned(),
            bypass_permissions: false,
            input: case["input"].as_object().cloned().expect("input object"),
        };
        let result = review_tool_with_claude_bash_hook(
            &event,
            "/repo",
            &ClaudeBashHookOptions {
                command: Some(script.display().to_string()),
                timeout_ms: None,
            },
        )
        .await
        .expect("review");
        actual.push(json!({
            "name": case["name"],
            "result": result_value(result),
        }));
    }

    assert_eq!(actual, expected);
}
