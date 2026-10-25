//! Opt-in native clean-shutdown commands and tools; deployment is illustrative only.
use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};
use std::sync::Arc;

fn result(text: &JsString, details: Value) -> AgentToolResult {
    AgentToolResult {
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: Some(JsValue::from(details)),
        is_error: None,
        terminate: None,
    }
}

pub fn shutdown_command_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_command(
        "quit",
        Some("Exit pi cleanly".into()),
        None,
        Arc::new(|_, ctx| {
            Box::pin(async move {
                ctx.shutdown()?;
                Ok(())
            })
        }),
    )?;
    pi.register_tool(ToolDefinition {
        name: "finish_and_exit".into(),
        label: "Finish and Exit".into(),
        description: "Complete a task and exit pi".into(),
        parameters: json!({"type":"object","properties":{}}),
        prompt_snippet: None,
        prompt_guidelines: None,
        approval_required: None,
        approval_kind: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        render_call: None,
        render_result: None,
        execute: Arc::new(|_, _, _, _, ctx| {
            Box::pin(async move {
                let ctx = pi_coding_agent::core::extensions::types::require_extension_context(ctx)?;
                ctx.shutdown()?;
                Ok(result(
                    &JsString::from("Shutdown requested. Exiting after this response."),
                    json!({}),
                ))
            })
        }),
    })?;
    pi.register_tool(ToolDefinition {
        name: "deploy_and_exit".into(), label: "Deploy and Exit".into(),
        description: "Deploy the application and exit pi".into(),
        parameters: json!({"type":"object","properties":{"environment":{"type":"string","description":"Target environment (e.g., production, staging)"}},"required":["environment"]}),
        prompt_snippet: None, prompt_guidelines: None, approval_required: None, approval_kind: None,
        render_shell: None, prepare_arguments: None, prepare_execution: None,
        execution_mode: None, render_call: None, render_result: None,
        execute: Arc::new(|_, params, _, on_update, ctx| Box::pin(async move {
let ctx = pi_coding_agent::core::extensions::types::require_extension_context(ctx)?;
            let environment = params["environment"].as_string().ok_or_else(|| HandlerError::new("environment must be a string"))?;
            if let Some(update) = on_update {
                update(result(&js_format!("Deploying to {}...", environment), json!({})))?;
                update(result(&JsString::from("Deployment complete, exiting..."), json!({})))?;
            }
            ctx.shutdown()?;
            Ok(result(&JsString::from("Done! Shutdown requested."), json!({"environment":environment})))
        })),
    })
}
