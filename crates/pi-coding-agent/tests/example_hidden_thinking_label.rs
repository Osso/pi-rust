#[path = "../examples/extensions/hidden_thinking_label.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;

#[tokio::test]
async fn label_reset_and_subsequent_session_start_match_ts() {
    let actual = harness::compare(
        "hidden-thinking-label",
        harness::factory(example::hidden_thinking_label_extension),
        json!({"steps":[
            {"op":"event","event":{"type":"session_start","reason":"startup"}},
            {"op":"command","name":"thinking-label","args":"\u{feff} 🚀 thinking \u{2028}"},
            {"op":"event","event":{"type":"session_start","reason":"resume"}},
            {"op":"command","name":"thinking-label","args":" \t"},
            {"op":"event","event":{"type":"session_start","reason":"resume"}},
            {"op":"command","name":"thinking-label","args":"\u{85}"}
        ]}),
    )
    .await;
    assert_eq!(actual["trace"][0], json!(["thinkingLabel", "Pondering..."]));
    assert_eq!(actual["trace"][4], json!(["thinkingLabel", null]));
    assert_eq!(actual["trace"][6], json!(["thinkingLabel", "Pondering..."]));
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
