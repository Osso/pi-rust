#[path = "../examples/sdk/09_api_keys_and_oauth.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/sdk_examples.rs"]
mod support;
use serde_json::json;

#[tokio::test]
async fn reuses_default_runtime_for_key_override_without_persisting_key() {
    let a = support::fixture().await;
    let b = support::fixture().await;
    let c = support::fixture().await;
    let runtime = a.options.model_runtime.as_ref().unwrap().clone();
    let output = support::Output::default();
    let _main = example::main;
    example::run([a.options, b.options, c.options], output.clone())
        .await
        .unwrap();
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/09-api-keys-and-oauth.ts",
    );
    let expected = source.request(&json!({}));
    assert_eq!(output.text(), expected["stdout"]);
    assert!(runtime.has_configured_auth("anthropic"));
    assert!(!a.home.path().join("agent/auth.json").exists());
    assert!(a.requests.lock().unwrap().is_empty());
    let _homes = (a.home, b.home, c.home);
}
