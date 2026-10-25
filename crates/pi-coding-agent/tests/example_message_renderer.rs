#[path = "../examples/extensions/message_renderer.rs"]
mod example;
#[path = "support/extensions.rs"]
mod extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
#[path = "support/oracle.rs"]
mod oracle;
use serde_json::json;
#[tokio::test]
async fn status_command_and_expanded_level_rendering_match() {
    let before = pi_js_compat::date::js_now_millis();
    let mut steps = vec![];
    for args in [
        "",
        "  hello   world  ",
        "warn",
        "error   broken  now",
        "warning test",
        "\u{feff}warn\u{a0}🚀𐀀",
    ] {
        steps.push(json!({"op":"command","name":"status","args":args}));
    }
    for details in [
        json!(null),
        json!({"level":"warn","timestamp":1}),
        json!({"level":"error","timestamp":1700000000000i64}),
        json!({"level":"custom","timestamp":0}),
    ] {
        for expanded in [false, true] {
            steps.push(json!({"op":"renderMessage","message":{"customType":"status-update","content":"A wrapping status 🚀𐀀 line","display":true,"details":details,"timestamp":0},"expanded":expanded,"width":32}));
        }
    }
    steps.push(json!({"op":"renderMessage","message":{"customType":"status-update","content":[{"type":"text","text":"block content"},{"type":"image","mimeType":"image/png","data":"AA=="}],"display":true,"timestamp":0},"expanded":false,"width":48}));
    let actual = harness::compare(
        "message-renderer",
        harness::factory(example::message_renderer_extension),
        json!({"steps":steps}),
    )
    .await;
    assert_eq!(actual["trace"][1][1]["content"], "hello   world");
    assert_eq!(actual["trace"][2][1]["content"], "Status update");
    assert_eq!(actual["trace"][3][1]["content"], "broken now");
    for i in 0..6 {
        let time = actual["trace"][i][1]["details"]["timestamp"].as_f64().unwrap();
        assert!(time >= before && time <= pi_js_compat::date::js_now_millis());
    }
}
