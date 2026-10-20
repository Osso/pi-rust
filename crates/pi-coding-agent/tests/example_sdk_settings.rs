#[path = "../examples/sdk/10_settings.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/sdk_examples.rs"]
mod support;
use serde_json::json;

#[tokio::test]
async fn prints_disk_settings_flushes_low_and_keeps_overrides_ephemeral() {
    let a = support::fixture().await;
    let b = support::fixture().await;
    let path = a.home.path().join("agent/settings.json");
    std::fs::write(&path, "{\"defaultThinkingLevel\":\"high\"}").unwrap();
    let output = support::Output::default();
    let _main = example::main;
    let settings = example::run([a.options, b.options], output.clone()).await.unwrap();
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/10-settings.ts",
    );
    let expected = source.request(&json!({"settings":{"defaultThinkingLevel":"high"}}));
    assert_eq!(output.text(), expected["stdout"]);
    assert_eq!(settings["compaction"]["enabled"], false);
    assert_eq!(settings["retry"]["maxRetries"], 5);
    let persisted: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(persisted["defaultThinkingLevel"], "low");
    assert!(persisted.get("retry").is_none());
    assert!(a.requests.lock().unwrap().is_empty());
    let _homes = (a.home, b.home);
}
