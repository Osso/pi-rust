#[path = "../examples/sdk/02_custom_model.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/sdk_examples.rs"]
mod support;

#[tokio::test]
async fn selects_first_available_model_and_streams_greeting() {
    let fixture = support::fixture().await;
    let _home = fixture.home;
    let runtime = fixture.options.model_runtime.as_ref().unwrap().clone();
    let available = runtime.get_available(None, Default::default()).await.unwrap();
    let names: Vec<_> = available.iter().map(|m| format!("{}/{}", m.provider, m.id)).collect();
    assert!(!names.is_empty());
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/02-custom-model.ts",
    );
    let models: Vec<_> = [("anthropic", "claude-opus-4-5"), ("my-provider", "my-model")]
        .into_iter()
        .filter_map(|(provider, id)| runtime.get_model(provider, id))
        .collect();
    let expected = source.request(&serde_json::json!({"models":models,"available":available}));
    let output = support::Output::default();
    let _main = example::main;
    example::run(fixture.options, output.clone()).await.unwrap();
    assert!(output.text().contains(&format!(
        "Available models: {}\n",
        serde_json::to_string(&names).unwrap()
    )));
    assert!(output.text().ends_with("Hello!\n"));
    assert_eq!(output.text(), expected["stdout"]);
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0]["model"]["id"], available[0].id);
    assert_eq!(
        requests[0]["context"]["messages"][0]["content"][0]["text"],
        "Say hello in one sentence."
    );
}
