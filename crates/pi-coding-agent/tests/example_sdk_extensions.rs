#[path = "../examples/sdk/06_extensions.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/sdk_examples.rs"]
mod support;
use serde_json::json;

#[tokio::test]
async fn native_inline_extension_runs_before_streaming() {
    let fixture = support::fixture().await;
    let _home = fixture.home;
    let output = support::Output::default();
    let _main = example::main;
    example::run(
        fixture.options,
        std::sync::Arc::new(pi_coding_agent::core::extensions::loader::ExtensionRegistry::new()),
        output.clone(),
    )
    .await
    .unwrap();
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/06-extensions.ts",
    );
    let expected = source.request(&json!({}));
    assert_eq!(output.text(), expected["stdout"]);
    assert_eq!(output.text(), "[Inline Extension] Agent starting\nHello!\n");
    assert_eq!(
        fixture.requests.lock().unwrap()[0]["context"]["messages"][0]["content"][0]["text"],
        "List files in the current directory."
    );
}
