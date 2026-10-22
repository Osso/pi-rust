use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use pi_ai::api::pi_messages::{PiMessagesOptions, PiMessagesToolChoice, stream, stream_simple};
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse, SimpleStreamOptions};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, Context, Modality, Model, ModelCost, ModelCostRates,
    StopReason, TextContent, ToolCall, UserContent, UserMessage, UserRole,
};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

const BASE_URL: &str = "http://127.0.0.1:1234/v1";

struct ScriptedFetch {
    requests: Arc<Mutex<Vec<HttpRequest>>>,
    response: ScriptedResponse,
    fail: Option<String>,
}

struct ScriptedResponse {
    status: u16,
    status_text: String,
    headers: Vec<(String, String)>,
    body: String,
    hang: bool,
}

impl Fetch for ScriptedFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<CancellationToken>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.requests.lock().unwrap().push(request);
        if let Some(fail) = &self.fail {
            let fail = fail.clone();
            return Box::pin(async move { Err(CallbackError::new(fail)) });
        }
        let response = &self.response;
        let body = ChunkBody {
            chunks: vec![response.body.clone().into_bytes()].into(),
            hang: response.hang,
        };
        Box::pin(async move {
            Ok(HttpResponse {
                status: response.status,
                status_text: response.status_text.clone(),
                headers: response.headers.clone(),
                body: Box::new(body),
            })
        })
    }
}

struct ChunkBody {
    chunks: VecDeque<Vec<u8>>,
    hang: bool,
}

impl HttpBody for ChunkBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        if self.hang {
            return Box::pin(std::future::pending());
        }
        let chunk = self.chunks.pop_front();
        Box::pin(async move { chunk.map(Ok) })
    }
}

fn model() -> Model {
    Model {
        id: "auto".to_owned(),
        name: "Radius Auto".to_owned(),
        api: "pi-messages".to_owned(),
        provider: "radius".to_owned(),
        base_url: (BASE_URL.to_owned()).into(),
        reasoning: false,
        thinking_level_map: None,
        input: vec![Modality::Text],
        cost: ModelCost {
            rates: ModelCostRates {
                input: 1.0,
                output: 2.0,
                cache_read: 0.1,
                cache_write: 0.2,
            },
            tiers: None,
        },
        context_window: 128_000.0,
        auto_compaction_threshold: None,
        max_tokens: 16_384.0,
        sampling_params: None,
        headers: None,
        compat: None,
    }
}

fn context() -> Context {
    Context {
        system_prompt: None,
        messages: vec![pi_ai::types::Message::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Text(("Hello".to_owned()).into()),
            input_source: None,
            provider_native: None,
            timestamp: 1,
        })],
        tools: None,
    }
}

fn usage() -> Value {
    json!({
        "input": 10, "output": 5, "cacheRead": 0, "cacheWrite": 0, "totalTokens": 15,
        "cost": {"input": 0.1, "output": 0.2, "cacheRead": 0, "cacheWrite": 0, "total": 0.3},
    })
}

fn sse(events: &[Value]) -> String {
    events.iter().map(|event| format!("data: {event}\n\n")).collect()
}

fn fetch_with(response: ScriptedResponse) -> (Arc<ScriptedFetch>, Arc<Mutex<Vec<HttpRequest>>>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    (
        Arc::new(ScriptedFetch {
            requests: Arc::clone(&requests),
            response,
            fail: None,
        }),
        requests,
    )
}

fn fetch_failing(message: &str) -> Arc<ScriptedFetch> {
    Arc::new(ScriptedFetch {
        requests: Arc::new(Mutex::new(Vec::new())),
        response: ScriptedResponse {
            status: 200,
            status_text: "OK".to_owned(),
            headers: Vec::new(),
            body: String::new(),
            hang: false,
        },
        fail: Some(message.to_owned()),
    })
}

fn options(fetch: Arc<ScriptedFetch>, api_key: &str) -> PiMessagesOptions {
    let mut options = PiMessagesOptions::default();
    options.stream.request.fetch = Some(fetch);
    options.stream.request.api_key = Some((api_key.to_owned()).into());
    options
}

async fn collect(
    mut event_stream: pi_ai::utils::event_stream::AssistantMessageEventStream,
) -> (Vec<AssistantMessageEvent>, AssistantMessage) {
    let mut events = Vec::new();
    while let Some(event) = event_stream.next().await {
        events.push(event.expect("stream event"));
    }
    let message = event_stream.result().await.expect("stream result");
    (events, message)
}

#[tokio::test]
async fn streams_text_and_tool_calls_and_resolves_the_terminal_message() {
    let events = [
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
    let (fetch, requests) = fetch_with(ScriptedResponse {
        status: 200,
        status_text: "OK".to_owned(),
        headers: vec![("content-type".to_owned(), "text/event-stream".to_owned())],
        body: sse(&events),
        hang: false,
    });
    let mut stream_options = options(fetch, "test-key");
    stream_options.stream.session_id = Some(("session-1".to_owned()).into());
    stream_options.tool_choice = Some(PiMessagesToolChoice::Auto);
    stream_options.stream.max_tokens = Some(100.0);
    stream_options.stream.request.headers = Some([("x-custom".to_owned(), Some("1".to_owned()))].into_iter().collect());

    let (events, message) = collect(stream(model(), context(), stream_options)).await;

    let partial_stop_reasons: Vec<StopReason> = events
        .iter()
        .filter_map(|event| match event {
            AssistantMessageEvent::Start { partial }
            | AssistantMessageEvent::TextStart { partial, .. }
            | AssistantMessageEvent::TextDelta { partial, .. }
            | AssistantMessageEvent::TextEnd { partial, .. }
            | AssistantMessageEvent::ThinkingStart { partial, .. }
            | AssistantMessageEvent::ThinkingDelta { partial, .. }
            | AssistantMessageEvent::ThinkingEnd { partial, .. }
            | AssistantMessageEvent::ToolCallStart { partial, .. }
            | AssistantMessageEvent::ToolCallDelta { partial, .. }
            | AssistantMessageEvent::ToolCallEnd { partial, .. } => Some(partial.stop_reason),
            AssistantMessageEvent::Done { .. } | AssistantMessageEvent::Error { .. } => None,
        })
        .collect();
    assert_eq!(partial_stop_reasons[0], StopReason::Pending);
    assert_eq!(message.stop_reason, StopReason::ToolUse, "{:?}", message.error_message);
    assert_eq!(message.usage.input, 10.0);
    assert_eq!(message.usage.output, 5.0);
    assert_eq!(message.usage.total_tokens, 15.0);
    assert_eq!(message.response_id.as_deref(), Some("resp_1"));
    assert_eq!(message.model, "auto");
    assert_eq!(message.provider, "radius");
    assert_eq!(
        message.content,
        vec![
            AssistantContent::Text(TextContent::new("Hello")),
            AssistantContent::ToolCall(ToolCall::new(
                "call_1",
                "read",
                json!({"path": "a.txt"}).as_object().unwrap().clone(),
            )),
        ]
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AssistantMessageEvent::TextDelta { .. }))
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, AssistantMessageEvent::ToolCallEnd { .. }))
            .count(),
        1
    );

    let requests = requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let request = &requests[0];
    assert_eq!(request.url, "http://127.0.0.1:1234/v1/messages");
    let headers = request
        .headers
        .iter()
        .cloned()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(headers["authorization"], "Bearer test-key");
    assert_eq!(headers["x-custom"], "1");
    let body: Value = serde_json::from_slice(request.body.as_ref().expect("body")).expect("JSON");
    assert_eq!(
        body,
        json!({
            "model": "auto",
            "context": {"messages": [{"role": "user", "content": "Hello", "timestamp": 1}]},
            "options": {"maxTokens": 100, "sessionId": "session-1", "toolChoice": "auto"},
        })
    );
}

#[tokio::test]
async fn appends_debug_and_reports_response_headers_via_on_response() {
    let (fetch, requests) = fetch_with(ScriptedResponse {
        status: 200,
        status_text: "OK".to_owned(),
        headers: vec![
            ("content-type".to_owned(), "text/event-stream".to_owned()),
            ("x-pi-gateway-upstream-provider".to_owned(), "anthropic".to_owned()),
        ],
        body: sse(&[json!({"type": "done", "reason": "stop", "usage": usage()})]),
        hang: false,
    });
    let observed = Arc::new(Mutex::new(None));
    let observed_clone = Arc::clone(&observed);
    let mut stream_options = SimpleStreamOptions::default();
    stream_options.stream.request.fetch = Some(fetch);
    stream_options.stream.request.api_key = Some(("test-key".to_owned()).into());
    stream_options.stream.request.on_response = Some(Arc::new(move |response, _model| {
        let observed = Arc::clone(&observed_clone);
        Box::pin(async move {
            *observed.lock().unwrap() = Some(response);
            Ok(())
        })
    }));
    stream_options.stream.extra.insert("debug".to_owned(), json!(true));

    let (_, message) = collect(stream_simple(model(), context(), stream_options)).await;

    assert_eq!(message.stop_reason, StopReason::Stop);
    assert_eq!(
        requests.lock().unwrap()[0].url,
        "http://127.0.0.1:1234/v1/messages?debug=1"
    );
    assert_eq!(
        observed.lock().unwrap().as_ref().unwrap().headers["x-pi-gateway-upstream-provider"],
        "anthropic"
    );
}

#[tokio::test]
async fn surfaces_backend_error_responses_with_diagnostics() {
    let (fetch, _) = fetch_with(ScriptedResponse {
        status: 401,
        status_text: "Unauthorized".to_owned(),
        headers: Vec::new(),
        body: json!({"error": {"message": "Token expired", "code": "unauthorized"}}).to_string(),
        hang: false,
    });

    let (_, message) = collect(stream(model(), context(), options(fetch, "stale"))).await;

    assert_eq!(message.stop_reason, StopReason::Error);
    let error_message = message.error_message.as_ref().unwrap_or_default();
    assert!(error_message.contains("401"));
    assert!(error_message.contains("Token expired"));
    assert!(error_message.contains("unauthorized"));
    let diagnostics = message.diagnostics.as_ref().expect("diagnostics");
    assert_eq!(diagnostics[0].r#type, "pi_messages_response_failure");
    assert_eq!(diagnostics[0].details.as_ref().unwrap()["status"], json!(401));
}

#[tokio::test]
async fn propagates_server_sent_error_events() {
    let events = [
        json!({"type": "start"}),
        json!({"type": "error", "reason": "error", "usage": usage(), "errorMessage": "Upstream failed"}),
    ];
    let (fetch, _) = fetch_with(ScriptedResponse {
        status: 200,
        status_text: "OK".to_owned(),
        headers: Vec::new(),
        body: sse(&events),
        hang: false,
    });

    let (_, message) = collect(stream(model(), context(), options(fetch, "test-key"))).await;

    assert_eq!(message.stop_reason, StopReason::Error);
    assert_eq!(message.error_message.as_ref(), Some("Upstream failed"));
    assert_eq!(message.usage.input, 10.0);
    assert_eq!(message.usage.total_tokens, 15.0);
}

#[tokio::test]
async fn errors_when_no_api_key_is_provided() {
    let (_, message) = collect(stream(model(), context(), PiMessagesOptions::default())).await;
    assert_eq!(message.stop_reason, StopReason::Error);
    assert!(
        message
            .error_message
            .as_ref()
            .unwrap_or_default()
            .contains("No API key provided")
    );
}

#[tokio::test]
async fn errors_when_the_stream_ends_without_a_terminal_event() {
    let events = [
        json!({"type": "start"}),
        json!({"type": "text_start", "contentIndex": 0}),
        json!({"type": "text_delta", "contentIndex": 0, "delta": "partial"}),
    ];
    let (fetch, _) = fetch_with(ScriptedResponse {
        status: 200,
        status_text: "OK".to_owned(),
        headers: Vec::new(),
        body: sse(&events),
        hang: false,
    });

    let (_, message) = collect(stream(model(), context(), options(fetch, "test-key"))).await;
    assert_eq!(message.stop_reason, StopReason::Error);
    assert!(
        message
            .error_message
            .as_ref()
            .unwrap_or_default()
            .contains("stream ended without a terminal event")
    );
}

#[tokio::test]
async fn records_rewrite_impact_diagnostics() {
    let events = [
        json!({"type": "text_start", "contentIndex": 0}),
        json!({"type": "text_delta", "contentIndex": 0, "delta": "hi"}),
        json!({
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
        }),
    ];
    let (fetch, _) = fetch_with(ScriptedResponse {
        status: 200,
        status_text: "OK".to_owned(),
        headers: Vec::new(),
        body: sse(&events),
        hang: false,
    });

    let (_, message) = collect(stream(model(), context(), options(fetch, "test-key"))).await;
    assert_eq!(message.stop_reason, StopReason::Stop);
    let diagnostics = message.diagnostics.as_ref().expect("diagnostics");
    assert_eq!(diagnostics[0].r#type, "pi_messages_rewrite");
    assert_eq!(diagnostics[0].details.as_ref().unwrap()["policyId"], json!("policy-1"));
    assert_eq!(diagnostics[0].details.as_ref().unwrap()["tokenCountChange"], json!(-3));
}

#[tokio::test]
async fn reports_aborted_fetch_errors_as_aborted() {
    let signal = CancellationToken::new();
    let mut stream_options = options(fetch_failing("This operation was aborted"), "test-key");
    stream_options.stream.request.signal = Some(signal.clone());
    signal.cancel();

    let (_, message) = collect(stream(model(), context(), stream_options)).await;
    assert_eq!(message.stop_reason, StopReason::Aborted);
}

#[tokio::test]
async fn errors_on_malformed_event_json() {
    let (fetch, _) = fetch_with(ScriptedResponse {
        status: 200,
        status_text: "OK".to_owned(),
        headers: Vec::new(),
        body: "data: not-json\n\n".to_owned(),
        hang: false,
    });

    let (_, message) = collect(stream(model(), context(), options(fetch, "test-key"))).await;
    assert_eq!(message.stop_reason, StopReason::Error);
}
