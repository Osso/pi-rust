//! Bundled `self-restart` extension (`extensions/self-restart/src/index.ts`).

use std::sync::Arc;

use pi_agent_core::agent::types::{AgentToolResult, AgentToolUpdateCallback};
use pi_ai::types::{TextContent, TextOrImageContent};
use serde_json::json;

use crate::core::extensions::types::{ExtensionApi, ExtensionContext, HandlerError, RestartOptions, ToolDefinition};

const RESTART_NOTICE: &str = "The agent process was restarted by tool request. Continue from the current session.";

pub fn self_restart_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_command(
        "restart",
        Some("Restart Pi and resume the current session".into()),
        None,
        Arc::new(|_args, ctx| {
            Box::pin(async move {
                ctx.restart(Some(restart_options()))?.await?;
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::from)?;

    pi.register_tool(ToolDefinition {
        name: "restart_self".to_owned(),
        label: "Restart Self".into(),
        description: "Restart the current Pi agent runtime and resume the same session.".into(),
        prompt_snippet: None,
        prompt_guidelines: Some(vec![
            "Use this tool only when restarting Pi itself is necessary to pick up runtime or extension changes.".into(),
            "After the restart, continue from the same session using the restart notice as the newest context.".into(),
        ]),
        approval_required: Some(true),
        approval_kind: None,
        parameters: json!({"type": "object", "properties": {}}),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(
            |_tool_call_id: String,
             _params: pi_js_compat::js_json::JsValue,
             _signal: Option<pi_js_compat::abort::AbortSignal>,
             _on_update: Option<AgentToolUpdateCallback>,
             ctx: Option<ExtensionContext>| {
                Box::pin(async move {
                    let ctx = crate::core::extensions::types::require_extension_context(ctx)?;
                    ctx.restart(Some(restart_options()))?.await?;
                    Ok(AgentToolResult {
                        content: vec![TextOrImageContent::Text(TextContent::new(
                            "Restart requested. Current session was reloaded.",
                        ))],
                        details: Some(json!({"restarted": true}).into()),
                        is_error: None,
                        terminate: None,
                    })
                })
            },
        ),
        render_call: None,
        render_result: None,
    })
    .map_err(HandlerError::from)
}

fn restart_options() -> RestartOptions {
    RestartOptions {
        notice: Some(RESTART_NOTICE.to_owned().into()),
        process: Some(true),
    }
}
