#[path = "../examples/sdk/05_tools.rs"]
mod example;
use crate::shared_oracle as oracle;
#[path = "support/sdk_examples.rs"]
mod support;
use serde_json::json;

#[tokio::test]
async fn selects_four_tool_sets_and_applies_custom_cwd() {
    let a = support::fixture().await;
    let b = support::fixture().await;
    let c = support::fixture().await;
    let d = support::fixture().await;
    let cwd = c.home.path().join("custom");
    std::fs::create_dir(&cwd).unwrap();
    let output = support::Output::default();
    let _main = example::main;
    let selected = example::run(
        [a.options, b.options, c.options, d.options],
        &cwd.display().to_string(),
        output.clone(),
    )
    .await
    .unwrap();
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/05-tools.ts",
    );
    let expected = source.request(&json!({}));
    assert_eq!(output.text(), expected["stdout"]);
    for (index, (path, tools)) in selected.iter().enumerate() {
        let mut expected_tools = expected["created"][index]["tools"].as_array().unwrap().clone();
        expected_tools.push(json!("end_turn")); // SDK mandatory termination tool.
        assert_eq!(json!(tools), json!(expected_tools));
        if index >= 2 {
            assert_eq!(path, &cwd.display().to_string());
        }
    }
    assert_eq!(selected.len(), 4);
    assert!(a.requests.lock().unwrap().is_empty());
    let _homes = (a.home, b.home, c.home, d.home);
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
