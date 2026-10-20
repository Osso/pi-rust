//! Opt-in native durable status-card entries, never sent to the LLM.
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::theme::{ThemeBg, ThemeColor};
use pi_js_compat::{
    date::{js_now_millis, to_locale_string},
    string::js_trim,
};
use pi_tui::{
    components::{r#box::BoxComponent, text::Text},
    tui::component,
};
use serde_json::json;
use std::{rc::Rc, sync::Arc};

pub fn entry_renderer_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_entry_renderer(
        "status-card",
        Arc::new(|entry, options, theme| {
            let data = entry
                .data
                .clone()
                .filter(|data| !data.is_null())
                .unwrap_or_else(|| json!({"message":"No data","timestamp":js_now_millis() as i64}));
            let theme = theme.clone();
            let bg_theme = theme.clone();
            let mut card = BoxComponent::new(
                1,
                1,
                Some(Rc::new(move |text| bg_theme.bg(ThemeBg::CustomMessageBg, text))),
            );
            card.add_child(component(Text::new(
                format!(
                    "{} {}",
                    theme.fg(ThemeColor::Accent, "[status]"),
                    data["message"].as_str().expect("status-card message must be a string")
                ),
                0,
                0,
            )));
            if options.expanded {
                let time = to_locale_string(
                    data["timestamp"]
                        .as_f64()
                        .expect("status-card timestamp must be a number") as i64,
                )
                .expect("cannot format local date: unavailable time zone or unsupported calendar range");
                card.add_child(component(Text::new(theme.fg(ThemeColor::Dim, &time), 0, 0)));
            }
            Some(component(card))
        }),
    )?;
    let api = pi.clone();
    pi.register_command("status-card", Some("Render a durable status card that is not sent to the LLM".into()),None,Arc::new(move |args,_| {
        let api = api.clone();
        Box::pin(async move {
            let message=js_trim(&args);
            api.append_entry("status-card",Some(json!({"message":if message.is_empty() {"Status card"} else {message},"timestamp":js_now_millis() as i64})))?;
            Ok(())
        })
    })).map_err(HandlerError::new)
}
