//! Opt-in native user-message delivery commands. Input whitespace is preserved.
use pi_ai::types::{TextContent, TextOrImageContent};
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::sync::Arc;

pub fn send_user_message_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    for (name, description) in [
        ("ask", "Send a user message to the agent"),
        ("steer", "Send a steering message (interrupts current processing)"),
        ("followup", "Queue a follow-up message (waits for current processing)"),
        ("askwith", "Send a user message with structured content"),
    ] {
        let api = pi.clone();
        pi.register_command(
            name,
            Some(description.into()),
            None,
            Arc::new(move |args, ctx| {
                let api = api.clone();
                Box::pin(async move { deliver(&api, name, args, ctx).await })
            }),
        )?;
    }
    Ok(())
}

async fn deliver(
    api: &ExtensionApi,
    name: &str,
    args: JsString,
    ctx: ExtensionCommandContext,
) -> Result<(), HandlerError> {
    let ui = ctx.ui()?;
    if args.trim().is_empty() {
        ui.notify(
            &JsString::from(format!("Usage: /{name} <message>")),
            Some(NotifyType::Warning),
        )?;
        return Ok(());
    }
    let idle = ctx.is_idle()?;
    if !idle && (name == "ask" || name == "askwith") {
        let message = if name == "ask" {
            "Agent is busy. Use /steer or /followup instead."
        } else {
            "Agent is busy"
        };
        ui.notify(&JsString::from(message), Some(NotifyType::Warning))?;
        return Ok(());
    }
    let content = if name == "askwith" {
        UserMessageContent::Blocks(vec![
            TextOrImageContent::Text(TextContent::new(js_format!("User request: {}", args))),
            TextOrImageContent::Text(TextContent::new("Please respond concisely.")),
        ])
    } else {
        UserMessageContent::Text(args)
    };
    let options = if idle {
        None
    } else {
        Some(SendUserMessageOptions {
            deliver_as: Some(if name == "steer" {
                UserMessageDelivery::Steer
            } else {
                UserMessageDelivery::FollowUp
            }),
            expand_prompt_templates: None,
        })
    };
    api.send_user_message(content, options)?;
    if !idle && name == "followup" {
        ui.notify(&JsString::from("Follow-up queued"), Some(NotifyType::Info))?;
    }
    Ok(())
}
