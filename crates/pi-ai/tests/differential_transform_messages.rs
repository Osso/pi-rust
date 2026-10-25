use crate::shared_oracle as oracle;

use pi_ai::api::transform_messages::{transform_messages, transform_messages_with};
use pi_ai::types::{AssistantMessage, Message, Model};
use serde_json::{Value, json};

const NOW: i64 = 1_700_000_000_000;

fn model(provider: &str, api: &str, id: &str, supports_images: bool) -> Value {
    json!({
        "id": id, "name": "Test Model", "api": api, "provider": provider,
        "baseUrl": "https://example.invalid", "reasoning": false,
        "input": if supports_images { json!(["text", "image"]) } else { json!(["text"]) },
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 128000, "maxTokens": 16000
    })
}

fn usage() -> Value {
    json!({"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
           "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}})
}

fn assistant(content: Value, provider: &str, api: &str, id: &str, stop_reason: &str, timestamp: i64) -> Value {
    json!({"role": "assistant", "content": content, "api": api, "provider": provider, "model": id,
           "usage": usage(), "stopReason": stop_reason, "timestamp": timestamp})
}

fn user(content: impl Into<Value>, timestamp: i64) -> Value {
    let content: Value = content.into();
    json!({"role": "user", "content": content, "timestamp": timestamp})
}

fn tool_result(id: &str, name: &str, content: Value, is_error: bool, timestamp: i64) -> Value {
    json!({"role": "toolResult", "toolCallId": id, "toolName": name, "content": content,
           "isError": is_error, "timestamp": timestamp})
}

fn text(value: &str) -> Value {
    json!({"type": "text", "text": value})
}

fn image() -> Value {
    json!({"type": "image", "data": "AAAA", "mimeType": "image/png"})
}

fn tool_call(id: &str, name: &str) -> Value {
    json!({"type": "toolCall", "id": id, "name": name, "arguments": {"command": "ls"}})
}

fn scenarios() -> Vec<(Value, Value, &'static str)> {
    let anthropic = model("github-copilot", "anthropic-messages", "claude-sonnet-4.6", true);
    let vision_other = model("openai", "openai-responses", "other", true);
    let text_only = model("openai", "openai-completions", "text-only", false);
    let mut scenarios = Vec::new();

    scenarios.push((
        json!([
            user("hello", 1),
            assistant(json!([
                {"type": "thinking", "thinking": "Let me think about this...", "thinkingSignature": "reasoning_content"},
                text("Hi there!")
            ]), "github-copilot", "openai-completions", "gpt-4o", "stop", 2)
        ]),
        anthropic.clone(),
        "anthropic",
    ));
    scenarios.push((
        json!([
            user("run a command", 1),
            assistant(json!([{"type": "toolCall", "id": "call_123", "name": "bash", "arguments": {}, "thoughtSignature": "encrypted"}]),
                "github-copilot", "openai-responses", "gpt-5", "toolUse", 2),
            tool_result("call_123", "bash", json!([text("output")]), false, 3)
        ]),
        anthropic.clone(),
        "anthropic",
    ));
    scenarios.push((
        json!([
            user("read the file", 1),
            assistant(
                json!([tool_call("call_123|fc_123", "read")]),
                "github-copilot",
                "anthropic-messages",
                "claude-sonnet-4.6",
                "toolUse",
                2
            ),
            tool_result("call_123|fc_123", "read", json!([text("done")]), false, 3)
        ]),
        anthropic.clone(),
        "anthropic",
    ));
    scenarios.push((
        json!([
            user("read the file", 1),
            assistant(
                json!([tool_call("call_123|fc_123", "read")]),
                "github-copilot",
                "openai-responses",
                "gpt-5",
                "toolUse",
                2
            )
        ]),
        anthropic.clone(),
        "anthropic",
    ));
    scenarios.push((
        json!([
            user("run commands", 1),
            assistant(
                json!([tool_call("call_1|fc_1", "read"), tool_call("call_2|fc_2", "bash")]),
                "github-copilot",
                "openai-responses",
                "gpt-5",
                "toolUse",
                2
            ),
            tool_result("call_1|fc_1", "read", json!([text("done")]), false, 3)
        ]),
        anthropic.clone(),
        "anthropic",
    ));
    scenarios.push((
        json!([
            user(json!([text("look"), image(), image(), text("done")]), 1),
            tool_result("call_1", "read", json!([image()]), false, 2)
        ]),
        text_only.clone(),
        "none",
    ));
    scenarios.push((
        json!([user(json!([text("look"), image()]), 1)]),
        anthropic.clone(),
        "none",
    ));
    scenarios.push((
        json!([
            user("hello", 1),
            assistant(
                json!([{"type": "thinking", "thinking": "", "thinkingSignature": "opaque", "redacted": true}]),
                "github-copilot",
                "anthropic-messages",
                "claude-sonnet-4.6",
                "stop",
                2
            )
        ]),
        anthropic.clone(),
        "none",
    ));
    scenarios.push((
        json!([
            user("hello", 1),
            assistant(
                json!([{"type": "thinking", "thinking": "", "thinkingSignature": "opaque", "redacted": true}]),
                "github-copilot",
                "anthropic-messages",
                "claude-sonnet-4.6",
                "stop",
                2
            )
        ]),
        vision_other.clone(),
        "none",
    ));
    scenarios.push((
        json!([
            user("hello", 1),
            assistant(
                json!([{"type": "thinking", "thinking": "", "thinkingSignature": "sig"},
                             {"type": "thinking", "thinking": "   "},
                             {"type": "thinking", "thinking": "plan", "thinkingSignature": "sig2"}]),
                "github-copilot",
                "anthropic-messages",
                "claude-sonnet-4.6",
                "stop",
                2
            )
        ]),
        anthropic.clone(),
        "none",
    ));
    scenarios.push((
        json!([
            user("hello", 1),
            assistant(
                json!([{"type": "thinking", "thinking": "plan", "thinkingSignature": "sig"}]),
                "github-copilot",
                "anthropic-messages",
                "claude-sonnet-4.6",
                "stop",
                2
            )
        ]),
        vision_other.clone(),
        "none",
    ));
    scenarios.push((
        json!([
            user("run", 1),
            assistant(
                json!([tool_call("call_1", "bash")]),
                "github-copilot",
                "openai-responses",
                "gpt-5",
                "toolUse",
                2
            ),
            assistant(json!([]), "github-copilot", "openai-responses", "gpt-5", "error", 3),
            user("next", 4)
        ]),
        anthropic.clone(),
        "none",
    ));
    scenarios.push((
        json!([
            user("run", 1),
            assistant(
                json!([tool_call("call_1", "bash")]),
                "github-copilot",
                "openai-responses",
                "gpt-5",
                "toolUse",
                2
            ),
            assistant(json!([]), "github-copilot", "openai-responses", "gpt-5", "aborted", 3),
            user("next", 4)
        ]),
        anthropic.clone(),
        "none",
    ));
    scenarios.push((
        json!([
            assistant(
                json!([tool_call("call_1", "bash")]),
                "github-copilot",
                "openai-responses",
                "gpt-5",
                "toolUse",
                2
            ),
            user("never mind", 3)
        ]),
        anthropic.clone(),
        "none",
    ));
    scenarios.push((
        json!([
            user("run", 1),
            assistant(
                json!([{"type": "toolCall", "id": "call_1", "name": "bash", "arguments": {}, "thoughtSignature": ""}]),
                "github-copilot",
                "openai-responses",
                "gpt-5",
                "toolUse",
                2
            )
        ]),
        anthropic.clone(),
        "anthropic",
    ));
    scenarios.push((
        json!([
            user("run", 1),
            assistant(
                json!([tool_call("call_1", "bash")]),
                "github-copilot",
                "openai-responses",
                "gpt-5",
                "toolUse",
                2
            ),
            tool_result("unknown", "bash", json!([text("stray")]), false, 3)
        ]),
        anthropic.clone(),
        "none",
    ));
    scenarios.push((
        json!([
            user("hello", 1),
            assistant(
                json!([text("plain")]),
                "github-copilot",
                "openai-completions",
                "gpt-4o",
                "stop",
                2
            )
        ]),
        anthropic,
        "none",
    ));
    let signed_text = json!([{"type": "text", "text": "signed", "textSignature": "{\"v\":1,\"id\":\"msg_1\"}"}]);
    scenarios.push((
        json!([
            user("hello", 1),
            assistant(
                signed_text.clone(),
                "github-copilot",
                "anthropic-messages",
                "claude-sonnet-4.6",
                "stop",
                2
            )
        ]),
        vision_other.clone(),
        "none",
    ));
    scenarios.push((
        json!([
            user("hello", 1),
            assistant(
                signed_text,
                "github-copilot",
                "anthropic-messages",
                "claude-sonnet-4.6",
                "stop",
                2
            )
        ]),
        model("github-copilot", "anthropic-messages", "claude-sonnet-4.6", true),
        "none",
    ));
    scenarios
}

fn observe(request: &Value) -> Value {
    let messages: Vec<Message> = serde_json::from_value(request["messages"].clone()).expect("messages");
    let model: Model = serde_json::from_value(request["model"].clone()).expect("model");
    let now = request["now"].as_f64().expect("now");
    let transformed = if request["normalize"] == "anthropic" {
        transform_messages_with(&messages, &model, Some(&anthropic_normalize), now)
    } else {
        transform_messages(&messages, &model, now)
    };
    json!({"id": request["id"], "ok": true, "output": {"messages": transformed}})
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

#[test]
fn message_transformation_matches_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn(
        "transform-messages-oracle.ts",
        "packages/ai/src/api/transform-messages.ts",
    );
    for (id, (messages, model, normalize)) in scenarios().into_iter().enumerate() {
        let request = json!({"id": id, "messages": messages, "model": model, "normalize": normalize, "now": NOW});
        assert_eq!(observe(&request), oracle.request(&request), "scenario {id}");
    }
}
