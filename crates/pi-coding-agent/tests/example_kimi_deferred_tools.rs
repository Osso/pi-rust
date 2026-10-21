#[path = "../examples/extensions/kimi_deferred_tools.rs"]
mod example;
#[path = "support/extensions.rs"]
mod extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
#[path = "support/oracle.rs"]
mod oracle;
use serde_json::json;

#[tokio::test]
async fn deferred_tools_activate_once_and_reset_on_session_start() {
    let actual = harness::compare(
        "kimi-deferred-tools",
        harness::factory(example::kimi_deferred_tools_extension),
        json!({"metadata":true,"steps":[
        {"op":"event","event":{"type":"session_start","reason":"startup"}},
        {"op":"tool","name":"tool_search","params":{"query":"weather"}},
        {"op":"tool","name":"tool_search","params":{"query":"CALCULATE 100 + 500"}},
        {"op":"tool","name":"tool_search","params":{"query":"calculator"}},
        {"op":"tool","name":"Calculator","params":{"expr":"100 + 500"}},
        {"op":"event","event":{"type":"session_start","reason":"startup"}},
        {"op":"tool","name":"tool_search","params":{"query":"calc"}}
        ]}),
    )
    .await;
    assert_eq!(actual["trace"][0], json!(["setActiveTools", ["tool_search"]]));
    assert_eq!(
        actual["trace"][2],
        json!(["setActiveTools", ["tool_search", "Calculator"]])
    );
    assert_eq!(actual["trace"][4][1]["details"]["added"], json!([]));
    assert_eq!(actual["trace"][5][1]["content"][0]["text"], "42");
}
