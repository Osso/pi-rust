use crate::shared_oracle as oracle;

use pi_ai::api::github_copilot_headers::{
    build_copilot_dynamic_headers, has_copilot_vision_input, infer_copilot_initiator,
};
use pi_ai::types::Message;
use serde_json::{Value, json};

fn user_text(text: &str) -> Value {
    json!({"role": "user", "content": text, "timestamp": 1})
}

fn user_blocks(blocks: Value) -> Value {
    json!({"role": "user", "content": blocks, "timestamp": 1})
}

fn assistant() -> Value {
    json!({
        "role": "assistant", "content": [], "api": "openai-completions", "provider": "github-copilot",
        "model": "gpt-5",
        "usage": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
                  "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}},
        "stopReason": "stop", "timestamp": 1
    })
}

fn tool_result(blocks: Value) -> Value {
    json!({"role": "toolResult", "toolCallId": "call_1", "toolName": "read", "content": blocks,
           "isError": false, "timestamp": 1})
}

fn image() -> Value {
    json!({"type": "image", "data": "AAAA", "mimeType": "image/png"})
}

fn requests() -> Vec<Value> {
    let messages = vec![
        json!([]),
        json!([user_text("hi")]),
        json!([user_text("hi"), assistant()]),
        json!([tool_result(json!([{"type": "text", "text": "ok"}]))]),
        json!([user_blocks(json!([image()]))]),
        json!([tool_result(json!([{"type": "text", "text": "ok"}, image()]))]),
        json!([
            user_blocks(json!([{"type": "text", "text": "look"}, image()])),
            assistant(),
        ]),
        json!([user_text("hi"), tool_result(json!([image()]))]),
    ];
    let mut requests = Vec::new();
    for message_list in messages {
        for has_images in [false, true] {
            requests.push(json!({"id": requests.len(), "messages": message_list, "hasImages": has_images}));
        }
    }
    requests
}

fn observe(request: &Value) -> Value {
    let messages: Vec<Message> = serde_json::from_value(request["messages"].clone()).expect("messages");
    let has_images = request["hasImages"].as_bool().expect("hasImages");
    json!({
        "id": request["id"],
        "ok": true,
        "output": {
            "initiator": infer_copilot_initiator(&messages),
            "hasImages": has_copilot_vision_input(&messages),
            "headers": build_copilot_dynamic_headers(&messages, has_images)
        }
    })
}

#[test]
fn copilot_headers_match_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn(
        "github-copilot-headers-oracle.ts",
        "packages/ai/src/api/github-copilot-headers.ts",
    );
    for request in requests() {
        assert_eq!(observe(&request), oracle.request(&request), "request {request}");
    }
}
