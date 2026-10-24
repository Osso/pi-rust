#[path = "support/provider_normalize.rs"]
pub mod provider_normalize;

use std::sync::Arc;

#[path = "support/abort_reason.rs"]
mod abort_reason;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/recording_fetch.rs"]
mod recording_fetch;
#[path = "support/ws_server.rs"]
mod ws_server;

use oracle::TsOracle;
use pi_ai::api::openai_codex_responses::{
    OpenAICodexResponsesOptions, close_openai_codex_websocket_sessions, reset_openai_codex_websocket_debug_stats,
    stream,
};
use pi_ai::request::{
    BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse, ProviderRequestOptions, StreamOptions,
};
use pi_ai::types::{Context, Model, Transport};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use recording_fetch::{RecordingFetch, ScriptedResponse};
use serde_json::{Value, json};
use ws_server::{ServerScript, completed_event, start_ws_server};

const SOURCE_MODULE: &str = "packages/ai/src";
const SESSION_ID: &str = "diff-session";

fn mock_token() -> String {
    let payload = json!({"https://api.openai.com/auth": {"chatgpt_account_id": "acc_test"}});
    let encoded = pi_js_compat::base64::btoa(payload.to_string().as_bytes());
    format!("aaa.{encoded}.bbb")
}

fn model(base_url: &str) -> Model {
    serde_json::from_value(json!({
        "id": "gpt-5.1-codex",
        "name": "GPT-5.1 Codex",
        "api": "openai-codex-responses",
        "provider": "openai-codex",
        "baseUrl": base_url,
        "reasoning": true,
        "input": ["text"],
        "cost": {"input": 1.0, "output": 2.0, "cacheRead": 0.5, "cacheWrite": 0.25},
        "contextWindow": 400000,
        "maxTokens": 128000
    }))
    .expect("model")
}

fn context() -> Context {
    serde_json::from_value(json!({
        "systemPrompt": "You are a helpful assistant.",
        "messages": [{"role": "user", "content": "Say hello", "timestamp": 1}]
    }))
    .expect("context")
}

fn options(
    fetch: Arc<dyn Fetch>,
    transport: Transport,
    session_id: Option<&str>,
    timeout_ms: Option<f64>,
) -> OpenAICodexResponsesOptions {
    OpenAICodexResponsesOptions {
        stream: StreamOptions {
            request: ProviderRequestOptions {
                api_key: Some((mock_token()).into()),
                fetch: Some(fetch),
                timeout_ms,
                ..ProviderRequestOptions::default()
            },
            transport: Some(transport),
            session_id: session_id.map(Into::into),
            ..StreamOptions::default()
        },
        ..OpenAICodexResponsesOptions::default()
    }
}

async fn collect(mut stream: AssistantMessageEventStream) -> (Vec<Value>, Value) {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(normalize_event(
            &serde_json::to_value(event.expect("event")).expect("event json"),
        ));
    }
    let result = normalize(&serde_json::to_value(stream.result().await.expect("result")).expect("result json"));
    (events, result)
}

fn assert_events(actual: &[Value], expected: &Value, label: &str) {
    assert_eq!(Value::Array(actual.to_vec()), *expected, "events mismatch for {label}");
}

fn normalize_event(value: &Value) -> Value {
    let mut normalized = value.clone();
    provider_normalize::normalize_event_clocks(&mut normalized);
    normalized
}

fn normalize(value: &Value) -> Value {
    let mut normalized = value.clone();
    provider_normalize::normalize_message_clock(&mut normalized);
    normalized
}

fn sse_payload(events: &[Value]) -> String {
    events.iter().map(|event| format!("data: {event}\n\n")).collect()
}

struct CaptureSignalFetch {
    inner: Arc<dyn Fetch>,
    signals: std::sync::Mutex<Vec<Option<pi_js_compat::abort::AbortSignal>>>,
}
impl Fetch for CaptureSignalFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<pi_js_compat::abort::AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.signals.lock().unwrap().push(signal.clone());
        self.inner.fetch(request, signal)
    }
}
impl CaptureSignalFetch {
    fn snapshots(&self, caller: Option<&pi_js_compat::abort::AbortSignal>) -> pi_js_compat::js_json::JsValue {
        pi_js_compat::js_json::JsValue::Array(self.signals.lock().unwrap().iter().map(|signal| {
            let same = caller.zip(signal.as_ref()).is_some_and(|(caller,signal)|caller==signal);
            let state = match signal {
                Some(signal) => pi_js_compat::js_json!({"aborted":signal.aborted(),"reason":abort_reason::describe(signal.reason())}),
                None => pi_js_compat::js_json!({"kind":"no-signal"}),
            };
            pi_js_compat::js_json!({"sameAsCaller":same,"state":state})
        }).collect())
    }
}

struct PendingBodyFetch {
    payload: Vec<u8>,
}

impl PendingBodyFetch {
    fn new(payload: Vec<u8>) -> Arc<Self> {
        Arc::new(Self { payload })
    }
}

impl Fetch for PendingBodyFetch {
    fn fetch(
        &self,
        _request: HttpRequest,
        _signal: Option<pi_js_compat::abort::AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        let payload = self.payload.clone();
        Box::pin(async move {
            Ok(HttpResponse {
                status: 200,
                status_text: String::new(),
                headers: vec![("content-type".to_owned(), "text/event-stream".to_owned())],
                body: Box::new(PendingBody { payload: Some(payload) }),
            })
        })
    }
}

struct PendingBody {
    payload: Option<Vec<u8>>,
}

impl HttpBody for PendingBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        if let Some(payload) = self.payload.take() {
            return Box::pin(async move { Some(Ok(payload)) });
        }
        Box::pin(async { std::future::pending().await })
    }
}

struct PendingHeaderFetch;

impl Fetch for PendingHeaderFetch {
    fn fetch(
        &self,
        _request: HttpRequest,
        _signal: Option<pi_js_compat::abort::AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async { std::future::pending().await })
    }
}

fn text_events() -> Vec<Value> {
    vec![
        json!({"type": "response.output_item.added", "output_index": 0,
            "item": {"type": "message", "id": "msg_1", "role": "assistant", "status": "in_progress", "content": []}}),
        json!({"type": "response.content_part.added", "output_index": 0, "part": {"type": "output_text", "text": ""}}),
        json!({"type": "response.output_text.delta", "output_index": 0, "delta": "Hello"}),
        json!({"type": "response.output_item.done", "output_index": 0,
            "item": {"type": "message", "id": "msg_1", "role": "assistant", "status": "completed",
                "content": [{"type": "output_text", "text": "Hello"}]}}),
        json!({"type": "response.completed", "response": {"status": "completed", "end_turn": false,
            "usage": {"input_tokens": 5, "output_tokens": 3, "total_tokens": 8,
                "input_tokens_details": {"cached_tokens": 0}}}}),
    ]
}

fn reasoning_events() -> Vec<Value> {
    vec![
        json!({"type": "response.output_item.added", "output_index": 0,
            "item": {"type": "reasoning", "id": "rs_1", "summary": []}}),
        json!({"type": "response.reasoning_summary_text.delta", "output_index": 0, "delta": "think"}),
        json!({"type": "response.output_item.done", "output_index": 0,
            "item": {"type": "reasoning", "id": "rs_1", "summary": [{"type": "summary_text", "text": "think"}],
                "encrypted_content": "enc"}}),
        json!({"type": "response.completed", "response": {"status": "completed",
            "usage": {"input_tokens": 5, "output_tokens": 3, "total_tokens": 8}}}),
    ]
}

fn tool_call_events() -> Vec<Value> {
    vec![
        json!({"type": "response.output_item.added", "output_index": 0,
            "item": {"type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "lookup", "arguments": ""}}),
        json!({"type": "response.function_call_arguments.delta", "output_index": 0, "delta": "{\"x\":1}"}),
        json!({"type": "response.output_item.done", "output_index": 0,
            "item": {"type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "lookup",
                "arguments": "{\"x\":1}"}}),
        json!({"type": "response.completed", "response": {"status": "completed",
            "usage": {"input_tokens": 5, "output_tokens": 3, "total_tokens": 8}}}),
    ]
}

fn usage_events() -> Vec<Value> {
    vec![json!({"type": "response.completed", "response": {"status": "completed",
        "usage": {"input_tokens": 100, "output_tokens": 40, "total_tokens": 140,
            "input_tokens_details": {"cached_tokens": 30, "cache_write_tokens": 10},
            "output_tokens_details": {"reasoning_tokens": 7}}}})]
}

fn incomplete_events() -> Vec<Value> {
    vec![
        json!({"type": "response.output_item.added", "output_index": 0,
            "item": {"type": "message", "id": "msg_1", "role": "assistant", "status": "in_progress", "content": []}}),
        json!({"type": "response.output_text.delta", "output_index": 0, "delta": "Hello"}),
        json!({"type": "response.output_item.done", "output_index": 0,
            "item": {"type": "message", "id": "msg_1", "role": "assistant", "status": "completed",
                "content": [{"type": "output_text", "text": "Hello"}]}}),
        json!({"type": "response.incomplete", "response": {"status": "incomplete",
            "incomplete_details": {"reason": "max_output_tokens"},
            "usage": {"input_tokens": 5, "output_tokens": 3, "total_tokens": 8}}}),
    ]
}

struct Scenario {
    name: &'static str,
    request: Value,
    fetch: Arc<dyn Fetch>,
}

fn scenario(name: &'static str, events: Vec<Value>, fetch: Arc<dyn Fetch>, extra: Value) -> Scenario {
    let mut request = json!({
        "operation": "sse",
        "events": events,
        "maxRetries": 0,
    });
    if let (Value::Object(target), Value::Object(extra)) = (&mut request, extra) {
        target.extend(extra);
    }
    Scenario { name, request, fetch }
}

fn sse_fetch(events: &[Value]) -> Arc<dyn Fetch> {
    RecordingFetch::new(vec![ScriptedResponse::sse(&[&sse_payload(events)])])
}

fn scenarios() -> Vec<Scenario> {
    let text = text_events();
    let retry_events = text_events();
    let abort_events = vec![
        json!({"type": "response.output_item.added", "output_index": 0,
            "item": {"type": "message", "id": "msg_1", "role": "assistant", "status": "in_progress", "content": []}}),
        json!({"type": "response.output_text.delta", "output_index": 0, "delta": "one"}),
    ];
    vec![
        scenario("text", text.clone(), sse_fetch(&text), json!({})),
        scenario(
            "reasoning",
            reasoning_events(),
            sse_fetch(&reasoning_events()),
            json!({}),
        ),
        scenario(
            "tool_call",
            tool_call_events(),
            sse_fetch(&tool_call_events()),
            json!({}),
        ),
        scenario("usage", usage_events(), sse_fetch(&usage_events()), json!({})),
        scenario(
            "incomplete",
            incomplete_events(),
            sse_fetch(&incomplete_events()),
            json!({}),
        ),
        scenario(
            "malformed",
            Vec::new(),
            RecordingFetch::new(vec![ScriptedResponse::sse(&["data: {not json}\n\n"])]),
            json!({"rawPayload": "data: {not json}\n\n"}),
        ),
        scenario(
            "http_error",
            Vec::new(),
            RecordingFetch::new(vec![ScriptedResponse::json(
                429,
                r#"{"error":{"code":"rate_limit_exceeded","message":"rate limited"}}"#,
            )]),
            json!({"retryStatus": 429, "retryBody": r#"{"error":{"code":"rate_limit_exceeded","message":"rate limited"}}"#}),
        ),
        scenario(
            "retry",
            retry_events.clone(),
            RecordingFetch::new(vec![
                ScriptedResponse::json(500, "server error").with_header("retry-after-ms", "0"),
                ScriptedResponse::sse(&[&sse_payload(&retry_events)]),
            ]),
            json!({"retryStatus": 500, "retryHeaders": [["retry-after-ms", "0"]], "retryBody": "server error", "maxRetries": 1}),
        ),
        scenario(
            "idle_timeout",
            Vec::new(),
            PendingBodyFetch::new(Vec::new()),
            json!({"timeoutMs": 50, "pendingBody": true}),
        ),
        scenario(
            "abort",
            abort_events.clone(),
            PendingBodyFetch::new(sse_payload(&abort_events).into_bytes()),
            json!({"pendingBody": true, "abortAfterDelta": true}),
        ),
        scenario(
            "header_timeout",
            Vec::new(),
            Arc::new(PendingHeaderFetch),
            json!({"timeoutMs": 50, "pendingHeaders": true}),
        ),
    ]
}

fn request_value(request: &HttpRequest) -> Value {
    let headers = request
        .headers
        .iter()
        .map(|(name, value)| (name.clone(), Value::String(value.to_owned())))
        .collect::<serde_json::Map<_, _>>();
    let body = request
        .body
        .as_deref()
        .map(|body| {
            let bytes = if request
                .headers
                .iter()
                .any(|(name, value)| name == "content-encoding" && value == "zstd")
            {
                zstd::stream::decode_all(body).expect("zstd body")
            } else {
                body.to_vec()
            };
            serde_json::from_slice::<Value>(&bytes).expect("json body")
        })
        .unwrap_or(Value::Null);
    json!({"url": request.url, "method": request.method, "headers": headers, "body": body})
}

fn normalize_requests(value: &Value) -> Value {
    value.clone()
}

#[tokio::test]
async fn differential_old_tool_metadata_does_not_reject_before_fetch() {
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let context: Context = serde_json::from_value(json!({"messages":[],"tools":[{
        "name":"legacy","description":"ignored metadata","parameters":{"type":"object"},
        "constrainedSampling":{"type":"grammar","variants":{}}
    }]}))
    .unwrap();
    let mut model = model("https://chatgpt.com/backend-api");
    model.compat = Some(pi_ai::types::ModelCompat::OpenAIResponses(
        serde_json::from_value(json!({"supportsOpenAIGrammarTools":true})).unwrap(),
    ));
    let expected = oracle.request(&json!({"id":"old-tool","operation":"sse","events":text_events(),
        "modelOverrides":{"compat":{"supportsOpenAIGrammarTools":true}},"context":context}));
    assert_eq!(expected["ok"], true, "{expected}");
    let fetch = RecordingFetch::new(vec![ScriptedResponse::sse(&[&sse_payload(&text_events())])]);
    let opts = options(fetch.clone(), Transport::Sse, Some(SESSION_ID), None);
    let _ = collect(stream(model, context, opts)).await;
    let requests = fetch.requests();
    assert_eq!(requests.len(), expected["output"]["requests"].as_array().unwrap().len());
    assert_eq!(
        request_value(&requests[0])["body"]["tools"],
        expected["output"]["requests"][0]["body"]["tools"]
    );
}

// Frozen options:88–94 omit toolChoice; buildRequestBody always emits auto.
#[tokio::test]
async fn source_absent_tool_choice_is_ignored_without_validation_or_request_override() {
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    for choice in ["required", "none", "unknown"] {
        let expected = oracle.request(
            &json!({"id":"old-choice","operation":"sse","events":text_events(),"options":{"toolChoice":choice}}),
        );
        assert_eq!(expected["ok"], true, "{expected}");
        let fetch = RecordingFetch::new(vec![ScriptedResponse::sse(&[&sse_payload(&text_events())])]);
        let mut base = options(fetch.clone(), Transport::Sse, Some(SESSION_ID), None).stream;
        base.extra.insert("toolChoice".into(), json!(choice));
        let opts = OpenAICodexResponsesOptions::from_stream_options(base).unwrap();
        let _ = collect(stream(model("https://chatgpt.com/backend-api"), context(), opts)).await;
        assert_eq!(
            request_value(&fetch.requests()[0])["body"],
            expected["output"]["requests"][0]["body"],
            "choice={choice}"
        );
    }
}

// Frozen CODEX_TOOL_CALL_PROVIDERS:64–70 includes openai-codex-team.
#[tokio::test]
async fn codex_team_preserves_responses_tool_call_ids_in_followup_request() {
    let mut context_value = serde_json::to_value(context()).unwrap();
    let messages = context_value["messages"].as_array_mut().unwrap();
    messages.push(json!({"role":"assistant","api":"openai-codex-responses","provider":"openai-codex-team","model":"gpt-5.1-codex","content":[{"type":"toolCall","id":"call_team|fc_team","name":"lookup","arguments":{"n":1}}],"usage":pi_ai::types::Usage::default(),"stopReason":"toolUse","timestamp":2}));
    messages.push(json!({"role":"toolResult","toolCallId":"call_team|fc_team","toolName":"lookup","content":[{"type":"text","text":"found"}],"isError":false,"timestamp":3}));
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let expected=oracle.request(&json!({"id":"team-ids","operation":"sse","events":text_events(),"context":context_value,"modelOverrides":{"provider":"openai-codex-team"}}));
    assert_eq!(expected["ok"], true, "{expected}");
    let mut native_model = model("https://chatgpt.com/backend-api");
    native_model.provider = "openai-codex-team".into();
    let fetch = RecordingFetch::new(vec![ScriptedResponse::sse(&[&sse_payload(&text_events())])]);
    let _ = collect(stream(
        native_model,
        serde_json::from_value(context_value).unwrap(),
        options(fetch.clone(), Transport::Sse, Some(SESSION_ID), None),
    ))
    .await;
    assert_eq!(
        request_value(&fetch.requests()[0])["body"],
        expected["output"]["requests"][0]["body"]
    );
}

#[tokio::test]
async fn differential_open_tool_error_cleans_scratch_after_delta_snapshots() {
    let events = vec![
        json!({"type":"response.output_item.added","output_index":0,"item":{
            "type":"function_call","id":"fc_open","call_id":"call_open","name":"lookup","arguments":""}}),
        json!({"type":"response.function_call_arguments.delta","output_index":0,"delta":"{\"n\":1"}),
        json!({"type":"error","code":"probe","message":"open tool failed"}),
    ];
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let expected = oracle.request(&json!({"id":"open-tool-error","operation":"sse","events":events,"maxRetries":0}));
    assert_eq!(expected["ok"], true);
    let mut opts = options(sse_fetch(&events), Transport::Sse, Some(SESSION_ID), None);
    opts.stream.request.max_retries = Some(0.0);
    let (events, result) = collect(stream(model("https://chatgpt.com/backend-api"), context(), opts)).await;
    assert_events(&events, &expected["output"]["events"], "open-tool-error");
    assert_eq!(result, expected["output"]["result"]);
}

#[tokio::test]
async fn sse_header_timeout_uses_node_timeout_validation_before_fetch() {
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    for timeout in [4_294_967_296.0, 1e20] {
        let expected = oracle.request(
            &json!({"id":"timeout-domain","operation":"sse","events":text_events(),"timeoutMs":timeout,"maxRetries":0}),
        );
        assert_eq!(expected["ok"], true, "{expected}");
        let fetch = RecordingFetch::new(vec![ScriptedResponse::sse(&[&sse_payload(&text_events())])]);
        let mut options = options(fetch.clone(), Transport::Sse, Some(SESSION_ID), Some(timeout));
        options.stream.request.max_retries = Some(0.0);
        let (events, result) = collect(stream(model("https://chatgpt.com/backend-api"), context(), options)).await;
        assert_eq!(
            fetch.requests().len(),
            expected["output"]["requests"].as_array().unwrap().len(),
            "timeout {timeout}"
        );
        assert_events(&events, &expected["output"]["events"], "timeout-domain");
        assert_eq!(result, expected["output"]["result"], "timeout {timeout}");
    }
}

#[tokio::test]
async fn sse_fetch_signal_identity_and_finished_lifetime_match_frozen() {
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    for (caller, timeout) in [
        (false, None),
        (true, None),
        (false, Some(60_000.0)),
        (true, Some(60_000.0)),
    ] {
        let mut request = pi_js_compat::js_json!({"id":"signal-lifetime","operation":"sse","events":text_events(),"omitSignal":!caller,"observeSignals":true,"abortAfterCompletion":true,"maxRetries":0});
        if let Some(timeout) = timeout {
            request
                .as_object_mut()
                .unwrap()
                .insert("timeoutMs".into(), pi_js_compat::js_json::JsValue::Number(timeout));
        }
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], pi_js_compat::js_json::JsValue::Bool(true));
        let fetch = Arc::new(CaptureSignalFetch {
            inner: RecordingFetch::new(vec![ScriptedResponse::sse(&[&sse_payload(&text_events())])]),
            signals: std::sync::Mutex::new(Vec::new()),
        });
        let controller = pi_js_compat::abort::AbortController::new();
        let signal = caller.then(|| controller.signal());
        let mut options = options(fetch.clone(), Transport::Sse, Some(SESSION_ID), timeout);
        options.stream.request.signal = signal.clone();
        options.stream.request.max_retries = Some(0.0);
        let _ = collect(stream(model("https://chatgpt.com/backend-api"), context(), options)).await;
        controller.abort(Some(pi_js_compat::abort::AbortReason::Value(
            pi_js_compat::js_json::JsValue::String(pi_js_compat::js_json::JsString::from_utf16(
                "finished".encode_utf16().chain([0xd800]).collect::<Vec<_>>(),
            )),
        )));
        assert_eq!(
            fetch.snapshots(signal.as_ref()),
            expected["output"]["fetchSignals"],
            "caller={caller}, timeout={timeout:?}"
        );
    }
}

#[tokio::test]
async fn sse_header_timeout_exposes_timeout_dom_exception_to_fetch() {
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    for caller in [false, true] {
        let expected = oracle.request(&pi_js_compat::js_json!({
            "id":"timeout-reason","operation":"sse","pendingHeaders":true,
            "timeoutMs":5,"maxRetries":0,"omitSignal":!caller,"observeSignals":true
        }));
        let fetch = Arc::new(CaptureSignalFetch {
            inner: Arc::new(PendingHeaderFetch),
            signals: std::sync::Mutex::new(Vec::new()),
        });
        let controller = pi_js_compat::abort::AbortController::new();
        let signal = caller.then(|| controller.signal());
        let mut options = options(fetch.clone(), Transport::Sse, Some(SESSION_ID), Some(5.0));
        options.stream.request.signal = signal.clone();
        options.stream.request.max_retries = Some(0.0);
        let (_, result) = collect(stream(model("https://chatgpt.com/backend-api"), context(), options)).await;
        assert_eq!(
            pi_js_compat::js_json::JsValue::from(result),
            expected["output"]["result"]
        );
        assert_eq!(fetch.snapshots(signal.as_ref()), expected["output"]["fetchSignals"]);
    }
}

#[tokio::test]
async fn differential_sse_scenarios() {
    for scenario in scenarios() {
        let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
        let request = json!({"id": scenario.name, "operation": "sse"});
        let mut request = request;
        if let Value::Object(target) = &mut request
            && let Value::Object(extra) = &scenario.request
        {
            target.extend(extra.clone());
        }
        let oracle_output = oracle.request(&request);
        assert_eq!(
            oracle_output["ok"], true,
            "oracle failed for {}: {oracle_output}",
            scenario.name
        );
        let expected = &oracle_output["output"];

        let max_retries = scenario.request.get("maxRetries").and_then(Value::as_f64);
        let timeout_ms = scenario.request.get("timeoutMs").and_then(Value::as_f64);
        let mut opts = options(scenario.fetch.clone(), Transport::Sse, Some(SESSION_ID), timeout_ms);
        opts.stream.request.max_retries = max_retries;
        let (events, result) = if scenario.name == "abort" {
            let token_controller = pi_js_compat::abort::AbortController::new();
            let token = token_controller.signal();
            let abort_events = vec![
                json!({"type": "response.output_item.added", "output_index": 0,
                    "item": {"type": "message", "id": "msg_1", "role": "assistant", "status": "in_progress", "content": []}}),
                json!({"type": "response.output_text.delta", "output_index": 0, "delta": "one"}),
            ];
            let fetch = RecordingFetch::with_abort_after_chunks(
                vec![ScriptedResponse::sse(&[&sse_payload(&abort_events)])],
                1,
                token_controller.clone(),
            );
            let mut opts = options(fetch, Transport::Sse, Some(SESSION_ID), timeout_ms);
            opts.stream.request.signal = Some(token);
            collect(stream(model("https://chatgpt.com/backend-api"), context(), opts)).await
        } else {
            collect(stream(model("https://chatgpt.com/backend-api"), context(), opts)).await
        };
        assert_events(&events, &expected["events"], scenario.name);
        assert_eq!(result, expected["result"], "result mismatch for {}", scenario.name);
    }
}

#[tokio::test]
async fn differential_sse_requests() {
    for scenario in scenarios() {
        let fetch: Arc<RecordingFetch> = match scenario.name {
            "text" => RecordingFetch::new(vec![ScriptedResponse::sse(&[&sse_payload(&text_events())])]),
            "retry" => RecordingFetch::new(vec![
                ScriptedResponse::json(500, "server error").with_header("retry-after-ms", "0"),
                ScriptedResponse::sse(&[&sse_payload(&text_events())]),
            ]),
            "http_error" => RecordingFetch::new(vec![ScriptedResponse::json(
                429,
                r#"{"error":{"code":"rate_limit_exceeded","message":"rate limited"}}"#,
            )]),
            _ => continue,
        };
        let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
        let mut request = scenario.request.clone();
        if let Value::Object(target) = &mut request {
            target.insert("id".to_owned(), Value::String(scenario.name.to_owned()));
        }
        let oracle_output = oracle.request(&request);
        let expected = &oracle_output["output"]["requests"];
        let mut opts = options(fetch.clone(), Transport::Sse, Some(SESSION_ID), None);
        opts.stream.request.max_retries = scenario.request.get("maxRetries").and_then(Value::as_f64);
        let _ = collect(stream(model("https://chatgpt.com/backend-api"), context(), opts)).await;
        let actual = Value::Array(fetch.requests().iter().map(request_value).collect());
        assert_eq!(
            normalize_requests(&actual),
            normalize_requests(expected),
            "request mismatch for {}",
            scenario.name
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn differential_websocket_cached_continuation() {
    reset_openai_codex_websocket_debug_stats(None);
    close_openai_codex_websocket_sessions(None);
    let script: ServerScript = Arc::new(|connection, frame| {
        if connection == 0 && frame == 0 {
            completed_event("resp_1", Some("Hello"), None)
        } else {
            completed_event("resp_2", Some("Done"), None)
        }
    });

    let oracle_server = start_ws_server(script.clone()).await;
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let oracle_output = oracle.request(&json!({
        "id": "ws_cached",
        "operation": "ws_cached",
        "wsUrl": oracle_server.url
    }));
    assert_eq!(oracle_output["ok"], true, "oracle failed: {oracle_output}");
    let expected = &oracle_output["output"];
    let oracle_records = snapshot(oracle_server.records.connections.lock().unwrap().as_slice());
    drop(oracle_server);

    let rust_server = start_ws_server(script).await;
    let first_context = context();
    let first = collect(stream(
        model(&rust_server.url),
        first_context.clone(),
        options(
            RecordingFetch::new(Vec::new()),
            Transport::WebsocketCached,
            Some("diff-ws-cached"),
            None,
        ),
    ))
    .await;
    let mut second_context = first_context;
    second_context.messages.push(pi_ai::types::Message::Assistant(Box::new(
        serde_json::from_value(first.1.clone()).expect("assistant message"),
    )));
    second_context
        .messages
        .push(serde_json::from_value(json!({"role": "user", "content": "Now finish", "timestamp": 2})).expect("user"));
    let second = collect(stream(
        model(&rust_server.url),
        second_context,
        options(
            RecordingFetch::new(Vec::new()),
            Transport::WebsocketCached,
            Some("diff-ws-cached"),
            None,
        ),
    ))
    .await;
    let rust_records = snapshot(rust_server.records.connections.lock().unwrap().as_slice());

    assert_eq!(rust_records, oracle_records, "websocket frames/headers mismatch");
    assert_events(&first.0, &expected["firstEvents"], "first");
    assert_eq!(first.1, expected["first"], "first result mismatch");
    assert_events(&second.0, &expected["secondEvents"], "second");
    assert_eq!(second.1, expected["second"], "second result mismatch");
    close_openai_codex_websocket_sessions(Some(&pi_js_compat::js_json::JsString::from("diff-ws-cached")));
    reset_openai_codex_websocket_debug_stats(Some(&pi_js_compat::js_json::JsString::from("diff-ws-cached")));
}

fn snapshot(connections: &[ws_server::ServerConnection]) -> Value {
    Value::Array(
        connections
            .iter()
            .map(|connection| {
                let headers = connection
                    .headers
                    .iter()
                    .map(|(name, value)| {
                        let value = match name.as_str() {
                            "user-agent" => "<ua>",
                            "host" => "<host>",
                            "sec-websocket-key" => "<key>",
                            _ => value,
                        };
                        (name.clone(), Value::String(value.to_owned()))
                    })
                    .collect::<serde_json::Map<_, _>>();
                json!({"headers": headers, "frames": connection.frames})
            })
            .collect(),
    )
}

fn assert_diagnostic_clock(value: &Value, bounds: [u64; 2], side: &str) {
    let timestamp = value
        .as_u64()
        .expect("diagnostic timestamp must be numeric milliseconds");
    assert!(
        bounds[0] <= timestamp && timestamp <= bounds[1],
        "{side} diagnostic {timestamp} outside invocation {bounds:?}"
    );
}

fn assert_timed_diagnostic(actual: &Value, expected: &Value, native_bounds: [u64; 2], source_bounds: [u64; 2]) {
    let actual = actual.as_object().unwrap();
    let expected = expected.as_object().unwrap();
    assert_eq!(actual.keys().collect::<Vec<_>>(), expected.keys().collect::<Vec<_>>());
    for (key, value) in expected {
        match key.as_str() {
            "timestamp" => {
                assert_diagnostic_clock(&actual[key], native_bounds, "native");
                assert_diagnostic_clock(value, source_bounds, "source");
            }
            "error" => {
                let mut expected_error = value.as_object().unwrap().clone();
                // Approved2026-10-22: omit V8 stack text, never fabricate it.
                if let Some(stack) = expected_error.remove("stack") {
                    assert!(stack.is_string());
                }
                assert_eq!(actual[key], Value::Object(expected_error));
            }
            _ => assert_eq!(actual[key], *value, "diagnostic field {key}"),
        }
    }
}

fn assert_timed_message(actual: &Value, expected: &Value, native_bounds: [u64; 2], source_bounds: [u64; 2]) {
    let actual = actual.as_object().unwrap();
    let expected = expected.as_object().unwrap();
    assert_eq!(actual.keys().collect::<Vec<_>>(), expected.keys().collect::<Vec<_>>());
    for (key, value) in expected {
        if key != "diagnostics" {
            assert_eq!(actual[key], *value, "message field {key}");
            continue;
        }
        let actual = actual[key].as_array().unwrap();
        let expected = value.as_array().unwrap();
        assert_eq!(actual.len(), expected.len());
        assert!(!actual.is_empty(), "timeout must emit its diagnostic");
        for (actual, expected) in actual.iter().zip(expected) {
            assert_timed_diagnostic(actual, expected, native_bounds, source_bounds);
        }
    }
}

fn assert_timed_timeout_output(events: &[Value], result: &Value, expected: &Value, native_bounds: [u64; 2]) {
    let source_bounds = [
        expected["clockBounds"][0].as_u64().unwrap(),
        expected["clockBounds"][1].as_u64().unwrap(),
    ];
    let expected_events = expected["events"].as_array().unwrap();
    assert_eq!(events.len(), expected_events.len());
    assert_eq!(
        events.len(),
        1,
        "timeout before first data emits exactly one error event"
    );
    let mut actual_event = events[0].as_object().unwrap().clone();
    let mut expected_event = expected_events[0].as_object().unwrap().clone();
    let actual_message = actual_event.remove("error").unwrap();
    let expected_message = expected_event.remove("error").unwrap();
    assert_eq!(actual_event, expected_event);
    assert_timed_message(&actual_message, &expected_message, native_bounds, source_bounds);
    assert_timed_message(result, &expected["result"], native_bounds, source_bounds);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn differential_websocket_idle_timeout() {
    let oracle_server = start_ws_server(Arc::new(|_, _| Vec::new())).await;
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let oracle_output = oracle.request(&json!({
        "id": "ws_idle",
        "operation": "ws_idle",
        "wsUrl": oracle_server.url
    }));
    assert_eq!(oracle_output["ok"], true, "oracle failed: {oracle_output}");
    let expected = &oracle_output["output"];
    drop(oracle_server);

    let rust_server = start_ws_server(Arc::new(|_, _| Vec::new())).await;
    let before = pi_ai::request::now_millis();
    let (events, result) = collect(stream(
        model(&rust_server.url),
        context(),
        options(
            RecordingFetch::new(Vec::new()),
            Transport::Websocket,
            Some("diff-ws-idle"),
            Some(50.0),
        ),
    ))
    .await;
    let after = pi_ai::request::now_millis();
    assert_timed_timeout_output(&events, &result, expected, [before, after]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn differential_websocket_connect_timeout() {
    let oracle_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let oracle_address = oracle_listener.local_addr().expect("address");
    let oracle_task = tokio::spawn(async move {
        let (stream, _) = oracle_listener.accept().await.expect("accept");
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        drop(stream);
    });
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let oracle_output = oracle.request(&json!({
        "id": "ws_connect_timeout",
        "operation": "ws_connect_timeout",
        "wsUrl": format!("ws://{oracle_address}")
    }));
    assert_eq!(oracle_output["ok"], true, "oracle failed: {oracle_output}");
    let expected = &oracle_output["output"];
    oracle_task.abort();

    let rust_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let rust_address = rust_listener.local_addr().expect("address");
    let rust_task = tokio::spawn(async move {
        let (stream, _) = rust_listener.accept().await.expect("accept");
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        drop(stream);
    });
    let mut opts = options(
        RecordingFetch::new(Vec::new()),
        Transport::Websocket,
        Some("diff-ws-connect"),
        None,
    );
    opts.stream.websocket_connect_timeout_ms = Some(50.0);
    let before = pi_ai::request::now_millis();
    let (events, result) = collect(stream(model(&format!("ws://{rust_address}")), context(), opts)).await;
    let after = pi_ai::request::now_millis();
    assert_timed_timeout_output(&events, &result, expected, [before, after]);
    rust_task.abort();
}
