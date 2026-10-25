#[path = "../examples/sdk/12_full_control.rs"]
mod example;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/sdk_prompt.rs"]
mod prompt;
#[path = "support/sdk_examples.rs"]
mod support;
use pi_js_compat::js_json::JsString;
use serde_json::json;

#[tokio::test]
async fn defaults_use_private_runtime_paths_sonnet_and_environment_key() {
    if std::env::var_os("SDK_DEFAULTS_CHILD").is_none() {
        for key in [Some("private-env-key"), Some(""), None] {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "defaults_use_private_runtime_paths_sonnet_and_environment_key",
                    "--nocapture",
                ])
                .env("SDK_DEFAULTS_CHILD", "1")
                .env_remove("ANTHROPIC_API_KEY");
            match key {
                Some(key) => {
                    command.env("MY_ANTHROPIC_KEY", key);
                }
                None => {
                    command.env_remove("MY_ANTHROPIC_KEY");
                }
            }
            let result = command.output().unwrap();
            assert!(
                result.status.success(),
                "{}{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }
        return;
    }
    let mut fixture = support::fixture().await;
    fixture.options.model = None;
    fixture.options.model_registry = None;
    let agent = fixture.home.path().join("agent");
    let auth = r#"{"fixture-path":{"type":"api_key","key":"private-path-key"}}"#;
    std::fs::write(agent.join("auth.json"), auth).unwrap();
    std::fs::write(agent.join("models.json"), r#"{"providers":{"fixture-path":{"baseUrl":"https://unused.invalid","api":"openai-completions","models":[{"id":"private-model","name":"Private model"}]}}}"#).unwrap();
    let key = example::read_api_key();
    assert_eq!(
        key,
        std::env::var("MY_ANTHROPIC_KEY").ok(),
        "entrypoint reads the exact environment value"
    );
    let mut real = oracle::TsOracle::spawn(
        "sdk-runtime-state-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let state = real.request(&json!({"agentDir":agent,"key":key,"inMemory":true}));
    let options = example::configure_options(fixture.options, key.as_deref())
        .await
        .unwrap();
    assert_eq!(options.agent_dir.as_ref(), agent.to_str().map(JsString::from).as_ref());
    let model = options.model.unwrap();
    assert_eq!(
        (model.provider.as_str(), model.id.as_str()),
        ("anthropic", "claude-sonnet-4-5")
    );
    assert_eq!(serde_json::to_value(&model).unwrap(), state["sonnet"]);
    let runtime = options.model_registry.unwrap();
    assert_eq!(
        serde_json::to_value(runtime.find("fixture-path", "private-model")).unwrap(),
        state["customModel"]
    );
    assert!(
        runtime.find("fixture-path", "private-model").is_none(),
        "frozen SDK12:27 uses ModelRegistry.inMemory, never models.json"
    );
    assert_eq!(
        runtime.auth_storage().has_auth("fixture-path").unwrap(),
        state["customStoredAuth"].as_bool().unwrap()
    );
    let expected_key = key.filter(|key| !key.is_empty());
    assert_eq!(
        runtime.auth_storage().has_auth("anthropic").unwrap(),
        expected_key.is_some()
    );
    assert_eq!(
        runtime.auth_storage().has_auth("anthropic").unwrap(),
        state["anthropicAuth"].as_bool().unwrap()
    );
    assert_eq!(state["key"], json!(expected_key));
    if let Some(expected_key) = expected_key {
        assert_eq!(
            runtime.get_api_key_for_provider("anthropic").await.unwrap().as_ref(),
            Some(JsString::from(expected_key.as_str())).as_ref()
        );
    }
    assert_eq!(
        std::fs::read_to_string(agent.join("auth.json")).unwrap(),
        auth,
        "environment key is never persisted"
    );
    assert!(fixture.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn replaces_discovery_settings_prompt_and_tools() {
    let fixture = support::fixture().await;
    let cwd = fixture.options.cwd.clone().unwrap();
    let _home = fixture.home;
    let output = support::Output::default();
    let _main = example::main;
    let before = pi_js_compat::date::js_now_millis();
    example::run(fixture.options, None, output.clone()).await.unwrap();
    let after = pi_js_compat::date::js_now_millis();
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/12-full-control.ts",
    );
    let expected = source.request(&json!({}));
    assert_eq!(output.text(), expected["stdout"]);
    let requests = fixture.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0]["reasoning"].is_null(),
        "thinking off reaches provider without reasoning"
    );
    let context = &requests[0]["context"];
    prompt::assert_from_frozen_builder(
        &context["systemPrompt"],
        &expected["created"][0]["customPrompt"],
        &cwd,
        [before, after],
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
