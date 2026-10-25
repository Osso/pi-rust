#[path = "../examples/extensions/custom_footer.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::{Value, json};
fn assistant(input: f64, output: f64, cost: f64) -> Value {
    json!({"role":"assistant","content":[],"api":"faux","provider":"faux","model":"faux-1","usage":{"input":input,"output":output,"cacheRead":99,"cacheWrite":88,"totalTokens":input+output+187.0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":cost}},"stopReason":"stop","timestamp":0})
}
#[tokio::test]
async fn footer_tracks_branch_tokens_live_and_unsubscribes_on_restore() {
    let mut steps = vec![
        json!({"op":"command","name":"footer"}),
        json!({"op":"footer","width":80}),
    ];
    steps.push(json!({"op":"append","message":assistant(999.0,1000.0,0.12345)}));
    steps.push(json!({"op":"append","message":{"role":"user","content":"not counted","timestamp":0}}));
    for width in [0, 1, 12, 40, 80] {
        steps.push(json!({"op":"footer","width":width}));
    }
    steps.push(json!({"op":"append","message":assistant(2345.0,12.0,0.05)}));
    steps.push(json!({"op":"branchChange","name":"topic"}));
    steps.push(json!({"op":"footer","width":80}));
    steps.push(json!({"op":"command","name":"footer"}));
    steps.push(json!({"op":"branchChange","name":"after-dispose"}));
    steps.push(json!({"op":"command","name":"footer"}));
    steps.push(json!({"op":"footer","width":80}));
    steps.push(json!({"op":"command","name":"footer"}));
    let actual = harness::compare(
        "custom-footer",
        harness::factory(example::custom_footer_extension),
        json!({"footer":true,"branch":"main","steps":steps}),
    )
    .await;
    assert_eq!(
        actual["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|item| item[0] == "requestRender")
            .count(),
        1
    );
    assert!(actual["trace"][2][1][0].as_str().unwrap().contains("no-model (main)"));
}
#[tokio::test]
async fn footer_without_git_branch_omits_parentheses() {
    let actual = harness::compare(
        "custom-footer",
        harness::factory(example::custom_footer_extension),
        json!({"footer":true,"model":{"id":"faux-footer-1","name":"Fixture","api":"faux","provider":"faux","baseUrl":"http://invalid","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":4096,"maxTokens":128},"steps":[{"op":"command","name":"footer"},{"op":"footer","width":80}]}),
    )
    .await;
    let line = actual["trace"][2][1][0].as_str().unwrap();
    assert!(!line.contains('('));
    assert!(line.contains("faux-footer-1"));
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
