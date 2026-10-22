use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, Context, DoneReason, ErrorReason, Message, Model,
    ModelCompat, StopReason, ToolResultContent, Usage, UserContent,
};
use serde_json::{Value, json};

fn round_trip<T>(wire: &Value) -> T
where
    T: serde::de::DeserializeOwned + serde::Serialize,
{
    let parsed: T = serde_json::from_value(wire.clone()).expect("wire JSON must parse");
    let reserialized = serde_json::to_value(&parsed).expect("value must serialize");
    assert_eq!(&reserialized, wire);
    parsed
}

fn assistant_wire() -> Value {
    json!({
        "role": "assistant",
        "content": [
            {"type": "thinking", "thinking": "plan", "thinkingSignature": "rs_1", "redacted": false},
            {"type": "text", "text": "hello", "textSignature": "{\"v\":1,\"id\":\"msg_1\"}"},
            {"type": "toolCall", "id": "call_1", "name": "read", "arguments": {"path": "a.txt", "limit": 10},
             "execution": {"agentId": "agent_7"}, "thoughtSignature": "sig", "namespace": "ns"}
        ],
        "api": "anthropic-messages",
        "provider": "anthropic",
        "model": "claude-opus-5-5",
        "responseId": "msg_01",
        "usage": {
            "input": 12, "output": 34, "cacheRead": 0, "cacheWrite": 5, "cacheWrite1h": 5,
            "reasoning": 7, "totalTokens": 51,
            "cost": {"input": 0.25, "output": 1.5, "cacheRead": 0, "cacheWrite": 0.125, "total": 1.875}
        },
        "stopReason": "toolUse",
        "endTurn": false,
        "timestamp": 1_760_000_000_123_i64
    })
}

#[test]
fn empty_stream_message_starts_with_frozen_stop_reason() {
    let message = AssistantMessage::empty("anthropic-messages", "anthropic", "test", 0);
    assert_eq!(message.stop_reason, StopReason::Stop);
}

#[test]
fn assistant_message_round_trips_exact_wire_shape() {
    let message: Message = round_trip(&assistant_wire());
    let Message::Assistant(assistant) = message else {
        panic!("role assistant must parse as Message::Assistant");
    };
    assert_eq!(assistant.stop_reason, StopReason::ToolUse);
    assert_eq!(assistant.usage.total_tokens, 51.0);
    let AssistantContent::ToolCall(call) = &assistant.content[2] else {
        panic!("third block must be a tool call");
    };
    assert_eq!(call.arguments["limit"], pi_js_compat::js_json::JsValue::Number(10.0));
}

fn round_trip_stream_block(kind: &str, block: Value) {
    let mut partial = assistant_wire();
    partial["content"] = json!([block]);
    let event = json!({"type": kind, "contentIndex": 0, "partial": partial});
    round_trip::<AssistantMessageEvent>(&event);
}

#[test]
fn valid_stream_scratch_text_index_round_trips() {
    for index in [json!(3), json!(-1.5), json!(1e100)] {
        round_trip_stream_block("text_start", json!({"type": "text", "text": "", "index": index}));
    }
}

#[test]
fn valid_stream_scratch_thinking_index_round_trips() {
    round_trip_stream_block(
        "thinking_start",
        json!({"type": "thinking", "thinking": "plan", "thinkingSignature": "sig", "index": 7}),
    );
}

#[test]
fn valid_stream_scratch_tool_buffer_round_trips() {
    round_trip_stream_block(
        "toolcall_start",
        json!({"type": "toolCall", "id": "call_1", "name": "read", "arguments": {},
                                   "index": 11, "partialJson": "{\"path\":"}),
    );
}

#[test]
fn integral_costs_serialize_like_javascript_numbers() {
    let usage = Usage::default();
    let wire = serde_json::to_string(&usage).expect("usage must serialize");
    assert_eq!(
        wire,
        r#"{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}}"#
    );
}

#[test]
fn user_message_accepts_string_and_block_content() {
    let text: Message = round_trip(&json!({"role": "user", "content": "hi", "timestamp": 1}));
    let Message::User(user) = text else {
        panic!("must be user")
    };
    assert_eq!(user.content, UserContent::Text("hi".into()));

    let blocks: Message = round_trip(&json!({
        "role": "user",
        "content": [{"type": "text", "text": "look"}, {"type": "image", "data": "AAAA", "mimeType": "image/png"}],
        "inputSource": "rpc",
        "providerNative": {"provider": "openai", "api": "openai-responses", "format": "openai.responses.input", "value": [1, 2]},
        "timestamp": 2
    }));
    let Message::User(user) = blocks else {
        panic!("must be user")
    };
    assert!(matches!(user.content, UserContent::Blocks(ref b) if b.len() == 2));
}

#[test]
fn tool_result_message_round_trips_with_arbitrary_details() {
    let message: Message = round_trip(&json!({
        "role": "toolResult",
        "toolCallId": "call_1",
        "toolName": "bash",
        "content": [{"type": "text", "text": "ok"}],
        "details": {"exitCode": 0, "truncated": null},
        "addedToolNames": ["grep"],
        "isError": false,
        "timestamp": 3
    }));
    let Message::ToolResult(result) = message else {
        panic!("must be toolResult")
    };
    assert!(matches!(&result.content[0], ToolResultContent::Text(t) if t.text == "ok"));
}

#[test]
fn unknown_role_is_rejected() {
    let parsed = serde_json::from_value::<Message>(json!({"role": "system", "content": "x", "timestamp": 1}));
    assert!(parsed.is_err());
}

#[test]
fn stream_events_round_trip_with_snake_case_tags() {
    let partial = assistant_wire();
    let events = [
        json!({"type": "start", "partial": partial}),
        json!({"type": "text_delta", "contentIndex": 1, "delta": "he", "partial": partial}),
        json!({"type": "toolcall_end", "contentIndex": 2, "toolCall": partial["content"][2], "partial": partial}),
        json!({"type": "done", "reason": "toolUse", "message": partial}),
        json!({"type": "error", "reason": "aborted", "error": partial}),
    ];
    for wire in &events {
        let _: AssistantMessageEvent = round_trip(wire);
    }
    let done: AssistantMessageEvent = serde_json::from_value(events[3].clone()).expect("done parses");
    assert!(matches!(
        done,
        AssistantMessageEvent::Done {
            reason: DoneReason::ToolUse,
            ..
        }
    ));
    let error: AssistantMessageEvent = serde_json::from_value(events[4].clone()).expect("error parses");
    assert!(matches!(
        error,
        AssistantMessageEvent::Error {
            reason: ErrorReason::Aborted,
            ..
        }
    ));
}

#[test]
fn done_event_rejects_error_reasons() {
    let wire = json!({"type": "done", "reason": "error", "message": assistant_wire()});
    assert!(serde_json::from_value::<AssistantMessageEvent>(wire).is_err());
}

#[test]
fn model_compat_is_typed_by_api() {
    let completions: Model = round_trip(&json!({
        "id": "deepseek/deepseek-v4.1-flash-1017",
        "name": "DeepSeek V4.1 Flash",
        "api": "openai-completions",
        "provider": "openrouter",
        "baseUrl": "https://openrouter.ai/api/v1",
        "reasoning": true,
        "thinkingLevelMap": {"off": null, "high": "high"},
        "input": ["text", "image"],
        "cost": {"input": 0.1, "output": 0.4, "cacheRead": 0.01, "cacheWrite": 0,
                 "tiers": [{"input": 0.2, "output": 0.8, "cacheRead": 0.02, "cacheWrite": 0, "inputTokensAbove": 200000}]},
        "contextWindow": 1_000_000,
        "maxTokens": 131_072,
        "compat": {"supportsStore": false, "maxTokensField": "max_tokens", "thinkingFormat": "openrouter",
                   "openRouterRouting": {"only": ["deepseek"], "sort": {"by": "price", "partition": null}}}
    }));
    assert!(
        matches!(completions.compat, Some(ModelCompat::OpenAICompletions(ref c)) if c.supports_store == Some(false))
    );

    let anthropic: Model = round_trip(&json!({
        "id": "claude-opus-5-5", "name": "Claude Opus 5.5", "api": "anthropic-messages", "provider": "anthropic",
        "baseUrl": "https://api.anthropic.com", "reasoning": true, "input": ["text"],
        "cost": {"input": 5, "output": 25, "cacheRead": 0.5, "cacheWrite": 6.25},
        "contextWindow": 1_000_000, "maxTokens": 128_000,
        "compat": {"forceAdaptiveThinking": true, "supportsTemperature": false}
    }));
    assert!(
        matches!(anthropic.compat, Some(ModelCompat::AnthropicMessages(ref c)) if c.force_adaptive_thinking == Some(true))
    );
}

#[test]
fn custom_api_model_parses_without_compat() {
    let model: Model = round_trip(&json!({
        "id": "local", "name": "Local", "api": "my-custom-api", "provider": "ollama", "baseUrl": "http://localhost",
        "reasoning": false, "input": ["text"], "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 8192, "maxTokens": 4096
    }));
    assert_eq!(model.api, "my-custom-api");
}

#[test]
fn context_round_trips_tools_with_json_schema_parameters() {
    let context: Context = round_trip(&json!({
        "systemPrompt": "be brief",
        "messages": [{"role": "user", "content": "hi", "timestamp": 1}],
        "tools": [{
            "name": "read", "description": "Read a file",
            "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]},
            "constrainedSampling": {"type": "json_schema", "strict": "prefer"}
        }, {
            "name": "noop", "description": "", "parameters": {"type": "object"}, "constrainedSampling": false
        }]
    }));
    assert_eq!(context.tools.as_ref().map(Vec::len), Some(2));
}

#[test]
fn assistant_message_defaults_omit_optional_fields() {
    let message = AssistantMessage::empty("openai-responses", "openai", "gpt-6", 42);
    let wire = serde_json::to_value(&message).expect("serializes");
    assert_eq!(
        wire,
        json!({
            "role": "assistant", "content": [], "api": "openai-responses", "provider": "openai", "model": "gpt-6",
            "usage": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
                      "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}},
            "stopReason": "stop", "timestamp": 42
        })
    );
}
