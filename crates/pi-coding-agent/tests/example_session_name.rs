#[path = "../examples/extensions/session_name.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;

#[tokio::test]
async fn names_trim_read_back_and_propagate_action_errors() {
    let actual = harness::compare(
        "session-name",
        harness::factory(example::session_name_extension),
        json!({"steps":[
            {"op":"command","name":"session-name","args":""},
            {"op":"command","name":"session-name","args":"\u{feff}\u{a0}Build 🚀𐀀\u{2028}"},
            {"op":"command","name":"session-name","args":" \t"},
            {"op":"command","name":"session-name","args":"\u{85}kept\u{85}"},
            {"op":"command","name":"session-name","args":""}
        ]}),
    )
    .await;
    assert_eq!(actual["trace"][0], json!(["notify", "No session name set", "info"]));
    assert_eq!(actual["trace"][1], json!(["setSessionName", "Build 🚀𐀀"]));
    assert_eq!(actual["trace"][3], json!(["notify", "Session: Build 🚀𐀀", "info"]));
    let error = harness::compare(
        "session-name",
        harness::factory(example::session_name_extension),
        json!({"actionError":"name rejected", "steps":[
            {"op":"command","name":"session-name","args":"new"}
        ]}),
    )
    .await;
    assert_eq!(error["trace"], json!([["error", "name rejected"]]));
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
