//! Behavioral tests for `coding-agent/src/core/compaction/utils.ts` serialization.

use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantRole, Message, StopReason, TextContent, TextOrImageContent,
    ThinkingContent, ThinkingTag, ToolCall, ToolResultMessage, ToolResultRole, Usage, UserContent, UserMessage,
    UserRole,
};
use pi_coding_agent::core::compaction::serialize_conversation;
use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};

fn tool_result(text: &str, details: Option<Value>) -> Message {
    Message::ToolResult(ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: "tc1".to_owned(),
        tool_name: "read".to_owned(),
        content: vec![TextOrImageContent::Text(TextContent::new(text))],
        details: details.map(JsValue::from),
        is_error: false,
        timestamp: 0.0,
    })
}

fn assistant(content: Vec<AssistantContent>) -> Message {
    Message::Assistant(Box::new(AssistantMessage {
        role: AssistantRole::Tag,
        content,
        api: "anthropic".to_owned(),
        provider: "anthropic".to_owned(),
        model: "test".to_owned(),
        response_model: None,
        response_id: None,
        image_generation_result: None,
        diagnostics: None,
        usage: Usage::default(),
        stop_reason: StopReason::Stop,
        error_message: None,
        timestamp: 0.0,
    }))
}

#[test]
fn truncates_long_tool_results() {
    let long_content = "x".repeat(5000);
    let result = serialize_conversation(&[tool_result(&long_content, None)]);

    assert!(result.contains("[Tool result]:"));
    assert!(result.contains("[... 3000 more characters truncated]"));
    assert!(!result.contains(&"x".repeat(3000)));
    assert!(result.contains(&"x".repeat(2000)));
}

#[test]
fn preserves_full_output_paths_for_truncated_tool_results() {
    let text = format!(
        "{}\n[Showing lines 1-5000 of 5000. Full output: /tmp/pi-full-output.log]",
        "x".repeat(5000)
    );
    let result = serialize_conversation(&[tool_result(
        &text,
        Some(json!({
            "truncation": { "truncated": true },
            "fullOutputPath": "/tmp/pi-full-output.log",
        })),
    )]);

    assert!(result.contains("more characters truncated]"));
    assert!(result.contains("[Output truncated. Full output: /tmp/pi-full-output.log]"));
}

#[test]
fn truncates_long_tool_call_arguments() {
    let arguments = json!({ "path": "large.txt", "content": "x".repeat(5000) })
        .as_object()
        .expect("object")
        .clone();
    let result = serialize_conversation(&[assistant(vec![AssistantContent::ToolCall(ToolCall::new(
        "tc1",
        "write",
        arguments
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
    ))])]);

    assert!(result.contains("[Assistant tool calls]: write("));
    assert!(result.contains("path=\"large.txt\""));
    assert!(result.contains("[... 4002 more characters truncated]"));
    assert!(!result.contains(&"x".repeat(3000)));
}

#[test]
fn keeps_short_tool_results_intact() {
    let short_content = "x".repeat(1500);
    let result = serialize_conversation(&[tool_result(&short_content, None)]);
    assert_eq!(result, format!("[Tool result]: {short_content}"));
    assert!(!result.contains("truncated"));
}

#[test]
fn omits_assistant_thinking() {
    let result = serialize_conversation(&[assistant(vec![
        AssistantContent::Thinking(ThinkingContent {
            r#type: ThinkingTag::Tag,
            thinking: JsString::from("private chain of thought"),
            thinking_signature: None,
            redacted: None,
            index: None,
        }),
        AssistantContent::Text(TextContent::new("visible answer")),
    ])]);

    assert!(result.contains("[Assistant]: visible answer"));
    assert!(!result.contains("private chain of thought"));
}

#[test]
fn does_not_truncate_assistant_or_user_messages() {
    let long_text = "y".repeat(5000);
    let messages = vec![
        Message::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Blocks(vec![TextOrImageContent::Text(TextContent::new(long_text.clone()))]),
            input_source: None,
            provider_native: None,
            timestamp: 0.0,
        }),
        assistant(vec![AssistantContent::Text(TextContent::new(long_text.clone()))]),
    ];

    let result = serialize_conversation(&messages);
    assert!(!result.contains("truncated"));
    assert!(result.contains(&long_text));
}

#[test]
fn joins_text_blocks_and_tool_calls_in_source_order() {
    let messages = vec![
        Message::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Blocks(vec![
                TextOrImageContent::Text(TextContent::new("first")),
                TextOrImageContent::Text(TextContent::new("second")),
            ]),
            input_source: None,
            provider_native: None,
            timestamp: 0.0,
        }),
        assistant(vec![
            AssistantContent::Text(TextContent::new("line one")),
            AssistantContent::Text(TextContent::new("line two")),
            AssistantContent::ToolCall(ToolCall::new(
                "a",
                "read",
                (json!({ "path": "a.ts" }).as_object().expect("object").clone())
                    .into_iter()
                    .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                    .collect::<pi_js_compat::js_json::JsObject>(),
            )),
            AssistantContent::ToolCall(ToolCall::new(
                "b",
                "edit",
                (json!({ "path": "b.ts" }).as_object().expect("object").clone())
                    .into_iter()
                    .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
                    .collect::<pi_js_compat::js_json::JsObject>(),
            )),
        ]),
    ];

    assert_eq!(
        serialize_conversation(&messages),
        "[User]: firstsecond\n\n[Assistant]: line one\nline two\n\n[Assistant tool calls]: read(path=\"a.ts\"); edit(path=\"b.ts\")"
    );
}

#[test]
fn serializes_numeric_keys_and_nested_values_in_js_order() {
    let arguments: serde_json::Map<String, Value> =
        serde_json::from_str(r#"{"nested":{"2":2.0,"1":1.0,"01":3},"2":2.0,"1":1.0,"4294967295":4,"01":5}"#).unwrap();
    let messages = [assistant(vec![AssistantContent::ToolCall(ToolCall::new(
        "id",
        "inspect",
        arguments
            .into_iter()
            .map(|(key, value)| (JsString::from(key), JsValue::from(value)))
            .collect::<pi_js_compat::js_json::JsObject>(),
    ))])];
    assert_eq!(
        serialize_conversation(&messages),
        r#"[Assistant tool calls]: inspect(1=1, 2=2, nested={"1":1,"2":2,"01":3}, 4294967295=4, 01=5)"#
    );
}
