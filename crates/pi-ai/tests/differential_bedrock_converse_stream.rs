#[path = "support/provider_normalize.rs"]
pub mod provider_normalize;

#[path = "support/oracle.rs"]
mod oracle;

use std::collections::BTreeMap;
use std::sync::Arc;

use pi_ai::api::bedrock_converse_stream::{stream, stream_simple};
use pi_ai::api::bedrock_sigv4::{AwsCredentials, sign_request};
use pi_ai::request::{HttpRequest, ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, CacheRetention, Context, Message, Model, ProviderEnv,
    StopReason, Tool, ToolResultContent, ToolResultMessage, ToolResultRole, UserContent, UserMessage, UserRole,
};
use pi_js_compat::abort::AbortSignal;
use serde_json::{Map, Value, json};

const ACCESS_KEY: &str = "AKIDEXAMPLE";
const SECRET_KEY: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";

#[derive(Clone)]
enum FrameSpec {
    Event {
        event_type: String,
        payload: Value,
        corrupt_message: bool,
        corrupt_prelude: bool,
    },
    Exception {
        exception_type: String,
        payload: Value,
    },
}

impl FrameSpec {
    fn event(event_type: &str, payload: Value) -> Self {
        Self::Event {
            event_type: event_type.to_owned(),
            payload,
            corrupt_message: false,
            corrupt_prelude: false,
        }
    }

    fn exception(exception_type: &str, payload: Value) -> Self {
        Self::Exception {
            exception_type: exception_type.to_owned(),
            payload,
        }
    }

    fn to_ts(&self) -> Value {
        match self {
            Self::Event {
                event_type,
                payload,
                corrupt_message,
                corrupt_prelude,
            } => json!({
                "kind": "event",
                "type": event_type,
                "payload": payload,
                "corruptMessage": corrupt_message,
                "corruptPrelude": corrupt_prelude,
            }),
            Self::Exception {
                exception_type,
                payload,
            } => json!({
                "kind": "exception",
                "type": exception_type,
                "payload": payload,
            }),
        }
    }

    fn to_bytes(&self) -> Vec<u8> {
        match self {
            Self::Event {
                event_type,
                payload,
                corrupt_message,
                corrupt_prelude,
            } => {
                let body = serde_json::to_string(payload).expect("payload");
                let mut frame = encode_frame_bytes(event_type, body.as_bytes(), *corrupt_message);
                if *corrupt_prelude {
                    let crc = pi_ai::api::bedrock_eventstream::crc32(&frame[0..8]) ^ 1;
                    frame[8..12].copy_from_slice(&crc.to_be_bytes());
                }
                frame
            }
            Self::Exception {
                exception_type,
                payload,
            } => {
                let body = serde_json::to_string(payload).expect("payload");
                encode_exception_frame(exception_type, body.as_bytes())
            }
        }
    }
}

fn encode_frame_bytes(event_type: &str, payload: &[u8], corrupt_message: bool) -> Vec<u8> {
    let headers = [
        (":message-type", "event"),
        (":event-type", event_type),
        (":content-type", "application/json"),
    ];
    encode_frame(&headers, payload, corrupt_message)
}

fn encode_exception_frame(exception_type: &str, payload: &[u8]) -> Vec<u8> {
    let headers = [
        (":message-type", "exception"),
        (":exception-type", exception_type),
        (":content-type", "application/json"),
    ];
    encode_frame(&headers, payload, false)
}

fn encode_frame(headers: &[(&str, &str)], payload: &[u8], corrupt_message: bool) -> Vec<u8> {
    let mut header_bytes = Vec::new();
    for (name, value) in headers {
        header_bytes.push(name.len() as u8);
        header_bytes.extend_from_slice(name.as_bytes());
        header_bytes.push(7);
        header_bytes.extend_from_slice(&(value.len() as u16).to_be_bytes());
        header_bytes.extend_from_slice(value.as_bytes());
    }
    let total_length = 12 + header_bytes.len() + payload.len() + 4;
    let mut frame = vec![0u8; total_length];
    frame[0..4].copy_from_slice(&(total_length as u32).to_be_bytes());
    frame[4..8].copy_from_slice(&(header_bytes.len() as u32).to_be_bytes());
    let prelude_crc = pi_ai::api::bedrock_eventstream::crc32(&frame[0..8]);
    frame[8..12].copy_from_slice(&prelude_crc.to_be_bytes());
    frame[12..12 + header_bytes.len()].copy_from_slice(&header_bytes);
    frame[12 + header_bytes.len()..total_length - 4].copy_from_slice(payload);
    let mut message_crc = pi_ai::api::bedrock_eventstream::crc32(&frame[0..total_length - 4]);
    if corrupt_message {
        message_crc ^= 1;
    }
    frame[total_length - 4..].copy_from_slice(&message_crc.to_be_bytes());
    frame
}

#[derive(Clone)]
struct ResponseSpec {
    status: u16,
    headers: Vec<(String, String)>,
    frames: Vec<FrameSpec>,
    text: Option<String>,
    chunk_sizes: Vec<usize>,
}

impl ResponseSpec {
    fn eventstream(frames: Vec<FrameSpec>) -> Self {
        Self {
            status: 200,
            headers: vec![
                (
                    "content-type".to_owned(),
                    "application/vnd.amazon.eventstream".to_owned(),
                ),
                ("x-amzn-requestid".to_owned(), "req-1".to_owned()),
            ],
            frames,
            text: None,
            chunk_sizes: vec![3, 8, 5, 13],
        }
    }

    fn http(status: u16, headers: Vec<(&str, &str)>, body: &str) -> Self {
        Self {
            status,
            headers: headers
                .into_iter()
                .map(|(name, value)| (name.to_owned(), value.to_owned()))
                .collect(),
            frames: Vec::new(),
            text: Some(body.to_owned()),
            chunk_sizes: Vec::new(),
        }
    }

    fn to_ts(&self) -> Value {
        json!({
            "status": self.status,
            "headers": self.headers,
            "frames": self.frames.iter().map(FrameSpec::to_ts).collect::<Vec<_>>(),
            "text": self.text,
            "chunkSizes": self.chunk_sizes,
        })
    }

    fn to_bytes(&self) -> Vec<u8> {
        if let Some(text) = &self.text {
            return text.as_bytes().to_vec();
        }
        self.frames.iter().flat_map(FrameSpec::to_bytes).collect()
    }
}

struct Scenario {
    name: &'static str,
    model: Model,
    context: Context,
    options: StreamOptions,
    process_env: BTreeMap<String, String>,
    script: Vec<ResponseSpec>,
    capture_payload: bool,
    simple: Option<SimpleStreamOptions>,
}

fn default_env() -> ProviderEnv {
    let mut env = ProviderEnv::new();
    env.insert("AWS_ACCESS_KEY_ID".to_owned(), (ACCESS_KEY.to_owned()).into());
    env.insert("AWS_SECRET_ACCESS_KEY".to_owned(), (SECRET_KEY.to_owned()).into());
    env.insert("AWS_REGION".to_owned(), ("us-east-1".to_owned()).into());
    env
}

fn model() -> Model {
    Model {
        properties: Default::default(),
        id: "us.anthropic.claude-opus-4-8".to_owned(),
        name: "Claude Opus 4.8".into(),
        api: "bedrock-converse-stream".to_owned(),
        provider: "amazon-bedrock".to_owned(),
        base_url: ("https://bedrock-runtime.us-east-1.amazonaws.com".to_owned()).into(),
        reasoning: true,
        thinking_level_map: None,
        input: vec![pi_ai::types::Modality::Text, pi_ai::types::Modality::Image],
        cost: pi_ai::types::ModelCost {
            properties: Default::default(),
            rates: pi_ai::types::ModelCostRates {
                input: 3.0,
                output: 15.0,
                cache_read: 0.3,
                cache_write: 3.75,
            },
            tiers: None,
        },
        context_window: 200_000.0,
        auto_compaction_threshold: None,
        max_tokens: 64_000.0,
        sampling_params: None,
        headers: None,
        compat: None,
    }
}

fn context() -> Context {
    Context {
        system_prompt: None,
        messages: vec![Message::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Text(("hi".to_owned()).into()),
            input_source: None,
            provider_native: None,
            timestamp: 0.0,
        })],
        tools: None,
    }
}

fn options() -> StreamOptions {
    StreamOptions {
        request: ProviderRequestOptions {
            env: Some(default_env()),
            ..Default::default()
        },
        cache_retention: Some(CacheRetention::None),
        ..Default::default()
    }
}

fn text_script() -> Vec<ResponseSpec> {
    vec![ResponseSpec::eventstream(vec![
        FrameSpec::event("messageStart", json!({"role": "assistant"})),
        FrameSpec::event(
            "contentBlockDelta",
            json!({"contentBlockIndex": 0, "delta": {"text": "Hello"}}),
        ),
        FrameSpec::event("contentBlockStop", json!({"contentBlockIndex": 0})),
        FrameSpec::event("messageStop", json!({"stopReason": "end_turn"})),
        FrameSpec::event(
            "metadata",
            json!({"usage": {"inputTokens": 10, "outputTokens": 2, "totalTokens": 12}}),
        ),
    ])]
}

fn scenarios() -> Vec<Scenario> {
    let mut scenarios = Vec::new();
    scenarios.push(Scenario {
        name: "fractional-interleaved-scratch",
        model: model(),
        context: context(),
        options: options(),
        process_env: BTreeMap::new(),
        capture_payload: false,
        simple: None,
        script: vec![ResponseSpec::eventstream(vec![
            FrameSpec::event("messageStart", json!({"role":"assistant"})),
            FrameSpec::event(
                "contentBlockDelta",
                json!({"contentBlockIndex":0.25,"delta":{"text":"a"}}),
            ),
            FrameSpec::event(
                "contentBlockDelta",
                json!({"contentBlockIndex":0.5,"delta":{"text":"b"}}),
            ),
            FrameSpec::event(
                "contentBlockDelta",
                json!({"contentBlockIndex":0.25,"delta":{"text":"c"}}),
            ),
            FrameSpec::event(
                "contentBlockStart",
                json!({"contentBlockIndex":0.75,"start":{"toolUse":{"toolUseId":"call_frac","name":"lookup"}}}),
            ),
            FrameSpec::event(
                "contentBlockDelta",
                json!({"contentBlockIndex":0.75,"delta":{"toolUse":{"input":"{\"n\":1"}}}),
            ),
            FrameSpec::event(
                "contentBlockDelta",
                json!({"contentBlockIndex":0.875,"delta":{"reasoningContent":{"text":"why","signature":"sig"}}}),
            ),
            FrameSpec::event("contentBlockStop", json!({"contentBlockIndex":0.75})),
            FrameSpec::event("contentBlockStop", json!({"contentBlockIndex":0.5})),
            FrameSpec::event("contentBlockStop", json!({"contentBlockIndex":0.25})),
            FrameSpec::event("contentBlockStop", json!({"contentBlockIndex":0.875})),
            FrameSpec::event("messageStop", json!({"stopReason":"end_turn"})),
        ])],
    });
    scenarios.push(Scenario {
        name: "text",
        model: model(),
        context: context(),
        options: options(),
        process_env: BTreeMap::new(),
        script: text_script(),
        capture_payload: false,
        simple: None,
    });

    let reasoning_body = vec![
        FrameSpec::event("messageStart", json!({"role": "assistant"})),
        FrameSpec::event(
            "contentBlockDelta",
            json!({"contentBlockIndex": 0, "delta": {"reasoningContent": {"text": "think"}}}),
        ),
        FrameSpec::event(
            "contentBlockDelta",
            json!({"contentBlockIndex": 0, "delta": {"reasoningContent": {"signature": "sig"}}}),
        ),
        FrameSpec::event("contentBlockStop", json!({"contentBlockIndex": 0})),
        FrameSpec::event(
            "contentBlockDelta",
            json!({"contentBlockIndex": 1, "delta": {"text": "Answer"}}),
        ),
        FrameSpec::event("contentBlockStop", json!({"contentBlockIndex": 1})),
        FrameSpec::event("messageStop", json!({"stopReason": "end_turn"})),
        FrameSpec::event(
            "metadata",
            json!({"usage": {"inputTokens": 5, "outputTokens": 7, "cacheReadInputTokens": 2, "cacheWriteInputTokens": 3, "totalTokens": 17}}),
        ),
    ];
    scenarios.push(Scenario {
        name: "reasoning",
        model: model(),
        context: context(),
        options: options(),
        process_env: BTreeMap::new(),
        script: vec![ResponseSpec::eventstream(reasoning_body)],
        capture_payload: false,
        simple: None,
    });

    let tool_body = vec![
        FrameSpec::event("messageStart", json!({"role": "assistant"})),
        FrameSpec::event(
            "contentBlockStart",
            json!({"contentBlockIndex": 0, "start": {"toolUse": {"toolUseId": "tool-1", "name": "edit"}}}),
        ),
        FrameSpec::event(
            "contentBlockDelta",
            json!({"contentBlockIndex": 0, "delta": {"toolUse": {"input": "{\"path\":\"/workspace/file.js\",\"edits\":["}}}),
        ),
        FrameSpec::event(
            "contentBlockDelta",
            json!({"contentBlockIndex": 0, "delta": {"toolUse": {"input": "{\"oldText\":\"first\",\"newText\":\"second\",\"\":\"\"}]}"}}}),
        ),
        FrameSpec::event("contentBlockStop", json!({"contentBlockIndex": 0})),
        FrameSpec::event("messageStop", json!({"stopReason": "tool_use"})),
    ];
    scenarios.push(Scenario {
        name: "tool-use",
        model: model(),
        context: context(),
        options: options(),
        process_env: BTreeMap::new(),
        script: vec![ResponseSpec::eventstream(tool_body)],
        capture_payload: false,
        simple: None,
    });

    let mut cache_context = context();
    cache_context.system_prompt = Some(("You are helpful.".to_owned()).into());
    let mut cache_options = options();
    cache_options.cache_retention = Some(CacheRetention::Long);
    scenarios.push(Scenario {
        name: "cache-points",
        model: model(),
        context: cache_context,
        options: cache_options,
        process_env: BTreeMap::new(),
        script: vec![ResponseSpec::eventstream(vec![
            FrameSpec::event("messageStart", json!({"role": "assistant"})),
            FrameSpec::event("messageStop", json!({"stopReason": "end_turn"})),
        ])],
        capture_payload: false,
        simple: None,
    });

    for (name, reasoning, thinking_display, govcloud) in [
        ("thinking-adaptive-high", "high", Some("summarized"), false),
        ("thinking-adaptive-xhigh", "xhigh", Some("summarized"), false),
        ("thinking-adaptive-govcloud", "high", Some("summarized"), true),
        ("thinking-budget", "high", Some("omitted"), false),
    ] {
        let mut scenario_model = if name == "thinking-budget" {
            let mut model = model();
            model.id = "us.anthropic.claude-sonnet-4-5-20250929-v1:0".to_owned();
            model.name = "Claude Sonnet 4.5".into();
            model
        } else {
            model()
        };
        if govcloud {
            scenario_model.id = "global.anthropic.claude-opus-4-8-v1".to_owned();
        }
        let mut scenario_options = options();
        scenario_options.extra.insert("reasoning".to_owned(), json!(reasoning));
        if let Some(display) = thinking_display {
            scenario_options
                .extra
                .insert("thinkingDisplay".to_owned(), json!(display));
        }
        if name == "thinking-budget" {
            scenario_options
                .extra
                .insert("thinkingBudgets".to_owned(), json!({"high": 4096}));
        }
        if govcloud {
            scenario_options
                .extra
                .insert("region".to_owned(), json!("us-gov-west-1"));
        }
        scenarios.push(Scenario {
            name,
            model: scenario_model,
            context: context(),
            options: scenario_options,
            process_env: BTreeMap::new(),
            script: vec![ResponseSpec::eventstream(vec![
                FrameSpec::event("messageStart", json!({"role": "assistant"})),
                FrameSpec::event("messageStop", json!({"stopReason": "end_turn"})),
            ])],
            capture_payload: true,
            simple: None,
        });
    }

    let mut rich_context = context();
    rich_context.messages = vec![
        Message::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Blocks(vec![
                pi_ai::types::TextOrImageContent::Text(pi_ai::types::TextContent::new("look")),
                pi_ai::types::TextOrImageContent::Image(pi_ai::types::ImageContent::new("aGVsbG8=", "image/png")),
            ]),
            input_source: None,
            provider_native: None,
            timestamp: 0.0,
        }),
        Message::Assistant(Box::new(AssistantMessage {
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
            timestamp: 0.0,
        })),
        Message::ToolResult(ToolResultMessage {
            role: ToolResultRole::Tag,
            tool_call_id: "tool-1".to_owned(),
            tool_name: "lookup".to_owned(),
            content: vec![
                ToolResultContent::Text(pi_ai::types::TextContent::new("result")),
                ToolResultContent::Image(pi_ai::types::ImageContent::new("aGVsbG8=", "image/png")),
                ToolResultContent::Text(pi_ai::types::TextContent::new("")),
            ],
            details: None,
            is_error: false,
            timestamp: 0.0,
        }),
        Message::User(UserMessage {
            role: UserRole::Tag,
            content: UserContent::Text(("   ".to_owned()).into()),
            input_source: None,
            provider_native: None,
            timestamp: 0.0,
        }),
    ];
    rich_context.tools = Some(vec![Tool::new(
        "lookup",
        "Look up",
        json!({"type": "object", "properties": {"q": {"type": "string"}}, "required": ["q"]}),
    )]);
    let mut rich_options = options();
    rich_options.temperature = Some(0.5);
    rich_options.max_tokens = Some(1234.0);
    rich_options.extra.insert("toolChoice".to_owned(), json!("auto"));
    rich_options
        .extra
        .insert("requestMetadata".to_owned(), json!({"team": "core"}));
    let mut rich_headers = pi_ai::types::ProviderHeaders::new();
    rich_headers.insert("x-custom".to_owned(), Some(("v".to_owned()).into()));
    rich_headers.insert("authorization".to_owned(), Some(("evil".to_owned()).into()));
    rich_options.request.headers = Some(rich_headers);
    scenarios.push(Scenario {
        name: "rich-messages",
        model: model(),
        context: rich_context,
        options: rich_options,
        process_env: BTreeMap::new(),
        script: vec![ResponseSpec::eventstream(vec![
            FrameSpec::event("messageStart", json!({"role": "assistant"})),
            FrameSpec::event("messageStop", json!({"stopReason": "end_turn"})),
        ])],
        capture_payload: false,
        simple: None,
    });

    let mut bearer_options = options();
    bearer_options.request.api_key = Some(("bedrock-api-key".to_owned()).into());
    scenarios.push(Scenario {
        name: "source-ignores-api-key",
        model: model(),
        context: context(),
        options: bearer_options,
        process_env: BTreeMap::new(),
        script: text_script(),
        capture_payload: false,
        simple: None,
    });

    let mut explicit_bearer = options();
    explicit_bearer
        .extra
        .insert("bearerToken".to_owned(), json!("bedrock-api-key"));
    scenarios.push(Scenario {
        name: "explicit-bearer",
        model: model(),
        context: context(),
        options: explicit_bearer,
        process_env: BTreeMap::new(),
        script: text_script(),
        capture_payload: false,
        simple: None,
    });

    let mut arn_model = model();
    arn_model.id = "arn:aws:bedrock:us-west-2:123456789012:application-inference-profile/abc123".to_owned();
    scenarios.push(Scenario {
        name: "arn-region",
        model: arn_model,
        context: context(),
        options: options(),
        process_env: BTreeMap::new(),
        script: text_script(),
        capture_payload: false,
        simple: None,
    });

    let mut custom_model = model();
    custom_model.base_url = ("https://bedrock-vpc.example.com".to_owned()).into();
    scenarios.push(Scenario {
        name: "custom-endpoint",
        model: custom_model,
        context: context(),
        options: options(),
        process_env: BTreeMap::new(),
        script: text_script(),
        capture_payload: false,
        simple: None,
    });

    let error_script = vec![
        ResponseSpec::http(
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
        ResponseSpec::http(
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
        ResponseSpec::http(
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
    ];
    scenarios.push(Scenario {
        name: "errors-retry",
        model: model(),
        context: context(),
        options: options(),
        process_env: BTreeMap::new(),
        script: error_script,
        capture_payload: false,
        simple: None,
    });

    scenarios.push(Scenario {
        name: "service-unavailable",
        model: model(),
        context: context(),
        options: options(),
        process_env: BTreeMap::new(),
        script: vec![
            ResponseSpec::http(
                400,
                vec![
                    ("content-type", "application/json"),
                    (
                        "x-amzn-errortype",
                        "ServiceUnavailableException:http://internal.amazon.com/coral/com.amazon.bedrock/",
                    ),
                    ("x-amzn-requestid", "req-service-1"),
                    ("retry-after", "0"),
                ],
                r#"{"message":"try later"}"#,
            ),
            ResponseSpec::http(
                400,
                vec![
                    ("content-type", "application/json"),
                    (
                        "x-amzn-errortype",
                        "ServiceUnavailableException:http://internal.amazon.com/coral/com.amazon.bedrock/",
                    ),
                    ("x-amzn-requestid", "req-service-2"),
                    ("retry-after", "0"),
                ],
                r#"{"message":"try later"}"#,
            ),
            ResponseSpec::http(
                400,
                vec![
                    ("content-type", "application/json"),
                    (
                        "x-amzn-errortype",
                        "ServiceUnavailableException:http://internal.amazon.com/coral/com.amazon.bedrock/",
                    ),
                    ("x-amzn-requestid", "req-service-3"),
                    ("retry-after", "0"),
                ],
                r#"{"message":"try later"}"#,
            ),
        ],
        capture_payload: false,
        simple: None,
    });

    scenarios.push(Scenario {
        name: "gateway-plain-error",
        model: model(),
        context: context(),
        options: options(),
        process_env: BTreeMap::new(),
        capture_payload: false,
        simple: None,
        script: vec![ResponseSpec::http(
            403,
            vec![("content-type", "text/plain")],
            "blocked by gateway WAF",
        )],
    });

    scenarios.push(Scenario {
        name: "mid-stream-exception",
        model: model(),
        context: context(),
        options: options(),
        process_env: BTreeMap::new(),
        script: vec![ResponseSpec::eventstream(vec![
            FrameSpec::event("messageStart", json!({"role": "assistant"})),
            FrameSpec::exception(
                "ThrottlingException",
                json!({"message": "Too many requests, please wait."}),
            ),
        ])],
        capture_payload: false,
        simple: None,
    });

    scenarios.push(Scenario {
        name: "corrupt-message-crc",
        model: model(),
        context: context(),
        options: options(),
        process_env: BTreeMap::new(),
        script: vec![ResponseSpec::eventstream(vec![
            FrameSpec::event("messageStart", json!({"role": "assistant"})),
            FrameSpec::Event {
                event_type: "contentBlockDelta".to_owned(),
                payload: json!({"contentBlockIndex": 0, "delta": {"text": "Hello"}}),
                corrupt_message: true,
                corrupt_prelude: false,
            },
        ])],
        capture_payload: false,
        simple: None,
    });

    scenarios.push(Scenario {
        name: "corrupt-prelude-crc",
        model: model(),
        context: context(),
        options: options(),
        process_env: BTreeMap::new(),
        script: vec![ResponseSpec::eventstream(vec![
            FrameSpec::event("messageStart", json!({"role": "assistant"})),
            FrameSpec::Event {
                event_type: "contentBlockDelta".to_owned(),
                payload: json!({"contentBlockIndex": 0, "delta": {"text": "Hello"}}),
                corrupt_message: false,
                corrupt_prelude: true,
            },
        ])],
        capture_payload: false,
        simple: None,
    });

    let credentials_path = std::env::temp_dir().join(format!("pi-bedrock-diff-{}", std::process::id()));
    std::fs::create_dir_all(&credentials_path).expect("temp dir");
    let credentials_file = credentials_path.join("credentials");
    std::fs::write(
        &credentials_file,
        format!("[dev]\naws_access_key_id = {ACCESS_KEY}\naws_secret_access_key = {SECRET_KEY}\n"),
    )
    .expect("credentials file");
    let mut profile_env = ProviderEnv::new();
    profile_env.insert("AWS_REGION".to_owned(), ("us-east-1".to_owned()).into());
    profile_env.insert(
        "AWS_SHARED_CREDENTIALS_FILE".to_owned(),
        (credentials_file.to_string_lossy().into_owned()).into(),
    );
    let mut profile_options = StreamOptions {
        request: ProviderRequestOptions {
            env: Some(profile_env),
            ..Default::default()
        },
        cache_retention: Some(CacheRetention::None),
        ..Default::default()
    };
    profile_options.extra.insert("profile".to_owned(), json!("dev"));
    let mut profile_process_env = BTreeMap::new();
    profile_process_env.insert(
        "AWS_SHARED_CREDENTIALS_FILE".to_owned(),
        credentials_file.to_string_lossy().into_owned(),
    );
    scenarios.push(Scenario {
        name: "profile-credentials",
        model: model(),
        context: context(),
        options: profile_options,
        process_env: profile_process_env,
        script: text_script(),
        capture_payload: false,
        simple: None,
    });

    let mut skip_env = ProviderEnv::new();
    skip_env.insert("AWS_REGION".to_owned(), ("us-east-1".to_owned()).into());
    skip_env.insert("AWS_BEDROCK_SKIP_AUTH".to_owned(), ("1".to_owned()).into());
    scenarios.push(Scenario {
        name: "skip-auth",
        model: model(),
        context: context(),
        options: StreamOptions {
            request: ProviderRequestOptions {
                env: Some(skip_env),
                ..Default::default()
            },
            cache_retention: Some(CacheRetention::None),
            ..Default::default()
        },
        process_env: BTreeMap::new(),
        script: text_script(),
        capture_payload: false,
        simple: None,
    });

    scenarios
}

fn scenario_to_oracle_request(id: usize, scenario: &Scenario) -> Value {
    let mut options = options_to_ts(&scenario.options);
    if let Some(simple) = &scenario.simple {
        options = simple_to_ts(simple);
    }
    if scenario.capture_payload {
        options["capturePayload"] = json!(true);
    }
    json!({
        "id": id,
        "model": serde_json::to_value(&scenario.model).expect("model"),
        "context": serde_json::to_value(&scenario.context).expect("context"),
        "options": options,
        "processEnv": scenario.process_env,
        "script": scenario.script.iter().map(ResponseSpec::to_ts).collect::<Vec<_>>(),
        "capturePayload": scenario.capture_payload,
    })
}

fn options_to_ts(options: &StreamOptions) -> Value {
    let mut map = Map::new();
    if let Some(temperature) = options.temperature {
        map.insert("temperature".to_owned(), json!(temperature));
    }
    if let Some(max_tokens) = options.max_tokens {
        map.insert("maxTokens".to_owned(), json!(max_tokens));
    }
    if let Some(cache_retention) = options.cache_retention {
        map.insert("cacheRetention".to_owned(), json!(cache_retention));
    }
    if let Some(env) = &options.request.env {
        map.insert("env".to_owned(), serde_json::to_value(env).expect("env"));
    }
    if let Some(api_key) = &options.request.api_key {
        map.insert("apiKey".to_owned(), json!(api_key));
    }
    if let Some(headers) = &options.request.headers {
        map.insert("headers".to_owned(), serde_json::to_value(headers).expect("headers"));
    }
    for (key, value) in &options.extra {
        map.insert(key.clone(), value.clone());
    }
    Value::Object(map)
}

fn simple_to_ts(simple: &SimpleStreamOptions) -> Value {
    let mut value = options_to_ts(&simple.stream);
    let map = value.as_object_mut().expect("options object");
    if let Some(reasoning) = simple.reasoning {
        map.insert("reasoning".to_owned(), json!(reasoning));
    }
    if let Some(budgets) = &simple.thinking_budgets {
        map.insert("thinkingBudgets".to_owned(), json!(budgets));
    }
    value
}

async fn run_rust(
    scenario: &Scenario,
) -> (
    Vec<HttpRequest>,
    Vec<AssistantMessageEvent>,
    AssistantMessage,
    Option<Value>,
) {
    let fetch = ScriptedFetch::new(
        scenario
            .script
            .iter()
            .map(|response| ScriptedResponse {
                status: response.status,
                headers: response.headers.clone(),
                chunks: split_chunks(response.to_bytes(), &response.chunk_sizes),
            })
            .collect(),
    );
    let mut options = scenario.options.clone();
    options.request.fetch = Some(fetch.clone());
    let captured = Arc::new(std::sync::Mutex::new(None));
    if scenario.capture_payload {
        let captured_clone = Arc::clone(&captured);
        options.request.on_payload = Some(Arc::new(move |payload, _model| {
            *captured_clone.lock().expect("capture") = Some(payload);
            Box::pin(async { Ok(None) })
        }));
    }
    let token_controller = pi_js_compat::abort::AbortController::new();
    let token = token_controller.signal();
    options.request.signal = Some(token.clone());
    let (events, message) = if let Some(simple) = &scenario.simple {
        let mut simple = simple.clone();
        simple.stream = options;
        drain_with_abort(
            stream_simple(scenario.model.clone(), scenario.context.clone(), simple),
            token,
        )
        .await
    } else {
        drain_with_abort(stream(scenario.model.clone(), scenario.context.clone(), options), token).await
    };
    (
        fetch.requests(),
        events,
        message,
        captured.lock().expect("capture").clone().map(|value| json!(value)),
    )
}

async fn drain_with_abort(
    mut stream: pi_ai::utils::event_stream::AssistantMessageEventStream,
    _token: AbortSignal,
) -> (Vec<AssistantMessageEvent>, AssistantMessage) {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.expect("stream event"));
    }
    let message = stream.result().await.expect("stream result");
    (events, message)
}

fn split_chunks(body: Vec<u8>, sizes: &[usize]) -> Vec<Vec<u8>> {
    if sizes.is_empty() {
        return vec![body];
    }
    let mut chunks = Vec::new();
    let mut index = 0;
    for size in sizes {
        if index >= body.len() {
            break;
        }
        let end = (index + size).min(body.len());
        chunks.push(body[index..end].to_vec());
        index = end;
    }
    if index < body.len() {
        chunks.push(body[index..].to_vec());
    }
    chunks
}

#[test]
fn host_version_masking_keeps_sdk_build_language_and_feature_identity() {
    let value = "aws-sdk-js/9 ua/2.1 os/linux#7 lang/rust md/nodejs#26 md/tsc#9 api/bedrock-runtime#9 m/E,e";
    assert_eq!(
        normalize_sdk_host_versions(value),
        "aws-sdk-js/9 ua/2.1 os/linux#<host-version> lang/rust md/nodejs#<host-version> md/tsc#9 api/bedrock-runtime#9 m/E,e"
    );
    let headers = json!({"user-agent":value}).as_object().unwrap().clone();
    assert_eq!(normalize_request_headers(&headers, false), headers);
}

fn normalize_sdk_host_versions(value: &str) -> String {
    value
        .split(' ')
        .map(|component| {
            if component.starts_with("os/")
                && let Some((platform, _version)) = component.split_once('#')
            {
                return format!("{platform}#<host-version>");
            }
            if component.starts_with("md/nodejs#") {
                return "md/nodejs#<host-version>".to_owned();
            }
            component.to_owned()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_request_headers(headers: &Map<String, Value>, default_user_agent: bool) -> Map<String, Value> {
    headers
        .iter()
        .map(|(name, value)| {
            let name = name.to_ascii_lowercase();
            let value = match name.as_str() {
                "x-amz-date" => json!("<clock>"),
                "amz-sdk-invocation-id" => json!("<generated>"),
                "user-agent" if default_user_agent => value
                    .as_str()
                    .map(|value| json!(normalize_sdk_host_versions(value)))
                    .unwrap_or_else(|| value.clone()),
                "authorization" => {
                    match value
                        .as_str()
                        .filter(|value| value.starts_with("AWS4-HMAC-SHA256 "))
                        .and_then(|value| value.split_once(", Signature="))
                    {
                        Some((scope, _signature)) => {
                            let mut parts = scope.split('/').collect::<Vec<_>>();
                            if parts.len() > 1
                                && parts[1].len() == 8
                                && parts[1].bytes().all(|byte| byte.is_ascii_digit())
                            {
                                parts[1] = "<clock-date>";
                            }
                            json!(format!("{}, Signature=<signature>", parts.join("/")))
                        }
                        None => value.clone(),
                    }
                }
                _ => value.clone(),
            };
            (name, value)
        })
        .collect()
}

fn headers_map(headers: &[(String, String)]) -> Map<String, Value> {
    let mut map = Map::new();
    for (name, value) in headers {
        map.insert(name.to_ascii_lowercase(), json!(value));
    }
    map
}

fn parse_amz_date(value: &str) -> i64 {
    let year: i64 = value[0..4].parse().expect("year");
    let month: i64 = value[4..6].parse().expect("month");
    let day: i64 = value[6..8].parse().expect("day");
    let hour: i64 = value[9..11].parse().expect("hour");
    let minute: i64 = value[11..13].parse().expect("minute");
    let second: i64 = value[13..15].parse().expect("second");
    let days = pi_js_compat::date::days_from_civil(year, month, day);
    (days * 86_400 + hour * 3600 + minute * 60 + second) * 1000
}

fn verify_signature(scenario: &Scenario, oracle_request: &Value) {
    if let Some(api_key) = scenario.options.extra.get("bearerToken").and_then(Value::as_str) {
        assert_eq!(
            oracle_request["headers"]["Authorization"].as_str(),
            Some(pi_js_compat::js_json::encode_node_utf8(&pi_js_compat::js_format!("Bearer {}", api_key)).as_str()),
            "bearer authorization mismatch for {}",
            scenario.name
        );
        return;
    }
    let skip_auth = scenario
        .options
        .request
        .env
        .as_ref()
        .and_then(|env| env.get("AWS_BEDROCK_SKIP_AUTH"))
        .is_some_and(|value| value == "1")
        || scenario.process_env.contains_key("AWS_BEDROCK_SKIP_AUTH");
    let headers = oracle_request["headers"].as_object().expect("headers");
    let authorization = headers
        .get("authorization")
        .and_then(Value::as_str)
        .expect("oracle authorization");
    let amz_date = headers
        .get("x-amz-date")
        .and_then(Value::as_str)
        .expect("oracle x-amz-date");
    let region = authorization.split('/').nth(2).expect("credential scope region");
    let mut rust_headers: Vec<(String, String)> = headers
        .iter()
        .filter(|(name, _)| !name.eq_ignore_ascii_case("authorization"))
        .map(|(name, value)| (name.clone(), value.as_str().expect("header string").to_owned()))
        .collect();
    let body = oracle_request["body"].as_str().expect("body").as_bytes();
    let hostname = oracle_request["hostname"].as_str().expect("hostname");
    let url = format!(
        "{}//{hostname}{}",
        oracle_request["protocol"].as_str().expect("protocol"),
        oracle_request["path"].as_str().expect("path")
    );
    let credentials = AwsCredentials {
        access_key_id: if skip_auth {
            ("dummy-access-key".to_owned()).into()
        } else {
            (ACCESS_KEY.to_owned()).into()
        },
        secret_access_key: if skip_auth {
            ("dummy-secret-key".to_owned()).into()
        } else {
            (SECRET_KEY.to_owned()).into()
        },
        session_token: (headers
            .get("x-amz-security-token")
            .and_then(Value::as_str)
            .map(str::to_owned))
        .map(pi_js_compat::js_json::JsString::from),
    };
    sign_request(
        "POST",
        &url,
        &mut rust_headers,
        body,
        &credentials,
        (&pi_js_compat::js_json::JsString::from(region), "bedrock"),
        parse_amz_date(amz_date),
    )
    .expect("sign");
    let rust_authorization = rust_headers
        .iter()
        .find(|(name, _)| name == "authorization")
        .map(|(_, value)| value.as_str())
        .expect("rust authorization");
    assert_eq!(
        rust_authorization, authorization,
        "SigV4 mismatch for scenario {}",
        scenario.name
    );
}

struct AbortLoopbackFetch(String);
impl pi_ai::request::Fetch for AbortLoopbackFetch {
    fn fetch(
        &self,
        mut request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> pi_ai::request::BoxFuture<'_, Result<pi_ai::request::HttpResponse, pi_ai::request::CallbackError>> {
        request.url = self.0.clone();
        Box::pin(async move { pi_ai::http::default_fetch().fetch(request, signal).await })
    }
}

fn decode_bedrock_reason(input: &pi_js_compat::js_json::JsValue) -> Option<pi_js_compat::abort::AbortReason> {
    use pi_js_compat::{abort::AbortReason, js_json::JsValue, node_error::NativeNodeError};
    if input["kind"] == JsValue::String("default".into()) {
        return None;
    }
    if input["kind"] == JsValue::String("value".into()) {
        return Some(AbortReason::Value(input["value"].clone()));
    }
    let mut error = NativeNodeError::new(
        input["name"].as_string().cloned().unwrap_or_else(|| "Error".into()),
        input["message"].as_string().unwrap().clone(),
    );
    if input["kind"] == JsValue::String("dom".into()) {
        error = error.with_property("code", JsValue::Number(23.0));
        Some(AbortReason::DomException(error))
    } else {
        Some(AbortReason::Error(error))
    }
}

async fn run_bedrock_abort_boundary(
    scenario: &Scenario,
    phase: &str,
    reason: Option<pi_js_compat::abort::AbortReason>,
) -> AssistantMessage {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let controller = pi_js_compat::abort::AbortController::new();
    let server_controller = controller.clone();
    let server_reason = reason.clone();
    let is_headers = phase == "headers";
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut bytes = Vec::new();
        while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            let mut chunk = [0; 4096];
            let read = socket.read(&mut chunk).await.unwrap();
            if read == 0 {
                return;
            }
            bytes.extend_from_slice(&chunk[..read]);
        }
        if is_headers {
            server_controller.abort(server_reason);
        } else {
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/vnd.amazon.eventstream\r\nTransfer-Encoding: chunked\r\n\r\n").await.unwrap();
            let frame = FrameSpec::event("messageStart", json!({"role":"assistant"})).to_bytes();
            socket
                .write_all(format!("{:x}\r\n", frame.len()).as_bytes())
                .await
                .unwrap();
            socket.write_all(&frame).await.unwrap();
            socket.write_all(b"\r\n").await.unwrap();
        }
        std::future::pending::<()>().await;
    });
    let mut options = scenario.options.clone();
    options.request.fetch = Some(Arc::new(AbortLoopbackFetch(url)));
    options.request.signal = Some(controller.signal());
    if phase == "immediate" {
        controller.abort(reason.clone());
    }
    if phase == "body" {
        options.request.on_response = Some(Arc::new(move |_, _| {
            controller.abort(reason.clone());
            Box::pin(async { Ok(()) })
        }));
    }
    let result = tokio::time::timeout(std::time::Duration::from_secs(4), async {
        let mut stream = stream(scenario.model.clone(), scenario.context.clone(), options);
        while stream.next().await.is_some() {}
        stream.result().await.unwrap()
    })
    .await;
    server.abort();
    if let Err(error) = server.await {
        assert!(error.is_cancelled(), "server task failed: {error}");
    }
    result.expect("bounded Bedrock abort boundary")
}

#[tokio::test]
async fn bedrock_custom_abort_reasons_match_real_smithy_header_and_body_boundaries() {
    use pi_js_compat::js_json::{JsString, JsValue};
    let mut oracle = oracle::TsOracle::spawn("bedrock-converse-stream-oracle.ts", "packages/ai/src");
    let scenario = scenarios().remove(0);
    let reasons = pi_js_compat::js_json::parse_lossless(
        r#"[
        {"kind":"default"},{"kind":"value","value":null},{"kind":"value","value":false},
        {"kind":"value","value":0},{"kind":"value","value":""},
        {"kind":"value","value":"native\ud800"},
        {"kind":"value","value":{"name":"Error","message":"not Error"}},
        {"kind":"value","value":["native\ud800",null]},
        {"kind":"error","name":"CallerError","message":"native\ud800"},
        {"kind":"dom","name":"TimeoutError","message":"deadline"}
    ]"#,
    )
    .unwrap();
    let mut failures = Vec::new();
    for phase in ["immediate", "headers", "body"] {
        for reason in reasons.as_array().unwrap() {
            let mut request = JsValue::from(scenario_to_oracle_request(1, &scenario));
            request
                .as_object_mut()
                .unwrap()
                .insert("abort".into(), JsValue::String(phase.into()));
            request
                .as_object_mut()
                .unwrap()
                .insert("abortReason".into(), reason.clone());
            if phase == "body" {
                request
                    .as_object_mut()
                    .unwrap()
                    .get_mut(&JsString::from("script"))
                    .unwrap()
                    .as_array_mut()
                    .unwrap()[0]
                    .as_object_mut()
                    .unwrap()
                    .insert("pendingBody".into(), JsValue::Bool(true));
            }
            eprintln!(
                "source phase={phase} reason={}",
                pi_js_compat::js_json::stringify_lossless(reason)
            );
            let source = oracle.request(&request);
            assert_eq!(source["ok"], JsValue::Bool(true), "{source:?}");
            eprintln!("native phase={phase}");
            let message = run_bedrock_abort_boundary(&scenario, phase, decode_bedrock_reason(reason)).await;
            let mut actual = pi_js_compat::js_json::to_value(&message).unwrap();
            let mut expected = source["output"]["message"].clone();
            for value in [&mut actual, &mut expected] {
                value
                    .as_object_mut()
                    .unwrap()
                    .insert("timestamp".into(), JsValue::Number(0.0));
            }
            if actual != expected {
                failures.push(format!(
                    "phase={phase} reason={} actual={} source={}",
                    pi_js_compat::js_json::stringify_lossless(reason),
                    pi_js_compat::js_json::stringify_lossless(&actual),
                    pi_js_compat::js_json::stringify_lossless(&expected)
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[tokio::test]
async fn bedrock_converse_stream_matches_the_pinned_typescript_module() {
    let mut oracle = oracle::TsOracle::spawn("bedrock-converse-stream-oracle.ts", "packages/ai/src");
    for (index, scenario) in scenarios().into_iter().enumerate() {
        let id = index + 1;
        let response = oracle.request(&scenario_to_oracle_request(id, &scenario));
        assert_eq!(
            response["ok"],
            json!(true),
            "oracle failed for {}: {response}",
            scenario.name
        );
        let output = &response["output"];

        let (requests, events, message, captured) = run_rust(&scenario).await;
        let oracle_requests = output["requests"].as_array().expect("requests");
        assert_eq!(
            requests.len(),
            oracle_requests.len(),
            "request count mismatch for {}; oracle={response}",
            scenario.name
        );
        for (rust_request, oracle_request) in requests.iter().zip(oracle_requests) {
            assert_eq!(
                oracle_request["method"].as_str(),
                Some(rust_request.method.as_str()),
                "method mismatch for {}",
                scenario.name
            );
            assert_eq!(
                oracle_request["body"].as_str(),
                rust_request
                    .body
                    .as_deref()
                    .map(|body| std::str::from_utf8(body).expect("body utf8")),
                "body mismatch for {}",
                scenario.name
            );
            let oracle_headers = oracle_request["headers"].as_object().expect("headers");
            let default_user_agent = !scenario
                .options
                .request
                .headers
                .as_ref()
                .is_some_and(|headers| headers.keys().any(|name| name.eq_ignore_ascii_case("user-agent")));
            assert_eq!(
                normalize_request_headers(&headers_map(&rust_request.headers), default_user_agent),
                normalize_request_headers(oracle_headers, default_user_agent),
                "headers mismatch for {}",
                scenario.name
            );
            verify_signature(&scenario, oracle_request);
        }

        let mut rust_events = serde_json::to_value(&events).expect("events");
        for event in rust_events.as_array_mut().unwrap() {
            provider_normalize::normalize_event_clocks(event);
        }
        let mut oracle_events = output["events"].clone();
        for event in oracle_events.as_array_mut().unwrap() {
            provider_normalize::normalize_event_clocks(event);
        }
        assert_eq!(rust_events, oracle_events, "event mismatch for {}", scenario.name);

        let mut rust_message = serde_json::to_value(&message).expect("message");
        provider_normalize::normalize_message_clock(&mut rust_message);
        let mut oracle_message = output["message"].clone();
        provider_normalize::normalize_message_clock(&mut oracle_message);
        assert_eq!(rust_message, oracle_message, "message mismatch for {}", scenario.name);

        if scenario.capture_payload {
            let rust_payload = captured.expect("captured payload");
            let oracle_payload = output["capturedPayload"].clone();
            assert_eq!(rust_payload, oracle_payload, "payload mismatch for {}", scenario.name);
        }
    }
}

struct ScriptedFetch {
    requests: std::sync::Mutex<Vec<HttpRequest>>,
    responses: std::sync::Mutex<std::collections::VecDeque<ScriptedResponse>>,
}

struct ScriptedResponse {
    status: u16,
    headers: Vec<(String, String)>,
    chunks: Vec<Vec<u8>>,
}

impl ScriptedFetch {
    fn new(responses: Vec<ScriptedResponse>) -> Arc<Self> {
        Arc::new(Self {
            requests: std::sync::Mutex::new(Vec::new()),
            responses: std::sync::Mutex::new(responses.into()),
        })
    }

    fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().expect("requests").clone()
    }
}

impl pi_ai::request::Fetch for ScriptedFetch {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> pi_ai::request::BoxFuture<'_, Result<pi_ai::request::HttpResponse, pi_ai::request::CallbackError>> {
        Box::pin(async move {
            self.requests.lock().expect("requests").push(request);
            if signal.as_ref().is_some_and(|signal| signal.aborted()) {
                return Err(pi_ai::request::CallbackError::new(pi_ai::http::ABORT_ERROR_MESSAGE));
            }
            let response = self
                .responses
                .lock()
                .expect("responses")
                .pop_front()
                .expect("scripted response");
            Ok(pi_ai::request::HttpResponse {
                status: response.status,
                status_text: String::new(),
                headers: response.headers,
                body: Box::new(ScriptedBody {
                    chunks: response.chunks.into(),
                    signal,
                }),
            })
        })
    }
}

struct ScriptedBody {
    chunks: std::collections::VecDeque<Vec<u8>>,
    signal: Option<AbortSignal>,
}

impl pi_ai::request::HttpBody for ScriptedBody {
    fn next_chunk(&mut self) -> pi_ai::request::BoxFuture<'_, Option<Result<Vec<u8>, pi_ai::request::CallbackError>>> {
        Box::pin(async move {
            if self.signal.as_ref().is_some_and(|signal| signal.aborted()) {
                return Some(Err(pi_ai::request::CallbackError::new(
                    pi_ai::http::ABORT_ERROR_MESSAGE,
                )));
            }
            self.chunks.pop_front().map(Ok)
        })
    }
}
