//! Select a model from the runtime, including custom and authenticated models.
mod common;
mod error;
use pi_agent_core::ThinkingLevel;
use pi_coding_agent::core::{
    model_runtime::{CreateModelRuntimeOptions, ModelRuntime},
    sdk::CreateAgentSessionOptions,
};
use pi_js_compat::js_json::JsString;
use std::io::Write;
use std::sync::{Arc, Mutex};

pub async fn run(
    mut options: CreateAgentSessionOptions,
    mut output: impl Write + Send + 'static,
) -> Result<(), JsString> {
    let runtime = match options.model_runtime.take() {
        Some(runtime) => runtime,
        None => ModelRuntime::create(CreateModelRuntimeOptions::default())
            .await
            .map_err(|e| e.message().clone())?,
    };
    for (provider, id, label) in [
        ("anthropic", "claude-opus-4-5", "Found model"),
        ("my-provider", "my-model", "Found custom model"),
    ] {
        if let Some(model) = runtime.get_model(provider, id) {
            writeln!(output, "{label}: {}/{}", model.provider, model.id).map_err(|e| JsString::from(e.to_string()))?;
        }
    }
    let available = runtime
        .get_available(None, Default::default())
        .await
        .map_err(|e| e.message().clone())?;
    let names: Vec<_> = available.iter().map(|m| format!("{}/{}", m.provider, m.id)).collect();
    writeln!(
        output,
        "Available models: {}",
        serde_json::to_string(&names).map_err(|e| JsString::from(e.to_string()))?
    )
    .map_err(|e| JsString::from(e.to_string()))?;
    if let Some(model) = available.first() {
        options.model = Some(model.clone());
        options.model_runtime = Some(runtime);
        options.thinking_level = Some(ThinkingLevel::Medium);
        common::stream(
            options,
            "Say hello in one sentence.",
            "",
            "\n",
            Arc::new(Mutex::new(output)),
        )
        .await?;
    }
    Ok(())
}
#[tokio::main(flavor = "current_thread")]
pub async fn main() -> Result<(), error::ExampleError> {
    run(CreateAgentSessionOptions::default(), std::io::stdout())
        .await
        .map_err(error::ExampleError)
}
