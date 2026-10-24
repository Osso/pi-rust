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
    OpenAICodexResponsesOptions, close_openai_codex_websocket_sessions, get_openai_codex_websocket_debug_stats,
    reset_openai_codex_websocket_debug_stats, stream,
};
use pi_ai::request::{
    BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse, ProviderRequestOptions, StreamOptions,
};
use pi_ai::types::{Context, Model, Transport};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use pi_js_compat::js_json::JsValue;
use recording_fetch::{RecordingFetch, ScriptedResponse};
use serde_json::{Value, json};
use ws_server::{FrameGate, ServerScript, completed_event, start_ws_server, start_ws_server_with_gate};

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

async fn collect_lossless(
    mut stream: AssistantMessageEventStream,
) -> (pi_js_compat::js_json::JsValue, pi_js_compat::js_json::JsValue) {
    use pi_js_compat::js_json::{JsString, JsValue, to_value};
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        let mut event = to_value(&event.unwrap()).unwrap();
        for key in ["partial", "message", "error"] {
            if let Some(message) = event.as_object_mut().unwrap().get_mut(&JsString::from(key)) {
                assert!(message.get("timestamp").is_some());
                message["timestamp"] = JsValue::Number(0.0);
            }
        }
        events.push(event);
    }
    let mut result = to_value(&stream.result().await.unwrap()).unwrap();
    result["timestamp"] = JsValue::Number(0.0);
    (JsValue::Array(events), result)
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

struct DelayedIgnoringSignalFetch {
    inner: Arc<dyn Fetch>,
}
impl Fetch for DelayedIgnoringSignalFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<pi_js_compat::abort::AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async move {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            self.inner.fetch(request, signal).await
        })
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
        signal: Option<pi_js_compat::abort::AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async move {
            match signal {
                Some(signal) => Err(CallbackError::from_rejection(signal.wait().await)),
                None => std::future::pending().await,
            }
        })
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

// Frozen164–167 caps only429;352–435 catches HTTP/callback failures inside each attempt.
#[tokio::test]
async fn sse_retry_transcript_matches_source_caps_catch_order_and_callback_rejection() {
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    for (status, delay, body, response_error, retry_error) in [
        (429u16, "11", "rate limited", None, None),
        (503, "11", "server error", None, None),
        (403, "0", "forbidden", None, None),
        (200, "0", "ignored", Some("response callback exploded"), None),
        (429, "0", "Monthly usage limit reached", None, None),
        (503, "0", "server error", None, None),
        (503, "0", "server error", None, Some("retry callback exploded")),
        (
            200,
            "0",
            "ignored",
            Some("response callback exploded"),
            Some("retry callback exploded"),
        ),
    ] {
        let mut request = json!({"id":"retry-transcript","operation":"sse","events":text_events(),"retryStatus":status,"retryBody":body,"retryHeaders":[["retry-after-ms",delay],["content-type","application/json"]],"maxRetries":1,"options":{"maxRetryDelayMs":5}});
        if let Some(message) = response_error {
            request["onResponseError"] = json!(message);
        }
        if let Some(message) = retry_error {
            request["onRetryError"] = json!(message);
        }
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], true, "{expected}");
        let fetch = RecordingFetch::new(vec![
            ScriptedResponse::json(status, body).with_header("retry-after-ms", delay),
            ScriptedResponse::sse(&[&sse_payload(&text_events())]),
        ]);
        let retries = Arc::new(std::sync::Mutex::new(Vec::<Value>::new()));
        let responses = Arc::new(std::sync::Mutex::new(Vec::<u16>::new()));
        let mut opts = options(fetch.clone(), Transport::Sse, Some(SESSION_ID), None);
        opts.stream.request.max_retries = Some(1.0);
        opts.stream.request.max_retry_delay_ms = Some(5.0);
        let recorded = retries.clone();
        opts.stream.on_retry = Some(Arc::new(move |event, _| {
            let first = {
                let mut events = recorded.lock().unwrap();
                events.push(serde_json::to_value(event).unwrap());
                events.len() == 1
            };
            if first && let Some(message) = retry_error {
                return Err(CallbackError::new(message));
            }
            Ok(())
        }));
        let recorded = responses.clone();
        opts.stream.request.on_response = Some(Arc::new(move |response, _| {
            let first = {
                let mut statuses = recorded.lock().unwrap();
                statuses.push(response.status as u16);
                statuses.len() == 1
            };
            Box::pin(async move {
                if first && let Some(message) = response_error {
                    return Err(CallbackError::new(message));
                }
                Ok(())
            })
        }));
        let (events, result) = collect(stream(model("https://chatgpt.com/backend-api"), context(), opts)).await;
        let actual_requests = Value::Array(fetch.requests().iter().map(request_value).collect());
        assert_eq!(
            normalize_requests(&actual_requests),
            normalize_requests(&expected["output"]["requests"]),
            "status={status},response_error={response_error:?}"
        );
        assert_eq!(json!(*retries.lock().unwrap()), expected["output"]["retries"]);
        assert_eq!(
            json!(*responses.lock().unwrap()),
            expected["output"]["responseStatuses"]
        );
        assert_events(&events, &expected["output"]["events"], "retry-transcript");
        assert_eq!(result, expected["output"]["result"]);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn websocket_event_errors_ids_and_terminal_statuses_match_frozen() {
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let mut cases = vec![
        json!({"type":"error"}),
        json!({"type":"error","message":"","code":""}),
        json!({"type":"error","message":"","code":"empty-message","request_id":" body-id "}),
        json!({"type":"error","message":"denied","request_id":" body-id ","headers":{"X-Request-ID":" header-id "}}),
        json!({"type":"error","error":{"message":"nested","request_id":" nested-id "}}),
        json!({"type":"response.failed"}),
        json!({"type":"response.failed","response":{"error":{"message":"","code":"failed","request_id":" nested-id "}}}),
        json!({"type":"response.failed","response":{"request_id":" response-id ","error":{"message":17,"request_id":" ignored-id "}}}),
        json!({"type":"response.failed","response":{"error":{"message":[],"code":""}},"headers":{"x-request-id":"   "},"request_id":"fallback-id"}),
        json!({"type":"response.failed","response":{"error":{"message":{},"code":null}}}),
    ];
    for kind in ["response.completed", "response.done", "response.incomplete"] {
        for status in [
            "completed",
            "incomplete",
            "failed",
            "cancelled",
            "queued",
            "in_progress",
            "unknown",
        ] {
            cases.push(json!({"type":kind,"response":{"id":"ws-status-id","status":status}}));
        }
    }
    let session = pi_js_compat::js_json::JsString::from("diff-ws-events");
    for event in cases {
        let events = vec![event.clone()];
        let script: ServerScript = Arc::new(move |_, _| events.clone());
        let source_server = start_ws_server(script.clone()).await;
        let expected = oracle.request(&json!({"id":"ws-events","operation":"ws_events","wsUrl":source_server.url}));
        assert_eq!(expected["ok"], true, "{expected}");
        let server = start_ws_server(script).await;
        let opts = options(
            RecordingFetch::new(Vec::new()),
            Transport::WebsocketCached,
            Some("diff-ws-events"),
            None,
        );
        let before = pi_ai::request::now_millis();
        let (actual, result) = collect(stream(model(&server.url), context(), opts)).await;
        let after = pi_ai::request::now_millis();
        close_openai_codex_websocket_sessions(Some(&session));
        reset_openai_codex_websocket_debug_stats(Some(&session));
        assert_clock_bounded_output(&actual, &result, &expected["output"], [before, after]);
        assert_eq!(
            snapshot(server.records.connections.lock().unwrap().as_slice()),
            snapshot(source_server.records.connections.lock().unwrap().as_slice()),
            "{event}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn codex_overflowed_node_timers_keep_the_configured_error_but_expire_at_one_ms() {
    // Frozen779/1122/1350 setTimeout has Node's >i32 overflow normalization.
    let timeout_ms = 2147483648.0;
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let mut missing = Vec::new();
    for boundary in ["sse-body", "ws-idle", "ws-connect"] {
        let server = start_ws_server(Arc::new(|_, _| Vec::new())).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let blocker = tokio::spawn(async move {
            let (_socket, _) = listener.accept().await.unwrap();
            std::future::pending::<()>().await;
        });
        let request = match boundary {
            "sse-body" => {
                json!({"id":boundary,"operation":"sse","events":[],"pendingBody":true,"timeoutMs":timeout_ms})
            }
            "ws-idle" => json!({"id":boundary,"operation":"ws_idle","wsUrl":server.url,"timeoutMs":timeout_ms}),
            _ => json!({"id":boundary,"operation":"ws_connect_timeout","wsUrl":url,"connectTimeoutMs":timeout_ms}),
        };
        let expected = oracle.request(&request);
        assert_eq!(expected["ok"], true, "{expected}");
        let controller = pi_js_compat::abort::AbortController::new();
        let mut opts = options(
            PendingBodyFetch::new(Vec::new()),
            if boundary == "sse-body" {
                Transport::Sse
            } else {
                Transport::Websocket
            },
            Some(if boundary == "ws-idle" {
                "diff-ws-idle"
            } else {
                "diff-ws-connect"
            }),
            Some(timeout_ms),
        );
        opts.stream.request.signal = Some(controller.signal());
        let target = if boundary == "ws-connect" {
            opts.stream.websocket_connect_timeout_ms = Some(timeout_ms);
            // A separate listening socket leaves the native handshake pending too.
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let native_url = format!("ws://{}", listener.local_addr().unwrap());
            let native_blocker = tokio::spawn(async move {
                let (_socket, _) = listener.accept().await.unwrap();
                std::future::pending::<()>().await;
            });
            (native_url, Some(native_blocker))
        } else {
            (server.url.clone(), None)
        };
        let before = pi_ai::request::now_millis();
        match tokio::time::timeout(
            std::time::Duration::from_millis(200),
            collect(stream(model(&target.0), context(), opts)),
        )
        .await
        {
            Ok((events, result)) => {
                let after = pi_ai::request::now_millis();
                if boundary == "sse-body" {
                    assert_events(&events, &expected["output"]["events"], boundary);
                    assert_eq!(result, expected["output"]["result"]);
                } else {
                    assert_timed_timeout_output(&events, &result, &expected["output"], [before, after]);
                }
            }
            Err(_) => {
                missing.push(boundary);
                controller.abort(None);
            }
        }
        blocker.abort();
        if let Some(blocker) = target.1 {
            blocker.abort();
        }
    }
    assert!(missing.is_empty(), "timers failed to expire: {missing:?}");
}

#[tokio::test]
async fn codex_sse_framing_matches_streaming_textdecoder_bom_and_js_trim() {
    // Frozen796-835: one streaming TextDecoder, then ECMAScript trim per data line.
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let terminal = r#"{"type":"response.completed","response":{"id":"sse-framing","status":"completed"}}"#;
    let mut failures = Vec::new();
    for raw in [
        format!("\u{feff}data: {terminal}\n\n"),
        format!("data: \u{feff}{terminal}\u{feff}\n\n"),
        format!("data: \u{85}{terminal}\u{85}\n\n"),
        format!("data: \u{200b}{terminal}\u{200b}\n\n"),
        format!("\u{feff}data: [DONE]\n\ndata: {terminal}\n\n"),
        format!("data: [DONE]\n\n\u{feff}data: {terminal}\n\n"),
    ] {
        let expected =
            oracle.request(&json!({"id":"sse-framing","operation":"sse","events":[],"rawPayload":raw,"chunkSize":1}));
        assert_eq!(expected["ok"], true, "{expected}");
        let fetch = RecordingFetch::new(vec![ScriptedResponse {
            status: 200,
            headers: vec![("content-type".into(), "text/event-stream".into())],
            chunks: raw.as_bytes().chunks(1).map(<[u8]>::to_vec).collect(),
        }]);
        let (events, result) = collect(stream(
            model("https://chatgpt.com/backend-api"),
            context(),
            options(fetch, Transport::Sse, Some(SESSION_ID), None),
        ))
        .await;
        if json!(events) != expected["output"]["events"] || result != expected["output"]["result"] {
            failures.push(json!({"raw":raw,"events":events,"result":result,"expected":expected["output"]}));
        }
    }
    assert!(failures.is_empty(), "{}", json!(failures));
}

#[tokio::test]
async fn codex_all_sdk_service_tiers_preserve_null_body_and_pricing() {
    // Frozen options91/buildBody526 accepts SDK service_tier including scale/null;
    // frozen554-595 resolves response-default to paid request tiers before pricing.
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let mut failures = Vec::new();
    for model_id in ["gpt-5.5", "gpt-5.1-codex"] {
        for tier in [
            Value::Null,
            json!("auto"),
            json!("default"),
            json!("flex"),
            json!("scale"),
            json!("priority"),
        ] {
            for response_tier in [None, Some("default"), Some("flex"), Some("priority")] {
                let mut events = usage_events();
                if let Some(tier) = response_tier {
                    events.last_mut().unwrap()["response"]["service_tier"] = json!(tier);
                }
                let expected = oracle.request(&json!({"id":"sdk-service-tier","operation":"sse","events":events,"modelOverrides":{"id":model_id},"options":{"serviceTier":tier}}));
                assert_eq!(expected["ok"], true, "{expected}");
                let fetch = RecordingFetch::new(vec![ScriptedResponse::sse(&[&sse_payload(&events)])]);
                let mut opts = options(fetch.clone(), Transport::Sse, Some(SESSION_ID), None);
                opts.stream.extra.insert("serviceTier".into(), tier.clone());
                let opts = match OpenAICodexResponsesOptions::from_stream_options(opts.stream) {
                    Ok(opts) => opts,
                    Err(error) => {
                        failures.push(json!({"tier":tier,"decodeError":error}));
                        continue;
                    }
                };
                let mut native_model = model("https://chatgpt.com/backend-api");
                native_model.id = model_id.into();
                let (events, result) = collect(stream(native_model, context(), opts)).await;
                let requests = json!(fetch.requests().iter().map(request_value).collect::<Vec<_>>());
                if normalize_requests(&requests) != normalize_requests(&expected["output"]["requests"])
                    || json!(events) != expected["output"]["events"]
                    || result != expected["output"]["result"]
                {
                    failures.push(json!({"tier":tier,"model":model_id,"responseTier":response_tier,"actual":{"requests":requests,"events":events,"result":result},"expected":expected["output"]}));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", json!(failures));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn payload_json_replacements_keep_sse_values_and_websocket_object_spread() {
    // Frozen263-265 permits any non-undefined callback value; frozen1538 spreads it.
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let session = pi_js_compat::js_json::JsString::from("diff-ws-events");
    for replacement in [
        json!([]),
        json!([17, "item"]),
        json!("éx"),
        json!(false),
        json!(0),
        json!({"7":"seven","1":"one","model":"replacement"}),
    ] {
        for websocket in [false, true] {
            let script: ServerScript = Arc::new(|_, _| completed_event("payload-response", Some("Hello"), None));
            let source_server = start_ws_server(script.clone()).await;
            let expected = oracle.request(&json!({"id":"payload-replacement","operation":if websocket {"ws_events"} else {"sse"},"wsUrl":source_server.url,"events":completed_event("payload-response",Some("Hello"),None),"onPayloadReplacement":replacement}));
            assert_eq!(expected["ok"], true, "{expected}");
            let server = start_ws_server(script).await;
            let fetch = RecordingFetch::new(vec![ScriptedResponse::sse(&[&sse_payload(&completed_event(
                "payload-response",
                Some("Hello"),
                None,
            ))])]);
            let mut opts = options(
                fetch.clone(),
                if websocket {
                    Transport::WebsocketCached
                } else {
                    Transport::Sse
                },
                Some(if websocket { "diff-ws-events" } else { SESSION_ID }),
                None,
            );
            let payload = JsValue::from(&replacement);
            opts.stream.request.on_payload = Some(Arc::new(move |_, _| {
                let payload = payload.clone();
                Box::pin(async move { Ok(Some(payload)) })
            }));
            let before = pi_ai::request::now_millis();
            let (events, result) = collect(stream(
                model(if websocket {
                    &server.url
                } else {
                    "https://chatgpt.com/backend-api"
                }),
                context(),
                opts,
            ))
            .await;
            let after = pi_ai::request::now_millis();
            if websocket {
                close_openai_codex_websocket_sessions(Some(&session));
                reset_openai_codex_websocket_debug_stats(Some(&session));
                assert_clock_bounded_output(&events, &result, &expected["output"], [before, after]);
                assert_eq!(
                    snapshot(server.records.connections.lock().unwrap().as_slice()),
                    snapshot(source_server.records.connections.lock().unwrap().as_slice()),
                    "replacement {replacement}"
                );
                let raw = server
                    .records
                    .connections
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|connection| connection.raw_frames.clone())
                    .collect::<Vec<_>>();
                let expected_raw = source_server
                    .records
                    .connections
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|connection| connection.raw_frames.clone())
                    .collect::<Vec<_>>();
                assert_eq!(raw, expected_raw, "wire JSON for replacement {replacement}");
            } else {
                assert_events(&events, &expected["output"]["events"], &replacement.to_string());
                assert_eq!(result, expected["output"]["result"]);
                assert_eq!(
                    json!(fetch.requests().iter().map(request_value).collect::<Vec<_>>()),
                    expected["output"]["requests"]
                );
            }
        }
    }
}

#[tokio::test]
async fn codex_terminal_status_preserves_frozen_done_even_for_error_stop_reasons() {
    // Frozen Codex410-453 casts the final stopReason, with no success guard.
    // shared714-727 maps completed/failed or cancelled to stopReason=error.
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    for kind in ["response.completed", "response.done", "response.incomplete"] {
        for status in [
            "completed",
            "incomplete",
            "failed",
            "cancelled",
            "queued",
            "in_progress",
            "unknown",
        ] {
            let events = vec![json!({"type":kind,"response":{"id":"status-boundary","status":status}})];
            let expected = oracle.request(&json!({"id":"terminal-status","operation":"sse","events":events}));
            assert_eq!(expected["ok"], true, "{expected}");
            let opts = options(sse_fetch(&events), Transport::Sse, Some(SESSION_ID), None);
            let (actual, result) = collect(stream(model("https://chatgpt.com/backend-api"), context(), opts)).await;
            assert_events(&actual, &expected["output"]["events"], &format!("{kind}/{status}"));
            assert_eq!(result, expected["output"]["result"], "{kind}/{status}");
        }
    }
}

#[tokio::test]
async fn codex_jwt_account_ingress_matches_atob_json_and_header_coercion() {
    use pi_js_compat::{js_json, js_json::stringify_lossless};
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let mut failures = Vec::new();
    for payload_error in [None, Some("payload callback before header coercion")] {
        for account_json in [
            r#""acc_test""#,
            "17",
            "true",
            "false",
            "0",
            "null",
            r#""""#,
            "[]",
            "[1,null,\"x\"]",
            "{}",
            r#"{"toString":null}"#,
            r#""café""#,
            r#""caf\u00e9""#,
            r#""\ud800""#,
            r#""\t acc_test \r\n""#,
            r#""a\nb""#,
            r#""a\u0000b""#,
        ] {
            let payload = format!("{{\"https://api.openai.com/auth\":{{\"chatgpt_account_id\":{account_json}}}}}");
            let encoded = pi_js_compat::base64::btoa(payload.as_bytes());
            let token = format!("aaa.{encoded}.bbb");
            let mut request = js_json!({"id":"jwt-account","operation":"sse","events":text_events(),"options":{"apiKey":token},"maxRetries":0});
            if let Some(message) = payload_error {
                request
                    .as_object_mut()
                    .unwrap()
                    .insert("onPayloadError".into(), JsValue::String(message.into()));
            }
            let expected = oracle.request(&request);
            assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
            let fetch = RecordingFetch::new(vec![ScriptedResponse::sse(&[&sse_payload(&text_events())])]);
            let mut opts = options(fetch.clone(), Transport::Sse, Some(SESSION_ID), None);
            opts.stream.request.api_key = Some(token.into());
            opts.stream.request.max_retries = Some(0.0);
            if let Some(message) = payload_error {
                opts.stream.request.on_payload = Some(Arc::new(move |_, _| {
                    Box::pin(async move { Err(CallbackError::new(message)) })
                }));
            }
            let (events, result) =
                collect_lossless(stream(model("https://chatgpt.com/backend-api"), context(), opts)).await;
            let requests = JsValue::from(Value::Array(fetch.requests().iter().map(request_value).collect()));
            if events != expected["output"]["events"]
                || result != expected["output"]["result"]
                || requests != expected["output"]["requests"]
            {
                failures.push(js_json!({"account":account_json,"payloadError":payload_error,"expected":expected["output"],"actual":{"events":events,"result":result,"requests":requests}}));
            }
        }
    }
    let path = std::env::temp_dir().join(format!("codex-jwt-account-{}.json", std::process::id()));
    std::fs::write(&path, stringify_lossless(&JsValue::Array(failures.clone()))).unwrap();
    assert!(
        failures.is_empty(),
        "{} JWT differences; {}",
        failures.len(),
        path.display()
    );
}

#[tokio::test]
async fn codex_header_construction_matches_frozen_validation_merge_and_deletion() {
    use pi_js_compat::{
        js_json,
        js_json::{parse_lossless, stringify_lossless},
    };
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let mut failures = Vec::new();
    for (initial, additional) in [
        ("{}", "{}"),
        (r#"{"X-Custom":" a\t "}"#, "{}"),
        (r#"{"X-Custom":"a","x-custom":"b"}"#, "{}"),
        (r#"{"X-Custom":"a","x-custom":"b"}"#, r#"{"X-CUSTOM":" c "}"#),
        (r#"{"X-Custom":"a"}"#, r#"{"x-custom":null}"#),
        (r#"{"x":"\u00a0a\u00a0"}"#, "{}"),
        (r#"{"x":"a\nb"}"#, "{}"),
        (r#"{"x":"a\u0000b"}"#, "{}"),
        (r#"{"x":"a\u0001b"}"#, "{}"),
        (r#"{"x":"\u0100"}"#, "{}"),
        (r#"{"x":"\ud800"}"#, "{}"),
        (r#"{"x y":"v"}"#, "{}"),
        (r#"{"":"v"}"#, "{}"),
        (r#"{"é":"v"}"#, "{}"),
        (r#"{"\u0100":"v"}"#, "{}"),
        (r#"{"authorization":"a\nb"}"#, "{}"),
        (r#"{"accept":"a\nb"}"#, "{}"),
        ("{}", r#"{"x":"a\nb"}"#),
        ("{}", r#"{"x":"a\u0000b"}"#),
        ("{}", r#"{"x y":null}"#),
        ("{}", r#"{"x y":"v"}"#),
        ("{}", r#"{"authorization":"a\nb"}"#),
        (r#"{"x y":"v","later":"\u0100"}"#, "{}"),
        (r#"{"x y":"\u0100"}"#, "{}"),
        (r#"{"Set-Cookie":"a","set-cookie":"b"}"#, "{}"),
        (r#"{"100":"bad100\nvalue","1":"bad1\nvalue"}"#, "{}"),
        ("{}", r#"{"100":"bad100\nvalue","1":"bad1\nvalue"}"#),
    ] {
        let initial = parse_lossless(initial).unwrap();
        let additional = parse_lossless(additional).unwrap();
        let expected = oracle.request(&js_json!({"id":"header-construction","operation":"sse","events":text_events(),"modelOverrides":{"headers":initial},"options":{"headers":additional},"maxRetries":0}));
        assert_eq!(expected["ok"], JsValue::Bool(true));
        let fetch = RecordingFetch::new(vec![ScriptedResponse::sse(&[&sse_payload(&text_events())])]);
        let mut opts = options(fetch.clone(), Transport::Sse, Some(SESSION_ID), None);
        opts.stream.request.headers = Some(
            additional
                .as_object()
                .unwrap()
                .iter()
                .map(|(name, value)| (name.to_utf8().unwrap(), value.as_string().cloned()))
                .collect(),
        );
        opts.stream.request.max_retries = Some(0.0);
        let mut test_model = model("https://chatgpt.com/backend-api");
        test_model.headers = Some(
            initial
                .as_object()
                .unwrap()
                .iter()
                .map(|(name, value)| (name.to_utf8().unwrap(), value.as_string().unwrap().clone()))
                .collect(),
        );
        let (events, result) = collect_lossless(stream(test_model, context(), opts)).await;
        let requests = JsValue::from(Value::Array(fetch.requests().iter().map(request_value).collect()));
        if events != expected["output"]["events"]
            || result != expected["output"]["result"]
            || requests != expected["output"]["requests"]
        {
            failures.push(js_json!({"initial":initial,"additional":additional,"expected":expected["output"],"actual":{"events":events,"result":result,"requests":requests}}));
        }
    }
    let path = std::env::temp_dir().join(format!("codex-header-construction-{}.json", std::process::id()));
    std::fs::write(&path, stringify_lossless(&JsValue::Array(failures.clone()))).unwrap();
    assert!(
        failures.is_empty(),
        "{} header differences; {}",
        failures.len(),
        path.display()
    );
}

#[tokio::test]
async fn codex_cache_retention_none_preserves_frozen_session_affinity() {
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let expected = oracle.request(&json!({"id":"cache-retention-none","operation":"sse","events":text_events(),"options":{"cacheRetention":"none"},"maxRetries":0}));
    assert_eq!(expected["ok"], true);
    let fetch = RecordingFetch::new(vec![ScriptedResponse::sse(&[&sse_payload(&text_events())])]);
    let mut opts = options(fetch.clone(), Transport::Sse, Some(SESSION_ID), None);
    opts.stream.cache_retention = Some(pi_ai::types::CacheRetention::None);
    opts.stream.request.max_retries = Some(0.0);
    let (events, result) = collect(stream(model("https://chatgpt.com/backend-api"), context(), opts)).await;
    let requests = Value::Array(fetch.requests().iter().map(request_value).collect());
    assert_eq!(requests, expected["output"]["requests"]);
    assert_events(&events, &expected["output"]["events"], "cache-retention-none");
    assert_eq!(result, expected["output"]["result"]);
}

#[tokio::test]
async fn codex_session_headers_keep_raw_id_and_only_clamp_the_body_cache_key() {
    use pi_js_compat::{js_json, js_json::JsString};
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let mut failures = Vec::new();
    for session_id in [
        JsString::from(""),
        JsString::from(" "),
        JsString::from("s".repeat(72)),
        JsString::from("s".repeat(64)) + JsString::from_utf16(vec![0xd800]),
    ] {
        let expected = oracle.request(&js_json!({"id":"session-boundaries","operation":"sse","events":text_events(),"options":{"sessionId":session_id},"maxRetries":0}));
        assert_eq!(expected["ok"], JsValue::Bool(true));
        let fetch = RecordingFetch::new(vec![ScriptedResponse::sse(&[&sse_payload(&text_events())])]);
        let mut opts = options(fetch.clone(), Transport::Sse, None, None);
        opts.stream.session_id = Some(session_id.clone());
        opts.stream.request.max_retries = Some(0.0);
        let (events, result) =
            collect_lossless(stream(model("https://chatgpt.com/backend-api"), context(), opts)).await;
        let requests = JsValue::from(Value::Array(fetch.requests().iter().map(request_value).collect()));
        if requests != expected["output"]["requests"]
            || events != expected["output"]["events"]
            || result != expected["output"]["result"]
        {
            failures.push(js_json!({"sessionId":session_id,"expected":expected["output"],"actual":{"requests":requests,"events":events,"result":result}}));
        }
    }
    let path = std::env::temp_dir().join(format!("codex-session-boundaries-{}.json", std::process::id()));
    std::fs::write(
        &path,
        pi_js_compat::js_json::stringify_lossless(&JsValue::Array(failures.clone())),
    )
    .unwrap();
    assert!(
        failures.is_empty(),
        "{} session differences; {}",
        failures.len(),
        path.display()
    );
}

#[tokio::test]
async fn response_and_retry_raw_rejections_follow_frozen_catch_boundaries() {
    use pi_js_compat::{js_json, js_json::parse_lossless};
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    for (boundary, status, max_retries) in [("response", 200, 0), ("retry-outer", 403, 1), ("retry-inner", 503, 1)] {
        for raw in [
            "null",
            "false",
            "0",
            r#""""#,
            r#""human\ud800error""#,
            r#"{"message":"","status":403,"body":"denied"}"#,
            r#"{"message":"plain object","status":403,"body":"denied"}"#,
            r#"{"toString":null}"#,
        ] {
            let rejection = parse_lossless(raw).unwrap();
            let mut request = js_json!({"id":"raw-callbacks","operation":"sse","events":text_events(),"retryStatus":status,"retryBody":"forbidden","retryHeaders":[["retry-after-ms","0"]],"maxRetries":max_retries});
            let field = if boundary == "response" {
                "onResponseRejection"
            } else {
                "onRetryRejection"
            };
            request.as_object_mut().unwrap().insert(field.into(), rejection.clone());
            let expected = oracle.request(&request);
            assert_eq!(expected["ok"], JsValue::Bool(true));
            let fetch = RecordingFetch::new(vec![
                ScriptedResponse::json(status, "forbidden").with_header("retry-after-ms", "0"),
                ScriptedResponse::sse(&[&sse_payload(&text_events())]),
            ]);
            let mut opts = options(fetch.clone(), Transport::Sse, Some(SESSION_ID), None);
            opts.stream.request.max_retries = Some(max_retries as f64);
            let retries = Arc::new(std::sync::Mutex::new(Vec::<JsValue>::new()));
            let responses = Arc::new(std::sync::Mutex::new(Vec::<u16>::new()));
            let recorded = retries.clone();
            let retry_rejection = rejection.clone();
            opts.stream.on_retry = Some(Arc::new(move |event, _| {
                let mut events = recorded.lock().unwrap();
                events.push(pi_js_compat::js_json::to_value(&event).unwrap());
                if boundary != "response" && events.len() == 1 {
                    return Err(CallbackError::from_rejection(pi_js_compat::abort::AbortReason::Value(
                        retry_rejection.clone(),
                    )));
                }
                Ok(())
            }));
            let recorded = responses.clone();
            opts.stream.request.on_response = Some(Arc::new(move |response, _| {
                let mut statuses = recorded.lock().unwrap();
                statuses.push(response.status as u16);
                let error = (boundary == "response" && statuses.len() == 1)
                    .then(|| CallbackError::from_rejection(pi_js_compat::abort::AbortReason::Value(rejection.clone())));
                Box::pin(async move { error.map_or(Ok(()), Err) })
            }));
            let (events, result) = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                collect_lossless(stream(model("https://chatgpt.com/backend-api"), context(), opts)),
            )
            .await
            .expect("raw callback stream settles");
            assert_eq!(events, expected["output"]["events"], "{boundary}/{raw}");
            assert_eq!(result, expected["output"]["result"], "{boundary}/{raw}");
            assert_eq!(
                JsValue::Array(retries.lock().unwrap().clone()),
                expected["output"]["retries"],
                "{boundary}/{raw}"
            );
            assert_eq!(
                JsValue::from(json!(*responses.lock().unwrap())),
                expected["output"]["responseStatuses"],
                "{boundary}/{raw}"
            );
            assert_eq!(
                fetch.requests().len(),
                expected["output"]["requests"].as_array().unwrap().len(),
                "{boundary}/{raw}"
            );
        }
    }
}

#[tokio::test]
async fn payload_callback_rejections_use_frozen_provider_error_normalization() {
    use pi_js_compat::{js_json, js_json::parse_lossless};
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    for raw in [
        "null",
        "false",
        "17",
        "[1,null,2]",
        r#""human\ud800error""#,
        r#"{"message":"plain object"}"#,
        r#"{"message":17}"#,
        r#"{"status":403,"message":"denied","body":"response body"}"#,
    ] {
        let rejection = parse_lossless(raw).unwrap();
        let expected = oracle.request(
            &js_json!({"id":"payload-rejection","operation":"sse","events":[],"onPayloadRejection":rejection}),
        );
        assert_eq!(expected["ok"], JsValue::Bool(true));
        let fetch = RecordingFetch::new(Vec::new());
        let mut opts = options(fetch.clone(), Transport::Sse, Some(SESSION_ID), None);
        opts.stream.request.on_payload = Some(Arc::new(move |_, _| {
            let rejection = rejection.clone();
            Box::pin(async move {
                Err(CallbackError::from_rejection(pi_js_compat::abort::AbortReason::Value(
                    rejection,
                )))
            })
        }));
        let (events, result) =
            collect_lossless(stream(model("https://chatgpt.com/backend-api"), context(), opts)).await;
        assert_eq!(events, expected["output"]["events"], "rejection {raw}");
        assert_eq!(result, expected["output"]["result"], "rejection {raw}");
        assert!(fetch.requests().is_empty());
        assert_eq!(expected["output"]["requests"], JsValue::Array(Vec::new()));
    }
}

// Frozen370–387 awaits fetch; a transport ignoring its signal may still succeed.
#[tokio::test]
async fn header_timeout_does_not_race_a_fetch_implementation_ignoring_abort() {
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let expected=oracle.request(&json!({"id":"ignoring-signal","operation":"sse","events":text_events(),"timeoutMs":5,"responseDelayMs":25,"maxRetries":0}));
    assert_eq!(expected["ok"], true, "{expected}");
    assert_eq!(expected["output"]["result"]["stopReason"], "stop");
    let fetch = Arc::new(DelayedIgnoringSignalFetch {
        inner: sse_fetch(&text_events()),
    });
    let opts = options(fetch, Transport::Sse, Some(SESSION_ID), Some(5.0));
    let (events, result) = collect(stream(model("https://chatgpt.com/backend-api"), context(), opts)).await;
    assert_events(&events, &expected["output"]["events"], "ignoring-signal");
    assert_eq!(result, expected["output"]["result"]);
}

#[tokio::test]
async fn http_error_ids_and_raw_json_coercions_match_frozen_without_utf16_projection() {
    use pi_js_compat::{
        js_json,
        js_json::{JsValue, stringify_lossless},
    };
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let mut failures = Vec::new();
    for (status, body, request_id) in [
        (400u16, "not JSON", None),
        (
            401,
            r#"{"error":{"message":"denied"},"request_id":"body-id"}"#,
            Some(" header-id "),
        ),
        (
            401,
            r#"{"headers":{"X-Request-ID":" inner-id "},"error":{"message":"denied"}}"#,
            None,
        ),
        (
            401,
            r#"{"request_id":"root-id","error":{"request_id":"error-id","message":"denied"},"response":{"request_id":"response-id"}}"#,
            None,
        ),
        (401, r#"{"error":{"request_id":"error-id","message":"denied"}}"#, None),
        (
            401,
            r#"{"response":{"error":{"request_id":"nested-response"}},"error":{"message":"denied"}}"#,
            None,
        ),
        (400, r#"{"error":{"message":17}}"#, None),
        (400, r#"{"error":{"message":true}}"#, None),
        (400, r#"{"error":{"message":false}}"#, None),
        (400, r#"{"error":{"message":{}}}"#, None),
        (400, r#"{"error":{"message":{"toString":0}}}"#, None),
        (400, r#"{"error":{"code":["usage_not_included"]}}"#, None),
        (
            400,
            r#"{"error":{"code":{"toString":0},"message":"ignored on coercion failure"}}"#,
            None,
        ),
        (
            429,
            r#"{"error":{"code":"rate_limit_exceeded","plan_type":"PRO","resets_at":0}}"#,
            None,
        ),
        (
            429,
            r#"{"error":{"code":"rate_limit_exceeded","plan_type":"\ud800A"}}"#,
            None,
        ),
        (429, r#"{"error":{"resets_at":"123"}}"#, None),
        (429, r#"{"error":{"resets_at":[123]}}"#, None),
        (429, r#"{"error":{"resets_at":{}}}"#, None),
        (
            429,
            r#"{"error":{"resets_at":{"toString":0}},"request_id":"before-error"}"#,
            None,
        ),
        (
            429,
            r#"{"error":{"plan_type":17,"message":"not reached"},"request_id":"before-error"}"#,
            None,
        ),
        (429, r#"{"error":false}"#, None),
        (429, r#"{"error":0}"#, None),
        (429, r#"{"error":""}"#, None),
        (429, r#"{"error":null}"#, None),
        (429, r#"{"error":17}"#, None),
        (400, "null", None),
    ] {
        let headers: Vec<_> = request_id.into_iter().map(|id| ("x-request-id", id)).collect();
        let expected=oracle.request(&js_json!({"id":"http-error-fields","operation":"sse","events":[],"retryStatus":status,"retryBody":body,"retryHeaders":headers,"maxRetries":0}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let mut response = ScriptedResponse::json(status, body);
        for (name, value) in headers {
            response = response.with_header(name, value);
        }
        let fetch = RecordingFetch::new(vec![response]);
        let mut opts = options(fetch, Transport::Sse, Some(SESSION_ID), None);
        opts.stream.request.max_retries = Some(0.0);
        let (events, result) =
            collect_lossless(stream(model("https://chatgpt.com/backend-api"), context(), opts)).await;
        if events != expected["output"]["events"] || result != expected["output"]["result"] {
            failures.push(js_json!({"status":status,"body":body,"actualEvents":events,"expectedEvents":expected["output"]["events"],"actual":result,"expected":expected["output"]["result"]}));
        }
    }
    if !failures.is_empty() {
        let path = std::env::temp_dir().join(format!("codex-http-fields-{}.json", std::process::id()));
        std::fs::write(&path, stringify_lossless(&JsValue::Array(failures.clone()))).unwrap();
        eprintln!("Full Codex HTTP differences: {}", path.display());
    }
    assert!(
        failures.is_empty(),
        "{} differing cases: {}",
        failures.len(),
        failures.first().map(stringify_lossless).unwrap_or_default()
    );
}

#[tokio::test]
async fn codex_error_events_preserve_frozen_request_ids_truthiness_and_human_utf16() {
    use pi_js_compat::{
        js_json,
        js_json::{JsValue, parse_lossless, stringify_lossless},
    };
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let mut failures = Vec::new();
    for (raw, header) in [
        (r#"{"type":"error","message":"denied","request_id":"event-id"}"#, None),
        (
            r#"{"type":"error","message":"denied","request_id":"event-id"}"#,
            Some(" http-id "),
        ),
        (
            r#"{"type":"error","message":"denied","headers":{"X-Request-ID":" event-header "},"request_id":"root-id"}"#,
            None,
        ),
        (r#"{"type":"error","message":"","code":"some_code"}"#, None),
        (r#"{"type":"error","message":"","code":""}"#, None),
        (
            r#"{"type":"error","message":"","code":"","error":{"code":"nested","message":"nested"}}"#,
            None,
        ),
        (r#"{"type":"error","message":true,"error":{"message":"nested"}}"#, None),
        (
            r#"{"type":"error","message":"human\ud800text","error":{"request_id":"req\udc00"}}"#,
            None,
        ),
        (
            r#"{"type":"response.failed","response":{"error":{"message":"","code":"empty"}}}"#,
            None,
        ),
        (
            r#"{"type":"response.failed","response":{"error":{"message":17}}}"#,
            None,
        ),
        (
            r#"{"type":"response.failed","response":{"error":{"message":{}}}}"#,
            None,
        ),
        (
            r#"{"type":"response.failed","response":{"error":{"message":{"toString":0}}}}"#,
            None,
        ),
        (
            r#"{"type":"response.failed","response":{"error":{"message":"failed","request_id":"nested-response"}}}"#,
            None,
        ),
        (
            r#"{"type":"response.failed","response":{"error":{"message":"failed","request_id":"nested-response"}}}"#,
            Some("outer-id"),
        ),
        ("null", None),
    ] {
        let event = parse_lossless(raw).unwrap();
        let headers: Vec<_> = header.into_iter().map(|id| ("x-request-id", id)).collect();
        let expected=oracle.request(&js_json!({"id":"event-errors","operation":"sse","events":[event],"responseHeaders":headers,"maxRetries":0}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
        let mut response = ScriptedResponse::sse(&[&format!("data: {raw}\n\n")]);
        for (name, value) in headers {
            response = response.with_header(name, value);
        }
        let fetch = RecordingFetch::new(vec![response]);
        let mut opts = options(fetch, Transport::Sse, Some(SESSION_ID), None);
        opts.stream.request.max_retries = Some(0.0);
        let (events, result) =
            collect_lossless(stream(model("https://chatgpt.com/backend-api"), context(), opts)).await;
        if events != expected["output"]["events"] || result != expected["output"]["result"] {
            failures.push(js_json!({"event":event,"header":header,"actualEvents":events,"expectedEvents":expected["output"]["events"],"actual":result,"expected":expected["output"]["result"]}));
        }
    }
    assert!(
        failures.is_empty(),
        "{} differences: {}",
        failures.len(),
        stringify_lossless(&JsValue::Array(failures))
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
    for (missing_context, reorder_payload, no_retention_hint, second_account) in [
        (false, Some("input"), false, None),
        (false, Some("body"), false, None),
        (false, None, false, None),
        (true, None, false, None),
        (false, None, true, None),
        (false, None, false, Some("acc_other")),
    ] {
        reset_openai_codex_websocket_debug_stats(Some(&pi_js_compat::js_json::JsString::from("diff-ws-cached")));
        close_openai_codex_websocket_sessions(Some(&pi_js_compat::js_json::JsString::from("diff-ws-cached")));
        let script: ServerScript = Arc::new(move |connection, frame| {
            if connection == 0 && frame == 0 {
                completed_event("resp_1", Some("Hello"), None)
            } else if missing_context && connection == 0 && frame == 1 {
                vec![
                    json!({"type":"error","code":"previous_response_not_found","message":"context gone","request_id":"ws_context"}),
                ]
            } else {
                completed_event("resp_2", Some("Done"), None)
            }
        });

        let oracle_server = start_ws_server(script.clone()).await;
        let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
        let mut oracle_request = json!({
            "id": "ws_cached", "operation": "ws_cached",
            "wsUrl": oracle_server.url, "reorderSecondPayload": reorder_payload,
            "secondAccountId": second_account
        });
        if no_retention_hint {
            oracle_request["cacheRetention"] = json!("none");
        }
        let oracle_output = oracle.request(&oracle_request);
        assert_eq!(oracle_output["ok"], true, "oracle failed: {oracle_output}");
        let expected = &oracle_output["output"];
        let oracle_records = snapshot(oracle_server.records.connections.lock().unwrap().as_slice());
        drop(oracle_server);

        let rust_server = start_ws_server(script).await;
        let first_context = context();
        let mut first_options = options(
            RecordingFetch::new(Vec::new()),
            Transport::WebsocketCached,
            Some("diff-ws-cached"),
            None,
        );
        first_options.stream.cache_retention = no_retention_hint.then_some(pi_ai::types::CacheRetention::None);
        let first = collect(stream(model(&rust_server.url), first_context.clone(), first_options)).await;
        let mut second_context = first_context;
        second_context.messages.push(pi_ai::types::Message::Assistant(Box::new(
            serde_json::from_value(first.1.clone()).expect("assistant message"),
        )));
        second_context.messages.push(
            serde_json::from_value(json!({"role": "user", "content": "Now finish", "timestamp": 2})).expect("user"),
        );
        let mut second_options = options(
            RecordingFetch::new(Vec::new()),
            Transport::WebsocketCached,
            Some("diff-ws-cached"),
            None,
        );
        second_options.stream.cache_retention = no_retention_hint.then_some(pi_ai::types::CacheRetention::None);
        if let Some(account) = second_account {
            let payload = json!({"https://api.openai.com/auth": {"chatgpt_account_id": account}});
            let encoded = pi_js_compat::base64::btoa(payload.to_string().as_bytes());
            second_options.stream.request.api_key = Some(format!("aaa.{encoded}.bbb").into());
        }
        second_options.stream.request.on_payload = Some(Arc::new(move |mut body, _| {
            Box::pin(async move {
                if reorder_payload == Some("body") {
                    body = reverse_object_properties(&body);
                } else if reorder_payload == Some("input") {
                    let input = body
                        .as_object_mut()
                        .unwrap()
                        .get_mut("input")
                        .unwrap()
                        .as_array_mut()
                        .unwrap();
                    input[0] = reverse_object_properties(&input[0]);
                }
                Ok(Some(body))
            })
        }));
        let second = collect(stream(model(&rust_server.url), second_context, second_options)).await;
        let rust_records = snapshot(rust_server.records.connections.lock().unwrap().as_slice());

        assert_eq!(
            rust_records, oracle_records,
            "websocket frames/headers mismatch, missing_context={missing_context}, reorder_payload={reorder_payload:?}, no_retention_hint={no_retention_hint}, second_account={second_account:?}"
        );
        assert_events(&first.0, &expected["firstEvents"], "first");
        assert_eq!(first.1, expected["first"], "first result mismatch");
        assert_events(&second.0, &expected["secondEvents"], "second");
        assert_eq!(second.1, expected["second"], "second result mismatch");
        close_openai_codex_websocket_sessions(Some(&pi_js_compat::js_json::JsString::from("diff-ws-cached")));
        reset_openai_codex_websocket_debug_stats(Some(&pi_js_compat::js_json::JsString::from("diff-ws-cached")));
    }
}

#[tokio::test]
async fn websocket_accounts_add_reuse_expire_and_close_independently() {
    // Approved delta of frozen900/935-950/1035-1044/1146-1208: each account
    // retains the same lifecycle; only cache identity changes.
    let session = pi_js_compat::js_json::JsString::from("diff-ws-accounts");
    close_openai_codex_websocket_sessions(Some(&session));
    reset_openai_codex_websocket_debug_stats(Some(&session));
    let script: ServerScript =
        Arc::new(|connection, frame| completed_event(&format!("resp_{connection}_{frame}"), Some("Hello"), None));
    let oracle_server = start_ws_server(script.clone()).await;
    let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
    let oracle_url = oracle_server.url.clone();
    let response = tokio::task::spawn_blocking(move || {
        oracle.request(&json!({
            "id": "account-lifecycle", "operation": "ws_accounts", "wsUrl": oracle_url
        }))
    })
    .await
    .unwrap();
    assert_eq!(response["ok"], true, "oracle failed: {response}");
    wait_for_account_closes(&oracle_server).await;
    let expected_records = snapshot(oracle_server.records.connections.lock().unwrap().as_slice());
    let expected_closes = account_close_records(&oracle_server);
    let server = start_ws_server(script).await;
    let mut outputs = Vec::new();
    for step in [
        "acc_a", "150000", "acc_b", "acc_a", "150000", "acc_b", "150000", "acc_b", "acc_a", "300000", "acc_b", "acc_a",
        "close", "acc_b", "acc_a",
    ] {
        if let Ok(milliseconds) = step.parse::<u64>() {
            tokio::time::pause();
            tokio::time::advance(std::time::Duration::from_millis(milliseconds)).await;
            tokio::task::yield_now().await;
            tokio::time::resume();
            continue;
        }
        if step == "close" {
            close_openai_codex_websocket_sessions(Some(&session));
            continue;
        }
        let mut options = options(
            RecordingFetch::new(Vec::new()),
            Transport::WebsocketCached,
            Some("diff-ws-accounts"),
            None,
        );
        let payload = json!({"https://api.openai.com/auth": {"chatgpt_account_id": step}});
        let encoded = pi_js_compat::base64::btoa(payload.to_string().as_bytes());
        options.stream.request.api_key = Some(format!("aaa.{encoded}.bbb").into());
        let (events, result) = collect(stream(model(&server.url), context(), options)).await;
        outputs.push(json!({"events":events,"result":result,"stats":get_openai_codex_websocket_debug_stats(&session)}));
    }
    close_openai_codex_websocket_sessions(Some(&session));
    wait_for_account_closes(&server).await;
    assert_eq!(outputs, response["output"].as_array().unwrap().clone());
    assert_eq!(
        snapshot(server.records.connections.lock().unwrap().as_slice()),
        expected_records
    );
    assert_eq!(account_close_records(&server), expected_closes);
    assert_eq!(
        expected_closes,
        json!([
            "idle_timeout",
            "idle_timeout",
            "idle_timeout",
            "debug_close",
            "debug_close",
            "debug_close",
            "debug_close"
        ])
    );
    reset_openai_codex_websocket_debug_stats(Some(&session));
}

#[tokio::test]
async fn websocket_busy_entry_survives_idle_ttl_then_expires_at_maximum_age() {
    // Frozen1152 does not age-evict a busy entry; the next idle acquisition
    // closes it with connection_age_limit after55minutes. No private cache mutation.
    let session = pi_js_compat::js_json::JsString::from("diff-ws-age");
    let script: ServerScript =
        Arc::new(|connection, frame| completed_event(&format!("age_{connection}_{frame}"), Some("Hello"), None));
    let source_gate = Arc::new(tokio::sync::Notify::new());
    let source_server = start_ws_server_with_gate(
        script.clone(),
        Some(FrameGate {
            connection: 0,
            frame: 1,
            release: source_gate.clone(),
        }),
    )
    .await;
    let source_url = source_server.url.clone();
    let source_call = tokio::task::spawn_blocking(move || {
        let mut oracle = TsOracle::spawn("openai-codex-responses-oracle.ts", SOURCE_MODULE);
        oracle.request(&json!({"id":"ws-age","operation":"ws_age","wsUrl":source_url}))
    });
    wait_for_websocket_frames(&source_server, 2).await;
    source_gate.notify_one();
    let expected = source_call.await.unwrap();
    assert_eq!(expected["ok"], true, "{expected}");
    wait_for_account_closes(&source_server).await;
    let gate = Arc::new(tokio::sync::Notify::new());
    let server = start_ws_server_with_gate(
        script,
        Some(FrameGate {
            connection: 0,
            frame: 1,
            release: gate.clone(),
        }),
    )
    .await;
    let call = || {
        stream(
            model(&server.url),
            context(),
            options(
                RecordingFetch::new(Vec::new()),
                Transport::WebsocketCached,
                Some("diff-ws-age"),
                None,
            ),
        )
    };
    let mut outputs = Vec::new();
    let first = collect(call()).await;
    outputs.push(json!({"events":first.0,"result":first.1,"stats":get_openai_codex_websocket_debug_stats(&session)}));
    let second = tokio::spawn(collect(call()));
    wait_for_websocket_frames(&server, 2).await;
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(55 * 60)).await;
    tokio::task::yield_now().await;
    tokio::time::resume();
    gate.notify_one();
    let second = second.await.unwrap();
    outputs.push(json!({"events":second.0,"result":second.1,"stats":get_openai_codex_websocket_debug_stats(&session)}));
    let third = collect(call()).await;
    outputs.push(json!({"events":third.0,"result":third.1,"stats":get_openai_codex_websocket_debug_stats(&session)}));
    close_openai_codex_websocket_sessions(Some(&session));
    wait_for_account_closes(&server).await;
    assert_eq!(outputs, expected["output"].as_array().unwrap().clone());
    assert_eq!(
        snapshot(server.records.connections.lock().unwrap().as_slice()),
        snapshot(source_server.records.connections.lock().unwrap().as_slice())
    );
    assert_eq!(account_close_records(&server), account_close_records(&source_server));
    assert_eq!(
        account_close_records(&server),
        json!(["connection_age_limit", "debug_close"])
    );
    reset_openai_codex_websocket_debug_stats(Some(&session));
}

async fn wait_for_websocket_frames(server: &ws_server::WsServer, count: usize) {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let notified = server.records.frame_notify.notified();
            let frames: usize = server
                .records
                .connections
                .lock()
                .unwrap()
                .iter()
                .map(|connection| connection.frames.len())
                .sum();
            if frames >= count {
                return;
            }
            notified.await;
        }
    })
    .await
    .expect("requested WebSocket frame count");
}

async fn wait_for_account_closes(server: &ws_server::WsServer) {
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            if server
                .records
                .connections
                .lock()
                .unwrap()
                .iter()
                .all(|connection| connection.close.is_some())
            {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("account cache sockets close");
}

fn account_close_records(server: &ws_server::WsServer) -> Value {
    let connections = server.records.connections.lock().unwrap();
    json!(
        connections
            .iter()
            .map(|connection| connection.close.as_ref().map(|(_, reason)| reason))
            .collect::<Vec<_>>()
    )
}

fn reverse_object_properties(value: &JsValue) -> JsValue {
    let properties = value.as_object().expect("fixture object");
    JsValue::Object(
        properties
            .iter()
            .rev()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
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

fn assert_diagnostic_clock(value: &Value, bounds: [i64; 2], side: &str) {
    let timestamp = value
        .as_i64()
        .expect("diagnostic timestamp must be numeric milliseconds");
    assert!(
        bounds[0] <= timestamp && timestamp <= bounds[1],
        "{side} diagnostic {timestamp} outside invocation {bounds:?}"
    );
}

fn assert_timed_diagnostic(actual: &Value, expected: &Value, native_bounds: [i64; 2], source_bounds: [i64; 2]) {
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

fn assert_timed_message(actual: &Value, expected: &Value, native_bounds: [i64; 2], source_bounds: [i64; 2]) {
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

fn assert_clock_bounded_output(events: &[Value], result: &Value, expected: &Value, native_bounds: [i64; 2]) {
    let source_bounds = [
        expected["clockBounds"][0].as_i64().unwrap(),
        expected["clockBounds"][1].as_i64().unwrap(),
    ];
    let expected_events = expected["events"].as_array().unwrap();
    assert_eq!(events.len(), expected_events.len());
    for (actual, expected) in events.iter().zip(expected_events) {
        let actual = actual.as_object().unwrap();
        let expected = expected.as_object().unwrap();
        assert_eq!(actual.keys().collect::<Vec<_>>(), expected.keys().collect::<Vec<_>>());
        for (key, value) in expected {
            if ["partial", "message", "error"].contains(&key.as_str()) {
                assert_timed_message(&actual[key], value, native_bounds, source_bounds);
            } else {
                assert_eq!(actual[key], *value, "event field {key}");
            }
        }
    }
    assert_timed_message(result, &expected["result"], native_bounds, source_bounds);
}

fn assert_timed_timeout_output(events: &[Value], result: &Value, expected: &Value, native_bounds: [i64; 2]) {
    let source_bounds = [
        expected["clockBounds"][0].as_i64().unwrap(),
        expected["clockBounds"][1].as_i64().unwrap(),
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
