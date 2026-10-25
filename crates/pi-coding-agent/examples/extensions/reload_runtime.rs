//! Opt-in native reload example: tools queue a command, commands reload directly.
use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_json::JsValue;
use serde_json::json;
use std::sync::Arc;

pub fn reload_runtime_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_command(
        "reload-runtime",
        Some("Reload extensions, skills, prompts, and themes".into()),
        None,
        Arc::new(|_, ctx| {
            Box::pin(async move {
                ctx.reload().await?;
                Ok(())
            })
        }),
    )?;
    let api = pi.clone();
    pi.register_tool(ToolDefinition {
        name: "reload_runtime".into(),
        label: "Reload Runtime".into(),
        description: "Reload extensions, skills, prompts, and themes".into(),
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
        execute: Arc::new(move |_, _, _, _, _| {
            let api = api.clone();
            Box::pin(async move {
                api.send_user_message(
                    UserMessageContent::Text("/reload-runtime".into()),
                    Some(SendUserMessageOptions {
                        deliver_as: Some(UserMessageDelivery::FollowUp),
                        expand_prompt_templates: None,
                    }),
                )?;
                Ok(AgentToolResult {
                    content: vec![TextOrImageContent::Text(TextContent::new(
                        "Queued /reload-runtime as a follow-up command.",
                    ))],
                    details: Some(JsValue::from(json!({}))),
                    is_error: None,
                    terminate: None,
                })
            })
        }),
    })
}
