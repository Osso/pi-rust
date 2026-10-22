//! Differential coverage for the `safe` and `approval-controls` bundled extensions.

#[path = "support/bundled_extension_commands.rs"]
mod command_support;
#[path = "support/bundled_extension_load.rs"]
mod load_support;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/bundled_extensions.rs"]
mod support;

use std::sync::Arc;

use pi_coding_agent::core::extensions::types::{ExtensionContext, ToolCallEvent, ToolName};
use serde_json::{Value, json};
use support::{Harness, RecordingUi, calls_json};

fn oracle_for(module: &str) -> oracle::TsOracle {
    oracle::TsOracle::spawn("bundled-extensions-oracle.ts", module)
}

fn tool_call(tool_name: &str) -> ToolCallEvent {
    ToolCallEvent {
        tool_name: ToolName::parse(tool_name),
        tool_call_id: format!("{tool_name}-call"),
        bypass_permissions: false,
        input: serde_json::Map::new(),
    }
}

async fn gate(harness: &Harness, tool_name: &str) -> Value {
    let ctx: ExtensionContext = harness.context();
    let gate = harness.extension.read().tool_gates[0].clone();
    let result = gate.call(tool_call(tool_name), ctx).await.unwrap();
    match result {
        Some(result) => json!(result),
        None => Value::Null,
    }
}

#[tokio::test]
async fn safe_matches_typescript() {
    let mut oracle = oracle_for("packages/coding-agent/extensions/safe/src/index.ts");
    let response = oracle.request(&json!({
        "id": 1,
        "extension": "safe",
        "steps": [
            {"op": "registrations"},
            {"op": "completions", "name": "safe", "prefix": "o"},
            {"op": "completions", "name": "safe", "prefix": "s"},
            {"op": "completions", "name": "safe", "prefix": "x"},
            {"op": "command", "name": "safe", "args": "on"},
            {"op": "gate", "toolName": "web_search"},
            {"op": "gate", "toolName": "ask_questions"},
            {"op": "gate", "toolName": "bash"},
            {"op": "gate", "toolName": "read"},
            {"op": "gate", "toolName": "pyrun_eval"},
            {"op": "gate", "toolName": "custom_tool"},
            {"op": "command", "name": "safe", "args": "off"},
            {"op": "gate", "toolName": "bash"},
            {"op": "command", "name": "safe", "args": "status"},
            {"op": "command", "name": "safe", "args": "bogus"}
        ]
    }));
    assert_eq!(response["ok"], true, "{response}");
    let steps = response["output"]["steps"].as_array().unwrap();

    let harness = Harness::load("/tmp", Arc::new(RecordingUi::default()), |api| {
        pi_coding_agent::bundled_extensions::safe::safe_extension(api)
    })
    .await;

    assert_eq!(harness.registrations(), steps[0]["result"]);
    let completions = harness
        .command("safe")
        .get_argument_completions
        .expect("safe completions");
    for (index, prefix) in ["o", "s", "x"].iter().enumerate() {
        let actual = serde_json::json!(completions(&pi_js_compat::js_json::JsString::from(*prefix)).unwrap());
        assert_eq!(actual, steps[index + 1]["result"], "completions {prefix}");
    }

    harness.run_command("safe", "on").await.unwrap();
    assert_eq!(calls_json(&harness.take_calls()), steps[4]["calls"]);

    for (index, tool_name) in [
        "web_search",
        "ask_questions",
        "bash",
        "read",
        "pyrun_eval",
        "custom_tool",
    ]
    .iter()
    .enumerate()
    {
        assert_eq!(
            gate(&harness, tool_name).await,
            steps[index + 5]["result"],
            "gate {tool_name}"
        );
        assert_eq!(calls_json(&harness.take_calls()), steps[index + 5]["calls"]);
    }

    harness.run_command("safe", "off").await.unwrap();
    assert_eq!(calls_json(&harness.take_calls()), steps[11]["calls"]);
    assert_eq!(gate(&harness, "bash").await, steps[12]["result"]);
    assert_eq!(calls_json(&harness.take_calls()), steps[12]["calls"]);

    harness.run_command("safe", "status").await.unwrap();
    assert_eq!(calls_json(&harness.take_calls()), steps[13]["calls"]);
    harness.run_command("safe", "bogus").await.unwrap();
    assert_eq!(calls_json(&harness.take_calls()), steps[14]["calls"]);
}

#[tokio::test]
async fn approval_controls_matches_typescript() {
    let mut oracle = oracle_for("packages/coding-agent/extensions/approval-controls/src/index.ts");
    let args = [
        "",
        "read-only session",
        "inherit session",
        "full-access global",
        "inherit project",
        "read-only",
        "bogus session",
    ];
    let mut steps = vec![json!({"op": "registrations"})];
    for arg in args {
        steps.push(json!({"op": "command", "name": "sandbox", "args": arg}));
    }
    steps.push(json!({"op": "command", "name": "approvals", "args": ""}));
    let response = oracle.request(&json!({
        "id": 2,
        "extension": "approval-controls",
        "steps": steps
    }));
    assert_eq!(response["ok"], true, "{response}");
    let steps = response["output"]["steps"].as_array().unwrap();

    let harness = Harness::load("/tmp", Arc::new(RecordingUi::default()), |api| {
        pi_coding_agent::bundled_extensions::approval_controls::approval_controls_extension(api)
    })
    .await;

    assert_eq!(harness.registrations(), steps[0]["result"]);
    for (index, arg) in args.iter().enumerate() {
        harness.run_command("sandbox", arg).await.unwrap();
        assert_eq!(
            calls_json(&harness.take_calls()),
            steps[index + 1]["calls"],
            "sandbox {arg:?}"
        );
    }
    harness.run_command("approvals", "").await.unwrap();
    assert_eq!(calls_json(&harness.take_calls()), steps[args.len() + 1]["calls"]);
}
