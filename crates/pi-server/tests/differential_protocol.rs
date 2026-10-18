#[path = "support/oracle.rs"]
mod oracle;

use pi_protocol::{
    EventEnvelope, JsonValue, PROTOCOL_VERSION, ServerEvent, ServerHello, ServerMessage, ServerSnapshot,
    SessionSnapshot, SessionSnapshotInput, TranscriptItem, encode_server_message,
};
use pi_server::protocol::{
    sanitize_protocol_details, to_protocol_assistant_message, to_protocol_model_metadata,
    to_protocol_tool_result_message, to_protocol_user_message,
};
use serde_json::{Value, json};

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).expect("hex digits"))
        .collect()
}

fn canonical_value(value: &pi_protocol::CborValue) -> pi_protocol::CborValue {
    use pi_protocol::CborValue;
    match value {
        CborValue::Array(values) => CborValue::Array(values.iter().map(canonical_value).collect()),
        CborValue::Map(entries) => {
            let mut entries: Vec<(String, CborValue)> = entries
                .iter()
                .map(|(key, value)| (key.clone(), canonical_value(value)))
                .collect();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            CborValue::Map(entries)
        }
        value => value.clone(),
    }
}

/// Canonicalizes CBOR map order so the comparison covers bridge values rather than serializer key order.
fn canonical_frame(frame_hex: &str) -> String {
    let frame = decode_hex(frame_hex);
    let payload = &frame[4..];
    let value = pi_protocol::decode_cbor(payload, None).expect("oracle frame must decode");
    hex(&pi_protocol::encode_cbor(&canonical_value(&value), None).expect("canonical frame must encode"))
}

fn snapshot_frame(item: TranscriptItem) -> String {
    let snapshot = SessionSnapshot::new(SessionSnapshotInput {
        id: "session-1".into(),
        name: None,
        cwd: "/workspace".into(),
        created_at: 1,
        updated_at: 1,
        phase: pi_protocol::SessionPhase::Idle,
        model: pi_protocol::ModelRef::new("test-provider".into(), "model-1".into()).expect("model ref"),
        thinking_level: pi_protocol::ThinkingLevel::Off,
        attached: true,
        locked: true,
        revision: 1,
        transcript: vec![item],
        queued_steer: Vec::new(),
        queued_steer_count: 0,
    })
    .expect("snapshot must build");
    let message = ServerMessage::Event(EventEnvelope::new(ServerEvent::SessionSnapshot(snapshot)));
    hex(&encode_server_message(&message, None).expect("frame must encode"))
}

fn model_metadata_frame(model: &pi_ai::types::Model, authenticated: bool) -> String {
    let metadata = to_protocol_model_metadata(model, authenticated).expect("model metadata must map");
    let snapshot = ServerSnapshot::new("server-1".into(), 0, Vec::new(), vec![metadata]).expect("snapshot");
    let message = ServerMessage::Hello(ServerHello::new("connection-1".into(), snapshot).expect("hello"));
    hex(&encode_server_message(&message, None).expect("frame must encode"))
}

fn model_fixture() -> Value {
    json!({
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
    })
}

fn zero_usage() -> Value {
    json!({
        "input": 0,
        "output": 0,
        "cacheRead": 0,
        "cacheWrite": 0,
        "totalTokens": 0,
        "cost": { "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0 }
    })
}

fn assistant_fixture(content: Value, stop_reason: &str) -> Value {
    json!({
        "role": "assistant",
        "content": content,
        "api": "test-api",
        "provider": "test-provider",
        "model": "model-1",
        "usage": zero_usage(),
        "stopReason": stop_reason,
        "timestamp": 123
    })
}

fn compare_ok(ts_oracle: &mut oracle::TsOracle, request: Value, rust_frame: String) {
    let response = ts_oracle.request(&request);
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    assert_eq!(
        canonical_frame(response["output"]["frameHex"].as_str().expect("frame hex")),
        canonical_frame(&rust_frame),
        "request {request}"
    );
}

fn compare_error(ts_oracle: &mut oracle::TsOracle, request: Value, rust_error: &str) {
    let response = ts_oracle.request(&request);
    assert_eq!(response["ok"], false, "expected oracle failure: {response}");
    assert_eq!(
        response["error"]["message"].as_str().expect("error message"),
        rust_error,
        "request {request}"
    );
}

#[test]
fn protocol_bridge_matches_the_typescript_source() {
    let mut ts_oracle = oracle::TsOracle::spawn("server-protocol-oracle.ts", "packages/server/src/protocol.ts");

    let model_json = model_fixture();
    let model: pi_ai::types::Model = serde_json::from_value(model_json.clone()).expect("model fixture");
    compare_ok(
        &mut ts_oracle,
        json!({ "id": 1, "operation": "model_metadata", "model": model_json, "authenticated": true }),
        model_metadata_frame(&model, true),
    );

    let assistant_json = json!({
        "role": "assistant",
        "content": [
            { "type": "text", "text": "hello" },
            { "type": "thinking", "thinking": "hmm", "redacted": false },
            { "type": "toolCall", "id": "call-1", "name": "read", "arguments": { "path": "README.md" } }
        ],
        "api": "test-api",
        "provider": "test-provider",
        "model": "model-1",
        "usage": {
            "input": 1,
            "output": 2,
            "cacheRead": 3,
            "cacheWrite": 4,
            "totalTokens": 10,
            "cost": { "input": 0.1, "output": 0.2, "cacheRead": 0.3, "cacheWrite": 0.4, "total": 1 }
        },
        "stopReason": "toolUse",
        "timestamp": 123
    });
    let message: pi_ai::types::AssistantMessage =
        serde_json::from_value(assistant_json.clone()).expect("assistant fixture");
    let item = to_protocol_assistant_message(&message, "message-1").expect("assistant must map");
    compare_ok(
        &mut ts_oracle,
        json!({ "id": 2, "operation": "assistant_message", "itemId": "message-1", "message": assistant_json }),
        snapshot_frame(TranscriptItem::Assistant(item)),
    );

    let user_json = json!({ "role": "user", "content": "hello", "timestamp": 1 });
    let user: pi_ai::types::UserMessage = serde_json::from_value(user_json.clone()).expect("user fixture");
    let item = to_protocol_user_message(&user, "user-1").expect("user must map");
    compare_ok(
        &mut ts_oracle,
        json!({ "id": 3, "operation": "user_message", "itemId": "user-1", "message": user_json }),
        snapshot_frame(TranscriptItem::User(item)),
    );

    let image_user_json = json!({
        "role": "user",
        "content": [{ "type": "image", "data": "AAAA", "mimeType": "image/png" }],
        "timestamp": 5
    });
    let user: pi_ai::types::UserMessage = serde_json::from_value(image_user_json.clone()).expect("image user fixture");
    let item = to_protocol_user_message(&user, "user-image").expect("user image must map");
    compare_ok(
        &mut ts_oracle,
        json!({ "id": 4, "operation": "user_message", "itemId": "user-image", "message": image_user_json }),
        snapshot_frame(TranscriptItem::User(item)),
    );

    let call_json = json!({
        "type": "toolCall",
        "id": "call-1",
        "name": "read",
        "arguments": { "path": "README.md" }
    });
    let tool_json = json!({
        "role": "toolResult",
        "toolCallId": "call-1",
        "toolName": "read",
        "content": [{ "type": "text", "text": "result" }],
        "details": { "lines": 2 },
        "isError": false,
        "timestamp": 2
    });
    let call: pi_ai::types::ToolCall = serde_json::from_value(call_json.clone()).expect("call fixture");
    let tool: pi_ai::types::ToolResultMessage = serde_json::from_value(tool_json.clone()).expect("tool fixture");
    let item = to_protocol_tool_result_message(&tool, "tool-1", &call).expect("tool must map");
    compare_ok(
        &mut ts_oracle,
        json!({ "id": 5, "operation": "tool_result", "itemId": "tool-1", "call": call_json, "message": tool_json }),
        snapshot_frame(TranscriptItem::Tool(item)),
    );

    let usage_json = json!({
        "input": 1.9,
        "output": -2,
        "cacheRead": 0,
        "cacheWrite": 3,
        "reasoning": 2.7,
        "totalTokens": 10,
        "cost": { "input": -1, "output": 0, "cacheRead": 0.5, "cacheWrite": 0, "total": 1 }
    });
    let usage: pi_ai::types::Usage = serde_json::from_value(usage_json.clone()).expect("usage fixture");
    let assistant_json = json!({
        "role": "assistant",
        "content": [],
        "api": "test-api",
        "provider": "test-provider",
        "model": "model-1",
        "usage": usage_json,
        "stopReason": "stop",
        "timestamp": 123
    });
    let message: pi_ai::types::AssistantMessage =
        serde_json::from_value(assistant_json.clone()).expect("assistant usage fixture");
    let item = to_protocol_assistant_message(&message, "assistant-1").expect("assistant usage must map");
    assert_eq!(item.usage().expect("usage").input(), 1);
    assert_eq!(usage.output, -2.0);
    compare_ok(
        &mut ts_oracle,
        json!({ "id": 6, "operation": "assistant_message", "itemId": "assistant-1", "message": assistant_json }),
        snapshot_frame(TranscriptItem::Assistant(item)),
    );

    let details = json!({ "text": "value", "nested": [1, true, null, { "key": "entry" }] });
    let mapped: JsonValue = sanitize_protocol_details(&details).expect("details must map");
    let item = pi_protocol::ToolTranscriptItem::new(pi_protocol::ToolTranscriptItemInput {
        id: "tool-1".into(),
        tool_call_id: "call-1".into(),
        tool_name: "read".into(),
        input: JsonValue::Object(Vec::new()),
        content: Vec::new(),
        details: Some(mapped),
        usage: None,
        timestamp: 1,
        state: pi_protocol::ToolTranscriptState::Complete,
    })
    .expect("tool item");
    compare_ok(
        &mut ts_oracle,
        json!({ "id": 7, "operation": "sanitize_details", "value": details }),
        snapshot_frame(TranscriptItem::Tool(item)),
    );
}

#[test]
fn protocol_bridge_errors_match_the_typescript_source() {
    let mut ts_oracle = oracle::TsOracle::spawn("server-protocol-oracle.ts", "packages/server/src/protocol.ts");

    let call_json = json!({ "type": "toolCall", "id": "call-1", "name": "read", "arguments": {} });
    let mismatched_call = json!({
        "role": "toolResult",
        "toolCallId": "call-2",
        "toolName": "read",
        "content": [],
        "isError": false,
        "timestamp": 2
    });
    let call: pi_ai::types::ToolCall = serde_json::from_value(call_json.clone()).expect("call fixture");
    let tool: pi_ai::types::ToolResultMessage = serde_json::from_value(mismatched_call.clone()).expect("tool fixture");
    let error = to_protocol_tool_result_message(&tool, "tool-1", &call).expect_err("mismatch must fail");
    compare_error(
        &mut ts_oracle,
        json!({ "id": 8, "operation": "tool_result", "itemId": "tool-1", "call": call_json, "message": mismatched_call }),
        &error.to_string(),
    );

    let empty_call = assistant_fixture(
        json!([{ "type": "toolCall", "id": "", "name": "read", "arguments": {} }]),
        "toolUse",
    );
    let message: pi_ai::types::AssistantMessage =
        serde_json::from_value(empty_call.clone()).expect("empty call fixture");
    let error = to_protocol_assistant_message(&message, "assistant-1").expect_err("empty call must fail");
    compare_error(
        &mut ts_oracle,
        json!({ "id": 9, "operation": "assistant_message", "itemId": "assistant-1", "message": empty_call }),
        &error.to_string(),
    );

    let negative_timestamp = json!({ "role": "user", "content": "hello", "timestamp": -1 });
    let user: pi_ai::types::UserMessage =
        serde_json::from_value(negative_timestamp.clone()).expect("negative timestamp fixture");
    let error = to_protocol_user_message(&user, "user-1").expect_err("negative timestamp must fail");
    compare_error(
        &mut ts_oracle,
        json!({ "id": 10, "operation": "user_message", "itemId": "user-1", "message": negative_timestamp }),
        &error.to_string(),
    );

    let deferred = assistant_fixture(json!([]), "deferred");
    let message: pi_ai::types::AssistantMessage = serde_json::from_value(deferred.clone()).expect("deferred fixture");
    let error = to_protocol_assistant_message(&message, "assistant-1").expect_err("deferred must fail");
    compare_error(
        &mut ts_oracle,
        json!({ "id": 11, "operation": "assistant_message", "itemId": "assistant-1", "message": deferred }),
        &error.to_string(),
    );

    let mut empty_error = assistant_fixture(json!([]), "error");
    empty_error["errorMessage"] = json!("");
    let message: pi_ai::types::AssistantMessage =
        serde_json::from_value(empty_error.clone()).expect("empty error fixture");
    let error = to_protocol_assistant_message(&message, "assistant-1").expect_err("empty error must fail");
    compare_error(
        &mut ts_oracle,
        json!({ "id": 12, "operation": "assistant_message", "itemId": "assistant-1", "message": empty_error }),
        &error.to_string(),
    );
}

#[test]
fn protocol_version_matches_the_pinned_protocol_crate() {
    assert_eq!(PROTOCOL_VERSION, 1);
}
