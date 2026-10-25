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
use pi_js_compat::js_json::{JsString, JsValue as Value};
use pi_js_compat::{js_json, js_json as json};

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
            .unwrap_or_else(|| "cat > /dev/null\n".to_owned());
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
        command: Some(command.to_str().expect("utf-8 test path").to_owned()),
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
        &JsString::from("/repo"),
        &options(&script),
    )
    .await
    .expect("review");

    assert_eq!(result, ClaudeBashHookReviewResult::Allow { updated_input: None });
    let payload: Value =
        js_json::parse_lossless(&std::fs::read_to_string(capture).expect("captured input")).expect("json");
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

    let result = review_tool_with_claude_bash_hook(
        &event("bash", json!({"command": "ls"})),
        &JsString::from("/repo"),
        &options(&script),
    )
    .await
    .expect("review");

    let ClaudeBashHookReviewResult::Allow {
        updated_input: Some(updated_input),
    } = result
    else {
        panic!("expected allow with updated input");
    };
    assert_eq!(
        updated_input.get("command").and_then(Value::as_string),
        Some(&JsString::from("ls -la"))
    );
    let payload: Value =
        js_json::parse_lossless(&std::fs::read_to_string(capture).expect("captured input")).expect("json");
    assert_eq!(payload["tool_name"], json!("Bash"));
    assert_eq!(payload["tool_input"]["cwd"], json!("/repo"));
}

#[tokio::test]
async fn hook_process_preserves_native_input_keys_and_output_reason() {
    let dir = TestDir::new();
    let capture = dir.path().join("native-input.json");
    let script = dir.script_with_capture(
        "native.sh",
        r#"{"hookSpecificOutput":{"permissionDecision":"ask","permissionDecisionReason":"ask\udfff"}}"#,
        Some(&capture),
    );
    let input =
        js_json::parse_lossless(r#"{"command":"echo raw\ud800","\ud800":"high","\udfff":"low","�":"replacement"}"#)
            .unwrap();
    let cwd = JsString::from_utf16(vec![0x2f, 0xd801]);
    let result = review_tool_with_claude_bash_hook(&event("bash", input.clone()), &cwd, &options(&script))
        .await
        .expect("native hook review");
    let expected_reason = JsString::from_utf16(vec![0x61, 0x73, 0x6b, 0xdfff]);
    assert_eq!(
        result,
        ClaudeBashHookReviewResult::Ask {
            reason: expected_reason
        }
    );
    let payload = js_json::parse_lossless(std::fs::read_to_string(capture).unwrap()).unwrap();
    assert_eq!(payload["cwd"], json!(cwd));
    let mut expected_input = input.as_object().unwrap().clone();
    expected_input.insert("cwd".into(), json!(cwd));
    assert_eq!(payload["tool_input"], Value::Object(expected_input));
}

#[tokio::test]
async fn maps_deny_block_and_ask_decisions() {
    let dir = TestDir::new();
    for (name, output, expected) in [
        (
            "deny.sh",
            r#"{"hookSpecificOutput":{"permissionDecision":"deny","permissionDecisionReason":"nope"}}"#,
            ClaudeBashHookReviewResult::Deny {
                reason: "nope".to_owned().into(),
            },
        ),
        (
            "block.sh",
            r#"{"hookSpecificOutput":{"permissionDecision":"block"}}"#,
            ClaudeBashHookReviewResult::Deny {
                reason: "Blocked by claude-bash-hook".to_owned().into(),
            },
        ),
        (
            "ask.sh",
            r#"{"hookSpecificOutput":{"permissionDecision":"ask","permissionDecisionReason":"confirm?"}}"#,
            ClaudeBashHookReviewResult::Ask {
                reason: "confirm?".to_owned().into(),
            },
        ),
        (
            "unknown.sh",
            r#"{"hookSpecificOutput":{"permissionDecision":"maybe"}}"#,
            ClaudeBashHookReviewResult::Unavailable,
        ),
    ] {
        let script = dir.script(name, output);
        let result = review_tool_with_claude_bash_hook(
            &event("bash", json!({"command": "ls"})),
            &JsString::from("/repo"),
            &options(&script),
        )
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
        let result = review_tool_with_claude_bash_hook(
            &event("bash", json!({"command": "ls"})),
            &JsString::from("/repo"),
            &options(&script),
        )
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
        let result =
            review_tool_with_claude_bash_hook(&event(tool_name, input), &JsString::from("/repo"), &options(&script))
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
    let result = review_tool_with_claude_bash_hook(
        &event("bash", json!({"command": "ls"})),
        &JsString::from("/repo"),
        &options,
    )
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
    assert_eq!(extension.read().approval_reviewers.len(), 1);
    let reviewer = extension.read().approval_reviewers[0].clone();
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
            reason: "blocked".to_owned().into()
        })
    );
}

#[tokio::test]
async fn unavailable_hook_does_not_register_a_reviewer() {
    let dir = TestDir::new();
    let extension = load_extension(&dir.path().join("missing")).await;
    assert_eq!(extension.read().approval_reviewers.len(), 0);
    let file = dir.path().join("not-executable");
    std::fs::write(&file, "not executable").unwrap();
    let extension = load_extension(&file).await;
    assert_eq!(extension.read().approval_reviewers.len(), 0);
}

#[tokio::test]
async fn hook_timeout_reports_node_abort_and_sends_sigterm() {
    let dir = TestDir::new();
    let script = dir.path().join("slow.py");
    let term = dir.path().join("term");
    executable_script::write_executable_script(
        &script,
        &format!(
            "#!/usr/bin/python3\nimport pathlib,signal,sys,time\nsignal.signal(signal.SIGTERM, lambda *args: (pathlib.Path({:?}).write_text('term'), sys.exit(0)))\nsys.stdin.read()\ntime.sleep(10)\n",
            term.to_str().unwrap()
        ),
    );
    let error = review_tool_with_claude_bash_hook(
        &event("bash", json!({"command":"ls"})),
        &JsString::from("/repo"),
        &ClaudeBashHookOptions {
            command: Some(script.to_str().unwrap().to_owned()),
            timeout_ms: Some(300),
        },
    )
    .await
    .unwrap_err();
    assert_eq!(error.message, "The operation was aborted");
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        while !term.exists() {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("SIGTERM received");
}
