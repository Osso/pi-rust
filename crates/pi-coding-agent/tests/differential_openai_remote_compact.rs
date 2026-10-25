//! Differential: `extensions/openai-remote-compact` helpers against the pinned TypeScript module.

#[path = "support/oracle.rs"]
mod oracle;

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use std::collections::HashSet;

use pi_agent_core::agent::AgentMessage;
use pi_ai::types::Model;
use pi_coding_agent::bundled_extensions::openai_remote_compact::{
    build_compact_endpoint, build_openai_compact_instructions, build_openai_compact_payload,
    build_openai_request_headers, extract_openai_compact_details, limit_openai_compact_input,
};
use serde_json::{Map, Value, json};

const SOURCE_MODULE: &str = "packages/coding-agent/extensions/openai-remote-compact/src/index.ts";

fn model(provider: &str, api: &str, base_url: &str, id: &str, image: bool) -> Value {
    json!({
        "id": id,
        "name": id,
        "api": api,
        "provider": provider,
        "baseUrl": base_url,
        "reasoning": true,
        "input": if image { json!(["text", "image"]) } else { json!(["text"]) },
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 100000,
        "maxTokens": 4096
    })
}

fn parse_model(value: &Value) -> Model {
    serde_json::from_value(value.clone()).expect("model")
}

fn codex_token(account_id: &str) -> String {
    let payload = json!({"https://api.openai.com/auth": {"chatgpt_account_id": account_id}}).to_string();
    let encoded = pi_js_compat::base64::btoa(payload.as_bytes())
        .replace('+', "-")
        .replace('/', "_")
        .trim_end_matches('=')
        .to_owned();
    format!("header.{encoded}.signature")
}

fn big_text(chars: usize) -> String {
    "a".repeat(chars)
}

#[test]
fn openai_remote_compact_helpers_match_typescript() {
    let text_model = model(
        "openai",
        "openai-responses",
        "https://api.openai.com/v1/",
        "gpt-5.6",
        false,
    );
    let image_model = model(
        "openai",
        "openai-responses",
        "https://api.openai.com/v1",
        "gpt-5.6",
        true,
    );
    let codex_model = model(
        "openai-codex",
        "openai-codex-responses",
        "https://chatgpt.com/backend-api/codex",
        "gpt-5.6-sol",
        false,
    );
    let codex_responses_model = model(
        "openai-codex-gc",
        "openai-codex-responses",
        "https://example.test/codex/responses/",
        "gpt-5.6-sol",
        false,
    );
    let token = codex_token("acct-123");
    let mut header_map = Map::new();
    header_map.insert("x-custom".to_owned(), Value::String("yes".to_owned()));
    header_map.insert("authorization".to_owned(), Value::String("Bearer preset".to_owned()));
    header_map.insert("x-null".to_owned(), Value::Null);

    let big_message = json!({
        "type": "message",
        "role": "user",
        "content": [{"type": "input_text", "text": big_text(500_000)}]
    });
    let image_message = json!({
        "type": "message",
        "role": "user",
        "content": [
            {"type": "input_text", "text": "look"},
            {"type": "input_image", "detail": "auto", "image_url": "data:image/png;base64,AAAA"}
        ]
    });
    let call = json!({
        "type": "function_call",
        "id": "item-1",
        "call_id": "call-1",
        "name": "read",
        "arguments": "{}"
    });
    let output = json!({"type": "function_call_output", "call_id": "call-1", "output": big_text(450_000)});
    let native = json!({"type": "compaction", "encrypted_content": "abc"});
    let small = json!({"type": "message", "role": "assistant", "content": [{"type": "output_text", "text": "hi"}]});
    let previous = vec![native.clone()];

    let cases = vec![
        json!({"kind": "extract", "name": "numeric-history-dedup", "model": text_model,
        "endpoint": "test", "response": {"output": [
            {"type":"compaction", "encrypted_content":"abc", "nested":{"2":2.0,"1":1.0}},
            {"type":"message", "id":"m1", "role":"assistant", "content":[{"type":"output_text","text":"x","score":2.0}]},
            {"type":"message", "id":"m2", "role":"assistant", "content":[{"type":"output_text","text":"x","score":2}]}
        ]}}),
        json!({"kind": "instructions", "name": "base", "base": "  Keep the goal.  "}),
        json!({"kind": "instructions", "name": "custom", "base": "base", "custom": "  focus here  "}),
        json!({"kind": "instructions", "name": "blank-custom", "base": "base", "custom": "   "}),
        json!({"kind": "endpoint", "name": "openai", "model": text_model}),
        json!({"kind": "endpoint", "name": "codex", "model": codex_model}),
        json!({"kind": "endpoint", "name": "codex-responses", "model": codex_responses_model}),
        json!({"kind": "headers", "name": "api-key", "model": text_model, "apiKey": "sk-test"}),
        json!({"kind": "headers", "name": "empty-api-key", "model": codex_model, "apiKey": ""}),
        json!({"kind": "headers", "name": "content-type-overridden", "model": text_model, "headers": {"content-type":"text/plain"}}),
        json!({"kind": "payload", "name": "oversized-pinned-message", "model": text_model, "instructions": "checkpoint", "previous": [], "pinnedMessageIndices": [0], "messages": [{"role":"user", "content":big_text(500_000), "timestamp":1}]}),
        json!({"kind": "headers", "name": "preset-auth", "model": text_model, "apiKey": "sk-test", "headers": header_map}),
        json!({"kind": "headers", "name": "codex", "model": codex_model, "apiKey": token}),
        json!({"kind": "limit", "name": "small", "items": [small.clone(), call.clone(), output.clone()]}),
        json!({"kind": "limit", "name": "truncate", "items": [big_message.clone()]}),
        json!({"kind": "limit", "name": "drop-images", "items": [image_message.clone()]}),
        json!({
            "kind": "limit",
            "name": "pinned-native",
            "items": [native.clone(), small.clone(), big_message.clone(), call.clone(), output.clone()],
            "pinnedIndices": [0],
            "nativeIndices": [0]
        }),
        json!({
            "kind": "extract",
            "name": "extract",
            "model": codex_model,
            "endpoint": "https://chatgpt.com/backend-api/codex/responses/compact",
            "response": {"output": [
                {"type": "compaction", "encrypted_content": "abc"},
                {"type": "message", "id": "m1", "role": "assistant", "content": [{"type": "output_text", "text": "one"}]},
                {"type": "message", "id": "m2", "role": "assistant", "content": [{"type": "output_text", "text": "two"}]},
                {"type": "message", "id": "m1", "role": "assistant", "content": [{"type": "output_text", "text": "one"}]}
            ]}
        }),
        json!({
            "kind": "payload",
            "name": "payload",
            "model": image_model,
            "instructions": "instructions",
            "previous": previous,
            "messages": [
                {"role": "user", "content": "hello", "timestamp": 1},
                {"role": "assistant", "content": [
                    {"type": "text", "text": "working"},
                    {"type": "toolCall", "id": "call-1|item-1", "name": "read", "arguments": {"2": 2.0, "1": 1.0, "path": "a"}}
                ], "api": "openai-responses", "provider": "openai", "model": "gpt-5.6",
                 "usage": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
                           "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}},
                 "stopReason": "toolUse", "timestamp": 2},
                {"role": "toolResult", "toolCallId": "call-1|item-1", "toolName": "read", "content": [
                    {"type": "text", "text": "contents"},
                    {"type": "image", "data": "AAAA", "mimeType": "image/png"}
                ], "isError": false, "timestamp": 3},
                {"role": "user", "content": [{"type": "text", "text": "next"}], "timestamp": 4}
            ]
        }),
        json!({
            "kind": "payload",
            "name": "payload-pinned",
            "model": text_model,
            "instructions": "instructions",
            "previous": [],
            "messages": [
                {"role": "user", "content": "first", "timestamp": 1},
                {"role": "user", "content": "second", "timestamp": 2}
            ],
            "pinnedMessageIndices": [1]
        }),
    ];

    let mut ts = oracle::TsOracle::spawn("coding-agent-openai-remote-compact-oracle.ts", SOURCE_MODULE);
    let response = ts.request(&json!({"id": 1, "operation": "openai-remote-compact", "cases": cases}));
    assert_eq!(response["ok"], true, "{response}");
    let results = response["output"]["results"].as_array().expect("results").clone();
    assert_eq!(results.len(), cases.len());

    for (index, case) in cases.iter().enumerate() {
        let name = case["name"].as_str().unwrap();
        let expected = &results[index]["value"];
        match case["kind"].as_str().unwrap() {
            "instructions" => {
                let actual = build_openai_compact_instructions(
                    &JsString::from(case["base"].as_str().unwrap()),
                    case["custom"].as_str().map(JsString::from).as_ref(),
                );
                assert_eq!(JsValue::String(actual), JsValue::from(expected.clone()), "{name}");
            }
            "endpoint" => {
                let actual = build_compact_endpoint(&parse_model(&case["model"]));
                assert_eq!(JsValue::String(actual), JsValue::from(expected.clone()), "{name}");
            }
            "headers" => {
                let headers = case.get("headers").map(|headers| JsValue::from(headers.clone()));
                let actual = build_openai_request_headers(
                    &parse_model(&case["model"]),
                    case["apiKey"].as_str().map(JsString::from).as_ref(),
                    headers.as_ref().and_then(JsValue::as_object),
                )
                .unwrap();
                assert_eq!(JsValue::Object(actual), JsValue::from(expected.clone()), "{name}");
            }
            "limit" => {
                let items = case["items"].as_array().unwrap().clone();
                let pinned: HashSet<usize> = case["pinnedIndices"]
                    .as_array()
                    .map(|indices| indices.iter().map(|index| index.as_u64().unwrap() as usize).collect())
                    .unwrap_or_default();
                let native: HashSet<usize> = case["nativeIndices"]
                    .as_array()
                    .map(|indices| indices.iter().map(|index| index.as_u64().unwrap() as usize).collect())
                    .unwrap_or_default();
                let items = items.into_iter().map(JsValue::from).collect();
                let actual = JsValue::Array(limit_openai_compact_input(items, &pinned, &native));
                assert_eq!(actual, JsValue::from(expected.clone()), "{name}");
            }
            "extract" => {
                let details = extract_openai_compact_details(
                    &parse_model(&case["model"]),
                    &JsValue::from(case["response"].clone()),
                    &JsString::from(case["endpoint"].as_str().unwrap()),
                )
                .expect("details");
                let actual = serde_json::to_value(details).expect("details json");
                // Compare the JS-visible bytes, not serde's internal integer/float variants.
                assert_eq!(
                    pi_js_compat::js_json::stringify(&actual),
                    serde_json::to_string(expected).unwrap(),
                    "{name}"
                );
            }
            "payload" => {
                let messages: Vec<AgentMessage> = case["messages"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|message| serde_json::from_value(message.clone()).expect("agent message"))
                    .collect();

                let previous: Vec<JsValue> = case["previous"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
                    .into_iter()
                    .map(JsValue::from)
                    .collect();
                let pinned: HashSet<usize> = case["pinnedMessageIndices"]
                    .as_array()
                    .map(|indices| indices.iter().map(|index| index.as_u64().unwrap() as usize).collect())
                    .unwrap_or_default();
                let payload = build_openai_compact_payload(
                    &parse_model(&case["model"]),
                    &messages,
                    &JsString::from(case["instructions"].as_str().unwrap()),
                    &previous,
                    pinned,
                )
                .expect("payload");
                let actual = serde_json::to_value(payload).expect("payload json");
                assert_eq!(actual, *expected, "{name}");
            }
            other => panic!("unknown case kind {other}"),
        }
    }
}

#[test]
fn numeric_properties_consume_compaction_budget_in_javascript_order() {
    let item: Value = serde_json::from_str(&format!(
        r#"{{"type":"message","2":{{"type":"input_text","text":"{}"}},"1":{{"type":"input_text","text":"{}"}}}}"#,
        "B".repeat(300_000),
        "A".repeat(300_000)
    ))
    .unwrap();
    let mut ts = oracle::TsOracle::spawn("coding-agent-openai-remote-compact-oracle.ts", SOURCE_MODULE);
    let response = ts.request(&json!({"id": 2, "operation": "openai-remote-compact", "cases": [
        {"kind": "limit", "name": "numeric-budget", "items": [item.clone()]}
    ]}));
    assert_eq!(response["ok"], true);
    let expected = &response["output"]["results"][0]["value"];
    println!(
        "pinned TS retained A={}, B={}",
        expected[0]["1"]["text"].as_str().unwrap().len(),
        expected[0]["2"]["text"].as_str().unwrap().len()
    );
    let actual = limit_openai_compact_input(vec![JsValue::from(item)], &HashSet::new(), &HashSet::new());
    assert!(
        serde_json::to_value(actual).unwrap() == *expected,
        "full compacted output differs from pinned TS"
    );
}

#[test]
fn codex_compaction_keeps_selected_model_for_each_provider() {
    let mut ts = oracle::TsOracle::spawn("coding-agent-openai-remote-compact-oracle.ts", SOURCE_MODULE);
    for provider in ["openai-codex", "openai-codex-gc", "openai-codex-team"] {
        let value = model(
            provider,
            "openai-codex-responses",
            "https://example.test/codex",
            "selected-model",
            false,
        );
        let native_model = parse_model(&value);
        assert!(
            pi_coding_agent::bundled_extensions::openai_remote_compact::is_openai_responses_model(Some(&native_model)),
            "{provider}"
        );
        let response = ts.request(&json!({"id": 3, "operation": "openai-remote-compact", "cases": [
            {"kind":"payload", "name":provider, "model":value, "messages":[], "instructions":"checkpoint", "previous":[]},
            {"kind":"extract", "name":provider, "model":value, "endpoint":"test", "response":{"output":[{"type":"compaction", "encrypted_content":"abc"}]}}
        ]}));
        assert_eq!(response["ok"], true, "{response}");
        let payload =
            build_openai_compact_payload(&native_model, &[], &JsString::from("checkpoint"), &[], HashSet::new())
                .unwrap();
        assert_eq!(
            serde_json::to_value(payload).unwrap(),
            response["output"]["results"][0]["value"],
            "{provider}"
        );
        let details = extract_openai_compact_details(
            &native_model,
            &JsValue::from(json!({"output":[{"type":"compaction", "encrypted_content":"abc"}]})),
            &JsString::from("test"),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(details).unwrap(),
            response["output"]["results"][1]["value"],
            "{provider}"
        );
    }
}
