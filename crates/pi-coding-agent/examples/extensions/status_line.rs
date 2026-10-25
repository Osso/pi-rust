//! Opt-in native `status-line.ts`; count belongs to the extension, not the session.
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::theme::ThemeColor;
use pi_js_compat::number::js_number_to_string;

pub fn status_line_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    let turns = Arc::new(Mutex::new(0.0));
    pi.on_session_start(
        |_event, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            Box::pin(async move {
                let ui = ctx.ui()?;
                ui.set_status(
                    &JsString::from("status-demo"),
                    Some(ExtensionStatus::Text(ui.theme()?.fg(ThemeColor::Dim, "Ready"))),
                )?;
                Ok(None)
            })
        },
    )?;
    let started = turns.clone();
    pi.on_turn_start(
        move |_event, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let turns = started.clone();
            Box::pin(async move {
                let count = {
                    let mut count = turns.lock().map_err(|error| HandlerError::new(error.to_string()))?;
                    *count += 1.0;
                    js_number_to_string(*count)
                };
                let ui = ctx.ui()?;
                let theme = ui.theme()?;
                let spinner = theme.fg(ThemeColor::Accent, "●");
                let text = theme.fg(ThemeColor::Dim, format!(" Turn {count}..."));
                ui.set_status(
                    &JsString::from("status-demo"),
                    Some(ExtensionStatus::Text(js_format!("{}{}", spinner, text))),
                )?;
                Ok(None)
            })
        },
    )?;
    pi.on_turn_end(
        move |_event, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            let turns = turns.clone();
            Box::pin(async move {
                let count = js_number_to_string(*turns.lock().map_err(|error| HandlerError::new(error.to_string()))?);
                let ui = ctx.ui()?;
                let theme = ui.theme()?;
                let check = theme.fg(ThemeColor::Success, "✓");
                let text = theme.fg(ThemeColor::Dim, format!(" Turn {count} complete"));
                ui.set_status(
                    &JsString::from("status-demo"),
                    Some(ExtensionStatus::Text(js_format!("{}{}", check, text))),
                )?;
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}
