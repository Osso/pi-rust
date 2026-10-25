#[path = "../examples/extensions/qna.rs"]
mod example;
use crate::shared_extensions as extension_support;
use crate::shared_oracle as oracle;
#[path = "support/extension_examples_b.rs"]
mod support;
use pi_coding_agent::core::extensions::*;
use serde_json::{Value, json};
use std::sync::Arc;
fn assistant(content: Value, reason: &str) -> Value {
    json!({"role":"assistant","content":content,"api":"faux","provider":"faux","model":"faux-1","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":reason,"timestamp":0})
}
async fn compare(mut request: Value) -> Value {
    request["modelFixture"]["provider"] = json!("qna-faux");
    request["modelFixture"]["id"] = json!("qna-1");
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::qna_extension(api);
        Box::pin(async move { result })
    });
    support::compare("qna", factory, request).await
}
#[tokio::test]
async fn validates_mode_model_and_branch_messages_before_extraction() {
    compare(json!({"modelFixture":{},"hasModel":false,"cases":[{"event":"command","name":"qna","mode":"rpc"},{"event":"command","name":"qna"}]})).await;
    compare(json!({"modelFixture":{},"cases":[
        {"event":"command","name":"qna"},
        {"event":"append","message":assistant(json!([{ "type":"thinking","thinking":"only thought"}]),"stop")},
        {"event":"command","name":"qna"},
        {"event":"append","message":assistant(json!([{ "type":"text","text":"incomplete?"}]),"length")},
        {"event":"command","name":"qna"}
    ]}))
    .await;
}
#[tokio::test]
async fn real_loader_faux_extraction_complete_prompt_and_editor_effects_match() {
    for (fixture, keys) in [
        (
            json!({"content":[{"type":"text","text":"Q: Choose Rust?\nA: "},{"type":"thinking","thinking":"hidden"},{"type":"text","text":"Q: Which DB?\nA: "}]}),
            json!([]),
        ),
        (json!({"content":[]}), json!([])),
        (json!({"stopReason":"aborted"}), json!([])),
        (json!({"error":"private failure"}), json!([])),
        (json!({}), json!(["\u{1b}"])),
    ] {
        let actual=compare(json!({"modelFixture":fixture,"cases":[
            {"event":"append","message":assistant(json!([{ "type":"text","text":"Choose Rust?"},{"type":"thinking","thinking":"hidden"},{"type":"text","text":"Which DB?"}]),"stop")},
            {"event":"append","message":assistant(json!([{ "type":"thinking","thinking":"skip me"}]),"stop")},
            {"event":"command","name":"qna","keys":keys,"width":40}
        ]})).await;
        assert_eq!(actual["errors"], json!([]));
    }
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
#[path = "support/ui_thread.rs"]
pub mod shared_ui_thread;
