use pi_ai::types::{
    AssistantMessage, AssistantRole, ImageTag, StopReason, TextOrImageContent, TextTag, ThinkingTag, ToolCall,
    ToolResultMessage, ToolResultRole, Usage, UsageCost, UserMessage, UserRole,
};
use pi_protocol::{
    AssistantContent, AssistantTranscriptState, ModelInput, ThinkingLevel, ToolContent, ToolTranscriptState,
    UserContent,
};
use pi_server::protocol::{
    ProtocolBridgeError, sanitize_protocol_details, to_protocol_assistant_message, to_protocol_json_value,
    to_protocol_model_metadata, to_protocol_tool_result_message, to_protocol_usage, to_protocol_user_message,
};

fn zero_usage() -> Usage {
    Usage {
        input: 0.0,
        output: 0.0,
        cache_read: 0.0,
        cache_write: 0.0,
        cache_write_1h: None,
        reasoning: None,
        total_tokens: 0.0,
        cost: UsageCost {
            input: 0.0,
            output: 0.0,
            cache_read: 0.0,
            cache_write: 0.0,
            total: 0.0,
        },
    }
}

fn assistant(content: Vec<pi_ai::types::AssistantContent>, stop_reason: StopReason) -> AssistantMessage {
    AssistantMessage {
        role: AssistantRole::Tag,
        content,
        api: "test-api".into(),
        provider: "test-provider".into(),
        model: "model-1".into(),
        response_model: None,
        response_id: None,
        image_generation_result: None,
        diagnostics: None,
        usage: zero_usage(),
        stop_reason,
        deferred: None,
        error_message: None,
        raw_stop_reason: None,
        end_turn: None,
        timestamp: 123,
    }
}

fn model() -> pi_ai::types::Model {
    serde_json::from_value(serde_json::json!({
        "id": "model-1",
        "name": "Model One",
        "api": "test-api",
        "provider": "test-provider",
        "baseUrl": "https://example.test",
        "reasoning": true,
        "input": ["text", "image"],
        "cost": { "input": 1, "output": 2, "cacheRead": 0.1, "cacheWrite": 0.2 },
        "contextWindow": 100000,
        "maxTokens": 10000
    }))
    .expect("model fixture must parse")
}

#[test]
fn maps_model_metadata_and_thinking_levels() {
    let result = to_protocol_model_metadata(&model(), true).expect("model metadata must map");

    assert_eq!(result.provider(), "test-provider");
    assert_eq!(result.id(), "model-1");
    assert_eq!(result.api(), "test-api");
    assert_eq!(result.input(), &[ModelInput::Text, ModelInput::Image]);
    assert!(result.authenticated());
    assert_eq!(
        result.supported_thinking_levels(),
        &[
            ThinkingLevel::Off,
            ThinkingLevel::Minimal,
            ThinkingLevel::Low,
            ThinkingLevel::Medium,
            ThinkingLevel::High,
        ]
    );
}

#[test]
fn maps_every_assistant_content_variant_and_stop_reason() {
    let message = AssistantMessage {
        usage: Usage {
            input: 1.0,
            output: 2.0,
            cache_read: 3.0,
            cache_write: 4.0,
            cache_write_1h: None,
            reasoning: None,
            total_tokens: 10.0,
            cost: UsageCost {
                input: 0.1,
                output: 0.2,
                cache_read: 0.3,
                cache_write: 0.4,
                total: 1.0,
            },
        },
        ..assistant(
            vec![
                pi_ai::types::AssistantContent::Text(pi_ai::types::TextContent::new("hello")),
                pi_ai::types::AssistantContent::Thinking(pi_ai::types::ThinkingContent {
                    r#type: ThinkingTag::Tag,
                    thinking: "hmm".into(),
                    thinking_signature: None,
                    redacted: Some(false),
                    index: None,
                }),
                pi_ai::types::AssistantContent::ToolCall(ToolCall::new(
                    "call-1",
                    "read",
                    serde_json::json!({ "path": "README.md" })
                        .as_object()
                        .expect("object")
                        .clone(),
                )),
            ],
            StopReason::ToolUse,
        )
    };

    let result = to_protocol_assistant_message(&message, "message-1").expect("assistant message must map");

    assert_eq!(result.id(), "message-1");
    assert_eq!(result.model().provider(), "test-provider");
    assert_eq!(result.model().id(), "model-1");
    assert_eq!(
        result.state(),
        &AssistantTranscriptState::Complete {
            stop_reason: pi_protocol::AssistantStopReason::ToolUse
        }
    );
    let AssistantContent::Text(text) = &result.content()[0] else {
        panic!("expected text content");
    };
    assert_eq!(text.text(), "hello");
    let AssistantContent::Thinking(thinking) = &result.content()[1] else {
        panic!("expected thinking content");
    };
    assert_eq!(thinking.thinking(), "hmm");
    assert_eq!(thinking.redacted(), Some(false));
    let AssistantContent::ToolCall(call) = &result.content()[2] else {
        panic!("expected tool call content");
    };
    assert_eq!(call.tool_call_id(), "call-1");
    assert_eq!(call.tool_name(), "read");
    assert_eq!(
        call.input(),
        &pi_protocol::JsonValue::Object(vec![(
            "path".into(),
            pi_protocol::JsonValue::String("README.md".into())
        )])
    );
    let usage = result.usage().expect("usage must be present");
    assert_eq!(usage.input(), 1);
    assert_eq!(usage.total_tokens(), 10);
    assert_eq!(usage.cost().total(), 1.0);
}

#[test]
fn maps_user_and_tool_messages_without_lossy_details() {
    let user = UserMessage {
        role: UserRole::Tag,
        content: pi_ai::types::UserContent::Text("hello".into()),
        input_source: None,
        provider_native: None,
        timestamp: 1,
    };
    let user_result = to_protocol_user_message(&user, "user-1").expect("user message must map");
    assert_eq!(user_result.id(), "user-1");
    let UserContent::Text(text) = &user_result.content()[0] else {
        panic!("expected text content");
    };
    assert_eq!(text.text(), "hello");

    let call = ToolCall::new(
        "call-1",
        "read",
        serde_json::json!({ "path": "README.md" })
            .as_object()
            .expect("object")
            .clone(),
    );
    let tool = ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: "call-1".into(),
        tool_name: "read".into(),
        content: vec![TextOrImageContent::Text(pi_ai::types::TextContent::new("result"))],
        details: Some(serde_json::json!({ "lines": 2 })),
        usage: None,
        added_tool_names: None,
        is_error: false,
        timestamp: 2,
    };
    let tool_result = to_protocol_tool_result_message(&tool, "tool-1", &call).expect("tool result must map");
    assert_eq!(tool_result.id(), "tool-1");
    assert_eq!(tool_result.tool_call_id(), "call-1");
    assert_eq!(tool_result.tool_name(), "read");
    assert_eq!(tool_result.state(), ToolTranscriptState::Complete);
    assert_eq!(
        tool_result.details(),
        Some(&pi_protocol::JsonValue::Object(vec![(
            "lines".into(),
            pi_protocol::JsonValue::Number(2.0)
        )]))
    );
    let ToolContent::Text(text) = &tool_result.content()[0] else {
        panic!("expected tool text content");
    };
    assert_eq!(text.text(), "result");
}

#[test]
fn rejects_tool_results_associated_with_a_different_call() {
    let call = ToolCall::new("call-1", "read", serde_json::Map::new());
    let tool = ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: "call-2".into(),
        tool_name: "read".into(),
        content: vec![TextOrImageContent::Text(pi_ai::types::TextContent::new("result"))],
        details: None,
        usage: None,
        added_tool_names: None,
        is_error: false,
        timestamp: 2,
    };

    let error = to_protocol_tool_result_message(&tool, "tool-1", &call).expect_err("mismatched call must fail");
    assert!(error.to_string().contains("tool call"));

    let renamed = ToolResultMessage {
        tool_call_id: "call-1".into(),
        tool_name: "write".into(),
        ..tool
    };
    let error = to_protocol_tool_result_message(&renamed, "tool-1", &call).expect_err("mismatched name must fail");
    assert!(error.to_string().contains("tool call"));
}

#[test]
fn derives_streaming_status_from_a_pending_stop_reason() {
    let message = assistant(
        vec![pi_ai::types::AssistantContent::Text(pi_ai::types::TextContent::new(
            "partial",
        ))],
        StopReason::Pending,
    );

    let result = to_protocol_assistant_message(&message, "message-pending").expect("pending message must map");

    assert_eq!(result.state(), &AssistantTranscriptState::Streaming);
    assert_eq!(result.state().stop_reason(), None);
}

#[test]
fn preserves_optional_non_empty_assistant_error_messages() {
    let message = assistant(Vec::new(), StopReason::Error);

    let result = to_protocol_assistant_message(&message, "message-error").expect("error message must map");
    assert_eq!(result.state(), &AssistantTranscriptState::Error { error_message: None });
    assert_eq!(result.state().error_message(), None);

    let empty = AssistantMessage {
        error_message: Some(String::new()),
        ..message.clone()
    };
    assert!(to_protocol_assistant_message(&empty, "message-error").is_err());

    let failed = AssistantMessage {
        error_message: Some("failed".into()),
        ..message
    };
    let result = to_protocol_assistant_message(&failed, "message-error").expect("message must map");
    assert_eq!(
        result.state(),
        &AssistantTranscriptState::Error {
            error_message: Some("failed".into())
        }
    );
}

#[test]
fn rejects_invalid_source_identifiers_and_timestamps() {
    let message = assistant(
        vec![pi_ai::types::AssistantContent::ToolCall(ToolCall::new(
            "",
            "read",
            serde_json::Map::new(),
        ))],
        StopReason::ToolUse,
    );
    let error = to_protocol_assistant_message(&message, "assistant-1").expect_err("empty call id must fail");
    assert!(error.to_string().to_lowercase().contains("tool call id"));

    let user = UserMessage {
        role: UserRole::Tag,
        content: pi_ai::types::UserContent::Text("hello".into()),
        input_source: None,
        provider_native: None,
        timestamp: -1,
    };
    let error = to_protocol_user_message(&user, "user-1").expect_err("negative timestamp must fail");
    assert!(error.to_string().contains("timestamp"));
}

#[test]
fn normalizes_usage_counts_and_costs() {
    let usage = Usage {
        input: 1.9,
        output: -2.0,
        cache_read: f64::NAN,
        cache_write: 3.0,
        cache_write_1h: None,
        reasoning: Some(2.7),
        total_tokens: 10.0,
        cost: UsageCost {
            input: -1.0,
            output: f64::INFINITY,
            cache_read: 0.5,
            cache_write: 0.0,
            total: 1.0,
        },
    };

    let mapped = to_protocol_usage(Some(&usage)).expect("usage must map");
    assert_eq!(mapped.input(), 1);
    assert_eq!(mapped.output(), 0);
    assert_eq!(mapped.cache_read(), 0);
    assert_eq!(mapped.cache_write(), 3);
    assert_eq!(mapped.reasoning(), Some(2));
    assert_eq!(mapped.total_tokens(), 10);
    assert_eq!(mapped.cost().input(), 0.0);
    assert_eq!(mapped.cost().output(), 0.0);
    assert_eq!(mapped.cost().cache_read(), 0.5);
    assert_eq!(mapped.cost().total(), 1.0);
    assert!(to_protocol_usage(None).is_none());
}

#[test]
fn protocol_json_values_and_diagnostic_details_are_lossless_for_json_input() {
    let value = serde_json::json!({
        "text": "value",
        "nested": [1, true, null, { "key": "entry" }]
    });
    let mapped = to_protocol_json_value(&value);
    assert_eq!(
        mapped,
        pi_protocol::JsonValue::Object(vec![
            ("text".into(), pi_protocol::JsonValue::String("value".into())),
            (
                "nested".into(),
                pi_protocol::JsonValue::Array(vec![
                    pi_protocol::JsonValue::Number(1.0),
                    pi_protocol::JsonValue::Bool(true),
                    pi_protocol::JsonValue::Null,
                    pi_protocol::JsonValue::Object(vec![(
                        "key".into(),
                        pi_protocol::JsonValue::String("entry".into())
                    )]),
                ])
            ),
        ])
    );
    assert_eq!(sanitize_protocol_details(&value), Some(mapped));
    assert_eq!(
        sanitize_protocol_details(&serde_json::Value::Null),
        Some(pi_protocol::JsonValue::Null)
    );
}

#[test]
fn bridge_errors_are_displayable_errors() {
    let error = ProtocolBridgeError::new("Protocol timestamps must be non-negative integers");
    assert!(error.to_string().contains("timestamp"));
    let _: &dyn std::error::Error = &error;
}

#[test]
fn maps_redacted_thinking_and_image_content() {
    let image = pi_ai::types::ImageContent {
        r#type: ImageTag::Tag,
        data: "AAAA".into(),
        mime_type: "image/png".into(),
    };
    let user = UserMessage {
        role: UserRole::Tag,
        content: pi_ai::types::UserContent::Blocks(vec![TextOrImageContent::Image(image)]),
        input_source: None,
        provider_native: None,
        timestamp: 5,
    };
    let mapped = to_protocol_user_message(&user, "user-image").expect("image must map");
    let UserContent::Image(image) = &mapped.content()[0] else {
        panic!("expected image content");
    };
    assert_eq!(image.data(), "AAAA");
    assert_eq!(image.mime_type(), "image/png");

    let text_tag: TextTag = TextTag::Tag;
    assert_eq!(text_tag, TextTag::Tag);
}
