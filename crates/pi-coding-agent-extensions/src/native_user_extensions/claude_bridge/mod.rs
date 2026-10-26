//! Native pi-claude-bridge, source snapshot 38d84cb549.
pub mod account_profiles;
pub mod account_state;
mod ask_claude;
pub mod ask_render;
pub mod config;
pub mod convert;
pub mod models;
mod native_prefix;
mod process;
pub mod prompt_capture;
mod provider;
mod session_records;
mod sessions;
pub mod stream;

use crate::core::extensions::{
    ExtensionRegistry,
    types::{ExtensionApi, HandlerError, ProviderConfig},
};
use pi_js_compat::js_json::JsValue;
use std::path::PathBuf;
use std::sync::Arc;

pub fn register(registry: &mut ExtensionRegistry) {
    registry.register_hidden(
        "<native:pi-claude-bridge>",
        Arc::new(|pi| {
            let result = activate(pi);
            Box::pin(async move { result })
        }),
    );
}

fn activate(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let cwd = PathBuf::from(pi.cwd().to_utf8().map_err(|e| HandlerError::new(e.to_string()))?);
    let agent_dir = crate::config::get_agent_dir().map_err(|e| HandlerError::new(e.to_string()))?;
    let agent_dir = PathBuf::from(agent_dir.to_utf8().map_err(|e| HandlerError::new(e.to_string()))?);
    let config_dir = crate::config::config_dir_name()
        .to_utf8()
        .map_err(|e| HandlerError::new(e.to_string()))?;
    let config = config::load_config(&agent_dir, &cwd, &config_dir);
    let settings = config
        .get("provider")
        .cloned()
        .unwrap_or(JsValue::Object(Default::default()));
    let source = PathBuf::from(
        pi.extension_path()
            .to_utf8()
            .map_err(|e| HandlerError::new(e.to_string()))?,
    );
    let executable = provider::executable_path(&settings, &source);
    let ask_name = config
        .get("askClaude")
        .and_then(|v| v.get("name"))
        .and_then(JsValue::as_string)
        .cloned()
        .unwrap_or_else(|| "AskClaude".into());
    let ask_name = ask_name.to_utf8().map_err(|e| HandlerError::new(e.to_string()))?;
    let bridge = provider::BridgeProvider::new(settings.clone(), executable, ask_name);
    let models = models::project_models(
        pi_ai::providers::models_data::provider_models("anthropic").unwrap_or_default(),
        &settings,
    );
    register_prompt_captures(pi, bridge.captures.clone())?;
    let stream_bridge = bridge.clone();
    pi.register_provider(
        "claude-bridge",
        ProviderConfig {
            base_url: Some("claude-bridge".into()),
            api_key: Some("not-used".into()),
            api: Some("claude-bridge".into()),
            models: Some(
                models
                    .into_iter()
                    .map(|model| {
                        let mut input: crate::core::model_registry::ProviderModelInput = model.into();
                        input.api = Some("claude-bridge".into());
                        input
                    })
                    .collect(),
            ),
            stream_simple: Some(Arc::new(move |model, context, options| {
                stream_bridge.stream(model, context, options)
            })),
            ..Default::default()
        },
    )
    .map_err(HandlerError::from)?;
    ask_claude::register(pi, config.get("askClaude").unwrap_or(&JsValue::Null), bridge.clone())?;
    pi.on_agent_end(
        move |_,
              ctx: crate::core::extensions::types::ExtensionContext|
              -> crate::core::extensions::types::BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let bridge = bridge.clone();
            Box::pin(async move {
                bridge.end_turn(&ctx.session_manager()?.get_session_id());
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)?;
    Ok(())
}

#[cfg(test)]
mod account_profile_tests;
#[cfg(test)]
mod integration_tests;
#[cfg(test)]
mod registration_tests;
#[cfg(test)]
mod session_tests;

fn register_prompt_captures(
    pi: &ExtensionApi,
    captures: Arc<std::sync::Mutex<prompt_capture::PromptCaptures>>,
) -> Result<(), HandlerError> {
    use crate::core::extensions::types::{
        BeforeAgentStartEvent, BeforeAgentStartEventResult, BoxFuture, ExtensionContext,
    };
    let last_options = Arc::new(std::sync::Mutex::new(None));
    let before_captures = captures.clone();
    let before_options = last_options.clone();
    pi.on_before_agent_start(
        move |event: BeforeAgentStartEvent,
              _|
              -> BoxFuture<'static, Result<Option<BeforeAgentStartEventResult>, HandlerError>> {
            before_captures
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .record(&event.system_prompt, &event.system_prompt_options, "before_agent_start");
            *before_options.lock().unwrap_or_else(std::sync::PoisonError::into_inner) =
                Some(event.system_prompt_options);
            Box::pin(async { Ok(None) })
        },
    )
    .map_err(HandlerError::new)?;
    let agent_captures = captures.clone();
    let agent_options = last_options.clone();
    pi.on_agent_start(
        move |_, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let captures = agent_captures.clone();
            let options = agent_options
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            Box::pin(async move {
                if let Some(options) = options {
                    captures
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .record(&ctx.get_system_prompt()?, &options, "agent_start");
                }
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)?;
    pi.on_turn_start(
        move |_, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let captures = captures.clone();
            let options = last_options
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            Box::pin(async move {
                if let Some(options) = options {
                    captures
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .record(&ctx.get_system_prompt()?, &options, "turn_start");
                }
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)?;
    Ok(())
}
