use crate::shared_oracle as oracle;

use pi_coding_agent::modes::json_event::to_json_print_mode_event;
use pi_js_compat::js_json::JsValue;
use serde_json::{Value, json};

fn partial() -> Value {
    json!({
        "role": "assistant",
        "content": [{"type": "text", "text": "Hi"}],
        "api": "anthropic-messages",
        "provider": "anthropic",
        "model": "claude-sonnet-4-5",
        "usage": {
            "input": 1,
            "output": 2,
            "cacheRead": 0,
            "cacheWrite": 0,
            "totalTokens": 3,
            "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}
        },
        "stopReason": "stop",
        "timestamp": 123
    })
}

fn events() -> Vec<Value> {
    vec![
        json!({"type": "agent_start"}),
        json!({"type": "agent_end", "messages": [], "willRetry": false}),
        json!({"type": "queue_update", "steering": ["a"], "followUp": ["b"]}),
        json!({"type": "steering_message_queued"}),
        json!({"type": "session_info_changed", "name": "x"}),
        json!({"type": "session_info_changed", "name": null}),
        json!({"type": "thinking_level_changed", "level": "high"}),
        json!({"type": "auto_retry_start", "attempt": 1, "maxAttempts": 3, "delayMs": 100, "errorMessage": "boom"}),
        json!({"type": "auto_retry_end", "success": false, "attempt": 3, "finalError": "boom"}),
        json!({"type": "message_start", "message": partial()}),
        json!({"type": "message_end", "message": partial()}),
        json!({
            "type": "message_update",
            "message": partial(),
            "assistantMessageEvent": {"type": "start", "partial": partial()}
        }),
        json!({
            "type": "message_update",
            "message": partial(),
            "assistantMessageEvent": {
                "type": "text_start",
                "contentIndex": 0,
                "partial": partial()
            }
        }),
        json!({
            "type": "message_update",
            "message": partial(),
            "assistantMessageEvent": {
                "type": "text_delta",
                "contentIndex": 0,
                "delta": "Hi",
                "partial": partial()
            }
        }),
        json!({
            "type": "message_update",
            "message": partial(),
            "assistantMessageEvent": {
                "type": "text_end",
                "contentIndex": 0,
                "content": "Hi",
                "partial": partial()
            }
        }),
        json!({
            "type": "message_update",
            "message": partial(),
            "assistantMessageEvent": {
                "type": "thinking_delta",
                "contentIndex": 1,
                "delta": "hm",
                "partial": partial()
            }
        }),
        json!({
            "type": "message_update",
            "message": partial(),
            "assistantMessageEvent": {
                "type": "toolcall_delta",
                "contentIndex": 2,
                "delta": "{}",
                "partial": partial()
            }
        }),
        json!({
            "type": "message_update",
            "message": partial(),
            "assistantMessageEvent": {
                "type": "toolcall_end",
                "contentIndex": 2,
                "toolCall": {"type": "toolCall", "id": "t1", "name": "read", "arguments": {}},
                "partial": partial()
            }
        }),
        json!({
            "type": "message_update",
            "message": partial(),
            "assistantMessageEvent": {"type": "done", "reason": "stop", "message": partial()}
        }),
        json!({
            "type": "message_update",
            "message": partial(),
            "assistantMessageEvent": {
                "type": "error",
                "reason": "error",
                "error": partial()
            }
        }),
    ]
}

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn(
        "coding-agent-json-event-oracle.ts",
        "packages/coding-agent/src/modes/json-event.ts",
    );
    let mut fixtures: Vec<JsValue> = events().into_iter().map(JsValue::from).collect();
    for encoded in [
        r#"{"before":1,"type":"message_start","message":{"role":"user","content":"\ud800","details":{"\ud800":"\udc00"},"summary":"body","output":"body","providerNative":"body","imageGenerationResult":"body","keep":"\udc00"},"after":2}"#,
        r#"{"type":"message_update","keep":"\ud800","message":{"content":"body"},"assistantMessageEvent":{"type":"text_delta","partial":{},"content":"body","toolCall":{},"message":{},"error":{},"delta":"\udc00","keep":{"\ud800":"\udc00"}}}"#,
        r#"{"type":"turn_end","message":{"content":"body"},"toolResults":[],"turnIndex":0.25,"keep":"\ud800"}"#,
        r#"{"type":"agent_end","messages":[{"content":"body"}],"willRetry":false,"keep":"\udc00"}"#,
        r#"{"type":"tool_execution_end","toolCallId":"fixture","result":{"content":[{"text":"\ud800"}],"details":{"\ud800":"\udc00"},"terminate":true,"keep":"\udc00"},"keep":"\ud800"}"#,
        r#"{"type":"message_end","message":{"role":"user","content":"\ud800","details":{"\ud800":"\udc00"}},"keep":"\udc00"}"#,
    ] {
        fixtures.push(pi_js_compat::js_json::parse_lossless(encoded).unwrap());
    }
    let response = oracle.request(&pi_js_compat::js_json!({"id": 1, "events": fixtures}));
    assert_eq!(response["ok"], JsValue::Bool(true), "oracle failed: {response:?}");

    let expected = response["output"]["events"]
        .as_array()
        .expect("oracle events must be an array");
    assert_eq!(expected.len(), fixtures.len());

    for (index, event) in fixtures.iter().enumerate() {
        let actual = to_json_print_mode_event(event);
        assert_eq!(
            pi_js_compat::js_json::stringify_lossless(&actual),
            pi_js_compat::js_json::stringify_lossless(&expected[index]),
            "event {index} differs"
        );
    }
}
