use crate::shared_oracle as oracle;

use pi_ai::types::Context;
use pi_ai::utils::estimate::{estimate_context_tokens, estimate_messages_tokens};
use serde_json::{Value, json};

fn usage(total: u32) -> Value {
    json!({"input": total, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": total,
           "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}})
}

fn assistant(timestamp: i64, total: u32, stop_reason: &str, content: Value) -> Value {
    json!({"role": "assistant", "content": content, "api": "openai-responses", "provider": "openai",
           "model": "test-model", "usage": usage(total), "stopReason": stop_reason, "timestamp": timestamp})
}

fn tool(name: &str, description: &str) -> Value {
    json!({"name": name, "description": description, "parameters": {"type": "object"}})
}

fn requests() -> Vec<Value> {
    vec![
        json!({"id": 0, "context": {
            "systemPrompt": "system",
            "messages": [
                {"role": "user", "content": "summary", "timestamp": 200},
                assistant(100, 9_500, "stop", json!([{"type": "text", "text": "kept"}])),
                {"role": "user", "content": "x".repeat(4_000), "timestamp": 300}
            ]
        }}),
        json!({"id": 1, "context": {"messages": [
            {"role": "user", "content": "summary", "timestamp": 200},
            assistant(100, 9_500, "stop", json!([{"type": "text", "text": "kept"}])),
            {"role": "user", "content": "new prompt", "timestamp": 300},
            assistant(400, 2_000, "stop", json!([{"type": "text", "text": "kept"}])),
            {"role": "user", "content": "tail", "timestamp": 500}
        ]}}),
        json!({"id": 2, "context": {
            "systemPrompt": "abcd",
            "messages": [{"role": "user", "content": "abcd", "timestamp": 1}],
            "tools": [tool("late_tool", "The late_tool tool")]
        }}),
        json!({"id": 3, "context": {
            "systemPrompt": "ignored after usage",
            "messages": [
                {"role": "user", "content": "hello", "timestamp": 1},
                assistant(2, 100, "stop", json!([])),
                {"role": "toolResult", "toolCallId": "call_1", "toolName": "base",
                 "content": [{"type": "text", "text": "done"}], "addedToolNames": ["late_tool"],
                 "isError": false, "timestamp": 3}
            ],
            "tools": [tool("late_tool", "late"), tool("other_tool", "other")]
        }}),
        json!({"id": 4, "messages": [
            {"role": "user", "content": [{"type": "text", "text": "😀😀"}, {"type": "image", "data": "AA", "mimeType": "image/png"}], "timestamp": 1},
            {"role": "toolResult", "toolCallId": "call_1", "toolName": "read",
             "content": [{"type": "text", "text": "abcdefgh"}, {"type": "image", "data": "AA", "mimeType": "image/png"}],
             "isError": false, "timestamp": 2},
            assistant(3, 0, "toolUse", json!([
                {"type": "thinking", "thinking": "plan"},
                {"type": "text", "text": "hello"},
                {"type": "toolCall", "id": "call_2", "name": "read", "arguments": {"path": "a.txt", "limit": 10}}
            ]))
        ]}),
        json!({"id": 5, "messages": [
            {"role": "user", "content": "summary", "timestamp": 200},
            assistant(100, 9_500, "aborted", json!([{"type": "text", "text": "kept"}])),
            {"role": "user", "content": "tail", "timestamp": 300}
        ]}),
        json!({"id": 6, "messages": [
            {"role": "user", "content": "summary", "timestamp": 200},
            assistant(100, 9_500, "error", json!([{"type": "text", "text": "kept"}])),
            {"role": "user", "content": "tail", "timestamp": 300}
        ]}),
        json!({"id": 7, "context": {"systemPrompt": "", "messages": [], "tools": []}}),
        json!({"id": 8, "messages": []}),
        json!({"id": 10, "context": {"messages": [], "tools": [{
            "name": "counter", "description": "Count",
            "parameters": {"type": "object", "properties": {"limit": {
                "type": "number", "minimum": 1.0, "maximum": 5.0,
                "default": 2.0, "multipleOf": 1.0
            }}}
        }]}}),
        json!({"id": 11, "context": {"messages": [], "tools": [{
            "name": "ratio", "description": "Scale",
            "parameters": {"type": "object", "properties": {"threshold": {
                "type": "number", "minimum": 1e-6, "maximum": 1e6, "default": 0.000001
            }}}
        }]}}),
        json!({"id": 9, "messages": [
            {"role": "user", "content": "a".repeat(4_001), "timestamp": 1}
        ]}),
    ]
}

fn observe(request: &Value) -> Value {
    let estimate = if let Some(messages) = request.get("messages") {
        let messages: Vec<pi_ai::types::Message> = serde_json::from_value(messages.clone()).expect("messages");
        estimate_messages_tokens(&messages)
    } else {
        let context: Context = serde_json::from_value(request["context"].clone()).expect("context");
        estimate_context_tokens(&context)
    };
    json!({"id": request["id"], "ok": true, "output": estimate})
}

#[test]
fn context_estimation_matches_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn("estimate-oracle.ts", "packages/ai/src/utils/estimate.ts");
    for request in requests() {
        assert_eq!(observe(&request), oracle.request(&request), "request {request}");
    }
}
