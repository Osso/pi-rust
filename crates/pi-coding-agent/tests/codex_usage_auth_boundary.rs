//! Usage may read stored/runtime credentials, never environment fallback credentials.
#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/extensions.rs"]
mod support;
use pi_coding_agent::bundled_extensions::codex_usage::{CodexUsageOptions, codex_usage_extension};
use pi_coding_agent::core::extensions::create_extension_runtime;
use std::sync::Arc;

#[tokio::test]
async fn usage_does_not_use_environment_api_key_fallback() {
    pi_js_compat::process_env::set("OPENAI_API_KEY", "not-a-codex-token");
    let fetch = Arc::new(bundled::StubFetch::new(Vec::new()));
    let runtime = create_extension_runtime();
    let options = CodexUsageOptions {
        fetch: Some(fetch.clone()),
    };
    let extension = support::extension(&runtime, "/repo", "<usage>", move |api| {
        codex_usage_extension(api, &options).unwrap()
    })
    .await;
    let mut options = bundled::HarnessOptions::new("/repo");
    let mut model = bundled::test_model(false);
    model.provider = "openai".to_owned();
    options.model = Some(model);
    let handler = extension.read().commands["usage"].handler.clone();
    let harness = bundled::Harness::new(vec![extension], runtime, options).await;
    let error = handler("".into(), harness.command_context()).await.unwrap_err();
    assert_eq!(
        error.message,
        "OpenAI Codex provider openai is not logged in. Run /login openai."
    );
    assert!(fetch.requests.lock().unwrap().is_empty());
    pi_js_compat::process_env::clear();
}
