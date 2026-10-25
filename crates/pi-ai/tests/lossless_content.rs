use crate::shared_oracle as oracle;

use pi_ai::types::ToolResultMessage;
use pi_js_compat::js_json::{JsString, parse_lossless};

#[test]
fn system_prompt_round_trip_keeps_extension_injected_utf16() {
    let source = r#"{"systemPrompt":"policy\ud800","messages":[]}"#;
    let context: pi_ai::types::Context = serde_json::from_str(source).unwrap();
    let output = serde_json::to_string(&context).unwrap();
    assert_eq!(parse_lossless(&output).unwrap(), parse_lossless(source).unwrap());
}

#[test]
fn tool_message_round_trip_keeps_text_details_and_null_presence() {
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-content-oracle.ts",
        "packages/ai/src/utils/sanitize-unicode.ts",
    );
    for details in ["", r#", "details":null"#, r#", "details":{"\ud800":"\udfff"}"#] {
        let source = format!(
            r#"{{"role":"toolResult","toolCallId":"call","toolName":"probe","content":[{{"type":"text","text":"x\ud800\udfff\ud800"}}]{details},"isError":false,"timestamp":1}}"#
        );
        let message: ToolResultMessage = serde_json::from_str(&source).unwrap();
        let output = serde_json::to_string(&message).unwrap();
        assert!(message.content.iter().any(|part| match part {
            pi_ai::types::TextOrImageContent::Text(text) => text.text_utf8().is_err(),
            _ => false,
        }));
        let reference = oracle.request(&serde_json::json!({"wire": source}));
        assert_eq!(reference["units"], serde_json::json!([120, 0xd800, 0xdfff, 0xd800]));
        let reverse = oracle.request(&serde_json::json!({"wire": output}));
        assert_eq!(reverse["units"], reference["units"]);
        assert_eq!(reverse["wire"], output);
        assert_eq!(
            pi_ai::utils::sanitize_unicode::sanitize_surrogates(JsString::join(
                message.content.iter().filter_map(|part| match part {
                    pi_ai::types::TextOrImageContent::Text(text) => Some(&text.text),
                    _ => None,
                }),
                "\n"
            )),
            reference["sanitized"].as_str().unwrap()
        );
        assert_eq!(parse_lossless(&output).unwrap(), parse_lossless(&source).unwrap());
    }
}

#[test]
fn provider_native_context_round_trip_keeps_utf16() {
    let source = r#"{"provider":"openai","api":"openai-responses","format":"openai.responses.input","value":[{"text":"\ud800","\udfff":null}]}"#;
    let native: pi_ai::types::ProviderNativeContext = serde_json::from_str(source).unwrap();
    assert_eq!(
        parse_lossless(serde_json::to_string(&native).unwrap()).unwrap(),
        parse_lossless(source).unwrap()
    );
}

#[test]
fn tool_arguments_round_trip_arbitrary_utf16_keys_and_values() {
    let source =
        r#"{"type":"toolCall","id":"call","name":"probe","arguments":{"\ud800":["\udfff",{"name":"\ud800"}]}}"#;
    let call: pi_ai::types::ToolCall = serde_json::from_str(source).unwrap();
    let output = serde_json::to_string(&call).unwrap();
    assert_eq!(parse_lossless(&output).unwrap(), parse_lossless(source).unwrap());
}

#[derive(Default)]
struct CaptureRequest(std::sync::Mutex<Vec<pi_ai::request::HttpRequest>>);
impl pi_ai::request::Fetch for CaptureRequest {
    fn fetch(
        &self,
        request: pi_ai::request::HttpRequest,
        _signal: Option<pi_js_compat::abort::AbortSignal>,
    ) -> pi_ai::request::BoxFuture<'_, Result<pi_ai::request::HttpResponse, pi_ai::request::CallbackError>> {
        self.0.lock().unwrap().push(request);
        Box::pin(async { Err(pi_ai::request::CallbackError::new("captured request")) })
    }
}

#[test]
fn user_and_thinking_content_round_trip_lone_surrogate_units() {
    for source in [
        r#"{"role":"user","content":"\ud800","timestamp":1}"#,
        r#"{"role":"assistant","content":[{"type":"thinking","thinking":"\udfff"}],"api":"probe","provider":"probe","model":"probe","usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"stop","timestamp":1}"#,
    ] {
        let message: pi_ai::types::Message = serde_json::from_str(source).unwrap();
        assert_eq!(
            parse_lossless(serde_json::to_string(&message).unwrap()).unwrap(),
            parse_lossless(source).unwrap()
        );
    }
}

#[tokio::test]
async fn openai_reasoning_signature_keeps_unsanitized_utf16_in_http_json() {
    use pi_ai::types::{AssistantContent, AssistantMessage, Context, Message, Model, StopReason, ThinkingContent};
    use pi_js_compat::js_json::JsString;
    let model: Model = serde_json::from_value(serde_json::json!({
        "id":"probe", "name":"probe", "api":"openai-completions", "provider":"openai", "baseUrl":"https://provider.invalid/v1", "reasoning":true, "input":["text"],
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}, "contextWindow":10000, "maxTokens":1000
    })).unwrap();
    let fetch = std::sync::Arc::new(CaptureRequest::default());
    let mut options = pi_ai::request::StreamOptions::default();
    options.request.fetch = Some(fetch.clone());
    options.request.api_key = Some("test".into());
    options.request.max_retries = Some(0.0);
    let mut thinking = ThinkingContent::new(JsString::from_utf16(vec![0xd800]));
    thinking.thinking_signature = Some("reasoning_content".into());
    let mut assistant = AssistantMessage::empty("openai-completions", "openai", "probe", 1.0);
    assistant.content.push(AssistantContent::Thinking(thinking));
    assistant
        .content
        .push(AssistantContent::Text(pi_ai::types::TextContent::new("visible")));
    assistant.stop_reason = StopReason::Stop;
    let mut stream = pi_ai::api::openai_completions::stream(
        model,
        Context {
            system_prompt: None,
            messages: vec![Message::Assistant(Box::new(assistant))],
            tools: None,
        },
        options,
    );
    while stream.next().await.is_some() {}
    let requests = fetch.0.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let body = std::str::from_utf8(requests[0].body.as_ref().unwrap()).unwrap();
    assert!(body.contains(r#""reasoning_content":"\ud800""#), "{body}");
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-content-oracle.ts",
        "packages/ai/src/utils/sanitize-unicode.ts",
    );
    let reference = oracle.request(&serde_json::json!({"wire":body}));
    assert_eq!(reference["wire"], body);
}

fn has_arguments(value: &pi_js_compat::js_json::JsValue, expected: &pi_js_compat::js_json::JsValue) -> bool {
    use pi_js_compat::js_json::JsValue;
    if value == expected {
        return true;
    }
    match value {
        JsValue::Array(items) => items.iter().any(|item| has_arguments(item, expected)),
        JsValue::Object(object) => object.values().any(|item| has_arguments(item, expected)),
        JsValue::String(text) => text
            .to_utf8()
            .ok()
            .and_then(|text| parse_lossless(&text).ok())
            .is_some_and(|inner| &inner == expected),
        _ => false,
    }
}

#[tokio::test]
async fn five_provider_argument_bodies_keep_utf16_and_match_node_json() {
    use pi_ai::types::{AssistantContent, AssistantMessage, Context, Message, Model, StopReason};
    let expected = parse_lossless(r#"{"\ud800":"\udfff"}"#).unwrap();
    for (api, provider) in [
        ("anthropic-messages", "anthropic"),
        ("google-generative-ai", "google"),
        ("openai-completions", "openai"),
        ("openai-responses", "openai"),
        ("mistral-conversations", "mistral"),
    ] {
        let model: Model = serde_json::from_value(serde_json::json!({
            "id":"probe", "name":"probe", "api":api, "provider":provider, "baseUrl":"https://provider.invalid/v1", "reasoning":false, "input":["text"],
            "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}, "contextWindow":10000, "maxTokens":1000
        })).unwrap();
        let call: pi_ai::types::ToolCall =
            serde_json::from_str(r#"{"type":"toolCall","id":"call","name":"probe","arguments":{"\ud800":"\udfff"}}"#)
                .unwrap();
        let mut assistant = AssistantMessage::empty(api, provider, "probe", 1.0);
        assistant.content.push(AssistantContent::ToolCall(call));
        assistant.stop_reason = StopReason::ToolUse;
        let fetch = std::sync::Arc::new(CaptureRequest::default());
        let mut options = pi_ai::request::StreamOptions::default();
        options.request.fetch = Some(fetch.clone());
        options.request.api_key = Some("test".into());
        options.request.max_retries = Some(0.0);
        let context = Context {
            system_prompt: None,
            messages: vec![Message::Assistant(Box::new(assistant))],
            tools: None,
        };
        let mut stream = match api {
            "anthropic-messages" => pi_ai::api::anthropic_messages::stream(model, context, options),
            "google-generative-ai" => pi_ai::api::google_generative_ai::stream(model, context, options),
            "openai-completions" => pi_ai::api::openai_completions::stream(model, context, options),
            "openai-responses" => pi_ai::api::openai_responses::stream(model, context, options),
            "mistral-conversations" => pi_ai::api::mistral_conversations::stream(
                model,
                context,
                pi_ai::MistralOptions {
                    stream: options,
                    ..Default::default()
                },
            ),
            _ => unreachable!(),
        };
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while stream.next().await.is_some() {}
        })
        .await
        .unwrap_or_else(|_| panic!("{api} did not terminate after captured request"));
        let requests = fetch.0.lock().unwrap();
        assert_eq!(requests.len(), 1, "{api}");
        let body = std::str::from_utf8(requests[0].body.as_ref().unwrap()).unwrap();
        assert!(
            has_arguments(&parse_lossless(body).unwrap(), &expected),
            "{api}: {body}"
        );
        let mut oracle = oracle::TsOracle::spawn(
            "lossless-content-oracle.ts",
            "packages/ai/src/utils/sanitize-unicode.ts",
        );
        let reference = oracle.request(&serde_json::json!({"wire":body}));
        assert_eq!(reference["wire"], body, "{api}");
    }
}

#[test]
fn complete_and_partial_argument_json_retains_utf16_like_frozen_parser() {
    use pi_js_compat::js_json::{JsString, stringify_lossless};
    let mut oracle = oracle::TsOracle::spawn("lossless-arguments-oracle.ts", "packages/ai/src/utils/json-parse.ts");
    let mut inputs: Vec<JsString> = [
        r#"{"name":"\ud800"}"#,
        r#"{"\ud800":["\udfff",{"text":"x"}]}"#,
        r#"{"name":"\ud800"#,
        r#"{"name":"x\udfff","next":[1,"last"#,
    ]
    .into_iter()
    .map(JsString::from)
    .collect();
    inputs.push(JsString::join(
        [
            JsString::from("{\"name\":\""),
            JsString::from_utf16(vec![0xd800]),
            JsString::from("\"}"),
        ],
        "",
    ));
    inputs.push(JsString::join(
        [JsString::from("{\"name\":\""), JsString::from_utf16(vec![0xdfff])],
        "",
    ));
    for input in inputs {
        let result = pi_ai::utils::json_parse::parse_streaming_json(Some(&input));
        let expected = oracle.request(&serde_json::json!({"inputUnits":input.units()}));
        assert_eq!(
            stringify_lossless(&result),
            expected["wire"].as_str().unwrap(),
            "{:?}",
            input.units()
        );
    }
}

struct SseBody(Option<Vec<u8>>);
impl pi_ai::request::HttpBody for SseBody {
    fn next_chunk(&mut self) -> pi_ai::request::BoxFuture<'_, Option<Result<Vec<u8>, pi_ai::request::CallbackError>>> {
        let chunk = self.0.take();
        Box::pin(async move { chunk.map(Ok) })
    }
}
#[tokio::test]
async fn anthropic_sse_retains_text_thinking_and_native_utf16_argument_fragments() {
    use pi_ai::types::{AssistantContent, Context, Model, StopReason};
    let model: Model = serde_json::from_value(serde_json::json!({
        "id":"probe", "name":"probe", "api":"anthropic-messages", "provider":"anthropic", "baseUrl":"https://provider.invalid/v1", "reasoning":true, "input":["text"],
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}, "contextWindow":10000, "maxTokens":1000
    })).unwrap();
    let events = concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg\",\"model\":\"probe\",\"usage\":{\"input_tokens\":1,\"output_tokens\":0}}}\n\n",
        r#"event: content_block_start
 data: {"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}

 event: content_block_delta
 data: {"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"\ud800"}}

 event: content_block_stop
 data: {"type":"content_block_stop","index":0}

 event: content_block_start
 data: {"type":"content_block_start","index":1,"content_block":{"type":"thinking","thinking":""}}

 event: content_block_delta
 data: {"type":"content_block_delta","index":1,"delta":{"type":"thinking_delta","thinking":"\udfff"}}

 event: content_block_delta
 data: {"type":"content_block_delta","index":1,"delta":{"type":"signature_delta","signature":"sig"}}

 event: content_block_stop
 data: {"type":"content_block_stop","index":1}

 event: content_block_start
 data: {"type":"content_block_start","index":2,"content_block":{"type":"tool_use","id":"call","name":"probe","input":{}}}

 event: content_block_delta
 data: {"type":"content_block_delta","index":2,"delta":{"type":"input_json_delta","partial_json":"{\"name\":\"\ud800\"}"}}

 event: content_block_stop
 data: {"type":"content_block_stop","index":2}

 event: message_delta
 data: {"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":1}}

 event: message_stop
 data: {"type":"message_stop"}

"#
    );
    let events = events.replace("\n ", "\n");
    let mut options = pi_ai::request::StreamOptions::default();
    options.request.api_key = Some("test".into());
    options.request.fetch = Some(std::sync::Arc::new(OwnedSse(events)));
    options.request.max_retries = Some(0.0);
    let mut stream = pi_ai::api::anthropic_messages::stream(
        model,
        Context {
            system_prompt: None,
            messages: vec![],
            tools: None,
        },
        options,
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while stream.next().await.is_some() {}
    })
    .await
    .unwrap();
    let result = stream.result().await.unwrap();
    assert_eq!(result.stop_reason, StopReason::ToolUse, "{:?}", result.error_message);
    assert!(matches!(&result.content[0], AssistantContent::Text(text) if text.text.units() == [0xd800]));
    assert!(matches!(&result.content[1], AssistantContent::Thinking(text) if text.thinking.units() == [0xdfff]));
    let AssistantContent::ToolCall(call) = &result.content[2] else {
        panic!("tool call missing")
    };
    assert_eq!(
        call.arguments.get("name").unwrap().as_string().unwrap().units(),
        &[0xd800]
    );
}

struct OwnedSse(String);
impl pi_ai::request::Fetch for OwnedSse {
    fn fetch(
        &self,
        _: pi_ai::request::HttpRequest,
        _: Option<pi_js_compat::abort::AbortSignal>,
    ) -> pi_ai::request::BoxFuture<'_, Result<pi_ai::request::HttpResponse, pi_ai::request::CallbackError>> {
        let bytes = self.0.as_bytes().to_vec();
        Box::pin(async {
            Ok(pi_ai::request::HttpResponse {
                status: 200,
                status_text: "OK".into(),
                headers: vec![],
                body: Box::new(SseBody(Some(bytes))),
            })
        })
    }
}

#[tokio::test]
async fn native_context_items_reach_http_json_without_scalar_projection() {
    use pi_ai::types::{
        Context, Message, Model, ProviderNativeContext, ProviderNativeFormat, UserContent, UserMessage, UserRole,
    };
    let model: Model = serde_json::from_value(serde_json::json!({"id":"probe","name":"probe","api":"openai-responses","provider":"openai","baseUrl":"https://provider.invalid/v1","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":10000,"maxTokens":1000})).unwrap();
    let native = ProviderNativeContext {
        provider: "openai".into(),
        api: "openai-responses".into(),
        format: ProviderNativeFormat::OpenAIResponsesInput,
        value: parse_lossless(
            r#"[{"type":"message","role":"user","content":[{"type":"input_text","text":"\ud800","\udfff":true}]}]"#,
        )
        .unwrap(),
    };
    let user = UserMessage {
        role: UserRole::Tag,
        content: UserContent::Text("fallback".into()),
        input_source: None,
        provider_native: Some(native),
        timestamp: 1.0,
    };
    let fetch = std::sync::Arc::new(CaptureRequest::default());
    let mut options = pi_ai::request::StreamOptions::default();
    options.request.fetch = Some(fetch.clone());
    options.request.api_key = Some("test".into());
    options.request.max_retries = Some(0.0);
    let mut stream = pi_ai::api::openai_responses::stream(
        model,
        Context {
            system_prompt: None,
            messages: vec![Message::User(user)],
            tools: None,
        },
        options,
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while stream.next().await.is_some() {}
    })
    .await
    .unwrap();
    let requests = fetch.0.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let body = std::str::from_utf8(requests[0].body.as_ref().unwrap()).unwrap();
    assert!(body.contains(r#""text":"\ud800""#), "{body}");
    assert!(body.contains(r#""\udfff":true"#), "{body}");
    assert!(!body.contains("fallback"));
    let mut oracle = oracle::TsOracle::spawn(
        "lossless-content-oracle.ts",
        "packages/ai/src/utils/sanitize-unicode.ts",
    );
    assert_eq!(oracle.request(&serde_json::json!({"wire":body}))["wire"], body);
}

fn prompt_cache_probe(api: &str, provider: &str) -> pi_ai::types::Model {
    serde_json::from_value(serde_json::json!({
        "id":"probe", "name":"probe", "api":api, "provider":provider, "baseUrl":"https://provider.invalid/v1", "reasoning":false, "input":["text"],
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0}, "contextWindow":10000, "maxTokens":1000
    }))
    .unwrap()
}

fn unmatched_prompt_cache_key() -> pi_js_compat::js_json::JsString {
    use pi_js_compat::js_json::JsString;
    JsString::from_utf16(vec![0xd800])
        + JsString::from("a".repeat(62))
        + JsString::from_utf16(vec![0xd83d, 0xde00, 0xdc00])
}

fn prompt_cache_options(fetch: std::sync::Arc<CaptureRequest>) -> pi_ai::request::StreamOptions {
    let mut options = pi_ai::request::StreamOptions::default();
    options.request.fetch = Some(fetch);
    options.request.api_key = Some("test".into());
    options.request.max_retries = Some(0.0);
    options.session_id = Some(unmatched_prompt_cache_key());
    options
}

fn empty_context() -> pi_ai::types::Context {
    pi_ai::types::Context {
        system_prompt: None,
        messages: vec![],
        tools: None,
    }
}

// Frozen azure-openai-responses.ts:262 sends the session ID only as prompt_cache_key.
#[tokio::test]
async fn azure_prompt_cache_key_keeps_raw_codepoints_in_http_json() {
    use pi_js_compat::js_json::JsValue;
    let fetch = std::sync::Arc::new(CaptureRequest::default());
    let mut stream = pi_ai::api::azure_openai_responses::stream(
        prompt_cache_probe("azure-openai-responses", "azure-openai-responses"),
        empty_context(),
        prompt_cache_options(fetch.clone()),
    );
    while stream.next().await.is_some() {}
    let requests = fetch.0.lock().unwrap();
    assert_eq!(requests.len(), 1);
    let body = std::str::from_utf8(requests[0].body.as_ref().unwrap()).unwrap();
    let parsed = parse_lossless(body).unwrap();
    assert_eq!(
        parsed["prompt_cache_key"],
        JsValue::String(unmatched_prompt_cache_key().slice(0, 65))
    );
    assert!(body.contains("\\ud800"));
}

// Frozen openai-responses.ts:200-204 also sends the session ID as a header, whose
// ByteString conversion rejects the unmatched unit before fetch.
#[tokio::test]
async fn openai_unmatched_session_id_is_rejected_at_header_boundary() {
    let fetch = std::sync::Arc::new(CaptureRequest::default());
    let mut stream = pi_ai::api::openai_responses::stream(
        prompt_cache_probe("openai-responses", "openai"),
        empty_context(),
        prompt_cache_options(fetch.clone()),
    );
    while stream.next().await.is_some() {}
    let output = stream.result().await.unwrap();
    assert_eq!(output.stop_reason, pi_ai::types::StopReason::Error);
    assert_eq!(
        output.error_message.unwrap(),
        "Cannot convert argument to a ByteString because the character at index 0 has a value of 55296 which is greater than 255."
    );
    assert!(fetch.0.lock().unwrap().is_empty());
}
