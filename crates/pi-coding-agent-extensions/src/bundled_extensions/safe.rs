//! Bundled `safe` extension (`extensions/safe/src/index.ts`).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use pi_js_compat::js_json::JsString;

use crate::core::extensions::types::{
    ArgumentCompletions, BoxFuture, ExtensionApi, ExtensionCommandContext, ExtensionContext, HandlerError, NotifyType,
    ToolCallEvent, ToolCallEventResult, ToolGate,
};

const SAFE_STATUS_KEY: &str = "safe";
const SAFE_STATUS_TEXT: &str = "safe:on";
const SAFE_COMMAND_ACTIONS: [&str; 3] = ["on", "off", "status"];

pub fn safe_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let enabled = Arc::new(AtomicBool::new(false));

    let session_start_enabled = Arc::clone(&enabled);
    pi.on_session_start(
        move |_event, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let enabled = session_start_enabled.load(Ordering::SeqCst);
            Box::pin(async move {
                update_safe_status(&ctx, enabled)?;
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)?;

    let gate_enabled = Arc::clone(&enabled);
    pi.register_tool_gate(Arc::new(
        move |event: ToolCallEvent,
              _ctx: ExtensionContext|
              -> BoxFuture<'static, Result<Option<ToolCallEventResult>, HandlerError>> {
            let enabled = gate_enabled.load(Ordering::SeqCst);
            Box::pin(async move { Ok(block_disallowed_tool(&event, enabled)) })
        },
    ) as Arc<dyn ToolGate + Send + Sync>)
        .map_err(HandlerError::new)?;

    let command_enabled = Arc::clone(&enabled);
    let completions: ArgumentCompletions = Arc::new(|prefix: &JsString| {
        let matches: Vec<pi_js_compat::js_json::JsValue> = SAFE_COMMAND_ACTIONS
            .iter()
            .filter(|action| JsString::from(**action).starts_with(prefix))
            .map(|action| pi_js_compat::js_json!({"value": *action, "label": *action}))
            .collect();
        Ok(if matches.is_empty() { None } else { Some(matches) })
    });
    pi.register_command(
        "safe",
        Some("Restrict tool calls to web_search and ask_questions for this session.".into()),
        Some(completions),
        Arc::new(move |args: JsString, ctx: ExtensionCommandContext| {
            let enabled = Arc::clone(&command_enabled);
            Box::pin(async move {
                let action = parse_safe_command(if args.is_empty() { "status".into() } else { args });
                let Some(action) = action else {
                    ctx.ui()?
                        .notify(&"Usage: /safe on|off|status".into(), Some(NotifyType::Error))?;
                    return Ok(());
                };

                if action == "status" {
                    ctx.ui()?.notify(
                        &safe_mode_status(enabled.load(Ordering::SeqCst)).into(),
                        Some(NotifyType::Info),
                    )?;
                    return Ok(());
                }

                let next = action == "on";
                enabled.store(next, Ordering::SeqCst);
                update_safe_status(&ctx, next)?;
                ctx.ui()?.notify(
                    &if next {
                        "Safe mode enabled".into()
                    } else {
                        "Safe mode disabled".into()
                    },
                    Some(NotifyType::Info),
                )?;
                ctx.ui()?.set_editor_text(&JsString::default())?;
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}

fn safe_mode_status(enabled: bool) -> &'static str {
    if enabled { "Safe mode is on" } else { "Safe mode is off" }
}

fn update_safe_status(ctx: &ExtensionContext, enabled: bool) -> Result<(), HandlerError> {
    let text = enabled.then(|| JsString::from(SAFE_STATUS_TEXT));
    ctx.ui()?.set_status(
        &SAFE_STATUS_KEY.into(),
        text.map(crate::core::footer_data_provider::ExtensionStatus::Text),
    )?;
    Ok(())
}

fn parse_safe_command(args: JsString) -> Option<&'static str> {
    let value = args.trim();
    SAFE_COMMAND_ACTIONS.into_iter().find(|action| value == *action)
}

fn block_disallowed_tool(event: &ToolCallEvent, enabled: bool) -> Option<ToolCallEventResult> {
    if !enabled || is_allowed_safe_mode_tool(event.tool_name.as_str()) {
        return None;
    }
    Some(ToolCallEventResult {
        block: Some(true),
        reason: Some(format!("Safe mode blocks tool: {}", event.tool_name.as_str()).into()),
        ..Default::default()
    })
}

fn is_allowed_safe_mode_tool(tool_name: &str) -> bool {
    matches!(tool_name, "web_search" | "ask_questions")
}
