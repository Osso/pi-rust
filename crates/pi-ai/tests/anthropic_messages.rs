#[path = "support/anthropic.rs"]
mod support;

use std::sync::Arc;

use pi_ai::api::anthropic_messages::stream;
use pi_ai::types::{AssistantContent, Model, StopReason};
use pi_js_compat::abort::AbortSignal;
use serde_json::{Value, json};
use support::*;

fn model(provider: &str, id: &str, compat: Option<Value>) -> Value {
    let mut value = json!({
        "id": id,
        "name": id,
        "api": "anthropic-messages",
        "provider": provider,
        "baseUrl": "https://api.anthropic.com",
        "reasoning": true,
        "input": ["text", "image"],
        "cost": {"input": 5.0, "output": 25.0, "cacheRead": 0.5, "cacheWrite": 6.25},
        "contextWindow": 200000,
        "maxTokens": 4096,
        "headers": sdk_environment_headers(),
    });
    if let Some(compat) = compat {
        value["compat"] = compat;
    }
    value
}

fn context(messages: Value, system: Option<&str>, tools: Option<Value>) -> Value {
    let mut value = json!({"messages": messages});
    if let Some(system) = system {
        value["systemPrompt"] = Value::String(system.to_owned());
    }
    if let Some(tools) = tools {
        value["tools"] = tools;
    }
    value
}

fn user(content: Value) -> Value {
    json!({"role": "user", "content": content, "timestamp": 1})
}

fn event(name: &str, data: Value) -> (String, String) {
    (name.to_owned(), serde_json::to_string(&data).expect("event data"))
}

fn sse(events: &[(String, String)]) -> String {
    events
        .iter()
        .map(|(name, data)| format!("event: {name}\ndata: {data}\n"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn minimal_events(delta_usage: Option<Value>) -> Vec<(String, String)> {
    let mut message_delta = json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}});
    if let Some(usage) = delta_usage {
        message_delta["usage"] = usage;
    }
    vec![
        event(
            "message_start",
            json!({"type": "message_start", "message": {"id": "msg_test", "usage": {
                "input_tokens": 12, "output_tokens": 0,
                "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0}}}),
        ),
        event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": ""}}),
        ),
        event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": "Hello"}}),
        ),
        event("content_block_stop", json!({"type": "content_block_stop", "index": 0})),
        event("message_delta", message_delta),
        event("message_stop", json!({"type": "message_stop"})),
    ]
}

fn sse_response(body: String) -> ScriptedResponse {
    ScriptedResponse::sse(&[body.as_str()])
}

fn status_response(status: u16, headers: &[(&str, &str)], body: &str) -> ScriptedResponse {
    let mut response = ScriptedResponse::json(status, body);
    for (name, value) in headers {
        if !name.eq_ignore_ascii_case("content-type") {
            response = response.with_header(name, value);
        }
    }
    response
}

async fn run_stream(
    model: Value,
    context: Value,
    options: Value,
    script: Vec<ScriptedResponse>,
    signal: Option<AbortSignal>,
) -> (Vec<Value>, Value, Vec<Value>) {
    run_stream_with_fetch(model, context, options, RecordingFetch::new(script), signal).await
}

async fn run_stream_with_fetch(
    model: Value,
    context: Value,
    options: Value,
    fetch: Arc<RecordingFetch>,
    signal: Option<AbortSignal>,
) -> (Vec<Value>, Value, Vec<Value>) {
    let options = stream_options(&options, fetch.clone(), signal);
    let model: Model = serde_json::from_value(model).expect("model");
    let context = serde_json::from_value(context).expect("context");
    let (events, result) = collect(stream(model, context, options)).await;
    (
        events_json(&events),
        message_json(&result),
        requests_json(&fetch.requests()),
    )
}

fn api_key() -> Value {
    json!({"apiKey": "test-key"})
}

#[tokio::test]
async fn frozen_source_discards_initial_text_thinking_and_signature() {
    let events = vec![
        event(
            "message_start",
            json!({"type": "message_start", "message": {"id": "msg_initial_content", "usage": {
                "input_tokens": 12, "output_tokens": 0,
                "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0}}}),
        ),
        event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 0, "content_block": {"type": "text", "text": "Initial text"}}),
        ),
        event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "text_delta", "text": " plus delta"}}),
        ),
        event("content_block_stop", json!({"type": "content_block_stop", "index": 0})),
        event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 1, "content_block": {
                "type": "thinking", "thinking": "Initial thinking", "signature": "initial signature"}}),
        ),
        event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 1, "delta": {"type": "thinking_delta", "thinking": " plus delta"}}),
        ),
        event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 1, "delta": {"type": "signature_delta", "signature": " plus delta"}}),
        ),
        event("content_block_stop", json!({"type": "content_block_stop", "index": 1})),
        event(
            "message_delta",
            json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {
                "input_tokens": 12, "output_tokens": 5,
                "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0}}),
        ),
        event("message_stop", json!({"type": "message_stop"})),
    ];
    let (_events, result, _requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("Say hello."))]), None, None),
        api_key(),
        vec![sse_response(sse(&events))],
        None,
    )
    .await;

    assert_eq!(
        result["content"],
        json!([
            {"type": "text", "text": " plus delta"},
            {"type": "thinking", "thinking": " plus delta",
             "thinkingSignature": " plus delta"}
        ])
    );
    assert_eq!(result["stopReason"], json!("stop"));
}

#[tokio::test]
async fn repairs_malformed_sse_json_and_streamed_tool_json() {
    let malformed = r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\"A\H\",\"text\":\"col1	col2\"}"}}"#.to_owned();
    let events = vec![
        event(
            "message_start",
            json!({"type": "message_start", "message": {"id": "msg_test", "usage": {
                "input_tokens": 12, "output_tokens": 0,
                "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0}}}),
        ),
        event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 0, "content_block": {
                "type": "tool_use", "id": "toolu_test", "name": "edit", "input": {}}}),
        ),
        ("content_block_delta".to_owned(), malformed),
        event("content_block_stop", json!({"type": "content_block_stop", "index": 0})),
        event(
            "message_delta",
            json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {
                "input_tokens": 12, "output_tokens": 5,
                "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0}}),
        ),
        event("message_stop", json!({"type": "message_stop"})),
    ];
    let (_events, result, _requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(
            json!([user(json!("Use the edit tool."))]),
            None,
            Some(json!([{"name": "edit", "description": "Edit a file.",
                          "parameters": {"type": "object", "properties": {"path": {"type": "string"}, "text": {"type": "string"}}}}])),
        ),
        api_key(),
        vec![sse_response(sse(&events))],
        None,
    )
    .await;

    assert_eq!(result["stopReason"], json!("toolUse"));
    assert_eq!(
        result["content"],
        json!([{"type": "toolCall", "id": "toolu_test", "name": "edit",
                "arguments": {"path": "A\\H", "text": "col1\tcol2"}}])
    );
}

#[tokio::test]
async fn preserves_refusal_and_sensitive_stop_reasons() {
    let refusal = "This request triggered restrictions.";
    let refusal_events = vec![
        event(
            "message_start",
            json!({"type": "message_start", "message": {"id": "msg_refusal", "usage": {"input_tokens": 412, "output_tokens": 0}}}),
        ),
        event(
            "message_delta",
            json!({"type": "message_delta", "delta": {
                "stop_reason": "refusal",
                "stop_details": {"type": "refusal", "category": "cyber", "explanation": refusal}},"usage": {"input_tokens": 412, "output_tokens": 0}}),
        ),
        event("message_stop", json!({"type": "message_stop"})),
    ];
    let (_events, result, _requests) = run_stream(
        model("anthropic", "claude-fable-5", None),
        context(json!([user(json!("blocked"))]), None, None),
        api_key(),
        vec![sse_response(sse(&refusal_events))],
        None,
    )
    .await;
    assert_eq!(result["stopReason"], json!("error"));
    assert!(result.get("rawStopReason").is_none());
    assert_eq!(result["errorMessage"], json!(refusal));

    let sensitive_events = vec![
        event(
            "message_start",
            json!({"type": "message_start", "message": {"id": "msg_sensitive", "usage": {"input_tokens": 12, "output_tokens": 0}}}),
        ),
        event(
            "message_delta",
            json!({"type": "message_delta", "delta": {"stop_reason": "sensitive"}, "usage": {"input_tokens": 12, "output_tokens": 0}}),
        ),
        event("message_stop", json!({"type": "message_stop"})),
    ];
    let (_events, result, _requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("blocked"))]), None, None),
        api_key(),
        vec![sse_response(sse(&sensitive_events))],
        None,
    )
    .await;
    assert_eq!(result["stopReason"], json!("error"));
    assert!(result.get("rawStopReason").is_none());
    assert_eq!(result["errorMessage"], json!("Provider stopped with: sensitive"));
}

#[tokio::test]
async fn message_delta_without_usage_preserves_message_start_usage() {
    let (_events, result, _requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("Say hello."))]), None, None),
        api_key(),
        vec![sse_response(sse(&minimal_events(None)))],
        None,
    )
    .await;
    assert_eq!(result["stopReason"], json!("stop"));
    assert_eq!(result["usage"]["input"], json!(12));
    assert_eq!(result["usage"]["totalTokens"], json!(12));
}

#[tokio::test]
async fn ignores_unknown_sse_events_after_message_stop() {
    let mut events = minimal_events(Some(json!({
        "input_tokens": 12, "output_tokens": 5,
        "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0
    })));
    events.push(event("done", json!("[DONE]")));
    events.push(("proxy.stats".to_owned(), "not json".to_owned()));
    let (_events, result, _requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("Say hello."))]), None, None),
        api_key(),
        vec![sse_response(sse(&events))],
        None,
    )
    .await;
    assert_eq!(result["stopReason"], json!("stop"));
    assert_eq!(result["content"], json!([{"type": "text", "text": "Hello"}]));
}

#[tokio::test]
async fn truncated_stream_without_message_stop_is_an_error() {
    let mut events = minimal_events(None);
    events.pop();
    let (events, result, _requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, None),
        api_key(),
        vec![sse_response(sse(&events))],
        None,
    )
    .await;
    assert_eq!(result["stopReason"], json!("error"));
    assert_eq!(
        result["errorMessage"],
        json!("Anthropic stream ended before message_stop")
    );
    assert_eq!(events.last().expect("error event")["type"], json!("error"));
}

#[tokio::test]
async fn prices_cache_write_1h_at_twice_input_rate() {
    let events = vec![
        event(
            "message_start",
            json!({"type": "message_start", "message": {"id": "msg_test", "usage": {
                "input_tokens": 100, "output_tokens": 0,
                "cache_read_input_tokens": 0, "cache_creation_input_tokens": 1000000,
                "cache_creation": {"ephemeral_5m_input_tokens": 600000, "ephemeral_1h_input_tokens": 400000}}}}),
        ),
        event(
            "message_delta",
            json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {
                "input_tokens": 100, "output_tokens": 5,
                "cache_read_input_tokens": 0, "cache_creation_input_tokens": 1000000}}),
        ),
        event("message_stop", json!({"type": "message_stop"})),
    ];
    let (_events, result, _requests) = run_stream(
        model("anthropic", "claude-opus-4-8", None),
        context(json!([user(json!("hi"))]), None, None),
        api_key(),
        vec![sse_response(sse(&events))],
        None,
    )
    .await;
    assert_eq!(result["usage"]["cacheWrite"], json!(1000000));
    assert_eq!(result["usage"]["cacheWrite1h"], json!(400000));
    let cache_write = result["usage"]["cost"]["cacheWrite"].as_f64().expect("cost");
    assert!((cache_write - 7.75).abs() < 1e-10, "cache write cost {cache_write}");
}

#[tokio::test]
async fn normalizes_tool_call_ids_in_request_messages() {
    let (_events, _result, requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(
            json!([
                user(json!("run")),
                {"role": "assistant", "content": [
                    {"type": "toolCall", "id": "call/1", "name": "bash", "arguments": {}}
                ], "api": "anthropic-messages", "provider": "anthropic", "model": "claude-haiku-4-5",
                 "usage": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
                           "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}},
                 "stopReason": "toolUse", "timestamp": 2},
                {"role": "toolResult", "toolCallId": "call/1", "toolName": "bash",
                 "content": [{"type": "text", "text": "ok"}], "isError": false, "timestamp": 3}
            ]),
            None,
            None,
        ),
        json!({"apiKey": "test-key", "cacheRetention": "none"}),
        vec![sse_response(sse(&minimal_events(Some(json!({
            "input_tokens": 1, "output_tokens": 1
        })))))],
        None,
    )
    .await;

    let body: Value = serde_json::from_str(requests[0]["body"].as_str().expect("body")).expect("json");
    assert_eq!(body["messages"][1]["content"][0]["id"], json!("call_1"));
    assert_eq!(body["messages"][2]["content"][0]["tool_use_id"], json!("call_1"));
}

#[tokio::test]
async fn empty_thinking_signature_handling_follows_compat() {
    let assistant = json!({"role": "assistant", "content": [
        {"type": "thinking", "thinking": "internal reasoning", "thinkingSignature": ""}
    ], "api": "anthropic-messages", "provider": "vendor", "model": "mimo-v2.5-pro",
    "usage": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
              "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}},
    "stopReason": "stop", "timestamp": 2});

    for (compat, expected) in [
        (None, json!([{"type": "text", "text": "internal reasoning"}])),
        (
            Some(json!({"allowEmptySignature": true})),
            json!([{"type": "thinking", "thinking": "internal reasoning", "signature": ""}]),
        ),
    ] {
        let model = {
            let mut value = model("vendor", "mimo-v2.5-pro", compat);
            value["baseUrl"] = json!("http://127.0.0.1:9");
            value
        };
        let (_events, _result, requests) = run_stream(
            model,
            context(
                json!([user(json!("first")), assistant.clone(), user(json!("second"))]),
                None,
                None,
            ),
            json!({"apiKey": "fake-key", "cacheRetention": "none"}),
            vec![sse_response(sse(&minimal_events(Some(json!({
                "input_tokens": 1, "output_tokens": 1
            })))))],
            None,
        )
        .await;
        let body: Value = serde_json::from_str(requests[0]["body"].as_str().expect("body")).expect("json");
        let assistant = body["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .find(|message| message["role"] == json!("assistant"))
            .expect("assistant message");
        assert_eq!(assistant["content"], expected);
    }
}

#[tokio::test]
async fn temperature_is_omitted_for_thinking_and_unsupported_models() {
    for (compat, thinking_enabled) in [
        (None, Some(true)),
        (Some(json!({"supportsTemperature": false})), Some(false)),
    ] {
        let (_events, _result, requests) = run_stream(
            model("vendor", "vendor--claude", compat),
            context(json!([user(json!("hi"))]), None, None),
            json!({"apiKey": "fake-key", "temperature": 0.0, "thinkingEnabled": thinking_enabled,
                   "cacheRetention": "none"}),
            vec![sse_response(sse(&minimal_events(Some(json!({
                "input_tokens": 1, "output_tokens": 1
            })))))],
            None,
        )
        .await;
        let body: Value = serde_json::from_str(requests[0]["body"].as_str().expect("body")).expect("json");
        assert!(body.get("temperature").is_none(), "temperature must be omitted: {body}");
    }
}

#[tokio::test]
async fn eager_tool_input_streaming_defaults_and_legacy_beta() {
    let tool = json!([{"name": "lookup", "description": "Look up a value",
                       "parameters": {"type": "object", "properties": {"value": {"type": "string"}}}}]);

    let (_events, _result, requests) = run_stream(
        model(
            "vendor",
            "claude-opus-4-8",
            Some(json!({"forceAdaptiveThinking": true})),
        ),
        context(json!([user(json!("Use the tool"))]), None, Some(tool.clone())),
        json!({"apiKey": "test-key", "cacheRetention": "none"}),
        vec![sse_response(sse(&minimal_events(Some(json!({
            "input_tokens": 1, "output_tokens": 1
        })))))],
        None,
    )
    .await;
    let body: Value = serde_json::from_str(requests[0]["body"].as_str().expect("body")).expect("json");
    assert_eq!(body["tools"][0]["eager_input_streaming"], json!(true));
    assert!(requests[0]["headers"].get("anthropic-beta").is_none());

    let (_events, _result, requests) = run_stream(
        model(
            "vendor",
            "claude-opus-4-8",
            Some(json!({"forceAdaptiveThinking": true, "supportsEagerToolInputStreaming": false})),
        ),
        context(json!([user(json!("Use the tool"))]), None, Some(tool.clone())),
        json!({"apiKey": "test-key", "cacheRetention": "none"}),
        vec![sse_response(sse(&minimal_events(Some(json!({
            "input_tokens": 1, "output_tokens": 1
        })))))],
        None,
    )
    .await;
    let body: Value = serde_json::from_str(requests[0]["body"].as_str().expect("body")).expect("json");
    assert!(body["tools"][0].get("eager_input_streaming").is_none());
    assert_eq!(
        requests[0]["headers"]["anthropic-beta"],
        json!("fine-grained-tool-streaming-2025-05-14")
    );

    let (_events, _result, requests) = run_stream(
        model(
            "vendor",
            "claude-opus-4-8",
            Some(json!({"forceAdaptiveThinking": true, "supportsEagerToolInputStreaming": false})),
        ),
        context(json!([user(json!("no tools"))]), None, None),
        json!({"apiKey": "test-key", "cacheRetention": "none"}),
        vec![sse_response(sse(&minimal_events(Some(json!({
            "input_tokens": 1, "output_tokens": 1
        })))))],
        None,
    )
    .await;
    let body: Value = serde_json::from_str(requests[0]["body"].as_str().expect("body")).expect("json");
    assert!(body.get("tools").is_none());
    assert!(requests[0]["headers"].get("anthropic-beta").is_none());
}

#[tokio::test]
async fn cache_retention_controls_cache_control_ttl() {
    for (retention, compat, expected) in [
        (json!("short"), None, json!({"type": "ephemeral"})),
        (json!("long"), None, json!({"type": "ephemeral", "ttl": "1h"})),
        (
            json!("long"),
            Some(json!({"supportsLongCacheRetention": false})),
            json!({"type": "ephemeral"}),
        ),
        (json!("none"), None, Value::Null),
    ] {
        let (_events, _result, requests) = run_stream(
            model("anthropic", "claude-haiku-4-5", compat),
            context(json!([user(json!("hi"))]), Some("System prompt."), None),
            json!({"apiKey": "test-key", "cacheRetention": retention}),
            vec![sse_response(sse(&minimal_events(Some(json!({
                "input_tokens": 1, "output_tokens": 1
            })))))],
            None,
        )
        .await;
        let body: Value = serde_json::from_str(requests[0]["body"].as_str().expect("body")).expect("json");
        if expected.is_null() {
            assert!(body["system"][0].get("cache_control").is_none());
            assert!(body["messages"][0]["content"][0].get("cache_control").is_none());
        } else {
            assert_eq!(body["system"][0]["cache_control"], expected);
        }
    }
}

#[tokio::test]
async fn request_headers_carry_sdk_identity_and_auth() {
    let (_events, _result, requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, None),
        api_key(),
        vec![sse_response(sse(&minimal_events(Some(json!({
            "input_tokens": 1, "output_tokens": 1
        })))))],
        None,
    )
    .await;
    let headers = &requests[0]["headers"];
    assert_eq!(headers["x-api-key"], json!("test-key"));
    assert_eq!(headers["anthropic-version"], json!("2023-06-01"));
    assert_eq!(headers["anthropic-dangerous-direct-browser-access"], json!("true"));
    assert_eq!(headers["content-type"], json!("application/json"));
    assert_eq!(headers["user-agent"], json!("Anthropic/JS 0.91.1"));
    assert_eq!(headers["x-stainless-retry-count"], json!("0"));
    assert_eq!(headers["x-stainless-timeout"], json!("600"));
    assert_eq!(requests[0]["url"], json!("https://api.anthropic.com/v1/messages"));
}

#[tokio::test]
async fn header_owned_auth_skips_api_key_and_oauth_shaping() {
    let (_events, _result, requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), Some("System prompt."), None),
        json!({"headers": {"Authorization": "Bearer gateway-token"}}),
        vec![sse_response(sse(&minimal_events(Some(json!({
            "input_tokens": 1, "output_tokens": 1
        })))))],
        None,
    )
    .await;
    let headers = &requests[0]["headers"];
    assert_eq!(headers["authorization"], json!("Bearer gateway-token"));
    assert!(headers.get("x-api-key").is_none());
    let beta = headers["anthropic-beta"].as_str().unwrap_or_default();
    assert!(!beta.contains("oauth-2025-04-20"));
    let body: Value = serde_json::from_str(requests[0]["body"].as_str().expect("body")).expect("json");
    assert_eq!(body["system"][0]["text"], json!("System prompt."));
}

#[tokio::test]
async fn oauth_tokens_use_claude_code_shaping_and_map_tool_names() {
    let events = vec![
        event(
            "message_start",
            json!({"type": "message_start", "message": {"id": "msg_test", "usage": {"input_tokens": 1, "output_tokens": 0}}}),
        ),
        event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 0, "content_block": {
                "type": "tool_use", "id": "toolu_1", "name": "TodoWrite", "input": {}}}),
        ),
        event("content_block_stop", json!({"type": "content_block_stop", "index": 0})),
        event(
            "message_delta",
            json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {"input_tokens": 1, "output_tokens": 1}}),
        ),
        event("message_stop", json!({"type": "message_stop"})),
    ];
    let (events, result, requests) = run_stream(
        model("anthropic", "claude-sonnet-4-6", None),
        context(
            json!([user(json!("Add a todo"))]),
            Some("System prompt."),
            Some(json!([{"name": "todowrite", "description": "Write a todo",
                          "parameters": {"type": "object", "properties": {"task": {"type": "string"}}}}])),
        ),
        json!({"apiKey": "sk-ant-oat-test", "cacheRetention": "none"}),
        vec![sse_response(sse(&events))],
        None,
    )
    .await;

    let headers = &requests[0]["headers"];
    assert_eq!(headers["authorization"], json!("Bearer sk-ant-oat-test"));
    assert!(headers.get("x-api-key").is_none());
    assert_eq!(headers["user-agent"], json!("claude-cli/2.1.75"));
    assert_eq!(headers["x-app"], json!("cli"));
    let beta = headers["anthropic-beta"].as_str().expect("beta");
    assert!(beta.contains("claude-code-20250219"));
    assert!(beta.contains("oauth-2025-04-20"));

    let body: Value = serde_json::from_str(requests[0]["body"].as_str().expect("body")).expect("json");
    assert_eq!(
        body["system"],
        json!([
            {"type": "text", "text": "You are Claude Code, Anthropic's official CLI for Claude."},
            {"type": "text", "text": "System prompt."}
        ])
    );
    assert_eq!(body["tools"][0]["name"], json!("TodoWrite"));
    let tool_call = result["content"]
        .as_array()
        .expect("content")
        .iter()
        .find(|block| block["type"] == json!("toolCall"))
        .expect("tool call");
    assert_eq!(tool_call["name"], json!("todowrite"));
    assert!(events.iter().any(|event| event["type"] == json!("toolcall_end")));
}

#[tokio::test]
async fn copilot_uses_bearer_auth_and_dynamic_headers() {
    let mut model = model(
        "github-copilot",
        "claude-sonnet-4.6",
        Some(json!({"forceAdaptiveThinking": true})),
    );
    model["headers"] = json!({
        "User-Agent": "GitHubCopilotChat/0.35.0",
        "Copilot-Integration-Id": "vscode-chat",
        "X-Stainless-OS": "Linux",
        "X-Stainless-Arch": "x64",
        "X-Stainless-Runtime": "node",
        "X-Stainless-Runtime-Version": "v22.0.0"
    });
    let (_events, _result, requests) = run_stream(
        model,
        context(
            json!([user(json!("Hello"))]),
            Some("You are a helpful assistant."),
            None,
        ),
        json!({"apiKey": "tid_copilot_session_test_token", "cacheRetention": "none"}),
        vec![sse_response(sse(&minimal_events(Some(json!({
            "input_tokens": 1, "output_tokens": 1
        })))))],
        None,
    )
    .await;
    let headers = &requests[0]["headers"];
    assert_eq!(headers["authorization"], json!("Bearer tid_copilot_session_test_token"));
    assert!(headers.get("x-api-key").is_none());
    assert_eq!(headers["copilot-integration-id"], json!("vscode-chat"));
    assert_eq!(headers["x-initiator"], json!("user"));
    assert_eq!(headers["openai-intent"], json!("conversation-edits"));
    let beta = headers["anthropic-beta"].as_str().unwrap_or_default();
    assert!(!beta.contains("fine-grained-tool-streaming"));
    assert!(!beta.contains("interleaved-thinking-2025-05-14"));
}

#[tokio::test]
async fn retries_retryable_statuses_and_reports_final_http_error() {
    let (_events, _result, requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, None),
        json!({"apiKey": "test-key", "maxRetries": 1}),
        vec![
            status_response(429, &[("retry-after", "0")], "rate limited"),
            sse_response(sse(&minimal_events(Some(json!({
                "input_tokens": 1, "output_tokens": 1
            }))))),
        ],
        None,
    )
    .await;
    assert_eq!(requests.len(), 2);

    let (_events, result, requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, None),
        json!({"apiKey": "test-key"}),
        vec![status_response(
            400,
            &[("content-type", "application/json")],
            r#"{"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: too large"}}"#,
        )],
        None,
    )
    .await;
    assert_eq!(requests.len(), 1);
    assert_eq!(result["stopReason"], json!("error"));
    assert_eq!(
        result["errorMessage"],
        json!(r#"400 {"type":"error","error":{"type":"invalid_request_error","message":"max_tokens: too large"}}"#)
    );
}

#[tokio::test]
async fn aborting_mid_stream_emits_an_aborted_error_event() {
    let signal_controller = pi_js_compat::abort::AbortController::new();
    let signal = signal_controller.signal();
    let body = sse(&minimal_events(Some(json!({
        "input_tokens": 1, "output_tokens": 1
    }))));
    let chunks: Vec<String> = body
        .as_bytes()
        .chunks(40)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect();
    let chunk_refs: Vec<&str> = chunks.iter().map(String::as_str).collect();
    let response = ScriptedResponse::sse(&chunk_refs);
    let fetch = RecordingFetch::with_abort_after_chunks(vec![response], 2, signal_controller.clone());
    let (events, result, _requests) = run_stream_with_fetch(
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, None),
        api_key(),
        fetch,
        Some(signal),
    )
    .await;
    assert_eq!(result["stopReason"], json!("aborted"));
    assert_eq!(result["errorMessage"], json!("Request was aborted"));
    assert_eq!(events.last().expect("error event")["type"], json!("error"));
}

#[tokio::test]
async fn usage_reports_reasoning_tokens_and_cost() {
    let events = vec![
        event(
            "message_start",
            json!({"type": "message_start", "message": {"id": "msg_test", "usage": {
                "input_tokens": 100, "output_tokens": 0,
                "cache_read_input_tokens": 10, "cache_creation_input_tokens": 20}}}),
        ),
        event(
            "message_delta",
            json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {
                "input_tokens": 100, "output_tokens": 50,
                "cache_read_input_tokens": 10, "cache_creation_input_tokens": 20,
                "output_tokens_details": {"thinking_tokens": 7}}}),
        ),
        event("message_stop", json!({"type": "message_stop"})),
    ];
    let (_events, result, _requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, None),
        api_key(),
        vec![sse_response(sse(&events))],
        None,
    )
    .await;
    assert_eq!(result["usage"]["reasoning"], json!(7));
    assert_eq!(result["usage"]["totalTokens"], json!(180));
    assert_eq!(result["usage"]["cost"]["input"], json!(0.0005));
}

#[tokio::test]
async fn redacted_thinking_is_replayed_as_redacted_thinking() {
    let events = vec![
        event(
            "message_start",
            json!({"type": "message_start", "message": {"id": "msg_test", "usage": {"input_tokens": 1, "output_tokens": 0}}}),
        ),
        event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 0, "content_block": {
                "type": "redacted_thinking", "data": "opaque"}}),
        ),
        event("content_block_stop", json!({"type": "content_block_stop", "index": 0})),
        event(
            "message_delta",
            json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {"input_tokens": 1, "output_tokens": 1}}),
        ),
        event("message_stop", json!({"type": "message_stop"})),
    ];
    let (events, result, requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(
            json!([
                user(json!("hi")),
                {"role": "assistant", "content": [
                    {"type": "thinking", "thinking": "[Reasoning redacted]", "thinkingSignature": "opaque", "redacted": true}
                ], "api": "anthropic-messages", "provider": "anthropic", "model": "claude-haiku-4-5",
                 "usage": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
                           "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}},
                 "stopReason": "stop", "timestamp": 2}
            ]),
            None,
            None,
        ),
        json!({"apiKey": "test-key", "cacheRetention": "none"}),
        vec![sse_response(sse(&events))],
        None,
    )
    .await;
    assert_eq!(
        result["content"],
        json!([{"type": "thinking", "thinking": "[Reasoning redacted]",
                "thinkingSignature": "opaque", "redacted": true}])
    );
    assert!(events.iter().any(|event| event["type"] == json!("thinking_end")));
    let body: Value = serde_json::from_str(requests[0]["body"].as_str().expect("body")).expect("json");
    assert_eq!(
        body["messages"][1]["content"][0],
        json!({"type": "redacted_thinking", "data": "opaque"})
    );
}

#[tokio::test]
async fn images_are_sent_as_base64_blocks() {
    let (_events, _result, requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(
            json!([user(json!([
                {"type": "text", "text": "look"},
                {"type": "image", "data": "AAAA", "mimeType": "image/png"}
            ]))]),
            None,
            None,
        ),
        json!({"apiKey": "test-key", "cacheRetention": "none"}),
        vec![sse_response(sse(&minimal_events(Some(json!({
            "input_tokens": 1, "output_tokens": 1
        })))))],
        None,
    )
    .await;
    let body: Value = serde_json::from_str(requests[0]["body"].as_str().expect("body")).expect("json");
    assert_eq!(
        body["messages"][0]["content"][1],
        json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"}})
    );
}

#[tokio::test]
async fn stop_reason_mapping_covers_provider_values() {
    for (provider_reason, expected) in [
        ("end_turn", "stop"),
        ("max_tokens", "length"),
        ("tool_use", "toolUse"),
        ("pause_turn", "stop"),
        ("stop_sequence", "stop"),
    ] {
        let events = vec![
            event(
                "message_start",
                json!({"type": "message_start", "message": {"id": "msg", "usage": {"input_tokens": 1, "output_tokens": 0}}}),
            ),
            event(
                "message_delta",
                json!({"type": "message_delta", "delta": {"stop_reason": provider_reason}, "usage": {"output_tokens": 1}}),
            ),
            event("message_stop", json!({"type": "message_stop"})),
        ];
        let (_events, result, _requests) = run_stream(
            model("anthropic", "claude-haiku-4-5", None),
            context(json!([user(json!("hi"))]), None, None),
            api_key(),
            vec![sse_response(sse(&events))],
            None,
        )
        .await;
        assert_eq!(result["stopReason"], json!(expected), "{provider_reason}");
        assert!(result.get("rawStopReason").is_none());
    }

    let events = vec![
        event(
            "message_start",
            json!({"type": "message_start", "message": {"id": "msg", "usage": {"input_tokens": 1, "output_tokens": 0}}}),
        ),
        event(
            "message_delta",
            json!({"type": "message_delta", "delta": {"stop_reason": "future_reason"}}),
        ),
        event("message_stop", json!({"type": "message_stop"})),
    ];
    let (_events, result, _requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, None),
        api_key(),
        vec![sse_response(sse(&events))],
        None,
    )
    .await;
    assert_eq!(result["stopReason"], json!("error"));
    assert_eq!(result["errorMessage"], json!("Unhandled stop reason: future_reason"));
}

#[tokio::test]
async fn provider_env_long_cache_retention_is_used_as_a_fallback() {
    let (_events, _result, requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), Some("System."), None),
        json!({"apiKey": "test-key", "env": {"PI_CACHE_RETENTION": "long"}}),
        vec![sse_response(sse(&minimal_events(Some(json!({
            "input_tokens": 1, "output_tokens": 1
        })))))],
        None,
    )
    .await;
    let body: Value = serde_json::from_str(requests[0]["body"].as_str().expect("body")).expect("json");
    assert_eq!(
        body["system"][0]["cache_control"],
        json!({"type": "ephemeral", "ttl": "1h"})
    );
}

#[tokio::test]
async fn lazy_api_wrapper_streams_the_response() {
    use pi_ai::api::anthropic_messages_lazy::anthropic_messages_api;
    use pi_ai::request::ProviderStreams;

    let api = anthropic_messages_api();

    let fetch = RecordingFetch::new(vec![sse_response(sse(&minimal_events(Some(json!({
        "input_tokens": 1, "output_tokens": 1
    })))))]);
    let options = stream_options(&api_key(), fetch, None);
    let request_model: Model = serde_json::from_value(model("anthropic", "claude-haiku-4-5", None)).expect("model");
    let request_context: pi_ai::types::Context =
        serde_json::from_value(context(json!([user(json!("hi"))]), None, None)).expect("context");
    let (_events, result) = collect(
        api.stream(request_model, request_context, options)
            .expect("acquisition"),
    )
    .await;
    assert_eq!(result.stop_reason, StopReason::Stop);
}

#[tokio::test]
async fn unparseable_sse_event_reports_decoder_context() {
    let events = vec![("message_start".to_owned(), "not json".to_owned())];
    let (_events, result, _requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, None),
        api_key(),
        vec![sse_response(sse(&events))],
        None,
    )
    .await;
    let message = result["errorMessage"].as_str().expect("error message");
    assert!(
        message.starts_with("Could not parse Anthropic SSE event message_start: "),
        "{message}"
    );
    assert!(
        message.contains("; data=not json; raw=event: message_start\\ndata: not json"),
        "{message}"
    );
}

#[tokio::test]
async fn assistant_content_round_trips_as_expected_types() {
    let (_events, result, _requests) = run_stream(
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, None),
        api_key(),
        vec![sse_response(sse(&minimal_events(Some(json!({
            "input_tokens": 1, "output_tokens": 1
        })))))],
        None,
    )
    .await;
    assert!(matches!(
        serde_json::from_value::<AssistantContent>(result["content"][0].clone()).expect("content"),
        AssistantContent::Text(_)
    ));
    assert_eq!(result["stopReason"], json!("stop"));
    assert_ne!(result["stopReason"], json!(StopReason::Error));
}
