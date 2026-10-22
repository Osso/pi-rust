use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{ProxyStreamOptions, stream_proxy};
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_ai::types::{
    AssistantMessage, AssistantMessageEvent, Context, ErrorReason, Model, ModelCost, ModelCostRates, StopReason,
};
use serde_json::json;
use tokio_util::sync::CancellationToken;

struct ScriptedBody {
    chunks: Vec<Vec<u8>>,
}

impl HttpBody for ScriptedBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async move {
            if self.chunks.is_empty() {
                None
            } else {
                Some(Ok(self.chunks.remove(0)))
            }
        })
    }
}

struct ScriptedFetch {
    status: u16,
    status_text: String,
    chunks: Vec<Vec<u8>>,
    requests: Arc<Mutex<Vec<HttpRequest>>>,
    honour_cancel: bool,
}

impl Fetch for ScriptedFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<CancellationToken>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.requests.lock().unwrap().push(request);
        let status = self.status;
        let status_text = self.status_text.clone();
        let chunks = self.chunks.clone();
        let honour_cancel = self.honour_cancel;
        Box::pin(async move {
            if honour_cancel && signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                return Err(CallbackError::new("This operation was aborted"));
            }
            Ok(HttpResponse {
                status,
                status_text,
                headers: Vec::new(),
                body: Box::new(ScriptedBody { chunks }),
            })
        })
    }
}

fn model() -> Model {
    Model {
        id: "gpt-5.4".to_owned(),
        name: "GPT-5.4".to_owned(),
        api: "openai-responses".to_owned(),
        provider: "openai".to_owned(),
        base_url: JsString::from("https://api.openai.com/v1"),
        reasoning: true,
        thinking_level_map: None,
        input: vec![pi_ai::types::Modality::Text],
        cost: ModelCost {
            rates: ModelCostRates::default(),
            tiers: None,
        },
        context_window: 400000.0,
        auto_compaction_threshold: None,
        max_tokens: 128000.0,
        sampling_params: None,
        headers: None,
        compat: None,
    }
}

fn sse(events: &[serde_json::Value]) -> Vec<u8> {
    events
        .iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect::<String>()
        .into_bytes()
}

fn options(fetch: Arc<dyn Fetch>, signal: Option<CancellationToken>) -> ProxyStreamOptions {
    ProxyStreamOptions {
        auth_token: "test-token".to_owned(),
        proxy_url: "https://proxy.example.com".to_owned(),
        signal,
        fetch: Some(fetch),
        ..Default::default()
    }
}

async fn collect(
    mut stream: pi_ai::utils::event_stream::AssistantMessageEventStream,
) -> (Vec<AssistantMessageEvent>, AssistantMessage) {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.expect("proxy stream must not fail"));
    }
    let result = stream.result().await.expect("proxy stream must settle");
    (events, result)
}

#[tokio::test]
async fn proxy_stream_preserves_utf16_deltas_arguments_and_request_context() {
    use pi_ai::types::AssistantContent;
    use pi_js_compat::js_json::{JsValue, parse_lossless};
    let requests = Arc::new(Mutex::new(Vec::new()));
    let events = [
        r#"{"type":"start"}"#,
        r#"{"type":"text_start","contentIndex":0}"#,
        r#"{"type":"text_delta","contentIndex":0,"delta":"\ud800"}"#,
        r#"{"type":"text_delta","contentIndex":0,"delta":"😀"}"#,
        r#"{"type":"text_delta","contentIndex":0,"delta":"\udfff"}"#,
        r#"{"type":"text_end","contentIndex":0}"#,
        r#"{"type":"thinking_start","contentIndex":1}"#,
        r#"{"type":"thinking_delta","contentIndex":1,"delta":"\udfff"}"#,
        r#"{"type":"thinking_end","contentIndex":1}"#,
        r#"{"type":"toolcall_start","contentIndex":2,"id":"call","toolName":"probe"}"#,
        r#"{"type":"toolcall_delta","contentIndex":2,"delta":"{\"\ud800\":\"\udfff\"}"}"#,
        r#"{"type":"toolcall_end","contentIndex":2}"#,
    ];
    let mut wire = events
        .iter()
        .map(|event| format!("data: {event}\n\n"))
        .collect::<String>();
    wire.push_str(
        &String::from_utf8(sse(&[
            json!({"type":"done","reason":"stop","usage":pi_ai::types::Usage::default()}),
        ]))
        .unwrap(),
    );
    let fetch = Arc::new(ScriptedFetch {
        status: 200,
        status_text: "OK".into(),
        chunks: wire.as_bytes().chunks(1).map(<[u8]>::to_vec).collect(),
        requests: Arc::clone(&requests),
        honour_cancel: false,
    });
    let user: pi_ai::types::UserMessage =
        serde_json::from_str(r#"{"role":"user","content":"\ud800","timestamp":0}"#).unwrap();
    let mut tool = pi_ai::types::Tool::new("lookup", "fixture", json!({"type":"object"}));
    tool.label = Some(JsString::from_utf16(vec![0xd800]));
    tool.execution_mode = Some("parallel".into());
    let context = Context {
        system_prompt: None,
        messages: vec![pi_ai::types::Message::User(user)],
        tools: Some(vec![tool]),
    };
    let metadata = parse_lossless(r#"{"\ud800":{"nested":["\udfff",null,3]}}"#).unwrap();
    let mut options = options(fetch, None);
    options.stream_options.stream.metadata = metadata.as_object().cloned();
    let (_, result) = collect(stream_proxy(model(), context, options)).await;
    assert_eq!(result.stop_reason, StopReason::Stop, "{:?}", result.error_message);
    let AssistantContent::Text(text) = &result.content[0] else {
        panic!("text block");
    };
    assert_eq!(text.text.units(), &[0xd800, 0xd83d, 0xde00, 0xdfff]);
    let AssistantContent::Thinking(thinking) = &result.content[1] else {
        panic!("thinking block");
    };
    assert_eq!(thinking.thinking.units(), &[0xdfff]);
    let AssistantContent::ToolCall(call) = &result.content[2] else {
        panic!("tool call");
    };
    assert_eq!(
        JsValue::Object(call.arguments.clone()),
        parse_lossless(r#"{"\ud800":"\udfff"}"#).unwrap()
    );
    let requests = requests.lock().unwrap();
    let body = parse_lossless(String::from_utf8(requests[0].body.clone().unwrap()).unwrap()).unwrap();
    assert_eq!(
        body["context"]["messages"][0]["content"].as_string().unwrap().units(),
        &[0xd800]
    );
    assert_eq!(body["options"]["metadata"], metadata);
    assert_eq!(
        body["context"]["tools"][0]["label"].as_string().unwrap().units(),
        &[0xd800]
    );
    assert_eq!(
        body["context"]["tools"][0]["executionMode"].as_string(),
        Some(&JsString::from("parallel"))
    );
}

#[tokio::test]
async fn reconstructs_tool_call_from_deltas_without_trusting_end_payload() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let body = sse(&[
        json!({"type": "start"}),
        json!({"type": "toolcall_start", "contentIndex": 0, "id": "call_test|fc_test", "toolName": "lookup"}),
        json!({"type": "toolcall_delta", "contentIndex": 0, "delta": "{\"value\":\"hello\"}"}),
        json!({
            "type": "toolcall_end",
            "contentIndex": 0,
            "toolCall": {
                "type": "toolCall",
                "id": "call_test|fc_test",
                "name": "lookup",
                "arguments": {"value": "hello"},
                "namespace": "dynamic_tools"
            }
        }),
        json!({"type": "done", "reason": "toolUse", "usage": {
            "input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0,
            "totalTokens": 0,
            "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}
        }}),
    ]);
    let fetch = Arc::new(ScriptedFetch {
        status: 200,
        status_text: "OK".to_owned(),
        chunks: vec![body],
        requests: Arc::clone(&requests),
        honour_cancel: false,
    });

    let mut opts = options(fetch, None);
    opts.stream_options.stream.sampling_params = Some(json!({"top_p": 0.5}).as_object().unwrap().clone());
    let stream = stream_proxy(
        model(),
        Context {
            system_prompt: None,
            messages: Vec::new(),
            tools: None,
        },
        opts,
    );
    let (events, result) = collect(stream).await;

    let end_event = events
        .iter()
        .find(|event| matches!(event, AssistantMessageEvent::ToolCallEnd { .. }))
        .expect("toolcall_end must be emitted");
    let AssistantMessageEvent::ToolCallEnd { tool_call, .. } = end_event else {
        unreachable!()
    };
    assert_eq!(tool_call.namespace, None);
    assert_eq!(
        tool_call.arguments,
        pi_js_compat::js_json!({"value": "hello"}).as_object().unwrap().clone()
    );
    assert_eq!(result.content.len(), 1);
    let pi_ai::types::AssistantContent::ToolCall(tool_call) = &result.content[0] else {
        panic!("expected tool call content");
    };
    assert_eq!(
        serde_json::to_value(pi_js_compat::js_json::JsValue::Object(tool_call.arguments.clone())).unwrap(),
        json!({"value": "hello"})
    );
    assert_eq!(tool_call.namespace, None);
    assert_eq!(result.stop_reason, StopReason::ToolUse);

    let request = requests.lock().unwrap().first().cloned().expect("one request");
    assert_eq!(request.method, "POST");
    assert_eq!(request.url, "https://proxy.example.com/api/stream");
    assert!(
        request
            .headers
            .contains(&("Authorization".to_owned(), "Bearer test-token".to_owned()))
    );
    let body: serde_json::Value = serde_json::from_slice(request.body.as_ref().unwrap()).unwrap();
    assert_eq!(body["model"]["id"], json!("gpt-5.4"));
    assert_eq!(body["context"]["messages"], json!([]));
    assert_eq!(body["options"], json!({}));
}

struct PendingReadBody(Arc<tokio::sync::Notify>);

impl HttpBody for PendingReadBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        self.0.notify_one();
        Box::pin(std::future::pending())
    }
}

struct PendingReadFetch(Arc<tokio::sync::Notify>);

impl Fetch for PendingReadFetch {
    fn fetch(
        &self,
        _: HttpRequest,
        _: Option<CancellationToken>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        let started = self.0.clone();
        Box::pin(async move {
            Ok(HttpResponse {
                status: 200,
                status_text: "OK".into(),
                headers: Vec::new(),
                body: Box::new(PendingReadBody(started)),
            })
        })
    }
}

#[tokio::test]
async fn abort_cancels_a_pending_proxy_body_read() {
    let started = Arc::new(tokio::sync::Notify::new());
    let signal = CancellationToken::new();
    let stream = stream_proxy(
        model(),
        Context {
            system_prompt: None,
            messages: Vec::new(),
            tools: None,
        },
        options(Arc::new(PendingReadFetch(started.clone())), Some(signal.clone())),
    );
    started.notified().await;
    signal.cancel();
    let message = tokio::time::timeout(std::time::Duration::from_millis(200), stream.result())
        .await
        .expect("cancelled body read must settle")
        .unwrap();
    assert_eq!(message.stop_reason, StopReason::Aborted);
    assert_eq!(
        message.error_message.as_ref(),
        Some(&JsString::from("Request aborted by user"))
    );
}

#[tokio::test]
async fn empty_proxy_error_uses_status_message() {
    let fetch = Arc::new(ScriptedFetch {
        status: 500,
        status_text: "Internal Server Error".into(),
        chunks: vec![br#"{"error":""}"#.to_vec()],
        requests: Arc::new(Mutex::new(Vec::new())),
        honour_cancel: false,
    });
    let (_, message) = collect(stream_proxy(
        model(),
        Context {
            system_prompt: None,
            messages: Vec::new(),
            tools: None,
        },
        options(fetch, None),
    ))
    .await;
    assert_eq!(
        message.error_message.as_ref(),
        Some(&JsString::from("Proxy error: 500 Internal Server Error"))
    );
}

#[tokio::test]
async fn reports_proxy_error_body_message() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let fetch = Arc::new(ScriptedFetch {
        status: 500,
        status_text: "Internal Server Error".to_owned(),
        chunks: vec![br#"{"error":"boom"}"#.to_vec()],
        requests: Arc::clone(&requests),
        honour_cancel: false,
    });

    let stream = stream_proxy(
        model(),
        Context {
            system_prompt: None,
            messages: Vec::new(),
            tools: None,
        },
        options(fetch, None),
    );
    let (events, result) = collect(stream).await;

    let AssistantMessageEvent::Error { reason, error } = events.last().unwrap() else {
        panic!("expected terminal error event, got {:?}", events.last());
    };
    assert_eq!(*reason, ErrorReason::Error);
    assert_eq!(result.stop_reason, StopReason::Error);
    assert_eq!(
        result.error_message.as_ref(),
        Some(&JsString::from("Proxy error: boom"))
    );
    assert_eq!(error.error_message.as_ref(), Some(&JsString::from("Proxy error: boom")));
}

#[tokio::test]
async fn reports_aborted_when_the_local_signal_is_cancelled() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let fetch = Arc::new(ScriptedFetch {
        status: 200,
        status_text: "OK".to_owned(),
        chunks: vec![sse(&[json!({"type": "start"})])],
        requests: Arc::clone(&requests),
        honour_cancel: true,
    });
    let signal = CancellationToken::new();
    signal.cancel();

    let stream = stream_proxy(
        model(),
        Context {
            system_prompt: None,
            messages: Vec::new(),
            tools: None,
        },
        options(fetch, Some(signal)),
    );
    let (events, result) = collect(stream).await;

    let AssistantMessageEvent::Error { reason, .. } = events.last().unwrap() else {
        panic!("expected terminal error event");
    };
    assert_eq!(*reason, ErrorReason::Aborted);
    assert_eq!(result.stop_reason, StopReason::Aborted);
    assert_eq!(
        result.error_message.as_ref(),
        Some(&JsString::from("This operation was aborted"))
    );
}

#[tokio::test]
async fn rejects_text_delta_for_non_text_content() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let fetch = Arc::new(ScriptedFetch {
        status: 200,
        status_text: "OK".to_owned(),
        chunks: vec![sse(&[
            json!({"type": "start"}),
            json!({"type": "text_delta", "contentIndex": 0, "delta": "x"}),
        ])],
        requests: Arc::clone(&requests),
        honour_cancel: false,
    });

    let stream = stream_proxy(
        model(),
        Context {
            system_prompt: None,
            messages: Vec::new(),
            tools: None,
        },
        options(fetch, None),
    );
    let (events, result) = collect(stream).await;

    let AssistantMessageEvent::Error { reason, .. } = events.last().unwrap() else {
        panic!("expected terminal error event");
    };
    assert_eq!(*reason, ErrorReason::Error);
    assert_eq!(
        result.error_message.as_ref(),
        Some(&JsString::from("Received text_delta for non-text content"))
    );
    assert_eq!(result.content, Vec::<pi_ai::types::AssistantContent>::new());
}
