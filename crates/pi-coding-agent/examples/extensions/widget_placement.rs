//! Opt-in native port of `widget-placement.ts`; register its factory explicitly.
use pi_coding_agent::core::extensions::*;
use pi_js_compat::js_json::JsString;
pub fn widget_placement_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.on_session_start(
        |_, ctx: ExtensionContext| -> BoxFuture<'static, Result<Option<()>, HandlerError>> {
            Box::pin(async move {
                if ctx.has_ui()? {
                    let ui = ctx.ui()?;
                    ui.set_widget(
                        &JsString::from("widget-above"),
                        Some(&["Above editor widget".into()]),
                        None,
                    );
                    ui.set_widget(
                        &JsString::from("widget-below"),
                        Some(&["Below editor widget".into()]),
                        Some(&ExtensionWidgetOptions {
                            placement: Some(WidgetPlacement::BelowEditor),
                        }),
                    );
                }
                Ok(None)
            })
        },
    )
    .map_err(HandlerError::new)
}
