use pi_ai::api::transform_messages::{transform_messages, transform_messages_with};
use pi_ai::types::{
    AssistantContent, AssistantMessage, ImageContent, Message, Modality, Model, ModelCompat, ModelCost, StopReason,
    TextContent, TextOrImageContent, ThinkingContent, ToolCall, ToolResultMessage, UserContent, UserMessage,
};
use serde_json::json;

const NOW: f64 = 1_700_000_000_000.0;

fn model(provider: &str, api: &str, id: &str, supports_images: bool) -> Model {
    Model {
        properties: Default::default(),
        id: id.to_owned(),
        name: "Test Model".into(),
        api: api.to_owned(),
        provider: provider.to_owned(),
        base_url: ("https://example.invalid".to_owned()).into(),
        reasoning: false,
        thinking_level_map: None,
        input: if supports_images {
            vec![Modality::Text, Modality::Image]
        } else {
            vec![Modality::Text]
        },
        cost: ModelCost::default(),
        context_window: 128_000.0,
        auto_compaction_threshold: None,
        max_tokens: 16_000.0,
        headers: None,
        compat: None::<ModelCompat>,
    }
}

fn anthropic_model() -> Model {
    model("github-copilot", "anthropic-messages", "claude-sonnet-4.6", true)
}

fn anthropic_normalize(id: &str, _model: &Model, _source: &AssistantMessage) -> String {
    let normalized: String = id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' || character == '-' {
                character
            } else {
                '_'
            }
        })
        .collect();
    normalized.chars().take(64).collect()
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

fn assistant(content: Vec<AssistantContent>, provider: &str, api: &str, id: &str) -> Message {
    Message::Assistant(Box::new(AssistantMessage {
        content,
        api: api.to_owned(),
        provider: provider.to_owned(),
        model: id.to_owned(),
        stop_reason: StopReason::ToolUse,
        timestamp: 2.0,
        ..AssistantMessage::empty(api, provider, id, 2.0)
    }))
}

fn tool_result(tool_call_id: &str, tool_name: &str, timestamp: f64) -> Message {
    Message::ToolResult(ToolResultMessage {
        role: pi_ai::types::ToolResultRole::Tag,
        tool_call_id: tool_call_id.to_owned(),
        tool_name: tool_name.to_owned(),
        content: vec![TextOrImageContent::Text(TextContent::new("output"))],
        details: None,
        is_error: false,
        timestamp,
    })
}

fn tool_call(id: &str, name: &str) -> AssistantContent {
    AssistantContent::ToolCall(ToolCall::new(
        id,
        name,
        pi_js_compat::js_json::JsValue::from(serde_json::Value::Object(
            json!({"command": "ls"}).as_object().cloned().unwrap(),
        ))
        .as_object()
        .unwrap()
        .clone(),
    ))
}

fn assistant_content(message: &Message) -> &[AssistantContent] {
    let Message::Assistant(assistant) = message else {
        panic!("expected assistant message")
    };
    &assistant.content
}

#[test]
fn converts_thinking_blocks_to_text_when_the_source_model_differs() {
    let messages = vec![
        user(1.0, "hello"),
        assistant(
            vec![
                AssistantContent::Thinking(ThinkingContent {
                    thinking_signature: Some("reasoning_content".to_owned()),
                    ..ThinkingContent::new("Let me think about this...")
                }),
                AssistantContent::Text(TextContent::new("Hi there!")),
            ],
            "github-copilot",
            "openai-completions",
            "gpt-4o",
        ),
    ];
    let result = transform_messages(&messages, &anthropic_model(), NOW);
    let content = assistant_content(&result[1]);
    assert!(
        !content
            .iter()
            .any(|block| matches!(block, AssistantContent::Thinking(_)))
    );
    let texts: Vec<&pi_js_compat::js_json::JsString> = content
        .iter()
        .filter_map(|block| match block {
            AssistantContent::Text(text) => Some(&text.text),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["Let me think about this...", "Hi there!"]);
}

#[test]
fn removes_thought_signatures_when_migrating_between_models() {
    let mut call = ToolCall::new(
        "call_123",
        "bash",
        pi_js_compat::js_json::JsValue::from(serde_json::Value::Object(json!({}).as_object().cloned().unwrap()))
            .as_object()
            .unwrap()
            .clone(),
    );
    call.thought_signature = Some("encrypted".to_owned());
    let messages = vec![
        user(1.0, "run a command"),
        assistant(
            vec![AssistantContent::ToolCall(call)],
            "github-copilot",
            "openai-responses",
            "gpt-5",
        ),
        tool_result("call_123", "bash", 3.0),
    ];
    let result = transform_messages(&messages, &anthropic_model(), NOW);
    let AssistantContent::ToolCall(call) = &assistant_content(&result[1])[0] else {
        panic!("expected tool call")
    };
    assert_eq!(call.thought_signature, None);
}

#[test]
fn normalizes_invalid_tool_ids_even_when_replaying_the_same_model() {
    let messages = vec![
        user(1.0, "read the file"),
        assistant(
            vec![tool_call("call_123|fc_123", "read")],
            "github-copilot",
            "anthropic-messages",
            "claude-sonnet-4.6",
        ),
        tool_result("call_123|fc_123", "read", 3.0),
    ];
    let result = transform_messages_with(&messages, &anthropic_model(), Some(&anthropic_normalize), NOW);
    let AssistantContent::ToolCall(call) = &assistant_content(&result[1])[0] else {
        panic!("expected tool call")
    };
    assert_eq!(call.id, "call_123_fc_123");
    let Message::ToolResult(result) = &result[2] else {
        panic!("expected tool result")
    };
    assert_eq!(result.tool_call_id, "call_123_fc_123");
}

#[test]
fn adds_synthetic_tool_results_for_trailing_orphaned_tool_calls() {
    let messages = vec![
        user(1.0, "read the file"),
        assistant(
            vec![tool_call("call_123|fc_123", "read")],
            "github-copilot",
            "openai-responses",
            "gpt-5",
        ),
    ];
    let result = transform_messages_with(&messages, &anthropic_model(), Some(&anthropic_normalize), NOW);
    let Message::ToolResult(last) = result.last().expect("result") else {
        panic!("expected synthetic tool result")
    };
    assert_eq!(last.tool_call_id, "call_123_fc_123");
    assert_eq!(last.tool_name, "read");
    assert!(last.is_error);
    assert_eq!(
        last.content,
        vec![TextOrImageContent::Text(TextContent::new("No result provided"))]
    );
    assert_eq!(last.timestamp, NOW);
}

#[test]
fn adds_synthetic_results_only_for_trailing_calls_still_missing_results() {
    let messages = vec![
        user(1.0, "run commands"),
        assistant(
            vec![tool_call("call_1|fc_1", "read"), tool_call("call_2|fc_2", "bash")],
            "github-copilot",
            "openai-responses",
            "gpt-5",
        ),
        tool_result("call_1|fc_1", "read", 3.0),
    ];
    let result = transform_messages_with(&messages, &anthropic_model(), Some(&anthropic_normalize), NOW);
    let synthetic: Vec<&ToolResultMessage> = result
        .iter()
        .filter_map(|message| match message {
            Message::ToolResult(result) if result.is_error => Some(result),
            _ => None,
        })
        .collect();
    assert_eq!(synthetic.len(), 1);
    assert_eq!(synthetic[0].tool_call_id, "call_2_fc_2");
    assert_eq!(synthetic[0].tool_name, "bash");
}

#[test]
fn downgrades_images_for_text_only_models_and_collapses_consecutive_images() {
    let image = TextOrImageContent::Image(ImageContent::new("AAAA", "image/png"));
    let messages = vec![
        Message::User(UserMessage {
            role: pi_ai::types::UserRole::Tag,
            content: UserContent::Blocks(vec![
                TextOrImageContent::Text(TextContent::new("look")),
                image.clone(),
                image.clone(),
                TextOrImageContent::Text(TextContent::new("done")),
            ]),
            input_source: None,
            provider_native: None,
            timestamp: 1.0,
        }),
        Message::ToolResult(ToolResultMessage {
            role: pi_ai::types::ToolResultRole::Tag,
            tool_call_id: "call_1".to_owned(),
            tool_name: "read".to_owned(),
            content: vec![image],
            details: None,
            is_error: false,
            timestamp: 2.0,
        }),
    ];
    let result = transform_messages(
        &messages,
        &model("openai", "openai-completions", "text-only", false),
        NOW,
    );
    let Message::User(user) = &result[0] else {
        panic!("user")
    };
    let UserContent::Blocks(blocks) = &user.content else {
        panic!("blocks")
    };
    assert_eq!(
        blocks,
        &vec![
            TextOrImageContent::Text(TextContent::new("look")),
            TextOrImageContent::Text(TextContent::new("(image omitted: model does not support images)")),
            TextOrImageContent::Text(TextContent::new("done")),
        ]
    );
    let Message::ToolResult(result) = &result[1] else {
        panic!("tool result")
    };
    assert_eq!(
        result.content,
        vec![TextOrImageContent::Text(TextContent::new(
            "(tool image omitted: model does not support images)"
        ))]
    );
}

#[test]
fn keeps_images_for_vision_models() {
    let messages = vec![Message::User(UserMessage {
        role: pi_ai::types::UserRole::Tag,
        content: UserContent::Blocks(vec![TextOrImageContent::Image(ImageContent::new("AAAA", "image/png"))]),
        input_source: None,
        provider_native: None,
        timestamp: 1.0,
    })];
    let result = transform_messages(&messages, &anthropic_model(), NOW);
    assert_eq!(result, messages);
}

#[test]
fn keeps_redacted_thinking_only_for_the_same_model_and_drops_empty_thinking() {
    let redacted = AssistantContent::Thinking(ThinkingContent {
        redacted: Some(true),
        thinking_signature: Some("opaque".to_owned()),
        ..ThinkingContent::new("")
    });
    let same_model = vec![assistant(
        vec![redacted.clone()],
        "github-copilot",
        "anthropic-messages",
        "claude-sonnet-4.6",
    )];
    assert_eq!(
        assistant_content(&transform_messages(&same_model, &anthropic_model(), NOW)[0]).len(),
        1
    );
    assert!(
        assistant_content(
            &transform_messages(&same_model, &model("openai", "openai-responses", "other", true), NOW)[0]
        )
        .is_empty()
    );

    let empty_with_signature = AssistantContent::Thinking(ThinkingContent {
        thinking_signature: Some("sig".to_owned()),
        ..ThinkingContent::new("")
    });
    let signed = vec![assistant(
        vec![empty_with_signature],
        "github-copilot",
        "anthropic-messages",
        "claude-sonnet-4.6",
    )];
    assert_eq!(
        assistant_content(&transform_messages(&signed, &anthropic_model(), NOW)[0]).len(),
        1
    );

    let unsigned = vec![assistant(
        vec![AssistantContent::Thinking(ThinkingContent::new("  "))],
        "github-copilot",
        "anthropic-messages",
        "claude-sonnet-4.6",
    )];
    assert!(assistant_content(&transform_messages(&unsigned, &anthropic_model(), NOW)[0]).is_empty());
}

#[test]
fn skips_errored_and_aborted_assistant_messages_after_flushing_pending_calls() {
    let mut errored = AssistantMessage::empty("openai-responses", "github-copilot", "gpt-5", 3.0);
    errored.stop_reason = StopReason::Error;
    let messages = vec![
        user(1.0, "run"),
        assistant(
            vec![tool_call("call_1", "bash")],
            "github-copilot",
            "openai-responses",
            "gpt-5",
        ),
        Message::Assistant(Box::new(errored)),
        user(4.0, "next"),
    ];
    let result = transform_messages(&messages, &anthropic_model(), NOW);
    assert_eq!(result.len(), 4);
    assert!(matches!(&result[0], Message::User(_)));
    assert!(matches!(&result[1], Message::Assistant(_)));
    assert!(matches!(&result[2], Message::ToolResult(result) if result.is_error && result.tool_call_id == "call_1"));
    assert!(matches!(&result[3], Message::User(_)));
}

#[test]
fn user_messages_interrupt_tool_flow_with_synthetic_results() {
    let messages = vec![
        assistant(
            vec![tool_call("call_1", "bash")],
            "github-copilot",
            "openai-responses",
            "gpt-5",
        ),
        user(3.0, "never mind"),
    ];
    let result = transform_messages(&messages, &anthropic_model(), NOW);
    assert!(matches!(&result[0], Message::Assistant(_)));
    assert!(matches!(&result[1], Message::ToolResult(result) if result.tool_call_id == "call_1"));
    assert!(matches!(&result[2], Message::User(_)));
}
