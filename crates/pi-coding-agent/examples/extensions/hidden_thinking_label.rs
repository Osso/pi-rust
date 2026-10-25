//! Opt-in native `hidden-thinking-label.ts`; embed and register explicitly.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};
const DEFAULT_LABEL: &str = "Pondering...";

pub fn hidden_thinking_label_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let label = Arc::new(Mutex::new(JsString::from(DEFAULT_LABEL)));
    let current = label.clone();
    pi.on_session_start(move |_event, ctx: ExtensionContext| {
        let label = current.lock().unwrap().clone();
        Box::pin(async move {
            ctx.ui()?.set_hidden_thinking_label(Some(&JsString::from(&label)));
            Ok(None)
        }) as BoxFuture<'static, Result<Option<()>, HandlerError>>
    })?;
    pi.register_command(
        "thinking-label",
        Some("Set the hidden thinking label. Use without args to reset.".into()),
        None,
        Arc::new(move |args, ctx| {
            let current = label.clone();
            Box::pin(async move {
                let next = args.trim();
                let ui = ctx.ui()?;
                if next.is_empty() {
                    *current.lock().unwrap() = DEFAULT_LABEL.into();
                    ui.set_hidden_thinking_label(None);
                    ui.notify(
                        &JsString::from(format!("Hidden thinking label reset to: {DEFAULT_LABEL}")),
                        None,
                    )?;
                } else {
                    *current.lock().unwrap() = next.clone();
                    ui.set_hidden_thinking_label(Some(&next));
                    ui.notify(&js_format!("Hidden thinking label set to: {}", next), None)?;
                }
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
