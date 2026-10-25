//! `extensions/agents-core/src/agent-model-tool.ts`.

use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use pi_agent_core::AgentToolResult;
use pi_agent_core::harness::types::ThinkingLevel;
use pi_js_compat::string::js_trim;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::core::extensions::types::{ExtensionApi, HandlerError, ViewedSessionMutationTarget};
use crate::core::multi_agent_store::{MultiAgentStore, format_inactive_agent_selection_message, is_active_lifecycle};

use super::runtime::MultiAgentRuntimeHandles;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetAgentModelParams {
    pub agent_id: JsString,
    pub provider: Option<String>,
    pub model_id: Option<String>,
    pub thinking_level: Option<ThinkingLevel>,
}

pub fn set_agent_model_schema() -> Value {
    json!({
        "type": "object", "required": ["agentId"], "additionalProperties": false,
        "properties": {
            "agentId": {"type": "string"},
            "provider": {"type": "string", "description": "Model provider, e.g. openai-codex. Requires modelId."},
            "modelId": {"type": "string", "description": "Model ID within the provider. Requires provider."},
            "thinkingLevel": {
                "anyOf": (["off", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"].map(|level| json!({"type": "string", "const": level}))),
                "description": "Reasoning effort; clamped to what the model supports."
            }
        }
    })
}

/// Resolve an explicit child ID, never the selected/viewed agent.
pub fn resolve_live_agent_mutation_target(
    store: &MultiAgentStore,
    handles: &MultiAgentRuntimeHandles,
    agent_id: &JsString,
) -> Result<Arc<dyn ViewedSessionMutationTarget>, HandlerError> {
    let agent = store
        .get_agent(agent_id)
        .ok_or_else(|| HandlerError::new(js_format!("Agent not found: {}", agent_id)))?;
    if agent.agent_type == "background" {
        return Err(HandlerError::new(js_format!(
            "Agent {} is detached and not a live child session",
            agent_id
        )));
    }
    if !is_active_lifecycle(agent.lifecycle) {
        return Err(HandlerError::new(format_inactive_agent_selection_message(&agent)));
    }
    let session = handles
        .sessions
        .lock()
        .expect("sessions")
        .get(agent_id)
        .cloned()
        .ok_or_else(|| HandlerError::new(js_format!("Agent {} is not a live child session", agent_id)))?;
    session
        .session_mutation_target()
        .ok_or_else(|| HandlerError::new(js_format!("Agent {} does not support live session mutation", agent_id)))
}

fn read_requested_model(params: &SetAgentModelParams) -> Result<Option<(&str, &str)>, HandlerError> {
    let provider = params.provider.as_deref().map(js_trim).unwrap_or_default();
    let model_id = params.model_id.as_deref().map(js_trim).unwrap_or_default();
    if provider.is_empty() && model_id.is_empty() {
        return Ok(None);
    }
    if provider.is_empty() || model_id.is_empty() {
        return Err(HandlerError::new(
            "set_agent_model requires both provider and modelId to change the model",
        ));
    }
    Ok(Some((provider, model_id)))
}

pub async fn set_agent_model(
    store: &MultiAgentStore,
    handles: &MultiAgentRuntimeHandles,
    params: SetAgentModelParams,
) -> Result<AgentToolResult, HandlerError> {
    let requested = read_requested_model(&params)?;
    if requested.is_none() && params.thinking_level.is_none() {
        return Err(HandlerError::new(
            "set_agent_model requires provider and modelId, thinkingLevel, or both",
        ));
    }
    let target = resolve_live_agent_mutation_target(store, handles, &params.agent_id)?;
    if let Some((provider, model_id)) = requested {
        let model = target
            .model_registry()
            .find(provider, model_id)
            .ok_or_else(|| HandlerError::new(format!("Model not found: {provider}/{model_id}")))?;
        target
            .set_model_with_thinking_level(model, params.thinking_level)
            .await?;
    } else if let Some(level) = params.thinking_level {
        target.set_thinking_level(level)?;
    }
    let name = store
        .get_agent(&params.agent_id)
        .map(|agent| agent.display_name)
        .unwrap_or_else(|| pi_js_compat::js_json::JsString::from(&params.agent_id));
    let model = target.model();
    let description = model
        .as_ref()
        .map(|model| format!("{}/{}", model.provider, model.id))
        .unwrap_or_else(|| "no model".to_owned());
    let level = target.thinking_level();
    let mut details = json!({"agentId": params.agent_id, "thinkingLevel": level});
    if let Some(model) = model {
        details["provider"] = json!(model.provider);
        details["modelId"] = json!(model.id);
    }
    Ok(super::tool_results::tool_result(
        name + js_format!(
            " ({}) now uses {}, effort {}; applies from its next model request.",
            params.agent_id,
            description,
            level.as_str()
        ),
        details,
    ))
}

pub fn register_set_agent_model_tool(
    pi: &ExtensionApi,
    store: Arc<MultiAgentStore>,
    handles: MultiAgentRuntimeHandles,
) -> Result<(), HandlerError> {
    pi.register_tool(super::registration::tool_definition(
        "set_agent_model",
        "Set Agent Model",
        "Change a running child agent's model and/or reasoning effort by agent ID. The current model request is not interrupted; the change applies from the child's next model request.",
        Some(vec!["Use set_agent_model only when the user asks to change a running sub-agent's model or effort.".into()]),
        set_agent_model_schema(),
        Arc::new(move |_tool_call_id, params, _signal, _on_update, _ctx| {
            let store = Arc::clone(&store);
            let handles = handles.clone();
            Box::pin(async move {
                let params = pi_js_compat::js_json::from_value(&params).map_err(|error| HandlerError::new(error.to_string()))?;
                set_agent_model(&store, &handles, params).await
            })
        }),
    ))
}
