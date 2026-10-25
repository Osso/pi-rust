#[path = "support/provider_normalize.rs"]
pub mod provider_normalize;

#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/recording_fetch.rs"]
mod recording_fetch;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use oracle::TsOracle;
use pi_ai::api::azure_openai_responses::{stream, stream_simple};
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

fn model(base_url: &str) -> Value {
    json!({
        "id": "gpt-4.1", "name": "gpt-4.1", "api": "azure-openai-responses",
        "provider": "azure-openai-responses", "baseUrl": base_url,
        "reasoning": true, "input": ["text", "image"],
        "cost": {"input": 1.0, "output": 2.0, "cacheRead": 0.5, "cacheWrite": 0.75},
        "contextWindow": 128000, "maxTokens": 4096
    })
}

fn text_events(text: &str) -> Vec<Value> {
    vec![
        json!({"type": "response.created", "response": {"id": "resp_test"}}),
        json!({"type": "response.output_item.added", "output_index": 0,
            "item": {"type": "message", "id": "msg_test"}}),
        json!({"type": "response.output_text.delta", "output_index": 0, "delta": text}),
        json!({"type": "response.output_item.done", "output_index": 0,
            "item": {"type": "message", "id": "msg_test", "content": [{"type": "output_text", "text": text}]}}),
        json!({"type": "response.completed", "response": {"id": "resp_test", "status": "completed",
            "usage": {"input_tokens": 10, "output_tokens": 5, "total_tokens": 15}}}),
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
    StreamOptions {
        request,
        temperature: stream.get("temperature").and_then(Value::as_f64),
        sampling_params: stream.get("samplingParams").and_then(Value::as_object).cloned(),
        max_tokens: stream.get("maxTokens").and_then(Value::as_f64),
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
        metadata: stream
            .get("metadata")
            .and_then(|value| pi_js_compat::js_json::JsValue::from(value.clone()).as_object().cloned()),
        extra: extra.as_object().cloned().unwrap_or_default(),
        ..Default::default()
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
    json!({"url": request.url, "method": request.method, "headers": headers, "body": body})
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
    simple: bool,
    abort_after_chunks: Option<usize>,
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
                &sse_chunks(events, 9).iter().map(String::as_str).collect::<Vec<_>>(),
            )],
            reasoning: None,
            simple: false,
            abort_after_chunks: None,
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
            "headers": response.headers.iter().cloned().collect::<BTreeMap<_, _>>(),
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
    let fetch = match (scenario.abort_after_chunks, &controller) {
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
    assert_json_eq(&actual, expected, &format!("azure scenario {}", scenario.id));
}

#[tokio::test]
async fn differential_azure_openai_responses() {
    let mut oracle = TsOracle::spawn(
        "openai-responses-oracle.ts",
        "packages/ai/src/api/azure-openai-responses.ts",
    );
    let text = text_events("Hello");

    run_scenario(
        &mut oracle,
        Scenario::new(0, model("https://example.openai.azure.com"), &text)
            .stream(json!({"sessionId": "session-123", "maxTokens": 4})),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(1, model("https://ignored.example.com"), &text).extra(json!({
            "azureBaseUrl": "https://custom.example.com/openai/v1/",
            "azureApiVersion": "2025-04-01-preview",
            "azureDeploymentName": "deployment-a",
            "reasoningEffort": "high",
            "reasoningSummary": "concise"
        })),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(2, model("https://example.openai.azure.com"), &text).stream(json!({
            "env": {
                "AZURE_OPENAI_BASE_URL": "https://env.openai.azure.com/",
                "AZURE_OPENAI_API_VERSION": "v1-env",
                "AZURE_OPENAI_DEPLOYMENT_NAME_MAP": "gpt-4.1=env-deployment"
            }
        })),
    )
    .await;

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
    run_scenario(
        &mut oracle,
        Scenario::new(3, model("https://example.openai.azure.com"), &text).context(tool_context),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(4, model("https://example.openai.azure.com"), &text).reasoning("medium"),
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
        Scenario::new(5, model("https://example.openai.azure.com"), &text).context(image_context),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(6, model("https://example.openai.azure.com"), &text).responses(vec![ScriptedResponse::json(
            400,
            r#"{"error":{"message":"bad deployment"}}"#,
        )]),
    )
    .await;

    let empty_incomplete = vec![json!({
        "type": "response.incomplete",
        "response": {"status": "incomplete", "incomplete_details": {"reason": "max_output_tokens"}}
    })];
    run_scenario(
        &mut oracle,
        Scenario::new(7, model("https://example.openai.azure.com"), &empty_incomplete),
    )
    .await;

    let retry_chunks = sse_chunks(&text, 100);
    run_scenario(
        &mut oracle,
        Scenario::new(9, model("https://example.openai.azure.com"), &text)
            .stream(json!({"maxRetries":1}))
            .responses(vec![
                ScriptedResponse::json(429, r#"{"error":{"message":"retry"}}"#).with_header("retry-after", "0"),
                ScriptedResponse::sse(&retry_chunks.iter().map(String::as_str).collect::<Vec<_>>()),
            ]),
    )
    .await;

    let mut legacy_model = model("https://example.openai.azure.com");
    legacy_model["compat"] = json!({"supportsOpenAIGrammarTools":true});
    run_scenario(
        &mut oracle,
        Scenario::new(10, legacy_model, &text).context(
            json!({"messages":[],"tools":[{"name":"legacy","description":"ignored metadata",
                "parameters":{"type":"object","properties":{"input":{"type":"string"}}},
                "constrainedSampling":{"type":"grammar","variants":{}}}]}),
        ),
    )
    .await;

    let open_tool_error = vec![
        json!({"type":"response.output_item.added","output_index":0,"item":{
            "type":"function_call","id":"fc_open","call_id":"call_open","name":"lookup","arguments":""}}),
        json!({"type":"response.function_call_arguments.delta","output_index":0,"delta":"{\"n\":1"}),
        json!({"type":"error","code":"probe","message":"open tool failed"}),
    ];
    run_scenario(
        &mut oracle,
        Scenario::new(11, model("https://example.openai.azure.com"), &open_tool_error),
    )
    .await;

    let abort_chunks = sse_chunks(&text, 6);
    run_scenario(
        &mut oracle,
        Scenario::new(8, model("https://example.openai.azure.com"), &text)
            .abort_after(abort_chunks.len().min(4))
            .responses(vec![
                ScriptedResponse::sse(&abort_chunks.iter().map(String::as_str).collect::<Vec<_>>())
                    .with_header("x-request-id", "req_azure"),
            ]),
    )
    .await;
}
