use std::sync::Arc;

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
    let encoded = pi_ai::utils::base64::encode(payload.to_string().as_bytes());
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
                api_key: Some(mock_token()),
                fetch: Some(fetch),
                timeout_ms,
                ..ProviderRequestOptions::default()
            },
            transport: Some(transport),
            session_id: session_id.map(str::to_owned),
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
    let mut normalized = normalize(value);
    if normalized.get("type").and_then(Value::as_str) != Some("done")
        && let Some(partial) = normalized.get_mut("partial").and_then(Value::as_object_mut)
    {
        partial.remove("responseId");
        partial.remove("endTurn");
    }
    normalized
}

fn normalize(value: &Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.iter().map(normalize).collect()),
        Value::Object(object) => {
            let mut output = serde_json::Map::new();
            for (key, entry) in object {
                if key == "timestamp" {
                    output.insert(key.clone(), Value::from(0));
                } else if key == "stack" {
                    continue;
                } else if key == "errorMessage"
                    && entry.as_str().is_some_and(|message| {
                        message.starts_with("Invalid Codex SSE JSON:")
                            || message.starts_with("Invalid Codex WebSocket JSON:")
                    })
                {
                    output.insert(key.clone(), Value::String("<invalid json>".to_owned()));
                } else {
                    output.insert(key.clone(), normalize(entry));
                }
            }
            Value::Object(output)
        }
        other => other.clone(),
    }
}

fn sse_payload(events: &[Value]) -> String {
    events.iter().map(|event| format!("data: {event}\n\n")).collect()
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
        _signal: Option<tokio_util::sync::CancellationToken>,
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
        _signal: Option<tokio_util::sync::CancellationToken>,
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
        .map(|(name, value)| {
            let value = if name == "user-agent" { "<ua>" } else { value };
            (name.clone(), Value::String(value.to_owned()))
        })
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
    normalize(value)
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
            let token = tokio_util::sync::CancellationToken::new();
            let abort_events = vec![
                json!({"type": "response.output_item.added", "output_index": 0,
                    "item": {"type": "message", "id": "msg_1", "role": "assistant", "status": "in_progress", "content": []}}),
                json!({"type": "response.output_text.delta", "output_index": 0, "delta": "one"}),
            ];
            let fetch = RecordingFetch::with_abort_after_chunks(
                vec![ScriptedResponse::sse(&[&sse_payload(&abort_events)])],
                1,
                token.clone(),
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
    close_openai_codex_websocket_sessions(Some("diff-ws-cached"));
    reset_openai_codex_websocket_debug_stats(Some("diff-ws-cached"));
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
    assert_events(&events, &expected["events"], "events");
    assert_eq!(result, expected["result"]);
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
    let (events, result) = collect(stream(model(&format!("ws://{rust_address}")), context(), opts)).await;
    assert_events(&events, &expected["events"], "events");
    assert_eq!(result, expected["result"]);
    rust_task.abort();
}
