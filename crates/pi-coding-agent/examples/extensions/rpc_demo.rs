//! Opt-in native RPC extension UI demonstration; register explicitly.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::js_regex::JsRegex;
use std::sync::{
    Arc, LazyLock,
    atomic::{AtomicU64, Ordering},
};

static RECURSIVE_RM: LazyLock<JsRegex> =
    LazyLock::new(|| JsRegex::with_flags(r"\brm\s+(-rf?|--recursive)", "i").expect("literal rm regex"));
static SUDO: LazyLock<JsRegex> = LazyLock::new(|| JsRegex::with_flags(r"\bsudo\b", "i").expect("literal sudo regex"));

pub fn rpc_demo_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    register_lifecycle(pi)?;
    pi.on_tool_call(|event, ctx| {
        Box::pin(confirm_dangerous(event, ctx)) as BoxFuture<'static, Result<Option<ToolCallEventResult>, HandlerError>>
    })?;
    pi.on_session_before_switch(|event, ctx| {
        Box::pin(confirm_clear(event, ctx))
            as BoxFuture<'static, Result<Option<SessionBeforeSwitchResult>, HandlerError>>
    })?;
    pi.register_command(
        "rpc-input",
        Some("Prompt for text input (demonstrates ctx.ui.input in RPC)".into()),
        None,
        Arc::new(|_, ctx| Box::pin(prompt_input(ctx))),
    )?;
    pi.register_command(
        "rpc-editor",
        Some("Open multi-line editor (demonstrates ctx.ui.editor in RPC)".into()),
        None,
        Arc::new(|_, ctx| Box::pin(prompt_editor(ctx))),
    )?;
    pi.register_command(
        "rpc-prefill",
        Some("Prefill the input editor (demonstrates ctx.ui.setEditorText in RPC)".into()),
        None,
        Arc::new(|_, ctx| {
            Box::pin(async move {
                let ui = ctx.ui()?;
                ui.set_editor_text(&"This text was set by the rpc-demo extension.".into());
                ui.notify(&"Editor prefilled".into(), Some(NotifyType::Info));
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
fn register_lifecycle(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let turns = Arc::new(AtomicU64::new(0));
    let count = turns.clone();
    pi.on_session_start(move |event: SessionStartEvent, ctx: ExtensionContext| {
        let count = count.clone();
        Box::pin(async move {
            let ui = ctx.ui()?;
            ui.set_title(&JsString::from(if event.reason == SessionStartReason::New {
                "pi RPC Demo (new session)"
            } else {
                "pi RPC Demo"
            }));
            ui.set_widget(
                &"rpc-demo".into(),
                Some(&["--- RPC Extension UI Demo ---".into(), "Loaded and ready.".into()]),
                None,
            );
            ui.set_status(
                &"rpc-demo".into(),
                Some(&js_format!("Turns: {}", count.load(Ordering::Relaxed))),
            );
            Ok(None)
        }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })?;
    let count = turns.clone();
    pi.on_turn_start(move |_, ctx: ExtensionContext| {
        let count = count.clone();
        Box::pin(async move {
            let turn = count.fetch_add(1, Ordering::Relaxed) + 1;
            ctx.ui()?
                .set_status(&"rpc-demo".into(), Some(&js_format!("Turn {} running...", turn)));
            Ok(None)
        }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })?;
    pi.on_turn_end(move |_, ctx: ExtensionContext| {
        let count = turns.clone();
        Box::pin(async move {
            ctx.ui()?.set_status(
                &"rpc-demo".into(),
                Some(&js_format!("Turn {} done", count.load(Ordering::Relaxed))),
            );
            Ok(None)
        }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })
    .map_err(HandlerError::new)
}
async fn confirm_clear(
    event: SessionBeforeSwitchEvent,
    ctx: ExtensionContext,
) -> Result<Option<SessionBeforeSwitchResult>, HandlerError> {
    if event.reason != SessionSwitchReason::New || !ctx.has_ui()? {
        return Ok(None);
    }
    let ui = ctx.ui()?;
    if ui
        .confirm(&"Clear session?".into(), &"All messages will be lost.".into(), None)
        .await?
    {
        return Ok(None);
    }
    ui.notify(&"Clear cancelled".into(), Some(NotifyType::Info));
    Ok(Some(SessionBeforeSwitchResult { cancel: Some(true) }))
}
async fn prompt_input(ctx: ExtensionCommandContext) -> Result<(), HandlerError> {
    let ui = ctx.ui()?;
    let value = ui
        .input(&"Enter a value".into(), Some(&"type something...".into()), None)
        .await?;
    let message = value
        .filter(|v| !v.is_empty())
        .map(|v| js_format!("You entered: {}", v))
        .unwrap_or_else(|| "Input cancelled".into());
    ui.notify(&message, Some(NotifyType::Info));
    Ok(())
}
async fn prompt_editor(ctx: ExtensionCommandContext) -> Result<(), HandlerError> {
    let ui = ctx.ui()?;
    let text = ui
        .editor(&"Edit some text".into(), Some(&"Line 1\nLine 2\nLine 3".into()))
        .await?;
    let message = text
        .filter(|t| !t.is_empty())
        .map(|t| js_format!("Editor submitted ({} lines)", t.split('\n').count()))
        .unwrap_or_else(|| "Editor cancelled".into());
    ui.notify(&message, Some(NotifyType::Info));
    Ok(())
}
async fn confirm_dangerous(
    event: ToolCallEvent,
    ctx: ExtensionContext,
) -> Result<Option<ToolCallEventResult>, HandlerError> {
    if event.tool_name != ToolName::Bash {
        return Ok(None);
    }
    let command = event
        .input
        .get("command")
        .and_then(JsValue::as_string)
        .cloned()
        .unwrap_or_else(|| "undefined".into());
    if !RECURSIVE_RM.is_match(command.clone()) && !SUDO.is_match(command.clone()) {
        return Ok(None);
    }
    if !ctx.has_ui()? {
        return Ok(Some(block("Dangerous command blocked (no UI)")));
    }
    let ui = ctx.ui()?;
    let choice = ui
        .select(
            &js_format!("Dangerous command: {}", command),
            &["Allow".into(), "Block".into()],
            None,
        )
        .await?;
    if choice.as_ref().is_none_or(|choice| *choice != "Allow") {
        ui.notify(&"Command blocked by user".into(), Some(NotifyType::Warning));
        return Ok(Some(block("Blocked by user")));
    }
    ui.notify(&"Command allowed".into(), Some(NotifyType::Info));
    Ok(None)
}
fn block(reason: &str) -> ToolCallEventResult {
    ToolCallEventResult {
        block: Some(true),
        reason: Some(reason.into()),
        ..Default::default()
    }
}
