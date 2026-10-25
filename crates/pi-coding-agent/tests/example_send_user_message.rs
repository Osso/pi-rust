#[path = "../examples/extensions/send_user_message.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;

#[tokio::test]
async fn commands_preserve_content_and_idle_streaming_delivery() {
    let mut steps = Vec::new();
    for idle in [true, false] {
        steps.push(json!({"op":"idle","value":idle}));
        for command in ["ask", "steer", "followup", "askwith"] {
            for args in ["", "\u{feff}\u{a0}\u{2028}", "  Keep 🚀𐀀\n spacing  ", "\u{85}"] {
                steps.push(json!({"op":"command","name":command,"args":args}));
            }
        }
    }
    let actual = harness::compare(
        "send-user-message",
        harness::factory(example::send_user_message_extension),
        json!({"steps":steps}),
    )
    .await;
    assert_eq!(
        actual["trace"][2],
        json!(["sendUserMessage", "  Keep 🚀𐀀\n spacing  ", null])
    );
    assert!(
        actual["trace"]
            .as_array()
            .unwrap()
            .contains(&json!(["notify", "Follow-up queued", "info"]))
    );
}
#[tokio::test]
async fn action_failures_propagate_and_do_not_notify_queued() {
    let actual = harness::compare(
        "send-user-message",
        harness::factory(example::send_user_message_extension),
        json!({"sendError":"delivery failed","steps":[
        {"op":"idle","value":false},
        {"op":"command","name":"followup","args":"hello"},
        {"op":"command","name":"steer","args":"hello"},
        {"op":"idle","value":true},
        {"op":"command","name":"ask","args":"hello"},
        {"op":"command","name":"askwith","args":"hello"}
        ]}),
    )
    .await;
    assert_eq!(
        actual["trace"],
        json!([
            ["error", "delivery failed"],
            ["error", "delivery failed"],
            ["error", "delivery failed"],
            ["error", "delivery failed"]
        ])
    );
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
