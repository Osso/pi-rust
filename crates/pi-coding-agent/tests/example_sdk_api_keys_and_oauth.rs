#[path = "../examples/sdk/09_api_keys_and_oauth.rs"]
mod example;
use crate::shared_oracle as oracle;
#[path = "support/sdk_examples.rs"]
mod support;
use serde_json::json;

#[tokio::test]
async fn reuses_default_runtime_for_key_override_without_persisting_key() {
    let a = support::fixture().await;
    let mut b = support::fixture().await;
    b.options.model_registry = None;
    let agent = b.home.path().join("agent");
    std::fs::write(
        agent.join("auth.json"),
        r#"{"fixture-path":{"type":"api_key","key":"private-key"}}"#,
    )
    .unwrap();
    std::fs::write(agent.join("models.json"), r#"{"providers":{"fixture-path":{"baseUrl":"https://unused.invalid","api":"openai-completions","models":[{"id":"private-model","name":"Private model"}]}}}"#).unwrap();
    let c = support::fixture().await;
    let d = support::fixture().await;
    let runtime = a.options.model_registry.as_ref().unwrap().clone();
    let output = support::Output::default();
    let _main = example::main;
    let sessions = example::run([a.options, b.options, c.options, d.options], output.clone())
        .await
        .unwrap();
    let mut real = oracle::TsOracle::spawn(
        "sdk-runtime-state-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let state = real.request(&json!({"agentDir":agent}));
    let custom = sessions[1].model_registry();
    assert_eq!(
        serde_json::to_value(custom.find("fixture-path", "private-model")).unwrap(),
        state["customModel"]
    );
    assert_eq!(
        custom
            .has_configured_auth(&custom.find("fixture-path", "private-model").unwrap())
            .unwrap(),
        state["customAuth"].as_bool().unwrap()
    );
    assert!(
        custom
            .has_configured_auth(&custom.find("fixture-path", "private-model").unwrap())
            .unwrap(),
        "custom auth.json was read"
    );
    assert_eq!(
        custom.find("fixture-path", "private-model").unwrap().name,
        "Private model"
    );
    let mut source = oracle::TsOracle::spawn(
        "sdk-examples-oracle.ts",
        "packages/coding-agent/examples/sdk/09-api-keys-and-oauth.ts",
    );
    let expected = source.request(&json!({}));
    assert_eq!(output.text(), expected["stdout"]);
    assert_eq!(sessions.len(), 4);
    assert!(runtime.auth_storage().has_auth("anthropic").unwrap());
    runtime
        .auth_storage()
        .set_runtime_api_key("q15-auth-proof", &"shared-fixture".into());
    for index in [0, 2, 3] {
        assert!(
            sessions[index]
                .model_registry()
                .auth_storage()
                .has_auth("q15-auth-proof")
                .unwrap()
        );
    }
    assert!(
        !sessions[1]
            .model_registry()
            .auth_storage()
            .has_auth("q15-auth-proof")
            .unwrap()
    );
    assert!(
        sessions[3]
            .model_registry()
            .find("fixture-path", "private-model")
            .is_none()
    );
    assert!(!a.home.path().join("agent/auth.json").exists());
    assert!(a.requests.lock().unwrap().is_empty());
    let _homes = (a.home, b.home, c.home, d.home);
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
#[path = "support/scripted_registry.rs"]
pub mod shared_scripted_registry;
