#[path = "../examples/extensions/status_line.rs"]
mod example;
use crate::shared_extensions as extension_support;
#[path = "support/extension_examples_a.rs"]
mod harness;
use crate::shared_oracle as oracle;
use serde_json::json;

#[tokio::test]
async fn themed_turn_count_persists_across_session_starts() {
    let end = json!({"op":"event","event":{"type":"turn_end","turnIndex":99,
        "message":{"role":"user","content":"fixture","timestamp":0},"toolResults":[]}});
    let actual = harness::compare(
        "status-line",
        harness::factory(example::status_line_extension),
        json!({"steps":[
            {"op":"event","event":{"type":"session_start","reason":"startup"}},
            end,
            {"op":"event","event":{"type":"turn_start","turnIndex":99,"timestamp":0}},
            end,
            {"op":"event","event":{"type":"session_start","reason":"resume"}},
            {"op":"event","event":{"type":"turn_start","turnIndex":0,"timestamp":0}},
            end
        ]}),
    )
    .await;
    let trace = actual["trace"].as_array().unwrap();
    assert_eq!(trace.len(), 7);
    assert_eq!(
        trace[0],
        json!(["status", "status-demo", "\u{1b}[38;2;90;91;92mReady\u{1b}[39m"])
    );
    assert_eq!(
        trace[2],
        json!([
            "status",
            "status-demo",
            "\u{1b}[38;2;80;180;230m●\u{1b}[39m\u{1b}[38;2;90;91;92m Turn 1...\u{1b}[39m"
        ])
    );
    assert_eq!(
        trace[6],
        json!([
            "status",
            "status-demo",
            "\u{1b}[38;2;34;204;102m✓\u{1b}[39m\u{1b}[38;2;90;91;92m Turn 2 complete\u{1b}[39m"
        ])
    );
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
