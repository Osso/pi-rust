#[path = "../examples/extensions/permission_gate.rs"]
mod example;
#[path = "support/extension_examples_b.rs"]
mod support;
use serde_json::json;
use std::sync::Arc;
#[tokio::test]
async fn pinned_example_matches_runtime_effects() {
    let factory = Arc::new(|api: &pi_coding_agent::core::extensions::ExtensionApi| {
        let result = example::permission_gate_extension(api);
        Box::pin(async move { result }) as pi_coding_agent::core::extensions::BoxFuture<'static, _>
    });
    let actual=support::compare("permission-gate",factory,serde_json::from_str(r#"{"cases": [{"event": "tool_call", "toolName": "read", "input": {"command": "sudo"}, "hasUI": true, "selected": null}, {"event": "tool_call", "toolName": "bash", "input": {"command": "echo safe"}, "hasUI": true, "selected": null}, {"event": "tool_call", "toolName": "bash", "input": {"command": "rm -rf /private"}, "hasUI": false, "selected": null}, {"event": "tool_call", "toolName": "bash", "input": {"command": "SUDO whoami"}, "hasUI": true, "selected": "Yes"}, {"event": "tool_call", "toolName": "bash", "input": {"command": "chmod 777 file"}, "hasUI": true, "selected": "No"}, {"event": "tool_call", "toolName": "bash", "input": {"command": "chown user 777file"}, "hasUI": true, "selected": null}, {"event": "tool_call", "toolName": "bash", "input": {"command": "rm -r foo"}, "hasUI": true, "selected": "yes"}, {"event": "tool_call", "toolName": "bash", "input": {"command": "rm --recursive foo"}, "hasUI": true, "selected": "Yes"}, {"event": "tool_call", "toolName": "bash", "input": {"command": "rm -fr foo"}, "hasUI": true, "selected": null}, {"event": "tool_call", "toolName": "bash", "input": {"command": "éSuDoé echo"}, "hasUI": true, "selected": null}, {"event": "tool_call", "toolName": "bash", "input": {"command": "chmod\n777 file"}, "hasUI": true, "selected": null}, {"event": "tool_call", "toolName": "bash", "input": {"command": "rm﻿-rf file"}, "hasUI": true, "selected": null}]}"#).unwrap()).await;
    assert_eq!(actual["metadata"], json!({"commands":[],"tools":[]}));
}
