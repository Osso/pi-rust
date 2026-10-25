#[path = "../examples/extensions/reload_runtime.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;

#[tokio::test]
async fn command_reloads_but_tool_only_queues_followup() {
    let actual = harness::compare(
        "reload-runtime",
        harness::factory(example::reload_runtime_extension),
        json!({"steps":[
        {"op":"command","name":"reload-runtime"},
        {"op":"tool","name":"reload_runtime","params":{}}
        ]}),
    )
    .await;
    assert_eq!(actual["trace"][0], json!(["reload"]));
    assert_eq!(
        actual["trace"][1],
        json!(["sendUserMessage","/reload-runtime",{"deliverAs":"followUp"}])
    );
    assert_eq!(actual["trace"].as_array().unwrap().len(), 3);
}
#[tokio::test]
async fn queue_failure_propagates_without_success_result() {
    let actual = harness::compare(
        "reload-runtime",
        harness::factory(example::reload_runtime_extension),
        json!({"sendError":"queue closed","steps":[
        {"op":"tool","name":"reload_runtime","params":{}}
        ]}),
    )
    .await;
    assert_eq!(actual["trace"], json!([["error", "queue closed"]]));
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
