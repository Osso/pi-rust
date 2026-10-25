use std::sync::{Arc, Mutex};

use pi_ai::api::openai_codex_responses::{
    CodexReasoningEffort, OpenAICodexResponsesOptions, TextVerbosity, close_openai_codex_websocket_sessions,
    get_openai_codex_websocket_debug_stats, reset_openai_codex_websocket_debug_stats, stream, stream_simple,
};
use pi_ai::request::{
    BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse, ProviderRequestOptions, SimpleStreamOptions,
    StreamOptions,
};
use pi_ai::types::{AssistantMessage, AssistantMessageEvent, Context, Model, StopReason, Tool, Transport};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use serde_json::{Value, json};

// =============================================================================
// Scripted fetch
// =============================================================================

#[path = "support/recording_fetch.rs"]
mod recording_fetch;

use recording_fetch::{RecordingFetch, ScriptedResponse};

struct PendingHeaderFetch {
    requests: Mutex<Vec<HttpRequest>>,
}

impl PendingHeaderFetch {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            requests: Mutex::new(Vec::new()),
        })
    }

    fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl Fetch for PendingHeaderFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<pi_js_compat::abort::AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.requests.lock().unwrap().push(request);
        Box::pin(async move {
            match signal {
                Some(signal) => Err(CallbackError::from_rejection(signal.wait().await)),
                None => std::future::pending().await,
            }
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
        Box::pin(async {
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

// =============================================================================
// Fixtures
// =============================================================================

fn mock_token(account_id: &str) -> String {
    let payload = json!({"https://api.openai.com/auth": {"chatgpt_account_id": account_id}});
    let encoded = pi_js_compat::base64::btoa(payload.to_string().as_bytes());
    format!("aaa.{encoded}.bbb")
}

fn model() -> Model {
    serde_json::from_value(json!({
        "id": "gpt-5.1-codex",
        "name": "GPT-5.1 Codex",
        "api": "openai-codex-responses",
        "provider": "openai-codex",
        "baseUrl": "https://chatgpt.com/backend-api",
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

fn options(fetch: Arc<dyn Fetch>, transport: Transport) -> OpenAICodexResponsesOptions {
    OpenAICodexResponsesOptions {
        stream: StreamOptions {
            request: ProviderRequestOptions {
                api_key: Some((mock_token("acc_test")).into()),
                fetch: Some(fetch),
                ..ProviderRequestOptions::default()
            },
            transport: Some(transport),
            ..StreamOptions::default()
        },
        ..OpenAICodexResponsesOptions::default()
    }
}

fn sse_response(payload: &str) -> ScriptedResponse {
    ScriptedResponse::sse(&[payload])
}

fn text_sse(status: &str, end_turn: Option<bool>) -> String {
    let terminal_type = if status == "incomplete" {
        "response.incomplete"
    } else {
        "response.completed"
    };
    let events = vec![
        json!({"type": "response.output_item.added",
            "item": {"type": "message", "id": "msg_1", "role": "assistant", "status": "in_progress", "content": []}}),
        json!({"type": "response.content_part.added", "part": {"type": "output_text", "text": ""}}),
        json!({"type": "response.output_text.delta", "delta": "Hello"}),
        json!({"type": "response.output_item.done",
            "item": {"type": "message", "id": "msg_1", "role": "assistant", "status": "completed",
                "content": [{"type": "output_text", "text": "Hello"}]}}),
        json!({"type": terminal_type, "response": {
            "status": status,
            "end_turn": end_turn,
            "incomplete_details": if status == "incomplete" { json!({"reason": "max_output_tokens"}) } else { Value::Null },
            "usage": {"input_tokens": 5, "output_tokens": 3, "total_tokens": 8,
                "input_tokens_details": {"cached_tokens": 0}}
        }}),
    ];
    let mut payload = events
        .into_iter()
        .map(|event| format!("data: {event}"))
        .collect::<Vec<_>>()
        .join("\n\n");
    payload.push_str("\n\n");
    payload
}

async fn collect(mut stream: AssistantMessageEventStream) -> (Vec<AssistantMessageEvent>, AssistantMessage) {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.expect("stream event"));
    }
    let result = stream.result().await.expect("stream result");
    (events, result)
}

fn header<'a>(request: &'a HttpRequest, name: &str) -> Option<&'a str> {
    request
        .headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

fn decode_body(request: &HttpRequest) -> Value {
    let body = request.body.as_deref().expect("request body");
    let decoded = if header(request, "content-encoding") == Some("zstd") {
        zstd::stream::decode_all(body).expect("zstd body")
    } else {
        body.to_vec()
    };
    serde_json::from_slice(&decoded).expect("json body")
}

// =============================================================================
// SSE behavior
// =============================================================================

#[tokio::test]
async fn streams_sse_text_and_codex_headers() {
    let fetch = RecordingFetch::new(vec![sse_response(&text_sse("completed", None))]);
    let mut opts = options(fetch.clone(), Transport::Sse);
    opts.stream.session_id = Some("test-session-123".into());

    let (events, result) = collect(stream(model(), context(), opts)).await;

    assert!(
        events
            .iter()
            .any(|event| matches!(event, AssistantMessageEvent::TextDelta { delta, .. } if delta == "Hello"))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AssistantMessageEvent::Done { .. }))
    );
    assert_eq!(result.stop_reason, StopReason::Stop);
    assert_eq!(
        result.content.first().and_then(|block| match block {
            pi_ai::types::AssistantContent::Text(text) => Some(&text.text),
            _ => None,
        }),
        Some(&pi_js_compat::js_json::JsString::from("Hello"))
    );

    let requests = fetch.requests();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.url, "https://chatgpt.com/backend-api/codex/responses");
    let expected_authorization = format!("Bearer {}", mock_token("acc_test"));
    assert_eq!(header(request, "authorization"), Some(expected_authorization.as_str()));
    assert_eq!(header(request, "chatgpt-account-id"), Some("acc_test"));
    assert_eq!(header(request, "openai-beta"), Some("responses=experimental"));
    assert_eq!(header(request, "originator"), Some("pi"));
    assert_eq!(header(request, "accept"), Some("text/event-stream"));
    assert_eq!(header(request, "session-id"), Some("test-session-123"));
    assert_eq!(header(request, "x-client-request-id"), Some("test-session-123"));
    assert_eq!(header(request, "x-api-key"), None);

    let body = decode_body(request);
    assert_eq!(body["prompt_cache_key"], "test-session-123");
    assert_eq!(body["model"], "gpt-5.1-codex");
    assert_eq!(body["store"], false);
    assert_eq!(body["stream"], true);
    assert_eq!(body["text"]["verbosity"], "low");
    assert_eq!(body["tool_choice"], "auto");
    assert_eq!(body["include"][0], "reasoning.encrypted_content");
}

#[tokio::test]
async fn reports_sse_http_retries_through_on_retry() {
    let fetch = RecordingFetch::new(vec![
        ScriptedResponse::json(500, "server error").with_header("retry-after-ms", "0"),
        sse_response(&text_sse("completed", None)),
    ]);
    let retries = Arc::new(Mutex::new(Vec::new()));
    let captured = retries.clone();
    let mut opts = options(fetch.clone(), Transport::Sse);
    opts.stream.request.max_retries = Some(1.0);
    opts.stream.on_retry = Some(Arc::new(move |event, _model| {
        captured.lock().unwrap().push(event);
        Ok(())
    }));

    let (_, result) = collect(stream(model(), context(), opts)).await;

    assert_eq!(result.stop_reason, StopReason::Stop);
    assert_eq!(fetch.requests().len(), 2);
    let retries = retries.lock().unwrap();
    assert_eq!(retries.len(), 1);
    assert_eq!(retries[0].attempt, 1.0);
    assert_eq!(retries[0].max_attempts, Some(2.0));
    assert_eq!(retries[0].delay_ms, Some(0.0));
    assert_eq!(retries[0].reason, "HTTP 500");
}

#[tokio::test]
async fn extracts_account_id_from_unpadded_jwt_payload() {
    // JWT segments drop `=` padding; `atob` accepts the unpadded payload.
    let padded = mock_token("acc_x");
    let token = padded.replace('=', "");
    assert_ne!(token, padded, "fixture payload must need padding");
    let fetch = RecordingFetch::new(vec![sse_response(&text_sse("completed", None))]);
    let mut opts = options(fetch.clone(), Transport::Sse);
    opts.stream.request.api_key = Some((token).into());

    let (_, result) = collect(stream(model(), context(), opts)).await;

    assert_eq!(result.stop_reason, StopReason::Stop);
    assert_eq!(header(&fetch.requests()[0], "chatgpt-account-id"), Some("acc_x"));
}

#[tokio::test]
async fn empty_retry_after_ms_header_retries_immediately() {
    // `Number("")` is 0, so the source retries with a zero delay.
    let fetch = RecordingFetch::new(vec![
        ScriptedResponse::json(500, "server error").with_header("retry-after-ms", ""),
        sse_response(&text_sse("completed", None)),
    ]);
    let retries = Arc::new(Mutex::new(Vec::new()));
    let captured = retries.clone();
    let mut opts = options(fetch.clone(), Transport::Sse);
    opts.stream.request.max_retries = Some(1.0);
    opts.stream.on_retry = Some(Arc::new(move |event, _model| {
        captured.lock().unwrap().push(event);
        Ok(())
    }));

    let (_, result) = collect(stream(model(), context(), opts)).await;

    assert_eq!(result.stop_reason, StopReason::Stop);
    assert_eq!(retries.lock().unwrap()[0].delay_ms, Some(0.0));
}

#[tokio::test]
async fn completes_after_response_completed_while_body_stays_open() {
    let payload = text_sse("completed", Some(false));
    let fetch = PendingBodyFetch::new(payload.into_bytes());

    let result = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        collect(stream(model(), context(), options(fetch, Transport::Sse))).await
    })
    .await
    .expect("stream completes without body EOF")
    .1;

    assert_eq!(result.stop_reason, StopReason::Stop);
    assert!(serde_json::to_value(&result).unwrap().get("endTurn").is_none());
}

#[tokio::test]
async fn maps_incomplete_max_output_to_length() {
    let fetch = PendingBodyFetch::new(text_sse("incomplete", None).into_bytes());

    let result = tokio::time::timeout(std::time::Duration::from_secs(1), async {
        collect(stream(model(), context(), options(fetch, Transport::Sse))).await
    })
    .await
    .expect("stream completes")
    .1;

    assert_eq!(result.stop_reason, StopReason::Length);
    assert_eq!(result.error_message, None);
    assert!(serde_json::to_value(&result).unwrap().get("rawStopReason").is_none());
}

#[tokio::test]
async fn header_timeout_reports_configured_timeout() {
    let fetch = PendingHeaderFetch::new();
    let mut opts = options(fetch.clone(), Transport::Sse);
    opts.stream.request.timeout_ms = Some(10.0);

    let (_, result) = collect(stream(model(), context(), opts)).await;

    assert_eq!(result.stop_reason, StopReason::Error);
    assert_eq!(
        result
            .error_message
            .as_ref()
            .map(|text| text.to_utf8().unwrap())
            .as_deref(),
        Some("Codex SSE response headers timed out after 10ms")
    );
    assert_eq!(fetch.requests().len(), 1);
}

#[tokio::test]
async fn body_read_timeout_reports_configured_timeout() {
    let fetch = PendingBodyFetch::new(Vec::new());
    let mut opts = options(fetch, Transport::Sse);
    opts.stream.request.timeout_ms = Some(20.0);

    let (_, result) = collect(stream(model(), context(), opts)).await;

    assert_eq!(result.stop_reason, StopReason::Error);
    assert_eq!(
        result
            .error_message
            .as_ref()
            .map(|text| text.to_utf8().unwrap())
            .as_deref(),
        Some("Codex SSE body read timed out after 20ms")
    );
}

#[tokio::test]
async fn abort_mid_stream_stops_after_delivered_delta() {
    let payload = format!(
        "data: {}\n\ndata: {}\n\n",
        json!({"type": "response.output_item.added",
            "item": {"type": "message", "id": "msg_1", "role": "assistant", "status": "in_progress", "content": []}}),
        json!({"type": "response.output_text.delta", "delta": "one"})
    );
    let token_controller = pi_js_compat::abort::AbortController::new();
    let token = token_controller.signal();
    let fetch =
        RecordingFetch::with_abort_after_chunks(vec![ScriptedResponse::sse(&[&payload])], 1, token_controller.clone());
    let mut opts = options(fetch, Transport::Sse);
    opts.stream.request.signal = Some(token);

    let mut stream = stream(model(), context(), opts);
    let mut deltas = Vec::new();
    while let Some(event) = stream.next().await {
        if let AssistantMessageEvent::TextDelta { delta, .. } = event.expect("event") {
            deltas.push(delta);
        }
    }
    let result = stream.result().await.expect("result");

    assert_eq!(deltas, vec!["one".to_owned()]);
    assert_eq!(result.stop_reason, StopReason::Aborted);
    assert_eq!(
        result
            .error_message
            .as_ref()
            .map(|text| text.to_utf8().unwrap())
            .as_deref(),
        Some("Request was aborted")
    );
}

#[tokio::test]
async fn uses_raw_session_headers_and_keeps_affinity_for_cache_retention_none() {
    let fetch = RecordingFetch::new(vec![
        sse_response(&text_sse("completed", None)),
        sse_response(&text_sse("completed", None)),
    ]);
    let long_session = "x".repeat(67);
    let mut opts = options(fetch.clone(), Transport::Sse);
    opts.stream.session_id = Some(long_session.clone().into());
    collect(stream(model(), context(), opts)).await;

    let mut opts = options(fetch.clone(), Transport::Sse);
    opts.stream.session_id = Some("one-off-summary".into());
    opts.stream.cache_retention = Some(pi_ai::types::CacheRetention::None);
    collect(stream(model(), context(), opts)).await;

    let requests = fetch.requests();
    assert_eq!(header(&requests[0], "session-id"), Some(long_session.as_str()));
    assert_eq!(header(&requests[0], "x-client-request-id"), Some(long_session.as_str()));
    assert_eq!(decode_body(&requests[0])["prompt_cache_key"], "x".repeat(64));
    assert_eq!(header(&requests[1], "session-id"), Some("one-off-summary"));
    assert_eq!(header(&requests[1], "x-client-request-id"), Some("one-off-summary"));
    assert_eq!(decode_body(&requests[1])["prompt_cache_key"], "one-off-summary");
}

#[tokio::test]
async fn stream_simple_preserves_xhigh_and_clamps_minimal() {
    let fetch = RecordingFetch::new(vec![
        sse_response(&text_sse("completed", None)),
        sse_response(&text_sse("completed", None)),
    ]);
    let mut xhigh_model = model();
    xhigh_model.id = "gpt-5.5".to_owned();
    xhigh_model.thinking_level_map = Some(
        [(pi_ai::types::ModelThinkingLevel::Xhigh, Some("xhigh".into()))]
            .into_iter()
            .collect(),
    );
    let simple = SimpleStreamOptions {
        stream: StreamOptions {
            request: ProviderRequestOptions {
                api_key: Some((mock_token("acc_test")).into()),
                fetch: Some(fetch.clone()),
                ..ProviderRequestOptions::default()
            },
            transport: Some(Transport::Sse),
            ..StreamOptions::default()
        },
        reasoning: Some(pi_ai::types::ThinkingLevel::Xhigh),
        ..SimpleStreamOptions::default()
    };
    collect(stream_simple(xhigh_model, context(), simple).unwrap()).await;

    let mut minimal_model = model();
    minimal_model.id = "gpt-5.3-codex".to_owned();
    minimal_model.thinking_level_map = Some(
        [(pi_ai::types::ModelThinkingLevel::Minimal, Some("low".into()))]
            .into_iter()
            .collect(),
    );
    let simple = SimpleStreamOptions {
        stream: StreamOptions {
            request: ProviderRequestOptions {
                api_key: Some((mock_token("acc_test")).into()),
                fetch: Some(fetch.clone()),
                ..ProviderRequestOptions::default()
            },
            transport: Some(Transport::Sse),
            ..StreamOptions::default()
        },
        reasoning: Some(pi_ai::types::ThinkingLevel::Minimal),
        ..SimpleStreamOptions::default()
    };
    collect(stream_simple(minimal_model, context(), simple).unwrap()).await;

    let requests = fetch.requests();
    assert_eq!(
        decode_body(&requests[0])["reasoning"],
        json!({"effort": "xhigh", "summary": "auto"})
    );
    assert_eq!(
        decode_body(&requests[1])["reasoning"],
        json!({"effort": "low", "summary": "auto"})
    );
}

#[tokio::test]
async fn frozen_auto_tool_choice_and_null_strict_tools_ignore_the_old_hint() {
    let fetch = RecordingFetch::new(vec![sse_response(&text_sse("completed", None))]);
    let mut opts = options(fetch.clone(), Transport::Sse);
    opts.stream.extra.insert("toolChoice".into(), json!("required"));
    let mut ctx = context();
    ctx.tools = Some(vec![
        Tool::new(
            "optional",
            "Optional",
            json!({"type": "object", "properties": {"value": {"type": "string"}}}),
        ),
        Tool::new(
            "strict",
            "Strict",
            json!({"type": "object", "properties": {"value": {"type": "string"}}, "additionalProperties": false}),
        ),
    ]);

    collect(stream(model(), ctx, opts)).await;

    let body = decode_body(&fetch.requests()[0]);
    assert_eq!(body["tool_choice"], "auto");
    assert_eq!(body["tools"][0]["strict"], Value::Null);
    assert_eq!(body["tools"][0]["name"], "optional");
    assert_eq!(body["tools"][1]["strict"], Value::Null);
    assert_eq!(body["tools"][1]["name"], "strict");
}

#[tokio::test]
async fn applies_service_tier_pricing_when_codex_echoes_default() {
    let fetch = RecordingFetch::new(vec![sse_response(&format!(
        "data: {}\n\ndata: {}\n\n",
        json!({"type": "response.output_item.added",
            "item": {"type": "message", "id": "msg_1", "role": "assistant", "status": "in_progress", "content": []}}),
        json!({"type": "response.completed", "response": {
            "status": "completed",
            "service_tier": "default",
            "usage": {"input_tokens": 1000000, "output_tokens": 1000000, "total_tokens": 2000000,
                "input_tokens_details": {"cached_tokens": 0}}
        }})
    ))]);
    let mut opts = options(fetch, Transport::Sse);
    opts.service_tier = Some(pi_ai::api::openai_codex_responses::ServiceTier::Priority);

    let (_, result) = collect(stream(model(), context(), opts)).await;

    assert_eq!(result.usage.input, 1_000_000.0);
    assert_eq!(result.usage.output, 1_000_000.0);
    assert_eq!(result.usage.cost.input, 2.0);
    assert_eq!(result.usage.cost.output, 4.0);
    assert_eq!(result.usage.cost.total, 6.0);
}

#[tokio::test]
async fn zstd_compresses_sse_request_bodies() {
    let fetch = RecordingFetch::new(vec![sse_response(&text_sse("completed", None))]);
    let mut ctx = context();
    ctx.messages = serde_json::from_value(json!([
        {"role": "user", "content": "compress me ".repeat(400), "timestamp": 1}
    ]))
    .expect("messages");

    collect(stream(model(), ctx, options(fetch.clone(), Transport::Sse))).await;

    let request = &fetch.requests()[0];
    assert_eq!(header(request, "content-encoding"), Some("zstd"));
    let body = decode_body(request);
    assert_eq!(body["input"][0]["content"][0]["text"], "compress me ".repeat(400));
}

#[tokio::test(start_paused = true)]
async fn uses_exponential_backoff_across_retries() {
    let mut script = Vec::new();
    for _ in 0..3 {
        script.push(ScriptedResponse::json(
            429,
            r#"{"error":{"code":"rate_limit_exceeded","message":"rate limited"}}"#,
        ));
    }
    script.push(sse_response(&text_sse("completed", None)));
    let fetch = RecordingFetch::new(script);
    let retries = Arc::new(Mutex::new(Vec::new()));
    let captured = retries.clone();
    let mut opts = options(fetch.clone(), Transport::Sse);
    opts.stream.request.max_retries = Some(3.0);
    opts.stream.on_retry = Some(Arc::new(move |event, _model| {
        captured.lock().unwrap().push(event.delay_ms);
        Ok(())
    }));

    let (_, result) = collect(stream(model(), context(), opts)).await;

    assert_eq!(result.stop_reason, StopReason::Stop);
    assert_eq!(fetch.requests().len(), 4);
    assert_eq!(*retries.lock().unwrap(), vec![Some(1000.0), Some(2000.0), Some(4000.0)]);
}

#[tokio::test]
async fn retry_after_limits_cap_only_rate_limit_delays() {
    for (status, expected_delay) in [(429, 5.0), (503, 11.0)] {
        let fetch = RecordingFetch::new(vec![
            ScriptedResponse::json(status, "retry later").with_header("retry-after-ms", "11"),
            sse_response(&text_sse("completed", None)),
        ]);
        let mut opts = options(fetch.clone(), Transport::Sse);
        opts.stream.request.max_retries = Some(1.0);
        opts.stream.request.max_retry_delay_ms = Some(5.0);
        let delays = Arc::new(Mutex::new(Vec::new()));
        let observed = delays.clone();
        opts.stream.on_retry = Some(Arc::new(move |event, _| {
            observed.lock().unwrap().push(event.delay_ms);
            Ok(())
        }));
        let (_, result) = collect(stream(model(), context(), opts)).await;
        assert_eq!(result.stop_reason, StopReason::Stop);
        assert_eq!(fetch.requests().len(), 2);
        assert_eq!(*delays.lock().unwrap(), vec![Some(expected_delay)]);
    }
}

#[tokio::test]
async fn malformed_sse_json_is_a_protocol_error() {
    let fetch = RecordingFetch::new(vec![sse_response("data: {not json}\n\n")]);

    let (_, result) = collect(stream(model(), context(), options(fetch, Transport::Sse))).await;

    assert_eq!(result.stop_reason, StopReason::Error);
    assert!(
        result
            .error_message
            .as_ref()
            .map(|text| text.to_utf8().unwrap())
            .as_deref()
            .unwrap()
            .starts_with("Invalid Codex SSE JSON:")
    );
}

#[tokio::test]
async fn maps_usage_and_reasoning_tokens() {
    let fetch = RecordingFetch::new(vec![sse_response(&format!(
        "data: {}\n\n",
        json!({"type": "response.completed", "response": {
            "status": "completed",
            "usage": {"input_tokens": 100, "output_tokens": 40, "total_tokens": 140,
                "input_tokens_details": {"cached_tokens": 30, "cache_write_tokens": 10},
                "output_tokens_details": {"reasoning_tokens": 7}}
        }})
    ))]);
    let mut opts = options(fetch, Transport::Sse);
    opts.service_tier = None;

    let (_, result) = collect(stream(model(), context(), opts)).await;

    assert_eq!(result.usage.input, 70.0);
    assert_eq!(result.usage.output, 40.0);
    assert_eq!(result.usage.cache_read, 30.0);
    assert_eq!(result.usage.cache_write, 0.0);
    assert_eq!(result.usage.reasoning, Some(7.0));
    assert_eq!(result.usage.total_tokens, 140.0);
}

#[tokio::test]
async fn custom_reasoning_effort_and_verbosity_serialize() {
    let fetch = RecordingFetch::new(vec![sse_response(&text_sse("completed", None))]);
    let mut opts = options(fetch.clone(), Transport::Sse);
    opts.reasoning_effort = Some(CodexReasoningEffort::High);
    opts.reasoning_summary = Some(pi_ai::api::openai_codex_responses::ReasoningSummary::Concise);
    opts.text_verbosity = Some(TextVerbosity::High);

    collect(stream(model(), context(), opts)).await;

    let body = decode_body(&fetch.requests()[0]);
    assert_eq!(body["reasoning"], json!({"effort": "high", "summary": "concise"}));
    assert_eq!(body["text"]["verbosity"], "high");
}

#[tokio::test]
async fn missing_api_key_produces_error_stream() {
    let mut opts = options(RecordingFetch::new(Vec::new()), Transport::Sse);
    opts.stream.request.api_key = None;

    let (_, result) = collect(stream(model(), context(), opts)).await;

    assert_eq!(result.stop_reason, StopReason::Error);
    assert_eq!(
        result
            .error_message
            .as_ref()
            .map(|text| text.to_utf8().unwrap())
            .as_deref(),
        Some("No API key for provider: openai-codex")
    );
}

#[tokio::test]
async fn close_and_reset_debug_stats_are_public() {
    close_openai_codex_websocket_sessions(Some(&"missing-session".into()));
    reset_openai_codex_websocket_debug_stats(Some(&"missing-session".into()));
    assert_eq!(get_openai_codex_websocket_debug_stats(&"missing-session".into()), None);
}

// =============================================================================
// Scripted WebSocket server
// =============================================================================

#[path = "support/ws_server.rs"]
mod ws_server;

use ws_server::{completed_event, start_ws_server};

fn ws_model(base_url: &str) -> Model {
    let mut model = model();
    model.base_url = (base_url.to_owned()).into();
    model
}

fn ws_options(
    fetch: Arc<dyn Fetch>,
    transport: Transport,
    session_id: Option<&str>,
    token: &str,
) -> OpenAICodexResponsesOptions {
    OpenAICodexResponsesOptions {
        stream: StreamOptions {
            request: ProviderRequestOptions {
                api_key: Some((token.to_owned()).into()),
                fetch: Some(fetch),
                ..ProviderRequestOptions::default()
            },
            transport: Some(transport),
            session_id: session_id.map(Into::into),
            ..StreamOptions::default()
        },
        ..OpenAICodexResponsesOptions::default()
    }
}

fn text_of(result: &AssistantMessage) -> Option<&pi_js_compat::js_json::JsString> {
    result.content.iter().find_map(|block| match block {
        pi_ai::types::AssistantContent::Text(text) => Some(&text.text),
        _ => None,
    })
}

#[tokio::test]
async fn websocket_without_a_session_uses_a_crypto_random_uuid_v4_request_id() {
    let fetch = RecordingFetch::new(Vec::new());
    let server = start_ws_server(Arc::new(|_, _| completed_event("resp", Some("ok"), None))).await;
    collect(stream(
        ws_model(&server.url),
        context(),
        ws_options(fetch, Transport::Websocket, None, &mock_token("uuid-v4-account")),
    ))
    .await;
    let connections = server.records.connections.lock().unwrap();
    assert_eq!(connections.len(), 1);
    let id = connections[0].headers.get("x-client-request-id").expect("request id");
    assert_eq!(id.len(), 36);
    assert_eq!(id.as_bytes()[14], b'4', "frozen crypto.randomUUID generates v4, not v7");
    assert_eq!(connections[0].headers.get("session-id"), Some(id));
}

#[tokio::test]
async fn websocket_cached_continuation_sends_only_input_deltas() {
    let fetch = RecordingFetch::new(Vec::new());
    let server = start_ws_server(Arc::new(|connection, frame| {
        if connection == 0 && frame == 0 {
            completed_event("resp_1", None, None)
        } else {
            completed_event("resp_2", Some("Done"), None)
        }
    }))
    .await;
    let token = mock_token("acc_test");
    let mut first_context = context();
    first_context.messages = serde_json::from_value(json!([
        {"role": "user", "content": "Say hello", "timestamp": 1}
    ]))
    .expect("messages");
    let first = collect(stream(
        ws_model(&server.url),
        first_context.clone(),
        ws_options(fetch.clone(), Transport::WebsocketCached, Some("session-1"), &token),
    ))
    .await
    .1;

    let mut second_context = first_context;
    second_context
        .messages
        .push(pi_ai::types::Message::Assistant(Box::new(first)));
    second_context
        .messages
        .push(serde_json::from_value(json!({"role": "user", "content": "Now finish", "timestamp": 2})).expect("user"));
    let second = collect(stream(
        ws_model(&server.url),
        second_context,
        ws_options(fetch.clone(), Transport::WebsocketCached, Some("session-1"), &token),
    ))
    .await
    .1;

    assert_eq!(text_of(&second), Some(&pi_js_compat::js_json::JsString::from("Done")));
    let connections = server.records.connections.lock().unwrap();
    assert_eq!(connections.len(), 1);
    let frames = &connections[0].frames;
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0].get("previous_response_id"), None);
    assert_eq!(frames[0]["store"], false);
    assert_eq!(frames[1]["previous_response_id"], "resp_1");
    assert_eq!(frames[1]["input"].as_array().unwrap().len(), 1);
    assert_eq!(frames[1]["input"][0]["content"][0]["text"], "Now finish");

    let stats = get_openai_codex_websocket_debug_stats(&"session-1".into()).expect("stats");
    assert_eq!(stats.requests, 2);
    assert_eq!(stats.connections_created, 1);
    assert_eq!(stats.connections_reused, 1);
    assert_eq!(stats.cached_context_requests, 2);
    assert_eq!(stats.store_true_requests, 0);
    assert_eq!(stats.full_context_requests, 1);
    assert_eq!(stats.delta_requests, 1);
    assert_eq!(
        stats.last_delta_input_items,
        Some(pi_js_compat::js_json::JsValue::Number(1.0))
    );
    assert_eq!(
        stats.last_previous_response_id,
        Some(pi_js_compat::js_json::JsValue::String("resp_1".into()))
    );
    close_openai_codex_websocket_sessions(Some(&"session-1".into()));
    reset_openai_codex_websocket_debug_stats(Some(&"session-1".into()));
}

#[tokio::test]
async fn websocket_cache_is_scoped_to_the_authenticated_account() {
    let fetch = RecordingFetch::new(Vec::new());
    let server = start_ws_server(Arc::new(|_, _| completed_event("resp", Some("ok"), None))).await;
    let model = ws_model(&server.url);
    let context = Context {
        system_prompt: Some(Default::default()),
        messages: Vec::new(),
        tools: None,
    };
    for account in ["account-a", "account-b", "account-a"] {
        collect(stream(
            model.clone(),
            context.clone(),
            ws_options(
                fetch.clone(),
                Transport::WebsocketCached,
                Some("shared-session"),
                &mock_token(account),
            ),
        ))
        .await;
    }

    let connections = server.records.connections.lock().unwrap();
    assert_eq!(connections.len(), 2);
    assert_eq!(
        connections[0].headers.get("chatgpt-account-id").map(String::as_str),
        Some("account-a")
    );
    assert_eq!(
        connections[1].headers.get("chatgpt-account-id").map(String::as_str),
        Some("account-b")
    );
    let stats = get_openai_codex_websocket_debug_stats(&"shared-session".into()).expect("stats");
    assert_eq!(stats.connections_created, 2);
    assert_eq!(stats.connections_reused, 1);
    close_openai_codex_websocket_sessions(Some(&"shared-session".into()));
    reset_openai_codex_websocket_debug_stats(Some(&"shared-session".into()));
}

/// Node `http` writes header values as Latin-1 bytes; 0x80-0xFF units are valid single bytes.
#[tokio::test]
async fn websocket_handshake_sends_latin1_header_values_as_single_bytes() {
    let fetch = RecordingFetch::new(Vec::new());
    let server = start_ws_server(Arc::new(|_, _| completed_event("resp", Some("ok"), None))).await;
    let mut options = ws_options(
        fetch,
        Transport::WebsocketCached,
        Some("latin1-session"),
        &mock_token("acc"),
    );
    options.stream.request.headers = Some(indexmap::IndexMap::from([(
        "x-label".to_owned(),
        Some("caf\u{e9}".into()),
    )]));
    collect(stream(ws_model(&server.url), context(), options)).await;

    let connections = server.records.connections.lock().unwrap();
    assert_eq!(connections.len(), 1);
    assert_eq!(
        connections[0].headers.get("x-label").map(String::as_str),
        Some("caf\u{e9}")
    );
    close_openai_codex_websocket_sessions(Some(&"latin1-session".into()));
    reset_openai_codex_websocket_debug_stats(Some(&"latin1-session".into()));
}

#[tokio::test]
async fn closing_sessions_sends_debug_close_frame_to_cached_sockets() {
    let fetch = RecordingFetch::new(Vec::new());
    let server = start_ws_server(Arc::new(|_, _| completed_event("resp", Some("ok"), None))).await;
    let context = Context {
        system_prompt: Some(Default::default()),
        messages: Vec::new(),
        tools: None,
    };
    collect(stream(
        ws_model(&server.url),
        context,
        ws_options(
            fetch.clone(),
            Transport::WebsocketCached,
            Some("close-session"),
            &mock_token("acc"),
        ),
    ))
    .await;

    close_openai_codex_websocket_sessions(Some(&"close-session".into()));

    let close = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Some(close) = server.records.connections.lock().unwrap()[0].close.clone() {
                return close;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("server receives close frame");
    assert_eq!(close, (1000, "debug_close".to_owned()));
    reset_openai_codex_websocket_debug_stats(Some(&"close-session".into()));
}

#[tokio::test]
async fn websocket_connect_timeout_is_reported_without_sse_fallback() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("address");
    let accept_task = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.expect("accept");
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        drop(stream);
    });
    let fetch = RecordingFetch::new(Vec::new());
    let mut opts = ws_options(
        fetch.clone(),
        Transport::Auto,
        Some("ws-connect-timeout"),
        &mock_token("acc_test"),
    );
    opts.stream.websocket_connect_timeout_ms = Some(50.0);
    opts.stream.request.timeout_ms = Some(300_000.0);

    let (_, result) = collect(stream(ws_model(&format!("ws://{address}")), context(), opts)).await;

    assert_eq!(result.stop_reason, StopReason::Error);
    assert_eq!(
        result
            .error_message
            .as_ref()
            .map(|text| text.to_utf8().unwrap())
            .as_deref(),
        Some("WebSocket connect timeout after 50ms")
    );
    assert!(fetch.requests().is_empty());
    let stats = get_openai_codex_websocket_debug_stats(&"ws-connect-timeout".into()).expect("stats");
    assert_eq!(stats.websocket_failures, 1);
    assert_eq!(
        stats
            .last_websocket_error
            .as_ref()
            .map(|text| text.to_utf8().unwrap())
            .as_deref(),
        Some("WebSocket connect timeout after 50ms")
    );
    reset_openai_codex_websocket_debug_stats(Some(&"ws-connect-timeout".into()));
    accept_task.abort();
}

#[tokio::test]
async fn websocket_idle_timeout_before_first_event_emits_no_start() {
    let fetch = RecordingFetch::new(Vec::new());
    let server = start_ws_server(Arc::new(|_, _| Vec::new())).await;
    let mut opts = ws_options(
        fetch.clone(),
        Transport::Websocket,
        Some("ws-idle-before-start"),
        &mock_token("acc_test"),
    );
    opts.stream.request.timeout_ms = Some(50.0);

    let (events, result) = collect(stream(ws_model(&server.url), context(), opts)).await;

    assert_eq!(result.stop_reason, StopReason::Error);
    assert_eq!(
        result
            .error_message
            .as_ref()
            .map(|text| text.to_utf8().unwrap())
            .as_deref(),
        Some("WebSocket idle timeout after 50ms")
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, AssistantMessageEvent::Start { .. }))
    );
    let stats = get_openai_codex_websocket_debug_stats(&"ws-idle-before-start".into()).expect("stats");
    assert_eq!(stats.websocket_failures, 1);
    reset_openai_codex_websocket_debug_stats(Some(&"ws-idle-before-start".into()));
}

#[tokio::test]
async fn websocket_reconnects_once_when_connection_limit_is_reached() {
    let fetch = RecordingFetch::new(Vec::new());
    let server = start_ws_server(Arc::new(|connection, _| {
        if connection == 0 {
            vec![json!({"type": "error", "error": {"code": "websocket_connection_limit_reached"}})]
        } else {
            completed_event("resp_1", Some("ok"), None)
        }
    }))
    .await;
    let retries = Arc::new(Mutex::new(Vec::new()));
    let captured = retries.clone();
    let mut opts = ws_options(
        fetch.clone(),
        Transport::Websocket,
        Some("ws-limit"),
        &mock_token("acc_test"),
    );
    opts.stream.on_retry = Some(Arc::new(move |event, _model| {
        captured.lock().unwrap().push(event);
        Ok(())
    }));

    let (events, result) = collect(stream(ws_model(&server.url), context(), opts)).await;

    assert_eq!(result.stop_reason, StopReason::Stop);
    assert_eq!(text_of(&result), Some(&pi_js_compat::js_json::JsString::from("ok")));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AssistantMessageEvent::Start { .. }))
            .count(),
        1
    );
    assert_eq!(server.records.connections.lock().unwrap().len(), 2);
    let retries = retries.lock().unwrap();
    assert_eq!(retries.len(), 1);
    assert_eq!(retries[0].attempt, 1.0);
    assert_eq!(retries[0].reason, "WebSocket connection limit reached, reconnecting");
    close_openai_codex_websocket_sessions(Some(&"ws-limit".into()));
    reset_openai_codex_websocket_debug_stats(Some(&"ws-limit".into()));
}

#[tokio::test]
async fn websocket_reports_missing_cached_continuation_without_an_old_only_retry() {
    let fetch = RecordingFetch::new(Vec::new());
    let server = start_ws_server(Arc::new(|connection, frame| {
        if connection == 0 && frame == 0 {
            completed_event("resp_1", Some("Hello"), None)
        } else if connection == 0 {
            vec![json!({"type": "error", "error": {
                "code": "previous_response_not_found",
                "message": "Previous response with id 'resp_1' not found."
            }})]
        } else {
            completed_event("resp_2", Some("Recovered"), None)
        }
    }))
    .await;
    let token = mock_token("acc_test");
    let first_context = context();
    let first = collect(stream(
        ws_model(&server.url),
        first_context.clone(),
        ws_options(fetch.clone(), Transport::WebsocketCached, Some("missing"), &token),
    ))
    .await
    .1;

    let mut second_context = first_context;
    second_context
        .messages
        .push(pi_ai::types::Message::Assistant(Box::new(first)));
    second_context
        .messages
        .push(serde_json::from_value(json!({"role": "user", "content": "Now finish", "timestamp": 2})).expect("user"));
    let (events, second) = collect(stream(
        ws_model(&server.url),
        second_context,
        ws_options(fetch.clone(), Transport::WebsocketCached, Some("missing"), &token),
    ))
    .await;

    assert_eq!(second.stop_reason, StopReason::Error);
    assert_eq!(
        second.error_message.as_ref().unwrap(),
        "Codex error: Previous response with id 'resp_1' not found."
    );
    assert!(
        second.diagnostics.is_none(),
        "source API errors are not transport failures"
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AssistantMessageEvent::Start { .. }))
            .count(),
        0
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AssistantMessageEvent::Error { .. }))
            .count(),
        1
    );
    let connections = server.records.connections.lock().unwrap();
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].frames.len(), 2);
    assert_eq!(connections[0].frames[1]["previous_response_id"], "resp_1");
    assert_eq!(connections[0].frames[1]["input"].as_array().unwrap().len(), 1);
    let stats = get_openai_codex_websocket_debug_stats(&"missing".into()).expect("stats");
    assert_eq!(stats.requests, 2);
    assert_eq!(stats.connections_created, 1);
    assert_eq!(stats.connections_reused, 1);
    assert_eq!(stats.full_context_requests, 1);
    assert_eq!(stats.delta_requests, 1);
    assert_eq!(stats.websocket_failures, 0);
    close_openai_codex_websocket_sessions(Some(&"missing".into()));
    reset_openai_codex_websocket_debug_stats(Some(&"missing".into()));
}

#[tokio::test]
async fn websocket_reuses_session_when_cache_retention_is_none() {
    let fetch = RecordingFetch::new(Vec::new());
    let server = start_ws_server(Arc::new(|_, _| completed_event("resp", Some("ok"), None))).await;
    let mut opts = ws_options(
        fetch.clone(),
        Transport::Auto,
        Some("one-off-summary"),
        &mock_token("acc_test"),
    );
    opts.stream.cache_retention = Some(pi_ai::types::CacheRetention::None);

    collect(stream(ws_model(&server.url), context(), opts.clone())).await;
    collect(stream(ws_model(&server.url), context(), opts)).await;

    assert_eq!(server.records.connections.lock().unwrap().len(), 1);
    let stats = get_openai_codex_websocket_debug_stats(&"one-off-summary".into()).unwrap();
    assert_eq!(stats.requests, 2);
    assert_eq!(stats.connections_created, 1);
    assert_eq!(stats.connections_reused, 1);
    close_openai_codex_websocket_sessions(Some(&"one-off-summary".into()));
    reset_openai_codex_websocket_debug_stats(Some(&"one-off-summary".into()));
}
#[tokio::test]
async fn response_done_is_normalized_to_completed() {
    let fetch = RecordingFetch::new(vec![sse_response(&format!(
        "data: {}\n\n",
        json!({"type": "response.done", "response": {
            "status": "completed",
            "end_turn": true,
            "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}
        }})
    ))]);

    let (_, result) = collect(stream(model(), context(), options(fetch, Transport::Sse))).await;

    assert_eq!(result.stop_reason, StopReason::Stop);
    assert!(serde_json::to_value(&result).unwrap().get("endTurn").is_none());
    assert!(serde_json::to_value(&result).unwrap().get("rawStopReason").is_none());
}

#[tokio::test]
async fn streams_function_call_arguments() {
    let fetch = RecordingFetch::new(vec![sse_response(&format!(
        "data: {}\n\ndata: {}\n\ndata: {}\n\ndata: {}\n\n",
        json!({"type": "response.output_item.added", "output_index": 0, "item": {
            "type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "lookup", "arguments": ""
        }}),
        json!({"type": "response.function_call_arguments.delta", "output_index": 0, "delta": "{\"x\":1}"}),
        json!({"type": "response.output_item.done", "output_index": 0, "item": {
            "type": "function_call", "id": "fc_1", "call_id": "call_1", "name": "lookup", "arguments": "{\"x\":1}"
        }}),
        json!({"type": "response.completed", "response": {
            "status": "completed", "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}
        }})
    ))]);

    let (_, result) = collect(stream(model(), context(), options(fetch, Transport::Sse))).await;

    assert_eq!(result.stop_reason, StopReason::ToolUse);
    let call = result
        .content
        .iter()
        .find_map(|block| match block {
            pi_ai::types::AssistantContent::ToolCall(call) => Some(call),
            _ => None,
        })
        .expect("tool call");
    assert_eq!(call.id, "call_1|fc_1");
    assert_eq!(call.name, "lookup");
    assert_eq!(call.arguments["x"], pi_js_compat::js_json::JsValue::Number(1.0));
}

#[tokio::test]
async fn streams_encrypted_reasoning_summaries() {
    let fetch = RecordingFetch::new(vec![sse_response(&format!(
        "data: {}\n\ndata: {}\n\ndata: {}\n\ndata: {}\n\n",
        json!({"type": "response.output_item.added", "output_index": 0, "item": {
            "type": "reasoning", "id": "rs_1", "summary": []
        }}),
        json!({"type": "response.reasoning_summary_text.delta", "output_index": 0, "delta": "think"}),
        json!({"type": "response.output_item.done", "output_index": 0, "item": {
            "type": "reasoning", "id": "rs_1", "summary": [{"type": "summary_text", "text": "think"}],
            "encrypted_content": "enc"
        }}),
        json!({"type": "response.completed", "response": {
            "status": "completed", "usage": {"input_tokens": 1, "output_tokens": 1, "total_tokens": 2}
        }})
    ))]);

    let (_, result) = collect(stream(model(), context(), options(fetch, Transport::Sse))).await;

    let thinking = result
        .content
        .iter()
        .find_map(|block| match block {
            pi_ai::types::AssistantContent::Thinking(thinking) => Some(thinking),
            _ => None,
        })
        .expect("thinking block");
    assert_eq!(thinking.thinking, "think");
    let signature: Value =
        serde_json::from_str(thinking.thinking_signature.as_deref().expect("signature")).expect("json");
    assert_eq!(signature["encrypted_content"], "enc");
}

#[tokio::test]
async fn sends_native_websocket_pings_while_waiting() {
    let fetch = RecordingFetch::new(Vec::new());
    let server = start_ws_server(Arc::new(|_, _| Vec::new())).await;
    let mut opts = ws_options(fetch.clone(), Transport::Websocket, None, &mock_token("acc_test"));
    opts.stream.request.timeout_ms = Some(91_000.0);
    let stream = stream(ws_model(&server.url), context(), opts);

    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        server.records.frame_notify.notified(),
    )
    .await
    .expect("server receives response.create");
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(30)).await;
    tokio::time::advance(std::time::Duration::from_secs(30)).await;
    tokio::time::advance(std::time::Duration::from_secs(30)).await;
    tokio::time::advance(std::time::Duration::from_secs(1)).await;
    let result = stream.result().await.expect("result");

    assert_eq!(
        result
            .error_message
            .as_ref()
            .map(|text| text.to_utf8().unwrap())
            .as_deref(),
        Some("WebSocket idle timeout after 91000ms")
    );
    tokio::time::resume();
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    // The close frame can overtake the final ping on the wire; the client still
    // emitted a ping on each 30s interval tick.
    assert!(server.records.connections.lock().unwrap()[0].pings >= 2);
}

#[tokio::test]
async fn explicit_null_thinking_level_map_falls_back_to_logical_effort() {
    let fetch = RecordingFetch::new(vec![sse_response(&text_sse("completed", None))]);
    let mut opts = options(fetch.clone(), Transport::Sse);
    opts.reasoning_effort = Some(CodexReasoningEffort::None);
    let mut model = model();
    model.thinking_level_map = Some([(pi_ai::types::ModelThinkingLevel::Off, None)].into_iter().collect());

    collect(stream(model, context(), opts)).await;

    assert_eq!(
        decode_body(&fetch.requests()[0])["reasoning"],
        json!({"effort": "none", "summary": "auto"})
    );
}

#[tokio::test]
async fn honors_iso_8601_retry_after_dates() {
    // Observe the far-future parsed delay before sleep. Frozen caps only429;
    // a503must retain this delay even when maxRetryDelayMs is smaller.
    let fetch = RecordingFetch::new(vec![
        ScriptedResponse::json(
            503,
            r#"{"error":{"code":"temporarily_unavailable","message":"retry later"}}"#,
        )
        .with_header("retry-after", "2999-01-01T00:00:00Z"),
    ]);
    let mut opts = options(fetch.clone(), Transport::Sse);
    opts.stream.request.max_retries = Some(3.0);
    opts.stream.request.max_retry_delay_ms = Some(1000.0);
    let delays = Arc::new(Mutex::new(Vec::new()));
    let observed = delays.clone();
    opts.stream.on_retry = Some(Arc::new(move |event, _| {
        observed.lock().unwrap().push(event.delay_ms);
        Err(CallbackError::new("date proof done"))
    }));
    let (_, result) = collect(stream(model(), context(), opts)).await;

    assert_eq!(result.stop_reason, StopReason::Error);
    assert_eq!(result.error_message.unwrap(), "date proof done");
    let delays = delays.lock().unwrap();
    assert_eq!(delays.len(), 2, "HTTP callback rejection enters the retry catch once");
    assert_eq!(delays[1], Some(1000.0));
    let requested = delays[0].unwrap() / 1000.0;
    assert!(
        (30_000_000_000.0..40_000_000_000.0).contains(&requested),
        "delay seconds {requested}"
    );
    assert_eq!(fetch.requests().len(), 1);
}
