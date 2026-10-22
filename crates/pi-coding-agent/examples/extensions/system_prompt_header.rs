//! Opt-in native `system-prompt-header.ts`; displays JS UTF-16 prompt length.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_json::JsString;

pub fn system_prompt_header_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.on_agent_start(
        |_event, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            Box::pin(async move {
                let length = ctx.get_system_prompt()?.encode_utf16().count();
                ctx.ui()?.set_status(
                    &JsString::from("system-prompt"),
                    Some(&format!("System: {length} chars")),
                );
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)?;
    pi.on_session_shutdown(
        |_event, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            Box::pin(async move {
                ctx.ui()?.set_status(&JsString::from("system-prompt"), None);
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}
