//! Opt-in native durable status-card entries, never sent to the LLM.
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::theme::{ThemeBg, ThemeColor};
use pi_js_compat::date::{LocaleDateFormat, js_now_millis, to_locale_string};
use pi_js_compat::js_format;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_tui::{
    components::{r#box::BoxComponent, text::Text},
    tui::component,
};
use std::{rc::Rc, sync::Arc};

pub fn entry_renderer_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_entry_renderer(
        "status-card",
        Arc::new(|entry, options, theme| {
            let data =
                entry.data.clone().filter(|data| !data.is_null()).unwrap_or_else(
                    || pi_js_compat::js_json!({"message":"No data","timestamp":js_now_millis() as i64}),
                );
            let theme = theme.clone();
            let bg_theme = theme.clone();
            let mut card = BoxComponent::new(
                1,
                1,
                Some(Rc::new(move |text| bg_theme.bg(ThemeBg::CustomMessageBg, text))),
            );
            card.add_child(component(Text::new(
                js_format!(
                    "{} {}",
                    theme.fg(ThemeColor::Accent, "[status]"),
                    data["message"]
                        .as_string()
                        .expect("status-card message must be a string")
                ),
                0,
                0,
            )));
            if options.expanded {
                let time = to_locale_string(
                    data["timestamp"]
                        .as_f64()
                        .expect("status-card timestamp must be a number"),
                    LocaleDateFormat::DateTimeEnUs,
                );
                card.add_child(component(Text::new(theme.fg(ThemeColor::Dim, &time), 0, 0)));
            }
            Some(component(card))
        }),
    )?;
    let api = pi.clone();
    pi.register_command("status-card", Some("Render a durable status card that is not sent to the LLM".into()),None,Arc::new(move |args,_| {
        let api = api.clone();
        Box::pin(async move {
            let message=args.trim();
            api.append_entry(&JsString::from("status-card"),Some(json!({"message":if message.is_empty() {JsString::from("Status card")} else {message},"timestamp":js_now_millis() as i64})))?;
            Ok(())
        })
    })).map_err(HandlerError::new)
}
