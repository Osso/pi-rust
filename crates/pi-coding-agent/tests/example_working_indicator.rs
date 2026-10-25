#[path = "../examples/extensions/working_indicator.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;

#[tokio::test]
async fn modes_frames_order_reset_and_invalid_input_match_ts() {
    let mut steps = vec![json!({"op":"event","event":{"type":"session_start","reason":"startup"}})];
    for args in [
        "",
        "dot",
        "none",
        "PULSE",
        "spinner",
        "invalid",
        "reset",
        "default",
        "\u{feff} DOT \u{2028}",
        "\u{85}dot",
    ] {
        steps.push(json!({"op":"command","name":"working-indicator","args":args}));
        steps.push(json!({"op":"command","name":"working-indicator"}));
    }
    steps.push(json!({"op":"event","event":{"type":"session_start","reason":"resume"}}));
    let actual = harness::compare(
        "working-indicator",
        harness::factory(example::working_indicator_extension),
        json!({"steps":steps}),
    )
    .await;
    assert_eq!(actual["trace"][0][0], "indicator");
    assert_eq!(actual["trace"][0][1]["intervalMs"], 80);
    assert_eq!(actual["trace"][0][1]["frames"].as_array().unwrap().len(), 10);
    assert!(
        actual["trace"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| *v == json!(["indicator", null]))
    );
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
