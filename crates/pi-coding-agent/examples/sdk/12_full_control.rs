//! Replace discovery with explicit model, prompt, tools and in-memory settings.
mod common;
mod error;
use pi_agent_core::ThinkingLevel;
use pi_coding_agent::core::{
    model_runtime::{CreateModelRuntimeOptions, ModelRuntime},
    resource_loader::StaticResourceLoader,
    sdk::CreateAgentSessionOptions,
    session_manager::SessionManager,
    settings_manager::SettingsManager,
};
use serde_json::json;
use std::io::Write;
use std::sync::{Arc, Mutex};

pub fn read_api_key() -> Option<String> {
    std::env::var("MY_ANTHROPIC_KEY").ok()
}

pub async fn configure_options(
    mut options: CreateAgentSessionOptions,
    api_key: Option<&str>,
) -> Result<CreateAgentSessionOptions, String> {
    let agent_dir = options.agent_dir.clone().unwrap_or_else(|| "/tmp/my-agent".into());
    let runtime = match options.model_runtime.take() {
        Some(runtime) => runtime,
        None => ModelRuntime::create(CreateModelRuntimeOptions {
            auth_path: Some(format!("{agent_dir}/auth.json")),
            models_path: Some(Some(format!("{agent_dir}/models.json"))),
            ..Default::default()
        })
        .await
        .map_err(|e| e.to_string())?,
    };
    if let Some(key) = api_key.filter(|key| !key.is_empty()) {
        runtime
            .set_runtime_api_key("anthropic", key, Default::default())
            .await
            .map_err(|e| e.to_string())?;
    }
    let model = options
        .model
        .take()
        .or_else(|| pi_ai::compat::get_model("anthropic", "claude-sonnet-4-5"))
        .ok_or_else(|| "Model not found".to_owned())?;
    options.model = Some(model);
    options.model_runtime = Some(runtime);
    options.agent_dir = Some(agent_dir);
    options.thinking_level = Some(ThinkingLevel::Off);
    options.tools = Some(vec!["read".into(), "bash".into()]);
    options.resource_loader = Some(Arc::new(StaticResourceLoader {
        system_prompt: Some("You are a minimal assistant.\nAvailable: read, bash. Be concise.".into()),
        ..Default::default()
    }));
    options.settings_manager = Some(Arc::new(Mutex::new(SettingsManager::in_memory(
        json!({"compaction":{"enabled":false},"retry":{"enabled":true,"maxRetries":2}})
            .as_object()
            .expect("settings object")
            .clone(),
        Default::default(),
    ))));
    options.session_manager = Some(SessionManager::in_memory(options.cwd.as_deref(), None).map_err(|e| e.to_string())?);
    Ok(options)
}

pub async fn run(
    options: CreateAgentSessionOptions,
    api_key: Option<&str>,
    output: impl Write + Send + 'static,
) -> Result<(), String> {
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
