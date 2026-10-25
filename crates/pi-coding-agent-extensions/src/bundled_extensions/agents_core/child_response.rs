//! Current-turn assistant response extraction, retaining message identity.

use std::collections::HashSet;
use std::sync::Arc;

use pi_agent_core::AgentMessage;
use pi_ai::types::{AssistantContent, StopReason};
use pi_js_compat::js_json::JsString;

pub fn read_current_child_assistant_text(
    messages: &[Arc<AgentMessage>],
    previous_messages: &[Arc<AgentMessage>],
) -> Result<Option<JsString>, JsString> {
    let previous: HashSet<_> = previous_messages.iter().map(Arc::as_ptr).collect();
    let current: Vec<_> = messages
        .iter()
        .filter(|message| !previous.contains(&Arc::as_ptr(message)))
        .collect();
    let terminal = current.iter().rev().find_map(|message| message.as_assistant());
    if let Some(terminal) = terminal.filter(|message| message.stop_reason == StopReason::Error) {
        return Err(terminal
            .error_message
            .clone()
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| JsString::from("Child model request ended with error")));
    }
    for message in current.iter().rev() {
        let Some(assistant) = message.as_assistant() else {
            continue;
        };
        let text = JsString::join(
            assistant.content.iter().filter_map(|part| match part {
                AssistantContent::Text(text) => Some(text.text.clone()),
                _ => None,
            }),
            "\n",
        );
        if !text.is_empty() {
            return Ok(Some(text));
        }
    }
    Ok(None)
}
