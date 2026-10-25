//! Opt-in native `confirm-destructive.ts`; embed and register explicitly.
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::session_manager::{KnownSessionEntry, SessionEntry};
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;

pub fn confirm_destructive_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.on_session_before_switch(|event, ctx| {
        Box::pin(confirm_switch(event, ctx))
            as BoxFuture<'static, Result<Option<SessionBeforeSwitchResult>, HandlerError>>
    })?;
    pi.on_session_before_fork(|event: SessionBeforeForkEvent, ctx: ExtensionContext| {
        Box::pin(async move {
            if !ctx.has_ui()? {
                return Ok(None);
            }
            let prefix = event.entry_id.slice(0, 8);
            let ui = ctx.ui()?;
            let choice = ui
                .select(
                    &JsString::from(js_format!("Fork from entry {}?", prefix)),
                    &["Yes, create fork".into(), "No, stay in current session".into()],
                    None,
                )
                .await?;
            if choice.as_ref() == Some(&JsString::from("Yes, create fork")) {
                return Ok(None);
            }
            ui.notify(&JsString::from("Fork cancelled"), Some(NotifyType::Info))?;
            Ok(Some(SessionBeforeForkResult {
                cancel: Some(true),
                ..Default::default()
            }))
        }) as BoxFuture<'static, Result<Option<SessionBeforeForkResult>, HandlerError>>
    })
    .map_err(HandlerError::new)
}

async fn confirm_switch(
    event: SessionBeforeSwitchEvent,
    ctx: ExtensionContext,
) -> Result<Option<SessionBeforeSwitchResult>, HandlerError> {
    if !ctx.has_ui()? {
        return Ok(None);
    }
    let (title, message, cancelled) = if event.reason == SessionSwitchReason::New {
        (
            "Clear session?",
            "This will delete all messages in the current session.",
            "Clear cancelled",
        )
    } else {
        let has_user = ctx.session_manager()?.get_entries().iter().any(|entry| matches!(entry, SessionEntry::Known(KnownSessionEntry::Message(message)) if message.message.role() == "user"));
        if !has_user {
            return Ok(None);
        }
        (
            "Switch session?",
            "You have messages in the current session. Switch anyway?",
            "Switch cancelled",
        )
    };
    let ui = ctx.ui()?;
    if ui
        .confirm(&JsString::from(title), &JsString::from(message), None)
        .await?
    {
        return Ok(None);
    }
    ui.notify(&JsString::from(cancelled), Some(NotifyType::Info))?;
    Ok(Some(SessionBeforeSwitchResult { cancel: Some(true) }))
}
