use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use pi_agent_core::agent::{ProxyStreamOptions, stream_proxy};
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_ai::types::{
    AssistantMessage, AssistantMessageEvent, Context, ErrorReason, Model, ModelCost, ModelCostRates, StopReason,
};
use pi_js_compat::abort::AbortSignal;
use serde_json::json;

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
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.requests.lock().unwrap().push(request);
        let status = self.status;
        let status_text = self.status_text.clone();
        let chunks = self.chunks.clone();
        let honour_cancel = self.honour_cancel;
        Box::pin(async move {
            if honour_cancel && signal.as_ref().is_some_and(AbortSignal::aborted) {
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
        properties: Default::default(),
        id: "gpt-5.4".to_owned(),
        name: "GPT-5.4".into(),
        api: "openai-responses".to_owned(),
        provider: "openai".to_owned(),
        base_url: JsString::from("https://api.openai.com/v1"),
        reasoning: true,
        thinking_level_map: None,
        input: vec![pi_ai::types::Modality::Text],
        cost: ModelCost {
            properties: Default::default(),
            rates: ModelCostRates::default(),
        },
        context_window: 400000.0,
        auto_compaction_threshold: None,
        max_tokens: 128000.0,
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

fn options(fetch: Arc<dyn Fetch>, signal: Option<AbortSignal>) -> ProxyStreamOptions {
    ProxyStreamOptions {
        auth_token: "test-token".into(),
        proxy_url: "https://proxy.example.com".into(),
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
    opts.stream_options
        .stream
        .extra
        .insert("samplingParams".into(), json!({"top_p":0.5}));
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
    fn fetch(&self, _: HttpRequest, _: Option<AbortSignal>) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
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
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
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
    controller_signal.abort(None);
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
async fn reports_aborted_when_the_local_signal_aborted() {
    let requests = Arc::new(Mutex::new(Vec::new()));
    let fetch = Arc::new(ScriptedFetch {
        status: 200,
        status_text: "OK".to_owned(),
        chunks: vec![sse(&[json!({"type": "start"})])],
        requests: Arc::clone(&requests),
        honour_cancel: true,
    });
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    controller_signal.abort(None);

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

#[path = "../../pi-ai/tests/support/oracle.rs"]
mod oracle;

#[test]
fn unknown_proxy_events_warn_exactly_to_stderr() {
    let mut oracle = oracle::TsOracle::spawn("proxy-stream-oracle.ts", "packages/agent-core/src/proxy.ts");
    let response: serde_json::Value = oracle.request(&json!({"model":model(),"events":[{"type":"future-event"},{"type":"done","reason":"stop","usage":pi_ai::types::Usage::default()}]}));
    assert_eq!(
        response["warnings"],
        json!(["Unhandled proxy event type: future-event"])
    );
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "unknown_proxy_warning_child", "--nocapture"])
        .env("PI_PROXY_WARNING_CHILD", "1")
        .output()
        .unwrap();
    assert!(child.status.success(), "{}", String::from_utf8_lossy(&child.stderr));
    assert_eq!(
        String::from_utf8(child.stderr).unwrap(),
        "Unhandled proxy event type: future-event\n"
    );
    assert!(
        !String::from_utf8(child.stdout)
            .unwrap()
            .contains("Unhandled proxy event type:")
    );
}
#[tokio::test]
async fn unknown_proxy_warning_child() {
    if std::env::var_os("PI_PROXY_WARNING_CHILD").is_none() {
        return;
    }
    let fetch = Arc::new(ScriptedFetch {
        status: 200,
        status_text: "OK".into(),
        chunks: vec![sse(&[
            json!({"type":"future-event"}),
            json!({"type":"done","reason":"stop","usage":pi_ai::types::Usage::default()}),
        ])],
        requests: Arc::new(Mutex::new(vec![])),
        honour_cancel: false,
    });
    let (events, result) = collect(stream_proxy(
        model(),
        Context {
            system_prompt: None,
            messages: vec![],
            tools: None,
        },
        options(fetch, None),
    ))
    .await;
    assert_eq!(events.len(), 1);
    assert_eq!(result.stop_reason, StopReason::Stop);
}

#[tokio::test]
async fn non_contiguous_proxy_index_is_an_explicit_protocol_error() {
    // Frozen proxy.ts:246/280/313 assigns sparse slots; AGENTS 2026-10-22 records
    // the approved gap: native protocol error rather than sparse JS arrays.
    for kind in [
        "text_start",
        "thinking_start",
        "toolcall_start",
        "text_delta",
        "thinking_delta",
        "toolcall_delta",
        "text_end",
        "thinking_end",
        "toolcall_end",
    ] {
        let fetch = Arc::new(ScriptedFetch {
            status: 200,
            status_text: "OK".into(),
            chunks: vec![sse(&[
                json!({"type":kind,"contentIndex":2,"id":"call","toolName":"lookup","delta":"x"}),
            ])],
            requests: Arc::new(Mutex::new(vec![])),
            honour_cancel: false,
        });
        let (_, result) = collect(stream_proxy(
            model(),
            Context {
                system_prompt: None,
                messages: vec![],
                tools: None,
            },
            options(fetch, None),
        ))
        .await;
        assert_eq!(result.stop_reason, StopReason::Error);
        assert_eq!(
            result.error_message.as_ref(),
            Some(&JsString::from(
                "Proxy protocol error: non-contiguous contentIndex 2 (next index: 0)"
            ))
        );
    }
}

fn normalize_proxy(value: &mut pi_js_compat::js_json::JsValue) {
    use pi_js_compat::js_json::JsValue;
    match value {
        JsValue::Array(values) => values.iter_mut().for_each(normalize_proxy),
        JsValue::Object(values) => {
            for (key, value) in values {
                if key == "timestamp" {
                    *value = JsValue::Number(0.0);
                } else {
                    normalize_proxy(value);
                }
            }
        }
        _ => {}
    }
}
#[tokio::test]
async fn proxy_event_matrix_matches_frozen_module() {
    use pi_js_compat::{
        js_json as native_json,
        js_json::{JsValue, parse_lossless, to_value},
    };
    let usage = pi_ai::types::Usage::default();
    let complete = vec![
        json!({"type":"start"}),
        json!({"type":"text_start","contentIndex":0}),
        json!({"type":"text_delta","contentIndex":0,"delta":"a😀"}),
        json!({"type":"text_end","contentIndex":0,"contentSignature":"sig"}),
        json!({"type":"thinking_start","contentIndex":1}),
        json!({"type":"thinking_delta","contentIndex":1,"delta":"thought"}),
        json!({"type":"thinking_end","contentIndex":1,"contentSignature":"thought-sig"}),
        json!({"type":"toolcall_start","contentIndex":2,"id":"call","toolName":"lookup"}),
        json!({"type":"toolcall_delta","contentIndex":2,"delta":"{\"value\":\"native\"}"}),
        json!({"type":"toolcall_end","contentIndex":2}),
        json!({"type":"done","reason":"toolUse","usage":usage}),
    ];
    let done = format!("data: {}\n", json!({"type":"done","reason":"stop","usage":usage}));
    let mut cases = vec![];
    for chunk in [1, 2, 7, 4096] {
        cases.push(native_json!({"model":model(),"events":complete,"chunkSize":chunk}));
    }
    cases.push(native_json!({"model":model(),"wire":format!("\u{feff}data: {{\"type\":\"text_start\",\"contentIndex\":0}}\ndata: {{\"type\":\"text_delta\",\"contentIndex\":0,\"delta\":\"hello\"}}\n{done}"),"chunkSize":1}));
    cases.push(native_json!({"model":model(),"wire":format!("data: \u{feff}{{\"type\":\"text_start\",\"contentIndex\":0}}\n{done}")}));
    cases.push(native_json!({"model":model(),"wire":"data: {oops}\n"}));
    cases.push(native_json!({"model":model(),"wire":format!("{done}data: {{unparsed")}));
    for event in [
        json!({"type":"text_delta","contentIndex":0,"delta":"x"}),
        json!({"type":"text_end","contentIndex":0}),
        json!({"type":"thinking_delta","contentIndex":0,"delta":"x"}),
        json!({"type":"thinking_end","contentIndex":0}),
        json!({"type":"toolcall_delta","contentIndex":0,"delta":"x"}),
        json!({"type":"toolcall_end","contentIndex":0}),
    ] {
        cases.push(
            native_json!({"model":model(),"events":[event,json!({"type":"done","reason":"stop","usage":usage})]}),
        );
    }
    for reason in ["error", "aborted"] {
        cases.push(native_json!({"model":model(),"events":[json!({"type":"error","reason":reason,"errorMessage":"failure","usage":usage})]}));
    }
    let mut oracle = oracle::TsOracle::spawn("proxy-stream-oracle.ts", "packages/agent-core/src/proxy.ts");
    for case in cases {
        let expected: JsValue = oracle.request(&case);
        let wire = case
            .get("wire")
            .and_then(JsValue::as_string)
            .map(|wire| wire.to_utf8().unwrap())
            .unwrap_or_else(|| {
                case["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|event| format!("data: {}\n\n", pi_js_compat::js_json::stringify_lossless(event)))
                    .collect()
            });
        let size = case
            .get("chunkSize")
            .and_then(JsValue::as_f64)
            .map(|size| size as usize)
            .unwrap_or(wire.len().max(1));
        let requests = Arc::new(Mutex::new(vec![]));
        let fetch = Arc::new(ScriptedFetch {
            status: 200,
            status_text: "OK".into(),
            chunks: wire.as_bytes().chunks(size).map(<[u8]>::to_vec).collect(),
            requests: requests.clone(),
            honour_cancel: false,
        });
        let (events, result) = collect(stream_proxy(
            model(),
            Context {
                system_prompt: None,
                messages: vec![],
                tools: None,
            },
            options(fetch, None),
        ))
        .await;
        let captured: Vec<_> = requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| parse_lossless(std::str::from_utf8(request.body.as_ref().unwrap()).unwrap()).unwrap())
            .collect();
        let mut actual = native_json!({"events":to_value(&events).unwrap(),"result":to_value(&result).unwrap(),"requests":captured,"warnings":[]});
        normalize_proxy(&mut actual);
        assert!(
            actual == expected,
            "case {}\nactual {}\nexpected {}",
            pi_js_compat::js_json::stringify_lossless(&case),
            pi_js_compat::js_json::stringify_lossless(&actual),
            pi_js_compat::js_json::stringify_lossless(&expected)
        );
    }
}

#[tokio::test]
async fn done_error_matches_frozen_proxy() {
    use pi_js_compat::{
        js_json as native_json,
        js_json::{JsValue, to_value},
    };
    let terminal = json!({"type":"done","reason":"error","usage":pi_ai::types::Usage::default()});
    let mut oracle = oracle::TsOracle::spawn("proxy-stream-oracle.ts", "packages/agent-core/src/proxy.ts");
    let expected: JsValue = oracle.request(&native_json!({"model":model(),"events":[terminal]}));
    let fetch = Arc::new(ScriptedFetch {
        status: 200,
        status_text: "OK".into(),
        chunks: vec![sse(&[terminal])],
        requests: Arc::new(Mutex::new(vec![])),
        honour_cancel: false,
    });
    let (events, result) = collect(stream_proxy(
        model(),
        Context {
            system_prompt: None,
            messages: vec![],
            tools: None,
        },
        options(fetch, None),
    ))
    .await;
    let mut actual = native_json!({"events":to_value(&events).unwrap(),"result":to_value(&result).unwrap()});
    normalize_proxy(&mut actual);
    assert_eq!(actual["events"], expected["events"]);
    assert_eq!(actual["result"], expected["result"]);
}

#[tokio::test]
async fn proxy_auth_url_and_option_headers_match_node_boundaries() {
    use pi_js_compat::{
        js_json as native_json,
        js_json::{JsValue, parse_lossless, to_value},
    };
    let mut oracle = oracle::TsOracle::spawn("proxy-stream-oracle.ts", "packages/agent-core/src/proxy.ts");
    let done = json!({"type":"done","reason":"stop","usage":pi_ai::types::Usage::default()});
    for (auth, url) in [
        (
            JsString::from_utf16(vec![0xd800]),
            JsString::from("https://proxy.invalid"),
        ),
        ("abc".into(), "invalid-url".into()),
        ("abc ".into(), "https://proxy.invalid".into()),
        ("ab\ncd".into(), "https://proxy.invalid".into()),
        (
            "abc".into(),
            JsString::from_utf16("https://proxy.invalid/".encode_utf16().chain([0xd800]).collect()),
        ),
    ] {
        let case = native_json!({"boundary":true,"model":model(),"events":[done],"options":{"authToken":auth,"proxyUrl":url,"headers":{"keep":"value","remove":"old"}},"headerDeletes":["remove"]});
        let expected: JsValue = oracle.request(&case);
        let requests = Arc::new(Mutex::new(vec![]));
        let fetch = Arc::new(ScriptedFetch {
            status: 200,
            status_text: "OK".into(),
            chunks: vec![sse(std::slice::from_ref(&done))],
            requests: requests.clone(),
            honour_cancel: false,
        });
        let mut opts = options(fetch, None);
        opts.auth_token = auth;
        opts.proxy_url = url;
        opts.stream_options.stream.request.headers =
            Some([("keep".into(), Some("value".into())), ("remove".into(), None)].into());
        let (events, result) = collect(stream_proxy(
            model(),
            Context {
                system_prompt: None,
                messages: vec![],
                tools: None,
            },
            opts,
        ))
        .await;
        let captured: Vec<_> = requests.lock().unwrap().iter().map(|request| {let headers: pi_js_compat::js_json::JsObject = request.headers.iter().map(|(key,value)| (key.to_ascii_lowercase().into(),JsValue::String(value.clone().into()))).collect(); native_json!({"url":request.url,"headers":JsValue::Object(headers),"body":parse_lossless(std::str::from_utf8(request.body.as_ref().unwrap()).unwrap()).unwrap()})}).collect();
        let mut actual = native_json!({"events":to_value(&events).unwrap(),"result":to_value(&result).unwrap(),"requests":captured,"warnings":[]});
        normalize_proxy(&mut actual);
        assert!(
            actual == expected,
            "case {}\nactual {}\nexpected {}",
            pi_js_compat::js_json::stringify_lossless(&case),
            pi_js_compat::js_json::stringify_lossless(&actual),
            pi_js_compat::js_json::stringify_lossless(&expected)
        );
    }
}

#[tokio::test]
async fn unterminated_proxy_stream_keeps_result_pending_like_frozen() {
    use pi_js_compat::{
        js_json as native_json,
        js_json::{JsValue, parse_lossless, to_value},
    };
    let mut oracle = oracle::TsOracle::spawn("proxy-stream-oracle.ts", "packages/agent-core/src/proxy.ts");
    for wire in [
        "",
        "data: {\"type\":\"start\"}\ndata: {\"type\":\"text_start\",\"contentIndex\":0}\ndata: {unparsed",
    ] {
        let case = native_json!({"expectPending":true,"model":model(),"wire":wire});
        let expected: JsValue = oracle.request(&case);
        let requests = Arc::new(Mutex::new(vec![]));
        let fetch = Arc::new(ScriptedFetch {
            status: 200,
            status_text: "OK".into(),
            chunks: vec![wire.as_bytes().to_vec()],
            requests: requests.clone(),
            honour_cancel: false,
        });
        let mut stream = stream_proxy(
            model(),
            Context {
                system_prompt: None,
                messages: vec![],
                tools: None,
            },
            options(fetch, None),
        );
        let mut events = vec![];
        while let Some(event) = stream.next().await {
            events.push(event.unwrap());
        }
        let pending = tokio::time::timeout(std::time::Duration::from_millis(10), stream.result())
            .await
            .is_err();
        let captured: Vec<_> = requests
            .lock()
            .unwrap()
            .iter()
            .map(|request| parse_lossless(std::str::from_utf8(request.body.as_ref().unwrap()).unwrap()).unwrap())
            .collect();
        let mut actual =
            native_json!({"events":to_value(&events).unwrap(),"pending":pending,"requests":captured,"warnings":[]});
        normalize_proxy(&mut actual);
        assert_eq!(actual, expected);
    }
}
