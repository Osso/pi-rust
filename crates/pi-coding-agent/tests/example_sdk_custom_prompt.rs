#[path = "../examples/sdk/03_custom_prompt.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/sdk_examples.rs"]
mod support;
use pi_js_compat::js_json::JsValue;
use serde_json::json;

#[tokio::test]
async fn replaces_then_appends_prompt_and_preserves_output_order() {
    let first = support::fixture().await;
    let second = support::fixture().await;
    let cwd = first.options.cwd.clone().unwrap();
    let _homes = (first.home, second.home);
    let disposed = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let observed = std::sync::Arc::clone(&disposed);
    let cleanup = pi_ai::session_resources::register_session_resource_cleanup(Box::new(move |session| {
        if let Some(id) = session {
            observed.lock().unwrap().push(id.to_owned());
        }
        Ok(())
    }));
    let output = support::Output::default();
    let _main = example::main;
    let before = pi_js_compat::date::js_now_millis();
    example::run([first.options, second.options], output.clone())
        .await
        .unwrap();
    let after = pi_js_compat::date::js_now_millis();
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/03-custom-prompt.ts",
    );
    let expected = source.request(&json!({"text":"Hello!"}));
    assert_eq!(output.text(), expected["stdout"]);
    let ids = disposed.lock().unwrap();
    assert_eq!(ids.len(), 2, "both native sessions run resource cleanup");
    assert_ne!(ids[0], ids[1]);
    cleanup.unregister();
    let requests = first.requests.lock().unwrap();
    let mut prompts = oracle::TsOracle::spawn(
        "coding-agent-system-prompt-oracle.ts",
        "packages/coding-agent/src/core/system-prompt.ts",
    );
    let mut expected_prompts = Vec::new();
    for now in [before, after] {
        let expected_prompt = prompts.request(&json!({
            "id":"custom-example", "nowMillis":now,
            "options":{"customPrompt":expected["created"][0]["customPrompt"],"cwd":cwd},
        }));
        assert_eq!(expected_prompt["ok"], true);
        expected_prompts.push(JsValue::from(expected_prompt["output"]["prompt"].clone()));
    }
    let actual = JsValue::from(requests[0]["context"]["systemPrompt"].clone());
    assert!(
        expected_prompts.contains(&actual),
        "actual={actual:?}; frozen={expected_prompts:?}"
    );
    assert_eq!(
        requests[0]["context"]["messages"][0]["content"][0]["text"],
        "What is 2 + 2?"
    );
    let requests = second.requests.lock().unwrap();
    assert!(
        requests[0]["context"]["systemPrompt"]
            .as_str()
            .unwrap()
            .contains(expected["created"][1]["appendPrompts"][0].as_str().unwrap())
    );
    assert_eq!(
        requests[0]["context"]["messages"][0]["content"][0]["text"],
        "List 3 benefits of TypeScript."
    );
}
