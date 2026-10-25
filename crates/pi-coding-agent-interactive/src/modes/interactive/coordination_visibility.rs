//! Frozen `coding-agent/src/modes/interactive/coordination-visibility.ts`.
use std::sync::LazyLock;

use pi_agent_core::agent::AgentMessage;
use pi_ai::types::{InputSource, TextOrImageContent, UserContent};
use pi_js_compat::js_json::{JsString, JsValue};
use pi_js_compat::js_regex::JsRegex;

use crate::core::agent_session::QueuedMessagePreview;

static MAILBOX_ENVELOPE: LazyLock<JsRegex> = LazyLock::new(|| {
    JsRegex::new(r"^From:\n(?:- host: [^\n]+ \(reply with toHost and toSessionId\)\n)?(?:- session: [^\n]+\n)?- agent: [^\n]+\n\nMessage:\n")
        .expect("frozen mailbox envelope regex")
});

pub fn is_coordination_preview(message: &QueuedMessagePreview) -> bool {
    message
        .custom_type
        .as_ref()
        .is_some_and(|kind| kind == "shared_channel")
        || (message.input_source == Some(InputSource::Extension) && MAILBOX_ENVELOPE.is_match(&message.text))
}

pub fn is_coordination_message(message: &AgentMessage) -> bool {
    match message {
        AgentMessage::Custom(object) => {
            object
                .get("role")
                .and_then(JsValue::as_string)
                .is_some_and(|role| role == "custom")
                && object
                    .get("customType")
                    .and_then(JsValue::as_string)
                    .is_some_and(|kind| kind == "shared_channel")
        }
        AgentMessage::User(user) if user.input_source == Some(InputSource::Extension) => {
            let text = match &user.content {
                UserContent::Text(text) => text.clone(),
                UserContent::Blocks(parts) => JsString::join(
                    parts.iter().filter_map(|part| match part {
                        TextOrImageContent::Text(text) => Some(text.text.clone()),
                        _ => None,
                    }),
                    "",
                ),
            };
            MAILBOX_ENVELOPE.is_match(text)
        }
        _ => false,
    }
}
