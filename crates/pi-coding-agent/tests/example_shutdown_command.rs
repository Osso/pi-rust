#[path = "../examples/extensions/shutdown_command.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;

#[tokio::test]
async fn shutdown_requests_follow_updates_and_precede_final_results() {
    let actual = harness::compare(
        "shutdown-command",
        harness::factory(example::shutdown_command_extension),
        json!({"updates":true,"steps":[
        {"op":"command","name":"quit"},
        {"op":"tool","name":"finish_and_exit","params":{}},
        {"op":"tool","name":"deploy_and_exit","params":{"environment":"staging🚀"}}
        ]}),
    )
    .await;
    assert_eq!(actual["trace"][0], json!(["shutdown"]));
    assert_eq!(
        actual["trace"][3],
        json!(["update",{"content":[{"type":"text","text":"Deploying to staging🚀..."}],"details":{}}])
    );
    assert_eq!(actual["trace"][5], json!(["shutdown"]));
    assert_eq!(actual["trace"][6][1]["details"], json!({"environment":"staging🚀"}));
}
#[tokio::test]
async fn deploy_without_update_callback_still_returns_result() {
    let actual = harness::compare(
        "shutdown-command",
        harness::factory(example::shutdown_command_extension),
        json!({"steps":[
        {"op":"tool","name":"deploy_and_exit","params":{"environment":""}}
        ]}),
    )
    .await;
    assert_eq!(actual["trace"].as_array().unwrap().len(), 2);
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
