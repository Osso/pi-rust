use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use pi_ai::api::mistral_conversations::{MistralOptions, MistralPromptMode, MistralToolChoice, stream, stream_simple};
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse, SimpleStreamOptions};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, Context, Modality, Model, ModelCost, ModelCostRates,
    StopReason, TextContent, TextOrImageContent, ThinkingContent, Tool, ToolCall, ToolResultMessage, ToolResultRole,
    Usage, UserContent, UserMessage, UserRole,
};
use pi_js_compat::abort::AbortSignal;
use serde_json::{Value, json};

const BASE_URL: &str = "https://api.mistral.ai";

struct ScriptedFetch {
    requests: Arc<Mutex<Vec<HttpRequest>>>,
    response: ScriptedResponse,
}

struct ScriptedResponse {
    status: u16,
    status_text: String,
    headers: Vec<(String, String)>,
    chunks: Vec<Vec<u8>>,
    hang: bool,
}

impl Fetch for ScriptedFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        self.requests.lock().unwrap().push(request);
        let response = &self.response;
        let body = ChunkBody {
            chunks: response.chunks.iter().cloned().collect(),
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

fn model(id: &str, reasoning: bool) -> Model {
    Model {
        properties: Default::default(),
        id: id.to_owned(),
        name: "Test Model".into(),
        api: "mistral-conversations".to_owned(),
        provider: "mistral".to_owned(),
        base_url: (BASE_URL.to_owned()).into(),
        reasoning,
        thinking_level_map: None,
        input: vec![Modality::Text, Modality::Image],
        cost: ModelCost {
            properties: Default::default(),
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

fn user(text: &str) -> UserMessage {
    UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text((text.to_owned()).into()),
        input_source: None,
        provider_native: None,
        timestamp: 1.0,
    }
}

fn context() -> Context {
    Context {
        system_prompt: None,
        messages: vec![pi_ai::types::Message::User(user("hello"))],
        tools: None,
    }
}

fn empty_usage() -> Usage {
    Usage::default()
}

fn scripted_response(chunks: Vec<String>) -> ScriptedResponse {
    ScriptedResponse {
        status: 200,
        status_text: "OK".to_owned(),
        headers: vec![("content-type".to_owned(), "text/event-stream".to_owned())],
        chunks: chunks.into_iter().map(String::into_bytes).collect(),
        hang: false,
    }
}

fn sse(events: &[Value]) -> ScriptedResponse {
    let body = events
        .iter()
        .map(|event| format!("data: {event}"))
        .collect::<Vec<_>>()
        .join("\r\n\r\n");
    scripted_response(vec![format!("{body}\r\n\r\ndata: [DONE]\r\n\r\n")])
}

fn fetch_with(response: ScriptedResponse) -> (Arc<ScriptedFetch>, Arc<Mutex<Vec<HttpRequest>>>) {
    let requests = Arc::new(Mutex::new(Vec::new()));
    (
        Arc::new(ScriptedFetch {
            requests: Arc::clone(&requests),
            response,
        }),
        requests,
    )
}

fn options(fetch: Arc<ScriptedFetch>, api_key: &str) -> MistralOptions {
    let mut options = MistralOptions::default();
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

fn terminal_event(finish_reason: &str) -> Value {
    json!({
        "id": "mistral-response-id",
        "model": "mistral-large-latest",
        "choices": [{"index": 0, "finish_reason": finish_reason, "delta": {}}],
        "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2},
    })
}

#[tokio::test]
async fn serializes_sdk_style_payloads_to_the_mistral_wire_format() {
    let (fetch, requests) = fetch_with({
        let mut response = sse(&[terminal_event("stop")]);
        response
            .headers
            .push(("x-request-id".to_owned(), "request-1".to_owned()));
        response
    });
    let mut context = context();
    context.system_prompt = Some(("Be precise".to_owned()).into());
    context.messages = vec![pi_ai::types::Message::User(UserMessage {
        role: UserRole::Tag,
        content: UserContent::Blocks(vec![
            TextOrImageContent::Text(TextContent::new("describe")),
            TextOrImageContent::Image(pi_ai::types::ImageContent::new("aGVsbG8=", "image/png")),
        ]),
        input_source: None,
        provider_native: None,
        timestamp: 1.0,
    })];
    context.tools = Some(vec![Tool::new(
        "lookup",
        "Look something up",
        json!({
            "type": "object",
            "properties": {"query": {"type": "string"}},
            "required": ["query"],
            "additionalProperties": false,
        }),
    )]);

    let captured = Arc::new(Mutex::new(None));
    let captured_clone = Arc::clone(&captured);
    let mut options = options(fetch, "secret");
    options.stream.max_tokens = Some(123.0);
    options.prompt_mode = Some(MistralPromptMode::Reasoning);
    options.reasoning_effort = Some("high".into());
    options.tool_choice = Some(MistralToolChoice::Function {
        name: "lookup".to_owned(),
    });
    options.stream.session_id = Some(("session-1".to_owned()).into());
    options.stream.request.headers = Some(
        [(
            "x-custom".to_owned(),
            Some(pi_js_compat::js_json::JsString::from("value")),
        )]
        .into_iter()
        .collect(),
    );
    options.stream.request.on_payload = Some(Arc::new(
        move |payload: pi_js_compat::js_json::JsValue, _model: Model| {
            let captured = Arc::clone(&captured_clone);
            Box::pin(async move {
                *captured.lock().unwrap() = Some(payload.clone());
                let mut augmented = payload;
                let object = augmented.as_object_mut().unwrap();
                object.insert(
                    ("topP".to_owned()).into(),
                    pi_js_compat::js_json::JsValue::from(json!(0.9)),
                );
                object.insert(
                    ("randomSeed".to_owned()).into(),
                    pi_js_compat::js_json::JsValue::from(json!(42)),
                );
                object.insert(
                    ("responseFormat".to_owned()).into(),
                    pi_js_compat::js_json::JsValue::from(json!({
                        "type": "json_schema",
                        "jsonSchema": {
                            "name": "result",
                            "schemaDefinition": {
                                "type": "object",
                                "properties": {"maxTokens": {"type": "number"}},
                            },
                        },
                    })),
                );
                object.insert(
                    ("presencePenalty".to_owned()).into(),
                    pi_js_compat::js_json::JsValue::from(json!(0.1)),
                );
                object.insert(
                    ("frequencyPenalty".to_owned()).into(),
                    pi_js_compat::js_json::JsValue::from(json!(0.2)),
                );
                object.insert(
                    ("parallelToolCalls".to_owned()).into(),
                    pi_js_compat::js_json::JsValue::from(json!(true)),
                );
                object.insert(
                    ("safePrompt".to_owned()).into(),
                    pi_js_compat::js_json::JsValue::from(json!(true)),
                );
                Ok(Some(augmented))
            })
        },
    ));

    let observed = Arc::new(Mutex::new(None));
    let observed_clone = Arc::clone(&observed);
    options.stream.request.on_response = Some(Arc::new(move |response, _model| {
        let observed = Arc::clone(&observed_clone);
        Box::pin(async move {
            *observed.lock().unwrap() = Some(response);
            Ok(())
        })
    }));

    let (_, message) = collect(stream(model("mistral-large-latest", false), context, options)).await;
    assert_eq!(message.stop_reason, StopReason::Stop);

    let payload = captured.lock().unwrap().clone().expect("payload captured");
    assert_eq!(payload["maxTokens"], pi_js_compat::js_json::JsValue::from(json!(123)));
    assert_eq!(
        payload["promptMode"],
        pi_js_compat::js_json::JsValue::from(json!("reasoning"))
    );
    assert_eq!(
        payload["promptCacheKey"],
        pi_js_compat::js_json::JsValue::from(json!("session-1"))
    );

    assert!(
        observed.lock().unwrap().is_none(),
        "frozen Mistral does not call onResponse"
    );

    let request = requests.lock().unwrap().first().cloned().expect("request");
    assert_eq!(request.url, "https://api.mistral.ai/v1/chat/completions");
    let headers = request
        .headers
        .iter()
        .cloned()
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(headers["authorization"], "Bearer secret");
    assert_eq!(headers["accept"], "text/event-stream");
    assert_eq!(headers["x-affinity"], "session-1");
    assert_eq!(headers["x-custom"], "value");

    let wire: Value = serde_json::from_slice(&request.body.expect("body")).expect("wire JSON");
    assert_eq!(wire["max_tokens"], json!(123));
    assert_eq!(wire["prompt_mode"], json!("reasoning"));
    assert_eq!(wire["reasoning_effort"], json!("high"));
    assert_eq!(
        wire["tool_choice"],
        json!({"type": "function", "function": {"name": "lookup"}})
    );
    assert_eq!(wire["prompt_cache_key"], json!("session-1"));
    assert_eq!(wire["top_p"], json!(0.9));
    assert_eq!(wire["random_seed"], json!(42));
    assert_eq!(wire["presence_penalty"], json!(0.1));
    assert_eq!(wire["frequency_penalty"], json!(0.2));
    assert_eq!(wire["parallel_tool_calls"], json!(true));
    assert_eq!(wire["safe_prompt"], json!(true));
    assert_eq!(
        wire["response_format"],
        json!({
            "type": "json_schema",
            "json_schema": {
                "name": "result",
                "schema": {
                    "type": "object",
                    "properties": {"maxTokens": {"type": "number"}},
                },
            },
        })
    );
    assert!(wire.get("maxTokens").is_none());
    assert!(wire.get("promptMode").is_none());
    assert!(wire.get("promptCacheKey").is_none());
    assert_eq!(
        wire["messages"],
        json!([
            {"role": "system", "content": "Be precise"},
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "describe"},
                    {"type": "image_url", "image_url": "data:image/png;base64,aGVsbG8="},
                ],
            },
        ])
    );
}

#[tokio::test]
async fn serializes_assistant_thinking_tool_calls_and_tool_results_for_replay() {
    let (fetch, requests) = fetch_with(sse(&[terminal_event("stop")]));
    let assistant = AssistantMessage {
        content: vec![
            AssistantContent::Thinking(ThinkingContent::new("reason")),
            AssistantContent::Text(TextContent::new("answer")),
            AssistantContent::ToolCall(ToolCall::new(
                "abc123456",
                "lookup",
                pi_js_compat::js_json::JsValue::from(serde_json::Value::Object(
                    json!({"query": "pi"}).as_object().unwrap().clone(),
                ))
                .as_object()
                .unwrap()
                .clone(),
            )),
        ],
        stop_reason: StopReason::ToolUse,
        usage: empty_usage(),
        ..AssistantMessage::empty("mistral-conversations", "mistral", "mistral-large-latest", 1.0)
    };
    let tool_result = ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: "abc123456".to_owned(),
        tool_name: "lookup".to_owned(),
        content: vec![
            TextOrImageContent::Text(TextContent::new("found")),
            TextOrImageContent::Image(pi_ai::types::ImageContent::new("aGVsbG8=", "image/png")),
        ],
        details: None,
        is_error: false,
        timestamp: 2.0,
    };
    let context = Context {
        system_prompt: None,
        messages: vec![
            pi_ai::types::Message::Assistant(Box::new(assistant)),
            pi_ai::types::Message::ToolResult(tool_result),
        ],
        tools: None,
    };

    let (_, message) = collect(stream(
        model("mistral-large-latest", false),
        context,
        options(fetch, "test"),
    ))
    .await;
    assert_eq!(message.stop_reason, StopReason::Stop);

    let request = requests.lock().unwrap().first().cloned().expect("request");
    let wire: Value = serde_json::from_slice(&request.body.expect("body")).expect("wire JSON");
    assert_eq!(
        wire["messages"],
        json!([
            {
                "role": "assistant",
                "prefix": false,
                "content": [
                    {"type": "thinking", "thinking": [{"type": "text", "text": "reason"}]},
                    {"type": "text", "text": "answer"},
                ],
                "tool_calls": [
                    {
                        "id": "abc123456",
                        "type": "function",
                        "function": {"name": "lookup", "arguments": "{\"query\":\"pi\"}"},
                        "index": 0,
                    },
                ],
            },
            {
                "role": "tool",
                "tool_call_id": "abc123456",
                "name": "lookup",
                "content": [
                    {"type": "text", "text": "found"},
                    {"type": "image_url", "image_url": "data:image/png;base64,aGVsbG8="},
                ],
            },
        ])
    );
}

#[tokio::test]
async fn parses_native_thinking_text_tool_calls_and_cached_token_usage() {
    let events = vec![
        json!({
            "id": "response-1",
            "model": "mistral-large-latest",
            "choices": [{"index": 0, "finish_reason": null,
                "delta": {"content": [{"type": "thinking", "thinking": [{"type": "text", "text": "reason"}]}]}}],
        }),
        json!({
            "id": "response-1",
            "model": "mistral-large-latest",
            "choices": [{"index": 0, "finish_reason": null,
                "delta": {"content": [{"type": "text", "text": "answer"}]}}],
        }),
        json!({
            "id": "response-1",
            "model": "mistral-large-latest",
            "choices": [{"index": 0, "finish_reason": null,
                "delta": {"tool_calls": [{"id": "abc123456", "index": 0,
                    "function": {"name": "lookup", "arguments": "{\"query\":"}}]}}],
        }),
        json!({
            "id": "response-1",
            "model": "mistral-large-latest",
            "choices": [{"index": 0, "finish_reason": "tool_calls",
                "delta": {"tool_calls": [{"id": "abc123456", "index": 0,
                    "function": {"name": "lookup", "arguments": "\"pi\"}"}}]}}],
            "usage": {"prompt_tokens": 10, "completion_tokens": 4, "total_tokens": 14,
                "prompt_tokens_details": {"cached_tokens": 3}},
        }),
    ];
    let (fetch, _) = fetch_with(sse(&events));

    let (_, message) = collect(stream(
        model("mistral-large-latest", false),
        context(),
        options(fetch, "test"),
    ))
    .await;

    assert_eq!(message.stop_reason, StopReason::ToolUse);
    assert!(serde_json::to_value(&message).unwrap().get("rawStopReason").is_none());
    assert_eq!(message.response_id.as_deref(), Some("response-1"));
    assert_eq!(
        message.content,
        vec![
            AssistantContent::Thinking(ThinkingContent::new("reason")),
            AssistantContent::Text(TextContent::new("answer")),
            AssistantContent::ToolCall(ToolCall::new(
                "abc123456",
                "lookup",
                pi_js_compat::js_json::JsValue::from(serde_json::Value::Object(
                    json!({"query": "pi"}).as_object().unwrap().clone()
                ))
                .as_object()
                .unwrap()
                .clone(),
            )),
        ]
    );
    assert_eq!(message.usage.input, 7.0);
    assert_eq!(message.usage.output, 4.0);
    assert_eq!(message.usage.cache_read, 3.0);
    assert_eq!(message.usage.cache_write, 0.0);
    assert_eq!(message.usage.total_tokens, 14.0);
}

#[tokio::test]
async fn parses_sse_and_utf8_sequences_split_across_transport_chunks() {
    let body = format!(
        "data: {}\r\n\r\ndata: [DONE]\r\n\r\n",
        json!({
            "id": "response-bytewise",
            "model": "mistral-large-latest",
            "choices": [{"index": 0, "finish_reason": "stop", "delta": {"content": "héllo 🌍"}}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 2, "total_tokens": 3},
        })
    );
    let bytes = body.into_bytes();
    let mut response = scripted_response(Vec::new());
    response.chunks = bytes.into_iter().map(|byte| vec![byte]).collect();
    let (fetch, _) = fetch_with(response);

    let (_, message) = collect(stream(
        model("mistral-large-latest", false),
        context(),
        options(fetch, "test"),
    ))
    .await;

    assert_eq!(message.stop_reason, StopReason::Stop);
    assert_eq!(
        message.content,
        vec![AssistantContent::Text(TextContent::new("héllo 🌍"))]
    );
}

#[tokio::test]
async fn merges_case_distinct_headers_before_sdk_normalization() {
    let (fetch, requests) = fetch_with(sse(&[terminal_event("stop")]));
    let mut model = model("mistral-large-latest", false);
    model.headers = Some(
        [
            (
                "Authorization".to_owned(),
                pi_js_compat::js_json::JsString::from("Bearer model-key"),
            ),
            (
                "X-Affinity".to_owned(),
                pi_js_compat::js_json::JsString::from("model-affinity"),
            ),
        ]
        .into_iter()
        .collect(),
    );
    let mut options = options(fetch, "request-key");
    options.stream.session_id = Some(("automatic-affinity".to_owned()).into());
    options.stream.request.headers = Some(
        [("authorization".to_owned(), None), ("x-affinity".to_owned(), None)]
            .into_iter()
            .collect(),
    );

    collect(stream(model, context(), options)).await;

    let request = requests.lock().unwrap().first().cloned().expect("request");
    let headers: std::collections::BTreeMap<_, _> = request.headers.iter().cloned().collect();
    assert_eq!(headers["authorization"], "Bearer model-key, null");
    assert_eq!(headers["x-affinity"], "model-affinity, automatic-affinity");
}

#[tokio::test]
async fn aborts_while_waiting_for_an_sse_chunk() {
    let mut response = scripted_response(Vec::new());
    response.hang = true;
    let (fetch, _) = fetch_with(response);
    let signal_controller = pi_js_compat::abort::AbortController::new();
    let signal = signal_controller.signal();
    let mut options = options(fetch, "test");
    options.stream.request.signal = Some(signal.clone());

    let result = stream(model("mistral-large-latest", false), context(), options);
    signal_controller.abort(None);
    let (_, message) = collect(result).await;

    assert_eq!(message.stop_reason, StopReason::Aborted);
    assert_eq!(
        message.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("This operation was aborted"))
    );
}

#[tokio::test(start_paused = true)]
async fn applies_the_sdk_default_deadline_while_ignoring_provider_timeout_option() {
    let mut response = scripted_response(Vec::new());
    response.hang = true;
    let (fetch, _) = fetch_with(response);
    let mut options = options(fetch, "test");
    options.stream.request.timeout_ms = Some(5.0);

    let before = tokio::time::Instant::now();
    let (_, message) = collect(stream(model("mistral-large-latest", false), context(), options)).await;

    assert_eq!(before.elapsed(), std::time::Duration::from_secs(60));
    assert_eq!(message.stop_reason, StopReason::Error);
    assert_eq!(
        message.error_message,
        Some("The operation was aborted due to timeout".into())
    );
}

#[tokio::test]
async fn preserves_http_status_and_response_bodies_in_errors() {
    let (fetch, _) = fetch_with(ScriptedResponse {
        status: 403,
        status_text: "Forbidden".to_owned(),
        headers: Vec::new(),
        chunks: vec![br#"{"message":"blocked by gateway"}"#.to_vec()],
        hang: false,
    });

    let (_, message) = collect(stream(
        model("mistral-large-latest", false),
        context(),
        options(fetch, "test"),
    ))
    .await;

    assert_eq!(message.stop_reason, StopReason::Error);
    assert_eq!(
        message.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from(
            r#"Mistral API error (403): {"message":"blocked by gateway"}"#
        ))
    );
}

#[tokio::test]
async fn maps_frozen_finish_reasons_without_raw_metadata() {
    let scenarios = [
        ("stop", StopReason::Stop, None),
        ("error", StopReason::Error, Some("An unknown error occurred")),
        ("unmapped_error", StopReason::Stop, None),
    ];
    for (finish_reason, expected_stop, expected_error) in scenarios {
        let (fetch, _) = fetch_with(sse(&[terminal_event(finish_reason)]));
        let (_, message) = collect(stream(
            model("devstral-medium-latest", false),
            context(),
            options(fetch, "test"),
        ))
        .await;

        assert_eq!(message.stop_reason, expected_stop, "{finish_reason}");
        assert!(serde_json::to_value(&message).unwrap().get("rawStopReason").is_none());
        assert_eq!(
            message.error_message.as_ref(),
            expected_error.map(pi_js_compat::js_json::JsString::from).as_ref()
        );
    }
}

#[tokio::test]
async fn normalizes_invalid_tool_call_ids_for_replay() {
    let (fetch, requests) = fetch_with(sse(&[terminal_event("stop")]));
    let assistant = AssistantMessage {
        content: vec![
            AssistantContent::ToolCall(ToolCall::new(
                "call_1|fc_1",
                "read",
                pi_js_compat::js_json::JsValue::from(serde_json::Value::Object(json!({}).as_object().unwrap().clone()))
                    .as_object()
                    .unwrap()
                    .clone(),
            )),
            AssistantContent::ToolCall(ToolCall::new(
                "abc123456",
                "lookup",
                pi_js_compat::js_json::JsValue::from(serde_json::Value::Object(json!({}).as_object().unwrap().clone()))
                    .as_object()
                    .unwrap()
                    .clone(),
            )),
        ],
        stop_reason: StopReason::ToolUse,
        usage: empty_usage(),
        ..AssistantMessage::empty("mistral-conversations", "mistral", "mistral-large-latest", 1.0)
    };
    let tool_result = ToolResultMessage {
        role: ToolResultRole::Tag,
        tool_call_id: "call_1|fc_1".to_owned(),
        tool_name: "read".to_owned(),
        content: vec![TextOrImageContent::Text(TextContent::new("done"))],
        details: None,
        is_error: false,
        timestamp: 2.0,
    };
    let context = Context {
        system_prompt: None,
        messages: vec![
            pi_ai::types::Message::Assistant(Box::new(assistant)),
            pi_ai::types::Message::ToolResult(tool_result),
        ],
        tools: None,
    };

    collect(stream(
        model("mistral-large-latest", false),
        context,
        options(fetch, "test"),
    ))
    .await;

    let request = requests.lock().unwrap().first().cloned().expect("request");
    let wire: Value = serde_json::from_slice(&request.body.expect("body")).expect("wire JSON");
    let normalized = wire["messages"][0]["tool_calls"][0]["id"]
        .as_str()
        .expect("tool call id")
        .to_owned();
    assert_eq!(normalized.len(), 9);
    assert_eq!(wire["messages"][0]["tool_calls"][1]["id"], json!("abc123456"));
    assert_eq!(wire["messages"][1]["tool_call_id"], json!(normalized));
}

async fn capture_reasoning_payload(model_id: &str, reasoning: Option<pi_ai::types::ThinkingLevel>) -> Value {
    let (fetch, _) = fetch_with(ScriptedResponse {
        status: 500,
        status_text: "Internal Server Error".to_owned(),
        headers: Vec::new(),
        chunks: vec![b"{}".to_vec()],
        hang: false,
    });
    let captured = Arc::new(Mutex::new(None));
    let captured_clone = Arc::clone(&captured);
    let mut stream_options = SimpleStreamOptions::default();
    stream_options.stream.request.fetch = Some(fetch);
    stream_options.stream.request.api_key = Some(("fake-key".to_owned()).into());
    stream_options.stream.request.on_payload = Some(Arc::new(
        move |payload: pi_js_compat::js_json::JsValue, _model: Model| {
            let captured = Arc::clone(&captured_clone);
            Box::pin(async move {
                *captured.lock().unwrap() = Some(payload.clone());
                Ok(None)
            })
        },
    ));
    stream_options.reasoning = reasoning;

    let event_stream = stream_simple(model(model_id, true), context(), stream_options);
    let (_, message) = collect(event_stream).await;
    assert_eq!(message.stop_reason, StopReason::Error);
    json!(captured.lock().unwrap().clone().expect("payload captured"))
}
#[tokio::test]
async fn selects_reasoning_controls_per_mistral_model() {
    let payload = capture_reasoning_payload("mistral-small-2603", Some(pi_ai::types::ThinkingLevel::Medium)).await;
    assert_eq!(payload["reasoningEffort"], json!("high"));
    assert!(payload.get("promptMode").is_none());

    let payload = capture_reasoning_payload("mistral-small-2603", None).await;
    assert!(payload.get("reasoningEffort").is_none());
    assert!(payload.get("promptMode").is_none());

    let payload = capture_reasoning_payload("magistral-medium-latest", Some(pi_ai::types::ThinkingLevel::Medium)).await;
    assert_eq!(payload["promptMode"], json!("reasoning"));
    assert!(payload.get("reasoningEffort").is_none());

    let payload = capture_reasoning_payload("mistral-medium-3.5", Some(pi_ai::types::ThinkingLevel::Medium)).await;
    assert_eq!(payload["reasoningEffort"], json!("high"));
    assert!(payload.get("promptMode").is_none());
}

#[tokio::test]
async fn uses_the_session_id_as_prompt_cache_key_unless_disabled() {
    let (fetch, requests) = fetch_with(sse(&[terminal_event("stop")]));
    let mut stream_options = options(fetch, "test");
    stream_options.stream.session_id = Some(("session-123".to_owned()).into());
    collect(stream(model("mistral-large-latest", false), context(), stream_options)).await;
    let request = requests.lock().unwrap().first().cloned().expect("request");
    let wire: Value = serde_json::from_slice(&request.body.expect("body")).expect("wire JSON");
    assert_eq!(wire["prompt_cache_key"], json!("session-123"));

    let (fetch, requests) = fetch_with(sse(&[terminal_event("stop")]));
    let mut stream_options = options(fetch, "test");
    stream_options.stream.session_id = Some(("session-123".to_owned()).into());
    stream_options.stream.cache_retention = Some(pi_ai::types::CacheRetention::None);
    collect(stream(model("mistral-large-latest", false), context(), stream_options)).await;
    let request = requests.lock().unwrap().first().cloned().expect("request");
    let wire: Value = serde_json::from_slice(&request.body.expect("body")).expect("wire JSON");
    assert!(wire.get("prompt_cache_key").is_none());
}

#[tokio::test]
async fn errors_when_no_api_key_is_provided() {
    let mut options = MistralOptions::default();
    options.stream.request.fetch = Some(fetch_with(sse(&[terminal_event("stop")])).0);
    let (_, message) = collect(stream(model("mistral-large-latest", false), context(), options)).await;
    assert_eq!(message.stop_reason, StopReason::Error);
    assert!(
        message
            .error_message
            .clone()
            .unwrap_or_default()
            .contains("No API key for provider: mistral")
    );
}

#[tokio::test]
async fn errors_on_malformed_stream_events() {
    let (fetch, _) = fetch_with(scripted_response(vec!["data: {\"choices\": 3}\n\n".to_owned()]));
    let (_, message) = collect(stream(
        model("mistral-large-latest", false),
        context(),
        options(fetch, "test"),
    ))
    .await;
    assert_eq!(message.stop_reason, StopReason::Error);
    let error = message.error_message.unwrap().to_utf8().unwrap();
    let issues: Value = serde_json::from_str(&error).unwrap();
    assert_eq!(
        issues,
        json!([
            {"expected":"string","code":"invalid_type","path":["data","id"],"message":"Invalid input: expected string, received undefined"},
            {"expected":"string","code":"invalid_type","path":["data","model"],"message":"Invalid input: expected string, received undefined"},
            {"expected":"array","code":"invalid_type","path":["data","choices"],"message":"Invalid input: expected array, received number"}
        ])
    );
}

#[tokio::test]
async fn accepts_a_valid_open_stream_without_a_terminal_finish_reason() {
    let (fetch, _) = fetch_with(scripted_response(vec![
        "data: {\"id\":\"open\",\"model\":\"mistral-large-latest\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"partial\"},\"finish_reason\":null}]}\n\n".to_owned(),
    ]));
    let (_, message) = collect(stream(
        model("mistral-large-latest", false),
        context(),
        options(fetch, "test"),
    ))
    .await;
    assert!(message.error_message.is_none());
    assert_eq!(message.stop_reason, StopReason::Stop);
    assert_eq!(
        message.content,
        vec![AssistantContent::Text(TextContent::new("partial"))]
    );
}
