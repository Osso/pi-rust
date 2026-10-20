//! Opt-in native custom status messages with expandable local-time details.
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::modes::interactive::theme::ThemeColor;
use pi_js_compat::{
    date::{js_now_millis, to_locale_string},
    string::{is_js_whitespace_char, js_trim},
};
use pi_tui::{
    components::{r#box::BoxComponent, text::Text},
    tui::component,
};
use serde_json::json;
use std::{rc::Rc, sync::Arc};

pub fn message_renderer_extension(pi: &ExtensionApi) -> Result<(), HandlerError> {
    pi.register_message_renderer(
        "status-update",
        Arc::new(|message, options, theme| {
            let level = message
                .details
                .as_ref()
                .and_then(|details| details["level"].as_str())
                .unwrap_or("info");
            let color = match level {
                "error" => ThemeColor::Error,
                "warn" => ThemeColor::Warning,
                _ => ThemeColor::Success,
            };
            let prefix = theme.fg(color, &format!("[{}]", level.to_uppercase()));
            let content = match &message.content {
                CustomMessageContent::Text(text) => text.as_str(),
                _ => panic!("status-update content must be a string"),
            };
            let mut text = format!("{prefix} {content}");
            if let Some(timestamp) = message
                .details
                .as_ref()
                .and_then(|details| details["timestamp"].as_f64())
                .filter(|time| options.expanded && *time != 0.0)
            {
                // en-US toLocaleString is date + comma-space + toLocaleTimeString.
                let local = to_locale_string(timestamp as i64)
                    .expect("cannot format local time: unavailable time zone or unsupported calendar range");
                let (_, time) = local.split_once(", ").expect("en-US local date separator");
                text.push_str(&format!("\n{}", theme.fg(ThemeColor::Dim, &format!("  at {time}"))));
            }
            let bg_theme = theme.clone();
            let mut card = BoxComponent::new(
                options.output_pad as usize,
                1,
                Some(Rc::new(move |text| bg_theme.bg(ThemeColor::CustomMessageBg, text))),
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
                let trimmed = js_trim(&args);
                let parts: Vec<_> = trimmed
                    .split(is_js_whitespace_char)
                    .filter(|part| !part.is_empty())
                    .collect();
                let (level, content) = match parts.first().copied() {
                    Some(level @ ("warn" | "error")) => {
                        let content = parts[1..].join(" ");
                        (
                            level,
                            if content.is_empty() {
                                "Status update".into()
                            } else {
                                content
                            },
                        )
                    }
                    _ => ("info", trimmed.to_owned()),
                };
                api.send_message(
                    CustomMessagePayload {
                        custom_type: "status-update".into(),
                        content: CustomMessageContent::Text(content),
                        display: true,
                        details: Some(json!({"level":level,"timestamp":js_now_millis()})),
                    },
                    None,
                )?;
                Ok(())
            })
        }),
    )
    .map_err(HandlerError::new)
}
