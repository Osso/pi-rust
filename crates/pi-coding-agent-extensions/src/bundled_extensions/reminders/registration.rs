use std::sync::Arc;

use pi_agent_core::AgentToolResult;
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_js_compat::date::js_now_millis;

use crate::core::extensions::types::{
    BoxFuture, ExtensionApi, ExtensionCommandContext, ExtensionContext, HandlerError, NotifyType, ToolDefinition,
    require_extension_context,
};

use super::delivery::ReminderDelivery;
use super::interface::{
    COMMAND_DESCRIPTION, TOOL_DESCRIPTION, TOOL_GUIDELINES, TOOL_SNIPPET, command_notification, execute_reminders_tool,
    reminder_tool_schema,
};

fn reminder_tool() -> ToolDefinition {
    ToolDefinition {
        name: "reminders".to_owned(),
        label: "Reminders".into(),
        description: TOOL_DESCRIPTION.into(),
        prompt_snippet: Some(TOOL_SNIPPET.into()),
        prompt_guidelines: Some(TOOL_GUIDELINES.iter().map(|text| (*text).into()).collect()),
        approval_required: None,
        approval_kind: None,
        parameters: reminder_tool_schema(),
        render_shell: None,
        prepare_arguments: None,
        prepare_execution: None,
        execution_mode: None,
        execute: Arc::new(|_id, params, _signal, _update, ctx| {
            Box::pin(async move {
                let ctx = require_extension_context(ctx)?;
                let path = ctx.control_db_path()?;
                let text =
                    execute_reminders_tool(&params, path.as_ref(), js_now_millis()).map_err(HandlerError::new)?;
                Ok(AgentToolResult {
                    content: vec![TextOrImageContent::Text(TextContent::new(text))],
                    details: None,
                    is_error: None,
                    terminate: None,
                })
            })
        }),
        render_call: None,
        render_result: None,
    }
}

pub fn reminders_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let delivery = ReminderDelivery::new(pi.clone());
    pi.on_session_start({
        let delivery = delivery.clone();
        move |_event, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let delivery = delivery.clone();
            Box::pin(async move {
                delivery.start(&ctx, 60_000)?;
                Ok(None)
            })
        }
    })
    .map_err(HandlerError::new)?;
    pi.on_session_shutdown({
        let delivery = delivery.clone();
        move |_event, _ctx| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let delivery = delivery.clone();
            Box::pin(async move {
                delivery.stop();
                Ok(None)
            })
        }
    })
    .map_err(HandlerError::new)?;
    pi.on_agent_end({
        let delivery = delivery.clone();
        move |_event, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let delivery = delivery.clone();
            Box::pin(async move {
                delivery.settle(&ctx);
                Ok(None)
            })
        }
    })
    .map_err(HandlerError::new)?;
    pi.register_tool(reminder_tool())?;
    pi.register_command(
        "reminders",
        Some(COMMAND_DESCRIPTION.into()),
        None,
        Arc::new(|args, ctx: ExtensionCommandContext| {
            Box::pin(async move {
                let path = ctx.control_db_path()?;
                let (message, level) = command_notification(&args, path.as_ref(), js_now_millis());
                let level = if level == "error" {
                    NotifyType::Error
                } else {
                    NotifyType::Info
                };
                ctx.ui()?.notify(&message, Some(level))?;
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)?;
    Ok(())
}
