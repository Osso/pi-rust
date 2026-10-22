#[path = "support/oracle.rs"]
mod oracle;
// The shared support module exposes helpers used by sibling test binaries.
#[allow(dead_code)]
#[path = "support/recording_fetch.rs"]
mod recording_fetch;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use pi_ai::api::pi_messages::{PiMessagesOptions, stream, stream_simple};
use pi_ai::request::{SimpleStreamOptions, StreamOptions};
use pi_ai::types::{Context, Model, ProviderEnv, ProviderHeaders, ThinkingLevel};
use recording_fetch::{RecordingFetch, ScriptedResponse};
use serde_json::{Value, json};

/// Rebuilds a value with the normalizations that only substitute
/// environment-derived values. Key order is left untouched.
fn canonical(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(canonical).collect()),
        Value::Object(object) => {
            let is_end_event = object
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|kind| matches!(kind, "text_end" | "thinking_end") && object.contains_key("partial"));
            Value::Object(
                object
                    .into_iter()
                    .filter(|(key, _)| !(is_end_event && matches!(key.as_str(), "contentSignature" | "redacted")))
                    .map(|(key, value)| (key, canonical(value)))
                    .collect(),
            )
        }
        Value::Number(number) => match number.as_f64() {
            Some(float) => pi_js_compat::number::js_number_value(float),
            None => Value::Number(number),
        },
        other => other,
    }
}

fn normalize(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                normalize(item);
            }
        }
        Value::Object(object) => {
            object.remove("partialArgs");
            object.remove("stack");
            for key in ["timestamp", "timestampMs"] {
                if object.contains_key(key) {
                    object.insert(key.to_owned(), json!(0));
                }
            }
            for entry in object.values_mut() {
                normalize(entry);
            }
        }
        _ => {}
    }
}

fn response_chunks(scenario: &Value) -> Vec<Vec<u8>> {
    let chunks = scenario["response"]["chunks"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|chunk| chunk.as_str().map(pi_js_compat::js_json::JsString::from))
        .collect::<Vec<_>>();
    if scenario["response"]["chunking"].as_str() == Some("bytes") {
        chunks
            .into_iter()
            .flat_map(String::into_bytes)
            .map(|byte| vec![byte])
            .collect()
    } else {
        chunks.into_iter().map(String::into_bytes).collect()
    }
}

fn parse_headers(value: &Value) -> Option<ProviderHeaders> {
    let object = value.as_object()?;
    let headers: indexmap::IndexMap<String, Option<pi_js_compat::js_json::JsString>> = object
        .iter()
        .map(|(name, value)| (name.clone(), value.as_str().map(pi_js_compat::js_json::JsString::from)))
        .collect();
    Some(headers)
}

fn parse_env(value: &Value) -> Option<ProviderEnv> {
    let object = value.as_object()?;
    let env: ProviderEnv = object
        .iter()
        .filter_map(|(name, value)| value.as_str().map(|value| (name.clone(), value.to_owned())))
        .collect();
    Some(env)
}

fn stream_options(scenario: &Value, fetch: Arc<RecordingFetch>) -> StreamOptions {
    let options = &scenario["options"];
    let mut stream_options = StreamOptions::default();
    stream_options.request.api_key = (options["apiKey"].as_str().map(pi_js_compat::js_json::JsString::from))
        .map(pi_js_compat::js_json::JsString::from);
    stream_options.request.fetch = Some(fetch);
    stream_options.request.headers = parse_headers(&options["headers"]);
    stream_options.request.env = parse_env(&options["env"]);
    stream_options.temperature = options["temperature"].as_f64();
    stream_options.max_tokens = options["maxTokens"].as_f64();
    stream_options.session_id = (options["sessionId"].as_str().map(pi_js_compat::js_json::JsString::from))
        .map(pi_js_compat::js_json::JsString::from);
    stream_options.cache_retention = match options["cacheRetention"].as_str() {
        Some("none") => Some(pi_ai::types::CacheRetention::None),
        Some("short") => Some(pi_ai::types::CacheRetention::Short),
        Some("long") => Some(pi_ai::types::CacheRetention::Long),
        _ => None,
    };
    if let Some(value) = options.get("toolChoice") {
        stream_options.extra.insert("toolChoice".to_owned(), value.clone());
    }
    if let Some(value) = options.get("reasoning") {
        stream_options.extra.insert("reasoning".to_owned(), value.clone());
    }
    if let Some(value) = options.get("debug") {
        stream_options.extra.insert("debug".to_owned(), value.clone());
    }
    if options["onPayload"].as_str() == Some("replace") {
        stream_options.request.on_payload = Some(Arc::new(|payload: Value, _model: Model| {
            Box::pin(async move {
                let mut payload = payload;
                payload["marker"] = json!(true);
                Ok(Some(payload))
            })
        }));
    }
    stream_options
}

fn parse_thinking_level(value: &str) -> Option<ThinkingLevel> {
    match value {
        "minimal" => Some(ThinkingLevel::Minimal),
        "low" => Some(ThinkingLevel::Low),
        "medium" => Some(ThinkingLevel::Medium),
        "high" => Some(ThinkingLevel::High),
        "xhigh" => Some(ThinkingLevel::Xhigh),
        "max" => Some(ThinkingLevel::Max),
        "ultra" => Some(ThinkingLevel::Ultra),
        _ => None,
    }
}

async fn observe(scenario: &Value) -> Value {
    let id = scenario["id"].clone();
    let model: Model = serde_json::from_value(scenario["model"].clone()).expect("model");
    let context: Context = serde_json::from_value(scenario["context"].clone()).expect("context");
    let response_headers = scenario["response"]["headers"]
        .as_object()
        .map(|headers| {
            headers
                .iter()
                .filter_map(|(name, value)| value.as_str().map(|value| (name.clone(), value.to_owned())))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut headers = vec![("content-type".to_owned(), "text/event-stream".to_owned())];
    headers.extend(response_headers);
    let response = ScriptedResponse {
        status: scenario["response"]["status"].as_u64().unwrap_or(200) as u16,
        headers,
        chunks: response_chunks(scenario),
    };
    let fetch = RecordingFetch::new(vec![response]);
    let mut options = stream_options(scenario, Arc::clone(&fetch));

    let observed = Arc::new(Mutex::new(None));
    let observed_clone = Arc::clone(&observed);
    options.request.on_response = Some(Arc::new(move |response, _model| {
        let observed = Arc::clone(&observed_clone);
        Box::pin(async move {
            *observed.lock().unwrap() = Some(json!({
                "status": response.status,
                "headers": response.headers,
            }));
            Ok(())
        })
    }));

    let api = scenario["api"].as_str().unwrap_or("stream");
    let mut event_stream = if api == "streamSimple" {
        let simple = SimpleStreamOptions {
            stream: options,
            reasoning: scenario["options"]["reasoning"].as_str().and_then(parse_thinking_level),
            deferred: None,
            thinking_budgets: None,
        };
        stream_simple(model, context, simple)
    } else {
        stream(model, context, PiMessagesOptions::from_stream_options(options))
    };

    let mut events = Vec::new();
    while let Some(event) = event_stream.next().await {
        let event = event.expect("stream event");
        events.push(serde_json::to_value(event).expect("event JSON"));
    }
    let message = event_stream.result().await.expect("stream result");
    let mut message = serde_json::to_value(message).expect("message JSON");
    normalize(&mut message);
    for event in &mut events {
        normalize(event);
    }

    let request = fetch.requests().first().cloned().map(|request| {
        let mut captured = serde_json::Map::new();
        captured.insert("url".to_owned(), json!(request.url));
        captured.insert("method".to_owned(), json!(request.method));
        captured.insert(
            "headers".to_owned(),
            json!(request.headers.iter().cloned().collect::<BTreeMap<_, _>>()),
        );
        if let Some(body) = request.body {
            captured.insert(
                "body".to_owned(),
                json!(String::from_utf8(body).expect("request body UTF-8")),
            );
        }
        Value::Object(captured)
    });

    canonical(json!({
        "id": id,
        "ok": true,
        "request": request,
        "onResponse": observed.lock().unwrap().clone(),
        "events": events,
        "message": message,
    }))
}

fn model() -> Value {
    json!({
        "id": "auto",
        "name": "Radius Auto",
        "api": "pi-messages",
        "provider": "radius",
        "baseUrl": "http://127.0.0.1:1234/v1",
        "reasoning": false,
        "input": ["text"],
        "cost": {"input": 1, "output": 2, "cacheRead": 0.1, "cacheWrite": 0.2},
        "contextWindow": 128000,
        "maxTokens": 16384,
    })
}

fn usage() -> Value {
    json!({
        "input": 10, "output": 5, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 15,
        "cost": {"input": 0.1, "output": 0.2, "cacheRead": 0, "cacheWrite": 0, "total": 0.3},
    })
}

fn context() -> Value {
    json!({"messages": [{"role": "user", "content": "Hello", "timestamp": 1}]})
}

fn sse(events: &[Value]) -> Vec<String> {
    events.iter().map(|event| format!("data: {event}\n\n")).collect()
}

fn scenario(id: usize, options: Value, response: Value) -> Value {
    json!({
        "id": id,
        "model": model(),
        "context": context(),
        "options": options,
        "response": response,
    })
}

fn scenarios() -> Vec<Value> {
    let mut scenarios = Vec::new();

    let text = [
        json!({"type": "start"}),
        json!({"type": "text_start", "contentIndex": 0}),
        json!({"type": "text_delta", "contentIndex": 0, "delta": "Hel"}),
        json!({"type": "text_delta", "contentIndex": 0, "delta": "lo"}),
        json!({"type": "text_end", "contentIndex": 0, "content": "Hello"}),
        json!({"type": "toolcall_start", "contentIndex": 1, "id": "call_1", "toolName": "read"}),
        json!({"type": "toolcall_delta", "contentIndex": 1, "delta": "{\"path\":"}),
        json!({"type": "toolcall_delta", "contentIndex": 1, "delta": "\"a.txt\"}"}),
        json!({
            "type": "toolcall_end",
            "contentIndex": 1,
            "toolCall": {"type": "toolCall", "id": "call_1", "name": "read", "arguments": {"path": "a.txt"}},
        }),
        json!({"type": "done", "reason": "toolUse", "usage": usage(), "responseId": "resp_1"}),
    ];
    scenarios.push(scenario(
        0,
        json!({"apiKey": "test-key", "maxTokens": 100, "sessionId": "session-1", "toolChoice": "auto", "headers": {"x-custom": "1"}}),
        json!({"chunks": sse(&text), "chunking": "bytes"}),
    ));

    let thinking = [
        json!({"type": "thinking_start", "contentIndex": 0}),
        json!({"type": "thinking_delta", "contentIndex": 0, "delta": "rea"}),
        json!({"type": "thinking_delta", "contentIndex": 0, "delta": "son"}),
        json!({"type": "thinking_end", "contentIndex": 0, "content": "reason", "contentSignature": "sig", "redacted": false}),
        json!({"type": "text_start", "contentIndex": 1}),
        json!({"type": "text_delta", "contentIndex": 1, "delta": "answer"}),
        json!({"type": "text_end", "contentIndex": 1, "content": "answer", "contentSignature": "text-sig"}),
        json!({"type": "done", "reason": "stop", "usage": usage()}),
    ];
    scenarios.push(scenario(
        1,
        json!({"apiKey": "test-key"}),
        json!({"chunks": sse(&thinking)}),
    ));

    let rewrite = json!({
        "type": "done",
        "reason": "stop",
        "usage": usage(),
        "rewrite": {
            "policyId": "policy-1",
            "policyVersion": 2,
            "changed": true,
            "tokenCountChange": -3,
            "messageCountChange": 0,
            "systemPromptChanged": false,
        },
    });
    scenarios.push(scenario(
        2,
        json!({"apiKey": "test-key"}),
        json!({"chunks": sse(&[
            json!({"type": "text_start", "contentIndex": 0}),
            json!({"type": "text_delta", "contentIndex": 0, "delta": "hi"}),
            rewrite,
        ])}),
    ));

    scenarios.push(scenario(
        3,
        json!({"apiKey": "test-key"}),
        json!({"chunks": sse(&[
            json!({"type": "start"}),
            json!({"type": "error", "reason": "error", "usage": usage(), "errorMessage": "Upstream failed",
                "rewrite": {"policyId": "p", "policyVersion": 1, "changed": false,
                    "tokenCountChange": 0, "messageCountChange": 0, "systemPromptChanged": false}}),
        ])}),
    ));

    scenarios.push(scenario(
        4,
        json!({"apiKey": "stale"}),
        json!({"status": 401, "chunks": [
            "{\"error\":{\"message\":\"Token expired\",\"code\":\"unauthorized\"}}"
        ]}),
    ));
    scenarios.push(scenario(
        5,
        json!({"apiKey": "test-key"}),
        json!({"status": 500, "chunks": ["not json body"]}),
    ));

    scenarios.push(scenario(
        6,
        json!({"apiKey": "test-key"}),
        json!({"chunks": sse(&[
            json!({"type": "start"}),
            json!({"type": "text_start", "contentIndex": 0}),
            json!({"type": "text_delta", "contentIndex": 0, "delta": "partial"}),
        ])}),
    ));
    scenarios.push(scenario(
        7,
        json!({"apiKey": "test-key"}),
        json!({"chunks": ["data: not-json\n\n"]}),
    ));

    scenarios.push(scenario(
        8,
        json!({"apiKey": "test-key", "debug": true}),
        json!({"headers": {"x-pi-gateway-upstream-provider": "anthropic"},
            "chunks": sse(&[json!({"type": "done", "reason": "stop", "usage": usage()})])}),
    ));

    scenarios.push(scenario(
        9,
        json!({"apiKey": "test-key", "onPayload": "replace"}),
        json!({"chunks": sse(&[json!({"type": "done", "reason": "stop", "usage": usage()})])}),
    ));

    let mut simple = scenario(
        11,
        json!({"apiKey": "test-key", "reasoning": "medium", "toolChoice": "required", "debug": true}),
        json!({"chunks": sse(&[json!({"type": "done", "reason": "stop", "usage": usage()})])}),
    );
    simple["api"] = json!("streamSimple");
    scenarios.push(simple);

    scenarios.push(scenario(
        12,
        json!({"apiKey": "test-key", "sessionId": "s1", "env": {"PI_CACHE_RETENTION": "long"},
            "headers": {"authorization": null, "x-extra": "yes"}, "temperature": 0.5}),
        json!({"chunks": sse(&[json!({"type": "done", "reason": "stop", "usage": usage()})])}),
    ));

    scenarios.push(scenario(
        13,
        json!({"apiKey": "test-key", "cacheRetention": "none", "sessionId": "s1"}),
        json!({"chunks": sse(&[json!({"type": "done", "reason": "length", "usage": usage()})])}),
    ));

    scenarios
}

#[test]
fn pi_messages_stream_matches_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn("pi-messages-oracle.ts", "packages/ai/src/api/pi-messages.ts");
    for scenario in scenarios() {
        let expected = canonical(oracle.request(&scenario));
        let observed = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(observe(&scenario));
        assert_eq!(observed, expected, "scenario {}", scenario["id"]);
    }
}
