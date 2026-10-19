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
        Some("Restart Pi and resume the current session".to_owned()),
        None,
        Arc::new(|_args, ctx| {
            Box::pin(async move {
                ctx.restart(Some(restart_options()))?.await;
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)?;

    pi.register_tool(ToolDefinition {
        name: "restart_self".to_owned(),
        label: "Restart Self".to_owned(),
        description: "Restart the current Pi agent runtime and resume the same session.".to_owned(),
        prompt_snippet: None,
        prompt_guidelines: Some(vec![
            "Use this tool only when restarting Pi itself is necessary to pick up runtime or extension changes."
                .to_owned(),
            "After the restart, continue from the same session using the restart notice as the newest context."
                .to_owned(),
        ]),
        approval_required: Some(true),
        approval_kind: None,
        parameters: json!({"type": "object", "properties": {}}),
        constrained_sampling: None,
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(
            |_tool_call_id: String,
             _params: serde_json::Value,
             _signal: Option<tokio_util::sync::CancellationToken>,
             _on_update: Option<AgentToolUpdateCallback>,
             ctx: ExtensionContext| {
                Box::pin(async move {
                    ctx.restart(Some(restart_options()))?.await;
                    Ok(AgentToolResult {
                        content: vec![TextOrImageContent::Text(TextContent::new(
                            "Restart requested. Current session was reloaded.",
                        ))],
                        details: Some(json!({"restarted": true})),
                        usage: None,
                        added_tool_names: None,
                        is_error: None,
                        terminate: None,
                    })
                })
            },
        ),
        render_call: None,
        render_result: None,
    })
    .map_err(HandlerError::new)
}

fn restart_options() -> RestartOptions {
    RestartOptions {
        notice: Some(RESTART_NOTICE.to_owned()),
        process: Some(true),
    }
}
