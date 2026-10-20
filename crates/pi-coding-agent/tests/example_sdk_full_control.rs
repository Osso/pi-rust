#[path = "../examples/sdk/12_full_control.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/sdk_examples.rs"]
mod support;
use serde_json::json;

#[tokio::test]
async fn replaces_discovery_settings_prompt_and_tools() {
    let fixture = support::fixture().await;
    let cwd = fixture.options.cwd.clone().unwrap();
    let _home = fixture.home;
    let output = support::Output::default();
    let _main = example::main;
    example::run(fixture.options, None, output.clone()).await.unwrap();
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/12-full-control.ts",
    );
    let expected = source.request(&json!({}));
    assert_eq!(output.text(), expected["stdout"]);
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let context = &requests[0]["context"];
    assert_eq!(
        context["systemPrompt"],
        format!("You are a minimal assistant.\nAvailable: read, bash. Be concise.\nCurrent working directory: {cwd}\n")
    );
    let names: Vec<_> = context["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["read", "bash", "end_turn"]);
    assert_eq!(
        context["messages"][0]["content"][0]["text"],
        "List files in the current directory."
    );
}
