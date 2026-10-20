#[path = "../examples/extensions/working_message_test.rs"]
mod example;
#[path = "support/extension_examples_b.rs"]
mod support;
use serde_json::json;
use std::sync::Arc;
#[tokio::test]
async fn pinned_example_matches_runtime_effects() {
    let factory = Arc::new(|api: &pi_coding_agent::core::extensions::ExtensionApi| {
        let result = example::working_message_test_extension(api);
        Box::pin(async move { result }) as pi_coding_agent::core::extensions::BoxFuture<'static, _>
    });
    let actual=support::compare("working-message-test",factory,serde_json::from_str(r#"{"cases": [{"event": "session_start"}, {"event": "session_start", "hasUI": false}, {"event": "session_start"}]}"#).unwrap()).await;
    assert_eq!(actual["metadata"], json!({"commands":[],"tools":[]}));
}
