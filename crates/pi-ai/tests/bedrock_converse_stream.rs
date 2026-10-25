mod bedrock_support;

use std::sync::{Arc, Mutex};

use bedrock_support::*;
use pi_ai::api::bedrock_converse_stream::{stream, stream_simple};
use pi_ai::request::{ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, CacheRetention, Context, ProviderHeaders, StopReason,
    ThinkingLevel, Tool, UserContent, UserMessage,
};
use serde_json::{Value, json};

fn context() -> Context {
    Context {
        system_prompt: None,
        messages: vec![pi_ai::types::Message::User(UserMessage {
            role: pi_ai::types::UserRole::Tag,
            content: UserContent::Text(("hi".to_owned()).into()),
            input_source: None,
            provider_native: None,
            timestamp: 0.0,
        })],
        tools: None,
    }
}

fn options(fetch: Arc<ScriptedFetch>, env: pi_ai::types::ProviderEnv) -> StreamOptions {
    StreamOptions {
        request: ProviderRequestOptions {
            fetch: Some(fetch),
            env: Some(env),
            ..Default::default()
        },
        cache_retention: Some(CacheRetention::None),
        ..Default::default()
    }
}

async fn drain(
    mut stream: pi_ai::utils::event_stream::AssistantMessageEventStream,
) -> (Vec<AssistantMessageEvent>, AssistantMessage) {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.expect("stream must not fail"));
    }
    let message = stream.result().await.expect("stream result");
    (events, message)
}

fn body_json(request: &pi_ai::request::HttpRequest) -> Value {
    serde_json::from_slice(request.body.as_deref().expect("body")).expect("request body JSON")
}

#[tokio::test]
async fn streams_text_usage_and_stop_reason() {
    let body = [
        message_start(),
        text_delta(0, "Hello"),
        text_delta(0, " world"),
        block_stop(0),
        message_stop("end_turn"),
        metadata(10, 2, 3, 4, 19),
    ]
    .concat();
    let fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body, &[3, 8, 5, 13])]);
    let (events, message) = drain(stream(model(), context(), options(fetch.clone(), env()))).await;

    let types: Vec<&str> = events
        .iter()
        .map(|event| match event {
            AssistantMessageEvent::Start { .. } => "start",
            AssistantMessageEvent::TextStart { .. } => "text_start",
            AssistantMessageEvent::TextDelta { .. } => "text_delta",
            AssistantMessageEvent::TextEnd { .. } => "text_end",
            AssistantMessageEvent::Done { .. } => "done",
            _ => "other",
        })
        .collect();
    assert_eq!(
        types,
        ["start", "text_start", "text_delta", "text_delta", "text_end", "done"]
    );
    assert_eq!(
        message.content,
        vec![AssistantContent::Text(pi_ai::types::TextContent::new("Hello world"))]
    );
    assert_eq!(message.stop_reason, StopReason::Stop);
    assert!(message.raw_stop_reason.is_none());
    assert_eq!(message.usage.input, 10.0);
    assert_eq!(message.usage.output, 2.0);
    assert_eq!(message.usage.cache_read, 3.0);
    assert_eq!(message.usage.cache_write, 4.0);
    assert_eq!(message.usage.total_tokens, 19.0);
    assert!((message.usage.cost.total - 0.000_075_9).abs() < 1e-12);

    let requests = fetch.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(
        requests[0].url,
        "https://bedrock-runtime.us-east-1.amazonaws.com/model/us.anthropic.claude-opus-4-8/converse-stream"
    );
    let request_body = body_json(&requests[0]);
    assert_eq!(
        request_body["messages"][0],
        json!({"role": "user", "content": [{"text": "hi"}]})
    );
    assert_eq!(request_body["inferenceConfig"], json!({"maxTokens": 64000}));
    let headers = &requests[0].headers;
    assert!(
        headers
            .iter()
            .any(|(name, value)| name == "authorization"
                && value.starts_with("AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/"))
    );
    assert!(
        headers
            .iter()
            .any(|(name, value)| name == "x-amz-date" && value.ends_with('Z'))
    );
}

#[tokio::test]
async fn streams_reasoning_with_signature() {
    let body = [
        message_start(),
        event_frame(
            "contentBlockDelta",
            json!({"contentBlockIndex": 0, "delta": {"reasoningContent": {"text": "think"}}}),
        ),
        event_frame(
            "contentBlockDelta",
            json!({"contentBlockIndex": 0, "delta": {"reasoningContent": {"signature": "sig"}}}),
        ),
        block_stop(0),
        message_stop("end_turn"),
        metadata(1, 1, 0, 0, 2),
    ]
    .concat();
    let fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body, &[4, 9])]);
    let (events, message) = drain(stream(model(), context(), options(fetch, env()))).await;

    assert!(
        events
            .iter()
            .any(|event| matches!(event, AssistantMessageEvent::ThinkingStart { .. }))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AssistantMessageEvent::ThinkingDelta { delta, .. } if delta == "think"))
    );
    let AssistantContent::Thinking(thinking) = &message.content[0] else {
        panic!("expected thinking block");
    };
    assert_eq!(thinking.thinking, "think");
    assert_eq!(thinking.thinking_signature.as_deref(), Some("sig"));
}

#[tokio::test]
async fn streams_tool_use_with_partial_json() {
    let body = [
        message_start(),
        event_frame(
            "contentBlockStart",
            json!({"contentBlockIndex": 0, "start": {"toolUse": {"toolUseId": "tool-1", "name": "lookup"}}}),
        ),
        event_frame(
            "contentBlockDelta",
            json!({"contentBlockIndex": 0, "delta": {"toolUse": {"input": "{\"q\":"}}}),
        ),
        event_frame(
            "contentBlockDelta",
            json!({"contentBlockIndex": 0, "delta": {"toolUse": {"input": "\"x\"}"}}}),
        ),
        block_stop(0),
        message_stop("tool_use"),
    ]
    .concat();
    let fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body, &[7, 11, 2])]);
    let (events, message) = drain(stream(model(), context(), options(fetch, env()))).await;

    assert!(
        events
            .iter()
            .any(|event| matches!(event, AssistantMessageEvent::ToolCallStart { content_index: 0, .. }))
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, AssistantMessageEvent::ToolCallDelta { delta, .. } if delta == "{\"q\":"))
    );
    let AssistantContent::ToolCall(tool_call) = &message.content[0] else {
        panic!("expected tool call");
    };
    assert_eq!(tool_call.id, "tool-1");
    assert_eq!(tool_call.name, "lookup");
    assert_eq!(
        tool_call.arguments,
        pi_js_compat::js_json::JsValue::from(json!({"q": "x"}))
            .as_object()
            .unwrap()
            .clone()
    );
    assert_eq!(message.stop_reason, StopReason::ToolUse);
    assert_eq!(
        serde_json::to_value(&message).expect("message")["content"][0].get("partialJson"),
        None
    );
}

#[tokio::test]
async fn adds_long_cache_points_to_system_and_last_user_message() {
    let body = [message_start(), message_stop("end_turn")].concat();
    let fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body, &[])]);
    let mut context = context();
    context.system_prompt = Some(("You are helpful.".to_owned()).into());
    let mut stream_options = options(fetch.clone(), env());
    stream_options.cache_retention = Some(CacheRetention::Long);
    let (_, message) = drain(stream(model(), context, stream_options)).await;
    assert_eq!(message.stop_reason, StopReason::Stop);

    let request_body = body_json(&fetch.requests()[0]);
    assert_eq!(request_body["system"][0], json!({"text": "You are helpful."}));
    assert_eq!(
        request_body["system"][1],
        json!({"cachePoint": {"type": "default", "ttl": "1h"}})
    );
    assert_eq!(
        request_body["messages"][0]["content"][1],
        json!({"cachePoint": {"type": "default", "ttl": "1h"}})
    );
}

#[tokio::test]
async fn builds_adaptive_and_budget_thinking_payloads() {
    let body = [message_start(), message_stop("end_turn")].concat();
    let adaptive_fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body.clone(), &[])]);
    let captured: Arc<Mutex<Option<Value>>> = Arc::new(Mutex::new(None));
    let captured_clone = Arc::clone(&captured);
    let mut stream_options = options(adaptive_fetch.clone(), env());
    stream_options.extra.insert("reasoning".to_owned(), json!("xhigh"));
    stream_options.request.on_payload = Some(Arc::new(move |payload, _model| {
        *captured_clone.lock().expect("capture") = Some(json!(payload));
        Box::pin(async { Ok(None) })
    }));
    drain(stream(model(), context(), stream_options)).await;
    let payload = captured.lock().expect("capture").clone().expect("payload");
    assert_eq!(
        payload["additionalModelRequestFields"],
        json!({
            "thinking": {"type": "adaptive", "display": "summarized"},
            "output_config": {"effort": "xhigh"},
        })
    );

    let mut budget_model = model();
    budget_model.id = "us.anthropic.claude-sonnet-4-5-20250929-v1:0".to_owned();
    budget_model.name = "Claude Sonnet 4.5".into();
    let budget_fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body, &[])]);
    let mut stream_options = options(budget_fetch.clone(), env());
    stream_options.extra.insert("reasoning".to_owned(), json!("high"));
    drain(stream(budget_model, context(), stream_options)).await;
    let request_body = body_json(&budget_fetch.requests()[0]);
    assert_eq!(
        request_body["additionalModelRequestFields"],
        json!({
            "thinking": {"type": "enabled", "budget_tokens": 16384, "display": "summarized"},
            "anthropic_beta": ["interleaved-thinking-2025-05-14"],
        })
    );
}

#[tokio::test]
async fn retries_throttling_then_reports_validation_without_extra_diagnostics() {
    let fetch = ScriptedFetch::new(vec![
        ScriptedResponse::http(
            429,
            vec![
                ("content-type", "application/json"),
                (
                    "x-amzn-errortype",
                    "ThrottlingException:http://internal.amazon.com/coral/com.amazon.bedrock/",
                ),
                ("x-amzn-requestid", "req-1"),
                ("retry-after", "0"),
            ],
            r#"{"message":"slow down"}"#,
        ),
        ScriptedResponse::http(
            429,
            vec![
                ("content-type", "application/json"),
                (
                    "x-amzn-errortype",
                    "ThrottlingException:http://internal.amazon.com/coral/com.amazon.bedrock/",
                ),
                ("x-amzn-requestid", "req-2"),
                ("retry-after", "0"),
            ],
            r#"{"message":"slow down"}"#,
        ),
        ScriptedResponse::http(
            400,
            vec![
                ("content-type", "application/json"),
                (
                    "x-amzn-errortype",
                    "ValidationException:http://internal.amazon.com/coral/com.amazon.bedrock/",
                ),
                ("x-amzn-requestid", "req-final"),
            ],
            r#"{"message":"The provided model identifier is invalid."}"#,
        ),
    ]);
    let (_, message) = drain(stream(model(), context(), options(fetch.clone(), env()))).await;

    assert_eq!(message.stop_reason, StopReason::Error);
    assert_eq!(
        message.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from(
            "Validation error: 400: [object Object]"
        ))
    );
    assert!(message.diagnostics.is_none());
    let requests = fetch.requests();
    assert_eq!(requests.len(), 3);
    let attempts: Vec<&str> = requests
        .iter()
        .map(|request| {
            request
                .headers
                .iter()
                .find(|(name, _)| name == "amz-sdk-request")
                .map(|(_, value)| value.as_str())
                .expect("attempt header")
        })
        .collect();
    assert_eq!(attempts, ["attempt=1; max=3", "attempt=2; max=3", "attempt=3; max=3"]);
}

#[tokio::test]
async fn reports_crc_checksum_failures() {
    let corrupt = encode_frame(
        &[
            (":message-type", "event"),
            (":event-type", "contentBlockDelta"),
            (":content-type", "application/json"),
        ],
        br#"{"contentBlockIndex":0,"delta":{"text":"Hello"}}"#,
        true,
    );
    let body = [message_start(), corrupt].concat();
    let fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body, &[])]);
    let (_, message) = drain(stream(model(), context(), options(fetch, env()))).await;

    assert_eq!(message.stop_reason, StopReason::Error);
    let error_message = message.error_message.expect("error message");
    assert!(error_message.starts_with("The message checksum ("), "{error_message:?}");
    assert!(error_message.contains("did not match the expected value of"));
    assert!(message.diagnostics.is_none());
}

#[tokio::test]
async fn aborts_before_the_request() {
    let fetch = ScriptedFetch::new(vec![]);
    let mut stream_options = options(fetch.clone(), env());
    let token_controller = pi_js_compat::abort::AbortController::new();
    let token = token_controller.signal();
    token_controller.abort(None);
    stream_options.request.signal = Some(token);
    let (_, message) = drain(stream(model(), context(), stream_options)).await;

    assert_eq!(message.stop_reason, StopReason::Aborted);
    assert_eq!(
        message.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("Request aborted"))
    );
    assert_eq!(fetch.request_count(), 1);
    assert!(message.diagnostics.is_none());
}

#[tokio::test]
async fn uses_bearer_token_and_skip_auth_credentials() {
    let body = [message_start(), message_stop("end_turn")].concat();
    let bearer_fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body.clone(), &[])]);
    let mut stream_options = options(bearer_fetch.clone(), env());
    stream_options
        .extra
        .insert("bearerToken".to_owned(), json!("bedrock-api-key"));
    drain(stream(model(), context(), stream_options)).await;
    let headers = &bearer_fetch.requests()[0].headers;
    assert!(
        headers
            .iter()
            .any(|(name, value)| name == "Authorization" && value == "Bearer bedrock-api-key")
    );
    assert!(!headers.iter().any(|(name, _)| name == "x-amz-date"));

    let skip_fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body, &[])]);
    let mut skip_env = env();
    skip_env.insert("AWS_BEDROCK_SKIP_AUTH".to_owned(), ("1".to_owned()).into());
    drain(stream(model(), context(), options(skip_fetch.clone(), skip_env))).await;
    let headers = &skip_fetch.requests()[0].headers;
    assert!(
        headers
            .iter()
            .any(|(name, value)| { name == "authorization" && value.contains("Credential=dummy-access-key/") })
    );
}

#[tokio::test]
async fn ignores_reserved_custom_headers() {
    let body = [message_start(), message_stop("end_turn")].concat();
    let fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body, &[])]);
    let mut headers = ProviderHeaders::new();
    headers.insert("x-custom".to_owned(), Some(("v".to_owned()).into()));
    headers.insert("authorization".to_owned(), Some(("evil".to_owned()).into()));
    headers.insert("x-amz-date".to_owned(), Some(("evil".to_owned()).into()));
    headers.insert("HOST".to_owned(), Some(("evil".to_owned()).into()));
    let mut stream_options = options(fetch.clone(), env());
    stream_options.request.headers = Some(headers);
    drain(stream(model(), context(), stream_options)).await;
    let request_headers = &fetch.requests()[0].headers;
    assert!(
        request_headers
            .iter()
            .any(|(name, value)| name == "x-custom" && value == "v")
    );
    assert!(!request_headers.iter().any(|(_, value)| value == "evil"));
    assert!(!request_headers.iter().any(|(name, _)| name == "HOST"));
}

#[tokio::test]
async fn resolves_regions_endpoints_and_inference_profile_arns() {
    let body = [message_start(), message_stop("end_turn")].concat();

    let eu_model = pi_ai::types::Model {
        id: "eu.anthropic.claude-sonnet-4-5-20250929-v1:0".to_owned(),
        base_url: ("https://bedrock-runtime.eu-central-1.amazonaws.com".to_owned()).into(),
        ..model()
    };
    let mut eu_env = env();
    eu_env.shift_remove("AWS_REGION");
    let eu_fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body.clone(), &[])]);
    drain(stream(eu_model.clone(), context(), options(eu_fetch.clone(), eu_env))).await;
    assert_eq!(
        eu_fetch.requests()[0].url,
        "https://bedrock-runtime.eu-central-1.amazonaws.com/model/eu.anthropic.claude-sonnet-4-5-20250929-v1%3A0/converse-stream"
    );

    let mut us_env = env();
    us_env.insert("AWS_REGION".to_owned(), ("us-east-2".to_owned()).into());
    let us_fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body.clone(), &[])]);
    drain(stream(model(), context(), options(us_fetch.clone(), us_env))).await;
    assert_eq!(
        us_fetch.requests()[0].url,
        "https://bedrock-runtime.us-east-2.amazonaws.com/model/us.anthropic.claude-opus-4-8/converse-stream"
    );

    let arn_model = pi_ai::types::Model {
        id: "arn:aws:bedrock:us-west-2:123456789012:application-inference-profile/abc123".to_owned(),
        ..model()
    };
    let arn_fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body.clone(), &[])]);
    drain(stream(arn_model, context(), options(arn_fetch.clone(), env()))).await;
    assert_eq!(
        arn_fetch.requests()[0].url,
        "https://bedrock-runtime.us-west-2.amazonaws.com/model/arn%3Aaws%3Abedrock%3Aus-west-2%3A123456789012%3Aapplication-inference-profile%2Fabc123/converse-stream"
    );
    assert!(
        arn_fetch.requests()[0]
            .headers
            .iter()
            .any(|(name, value)| { name == "authorization" && value.contains("/us-west-2/bedrock/aws4_request") })
    );

    let custom_model = pi_ai::types::Model {
        base_url: ("https://bedrock-vpc.example.com".to_owned()).into(),
        ..model()
    };
    let custom_fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body, &[])]);
    drain(stream(custom_model, context(), options(custom_fetch.clone(), env()))).await;
    assert_eq!(
        custom_fetch.requests()[0].url,
        "https://bedrock-vpc.example.com/model/us.anthropic.claude-opus-4-8/converse-stream"
    );
}

#[tokio::test]
async fn reads_static_profile_credentials() {
    let directory = std::env::temp_dir().join(format!("pi-bedrock-profile-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("temp dir");
    let credentials_path = directory.join("credentials");
    std::fs::write(
        &credentials_path,
        "[dev]\naws_access_key_id = PROFILEKEY\naws_secret_access_key = PROFILESECRET\n",
    )
    .expect("credentials file");

    let body = [message_start(), message_stop("end_turn")].concat();
    let fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body, &[])]);
    let mut profile_env = env();
    profile_env.shift_remove("AWS_ACCESS_KEY_ID");
    profile_env.shift_remove("AWS_SECRET_ACCESS_KEY");
    profile_env.insert(
        "AWS_SHARED_CREDENTIALS_FILE".to_owned(),
        (credentials_path.to_string_lossy().into_owned()).into(),
    );
    let mut stream_options = options(fetch.clone(), profile_env);
    stream_options.extra.insert("profile".to_owned(), json!("dev"));
    drain(stream(model(), context(), stream_options)).await;

    assert!(
        fetch.requests()[0]
            .headers
            .iter()
            .any(|(name, value)| { name == "authorization" && value.contains("Credential=PROFILEKEY/") })
    );
    std::fs::remove_dir_all(directory).ok();
}

#[tokio::test]
async fn surfaces_gateway_bodies_and_stop_reason_errors() {
    let fetch = ScriptedFetch::new(vec![ScriptedResponse::http(
        403,
        vec![("content-type", "text/plain"), ("x-amzn-requestid", "req-403")],
        "blocked by gateway WAF",
    )]);
    let (_, message) = drain(stream(model(), context(), options(fetch, env()))).await;
    assert_eq!(message.stop_reason, StopReason::Error);
    assert_eq!(
        message.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("403: blocked by gateway WAF"))
    );

    let body = [message_start(), message_stop("guardrail_intervened")].concat();
    let fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body, &[])]);
    let (_, message) = drain(stream(model(), context(), options(fetch, env()))).await;
    assert_eq!(message.stop_reason, StopReason::Error);
    assert!(message.raw_stop_reason.is_none());
    assert_eq!(
        message.error_message.as_ref(),
        Some(&pi_js_compat::js_json::JsString::from("An unknown error occurred"))
    );
}

#[tokio::test]
async fn converts_replayed_messages_and_documents() {
    let body = [message_start(), message_stop("end_turn")].concat();
    let fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body, &[])]);
    let mut context = context();
    context.messages = vec![
        pi_ai::types::Message::Assistant(Box::new(AssistantMessage {
            role: pi_ai::types::AssistantRole::Tag,
            content: vec![
                AssistantContent::Thinking(pi_ai::types::ThinkingContent {
                    r#type: pi_ai::types::ThinkingTag::Tag,
                    thinking: ("hmm".to_owned()).into(),
                    thinking_signature: Some("sig".to_owned()),
                    redacted: None,
                    index: None,
                }),
                AssistantContent::Text(pi_ai::types::TextContent::new("calling")),
                AssistantContent::ToolCall(pi_ai::types::ToolCall::new(
                    "tool-1",
                    "lookup",
                    pi_js_compat::js_json::JsValue::from(serde_json::Value::Object(
                        json!({"q": "x", "": ""}).as_object().unwrap().clone(),
                    ))
                    .as_object()
                    .unwrap()
                    .clone(),
                )),
            ],
            api: "bedrock-converse-stream".to_owned(),
            provider: "amazon-bedrock".to_owned(),
            model: model().id,
            response_model: None,
            response_id: None,
            image_generation_result: None,
            diagnostics: None,
            usage: Default::default(),
            stop_reason: StopReason::ToolUse,
            error_message: None,
            raw_stop_reason: None,
            timestamp: 0.0,
        })),
        pi_ai::types::Message::ToolResult(pi_ai::types::ToolResultMessage {
            role: pi_ai::types::ToolResultRole::Tag,
            tool_call_id: "tool-1".to_owned(),
            tool_name: "lookup".to_owned(),
            content: vec![
                pi_ai::types::ToolResultContent::Text(pi_ai::types::TextContent::new("result")),
                pi_ai::types::ToolResultContent::Text(pi_ai::types::TextContent::new("")),
            ],
            details: None,
            is_error: false,
            timestamp: 0.0,
        }),
        pi_ai::types::Message::User(UserMessage {
            role: pi_ai::types::UserRole::Tag,
            content: UserContent::Text(("   ".to_owned()).into()),
            input_source: None,
            provider_native: None,
            timestamp: 0.0,
        }),
    ];
    context.tools = Some(vec![Tool::new(
        "lookup",
        "Look up",
        json!({"type": "object", "properties": {"q": {"type": "string"}}, "required": ["q"]}),
    )]);
    drain(stream(model(), context, options(fetch.clone(), env()))).await;
    let request_body = body_json(&fetch.requests()[0]);

    assert_eq!(
        request_body["messages"][0]["content"],
        json!([
            {"reasoningContent": {"reasoningText": {"text": "hmm", "signature": "sig"}}},
            {"text": "calling"},
            {"toolUse": {"toolUseId": "tool-1", "name": "lookup", "input": {"q": "x", "": ""}}}
        ])
    );
    assert_eq!(
        request_body["messages"][1]["content"][0],
        json!({"toolResult": {"toolUseId": "tool-1", "content": [{"text": "result"}], "status": "success"}})
    );
    assert_eq!(request_body["messages"][2]["content"], json!([{"text": "<empty>"}]));
    assert_eq!(
        request_body["toolConfig"]["tools"][0]["toolSpec"]["name"],
        json!("lookup")
    );
}

#[tokio::test]
async fn stream_simple_adjusts_thinking_budget() {
    let body = [message_start(), message_stop("end_turn")].concat();
    let fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body, &[])]);
    let mut simple = SimpleStreamOptions {
        stream: options(fetch.clone(), env()),
        reasoning: Some(ThinkingLevel::High),
        ..Default::default()
    };
    simple.stream.max_tokens = Some(2000.0);
    let mut budget_model = model();
    budget_model.id = "us.anthropic.claude-sonnet-4-5-20250929-v1:0".to_owned();
    budget_model.name = "Claude Sonnet 4.5".into();
    drain(stream_simple(budget_model, context(), simple)).await;
    let request_body = body_json(&fetch.requests()[0]);
    assert_eq!(request_body["inferenceConfig"]["maxTokens"], json!(18384));
    assert_eq!(
        request_body["additionalModelRequestFields"]["thinking"]["budget_tokens"],
        json!(16384)
    );
}

#[tokio::test]
async fn accepts_tool_choice_and_thinking_display_options() {
    let body = [message_start(), message_stop("end_turn")].concat();
    let fetch = ScriptedFetch::new(vec![ScriptedResponse::ok(body, &[])]);
    let mut context = context();
    context.tools = Some(vec![Tool::new(
        "lookup",
        "Look up",
        json!({"type": "object", "properties": {"q": {"type": "string"}}}),
    )]);
    let mut stream_options = options(fetch.clone(), env());
    stream_options
        .extra
        .insert("toolChoice".to_owned(), json!({"type": "tool", "name": "lookup"}));
    stream_options
        .extra
        .insert("thinkingDisplay".to_owned(), json!("omitted"));
    stream_options.extra.insert("reasoning".to_owned(), json!("medium"));
    stream_options
        .extra
        .insert("thinkingBudgets".to_owned(), json!({"medium": 4096}));
    drain(stream(model(), context, stream_options)).await;
    let request_body = body_json(&fetch.requests()[0]);
    assert_eq!(
        request_body["toolConfig"]["toolChoice"],
        json!({"tool": {"name": "lookup"}})
    );
    assert_eq!(
        request_body["additionalModelRequestFields"]["thinking"],
        json!({"type": "adaptive", "display": "omitted"})
    );
    assert_eq!(
        request_body["additionalModelRequestFields"]["output_config"],
        json!({"effort": "medium"})
    );
}

#[tokio::test(start_paused = true)]
async fn x_amz_retry_after_uses_javascript_number_parsing() {
    // Smithy reads `x-amz-retry-after` with `Number`, so `"0x10"` is 16 ms.
    let body = [message_start(), text_delta(0, "ok"), message_stop("end_turn")].concat();
    let fetch = ScriptedFetch::new(vec![
        ScriptedResponse::http(
            429,
            vec![
                ("content-type", "application/json"),
                (
                    "x-amzn-errortype",
                    "ThrottlingException:http://internal.amazon.com/coral/com.amazon.bedrock/",
                ),
                ("x-amz-retry-after", "0x10"),
            ],
            r#"{"message":"slow down"}"#,
        ),
        ScriptedResponse::ok(body, &[3, 8, 5, 13]),
    ]);

    let task = tokio::spawn(drain(stream(model(), context(), options(fetch.clone(), env()))));
    let mut elapsed_ms = 0;
    while fetch.request_count() < 2 && elapsed_ms < 1000 {
        tokio::time::advance(std::time::Duration::from_millis(1)).await;
        elapsed_ms += 1;
    }
    assert!(
        (16..=25).contains(&elapsed_ms),
        "retry should follow the 16 ms hint, not the exponential backoff: {elapsed_ms} ms"
    );

    let (_, message) = task.await.expect("stream task");
    assert_eq!(message.stop_reason, StopReason::Stop);
}
