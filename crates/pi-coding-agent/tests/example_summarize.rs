#[path = "../examples/extensions/summarize.rs"]
mod example;
#[path = "support/extension_examples_i_session.rs"]
mod harness;
use serde_json::json;
#[tokio::test]
async fn every_summary_command_branch_and_ui_matches() {
    for request in [
        json!({}),
        json!({"messages":[{"role":"user","content":" \n ","timestamp":0}]}),
        json!({"messages":[{"role":"user","content":"goal","timestamp":0}],"missingModel":true}),
        json!({"messages":[{"role":"user","content":"goal","timestamp":0}],"noAuth":true}),
        json!({"messages":[{"role":"user","content":"goal","timestamp":0}],"mode":"json"}),
        json!({"messages":[{"role":"user","content":"goal","timestamp":0}],"hasUI":false}),
        json!({"messages":[{"role":"user","content":"goal","timestamp":0}],"response":[]}),
        json!({"messages":[{"role":"user","content":"goal","timestamp":0}],"error":"scripted model failure"}),
        json!({"messages":[{"role":"user","content":"goal","timestamp":0}],"key":"\u{1b}"}),
        json!({"rich":true}),
    ] {
        harness::compare("summarize", example::summarize_extension, request).await;
    }
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/ui_thread.rs"]
pub mod shared_ui_thread;
