#[path = "../examples/extensions/dynamic_tools.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;

#[tokio::test]
async fn dynamic_tools_match_complete_pinned_results() {
    let actual = harness::compare(
        "dynamic-tools",
        harness::factory(example::dynamic_tools_extension),
        json!({"steps":[
            {"op":"tools"},
            {"op":"event","event":{"type":"session_start","reason":"startup"}},
            {"op":"tools"},
            {"op":"tool","name":"echo_session","params":{"message":"exact 🚀\noutput"}},
            {"op":"command","name":"add-echo-tool","args":"  CuStOm_42  "},
            {"op":"tools"},
            {"op":"tool","name":"custom_42","params":{"message":""}},
            {"op":"command","name":"add-echo-tool","args":"CUSTOM_42"},
            {"op":"command","name":"add-echo-tool","args":"echo_session"},
            {"op":"command","name":"add-echo-tool","args":""},
            {"op":"command","name":"add-echo-tool","args":"   "},
            {"op":"command","name":"add-echo-tool","args":"bad-name"},
            {"op":"command","name":"add-echo-tool","args":"two words"},
            {"op":"command","name":"add-echo-tool","args":"écho"},
            {"op":"command","name":"add-echo-tool","args":"\u{feff}ZERO_0\u{feff}"},
            {"op":"tool","name":"zero_0","params":{"message":"zero"}},
            {"op":"event","event":{"type":"session_start","reason":"startup"}},
            {"op":"tools"}
        ]}),
    )
    .await;
    assert_eq!(actual["tools"], json!([]));
    assert_eq!(
        actual["trace"][3],
        json!(["tool",{"content":[{"type":"text","text":"[session] exact 🚀\noutput"}],"details":{"tool":"echo_session","prefix":"[session] "}}])
    );
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
