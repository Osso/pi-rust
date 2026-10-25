#[path = "../examples/sdk/06_extensions.rs"]
mod example;
use crate::shared_oracle as oracle;
#[path = "support/sdk_examples.rs"]
mod support;
use pi_js_compat::js_json::encode_node_utf8;
use serde_json::json;

#[tokio::test]
async fn native_inline_extension_runs_before_streaming() {
    let fixture = support::fixture().await;
    let _home = fixture.home;
    let output = support::Output::default();
    let _main = example::main;
    let mut registry = pi_coding_agent::core::extensions::loader::ExtensionRegistry::new();
    let loaded = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    for id in ["my-logging-extension.ts", "my-safety-extension.ts"] {
        let path = std::path::Path::new(&encode_node_utf8(fixture.options.cwd.as_ref().unwrap()))
            .join(id)
            .display()
            .to_string();
        // Discovery requires an existing path; the native registry supplies its code.
        std::fs::write(&path, "// native fixture registry ID\n").unwrap();
        let observed = std::sync::Arc::clone(&loaded);
        registry.register_hidden(
            path,
            std::sync::Arc::new(move |_| {
                observed.lock().unwrap().push(id);
                Box::pin(async { Ok(()) })
            }),
        );
    }
    example::run(fixture.options, std::sync::Arc::new(registry), output.clone())
        .await
        .unwrap();
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/06-extensions.ts",
    );
    let expected = source.request(&json!({}));
    assert_eq!(
        *loaded.lock().unwrap(),
        ["my-logging-extension.ts", "my-safety-extension.ts"]
    );
    assert_eq!(output.text(), expected["stdout"]);
    assert_eq!(output.text(), "[Inline Extension] Agent starting\nHello!\n");
    assert_eq!(
        fixture.requests.lock().unwrap()[0]["context"]["messages"][0]["content"][0]["text"],
        "List files in the current directory."
    );
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
