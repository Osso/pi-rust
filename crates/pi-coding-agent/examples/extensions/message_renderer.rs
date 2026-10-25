//! Opt-in native custom status messages with expandable local-time details.
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::theme::{ThemeBg, ThemeColor};
use pi_js_compat::date::{LocaleDateFormat, js_now_millis, to_locale_string};
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_regex::JsRegex;
use pi_tui::{
    components::{r#box::BoxComponent, text::Text},
    tui::component,
};
use std::{rc::Rc, sync::Arc};

pub fn message_renderer_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_message_renderer(
        "status-update",
        Arc::new(|message, options, theme| {
            let level = message
                .details
                .as_ref()
                .and_then(|details| details["level"].as_string())
                .cloned()
                .unwrap_or_else(|| JsString::from("info"));
            let color = if level == "error" {
                ThemeColor::Error
            } else if level == "warn" {
                ThemeColor::Warning
            } else {
                ThemeColor::Success
            };
            let prefix = theme.fg(color, &js_format!("[{}]", level.to_uppercase()));
            let value = pi_js_compat::js_json::to_value(&message.content).expect("custom message content serializes");
            let content = pi_js_compat::js_json::to_js_string(&value).expect("typed custom content converts to text");
            let mut text = js_format!("{} {}", prefix, content);
            if let Some(timestamp) = message
                .details
                .as_ref()
                .and_then(|details| details["timestamp"].as_f64())
                .filter(|time| options.expanded && *time != 0.0)
            {
                let time = to_locale_string(timestamp, LocaleDateFormat::TimeEnUs);
                text.push_str(js_format!("\n{}", theme.fg(ThemeColor::Dim, &format!("  at {time}"))));
            }
            let bg_theme = theme.clone();
            let mut card = BoxComponent::new(
                1,
                1,
                Some(Rc::new(move |text| bg_theme.bg(ThemeBg::CustomMessageBg, text))),
            );
            card.add_child(component(Text::new(text, 0, 0)));
            Some(component(card))
        }),
    )?;
    let api = pi.clone();
    pi.register_command(
        "status",
        Some("Send a status message (usage: /status [warn|error] message)".into()),
        None,
        Arc::new(move |args, _| {
            let api = api.clone();
            Box::pin(async move {
                let trimmed = args.trim();
                let parts: Vec<_> = JsRegex::new(r"\S+")
                    .expect("valid status token regex")
                    .find_iter(&trimmed)
                    .map(|part| part.text())
                    .collect();
                let (level, content) =
                    if let Some(level) = parts.first().filter(|level| *level == "warn" || *level == "error") {
                        let content = JsString::join(&parts[1..], " ");
                        (
                            level.clone(),
                            if content.is_empty() {
                                JsString::from("Status update")
                            } else {
                                content
                            },
                        )
                    } else {
                        (JsString::from("info"), trimmed)
                    };
                api.send_message(
                    CustomMessagePayload {
                        custom_type: "status-update".into(),
                        content: CustomMessageContent::Text(content),
                        display: true,
                        details: Some(pi_js_compat::js_json!({"level":level,"timestamp":js_now_millis() as i64})),
                    },
                    None,
                )?;
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
