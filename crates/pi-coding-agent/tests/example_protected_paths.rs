#[path = "../examples/extensions/protected_paths.rs"]
mod example;
use crate::shared_extensions as extension_support;
use crate::shared_oracle as oracle;
#[path = "support/extension_examples_b.rs"]
mod support;
use pi_coding_agent::core::extensions::*;
use serde_json::{Value, json};
use std::sync::Arc;

#[tokio::test]
async fn pinned_example_matches_runtime_effects() {
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = example::protected_paths_extension(api);
        Box::pin(async move { result })
    });
    let mut cases = Vec::new();
    for (tool, path, has_ui) in [
        ("write", ".env", true),
        ("edit", "nested/.env.backup", false),
        ("write", "x/.git/config", true),
        ("edit", "node_modules/pkg/a", true),
        ("write", "node_modules", true),
        ("write", ".ENV", true),
        ("edit", "src/λ🚀.rs", true),
        ("read", ".env", true),
    ] {
        cases.push(json!({"event":"tool_call","toolName":tool,"input":{"path":path},"hasUI":has_ui}));
    }
    let actual = support::compare("protected-paths", factory, json!({"cases":cases})).await;
    assert_eq!(actual["metadata"], json!({"commands":[],"tools":[]}));
    assert_eq!(
        actual["cases"][0]["result"],
        json!({"block":true,"reason":"Path \".env\" is protected"})
    );
    assert_eq!(
        actual["cases"][0]["ui"],
        json!([["notify", "Blocked write to protected path: .env", "warning"]])
    );
    assert_eq!(actual["cases"][1]["ui"], json!([]));
    assert_eq!(actual["cases"][4]["result"], Value::Null);
    assert_eq!(actual["cases"][7]["result"], Value::Null);
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
