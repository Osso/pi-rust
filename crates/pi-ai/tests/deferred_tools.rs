use pi_ai::types::{
    AssistantContent, AssistantMessage, Context, Message, StopReason, Tool, ToolCall, ToolResultContent,
    ToolResultMessage, UserContent, UserMessage,
};
use pi_ai::utils::deferred_tools::{split_deferred_tools, split_deferred_tools_with};
use serde_json::json;

fn tool(name: &str, description: &str) -> Tool {
    Tool {
        name: name.to_owned(),
        description: description.to_owned(),
        parameters: json!({"type": "object"}),
        constrained_sampling: None,
        typebox_kind: false,
    }
}

fn user(timestamp: i64) -> Message {
    Message::User(UserMessage {
        role: pi_ai::types::UserRole::Tag,
        content: UserContent::Text("hello".to_owned()),
        input_source: None,
        provider_native: None,
        timestamp,
    })
}

fn assistant_call(timestamp: i64, name: &str) -> Message {
    Message::Assistant(Box::new(AssistantMessage {
        content: vec![AssistantContent::ToolCall(ToolCall::new(
            "call_1",
            name,
            json!({}).as_object().cloned().unwrap(),
        ))],
        stop_reason: StopReason::ToolUse,
        timestamp,
        ..AssistantMessage::empty("anthropic-messages", "anthropic", "test", timestamp)
    }))
}

fn tool_result(timestamp: i64, added: Option<Vec<&str>>) -> Message {
    Message::ToolResult(ToolResultMessage {
        role: pi_ai::types::ToolResultRole::Tag,
        tool_call_id: "call_1".to_owned(),
        tool_name: "base_tool".to_owned(),
        content: vec![ToolResultContent::Text(pi_ai::types::TextContent::new("done"))],
        details: None,
        usage: None,
        added_tool_names: added.map(|names| names.into_iter().map(str::to_owned).collect()),
        is_error: false,
        timestamp,
    })
}

fn context(tools: Vec<Tool>, added: Option<Vec<&str>>) -> Context {
    Context {
        system_prompt: None,
        messages: vec![user(1), assistant_call(2, "base_tool"), tool_result(3, added), user(4)],
        tools: Some(tools),
    }
}

fn names(tools: &[Tool]) -> Vec<&str> {
    tools.iter().map(|tool| tool.name.as_str()).collect()
}

#[test]
fn disabled_splitting_keeps_every_tool_immediate() {
    let context = context(
        vec![tool("base_tool", "base"), tool("late_tool", "late")],
        Some(vec!["late_tool"]),
    );
    let split = split_deferred_tools(&context, false);
    assert_eq!(names(&split.immediate), ["base_tool", "late_tool"]);
    assert!(split.deferred.is_empty());
}

#[test]
fn transcript_loaded_tools_are_deferred() {
    let context = context(
        vec![tool("base_tool", "base"), tool("late_tool", "late")],
        Some(vec!["late_tool"]),
    );
    let split = split_deferred_tools(&context, true);
    assert_eq!(names(&split.immediate), ["base_tool"]);
    assert_eq!(split.deferred.len(), 1);
    assert_eq!(split.deferred[0].0, "late_tool");
}

#[test]
fn tools_already_used_before_the_result_stay_immediate() {
    let context = context(
        vec![tool("base_tool", "base"), tool("late_tool", "late")],
        Some(vec!["base_tool", "late_tool"]),
    );
    let split = split_deferred_tools(&context, true);
    assert_eq!(names(&split.immediate), ["base_tool"]);
    assert_eq!(split.deferred.len(), 1);
}

#[test]
fn custom_normalizer_applies_to_tools_and_transcript_names() {
    let context = context(
        vec![tool("BASE_TOOL", "base"), tool("LATE_TOOL", "late")],
        Some(vec!["Late_Tool"]),
    );
    let split = split_deferred_tools_with(&context, true, str::to_lowercase);
    assert_eq!(names(&split.immediate), ["BASE_TOOL"]);
    assert_eq!(split.deferred.len(), 1);
    assert_eq!(split.deferred[0].0, "late_tool");
}

#[test]
fn duplicate_tool_names_keep_first_position_and_last_value() {
    let context = Context {
        system_prompt: None,
        messages: vec![],
        tools: Some(vec![tool("a", "first"), tool("b", "b"), tool("a", "second")]),
    };
    let split = split_deferred_tools(&context, false);
    assert_eq!(names(&split.immediate), ["a", "b"]);
    assert_eq!(split.immediate[0].description, "second");
}
