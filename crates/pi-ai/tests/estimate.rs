use pi_ai::types::{
    AssistantContent, AssistantMessage, Context, Message, StopReason, TextContent, TextOrImageContent, Tool, ToolCall,
    ToolResultMessage, Usage, UserContent, UserMessage,
};
use pi_ai::utils::estimate::{
    TextOrImageInput, calculate_context_tokens, estimate_context_tokens, estimate_message_tokens,
    estimate_text_and_image_content_tokens, estimate_text_tokens,
};
use serde_json::json;

fn usage(total_tokens: f64) -> Usage {
    Usage {
        input: total_tokens,
        total_tokens,
        ..Usage::default()
    }
}

fn user(timestamp: f64, content: &str) -> Message {
    Message::User(UserMessage {
        role: pi_ai::types::UserRole::Tag,
        content: UserContent::Text((content.to_owned()).into()),
        input_source: None,
        provider_native: None,
        timestamp,
    })
}

fn assistant(timestamp: f64, total_tokens: f64) -> Message {
    Message::Assistant(Box::new(AssistantMessage {
        content: vec![AssistantContent::Text(TextContent::new("kept"))],
        usage: usage(total_tokens),
        stop_reason: StopReason::Stop,
        timestamp,
        ..AssistantMessage::empty("openai-responses", "openai", "test-model", timestamp)
    }))
}

#[test]
fn latest_assistant_usage_ignores_timestamp_order() {
    let context = Context {
        system_prompt: Some(("system".to_owned()).into()),
        messages: vec![
            user(200.0, "summary"),
            assistant(100.0, 9_500.0),
            user(300.0, &"x".repeat(4_000)),
        ],
        tools: None,
    };
    let estimate = estimate_context_tokens(&context);
    // Frozen estimate.ts:63–68 selects by array position, not timestamp.
    assert_eq!(estimate.tokens, 10_500.0);
    assert_eq!(estimate.usage_tokens, 9_500.0);
    assert_eq!(estimate.trailing_tokens, 1_000.0);
    assert_eq!(estimate.last_usage_index, Some(1));
}

#[test]
fn uses_assistant_usage_again_after_a_response_to_the_inserted_context() {
    let context = Context {
        system_prompt: None,
        messages: vec![
            user(200.0, "summary"),
            assistant(100.0, 9_500.0),
            user(300.0, "new prompt"),
            assistant(400.0, 2_000.0),
            user(500.0, "tail"),
        ],
        tools: None,
    };
    let estimate = estimate_context_tokens(&context);
    assert_eq!(estimate.tokens, 2_001.0);
    assert_eq!(estimate.usage_tokens, 2_000.0);
    assert_eq!(estimate.trailing_tokens, 1.0);
    assert_eq!(estimate.last_usage_index, Some(3));
}

#[test]
fn calculate_context_tokens_falls_back_to_the_component_sum() {
    let usage = Usage {
        input: 1_000.0,
        output: 500.0,
        cache_read: 200.0,
        cache_write: 100.0,
        total_tokens: 0.0,
        ..Usage::default()
    };
    assert_eq!(calculate_context_tokens(&usage), 1_800.0);
    assert_eq!(calculate_context_tokens(&Usage::default()), 0.0);
    let reported = Usage {
        input: 1.0,
        total_tokens: 5_000.0,
        ..Usage::default()
    };
    assert_eq!(calculate_context_tokens(&reported), 5_000.0);
}

#[test]
fn text_estimation_counts_utf16_code_units() {
    assert_eq!(estimate_text_tokens(""), 0.0);
    assert_eq!(estimate_text_tokens("abcd"), 1.0);
    assert_eq!(estimate_text_tokens("abcde"), 2.0);
    assert_eq!(estimate_text_tokens("😀"), 1.0);
    assert_eq!(estimate_text_tokens("😀😀😀😀😀"), 3.0);
}

#[test]
fn image_content_costs_the_fixed_estimate() {
    let blocks = vec![
        TextOrImageContent::Text(TextContent::new("abcd")),
        TextOrImageContent::Image(pi_ai::types::ImageContent::new("...", "image/png")),
    ];
    assert_eq!(
        estimate_text_and_image_content_tokens(TextOrImageInput::Blocks(&blocks)),
        1_201.0
    );
    assert_eq!(
        estimate_text_and_image_content_tokens(TextOrImageInput::String("abcdefgh")),
        2.0
    );
}

#[test]
fn tool_call_tokens_include_name_and_json_arguments() {
    let message = Message::Assistant(Box::new(AssistantMessage {
        content: vec![AssistantContent::ToolCall(ToolCall::new(
            "call_1",
            "read",
            pi_js_compat::js_json::JsValue::from(serde_json::Value::Object(
                json!({"path": "a.txt"}).as_object().cloned().unwrap(),
            ))
            .as_object()
            .unwrap()
            .clone(),
        ))],
        ..AssistantMessage::empty("anthropic-messages", "anthropic", "test", 1.0)
    }));
    // `read` (4) + `{"path":"a.txt"}` (16) = 20 characters.
    assert_eq!(estimate_message_tokens(&message), 5.0);
}

#[test]
fn context_prefix_adds_system_prompt_and_tool_definitions() {
    let tool = Tool {
        name: "late_tool".to_owned(),
        description: "The late_tool tool".into(),
        parameters: json!({"type": "object"}),
        label: None,
        execution_mode: None,
        typebox_kind: false,
    };
    let context = Context {
        system_prompt: Some(("abcd".to_owned()).into()),
        messages: vec![user(1.0, "abcd")],
        tools: Some(vec![tool.clone()]),
    };
    let estimate = estimate_context_tokens(&context);
    let tool_json = serde_json::to_string(&vec![tool]).expect("tool serializes");
    let expected = 1.0 + 1.0 + (tool_json.encode_utf16().count() as f64 / 4.0).ceil();
    assert_eq!(estimate.tokens, expected);
    assert_eq!(estimate.last_usage_index, None);
}

#[test]
fn tool_definitions_are_ignored_when_assistant_usage_exists() {
    let tool = Tool {
        name: "late_tool".to_owned(),
        description: "late".into(),
        parameters: json!({"type": "object"}),
        label: None,
        execution_mode: None,
        typebox_kind: false,
    };
    let context = Context {
        system_prompt: Some(("ignored after usage".to_owned()).into()),
        messages: vec![
            user(1.0, "hello"),
            assistant(2.0, 100.0),
            Message::ToolResult(ToolResultMessage {
                role: pi_ai::types::ToolResultRole::Tag,
                tool_call_id: "call_1".to_owned(),
                tool_name: "base".to_owned(),
                content: vec![TextOrImageContent::Text(TextContent::new("done"))],
                details: None,
                is_error: false,
                timestamp: 3.0,
            }),
        ],
        tools: Some(vec![tool.clone()]),
    };
    let estimate = estimate_context_tokens(&context);
    // Frozen estimate.ts:96–97 adds only the messages after reported usage.
    assert_eq!(estimate.tokens, 101.0);
    assert_eq!(estimate.usage_tokens, 100.0);
    assert_eq!(estimate.last_usage_index, Some(1));
}
