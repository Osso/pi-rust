//! `shouldContinueInterruptedSession` regressions used by RPC/print startup.

use pi_agent_core::agent::types::AgentMessage;
use pi_ai::types::{
    AssistantContent, AssistantMessage, StopReason, TextContent, ToolCall, ToolCallTag, ToolResultMessage,
    ToolResultRole, Usage, UserContent, UserMessage, UserRole,
};
use pi_coding_agent::core::agent_session::should_continue_interrupted_session;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::json;

fn user_message(text: &str) -> AgentMessage {
    AgentMessage::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![pi_ai::types::TextOrImageContent::Text(TextContent::new(text))]),
        input_source: None,
        provider_native: None,
        timestamp: 1.0,
    })
}

fn assistant_message(content: Vec<AssistantContent>, stop_reason: StopReason) -> AgentMessage {
    let mut message = AssistantMessage::empty("faux", "faux", "faux-1", 1.0);
    message.content = content;
    message.usage = Usage::default();
    message.stop_reason = stop_reason;
    AgentMessage::Assistant(Box::new(message))
}

fn end_turn_call() -> AssistantContent {
    AssistantContent::ToolCall(ToolCall {
        r#type: ToolCallTag::Tag,
        id: "call-1".to_owned(),
        name: "end_turn".to_owned(),
        arguments: (json!({"reason": "done"}).as_object().cloned().unwrap_or_default())
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
        execution: None,
        thought_signature: None,
        namespace: None,
        index: None,
        partial_json: None,
        partial_args: None,
        stream_index: None,
    })
}

fn end_turn_result(is_error: bool) -> AgentMessage {
    AgentMessage::ToolResult(ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: "call-1".to_owned(),
        tool_name: "end_turn".to_owned(),
        content: vec![pi_ai::types::TextOrImageContent::Text(TextContent::new(
            "Turn ended: done",
        ))],
        details: None,
        is_error,
        timestamp: 2.0,
    })
}

#[test]
fn empty_message_list_does_not_continue() {
    assert!(!should_continue_interrupted_session(&[]));
}

#[test]
fn a_trailing_user_message_continues() {
    assert!(should_continue_interrupted_session(&[user_message("resume me")]));
}

#[test]
fn a_completed_end_turn_batch_does_not_continue() {
    let messages = [
        user_message("hello"),
        assistant_message(vec![end_turn_call()], StopReason::ToolUse),
        end_turn_result(false),
    ];
    assert!(!should_continue_interrupted_session(&messages));
}

#[test]
fn an_unfinished_tool_batch_continues() {
    let messages = [
        user_message("hello"),
        assistant_message(vec![end_turn_call()], StopReason::ToolUse),
    ];
    assert!(should_continue_interrupted_session(&messages));
}

#[test]
fn an_aborted_assistant_message_continues() {
    let messages = [
        user_message("hello"),
        assistant_message(
            vec![AssistantContent::Text(TextContent::new("partial"))],
            StopReason::Aborted,
        ),
    ];
    assert!(should_continue_interrupted_session(&messages));
}

#[test]
fn self_restart_messages_are_skipped_before_the_check() {
    let mut restart = serde_json::Map::new();
    restart.insert("role".to_owned(), json!("custom"));
    restart.insert("customType".to_owned(), json!("self_restart"));
    restart.insert("content".to_owned(), json!("restarting"));
    let messages = [
        user_message("hello"),
        AgentMessage::Custom(
            restart
                .into_iter()
                .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                .collect::<pi_js_compat::js_json::JsObject>(),
        ),
    ];
    assert!(should_continue_interrupted_session(&messages));
}
