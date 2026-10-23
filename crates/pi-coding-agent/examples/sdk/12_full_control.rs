//! Replace discovery with explicit model, prompt, tools and in-memory settings.
mod common;
mod error;
use pi_agent_core::ThinkingLevel;
use pi_coding_agent::core::{
    model_registry::ModelRegistry, resource_loader::StaticResourceLoader, sdk::CreateAgentSessionOptions,
    session_manager::SessionManager, settings_manager::SettingsManager,
};
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::json;
use std::io::Write;
use std::sync::{Arc, Mutex};

pub fn read_api_key() -> Option<String> {
    std::env::var("MY_ANTHROPIC_KEY").ok()
}

pub async fn configure_options(
    mut options: CreateAgentSessionOptions,
    api_key: Option<&str>,
) -> Result<CreateAgentSessionOptions, JsString> {
    let agent_dir = options.agent_dir.clone().unwrap_or_else(|| "/tmp/my-agent".into());
    let runtime = match options.model_registry.take() {
        Some(runtime) => runtime,
        None => ModelRegistry::in_memory(
            pi_coding_agent::core::auth_storage::AuthStorage::create(Some(&(agent_dir.clone() + "/auth.json")))
                .map_err(|error| error.message().clone())?,
        ),
    };
    if let Some(key) = api_key.filter(|key| !key.is_empty()) {
        runtime
            .auth_storage()
            .set_runtime_api_key("anthropic", &JsString::from(key));
    }
    let model = options
        .model
        .take()
        .or_else(|| pi_ai::compat::get_model("anthropic", "claude-sonnet-4-5"))
        .ok_or_else(|| "Model not found".to_owned())?;
    options.model = Some(model);
    options.model_registry = Some(runtime);
    options.agent_dir = Some(agent_dir);
    options.thinking_level = Some(ThinkingLevel::Off);
    options.tools = Some(vec!["read".into(), "bash".into()]);
    options.resource_loader = Some(Arc::new(StaticResourceLoader {
        system_prompt: Some("You are a minimal assistant.\nAvailable: read, bash. Be concise.".into()),
        ..Default::default()
    }));
    options.settings_manager = Some(Arc::new(Mutex::new(SettingsManager::in_memory(
        (json!({"compaction":{"enabled":false},"retry":{"enabled":true,"maxRetries":2}})
            .as_object()
            .expect("settings object")
            .clone())
        .into_iter()
        .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
        .collect::<pi_js_compat::js_json::JsObject>(),
        Default::default(),
    ))));
    options.session_manager =
        Some(SessionManager::in_memory(options.cwd.as_ref(), None).map_err(|e| e.message().clone())?);
    Ok(options)
}

pub async fn run(
    options: CreateAgentSessionOptions,
    api_key: Option<&str>,
    output: impl Write + Send + 'static,
) -> Result<(), JsString> {
    common::stream(
        configure_options(options, api_key).await?,
        "List files in the current directory.",
        "",
        "\n",
        Arc::new(Mutex::new(output)),
    )
    .await
}
#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<(), error::ExampleError> {
    run(
        CreateAgentSessionOptions::default(),
        read_api_key().as_deref(),
        std::io::stdout(),
    )
    .await
    .map_err(error::ExampleError)
}
