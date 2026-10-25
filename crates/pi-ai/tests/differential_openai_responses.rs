#[path = "support/provider_normalize.rs"]
pub mod provider_normalize;

#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/recording_fetch.rs"]
mod recording_fetch;

use std::collections::BTreeSet;
use std::sync::Arc;

use oracle::TsOracle;
use pi_ai::api::openai_responses::{OpenAIResponsesApi, stream, stream_simple};
use pi_ai::request::{Fetch, HttpRequest, ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{CacheRetention, Context, Model, ProviderHeaders};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use pi_js_compat::abort::AbortSignal;
use recording_fetch::{RecordingFetch, ScriptedResponse};
use serde_json::{Value, json};

const ENV_HEADERS: [&str; 4] = [
    "x-stainless-os",
    "x-stainless-arch",
    "x-stainless-runtime",
    "x-stainless-runtime-version",
];

fn base_model(provider: &str, id: &str, reasoning: bool) -> Value {
    json!({
        "id": id, "name": id, "api": "openai-responses", "provider": provider,
        "baseUrl": if provider == "openrouter" { "https://openrouter.ai/api/v1" } else { "https://api.openai.com/v1" },
        "reasoning": reasoning,
        "input": ["text", "image"],
        "cost": {"input": 1.0, "output": 2.0, "cacheRead": 0.5, "cacheWrite": 0.75},
        "contextWindow": 128000, "maxTokens": 4096
    })
}

fn usage() -> Value {
    json!({
        "input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0,
        "totalTokens": 0.0,
        "cost": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0, "total": 0.0}
    })
}

fn text_events(text: &str) -> Vec<Value> {
    vec![
        json!({"type": "response.created", "response": {"id": "resp_test"}}),
        json!({"type": "response.output_item.added", "output_index": 0,
            "item": {"type": "message", "id": "msg_test", "phase": "final_answer"}}),
        json!({"type": "response.output_text.delta", "output_index": 0, "delta": text}),
        json!({"type": "response.output_item.done", "output_index": 0,
            "item": {"type": "message", "id": "msg_test", "phase": "final_answer",
                     "content": [{"type": "output_text", "text": text}]}}),
        json!({"type": "response.completed", "response": {"id": "resp_test", "status": "completed",
            "usage": {"input_tokens": 10, "output_tokens": 5, "total_tokens": 15,
                "input_tokens_details": {"cached_tokens": 2, "cache_write_tokens": 1},
                "output_tokens_details": {"reasoning_tokens": 3}}}}),
    ]
}

fn sse_chunks(events: &[Value], chunk_chars: usize) -> Vec<String> {
    let mut payload = String::new();
    for event in events {
        payload.push_str("data: ");
        payload.push_str(&event.to_string());
        payload.push_str("\n\n");
    }
    payload
        .chars()
        .collect::<Vec<_>>()
        .chunks(chunk_chars.max(1))
        .map(|chunk| chunk.iter().collect())
        .collect()
}

fn provider_headers(value: Option<&Value>) -> Option<ProviderHeaders> {
    value.and_then(Value::as_object).map(|headers| {
        headers
            .iter()
            .map(|(name, value)| {
                (
                    name.clone(),
                    match value {
                        Value::Null => None,
                        Value::String(value) => Some(pi_js_compat::js_json::JsString::from(value)),
                        other => Some(pi_js_compat::js_json::JsString::from(other.to_string())),
                    },
                )
            })
            .collect()
    })
}

fn stream_options(stream: &Value, extra: &Value, fetch: Arc<dyn Fetch>, signal: Option<AbortSignal>) -> StreamOptions {
    let mut request = ProviderRequestOptions {
        api_key: Some(
            (stream
                .get("apiKey")
                .and_then(Value::as_str)
                .unwrap_or("test-key")
                .to_owned())
            .into(),
        ),
        fetch: Some(fetch),
        signal,
        headers: provider_headers(stream.get("headers")),
        timeout_ms: stream.get("timeoutMs").and_then(Value::as_f64),
        max_retries: stream.get("maxRetries").and_then(Value::as_f64),
        max_retry_delay_ms: stream.get("maxRetryDelayMs").and_then(Value::as_f64),
        ..Default::default()
    };
    if let Some(env) = stream.get("env").and_then(Value::as_object) {
        request.env = Some(
            env.iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        pi_js_compat::js_json::JsString::from(value.as_str().unwrap_or("")),
                    )
                })
                .collect::<indexmap::IndexMap<_, _>>(),
        );
    }
    let mut extra_map = extra.as_object().cloned().unwrap_or_default();
    let known = [
        "apiKey",
        "headers",
        "timeoutMs",
        "maxRetries",
        "maxRetryDelayMs",
        "temperature",
        "samplingParams",
        "maxTokens",
        "cacheRetention",
        "sessionId",
        "metadata",
        "env",
        "transport",
        "websocketConnectTimeoutMs",
    ];
    for (key, value) in stream.as_object().into_iter().flatten() {
        if !known.contains(&key.as_str()) {
            extra_map.insert(key.clone(), value.clone());
        }
    }
    StreamOptions {
        request,
        cwd: None,
        temperature: stream.get("temperature").and_then(Value::as_f64),
        sampling_params: stream.get("samplingParams").and_then(Value::as_object).cloned(),
        max_tokens: stream.get("maxTokens").and_then(Value::as_f64),
        transport: stream
            .get("transport")
            .and_then(Value::as_str)
            .and_then(|transport| serde_json::from_value(Value::String(transport.to_owned())).ok()),
        cache_retention: stream
            .get("cacheRetention")
            .and_then(Value::as_str)
            .map(|retention| match retention {
                "none" => CacheRetention::None,
                "long" => CacheRetention::Long,
                _ => CacheRetention::Short,
            }),
        session_id: (stream.get("sessionId").and_then(Value::as_str).map(str::to_owned))
            .map(pi_js_compat::js_json::JsString::from),
        on_retry: None,
        websocket_connect_timeout_ms: stream.get("websocketConnectTimeoutMs").and_then(Value::as_f64),
        metadata: stream
            .get("metadata")
            .and_then(|value| pi_js_compat::js_json::JsValue::from(value.clone()).as_object().cloned()),
        extra: extra_map,
    }
}

fn request_value(request: &HttpRequest, configured_headers: &BTreeSet<String>) -> Value {
    let headers = request
        .headers
        .iter()
        .map(|(name, value)| {
            let value = if ENV_HEADERS.contains(&name.as_str()) && !configured_headers.contains(name) {
                "<env>"
            } else {
                value
            };
            (name.clone(), Value::String(value.to_owned()))
        })
        .collect::<serde_json::Map<_, _>>();
    let body = request
        .body
        .as_deref()
        .and_then(|body| serde_json::from_slice::<Value>(body).ok())
        .unwrap_or(Value::Null);
    json!({
        "url": request.url,
        "method": request.method,
        "headers": headers,
        "body": body
    })
}

fn assert_json_eq(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(actual), Value::Number(expected)) => {
            assert_eq!(actual.as_f64(), expected.as_f64(), "number mismatch at {path}");
        }
        (Value::Array(actual), Value::Array(expected)) => {
            assert_eq!(actual.len(), expected.len(), "array length mismatch at {path}");
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                assert_json_eq(actual, expected, &format!("{path}[{index}]"));
            }
        }
        (Value::Object(actual), Value::Object(expected)) => {
            assert_eq!(
                actual.keys().collect::<BTreeSet<_>>(),
                expected.keys().collect::<BTreeSet<_>>(),
                "object keys mismatch at {path}"
            );
            for (key, expected) in expected {
                assert_json_eq(&actual[key], expected, &format!("{path}.{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "value mismatch at {path}"),
    }
}

#[test]
fn clock_normalization_preserves_arbitrary_request_and_argument_json() {
    let data = json!({"timestamp":123,"timestampMs":456,"id":"probe_1700000000000_1",
        "stack":"user stack","partialJson":"user text","partialArgs":"args","streamIndex":7,"index":9});
    let mut capture = json!({"requests":[{"body":{"metadata":data}}],
        "events":[{"type":"toolcall_delta","partial":{"timestamp":1000,"content":[{"type":"toolCall","arguments":data}]}}],
        "result":{"timestamp":2000,"content":[{"type":"toolCall","arguments":data}],"errorMessage":"Invalid Codex SSE JSON: exact text"}});
    normalize_timestamps(&mut capture);
    assert_eq!(capture["events"][0]["partial"]["timestamp"], 0);
    assert_eq!(capture["result"]["timestamp"], 0);
    assert_eq!(capture["events"][0]["partial"]["content"][0]["arguments"], data);
    assert_eq!(capture["result"]["content"][0]["arguments"], data);
    assert_eq!(capture["requests"][0]["body"]["metadata"], data);
    assert_eq!(capture["result"]["errorMessage"], "Invalid Codex SSE JSON: exact text");
}

fn normalize_timestamps(value: &mut Value) {
    provider_normalize::normalize_capture_clocks(value);
}

async fn collect_stream(mut stream: AssistantMessageEventStream) -> (Vec<Value>, Value) {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(serde_json::to_value(event.expect("event")).expect("event json"));
    }
    let result = match stream.result().await {
        Ok(message) => serde_json::to_value(message).expect("result json"),
        Err(failure) => json!({"error": failure.message()}),
    };
    (events, result)
}

struct Scenario {
    id: i64,
    model: Value,
    context: Value,
    stream: Value,
    extra: Value,
    responses: Vec<ScriptedResponse>,
    reasoning: Option<String>,
    abort_after_chunks: Option<usize>,
    simple: bool,
}

impl Scenario {
    fn new(id: i64, model: Value, events: &[Value]) -> Self {
        Self {
            id,
            model,
            context: json!({"messages": [{"role": "user", "content": "hi", "timestamp": 1}]}),
            stream: json!({}),
            extra: json!({}),
            responses: vec![ScriptedResponse::sse(
                &sse_chunks(events, 7).iter().map(String::as_str).collect::<Vec<_>>(),
            )],
            reasoning: None,
            abort_after_chunks: None,
            simple: false,
        }
    }

    fn context(mut self, context: Value) -> Self {
        self.context = context;
        self
    }

    fn stream(mut self, stream: Value) -> Self {
        self.stream = stream;
        self
    }

    fn extra(mut self, extra: Value) -> Self {
        self.extra = extra;
        self
    }

    fn responses(mut self, responses: Vec<ScriptedResponse>) -> Self {
        self.responses = responses;
        self
    }

    fn reasoning(mut self, reasoning: &str) -> Self {
        self.reasoning = Some(reasoning.to_owned());
        self.simple = true;
        self
    }

    fn abort_after(mut self, chunks: usize) -> Self {
        self.abort_after_chunks = Some(chunks);
        self
    }
}

async fn run_scenario(oracle: &mut TsOracle, scenario: Scenario) {
    let mut request = json!({
        "id": scenario.id,
        "operation": if scenario.simple { "streamSimple" } else { "stream" },
        "model": scenario.model,
        "context": scenario.context,
        "stream": scenario.stream,
        "extra": scenario.extra,
        "responses": scenario.responses.iter().map(|response| json!({
            "status": response.status,
            "headers": response.headers.iter().cloned().collect::<indexmap::IndexMap<_, _>>(),
            "chunks": response.chunks.iter().map(|chunk| String::from_utf8_lossy(chunk).into_owned()).collect::<Vec<_>>()
        })).collect::<Vec<_>>(),
    });
    if let Some(reasoning) = &scenario.reasoning {
        request["reasoning"] = Value::String(reasoning.clone());
    }
    if let Some(chunks) = scenario.abort_after_chunks {
        request["abortAfterChunks"] = json!(chunks);
    }
    let expected = oracle.request(&request);
    assert_eq!(expected["ok"], Value::Bool(true), "oracle failed: {expected}");
    let expected = &expected["output"];

    let model: Model = serde_json::from_value(scenario.model.clone()).expect("model");
    let context: Context = serde_json::from_value(scenario.context.clone()).expect("context");
    let controller = scenario
        .abort_after_chunks
        .map(|_| pi_js_compat::abort::AbortController::new());
    let signal = controller.as_ref().map(|controller| controller.signal());
    let fetch: Arc<RecordingFetch> = match (scenario.abort_after_chunks, &controller) {
        (Some(chunks), Some(controller)) => {
            RecordingFetch::with_abort_after_chunks(scenario.responses.clone(), chunks, controller.clone())
        }
        _ => RecordingFetch::new(scenario.responses.clone()),
    };
    let options = stream_options(&scenario.stream, &scenario.extra, fetch.clone(), signal.clone());

    let stream = if scenario.simple {
        let reasoning = scenario
            .reasoning
            .as_deref()
            .and_then(|reasoning| serde_json::from_value(Value::String(reasoning.to_owned())).ok());
        stream_simple(
            model,
            context,
            SimpleStreamOptions {
                stream: options,
                reasoning,
                ..Default::default()
            },
        )
        .expect("simple acquisition")
    } else {
        stream(model, context, options)
    };
    let (events, result) = collect_stream(stream).await;
    let configured_headers = provider_normalize::configured_headers(&scenario.model, &scenario.stream);
    let requests = fetch
        .requests()
        .iter()
        .map(|request| request_value(request, &configured_headers))
        .collect::<Vec<_>>();
    let mut actual = json!({"requests": requests, "events": events, "result": result});
    normalize_timestamps(&mut actual);
    assert_json_eq(&actual, expected, &format!("openai-responses scenario {}", scenario.id));
    let _ = OpenAIResponsesApi;
}

fn model_with_compat(provider: &str, id: &str, compat: Value) -> Value {
    let mut model = base_model(provider, id, true);
    model["compat"] = compat;
    model
}

#[tokio::test]
async fn differential_openai_responses_stream() {
    let mut oracle = TsOracle::spawn("openai-responses-oracle.ts", "packages/ai/src/api/openai-responses.ts");

    let text = text_events("Hello");
    run_scenario(
        &mut oracle,
        Scenario::new(0, base_model("openai", "gpt-5.5", true), &text)
            .stream(json!({"sessionId": "session-123", "maxTokens": 5})),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(1, base_model("github-copilot", "gpt-5-mini", true), &text),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(2, base_model("openai", "gpt-5.5", true), &text)
            .extra(json!({"reasoningEffort": "high", "reasoningSummary": "concise"})),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(3, base_model("openai", "gpt-5.5", true), &text).reasoning("high"),
    )
    .await;

    let tool_model = model_with_compat("openai", "gpt-5.6-sol", json!({"supportsStrictMode": true}));
    let tool_context = json!({
        "messages": [{"role": "user", "content": "Use a tool.", "timestamp": 1}],
        "tools": [
            {"name": "ordinary", "description": "An ordinary tool",
             "parameters": {"type": "object", "properties": {"path": {"type": "string"}}}},
            {"name": "constrained", "description": "A constrained tool",
             "parameters": {"type": "object", "properties": {"value": {"type": "string"}}, "required": ["value"]},
             "constrainedSampling": {"type": "json_schema", "strict": "prefer"}}
        ]
    });
    run_scenario(&mut oracle, Scenario::new(4, tool_model, &text).context(tool_context)).await;

    let grammar_model = model_with_compat(
        "openai",
        "gpt-5.5",
        json!({"supportsOpenAIGrammarTools": true, "supportsStrictMode": true}),
    );
    let grammar_context = json!({
        "messages": [{"role": "user", "content": "Use a tool.", "timestamp": 1}],
        "tools": [
            {"name": "query", "description": "Query",
             "parameters": {"type": "object", "properties": {"input": {"type": "string"}}, "required": ["input"]},
             "constrainedSampling": {"type": "grammar", "variants": {"openai_lark": "start: /[a-z]+/"}}}
        ]
    });
    run_scenario(
        &mut oracle,
        Scenario::new(5, grammar_model, &text).context(grammar_context),
    )
    .await;

    let tool_events = vec![
        json!({"type": "response.output_item.added", "output_index": 0,
            "item": {"type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "edit", "arguments": ""}}),
        json!({"type": "response.function_call_arguments.delta", "output_index": 0, "delta": "{\"path\""}),
        json!({"type": "response.function_call_arguments.delta", "output_index": 0, "delta": ":\"README.md\"}"}),
        json!({"type": "response.function_call_arguments.done", "output_index": 0,
            "arguments": "{\"path\":\"README.md\"}"}),
        json!({"type": "response.output_item.done", "output_index": 0,
            "item": {"type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "edit",
                     "arguments": "{\"path\":\"README.md\"}"}}),
        json!({"type": "response.completed", "response": {"id": "resp_tool", "status": "completed"}}),
    ];
    run_scenario(
        &mut oracle,
        Scenario::new(6, base_model("openai", "gpt-5.5", true), &tool_events),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(7, base_model("openai", "gpt-5.5", true), &text).extra(json!({"serviceTier": "flex"})),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(8, base_model("openai", "gpt-5.5", true), &text)
            .stream(json!({"sessionId": "session-123", "cacheRetention": "long"})),
    )
    .await;

    let explicit_cache_model = model_with_compat("openai", "gpt-5.5", json!({"supportsExplicitPromptCacheMode": true}));
    run_scenario(
        &mut oracle,
        Scenario::new(9, explicit_cache_model, &text)
            .stream(json!({"sessionId": "session-123", "cacheRetention": "none"})),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(10, base_model("openrouter", "openai/gpt-5.5", true), &text)
            .stream(json!({"sessionId": "session-proxy"})),
    )
    .await;

    run_scenario(&mut oracle, Scenario::new(11, base_model("xai", "grok-4", true), &text)).await;

    run_scenario(
        &mut oracle,
        Scenario::new(12, base_model("openai", "gpt-5.5", true), &text).responses(vec![ScriptedResponse::json(
            400,
            r#"{"error":{"message":"bad request","type":"invalid_request_error"}}"#,
        )]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(13, base_model("openai", "gpt-5.5", true), &text)
            .responses(vec![ScriptedResponse::json(500, "server exploded")]),
    )
    .await;

    let retry_chunks = sse_chunks(&text, 11);
    run_scenario(
        &mut oracle,
        Scenario::new(14, base_model("openai", "gpt-5.5", true), &text)
            .stream(json!({"maxRetries": 1}))
            .responses(vec![
                ScriptedResponse::json(429, r#"{"error":{"message":"rate limited"}}"#)
                    .with_header("retry-after", "0.001"),
                ScriptedResponse::sse(&retry_chunks.iter().map(String::as_str).collect::<Vec<_>>()),
            ]),
    )
    .await;

    let abort_chunks = sse_chunks(&text, 5);
    run_scenario(
        &mut oracle,
        Scenario::new(15, base_model("openai", "gpt-5.5", true), &text).abort_after(abort_chunks.len().min(4)),
    )
    .await;

    let mut malformed = sse_chunks(&text, 9);
    malformed.insert(1, "data: {not json}\n\n".to_owned());
    run_scenario(
        &mut oracle,
        Scenario::new(16, base_model("openai", "gpt-5.5", true), &text).responses(vec![ScriptedResponse::sse(
            &malformed.iter().map(String::as_str).collect::<Vec<_>>(),
        )]),
    )
    .await;

    let empty_incomplete = vec![json!({
        "type": "response.incomplete",
        "response": {"status": "incomplete", "incomplete_details": {"reason": "max_output_tokens"}}
    })];
    run_scenario(
        &mut oracle,
        Scenario::new(17, base_model("openai", "gpt-5.5", true), &empty_incomplete),
    )
    .await;

    let image_context = json!({
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "look"},
                {"type": "image", "data": "AAAA", "mimeType": "image/png"}], "timestamp": 1}
        ]
    });
    run_scenario(
        &mut oracle,
        Scenario::new(18, base_model("openai", "gpt-5.5", true), &text).context(image_context),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(19, base_model("openai", "gpt-5.5", true), &text).responses(vec![
            ScriptedResponse::json(
                400,
                r#"{"error":{"message":"This model's maximum context length is 128000 tokens.","code":"context_length_exceeded"}}"#,
            ),
        ]),
    )
    .await;

    let mut legacy_model = base_model("openai", "gpt-5.5", true);
    legacy_model["compat"] = json!({"supportsOpenAIGrammarTools":true});
    run_scenario(
        &mut oracle,
        Scenario::new(20, legacy_model, &text).context(
            json!({"messages":[],"tools":[{"name":"legacy","description":"ignored metadata",
                "parameters":{"type":"object","properties":{"input":{"type":"string"}}},
                "constrainedSampling":{"type":"grammar","variants":{}}}]}),
        ),
    )
    .await;

    let _ = usage();
}

#[tokio::test]
async fn open_tool_scratch_matches_frozen_success_error_and_argument_done() {
    let mut oracle = TsOracle::spawn("openai-responses-oracle.ts", "packages/ai/src/api/openai-responses.ts");
    let opening = vec![
        json!({"type":"response.created","response":{"id":"resp_open"}}),
        json!({"type":"response.output_item.added","output_index":0,"item":{
            "type":"function_call","id":"fc_open","call_id":"call_open","name":"lookup","arguments":"{\"seed\":"}}),
        json!({"type":"response.function_call_arguments.delta","output_index":0,"delta":"1"}),
    ];
    let completed = json!({"type":"response.completed","response":{"id":"resp_open","status":"completed"}});
    let tails = [
        vec![completed.clone()],
        vec![json!({"type":"error","code":"probe","message":"open tool failed"})],
        vec![json!({"type":"response.incomplete","response":{"incomplete_details":{"reason":"content_filter"}}})],
        vec![
            json!({"type":"response.function_call_arguments.done","output_index":0,"arguments":"{\"seed\":1,\"tail\":2}"}),
            json!({"type":"response.output_item.done","output_index":0,"item":{
                "type":"function_call","id":"fc_open","call_id":"call_open","name":"lookup","arguments":""}}),
            completed.clone(),
        ],
        vec![
            json!({"type":"response.function_call_arguments.done","output_index":0,"arguments":"{\"replacement\":3}"}),
            json!({"type":"response.output_item.done","output_index":0,"item":{
                "type":"function_call","id":"fc_open","call_id":"call_open","name":"lookup","arguments":""}}),
            completed,
        ],
    ];
    for (index, tail) in tails.into_iter().enumerate() {
        let mut events = opening.clone();
        events.extend(tail);
        run_scenario(
            &mut oracle,
            Scenario::new(100 + index as i64, base_model("openai", "gpt-5.5", true), &events),
        )
        .await;
    }
}

#[tokio::test]
async fn responses_options_match_frozen_selection_empty_session_and_sdk_retry() {
    let mut source = TsOracle::spawn("openai-responses-oracle.ts", "packages/ai/src/api/openai-responses.ts");
    let text = text_events("boundary");
    run_scenario(
        &mut source,
        Scenario::new(200, base_model("openai", "gpt-5.5", true), &text)
            .extra(json!({"toolChoice":"none"}))
            .stream(json!({"samplingParams":{"seed":42,"store":true},"maxTokens":4.5})),
    )
    .await;
    run_scenario(
        &mut source,
        Scenario::new(201, base_model("openai", "gpt-5.5", true), &text).stream(json!({"sessionId":""})),
    )
    .await;
    run_scenario(
        &mut source,
        Scenario::new(202, base_model("openai", "gpt-5.5", true), &text)
            .extra(json!({"serviceTier":null,"reasoningSummary":null})),
    )
    .await;
    let chunks = sse_chunks(&text, 100);
    run_scenario(
        &mut source,
        Scenario::new(203, base_model("openai", "gpt-5.5", true), &text)
            .stream(json!({"maxRetries":1,"maxRetryDelayMs":1}))
            .responses(vec![
                ScriptedResponse::json(429, r#"{"error":{"message":"retry"}}"#).with_header("retry-after-ms", "2"),
                ScriptedResponse::sse(&chunks.iter().map(String::as_str).collect::<Vec<_>>()),
            ]),
    )
    .await;
}

#[tokio::test]
async fn response_auth_headers_and_service_tier_precedence_match_frozen() {
    let mut source = TsOracle::spawn("openai-responses-oracle.ts", "packages/ai/src/api/openai-responses.ts");
    let text = text_events("header auth");
    for (index, headers) in [
        json!({"Authorization":"Bearer caller"}),
        json!({"CF-AIG-Authorization":"gateway"}),
        json!({"Authorization":"  "}),
        json!({"Authorization":null}),
    ]
    .into_iter()
    .enumerate()
    {
        run_scenario(
            &mut source,
            Scenario::new(300 + index as i64, base_model("openai", "gpt-5.5", true), &text)
                .stream(json!({"apiKey":"","headers":headers})),
        )
        .await;
    }
    for (index, id) in ["gpt-5.5", "gpt-4.1"].into_iter().enumerate() {
        for (case, (request_tier, response_tier)) in [
            (json!("priority"), None),
            (json!("priority"), Some(json!("flex"))),
            (json!("flex"), Some(json!("default"))),
            (Value::Null, Some(json!("priority"))),
        ]
        .into_iter()
        .enumerate()
        {
            let mut events = text.clone();
            if let Some(tier) = response_tier {
                events.last_mut().unwrap()["response"]["service_tier"] = tier;
            }
            run_scenario(
                &mut source,
                Scenario::new(310 + (index * 4 + case) as i64, base_model("openai", id, true), &events)
                    .extra(json!({"serviceTier":request_tier})),
            )
            .await;
        }
    }
}
