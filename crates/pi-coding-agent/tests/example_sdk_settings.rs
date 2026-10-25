#[path = "../examples/sdk/10_settings.rs"]
mod example;
use crate::shared_oracle as oracle;
#[path = "support/sdk_examples.rs"]
mod support;
use pi_js_compat::js_json::JsValue;
use serde_json::json;

#[tokio::test]
async fn prints_disk_settings_flushes_low_and_keeps_overrides_ephemeral() {
    let a = support::fixture().await;
    let b = support::fixture().await;
    let path = a.home.path().join("agent/settings.json");
    std::fs::write(&path, r#"{"defaultThinkingLevel":"high","custom":{"2":2.0,"1":1.0}}"#).unwrap();
    let output = support::Output::default();
    let _main = example::main;
    let settings = example::run([a.options, b.options], output.clone(), std::io::sink())
        .await
        .unwrap();
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/10-settings.ts",
    );
    let expected = source.request(&json!({"settings":{"defaultThinkingLevel":"high","custom":{"2":2.0,"1":1.0}}}));
    assert_eq!(output.text(), expected["stdout"]);
    assert_eq!(settings["compaction"]["enabled"], JsValue::Bool(false));
    assert_eq!(settings["retry"]["maxRetries"], JsValue::Number(5.0));
    let persisted: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(persisted["defaultThinkingLevel"], "low");
    assert!(persisted.get("retry").is_none());
    assert!(a.requests.lock().unwrap().is_empty());
    let _homes = (a.home, b.home);
}

#[tokio::test]
async fn reports_real_pinned_parse_errors_on_stderr_without_rewriting_corrupt_settings() {
    let a = support::fixture().await;
    let b = support::fixture().await;
    let path = a.home.path().join("agent/settings.json");
    std::fs::write(&path, "{").unwrap();
    let mut state = oracle::TsOracle::spawn(
        "sdk-settings-state-oracle.ts",
        "packages/coding-agent/src/core/settings-manager.ts",
    );
    let expected_state = state.request(&json!({"cwd":a.options.cwd,"agentDir":a.options.agent_dir}));
    assert!(!expected_state["errors"].as_array().unwrap().is_empty());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{");
    let stdout = support::Output::default();
    let stderr = support::Output::default();
    example::run([a.options, b.options], stdout.clone(), stderr.clone())
        .await
        .unwrap();
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/10-settings.ts",
    );
    let expected =
        source.request(&json!({"settings":expected_state["settings"],"settingsErrors":expected_state["errors"]}));
    assert_eq!(stdout.text(), expected["stdout"]);
    assert_eq!(stderr.text(), expected["stderr"]);
    assert!(!stdout.text().contains("Warning"));
    assert!(stderr.text().starts_with("Warning (global settings): "));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "{");
    assert!(a.requests.lock().unwrap().is_empty());
    let _homes = (a.home, b.home);
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
