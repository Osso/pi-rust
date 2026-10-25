#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/anthropic.rs"]
mod support;

use std::sync::Arc;

use pi_ai::api::anthropic_messages::{stream, stream_simple};
use pi_ai::types::Model;
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

fn assistant(content: Value) -> Value {
    json!({
        "role": "assistant", "content": content, "api": "anthropic-messages",
        "provider": "anthropic", "model": "claude-haiku-4-5",
        "usage": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
                  "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}},
        "stopReason": "toolUse", "timestamp": 2
    })
}

fn tool_result(id: &str, name: &str, content: Value, added: Option<Value>) -> Value {
    let mut value = json!({"role": "toolResult", "toolCallId": id, "toolName": name,
                           "content": content, "isError": false, "timestamp": 3});
    if let Some(added) = added {
        value["addedToolNames"] = added;
    }
    value
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

fn scripted(body: String) -> Value {
    json!({"status": 200, "headers": [["content-type", "text/event-stream"]], "chunks": [body]})
}

fn text_events() -> Vec<(String, String)> {
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
        event(
            "message_delta",
            json!({"type": "message_delta", "delta": {"stop_reason": "end_turn"}, "usage": {
                "input_tokens": 12, "output_tokens": 5,
                "cache_read_input_tokens": 0, "cache_creation_input_tokens": 0}}),
        ),
        event("message_stop", json!({"type": "message_stop"})),
    ]
}

fn text_script() -> Vec<Value> {
    vec![scripted(sse(&text_events()))]
}

struct Scenario {
    kind: &'static str,
    model: Value,
    context: Value,
    options: Value,
    script: Vec<Value>,
    signal: bool,
}

fn scenario(kind: &'static str, model: Value, context: Value, options: Value, script: Vec<Value>) -> Scenario {
    Scenario {
        kind,
        model,
        context,
        options,
        script,
        signal: false,
    }
}

fn scenarios() -> Vec<Scenario> {
    let mut scenarios = Vec::new();
    let tool = json!([{"name": "lookup", "description": "Look up a value",
                       "parameters": {"type": "object", "properties": {"value": {"type": "string"}},
                                      "required": ["value"]}}]);

    scenarios.push(scenario(
        "stream",
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("Hello"))]), Some("You are precise."), None),
        json!({"apiKey": "test-key", "temperature": 0.2, "cacheRetention": "short"}),
        text_script(),
    ));

    let multi_events = vec![
        event(
            "message_start",
            json!({"type": "message_start", "message": {"id": "msg_multi", "usage": {
                "input_tokens": 20, "output_tokens": 0,
                "cache_read_input_tokens": 3, "cache_creation_input_tokens": 4,
                "cache_creation": {"ephemeral_5m_input_tokens": 1, "ephemeral_1h_input_tokens": 3}}}}),
        ),
        event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 0, "content_block": {
                "type": "thinking", "thinking": "", "signature": ""}}),
        ),
        event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "plan "}}),
        ),
        event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "thinking_delta", "thinking": "steps"}}),
        ),
        event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 0, "delta": {"type": "signature_delta", "signature": "sig"}}),
        ),
        event("content_block_stop", json!({"type": "content_block_stop", "index": 0})),
        event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 1, "content_block": {
                "type": "redacted_thinking", "data": "opaque"}}),
        ),
        event("content_block_stop", json!({"type": "content_block_stop", "index": 1})),
        event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 2, "content_block": {"type": "text", "text": ""}}),
        ),
        event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 2, "delta": {"type": "text_delta", "text": "Use "}}),
        ),
        event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 2, "delta": {"type": "text_delta", "text": "the tool."}}),
        ),
        event("content_block_stop", json!({"type": "content_block_stop", "index": 2})),
        event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 3, "content_block": {
                "type": "tool_use", "id": "toolu_1", "name": "lookup", "input": {}}}),
        ),
        event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 3, "delta": {
                "type": "input_json_delta", "partial_json": "{\"value\":"}}),
        ),
        event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 3, "delta": {
                "type": "input_json_delta", "partial_json": "\"x\"}"}}),
        ),
        event("content_block_stop", json!({"type": "content_block_stop", "index": 3})),
        event(
            "message_delta",
            json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {
                "input_tokens": 20, "output_tokens": 9,
                "cache_read_input_tokens": 3, "cache_creation_input_tokens": 4,
                "output_tokens_details": {"thinking_tokens": 2}}}),
        ),
        event("message_stop", json!({"type": "message_stop"})),
    ];
    scenarios.push(scenario(
        "stream",
        model("anthropic", "claude-haiku-4-5", None),
        context(
            json!([user(json!("Use the tool."))]),
            Some("System."),
            Some(tool.clone()),
        ),
        json!({"apiKey": "test-key", "cacheRetention": "long", "thinkingEnabled": true,
               "thinkingBudgetTokens": 2048, "thinkingDisplay": "omitted"}),
        vec![scripted(sse(&multi_events))],
    ));

    let adaptive = model(
        "anthropic",
        "claude-opus-4-8",
        Some(json!({"forceAdaptiveThinking": true})),
    );
    let mut adaptive = adaptive;
    adaptive["thinkingLevelMap"] = json!({
        "minimal": "low", "low": "low", "medium": "medium", "high": "high",
        "xhigh": "xhigh", "max": "max", "ultra": "max"
    });
    for level in ["minimal", "low", "medium", "high", "xhigh", "max", "ultra"] {
        scenarios.push(scenario(
            "streamSimple",
            adaptive.clone(),
            context(json!([user(json!("Think."))]), None, None),
            json!({"apiKey": "test-key", "reasoning": level, "maxTokens": 4096}),
            text_script(),
        ));
    }

    let mut legacy = model("vendor", "vendor--claude", None);
    legacy["maxTokens"] = json!(8192);
    scenarios.push(scenario(
        "streamSimple",
        legacy.clone(),
        context(json!([user(json!("Think."))]), None, None),
        json!({"apiKey": "test-key", "reasoning": "low", "maxTokens": 4096,
               "thinkingBudgets": {"low": 2048}}),
        text_script(),
    ));
    scenarios.push(scenario(
        "streamSimple",
        legacy.clone(),
        context(json!([user(json!("Think."))]), None, None),
        json!({"apiKey": "test-key", "reasoning": "xhigh", "maxTokens": 4096}),
        text_script(),
    ));
    scenarios.push(scenario(
        "streamSimple",
        legacy,
        context(json!([user(json!("No thinking."))]), None, None),
        json!({"apiKey": "test-key"}),
        text_script(),
    ));
    let mut off_null = model("vendor", "vendor--claude", None);
    off_null["thinkingLevelMap"] = json!({"off": null});
    scenarios.push(scenario(
        "streamSimple",
        off_null,
        context(json!([user(json!("No thinking."))]), None, None),
        json!({"apiKey": "test-key"}),
        text_script(),
    ));

    scenarios.push(scenario(
        "stream",
        model(
            "vendor",
            "claude-opus-4-8",
            Some(json!({"forceAdaptiveThinking": true})),
        ),
        context(json!([user(json!("Use the tool"))]), None, Some(tool.clone())),
        json!({"apiKey": "test-key", "cacheRetention": "none"}),
        text_script(),
    ));
    scenarios.push(scenario(
        "stream",
        model(
            "vendor",
            "claude-opus-4-8",
            Some(json!({"forceAdaptiveThinking": true, "supportsEagerToolInputStreaming": false})),
        ),
        context(json!([user(json!("Use the tool"))]), None, Some(tool.clone())),
        json!({"apiKey": "test-key", "cacheRetention": "none"}),
        text_script(),
    ));
    scenarios.push(scenario(
        "stream",
        model(
            "vendor",
            "claude-opus-4-8",
            Some(json!({"forceAdaptiveThinking": true, "supportsEagerToolInputStreaming": false})),
        ),
        context(json!([user(json!("No tools"))]), None, None),
        json!({"apiKey": "test-key", "cacheRetention": "none"}),
        text_script(),
    ));

    let strict_tools = json!([
        {"name": "lookup", "description": "Look up a value",
         "parameters": {"type": "object", "title": "LookupInput",
                        "properties": {"value": {"type": "string"}}},
         "constrainedSampling": {"type": "json_schema", "strict": "prefer"}},
        {"name": "plain", "description": "Plain tool",
         "parameters": {"type": "object", "properties": {"n": {"type": "number"}}},
         "constrainedSampling": {"type": "json_schema", "strict": "prefer"}}
    ]);
    scenarios.push(scenario(
        "stream",
        model("vendor", "vendor--claude", Some(json!({"supportsStrictTools": true}))),
        context(json!([user(json!("Use tools"))]), None, Some(strict_tools)),
        json!({"apiKey": "test-key", "cacheRetention": "none"}),
        text_script(),
    ));

    scenarios.push(scenario(
        "stream",
        model(
            "anthropic",
            "claude-opus-4-8",
            Some(json!({"supportsToolReferences": true})),
        ),
        context(
            json!([
                user(json!("use base")),
                assistant(json!([{"type": "toolCall", "id": "call_1", "name": "base_tool", "arguments": {}}])),
                tool_result(
                    "call_1",
                    "base_tool",
                    json!([{"type": "text", "text": "done"}]),
                    Some(json!(["late_tool"]))
                ),
                user(json!("tail"))
            ]),
            None,
            Some(json!([
                {"name": "base_tool", "description": "base", "parameters": {"type": "object"}},
                {"name": "late_tool", "description": "late", "parameters": {"type": "object"}}
            ])),
        ),
        json!({"apiKey": "test-key", "cacheRetention": "none"}),
        text_script(),
    ));

    let usage_events = vec![
        event(
            "message_start",
            json!({"type": "message_start", "message": {"id": "msg_usage", "usage": {
                "input_tokens": 100, "output_tokens": 0,
                "cache_read_input_tokens": 10, "cache_creation_input_tokens": 20,
                "cache_creation": {"ephemeral_5m_input_tokens": 10, "ephemeral_1h_input_tokens": 10}}}}),
        ),
        event(
            "message_delta",
            json!({"type": "message_delta", "delta": {"stop_reason": "max_tokens"}, "usage": {
                "input_tokens": 100, "output_tokens": 50,
                "cache_read_input_tokens": 10, "cache_creation_input_tokens": 20,
                "output_tokens_details": {"thinking_tokens": 7}}}),
        ),
        event("message_stop", json!({"type": "message_stop"})),
    ];
    scenarios.push(scenario(
        "stream",
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, None),
        json!({"apiKey": "test-key", "cacheRetention": "short"}),
        vec![scripted(sse(&usage_events))],
    ));

    for (retention, compat) in [
        (json!("short"), None),
        (json!("long"), None),
        (json!("long"), Some(json!({"supportsLongCacheRetention": false}))),
        (json!("none"), None),
    ] {
        scenarios.push(scenario(
            "stream",
            model("anthropic", "claude-haiku-4-5", compat),
            context(json!([user(json!("hi"))]), Some("System."), None),
            json!({"apiKey": "test-key", "cacheRetention": retention}),
            text_script(),
        ));
    }

    scenarios.push(scenario(
        "stream",
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
        text_script(),
    ));
    scenarios.push(scenario(
        "stream",
        model("anthropic", "claude-haiku-4-5", None),
        context(
            json!([
                user(json!("read image")),
                assistant(json!([{"type": "toolCall", "id": "call_1", "name": "read", "arguments": {}}])),
                tool_result(
                    "call_1",
                    "read",
                    json!([{"type": "image", "data": "BBBB", "mimeType": "image/jpeg"}]),
                    None
                )
            ]),
            None,
            None,
        ),
        json!({"apiKey": "test-key", "cacheRetention": "none"}),
        text_script(),
    ));

    scenarios.push(scenario(
        "stream",
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, None),
        json!({"apiKey": "test-key"}),
        vec![json!({"status": 400, "headers": [["content-type", "application/json"]],
                    "chunks": [r#"{"type":"error","error":{"type":"invalid_request_error","message":"bad request"}}"#]})],
    ));
    scenarios.push(scenario(
        "stream",
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, None),
        json!({"apiKey": "test-key"}),
        vec![json!({"status": 500, "headers": [], "chunks": ["server exploded"]})],
    ));
    scenarios.push(scenario(
        "stream",
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, None),
        json!({"apiKey": "test-key", "maxRetries": 1}),
        vec![
            json!({"status": 429, "headers": [["retry-after", "0"]], "chunks": ["rate limited"]}),
            scripted(sse(&text_events())),
        ],
    ));
    scenarios.push(scenario(
        "stream",
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, None),
        json!({"apiKey": "test-key", "maxRetries": 1}),
        vec![
            json!({"status": 500, "headers": [], "chunks": ["boom"]}),
            json!({"status": 500, "headers": [], "chunks": ["boom again"]}),
        ],
    ));
    let abort_chunks = sse(&text_events())
        .as_bytes()
        .chunks(40)
        .map(|chunk| String::from_utf8_lossy(chunk).into_owned())
        .collect::<Vec<_>>();
    let mut abort = scenario(
        "stream",
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, None),
        json!({"apiKey": "test-key"}),
        vec![
            json!({"status": 200, "headers": [["content-type", "text/event-stream"]],
                    "chunks": abort_chunks, "abortAtChunk": 2}),
        ],
    );
    abort.signal = true;
    scenarios.push(abort);

    let mut truncated = text_events();
    truncated.pop();
    scenarios.push(scenario(
        "stream",
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, None),
        json!({"apiKey": "test-key"}),
        vec![scripted(sse(&truncated))],
    ));

    let malformed = r#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\"A\H\",\"text\":\"col1	col2\"}"}}"#.to_owned();
    let repaired = vec![
        event(
            "message_start",
            json!({"type": "message_start", "message": {"id": "msg_repaired", "usage": {
                "input_tokens": 12, "output_tokens": 0}}}),
        ),
        event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 0, "content_block": {
                "type": "tool_use", "id": "toolu_1", "name": "lookup", "input": {}}}),
        ),
        ("content_block_delta".to_owned(), malformed),
        event("content_block_stop", json!({"type": "content_block_stop", "index": 0})),
        event(
            "message_delta",
            json!({"type": "message_delta", "delta": {"stop_reason": "tool_use"}, "usage": {
                "input_tokens": 12, "output_tokens": 5}}),
        ),
        event("message_stop", json!({"type": "message_stop"})),
    ];
    scenarios.push(scenario(
        "stream",
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("Use the tool."))]), None, Some(tool.clone())),
        json!({"apiKey": "test-key", "cacheRetention": "none"}),
        vec![scripted(sse(&repaired))],
    ));

    let oauth_events = vec![
        event(
            "message_start",
            json!({"type": "message_start", "message": {"id": "msg_oauth", "usage": {"input_tokens": 1, "output_tokens": 0}}}),
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
    scenarios.push(scenario(
        "stream",
        model("anthropic", "claude-sonnet-4-6", None),
        context(
            json!([user(json!("Add a todo"))]),
            Some("System prompt."),
            Some(json!([{"name": "todowrite", "description": "Write a todo",
                          "parameters": {"type": "object", "properties": {"task": {"type": "string"}}}}])),
        ),
        json!({"apiKey": "sk-ant-oat-test", "cacheRetention": "none"}),
        vec![scripted(sse(&oauth_events))],
    ));

    let mut copilot = model(
        "github-copilot",
        "claude-sonnet-4.6",
        Some(json!({"forceAdaptiveThinking": true})),
    );
    copilot["headers"] = json!({
        "User-Agent": "GitHubCopilotChat/0.35.0",
        "Copilot-Integration-Id": "vscode-chat",
        "X-Stainless-OS": "Linux",
        "X-Stainless-Arch": "x64",
        "X-Stainless-Runtime": "node",
        "X-Stainless-Runtime-Version": "v22.0.0"
    });
    scenarios.push(scenario(
        "stream",
        copilot,
        context(
            json!([user(json!("Hello"))]),
            Some("You are a helpful assistant."),
            None,
        ),
        json!({"apiKey": "tid_copilot_session_test_token", "cacheRetention": "none"}),
        text_script(),
    ));

    scenarios.push(scenario(
        "stream",
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), Some("System prompt."), None),
        json!({"headers": {"Authorization": "Bearer gateway-token"}}),
        text_script(),
    ));

    scenarios.push(scenario(
        "stream",
        model("anthropic", "claude-haiku-4-5", None),
        context(json!([user(json!("hi"))]), None, Some(tool.clone())),
        json!({"apiKey": "test-key", "cacheRetention": "none",
               "metadata": {"user_id": "user-1"},
               "toolChoice": {"type": "tool", "name": "lookup"}}),
        text_script(),
    ));
    scenarios.push(scenario(
        "stream",
        model(
            "anthropic",
            "claude-opus-4-8",
            Some(json!({"forceAdaptiveThinking": true})),
        ),
        context(json!([user(json!("hi"))]), None, None),
        json!({"apiKey": "test-key", "cacheRetention": "none", "thinkingEnabled": true,
               "effort": "xhigh", "thinkingDisplay": "summarized",
               "toolChoice": "any"}),
        text_script(),
    ));

    scenarios.push(scenario(
        "stream",
        model(
            "anthropic",
            "claude-haiku-4-5",
            Some(json!({"sendSessionAffinityHeaders": true})),
        ),
        context(json!([user(json!("hi"))]), None, None),
        json!({"apiKey": "test-key", "sessionId": "session-1", "cacheRetention": "short"}),
        text_script(),
    ));

    let empty_signature_assistant = json!({"role": "assistant", "content": [
        {"type": "thinking", "thinking": "internal", "thinkingSignature": ""}
    ], "api": "anthropic-messages", "provider": "vendor", "model": "vendor--claude",
    "usage": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
              "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}},
    "stopReason": "stop", "timestamp": 2});
    scenarios.push(scenario(
        "stream",
        model("vendor", "vendor--claude", Some(json!({"allowEmptySignature": true}))),
        context(
            json!([user(json!("first")), empty_signature_assistant, user(json!("second"))]),
            None,
            None,
        ),
        json!({"apiKey": "test-key", "cacheRetention": "none"}),
        text_script(),
    ));

    scenarios.push(scenario(
        "stream",
        model("vendor", "vendor--claude", Some(json!({"supportsTemperature": true}))),
        context(json!([user(json!("hi"))]), None, None),
        json!({"apiKey": "test-key", "temperature": 0.0, "thinkingEnabled": false,
               "cacheRetention": "none"}),
        text_script(),
    ));

    scenarios.push(scenario(
        "stream",
        model("anthropic", "claude-haiku-4-5", None),
        context(
            json!([
                user(json!("run")),
                assistant(json!([{"type": "toolCall", "id": "call/1", "name": "bash", "arguments": {}}])),
                tool_result("call/1", "bash", json!([{"type": "text", "text": "ok"}]), None)
            ]),
            None,
            None,
        ),
        json!({"apiKey": "test-key", "cacheRetention": "none"}),
        text_script(),
    ));

    for (block, delta) in [
        (
            json!({"type": "text", "text": ""}),
            json!({"type": "text_delta", "text": "hello"}),
        ),
        (
            json!({"type": "thinking", "thinking": "", "signature": "sig"}),
            json!({"type": "thinking_delta", "thinking": "plan"}),
        ),
        (
            json!({"type": "tool_use", "id": "call_open", "name": "lookup", "input": {}}),
            json!({"type": "input_json_delta", "partial_json": "{\"value\":\"x\"}"}),
        ),
    ] {
        let mut events = text_events()[..1].to_vec();
        events.push(event(
            "content_block_start",
            json!({"type": "content_block_start", "index": 3, "content_block": block}),
        ));
        events.push(event(
            "content_block_delta",
            json!({"type": "content_block_delta", "index": 3, "delta": delta}),
        ));
        events.push(event(
            "error",
            json!({"type": "error", "error": {"type": "overloaded_error", "message": "busy before block stop"}}),
        ));
        scenarios.push(scenario(
            "stream",
            model("anthropic", "claude-haiku-4-5", None),
            context(json!([user(json!("hi"))]), None, None),
            json!({"apiKey": "test-key", "cacheRetention": "none"}),
            vec![scripted(sse(&events))],
        ));
    }

    scenarios
}

fn scripted_response_from_json(value: &Value) -> ScriptedResponse {
    let status = u16::try_from(value["status"].as_u64().unwrap_or(200)).unwrap_or(200);
    let chunks: Vec<String> = value["chunks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|chunk| chunk.as_str().map(str::to_owned))
        .collect();
    let headers: Vec<(String, String)> = value["headers"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|pair| {
            let pair = pair.as_array()?;
            Some((pair.first()?.as_str()?.to_owned(), pair.get(1)?.as_str()?.to_owned()))
        })
        .collect();
    let is_event_stream = headers
        .iter()
        .any(|(name, value)| name.eq_ignore_ascii_case("content-type") && value.contains("text/event-stream"));
    let mut response = if is_event_stream {
        let chunk_refs: Vec<&str> = chunks.iter().map(String::as_str).collect();
        ScriptedResponse::sse(&chunk_refs)
    } else {
        ScriptedResponse::json(status, &chunks.join(""))
    };
    response.status = status;
    for (name, value) in headers {
        if !name.eq_ignore_ascii_case("content-type") {
            response = response.with_header(&name, &value);
        }
    }
    response
}

fn simple_stream_options(
    options: &Value,
    fetch: Arc<dyn pi_ai::request::Fetch>,
    signal: Option<AbortSignal>,
) -> pi_ai::request::SimpleStreamOptions {
    let mut stream_value = options.clone();
    if let Some(object) = stream_value.as_object_mut() {
        object.remove("reasoning");
        object.remove("thinkingBudgets");
        object.remove("deferred");
    }
    pi_ai::request::SimpleStreamOptions {
        stream: stream_options(&stream_value, fetch, signal),
        reasoning: options
            .get("reasoning")
            .and_then(|value| serde_json::from_value(value.clone()).ok()),
        thinking_budgets: options
            .get("thinkingBudgets")
            .and_then(|value| serde_json::from_value(value.clone()).ok()),
    }
}

async fn observe(request: &Value, controller: Option<pi_js_compat::abort::AbortController>) -> Value {
    let signal = controller.as_ref().map(|controller| controller.signal());
    let model: Model = serde_json::from_value(request["model"].clone()).expect("model");
    let context = serde_json::from_value(request["context"].clone()).expect("context");
    let script_values = request["script"].as_array().expect("script");
    let responses: Vec<ScriptedResponse> = script_values.iter().map(scripted_response_from_json).collect();
    let abort_at_chunk = script_values
        .first()
        .and_then(|value| value["abortAtChunk"].as_u64())
        .and_then(|index| usize::try_from(index).ok());
    let fetch = match (&controller, abort_at_chunk) {
        (Some(controller), Some(chunk)) => {
            RecordingFetch::with_abort_after_chunks(responses, chunk, controller.clone())
        }
        _ => RecordingFetch::new(responses),
    };
    let (events, result) = if request["kind"] == json!("streamSimple") {
        let options = simple_stream_options(&request["options"], fetch.clone(), signal);
        collect(stream_simple(model, context, options)).await
    } else {
        let options = stream_options(&request["options"], fetch.clone(), signal);
        collect(stream(model, context, options)).await
    };
    json!({
        "requests": requests_json(&fetch.requests()),
        "events": events_json(&events),
        "result": message_json(&result),
    })
}

#[tokio::test]
async fn anthropic_messages_matches_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn(
        "anthropic-messages-oracle.ts",
        "packages/ai/src/api/anthropic-messages.ts",
    );
    for (id, scenario) in scenarios().into_iter().enumerate() {
        let controller = scenario.signal.then(pi_js_compat::abort::AbortController::new);
        let request = json!({
            "id": id,
            "kind": scenario.kind,
            "model": scenario.model,
            "context": scenario.context,
            "options": scenario.options,
            "script": scenario.script,
        });
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], json!(true), "scenario {id}: {expected}");
        let observed = observe(&request, controller).await;
        assert_eq!(
            observed, expected["output"],
            "scenario {id}; observed={observed}; expected={}",
            expected["output"]
        );
    }
}
