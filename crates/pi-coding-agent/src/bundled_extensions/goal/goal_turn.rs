//! Port of `packages/coding-agent/extensions/goal/src/goal-turn.ts`.

use pi_agent_core::agent::AgentMessage;
use pi_ai::types::{AssistantContent, StopReason};

use crate::core::extensions::types::AgentEndEvent;

/// `findLastAssistantMessage`.
pub fn find_last_assistant_message(event: &AgentEndEvent) -> Option<&pi_ai::types::AssistantMessage> {
    event.messages.iter().rev().find_map(AgentMessage::as_assistant)
}

/// `didLastAssistantAbort`.
pub fn did_last_assistant_abort(event: &AgentEndEvent) -> bool {
    find_last_assistant_message(event).is_some_and(|message| message.stop_reason == StopReason::Aborted)
}

/// `didLastAssistantReturnEmpty`.
pub fn did_last_assistant_return_empty(event: &AgentEndEvent) -> bool {
    let Some(message) = find_last_assistant_message(event) else {
        return false;
    };
    if message.stop_reason == StopReason::Aborted {
        return false;
    }
    let text: String = message
        .content
        .iter()
        .filter_map(|part| match part {
            AssistantContent::Text(text) => Some(text.text.as_str()),
            _ => None,
        })
        .collect();
    let has_tool_call = message
        .content
        .iter()
        .any(|part| matches!(part, AssistantContent::ToolCall(_)));
    text.trim().is_empty() && !has_tool_call
}
