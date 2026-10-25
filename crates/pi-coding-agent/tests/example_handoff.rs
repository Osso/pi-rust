#[path = "../examples/extensions/handoff.rs"]
mod example;
#[path = "support/extension_examples_i_session.rs"]
mod harness;
use serde_json::json;
#[tokio::test]
async fn every_handoff_command_branch_and_replacement_ui_matches() {
    for request in [
        json!({"mode":"json"}),
        json!({"noModel":true}),
        json!({"args":" \n "}),
        json!({"args":"next"}),
        json!({"args":"next","messages":[{"role":"user","content":"goal","timestamp":0}]}),
        json!({"args":"next","messages":[{"role":"user","content":"goal","timestamp":0}],"editor":"edited draft"}),
        json!({"args":"next","messages":[{"role":"user","content":"goal","timestamp":0}],"editor":""}),
        json!({"args":"next","messages":[{"role":"user","content":"goal","timestamp":0}],"editor":"draft","cancelSession":true}),
        json!({"args":"next","messages":[{"role":"user","content":"goal","timestamp":0}],"aborted":true}),
        json!({"args":"next","messages":[{"role":"user","content":"goal","timestamp":0}],"key":"\u{1b}"}),
        json!({"args":"next","messages":[{"role":"user","content":"goal","timestamp":0}],"response":[],"editor":""}),
        json!({"args":"next","rich":true,"editor":"draft","compaction":0}),
        json!({"args":"next","rich":true,"editor":"draft","compaction":2}),
        json!({"args":"next","rich":true,"editor":"draft","compaction":99}),
    ] {
        harness::compare("handoff", example::handoff_extension, request).await;
    }
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/ui_thread.rs"]
pub mod shared_ui_thread;
