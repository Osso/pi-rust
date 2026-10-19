//! Behavioral tests for the bundled `claude-bash-hook` extension.

#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/executable_script.rs"]
mod executable_script;
#[path = "support/extensions.rs"]
mod support;

use std::path::{Path, PathBuf};

use pi_coding_agent::bundled_extensions::claude_bash_hook::{
    ClaudeBashHookOptions, ClaudeBashHookReviewResult, claude_bash_hook_extension, review_tool_with_claude_bash_hook,
};
use pi_coding_agent::core::extensions::types::{ApprovalReviewerResult, ToolCallEvent, ToolName};
use pi_coding_agent::core::extensions::{Extension, create_extension_runtime};
use serde_json::{Value, json};

struct TestDir {
    dir: tempfile::TempDir,
}

impl TestDir {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("temp dir"),
        }
    }

    fn path(&self) -> &Path {
        self.dir.path()
    }

    fn script(&self, name: &str, output: &str) -> PathBuf {
        self.script_with_capture(name, output, None)
    }

    fn script_with_capture(&self, name: &str, output: &str, capture: Option<&Path>) -> PathBuf {
        let path = self.path().join(name);
        let capture_line = capture
            .map(|capture| format!("cat > '{}'\n", capture.display()))
            .unwrap_or_default();
        let body = format!("#!/bin/sh\n{capture_line}printf '%s' '{output}'\n");
        executable_script::write_executable_script(&path, &body);
        path
    }
}

fn event(tool_name: &str, input: Value) -> ToolCallEvent {
    ToolCallEvent {
        tool_name: ToolName::parse(tool_name),
        tool_call_id: "test-call".to_owned(),
        bypass_permissions: false,
        input: input.as_object().cloned().expect("input object"),
    }
}

fn options(command: &Path) -> ClaudeBashHookOptions {
    ClaudeBashHookOptions {
        command: Some(command.display().to_string()),
        timeout_ms: None,
    }
}

#[tokio::test]
async fn reviews_pyrun_eval_with_the_hook() {
    let dir = TestDir::new();
    let capture = dir.path().join("input.json");
    let script = dir.script_with_capture(
        "allow.sh",
        r#"{"hookSpecificOutput":{"permissionDecision":"allow","permissionDecisionReason":"ok"}}"#,
        Some(&capture),
    );

    let result = review_tool_with_claude_bash_hook(
        &event("pyrun_eval", json!({"code": "print(1 + 1)"})),
        "/repo",
        &options(&script),
    )
    .await
    .expect("review");

    assert_eq!(result, ClaudeBashHookReviewResult::Allow { updated_input: None });
    let payload: Value =
        serde_json::from_str(&std::fs::read_to_string(capture).expect("captured input")).expect("json");
    assert_eq!(
        payload,
        json!({
            "access_mode": "supervised",
            "approval_policy": "on-request",
            "cwd": "/repo",
            "supports_updated_input": true,
            "tool_input": {"code": "print(1 + 1)", "cwd": "/repo"},
            "tool_name": "pyrun_eval"
        })
    );
}

#[tokio::test]
async fn maps_updated_input_and_bash_tool_names() {
    let dir = TestDir::new();
    let capture = dir.path().join("input.json");
    let script = dir.script_with_capture(
        "updated.sh",
        r#"{"hookSpecificOutput":{"permissionDecision":"allow","updatedInput":{"command":"ls -la"}}}"#,
        Some(&capture),
    );

    let result =
        review_tool_with_claude_bash_hook(&event("bash", json!({"command": "ls"})), "/repo", &options(&script))
            .await
            .expect("review");

    let ClaudeBashHookReviewResult::Allow {
        updated_input: Some(updated_input),
    } = result
    else {
        panic!("expected allow with updated input");
    };
    assert_eq!(updated_input.get("command").and_then(Value::as_str), Some("ls -la"));
    let payload: Value =
        serde_json::from_str(&std::fs::read_to_string(capture).expect("captured input")).expect("json");
    assert_eq!(payload["tool_name"], "Bash");
    assert_eq!(payload["tool_input"]["cwd"], "/repo");
}

#[tokio::test]
async fn maps_deny_block_and_ask_decisions() {
    let dir = TestDir::new();
    for (name, output, expected) in [
        (
            "deny.sh",
            r#"{"hookSpecificOutput":{"permissionDecision":"deny","permissionDecisionReason":"nope"}}"#,
            ClaudeBashHookReviewResult::Deny {
                reason: "nope".to_owned(),
            },
        ),
        (
            "block.sh",
            r#"{"hookSpecificOutput":{"permissionDecision":"block"}}"#,
            ClaudeBashHookReviewResult::Deny {
                reason: "Blocked by claude-bash-hook".to_owned(),
            },
        ),
        (
            "ask.sh",
            r#"{"hookSpecificOutput":{"permissionDecision":"ask","permissionDecisionReason":"confirm?"}}"#,
            ClaudeBashHookReviewResult::Ask {
                reason: "confirm?".to_owned(),
            },
        ),
        (
            "unknown.sh",
            r#"{"hookSpecificOutput":{"permissionDecision":"maybe"}}"#,
            ClaudeBashHookReviewResult::Unavailable,
        ),
    ] {
        let script = dir.script(name, output);
        let result =
            review_tool_with_claude_bash_hook(&event("bash", json!({"command": "ls"})), "/repo", &options(&script))
                .await
                .expect("review");
        assert_eq!(result, expected);
    }
}

#[tokio::test]
async fn ignores_malformed_or_missing_hook_output() {
    let dir = TestDir::new();
    for (name, output) in [
        ("empty.sh", ""),
        ("garbage.sh", "not json"),
        ("array.sh", "[]"),
        ("no-specific.sh", r#"{"other":true}"#),
    ] {
        let script = dir.script(name, output);
        let result =
            review_tool_with_claude_bash_hook(&event("bash", json!({"command": "ls"})), "/repo", &options(&script))
                .await
                .expect("review");
        assert_eq!(result, ClaudeBashHookReviewResult::Unavailable);
    }
}

#[tokio::test]
async fn skips_unreviewable_tools_and_inputs() {
    let dir = TestDir::new();
    let capture = dir.path().join("input.json");
    let script = dir.script_with_capture(
        "allow.sh",
        r#"{"hookSpecificOutput":{"permissionDecision":"allow"}}"#,
        Some(&capture),
    );

    for (tool_name, input) in [
        ("read", json!({"path": "x"})),
        ("bash", json!({"command": ""})),
        ("bash", json!({"command": 3})),
        ("pyrun_eval", json!({})),
        ("pyrun_eval", json!({"code": ""})),
    ] {
        let result = review_tool_with_claude_bash_hook(&event(tool_name, input), "/repo", &options(&script))
            .await
            .expect("review");
        assert_eq!(result, ClaudeBashHookReviewResult::Unavailable);
    }
    assert!(!capture.exists(), "hook must not run for unreviewable calls");
}

#[tokio::test]
async fn missing_hook_binary_is_unavailable() {
    let options = ClaudeBashHookOptions {
        command: Some("/definitely/not/a/hook".to_owned()),
        timeout_ms: None,
    };
    let result = review_tool_with_claude_bash_hook(&event("bash", json!({"command": "ls"})), "/repo", &options)
        .await
        .expect("review");
    assert_eq!(result, ClaudeBashHookReviewResult::Unavailable);
}

async fn load_extension(command: &Path) -> Extension {
    let options = options(command);
    support::extension(
        &create_extension_runtime(),
        "/repo",
        "<first-party:claude-bash-hook>",
        move |pi| claude_bash_hook_extension(pi, &options).expect("claude-bash-hook extension"),
    )
    .await
}

#[tokio::test]
async fn registers_an_approval_reviewer() {
    let dir = TestDir::new();
    let script = dir.script(
        "allow.sh",
        r#"{"hookSpecificOutput":{"permissionDecision":"deny","permissionDecisionReason":"blocked"}}"#,
    );
    let extension = load_extension(&script).await;
    assert_eq!(extension.approval_reviewers.len(), 1);
    let reviewer = extension.approval_reviewers[0].clone();
    let harness = bundled::Harness::new(
        vec![extension],
        create_extension_runtime(),
        bundled::HarnessOptions::new("/repo"),
    )
    .await;

    let result = reviewer
        .call(event("bash", json!({"command": "ls"})), harness.context())
        .await
        .expect("reviewer");

    assert_eq!(
        result,
        Some(ApprovalReviewerResult::Deny {
            reason: "blocked".to_owned()
        })
    );
}
