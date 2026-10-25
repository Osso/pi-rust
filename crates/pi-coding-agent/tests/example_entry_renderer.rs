#[path = "../examples/extensions/entry_renderer.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;
#[tokio::test]
async fn durable_cards_and_expanded_rendering_match() {
    let before = pi_js_compat::date::js_now_millis();
    let mut steps = vec![
        json!({"op":"command","name":"status-card","args":"\u{feff} \u{a0}"}),
        json!({"op":"command","name":"status-card","args":"  ready 🚀𐀀  "}),
    ];
    for width in [12, 40, 80] {
        for expanded in [false, true] {
            steps.push(json!({"op":"renderEntry","entry":{"customType":"status-card","id":"fixture","parentId":null,"timestamp":"2026-01-01T00:00:00.000Z","data":{"message":"ready 🚀𐀀 with a wrapping line","timestamp":0}},"expanded":expanded,"width":width}));
        }
    }
    steps.push(json!({"op":"renderEntry","entry":{"customType":"status-card","id":"fixture","parentId":null,"timestamp":"2026-01-01T00:00:00.000Z"},"expanded":false,"width":40}));
    let actual = harness::compare(
        "entry-renderer",
        harness::factory(example::entry_renderer_extension),
        json!({"steps":steps}),
    )
    .await;
    assert_eq!(actual["trace"][0][2]["message"], "Status card");
    assert_eq!(actual["trace"][1][2]["message"], "ready 🚀𐀀");
    for i in [0, 1] {
        let time = actual["trace"][i][2]["timestamp"].as_f64().unwrap();
        assert!(time >= before && time <= pi_js_compat::date::js_now_millis());
    }
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
