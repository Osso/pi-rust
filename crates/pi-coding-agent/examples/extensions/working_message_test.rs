//! Opt-in native port of `working-message-test.ts`; register its factory explicitly.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_json::JsString;
pub fn working_message_test_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.on_session_start(
        |_, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            Box::pin(async move {
                let ui = ctx.ui()?;
                ui.set_working_message(Some(&JsString::from("\x1b[38;2;155;86;63mWorking... (custom)\x1b[39m")));
                ui.set_working_indicator(Some(&WorkingIndicatorOptions {
                    frames: Some(vec!["\x1b[38;2;155;86;63m●\x1b[39m".into()]),
                    interval_ms: None,
                }));
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}
