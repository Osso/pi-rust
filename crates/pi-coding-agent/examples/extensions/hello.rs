//! Opt-in native `hello.ts`: embed this file and register its factory explicitly.
use pi_js_compat::js_format;
use std::sync::Arc;

use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::{ExtensionApi, HandlerError, ToolDefinition};
use serde_json::json;

pub fn hello_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_tool(ToolDefinition::define(ToolDefinition {
        name: "hello".into(),
        label: "Hello".into(),
        description: "A simple greeting tool".into(),
        parameters: json!({"type":"object","properties":{"name":{"type":"string","description":"Name to greet"}},"required":["name"]}),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(|_id, params, _signal, _on_update, _ctx| Box::pin(async move {
            let name = params["name"].as_string().ok_or_else(|| HandlerError::new("name must be a string"))?;
            Ok(AgentToolResult {
                content: vec![TextOrImageContent::Text(TextContent::new(js_format!("Hello, {}!", name)))],
                details: Some(pi_js_compat::js_json!({"greeted":name})),
                is_error: None,
                terminate: None,
            })
        })),
        render_call: None,
        render_result: None,
    }))
}
