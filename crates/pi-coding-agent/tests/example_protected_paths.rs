#[path = "../examples/extensions/protected_paths.rs"]
mod example;
#[path = "support/extension_examples_b.rs"]
mod support;
use serde_json::json;
use std::sync::Arc;
#[tokio::test]
async fn pinned_example_matches_runtime_effects() {
    let factory = Arc::new(|api: &pi_coding_agent::core::extensions::ExtensionApi| {
        let result = example::protected_paths_extension(api);
        Box::pin(async move { result }) as pi_coding_agent::core::extensions::BoxFuture<'static, _>
    });
    let actual=support::compare("protected-paths",factory,serde_json::from_str(r#"{"cases": [{"event": "tool_call", "toolName": "write", "input": {"path": ".env"}, "hasUI": true}, {"event": "tool_call", "toolName": "edit", "input": {"path": "nested/.env.backup"}, "hasUI": false}, {"event": "tool_call", "toolName": "write", "input": {"path": "x/.git/config"}, "hasUI": true}, {"event": "tool_call", "toolName": "edit", "input": {"path": "node_modules/pkg/a"}, "hasUI": true}, {"event": "tool_call", "toolName": "write", "input": {"path": "node_modules"}, "hasUI": true}, {"event": "tool_call", "toolName": "write", "input": {"path": ".ENV"}, "hasUI": true}, {"event": "tool_call", "toolName": "edit", "input": {"path": "src/λ🚀.rs"}, "hasUI": true}, {"event": "tool_call", "toolName": "read", "input": {"path": ".env"}, "hasUI": true}]}"#).unwrap()).await;
    assert_eq!(actual["metadata"], json!({"commands":[],"tools":[]}));
}
