//! Opt-in native Kimi deferred-tool loading demo. Calculator deliberately returns 42.
use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_json::JsValue;
use serde_json::json;
use std::sync::Arc;

pub fn kimi_deferred_tools_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_tool(ToolDefinition {
        name: "Calculator".into(), label: "Calculator".into(),
        description: "Evaluate a simple arithmetic expression.".into(),
        parameters: json!({"type":"object","properties":{"expr":{"type":"string","description":"An expression such as 100 + 500"}},"required":["expr"]}),
        prompt_snippet: None, prompt_guidelines: None, approval_required: None, approval_kind: None,
        constrained_sampling: None, render_shell: None, prepare_arguments: None, prepare_execution: None,
        execution_mode: None, render_call: None, render_result: None,
        execute: Arc::new(|_, _, _, _, _| Box::pin(async {
            Ok(AgentToolResult {
                content: vec![TextOrImageContent::Text(TextContent::new("42"))],
                details: Some(JsValue::from(json!({}))), is_error: None, terminate: None,
            })
        })),
    })?;
    let api = pi.clone();
    pi.register_tool(ToolDefinition {
        name: "tool_search".into(), label: "Tool Search".into(),
        description: "Find and activate tools for a capability.".into(),
        prompt_snippet: Some("Search for additional tools when the active tools cannot perform the task".into()),
        parameters: json!({"type":"object","properties":{"query":{"type":"string","description":"Capability to search for"}},"required":["query"]}),
        prompt_guidelines: None, approval_required: None, approval_kind: None,
        constrained_sampling: None, render_shell: None, prepare_arguments: None, prepare_execution: None,
        execution_mode: None, render_call: None, render_result: None,
        execute: Arc::new(move |_, params, _, _, _| {
            let api = api.clone();
            Box::pin(async move {
                let query = params["query"].as_string().ok_or_else(|| HandlerError::new("query must be a string"))?;
                let (text, details) = if query.to_lowercase().contains("calc") {
                    let mut active = api.get_active_tools()?;
                    let added = if active.iter().any(|name| name == "Calculator") { vec![] } else { vec!["Calculator"] };
                    if !added.is_empty() {
                        active.push("Calculator".into());
                        api.set_active_tools(active)?;
                    }
                    ("Success. Found 1 matching tool(s)", json!({"matches":["Calculator"],"added":added}))
                } else {
                    ("The relevant tools do not exist.", json!({"matches":[],"added":[]}))
                };
                Ok(AgentToolResult {
                    content: vec![TextOrImageContent::Text(TextContent::new(text))], details: Some(JsValue::from(details)),
                    is_error: None, terminate: None,
                })
            })
        }),
    })?;
    let api = pi.clone();
    pi.on_session_start(
        move |_, _: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let api = api.clone();
            Box::pin(async move {
                api.set_active_tools(vec!["tool_search".into()])?;
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}
