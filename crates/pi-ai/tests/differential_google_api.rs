#[path = "support/provider_normalize.rs"]
pub mod provider_normalize;

#[path = "support/google_fetch.rs"]
mod google_fetch;
#[path = "support/json_compare.rs"]
mod json_compare;
#[path = "support/oracle.rs"]
mod oracle;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use google_fetch::{RecordingFetch, ScriptedResponse};
use oracle::TsOracle;
use pi_ai::api::google_generative_ai;
use pi_ai::api::google_vertex;
use pi_ai::request::{Fetch, HttpRequest, ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{CacheRetention, Context, Model, ProviderHeaders, ThinkingBudgets};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use pi_js_compat::abort::AbortSignal;
use serde_json::{Value, json};

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures");

fn service_account_path() -> String {
    format!("{FIXTURES}/service-account.json")
}

fn authorized_user_path() -> String {
    format!("{FIXTURES}/authorized-user.json")
}

fn base_model(api: &str, provider: &str, id: &str, base_url: &str, reasoning: bool) -> Value {
    json!({
        "id": id, "name": id, "api": api, "provider": provider,
        "baseUrl": base_url,
        "reasoning": reasoning,
        "input": ["text", "image"],
        "cost": {"input": 0.3, "output": 2.5, "cacheRead": 0.03, "cacheWrite": 0.0},
        "contextWindow": 1048576, "maxTokens": 65536
    })
}

fn gemini_model(id: &str) -> Value {
    base_model(
        "google-generative-ai",
        "google",
        id,
        "https://generativelanguage.googleapis.com/v1beta",
        true,
    )
}

fn vertex_model(id: &str) -> Value {
    base_model(
        "google-vertex",
        "google-vertex",
        id,
        "https://{location}-aiplatform.googleapis.com",
        true,
    )
}

fn user_context(text: &str) -> Value {
    json!({"messages": [{"role": "user", "content": text, "timestamp": 1}]})
}

fn sse_events(events: &[Value]) -> Vec<Vec<u8>> {
    events
        .iter()
        .map(|event| format!("data: {event}\n\n").into_bytes())
        .collect()
}

fn split_payload(payload: &str, size: usize) -> Vec<Vec<u8>> {
    let bytes = payload.as_bytes();
    bytes.chunks(size.max(1)).map(<[u8]>::to_vec).collect()
}

fn text_events(text: &str) -> Vec<Value> {
    vec![json!({
        "responseId": "response-1",
        "candidates": [{"content": {"parts": [{"text": text}]}, "finishReason": "STOP"}],
        "usageMetadata": {
            "promptTokenCount": 5, "candidatesTokenCount": 2, "totalTokenCount": 7,
            "cachedContentTokenCount": 1, "thoughtsTokenCount": 3
        }
    })]
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::new();
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        encoded.push(ALPHABET[(first >> 2) as usize] as char);
        encoded.push(ALPHABET[(((first & 0b11) << 4) | (second >> 4)) as usize] as char);
        encoded.push(if chunk.len() > 1 {
            ALPHABET[(((second & 0b1111) << 2) | (third >> 6)) as usize] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            ALPHABET[(third & 0b111111) as usize] as char
        } else {
            '='
        });
    }
    encoded
}

fn base64url_decode(text: &str) -> Vec<u8> {
    let mut bits = Vec::new();
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            _ => continue,
        };
        bits.push(value);
    }
    let mut bytes = Vec::new();
    for chunk in bits.chunks(4) {
        let mut buffer = 0_u32;
        for (index, value) in chunk.iter().enumerate() {
            buffer |= u32::from(*value) << (18 - index * 6);
        }
        bytes.push((buffer >> 16) as u8);
        if chunk.len() > 2 {
            bytes.push((buffer >> 8) as u8);
        }
        if chunk.len() > 3 {
            bytes.push(buffer as u8);
        }
    }
    bytes
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
        api_key: (stream.get("apiKey").and_then(Value::as_str).map(str::to_owned))
            .map(pi_js_compat::js_json::JsString::from),
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
        session_id: stream.get("sessionId").and_then(Value::as_str).map(Into::into),
        on_retry: None,
        websocket_connect_timeout_ms: stream.get("websocketConnectTimeoutMs").and_then(Value::as_f64),
        metadata: stream
            .get("metadata")
            .and_then(|value| pi_js_compat::js_json::JsValue::from(value.clone()).as_object().cloned()),
        extra: extra_map,
    }
}

fn normalize_jwt_body(body: &str) -> (String, Option<Value>) {
    let Some(position) = body.find("assertion=") else {
        return (body.to_owned(), None);
    };
    let start = position + "assertion=".len();
    let end = body[start..]
        .find('&')
        .map(|offset| start + offset)
        .unwrap_or(body.len());
    let assertion = &body[start..end];
    let mut parts = assertion.split('.');
    let header = parts.next().unwrap_or_default();
    let claims = parts.next().unwrap_or_default();
    let signature = parts.next().unwrap_or_default();
    let header: Value = serde_json::from_slice(&base64url_decode(header)).unwrap_or(Value::Null);
    let mut claims: Value = serde_json::from_slice(&base64url_decode(claims)).unwrap_or(Value::Null);
    if let Some(claims) = claims.as_object_mut() {
        claims.insert("iat".to_owned(), json!(0));
        claims.insert("exp".to_owned(), json!(0));
    }
    let normalized = format!("{}<jwt>{}", &body[..start], &body[end..]);
    (
        normalized,
        Some(json!({
            "header": header,
            "claims": claims,
            "signatureLength": signature.len()
        })),
    )
}

fn request_value(request: &HttpRequest) -> Value {
    // The oracle records `Headers`, whose iteration order the Fetch spec defines as
    // lexicographic by name; `HttpRequest.headers` keeps construction order.
    let mut header_entries = request
        .headers
        .iter()
        .map(|(name, value)| {
            let value = value.as_str();
            (name.clone(), Value::String(value.to_owned()))
        })
        .collect::<Vec<_>>();
    header_entries.sort_by(|left, right| left.0.cmp(&right.0));
    let headers = header_entries.into_iter().collect::<serde_json::Map<_, _>>();
    let raw_body = request
        .body
        .as_deref()
        .map(|body| String::from_utf8_lossy(body).into_owned());
    let (body, jwt) = match raw_body {
        Some(body) => normalize_jwt_body(&body),
        None => (String::new(), None),
    };
    let mut value = json!({
        "method": request.method,
        "url": request.url,
        "headers": headers,
        "body": body
    });
    if let Some(jwt) = jwt {
        value["jwt"] = jwt;
    }
    value
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
                actual.keys().collect::<std::collections::BTreeSet<_>>(),
                expected.keys().collect::<std::collections::BTreeSet<_>>(),
                "object keys mismatch at {path}"
            );
            for (key, expected) in expected {
                assert_json_eq(&actual[key], expected, &format!("{path}.{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "value mismatch at {path}"),
    }
}

/// The JWT assertion is base64url-encoded into the request body, so its header
/// key order is part of the bytes sent on the wire.
fn assert_jwt_header_order(actual: &Value, expected: &Value, path: &str) {
    let actual_requests = actual["requests"].as_array().map(Vec::as_slice).unwrap_or_default();
    let expected_requests = expected["requests"].as_array().map(Vec::as_slice).unwrap_or_default();
    for (index, (actual_request, expected_request)) in actual_requests.iter().zip(expected_requests).enumerate() {
        if let (Some(actual_header), Some(expected_header)) = (
            actual_request.pointer("/jwt/header"),
            expected_request.pointer("/jwt/header"),
        ) {
            json_compare::assert_json_eq_ordered(
                actual_header,
                expected_header,
                &format!("{path}.requests[{index}].jwt.header"),
            );
        }
    }
}

fn scripted_function_call_ids(responses: &[ScriptedResponse]) -> Vec<Option<String>> {
    let mut ids = Vec::new();
    for response in responses {
        let body = response.chunks.concat();
        let text = String::from_utf8_lossy(&body);
        for line in text.lines().filter_map(|line| line.strip_prefix("data:")) {
            let Ok(event) = pi_js_compat::js_json::parse_lossless(line.trim()) else {
                continue;
            };
            append_scripted_call_ids(&event, &mut ids);
        }
    }
    ids
}

fn append_scripted_call_ids(event: &pi_js_compat::js_json::JsValue, ids: &mut Vec<Option<String>>) {
    use pi_js_compat::js_json::JsValue;
    let parts = event["candidates"][0]["content"]["parts"].as_array();
    for part in parts.into_iter().flatten() {
        if let Some(call) = part.get("functionCall") {
            ids.push(
                call.get("id")
                    .and_then(JsValue::as_string)
                    .and_then(|id| id.to_utf8().ok()),
            );
        }
    }
}

fn normalize_tool_id(block: &mut Value, supplied: Option<&String>, generated: &regex::Regex) {
    if let Some(Value::String(id)) = block.get_mut("id") {
        if supplied == Some(id) {
            return;
        }
        *id = generated.replace(id, "_<generated>").into_owned();
    }
}

fn normalize_generated_tool_ids(message: &mut Value, supplied: &[Option<String>], generated: &regex::Regex) {
    if let Some(content) = message.get_mut("content").and_then(Value::as_array_mut) {
        let calls = content
            .iter_mut()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("toolCall"));
        for (index, block) in calls.enumerate() {
            normalize_tool_id(block, supplied.get(index).and_then(Option::as_ref), generated);
        }
    }
}

fn normalize_events(capture: &mut Value, supplied: &[Option<String>]) {
    provider_normalize::normalize_capture_clocks(capture);
    let generated = regex::Regex::new(r"_1\d{12}_\d+$").expect("generated id pattern");
    if let Some(events) = capture.get_mut("events").and_then(Value::as_array_mut) {
        for event in events {
            for key in ["partial", "message", "error"] {
                if let Some(message) = event.get_mut(key) {
                    normalize_generated_tool_ids(message, supplied, &generated);
                }
            }
            let ordinal = event.get("contentIndex").and_then(Value::as_u64).and_then(|index| {
                event["partial"]["content"].as_array().map(|content| {
                    content
                        .iter()
                        .take(index as usize)
                        .filter(|block| block["type"] == "toolCall")
                        .count()
                })
            });
            if let Some(ordinal) = ordinal
                && let Some(block) = event.get_mut("toolCall")
            {
                normalize_tool_id(block, supplied.get(ordinal).and_then(Option::as_ref), &generated);
            }
        }
    }
    if let Some(message) = capture.get_mut("result") {
        normalize_generated_tool_ids(message, supplied, &generated);
    }
}

fn scenario_response(response: &ScriptedResponse) -> Value {
    let mut value = json!({
        "status": response.status,
        "statusText": "",
        "headers": response.headers.iter().cloned().collect::<BTreeMap<_, _>>(),
        "chunks": response.chunks.iter().map(|chunk| json!({"base64": base64(chunk)})).collect::<Vec<_>>()
    });
    if let Some((name, message)) = &response.throw {
        value["throw"] = json!({"name": name, "message": message});
    }
    value
}

struct Scenario {
    id: u32,
    api: &'static str,
    simple: bool,
    model: Value,
    context: Value,
    stream: Value,
    extra: Value,
    reasoning: Option<String>,
    thinking_budgets: Option<Value>,
    responses: Vec<ScriptedResponse>,
    token_responses: Vec<ScriptedResponse>,
    abort_after_chunks: Option<usize>,
    pre_aborted: bool,
    on_payload_patch: Option<Value>,
}

impl Scenario {
    fn new(id: u32, api: &'static str, model: Value) -> Self {
        Self {
            id,
            api,
            simple: false,
            model,
            context: user_context("hello"),
            stream: json!({"apiKey": "test-key"}),
            extra: json!({}),
            reasoning: None,
            thinking_budgets: None,
            responses: vec![ScriptedResponse::sse(&[])].into_iter().collect(),
            token_responses: Vec::new(),
            abort_after_chunks: None,
            pre_aborted: false,
            on_payload_patch: None,
        }
    }

    fn simple(mut self) -> Self {
        self.simple = true;
        self.stream = json!({"apiKey": "test-key"});
        self
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

    fn reasoning(mut self, reasoning: &str) -> Self {
        self.reasoning = Some(reasoning.to_owned());
        self.simple = true;
        self
    }

    fn thinking_budgets(mut self, budgets: Value) -> Self {
        self.thinking_budgets = Some(budgets);
        self
    }

    fn responses(mut self, responses: Vec<ScriptedResponse>) -> Self {
        self.responses = responses;
        self
    }

    fn token_responses(mut self, responses: Vec<ScriptedResponse>) -> Self {
        self.token_responses = responses;
        self
    }

    fn abort_after(mut self, chunks: usize) -> Self {
        self.abort_after_chunks = Some(chunks);
        self
    }

    fn pre_aborted(mut self) -> Self {
        self.pre_aborted = true;
        self
    }

    fn on_payload_patch(mut self, patch: Value) -> Self {
        self.on_payload_patch = Some(patch);
        self
    }

    fn request(&self) -> Value {
        let mut request = json!({
            "id": self.id,
            "api": self.api,
            "operation": if self.simple { "streamSimple" } else { "stream" },
            "model": self.model,
            "context": self.context,
            "stream": self.stream,
            "extra": self.extra,
            "responses": self.responses.iter().map(scenario_response).collect::<Vec<_>>(),
            "tokenResponses": self.token_responses.iter().map(scenario_response).collect::<Vec<_>>(),
        });
        if let Some(reasoning) = &self.reasoning {
            request["reasoning"] = Value::String(reasoning.clone());
        }
        if let Some(budgets) = &self.thinking_budgets {
            request["thinkingBudgets"] = budgets.clone();
        }
        if let Some(chunks) = self.abort_after_chunks {
            request["abortAfterChunks"] = json!(chunks);
        }
        if self.pre_aborted {
            request["preAborted"] = Value::Bool(true);
        }
        if let Some(patch) = &self.on_payload_patch {
            request["onPayloadPatch"] = patch.clone();
        }
        request
    }
}

async fn collect_stream(mut stream: AssistantMessageEventStream) -> (Vec<Value>, Value) {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(serde_json::to_value(event.expect("stream events are not failures")).expect("event serializes"));
    }
    let result = stream.result().await.expect("stream result");
    (events, serde_json::to_value(result).expect("result serializes"))
}

fn normalize_sdk_runtime_versions(capture: &mut Value, model: &Value, stream: &Value) {
    let configured: BTreeSet<_> = [model.get("headers"), stream.get("headers")]
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .flat_map(|headers| headers.keys().map(|key| key.to_ascii_lowercase()))
        .collect();
    let Some(requests) = capture.get_mut("requests").and_then(Value::as_array_mut) else {
        return;
    };
    for request in requests {
        for name in ["user-agent", "x-goog-api-client"] {
            if configured.contains(name) {
                continue;
            }
            if let Some(Value::String(value)) = request.get_mut("headers").and_then(|headers| headers.get_mut(name)) {
                *value = value
                    .split(' ')
                    .map(|component| {
                        if component.starts_with("gl-node/") {
                            "gl-node/<host-version>"
                        } else {
                            component
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
            }
        }
    }
}

#[test]
fn runtime_normalization_preserves_sdk_identity_and_explicit_headers() {
    let mut captured = json!({"requests":[{"headers":{
        "user-agent":"google-genai-sdk/1.52.0 gl-node/v26.11.1",
        "x-goog-api-client":"google-genai-sdk/9.9.9 gl-rust/1 gl-node/v26.11.1"
    }}]});
    normalize_sdk_runtime_versions(&mut captured, &json!({}), &json!({}));
    assert_eq!(
        captured["requests"][0]["headers"]["user-agent"],
        "google-genai-sdk/1.52.0 gl-node/<host-version>"
    );
    assert_eq!(
        captured["requests"][0]["headers"]["x-goog-api-client"],
        "google-genai-sdk/9.9.9 gl-rust/1 gl-node/<host-version>"
    );
    let mut explicit = json!({"requests":[{"headers":{"user-agent":"custom gl-node/v26.11.1"}}]});
    let original = explicit.clone();
    normalize_sdk_runtime_versions(
        &mut explicit,
        &json!({"headers":{"User-Agent":"custom gl-node/v26.11.1"}}),
        &json!({}),
    );
    assert_eq!(explicit, original);
}

async fn run_scenario(oracle: &mut TsOracle, scenario: Scenario) {
    let request = scenario.request();
    let expected = oracle.request(&request);
    assert_eq!(expected["ok"], Value::Bool(true), "oracle failed: {expected}");
    let mut expected = expected["output"].clone();
    let supplied = scripted_function_call_ids(&scenario.responses);
    normalize_events(&mut expected, &supplied);
    normalize_sdk_runtime_versions(&mut expected, &scenario.model, &scenario.stream);

    let model: Model = serde_json::from_value(scenario.model.clone()).expect("model");
    let context: Context = serde_json::from_value(scenario.context.clone()).expect("context");
    let controller =
        (scenario.abort_after_chunks.is_some() || scenario.pre_aborted).then(pi_js_compat::abort::AbortController::new);
    let signal = controller.as_ref().map(|controller| controller.signal());
    let mut responses = scenario.token_responses.clone();
    responses.extend(scenario.responses.clone());
    let fetch: Arc<RecordingFetch> = match (scenario.abort_after_chunks, &controller) {
        (Some(chunks), Some(controller)) => {
            RecordingFetch::with_abort_after_chunks(responses, chunks, controller.clone())
        }
        _ => RecordingFetch::new(responses),
    };
    if scenario.pre_aborted
        && let Some(controller) = &controller
    {
        controller.abort(None);
    }
    let options = stream_options(&scenario.stream, &scenario.extra, fetch.clone(), signal);
    let mut options = options;
    if let Some(patch) = scenario.on_payload_patch {
        let patch = Arc::new(patch);
        let patch = Arc::clone(&patch);
        options.request.on_payload = Some(Arc::new(move |_payload, _model| {
            let patch = Arc::clone(&patch);
            Box::pin(async move { Ok(Some((*patch).clone().into())) })
        }));
    }

    let stream = if scenario.simple {
        let reasoning = scenario
            .reasoning
            .as_deref()
            .and_then(|reasoning| serde_json::from_value(Value::String(reasoning.to_owned())).ok());
        let thinking_budgets = scenario
            .thinking_budgets
            .as_ref()
            .and_then(|budgets| serde_json::from_value::<ThinkingBudgets>(budgets.clone()).ok());
        let simple = SimpleStreamOptions {
            stream: options,
            reasoning,
            thinking_budgets,
            ..Default::default()
        };
        if scenario.api == "google-vertex" {
            google_vertex::stream_simple(model, context, simple)
        } else {
            google_generative_ai::stream_simple(model, context, simple)
        }
    } else if scenario.api == "google-vertex" {
        google_vertex::stream(model, context, options)
    } else {
        google_generative_ai::stream(model, context, options)
    };
    let (events, result) = collect_stream(stream).await;
    let requests = fetch.requests().iter().map(request_value).collect::<Vec<_>>();
    let mut actual = json!({"requests": requests, "events": events, "result": result});
    normalize_events(&mut actual, &supplied);
    normalize_sdk_runtime_versions(&mut actual, &scenario.model, &scenario.stream);
    assert_json_eq(&actual, &expected, &format!("google scenario {}", scenario.id));
    assert_jwt_header_order(&actual, &expected, &format!("google scenario {}", scenario.id));
}

fn tool(name: &str, strict: bool) -> Value {
    let mut tool = json!({
        "name": name,
        "description": "A tool",
        "parameters": {"type": "object", "properties": {"command": {"type": "string"}}, "required": ["command"]}
    });
    if strict {
        tool["constrainedSampling"] = json!({"type": "json_schema", "strict": "require"});
    }
    tool
}

fn sse_response(chunks: Vec<Vec<u8>>) -> ScriptedResponse {
    ScriptedResponse {
        status: 200,
        headers: vec![("content-type".to_owned(), "text/event-stream".to_owned())],
        chunks,
        throw: None,
    }
}

fn sse_events_response(events: &[Value]) -> ScriptedResponse {
    sse_response(sse_events(events))
}

fn token_response(token: &str) -> ScriptedResponse {
    ScriptedResponse::json(
        200,
        &json!({"access_token": token, "expires_in": 3600, "token_type": "Bearer"}).to_string(),
    )
}

#[tokio::test]
async fn differential_google_generative_ai() {
    let mut oracle = TsOracle::spawn("google-api-oracle.ts", "packages/ai/src/api");

    let text = text_events("Hello");
    run_scenario(
        &mut oracle,
        Scenario::new(0, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .stream(json!({
                "apiKey": "test-key", "temperature": 0.2, "maxTokens": 100,
                "headers": {"x-custom": "v"}
            }))
            .context(json!({
                "systemPrompt": "Be nice",
                "messages": [{"role": "user", "content": "hello", "timestamp": 1}]
            }))
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(1, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .simple()
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    for (id, reasoning) in [(2, "minimal"), (3, "low"), (4, "medium"), (5, "high")] {
        run_scenario(
            &mut oracle,
            Scenario::new(id, "google-generative-ai", gemini_model("gemini-2.5-flash"))
                .reasoning(reasoning)
                .responses(vec![sse_events_response(&text)]),
        )
        .await;
    }

    for (id, reasoning) in [(6, "minimal"), (7, "high")] {
        run_scenario(
            &mut oracle,
            Scenario::new(id, "google-generative-ai", gemini_model("gemini-2.5-flash-lite"))
                .reasoning(reasoning)
                .responses(vec![sse_events_response(&text)]),
        )
        .await;
    }

    run_scenario(
        &mut oracle,
        Scenario::new(8, "google-generative-ai", gemini_model("gemini-2.5-pro"))
            .reasoning("high")
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(9, "google-generative-ai", gemini_model("gemini-3.1-pro-preview"))
            .reasoning("medium")
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(10, "google-generative-ai", gemini_model("gemini-3.1-pro-preview"))
            .reasoning("minimal")
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(11, "google-generative-ai", gemini_model("gemini-3-flash-preview"))
            .reasoning("low")
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(12, "google-generative-ai", gemini_model("gemma-4-26b-a4b-it"))
            .reasoning("high")
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(13, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .reasoning("low")
            .thinking_budgets(json!({"low": 777}))
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(14, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .extra(json!({"thinking": {"enabled": true, "level": "LOW"}}))
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(15, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .extra(json!({"thinking": {"enabled": true, "budgetTokens": -1}}))
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    for (id, model_id) in [
        (16, "gemini-2.5-flash"),
        (17, "gemini-3.1-pro-preview"),
        (18, "gemini-3-flash-preview"),
        (19, "gemma-4-26b-a4b-it"),
    ] {
        run_scenario(
            &mut oracle,
            Scenario::new(id, "google-generative-ai", gemini_model(model_id))
                .extra(json!({"thinking": {"enabled": false}}))
                .responses(vec![sse_events_response(&text)]),
        )
        .await;
    }

    let thought_events = vec![
        json!({"responseId": "r1", "candidates": [{"content": {"parts": [
            {"text": "think ", "thought": true, "thoughtSignature": "SIG1"}]}}]}),
        json!({"candidates": [{"content": {"parts": [{"text": "more", "thought": true}]}}]}),
        json!({"candidates": [{"content": {"parts": [
            {"text": "answer", "thoughtSignature": "SIG2"}]}}]}),
        json!({"candidates": [{"content": {"parts": [{"text": " end"}]}, "finishReason": "STOP"}]}),
    ];
    run_scenario(
        &mut oracle,
        Scenario::new(20, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .responses(vec![sse_events_response(&thought_events)]),
    )
    .await;

    let tool_events = vec![json!({"candidates": [{"content": {"parts": [
        {"text": "calling"},
        {"functionCall": {"id": "call_1", "name": "bash", "args": {"command": "ls"}}},
        {"functionCall": {"name": "bash", "args": {"command": "pwd"}}}
    ]}, "finishReason": "STOP"}]})];
    run_scenario(
        &mut oracle,
        Scenario::new(21, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .responses(vec![sse_events_response(&tool_events)]),
    )
    .await;

    let duplicate_id_events = vec![
        json!({"candidates": [{"content": {"parts": [
            {"functionCall": {"id": "call_1", "name": "bash", "args": {"command": "ls"}}}]}}]}),
        json!({"candidates": [{"content": {"parts": [
            {"functionCall": {"id": "call_1", "name": "bash", "args": {"command": "pwd"}}}]},
            "finishReason": "STOP"}]}),
    ];
    run_scenario(
        &mut oracle,
        Scenario::new(22, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .responses(vec![sse_events_response(&duplicate_id_events)]),
    )
    .await;

    let tool_context = json!({
        "messages": [{"role": "user", "content": "Use a tool.", "timestamp": 1}],
        "tools": [tool("plain", false), tool("strict", true)]
    });
    run_scenario(
        &mut oracle,
        Scenario::new(23, "google-generative-ai", gemini_model("gemini-3.1-pro-preview"))
            .context(tool_context.clone())
            .extra(json!({"toolChoice": "auto"}))
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(24, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .context(tool_context.clone())
            .extra(json!({"toolChoice": "none"}))
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    let image_context = json!({
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "look"},
                {"type": "image", "data": "abc", "mimeType": "image/png"}], "timestamp": 1},
            {"role": "assistant", "content": [
                {"type": "toolCall", "id": "call_1", "name": "read", "arguments": {"path": "a"}},
                {"type": "toolCall", "id": "call_2", "name": "read", "arguments": {"path": "b"}}
            ], "api": "google-generative-ai", "provider": "google", "model": "gemini-2.5-flash",
             "usage": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
                "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}},
             "stopReason": "toolUse", "timestamp": 2},
            {"role": "toolResult", "toolCallId": "call_1", "toolName": "read",
                "content": [{"type": "text", "text": "alpha"}], "isError": false, "timestamp": 3},
            {"role": "toolResult", "toolCallId": "call_2", "toolName": "read",
                "content": [{"type": "image", "data": "img", "mimeType": "image/png"}],
                "isError": false, "timestamp": 4}
        ]
    });
    run_scenario(
        &mut oracle,
        Scenario::new(25, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .context(image_context.clone())
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    let mut gemini3_image_context = image_context.clone();
    gemini3_image_context["messages"][1]["model"] = json!("gemini-3-pro-preview");
    run_scenario(
        &mut oracle,
        Scenario::new(26, "google-generative-ai", gemini_model("gemini-3-pro-preview"))
            .context(gemini3_image_context)
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    let usage_events = vec![
        json!({"candidates": [{"content": {"parts": [{"text": "a"}]}}],
            "usageMetadata": {"promptTokenCount": 10, "cachedContentTokenCount": 4,
                "candidatesTokenCount": 1, "thoughtsTokenCount": 2, "totalTokenCount": 13}}),
        json!({"candidates": [{"content": {"parts": [{"text": "b"}]}, "finishReason": "STOP"}],
            "usageMetadata": {"promptTokenCount": 10, "cachedContentTokenCount": 4,
                "candidatesTokenCount": 5, "thoughtsTokenCount": 2, "totalTokenCount": 17}}),
    ];
    run_scenario(
        &mut oracle,
        Scenario::new(27, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .responses(vec![sse_events_response(&usage_events)]),
    )
    .await;

    let length_events = vec![json!({"candidates": [{"content": {"parts": [{"text": "x"}]},
        "finishReason": "MAX_TOKENS"}]})];
    run_scenario(
        &mut oracle,
        Scenario::new(28, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .responses(vec![sse_events_response(&length_events)]),
    )
    .await;

    for (id, reason) in [(29, "SAFETY"), (30, "MALFORMED_FUNCTION_CALL")] {
        let events = vec![json!({"candidates": [{"finishReason": reason}]})];
        run_scenario(
            &mut oracle,
            Scenario::new(id, "google-generative-ai", gemini_model("gemini-2.5-flash"))
                .responses(vec![sse_events_response(&events)]),
        )
        .await;
    }

    let unknown_reason = vec![json!({"candidates": [{"finishReason": "NOVEL"}]})];
    run_scenario(
        &mut oracle,
        Scenario::new(31, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .responses(vec![sse_events_response(&unknown_reason)]),
    )
    .await;

    let no_reason = vec![json!({"candidates": [{"content": {"parts": [{"text": "x"}]}}]})];
    run_scenario(
        &mut oracle,
        Scenario::new(32, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .responses(vec![sse_events_response(&no_reason)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(33, "google-generative-ai", gemini_model("gemini-2.5-flash")).responses(vec![
            ScriptedResponse::json(
                400,
                r#"{"error":{"code":400,"message":"bad request","status":"INVALID_ARGUMENT"}}"#,
            ),
        ]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(34, "google-generative-ai", gemini_model("gemini-2.5-flash")).responses(vec![ScriptedResponse {
            status: 500,
            headers: vec![("content-type".to_owned(), "text/plain".to_owned())],
            chunks: vec![b"server exploded".to_vec()],
            throw: None,
        }]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(35, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .stream(json!({"apiKey": "test-key", "maxRetries": 1}))
            .responses(vec![
                ScriptedResponse::json(429, r#"{"error":{"message":"rate limited"}}"#)
                    .with_header("retry-after", "0.001"),
                sse_events_response(&text),
            ]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(36, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .stream(json!({"apiKey": "test-key", "maxRetries": 2}))
            .responses(vec![ScriptedResponse::throw("TypeError", "fetch failed")]),
    )
    .await;

    let abort_events = vec![
        json!({"candidates": [{"content": {"parts": [{"text": "partial"}]}}]}),
        json!({"candidates": [{"content": {"parts": [{"text": " more"}]}, "finishReason": "STOP"}]}),
    ];
    run_scenario(
        &mut oracle,
        Scenario::new(37, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .abort_after(1)
            .responses(vec![sse_events_response(&abort_events)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(38, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .pre_aborted()
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(39, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .responses(vec![sse_response(vec![b"data: {\"candidates\":".to_vec()])]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(40, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .responses(vec![sse_response(vec![b"data: {oops}\n\n".to_vec()])]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(41, "google-generative-ai", gemini_model("gemini-2.5-flash")).responses(vec![sse_response(
            vec![br#"{"error":{"code":429,"status":"RESOURCE_EXHAUSTED","message":"quota"}}"#.to_vec()],
        )]),
    )
    .await;

    let crlf_payload = format!(
        "event: ping\r\n\r\ndata: {}\r\n\r\n",
        json!({"candidates": [{"content": {"parts": [{"text": "ok"}]}, "finishReason": "STOP"}]})
    );
    run_scenario(
        &mut oracle,
        Scenario::new(42, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .responses(vec![sse_response(vec![crlf_payload.into_bytes()])]),
    )
    .await;

    let mut awkward_payload = String::new();
    for event in &thought_events {
        awkward_payload.push_str(&format!("data: {event}\n\n"));
    }
    awkward_payload = awkward_payload.replace("answer", "héllo 😀");
    run_scenario(
        &mut oracle,
        Scenario::new(43, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .responses(vec![sse_response(split_payload(&awkward_payload, 7))]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(44, "google-generative-ai", gemini_model("gemini-2.5-flash"))
            .on_payload_patch(json!({
                "model": "gemini-2.5-flash",
                "contents": [{"parts": [{"text": "patched"}], "role": "user"}],
                "config": {"temperature": 0.9}
            }))
            .responses(vec![sse_events_response(&text)]),
    )
    .await;
}

#[tokio::test]
async fn supplied_tool_ids_that_look_generated_remain_exact() {
    let id = "caller_1234567890123_7";
    let events = vec![json!({"candidates":[{"content":{"parts":[{"functionCall":{
        "id":id,"name":"lookup","args":{"id":"nested_1234567890123_7"}
    }}]},"finishReason":"STOP"}]})];
    let scenario = Scenario::new(200, "google-generative-ai", gemini_model("gemini-2.5-flash"))
        .responses(vec![sse_events_response(&events)]);
    let mut oracle = TsOracle::spawn("google-api-oracle.ts", "packages/ai/src/api");
    let captured = oracle.request(&scenario.request());
    assert_eq!(
        captured["output"]["result"]["content"][0]["id"], id,
        "source capture must not rewrite supplied IDs"
    );
    run_scenario(&mut oracle, scenario).await;
}

#[tokio::test]
async fn differential_google_vertex() {
    let mut oracle = TsOracle::spawn_with_environment(
        "google-api-oracle.ts",
        "packages/ai/src/api",
        &[("NODE_TLS_REJECT_UNAUTHORIZED", "0".into())],
    );

    let text = text_events("Hello");
    let service_account = service_account_path();
    let authorized_user = authorized_user_path();

    run_scenario(
        &mut oracle,
        Scenario::new(100, "google-vertex", vertex_model("gemini-3-flash-preview"))
            .stream(json!({"apiKey": "AIzaSyRealKey"}))
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    let mut custom_model = vertex_model("gemini-3-flash-preview");
    custom_model["baseUrl"] = json!("https://proxy.example.com");
    run_scenario(
        &mut oracle,
        Scenario::new(101, "google-vertex", custom_model.clone())
            .stream(json!({"apiKey": "AIzaSyRealKey"}))
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    let mut versioned_model = vertex_model("gemini-3-flash-preview");
    versioned_model["baseUrl"] = json!("https://proxy.example.com/v1/projects/test-project/locations/global");
    run_scenario(
        &mut oracle,
        Scenario::new(102, "google-vertex", versioned_model)
            .stream(json!({"apiKey": "AIzaSyRealKey"}))
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(103, "google-vertex", vertex_model("gemini-3-flash-preview"))
            .stream(json!({
                "project": "test-project", "location": "us-central1",
                "env": {"GOOGLE_APPLICATION_CREDENTIALS": service_account}
            }))
            .token_responses(vec![token_response("test-token")])
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(104, "google-vertex", vertex_model("gemini-3-flash-preview"))
            .stream(json!({
                "project": "test-project", "location": "us-central1",
                "env": {"GOOGLE_APPLICATION_CREDENTIALS": authorized_user}
            }))
            .token_responses(vec![token_response("user-token")])
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    for (id, location, expected) in [
        (105, "global", "https://aiplatform.googleapis.com"),
        (106, "us", "https://aiplatform.us.rep.googleapis.com"),
        (107, "europe-west4", "https://europe-west4-aiplatform.googleapis.com"),
    ] {
        run_scenario(
            &mut oracle,
            Scenario::new(id, "google-vertex", vertex_model("gemini-2.5-flash"))
                .stream(json!({
                    "project": "test-project", "location": location,
                    "env": {"GOOGLE_APPLICATION_CREDENTIALS": service_account}
                }))
                .token_responses(vec![token_response("test-token")])
                .responses(vec![sse_events_response(&text)]),
        )
        .await;
        let _ = expected;
    }

    run_scenario(
        &mut oracle,
        Scenario::new(108, "google-vertex", custom_model)
            .stream(json!({
                "project": "test-project", "location": "us-central1",
                "env": {"GOOGLE_APPLICATION_CREDENTIALS": service_account}
            }))
            .token_responses(vec![token_response("test-token")])
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(109, "google-vertex", vertex_model("gemini-3-flash-preview"))
            .stream(json!({
                "apiKey": "<authenticated>", "project": "test-project", "location": "us-central1",
                "env": {"GOOGLE_APPLICATION_CREDENTIALS": service_account}
            }))
            .token_responses(vec![token_response("test-token")])
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(110, "google-vertex", vertex_model("gemini-3-flash-preview"))
            .stream(json!({
                "apiKey": "gcp-vertex-credentials", "project": "test-project", "location": "us-central1",
                "env": {"GOOGLE_APPLICATION_CREDENTIALS": service_account}
            }))
            .token_responses(vec![token_response("test-token")])
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(111, "google-vertex", vertex_model("gemini-3-flash-preview"))
            .reasoning("high")
            .stream(json!({
                "env": {
                    "GOOGLE_APPLICATION_CREDENTIALS": service_account,
                    "GOOGLE_CLOUD_PROJECT": "test-project",
                    "GOOGLE_CLOUD_LOCATION": "us-central1"
                }
            }))
            .token_responses(vec![token_response("test-token")])
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(112, "google-vertex", vertex_model("gemini-3-flash-preview"))
            .reasoning("high")
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(113, "google-vertex", vertex_model("gemini-3-flash-preview"))
            .stream(json!({"apiKey": "AIzaSyRealKey"}))
            .extra(json!({"thinking": {"enabled": false}}))
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(114, "google-vertex", vertex_model("gemini-3.1-pro-preview"))
            .stream(json!({"apiKey": "AIzaSyRealKey"}))
            .extra(json!({"thinking": {"enabled": true, "level": "MEDIUM"}}))
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(115, "google-vertex", vertex_model("gemini-2.5-flash"))
            .stream(json!({"apiKey": "AIzaSyRealKey"}))
            .extra(json!({"thinking": {"enabled": true, "budgetTokens": 1024}}))
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    let tool_context = json!({
        "messages": [{"role": "user", "content": "Use a tool.", "timestamp": 1}],
        "tools": [tool("strict", true)]
    });
    run_scenario(
        &mut oracle,
        Scenario::new(116, "google-vertex", vertex_model("gemini-3.1-pro-preview"))
            .stream(json!({"apiKey": "AIzaSyRealKey"}))
            .context(tool_context)
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    let image_context = json!({
        "messages": [
            {"role": "user", "content": "read", "timestamp": 1},
            {"role": "assistant", "content": [
                {"type": "toolCall", "id": "call_1", "name": "read", "arguments": {"path": "a"}}
            ], "api": "google-vertex", "provider": "google-vertex", "model": "gemini-3-flash-preview",
             "usage": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 0,
                "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}},
             "stopReason": "toolUse", "timestamp": 2},
            {"role": "toolResult", "toolCallId": "call_1", "toolName": "read",
                "content": [{"type": "image", "data": "img", "mimeType": "image/png"}],
                "isError": false, "timestamp": 3}
        ]
    });
    run_scenario(
        &mut oracle,
        Scenario::new(117, "google-vertex", vertex_model("gemini-3-flash-preview"))
            .stream(json!({"apiKey": "AIzaSyRealKey"}))
            .context(image_context)
            .responses(vec![sse_events_response(&text)]),
    )
    .await;

    let safety_events = vec![json!({"candidates": [{"finishReason": "SAFETY"}]})];
    run_scenario(
        &mut oracle,
        Scenario::new(118, "google-vertex", vertex_model("gemini-3-flash-preview"))
            .stream(json!({"apiKey": "AIzaSyRealKey"}))
            .responses(vec![sse_events_response(&safety_events)]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(119, "google-vertex", vertex_model("gemini-3-flash-preview"))
            .stream(json!({"apiKey": "AIzaSyRealKey", "maxRetries": 1}))
            .responses(vec![
                ScriptedResponse::json(500, r#"{"error":{"message":"boom"}}"#).with_header("retry-after", "0.001"),
                sse_events_response(&text),
            ]),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(120, "google-vertex", vertex_model("gemini-3-flash-preview"))
            .stream(json!({
                "project": "test-project", "location": "us-central1", "maxRetries": 1,
                "env": {"GOOGLE_APPLICATION_CREDENTIALS": service_account}
            }))
            .token_responses(vec![token_response("test-token")])
            .responses(vec![
                ScriptedResponse::json(500, r#"{"error":{"message":"boom"}}"#).with_header("retry-after", "0.001"),
                sse_events_response(&text),
            ]),
    )
    .await;
}
