#[path = "../examples/extensions/event_bus.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;

#[tokio::test]
async fn notifications_follow_session_context_and_real_bus_order() {
    let actual = harness::compare(
        "event-bus",
        harness::factory(example::event_bus_extension),
        json!({"observeBus":true,"steps":[
            {"op":"command","name":"emit","args":"before context"},
            {"op":"bus","channel":"my:notification","data":{"message":"also before","from":"peer"}},
            {"op":"event","event":{"type":"session_start","reason":"startup"}},
            {"op":"command","name":"emit","args":"\u{feff}  Ahoy 🚀\u{2028}"},
            {"op":"command","name":"emit","args":"\t"},
            {"op":"command","name":"emit","args":"\u{85}kept\u{85}"},
            {"op":"bus","channel":"other:channel","data":{"message":"ignored","from":"peer"}},
            {"op":"bus","channel":"my:notification","data":{"message":"cross-extension\nλ","from":"peer/𐀀"}},
            {"op":"event","event":{"type":"session_start","reason":"resume"}}
        ]}),
    )
    .await;
    assert_eq!(
        actual["observed"],
        json!([
            ["my:notification", {"message":"before context","from":"/emit command"}],
            ["my:notification", {"message":"also before","from":"peer"}],
            ["my:notification", {"message":"Session started","from":"event-bus-example"}],
            ["my:notification", {"message":"Ahoy 🚀","from":"/emit command"}],
            ["my:notification", {"message":"hello","from":"/emit command"}],
            ["my:notification", {"message":"\u{85}kept\u{85}","from":"/emit command"}],
            ["my:notification", {"message":"cross-extension\nλ","from":"peer/𐀀"}],
            ["my:notification", {"message":"Session started","from":"event-bus-example"}]
        ])
    );
    assert_eq!(
        actual["trace"],
        json!([
            ["notify", "Event from event-bus-example: Session started", "info"],
            ["notify", "Event from /emit command: Ahoy 🚀", "info"],
            ["notify", "Event from /emit command: hello", "info"],
            ["notify", "Event from /emit command: \u{85}kept\u{85}", "info"],
            ["notify", "Event from peer/𐀀: cross-extension\nλ", "info"],
            ["notify", "Event from event-bus-example: Session started", "info"]
        ])
    );
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
