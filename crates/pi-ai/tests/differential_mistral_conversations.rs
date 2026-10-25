pub use crate::shared_provider_normalize as provider_normalize;

use crate::shared_oracle as oracle;
// The shared support module exposes helpers used by sibling test binaries.
#[allow(dead_code)]
use crate::shared_recording_fetch as recording_fetch;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use pi_ai::api::mistral_conversations::{MistralOptions, stream, stream_simple};
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpRequest, HttpResponse, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{Context, Model, ProviderHeaders, ThinkingLevel};
use pi_js_compat::abort::AbortSignal;
use recording_fetch::{RecordingFetch, ScriptedResponse};
use serde_json::{Value, json};

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
            .flat_map(|text| pi_js_compat::js_json::encode_node_utf8(&text).into_bytes())
            .map(|byte| vec![byte])
            .collect()
    } else {
        chunks
            .into_iter()
            .map(|text| pi_js_compat::js_json::encode_node_utf8(&text).into_bytes())
            .collect()
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

fn stream_options(scenario: &Value, fetch: Arc<RecordingFetch>) -> StreamOptions {
    let options = &scenario["options"];
    let mut stream_options = StreamOptions::default();
    stream_options.request.api_key = options["apiKey"].as_str().map(pi_js_compat::js_json::JsString::from);
    stream_options.request.fetch = Some(fetch);
    stream_options.request.headers = parse_headers(&options["headers"]);
    stream_options.temperature = options["temperature"].as_f64();
    stream_options.max_tokens = options["maxTokens"].as_f64();
    stream_options.session_id = options["sessionId"].as_str().map(pi_js_compat::js_json::JsString::from);
    stream_options.cache_retention = match options["cacheRetention"].as_str() {
        Some("none") => Some(pi_ai::types::CacheRetention::None),
        Some("short") => Some(pi_ai::types::CacheRetention::Short),
        Some("long") => Some(pi_ai::types::CacheRetention::Long),
        _ => None,
    };
    for field in ["toolChoice", "promptMode", "reasoningEffort"] {
        if let Some(value) = options.get(field) {
            stream_options.extra.insert(field.to_owned(), value.clone());
        }
    }
    if options["onPayload"].as_str() == Some("augment") {
        stream_options.request.on_payload = Some(Arc::new(|payload: pi_js_compat::js_json::JsValue, _model: Model| {
            Box::pin(async move {
                let mut payload = payload;
                let object = payload.as_object_mut().expect("payload object");
                object.insert(
                    ("topP".to_owned()).into(),
                    pi_js_compat::js_json::JsValue::from(json!(0.9)),
                );
                object.insert(
                    ("randomSeed".to_owned()).into(),
                    pi_js_compat::js_json::JsValue::from(json!(42)),
                );
                object.insert(
                    ("responseFormat".to_owned()).into(),
                    pi_js_compat::js_json::JsValue::from(json!({
                        "type": "json_schema",
                        "jsonSchema": {
                            "name": "result",
                            "schemaDefinition": {
                                "type": "object",
                                "properties": {"maxTokens": {"type": "number"}},
                            },
                        },
                    })),
                );
                object.insert(
                    ("presencePenalty".to_owned()).into(),
                    pi_js_compat::js_json::JsValue::from(json!(0.1)),
                );
                object.insert(
                    ("frequencyPenalty".to_owned()).into(),
                    pi_js_compat::js_json::JsValue::from(json!(0.2)),
                );
                object.insert(
                    ("parallelToolCalls".to_owned()).into(),
                    pi_js_compat::js_json::JsValue::from(json!(true)),
                );
                object.insert(
                    ("safePrompt".to_owned()).into(),
                    pi_js_compat::js_json::JsValue::from(json!(true)),
                );
                Ok(Some(payload))
            })
        }));
    }
    stream_options
}

struct AbortBeforeResponse {
    recorder: Arc<RecordingFetch>,
    controller: pi_js_compat::abort::AbortController,
}
impl Fetch for AbortBeforeResponse {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async move {
            let signal = signal.expect("SDK Request signal");
            let _response = self.recorder.fetch(request, Some(signal.clone())).await?;
            self.controller.abort(None);
            Err(CallbackError::from_rejection(
                signal.reason().expect("aborted request reason"),
            ))
        })
    }
}

async fn observe(scenario: &Value) -> Value {
    let id = scenario["id"].clone();
    let model: Model = serde_json::from_value(scenario["model"].clone()).expect("model");
    let context: Context = serde_json::from_value(scenario["context"].clone()).expect("context");
    let chunks = response_chunks(scenario);
    let signal_controller = pi_js_compat::abort::AbortController::new();
    let signal = signal_controller.signal();
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
        chunks,
    };
    let fetch = RecordingFetch::new(vec![response]);
    let mut options = stream_options(scenario, Arc::clone(&fetch));
    if scenario.get("abort").is_some() {
        options.request.signal = Some(signal.clone());
        options.request.fetch = Some(Arc::new(AbortBeforeResponse {
            recorder: fetch.clone(),
            controller: signal_controller,
        }));
    }

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

    let payload_observation = Arc::new(Mutex::new(None));
    if scenario["options"]["observePayload"] == true {
        let previous = options.request.on_payload.take();
        let capture = payload_observation.clone();
        options.request.on_payload = Some(Arc::new(move |payload, model| {
            let previous = previous.clone();
            let capture = capture.clone();
            Box::pin(async move {
                *capture.lock().unwrap() = Some(pi_js_compat::js_json::stringify_lossless(&payload));
                match previous {
                    Some(callback) => callback(payload, model).await,
                    None => Ok(None),
                }
            })
        }));
    }
    let api = scenario["api"].as_str().unwrap_or("stream");
    let mut event_stream = if api == "streamSimple" {
        let simple = SimpleStreamOptions {
            stream: options,
            reasoning: scenario["options"]["reasoning"].as_str().and_then(parse_thinking_level),
            thinking_budgets: None,
        };
        stream_simple(model, context, simple).expect("simple acquisition")
    } else {
        stream(model, context, MistralOptions::from_stream_options(options))
    };

    let mut events = Vec::new();
    while let Some(event) = event_stream.next().await {
        let event = event.expect("stream event");
        events.push(serde_json::to_value(event).expect("event JSON"));
    }
    let message = event_stream.result().await.expect("stream result");
    let mut message = serde_json::to_value(message).expect("message JSON");
    provider_normalize::normalize_message_clock(&mut message);
    for event in &mut events {
        provider_normalize::normalize_event_clocks(event);
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

    let mut observation = json!({
        "id": id,
        "ok": true,
        "request": request,
        "onResponse": observed.lock().unwrap().clone(),
        "events": events,
        "message": message,
    });
    if scenario["options"]["observePayload"] == true {
        observation["onPayload"] = json!(payload_observation.lock().unwrap().clone());
    }
    observation
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

fn model(id: &str, reasoning: bool) -> Value {
    json!({
        "id": id,
        "name": "Test Model",
        "api": "mistral-conversations",
        "provider": "mistral",
        "baseUrl": "https://api.mistral.ai",
        "reasoning": reasoning,
        "input": ["text", "image"],
        "cost": {"input": 1, "output": 2, "cacheRead": 0.1, "cacheWrite": 0.2},
        "contextWindow": 128000,
        "maxTokens": 16384,
    })
}

fn reasoning_model(id: &str) -> Value {
    let mut model = model(id, true);
    model["thinkingLevelMap"] = json!({
        "xhigh": "high",
        "max": "high",
        "ultra": "high",
    });
    model
}

fn usage() -> Value {
    json!({"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
           "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}})
}

fn assistant(content: Value) -> Value {
    json!({
        "role": "assistant",
        "content": content,
        "api": "mistral-conversations",
        "provider": "mistral",
        "model": "mistral-large-latest",
        "usage": usage(),
        "stopReason": "toolUse",
        "timestamp": 1,
    })
}

fn user(content: Value) -> Value {
    json!({"role": "user", "content": content, "timestamp": 1})
}

fn terminal(finish_reason: Value) -> String {
    json!({
        "id": "mistral-response-id",
        "choices": [{"index": 0, "finish_reason": finish_reason, "delta": {}}],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
    })
    .to_string()
}

fn sse(events: &[String]) -> Vec<String> {
    let body = events
        .iter()
        .map(|event| format!("data: {event}"))
        .collect::<Vec<_>>()
        .join("\r\n\r\n");
    vec![format!("{body}\r\n\r\ndata: [DONE]\r\n\r\n")]
}

fn scenario(id: usize, model: Value, context: Value, options: Value, response: Value) -> Value {
    json!({"id": id, "model": model, "context": context, "options": options, "response": response})
}

fn scenarios() -> Vec<Value> {
    let mut scenarios = Vec::new();

    let text = json!({
        "id": "r1",
        "choices": [{"index": 0, "finish_reason": null, "delta": {"content": "héllo 🌍"}}],
    })
    .to_string();
    let finish = json!({
        "id": "r1",
        "choices": [{"index": 0, "finish_reason": "stop", "delta": {}}],
        "usage": {"prompt_tokens": 3, "completion_tokens": 2, "total_tokens": 5},
    })
    .to_string();
    scenarios.push(scenario(
        0,
        model("mistral-large-latest", false),
        json!({"messages": [user(json!("hello"))]}),
        json!({"apiKey": "test"}),
        json!({"chunks": sse(&[text, finish]), "chunking": "bytes"}),
    ));

    let thinking = json!({
        "id": "r2",
        "choices": [{"index": 0, "finish_reason": null,
            "delta": {"content": [{"type": "thinking", "thinking": [{"type": "text", "text": "reason"}]}]}}],
    })
    .to_string();
    let text_delta = json!({
        "id": "r2",
        "choices": [{"index": 0, "finish_reason": null,
            "delta": {"content": [{"type": "text", "text": "answer"}]}}],
    })
    .to_string();
    let tool_start = json!({
        "id": "r2",
        "choices": [{"index": 0, "finish_reason": null,
            "delta": {"tool_calls": [{"index": 0, "function": {"name": "lookup", "arguments": "{\"query\":"}}]}}],
    })
    .to_string();
    let tool_finish = json!({
        "id": "r2",
        "choices": [{"index": 0, "finish_reason": "tool_calls",
            "delta": {"tool_calls": [{"index": 0, "function": {"name": "lookup", "arguments": "\"pi\"}"}}]}}],
        "usage": {"prompt_tokens": 10, "completion_tokens": 4, "total_tokens": 14,
            "prompt_tokens_details": {"cached_tokens": 3}},
    })
    .to_string();
    scenarios.push(scenario(
        1,
        model("mistral-large-latest", false),
        json!({"messages": [user(json!("hello"))]}),
        json!({"apiKey": "test"}),
        json!({"chunks": sse(&[thinking, text_delta, tool_start, tool_finish])}),
    ));

    let tool_id = json!({
        "id": "r3",
        "choices": [{"index": 0, "finish_reason": null,
            "delta": {"tool_calls": [{"id": "abc123456", "index": 0,
                "function": {"name": "lookup", "arguments": "{\"query\":\"pi\"}"}}]}}],
        "usage": {"prompt_tokens": 5, "completion_tokens": 2, "total_tokens": 0,
            "num_cached_tokens": 9},
    })
    .to_string();
    scenarios.push(scenario(
        2,
        model("mistral-large-latest", false),
        json!({"messages": [user(json!("hello"))]}),
        json!({"apiKey": "test"}),
        json!({"chunks": sse(&[tool_id, terminal(json!("stop"))])}),
    ));

    for (index, finish_reason) in [
        "stop",
        "length",
        "model_length",
        "tool_calls",
        "error",
        "unmapped_error",
    ]
    .into_iter()
    .enumerate()
    {
        scenarios.push(scenario(
            10 + index,
            model("devstral-medium-latest", false),
            json!({"messages": [user(json!("hello"))]}),
            json!({"apiKey": "test"}),
            json!({"chunks": sse(&[terminal(json!(finish_reason))])}),
        ));
    }
    scenarios.push(scenario(
        16,
        model("devstral-medium-latest", false),
        json!({"messages": [user(json!("hello"))]}),
        json!({"apiKey": "test"}),
        json!({"chunks": sse(&[terminal(Value::Null)])}),
    ));

    scenarios.push(scenario(
        20,
        model("mistral-large-latest", false),
        json!({"messages": [user(json!("hello"))]}),
        json!({"apiKey": "test"}),
        json!({"status": 403, "chunks": ["{\"message\":\"blocked by gateway\"}"]}),
    ));
    scenarios.push(scenario(
        21,
        model("mistral-large-latest", false),
        json!({"messages": [user(json!("hello"))]}),
        json!({"apiKey": "test"}),
        json!({"status": 429, "headers": {"retry-after": "7"}, "chunks": ["{\"message\":\"slow down\"}"]}),
    ));
    scenarios.push(scenario(
        22,
        model("mistral-large-latest", false),
        json!({"messages": [user(json!("hello"))]}),
        json!({"apiKey": "test"}),
        json!({"status": 500, "chunks": []}),
    ));

    scenarios.push(scenario(
        30,
        model("mistral-large-latest", false),
        json!({"messages": [user(json!("hello"))]}),
        json!({"apiKey": "test"}),
        json!({"chunks": ["data: {\"choices\": 3}\n\n"]}),
    ));
    scenarios.push(scenario(
        31,
        model("mistral-large-latest", false),
        json!({"messages": [user(json!("hello"))]}),
        json!({"apiKey": "test"}),
        json!({"chunks": ["data: not-json\n\n"]}),
    ));
    scenarios.push(scenario(
        32,
        model("mistral-large-latest", false),
        json!({"messages": [user(json!("hello"))]}),
        json!({"apiKey": "test"}),
        json!({"chunks": ["data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial\"}}]}\n\n"]}),
    ));

    scenarios.push(scenario(
        40,
        model("mistral-large-latest", false),
        json!({"messages": [user(json!("hello"))]}),
        json!({"apiKey": "test"}),
        json!({"chunks": [], "chunking": "chunks"}),
    ));
    let mut abort = scenarios.pop().unwrap();
    abort["abort"] = json!("before-read");
    scenarios.push(abort);

    scenarios.push(scenario(
        50,
        model("mistral-large-latest", false),
        json!({
            "systemPrompt": "Be precise",
            "messages": [
                user(json!([
                    {"type": "text", "text": "describe"},
                    {"type": "image", "data": "aGVsbG8=", "mimeType": "image/png"},
                ])),
                assistant(json!([
                    {"type": "thinking", "thinking": "reason"},
                    {"type": "text", "text": "answer"},
                    {"type": "toolCall", "id": "call_1|fc_1", "name": "lookup", "arguments": {"query": "pi"}},
                ])),
                {"role": "toolResult", "toolCallId": "call_1|fc_1", "toolName": "lookup",
                 "content": [{"type": "text", "text": "found"}, {"type": "image", "data": "aGVsbG8=", "mimeType": "image/png"}],
                 "isError": false, "timestamp": 2},
            ],
            "tools": [{
                "name": "lookup",
                "description": "Look something up",
                "parameters": {
                    "type": "object",
                    "properties": {"query": {"type": "string"}},
                    "required": ["query"],
                    "additionalProperties": false,
                },
                "constrainedSampling": {"type": "json_schema", "strict": "require"},
            }],
        }),
        json!({
            "apiKey": "secret",
            "maxTokens": 123,
            "promptMode": "reasoning",
            "reasoningEffort": "high",
            "toolChoice": {"type": "function", "function": {"name": "lookup"}},
            "sessionId": "session-1",
            "headers": {"x-custom": "value"},
            "onPayload": "augment",
        }),
        json!({"chunks": sse(&[terminal(json!("stop"))])}),
    ));

    let mut suppressed = scenario(
        51,
        {
            let mut model = model("mistral-large-latest", false);
            model["headers"] = json!({"Authorization": "Bearer model-key", "X-Affinity": "model-affinity"});
            model
        },
        json!({"messages": [user(json!("hello"))]}),
        json!({
            "apiKey": "request-key",
            "sessionId": "automatic-affinity",
            "headers": {"authorization": null, "x-affinity": null},
        }),
        json!({"chunks": sse(&[terminal(json!("stop"))])}),
    );
    suppressed["id"] = json!(51);
    scenarios.push(suppressed);

    for (index, level) in ["minimal", "low", "medium", "high", "xhigh", "max", "ultra"]
        .into_iter()
        .enumerate()
    {
        for (model_index, model_id) in ["magistral-medium-latest", "mistral-small-2603", "mistral-medium-3.5"]
            .into_iter()
            .enumerate()
        {
            scenarios.push(scenario(
                100 + index * 10 + model_index,
                reasoning_model(model_id),
                json!({"messages": [user(json!("hello"))]}),
                json!({"apiKey": "test", "reasoning": level}),
                json!({"chunks": sse(&[terminal(json!("stop"))])}),
            ));
            let mut scenario = scenarios.pop().unwrap();
            scenario["api"] = json!("streamSimple");
            scenarios.push(scenario);
        }
    }

    for (index, model_id) in ["mistral-small-2603", "magistral-medium-latest", "mistral-large-latest"]
        .into_iter()
        .enumerate()
    {
        scenarios.push(scenario(
            200 + index,
            model(model_id, false),
            json!({"messages": [user(json!("hello"))]}),
            json!({"apiKey": "test", "reasoning": "high"}),
            json!({"chunks": sse(&[terminal(json!("stop"))])}),
        ));
        let mut scenario = scenarios.pop().unwrap();
        scenario["api"] = json!("streamSimple");
        scenarios.push(scenario);
    }

    scenarios
}

#[tokio::test]
async fn mistral_valid_sdk_events_preserve_full_native_snapshots() {
    let mut oracle = oracle::TsOracle::spawn(
        "mistral-conversations-oracle.ts",
        "packages/ai/src/api/mistral-conversations.ts",
    );
    let text = json!({"id":"r_valid","model":"mistral-large-latest","choices":[{"index":0,"delta":{"content":"hello"},"finish_reason":null}]}).to_string();
    let start = json!({"id":"r_valid","model":"mistral-large-latest","choices":[{"index":0,"delta":{"tool_calls":[{
        "index":0,"id":"abc123456","function":{"name":"lookup","arguments":"{\"n\":"}
    }]},"finish_reason":null}]})
    .to_string();
    let delta = json!({"id":"r_valid","model":"mistral-large-latest","choices":[{"index":0,"delta":{"tool_calls":[{
        "index":0,"id":"abc123456","function":{"name":"lookup","arguments":"1}"}
    }]},"finish_reason":null}]})
    .to_string();
    let mut stop: Value = serde_json::from_str(&terminal(json!("tool_calls"))).unwrap();
    stop["model"] = json!("mistral-large-latest");
    let cases = [
        vec![text.clone(), stop.to_string()],
        vec![start.clone(), delta, stop.to_string()],
        vec![start.clone()],
        vec![text],
        vec![start, "not-json".to_owned()],
    ];
    for (index, events) in cases.into_iter().enumerate() {
        let request = scenario(
            400 + index,
            model("mistral-large-latest", false),
            json!({"messages":[]}),
            json!({"apiKey":"test"}),
            json!({"chunks":sse(&events)}),
        );
        let expected = oracle.request(&request);
        let actual = observe(&request).await;
        assert_eq!(actual, expected, "valid SDK event case{index}");
    }
}

#[test]
fn mistral_sdk_chunk_header_validation_matches_frozen_errors() {
    let mut oracle = oracle::TsOracle::spawn(
        "mistral-conversations-oracle.ts",
        "packages/ai/src/api/mistral-conversations.ts",
    );
    for (index, chunk) in [
        json!({"choices":3}),
        json!({"id":null,"model":1,"choices":[]}),
        json!({"id":"valid","object":null,"model":"m","choices":[]}),
        json!({"id":"valid","created":1.25,"model":"m","choices":[]}),
        json!({"id":"valid","created":9007199254740992.0,"model":"m","choices":[]}),
    ]
    .into_iter()
    .enumerate()
    {
        let request = scenario(
            300 + index,
            model("mistral-large-latest", false),
            json!({"messages":[]}),
            json!({"apiKey":"test"}),
            json!({"chunks":sse(&[chunk.to_string()])}),
        );
        let expected = oracle.request(&request);
        let observed = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(observe(&request));
        assert_eq!(observed, expected, "header case{index}");
    }
}

async fn compare_mistral_contract(request: Value) {
    let mut source = oracle::TsOracle::spawn(
        "mistral-conversations-oracle.ts",
        "packages/ai/src/api/mistral-conversations.ts",
    );
    let expected = source.request(&request);
    assert_eq!(observe(&request).await, expected);
}

fn replay_context(text: &str) -> Value {
    json!({"tools":[{"name":"read","description":"Read","parameters":{"type":"object","properties":{}}}],
        "messages":[assistant(json!([{"type":"toolCall","id":"callvalid","name":"read","arguments":{"path":"file"}}])),
        {"role":"toolResult","toolCallId":"callvalid","toolName":"read","content":[{"type":"text","text":text}],"isError":false,"timestamp":1}]})
}

#[tokio::test]
async fn mistral_callback_receives_source_payload_before_sdk_defaults_and_ordering() {
    let chunk = json!({"id":"r","model":"mistral-large-latest","choices":[{"index":0,"delta":{"content":"done"},"finish_reason":"stop"}]}).to_string();
    compare_mistral_contract(scenario(
        730,
        model("mistral-large-latest", false),
        replay_context("done"),
        json!({"apiKey":"key","observePayload":true}),
        json!({"chunks":sse(&[chunk])}),
    ))
    .await;
}

#[tokio::test]
async fn mistral_tool_result_text_uses_javascript_bom_trimming() {
    let chunk = json!({"id":"r","model":"mistral-large-latest","choices":[{"index":0,"delta":{"content":"done"},"finish_reason":"stop"}]}).to_string();
    compare_mistral_contract(scenario(
        731,
        model("mistral-large-latest", false),
        replay_context("\u{feff}done\u{feff}"),
        json!({"apiKey":"key"}),
        json!({"chunks":sse(&[chunk])}),
    ))
    .await;
}

#[tokio::test]
async fn mistral_valid_thinking_interleaved_tools_usage_and_unknown_fields_match_source() {
    let aliases = [
        "promptTokensDetails",
        "prompt_tokens_details",
        "promptTokenDetails",
        "prompt_token_details",
        "numCachedTokens",
        "num_cached_tokens",
    ];
    for (index, alias) in aliases.into_iter().enumerate() {
        let mut usage = json!({"prompt_tokens":20,"completion_tokens":4,"total_tokens":24});
        usage[alias] = if alias.contains("Details") {
            json!({"cachedTokens":7})
        } else if alias.contains("details") {
            json!({"cached_tokens":7})
        } else {
            json!(7)
        };
        let chunks = [
            json!({"id":"r","model":"mistral-large-latest","choices":[{"index":0,"delta":{"content":[{"type":"thinking","thinking":[{"type":"text","text":"consider"}]},{"type":"text","text":"answer"}]},"finish_reason":null}]}),
            json!({"id":"later","model":"mistral-large-latest","choices":[{"index":0,"delta":{"tool_calls":[{"id":"call00001","index":0,"function":{"name":"read","arguments":"{\"x\":"}},{"id":"call00002","index":1,"function":{"name":"echo","arguments":{"v":1}}}]},"finish_reason":null}]}),
            json!({"id":"later","model":"mistral-large-latest","choices":[{"index":0,"delta":{"tool_calls":[{"id":"call00001","index":0,"function":{"name":"ignored","arguments":"2}"}}],"content":"tail"},"finish_reason":"tool_calls"}],"usage":usage,"unknown_server_field":{"a":1}}),
        ].map(|value|value.to_string());
        compare_mistral_contract(scenario(
            740 + index,
            model("mistral-large-latest", true),
            json!({"messages":[user(json!("hi"))]}),
            json!({"apiKey":"key"}),
            json!({"chunks":sse(&chunks)}),
        ))
        .await;
    }
}

#[tokio::test]
async fn mistral_sse_decoder_removes_only_the_leading_transport_bom() {
    for chunking in ["chunks", "bytes"] {
        let chunk = json!({"id":"r","model":"mistral-large-latest","choices":[{"index":0,"delta":{"content":"inside\u{feff}body"},"finish_reason":"stop"}]}).to_string();
        let mut chunks = sse(&[chunk]);
        chunks[0].insert(0, '\u{feff}');
        compare_mistral_contract(scenario(
            750,
            model("mistral-large-latest", false),
            json!({"messages":[user(json!("hi"))]}),
            json!({"apiKey":"key"}),
            json!({"chunks":chunks,"chunking":chunking}),
        ))
        .await;
    }
}

#[test]
fn mistral_stream_matches_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn(
        "mistral-conversations-oracle.ts",
        "packages/ai/src/api/mistral-conversations.ts",
    );
    for scenario in scenarios() {
        eprintln!("mistral scenario {}", scenario["id"]);
        let expected = oracle.request(&scenario);
        let observed = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime")
            .block_on(observe(&scenario));
        assert_eq!(
            observed, expected,
            "scenario {}; observed={observed}; expected={expected}",
            scenario["id"]
        );
    }
}
