#[path = "../examples/extensions/permission_gate.rs"]
mod example;
use crate::shared_extensions as extension_support;
use crate::shared_oracle as oracle;
#[path = "support/extension_examples_b.rs"]
mod support;
use pi_coding_agent::core::extensions::*;
use serde_json::{Value, json};
use std::sync::Arc;

async fn compare(cases: Vec<Value>) -> Value {
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::permission_gate_extension(api);
        Box::pin(async move { result })
    });
    support::compare("permission-gate", factory, json!({"cases":cases})).await
}

#[tokio::test]
async fn pinned_example_matches_runtime_effects() {
    let mut cases = Vec::new();
    for (tool, command, has_ui, selected) in [
        ("read", "sudo", true, None),
        ("bash", "echo safe", true, None),
        ("bash", "rm -rf /private", false, None),
        ("bash", "SUDO whoami", true, Some("Yes")),
        ("bash", "chmod 777 file", true, Some("No")),
        ("bash", "chown user 777file", true, None),
        ("bash", "rm -r foo", true, Some("yes")),
        ("bash", "rm --recursive foo", true, Some("Yes")),
        ("bash", "rm -fr foo", true, None),
        ("bash", "éSuDoé echo", true, None),
        ("bash", "chmod\n777 file", true, None),
        ("bash", "rm\u{feff}-rf file", true, None),
    ] {
        cases.push(
            json!({"event":"tool_call","toolName":tool,"input":{"command":command},"hasUI":has_ui,"selected":selected}),
        );
    }
    let actual = compare(cases).await;
    assert_eq!(actual["metadata"], json!({"commands":[],"tools":[]}));
    assert_eq!(
        actual["cases"][2]["result"],
        json!({"block":true,"reason":"Dangerous command blocked (no UI for confirmation)"})
    );
    assert_eq!(actual["cases"][3]["result"], Value::Null);
    assert_eq!(
        actual["cases"][4]["result"],
        json!({"block":true,"reason":"Blocked by user"})
    );
}

#[tokio::test]
async fn javascript_whitespace_casefold_and_dot_boundaries_match() {
    let mut cases = Vec::new();
    for whitespace in [
        "\t", "\n", "\r", "\u{b}", "\u{c}", " ", "\u{a0}", "\u{1680}", "\u{2000}", "\u{200a}", "\u{2028}", "\u{2029}",
        "\u{202f}", "\u{205f}", "\u{3000}", "\u{feff}", "\u{85}", "\u{200b}",
    ] {
        cases.push(json!({"event":"tool_call","toolName":"bash","hasUI":false,"input":{"command":format!("rm{whitespace}-rf file")}}));
    }
    for command in [
        "ſudo echo",
        "sudoX",
        "_sudo",
        "chmod\r777 file",
        "chmod\u{2028}777 file",
        "chown\u{2029}777 file",
        "chmod xxx777file",
        "rm -rfoo",
    ] {
        cases.push(json!({"event":"tool_call","toolName":"bash","hasUI":false,"input":{"command":command}}));
    }
    let actual = compare(cases).await;
    assert_eq!(
        actual["cases"][16]["result"],
        Value::Null,
        "JS excludes U+0085 from whitespace"
    );
    assert_eq!(
        actual["cases"][18]["result"],
        Value::Null,
        "non-Unicode /i does not fold long s"
    );
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
#[path = "support/ui_thread.rs"]
pub mod shared_ui_thread;
