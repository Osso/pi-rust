pub use crate::shared_provider_normalize as provider_normalize;

use crate::shared_json_compare as json_compare;
use crate::shared_oracle as oracle;
use crate::shared_recording_fetch as recording_fetch;

use std::collections::BTreeSet;
use std::sync::Arc;

use oracle::TsOracle;
use pi_ai::api::openai_completions::{stream, stream_simple};
use pi_ai::request::{Fetch, HttpRequest, ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{CacheRetention, Context, Model, ProviderHeaders};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use pi_js_compat::abort::AbortSignal;
use recording_fetch::{RecordingFetch, ScriptedResponse};
use serde_json::{Value, json};

const ENV_HEADERS: [&str; 4] = [
    "x-stainless-os",
    "x-stainless-arch",
    "x-stainless-runtime",
    "x-stainless-runtime-version",
];

fn model(provider: &str, id: &str, base_url: &str, reasoning: bool) -> Value {
    json!({
        "id": id, "name": id, "api": "openai-completions", "provider": provider,
        "baseUrl": base_url, "reasoning": reasoning,
        "input": ["text", "image"],
        "cost": {"input": 1.0, "output": 2.0, "cacheRead": 0.5, "cacheWrite": 0.75},
        "contextWindow": 128000, "maxTokens": 4096
    })
}

fn assistant(content: Value, provider: &str, model_id: &str) -> Value {
    let mut message = pi_ai::types::AssistantMessage::empty("openai-completions", provider, model_id, 1234.0);
    message.stop_reason = pi_ai::types::StopReason::ToolUse;
    let mut value = serde_json::to_value(message).unwrap();
    value["content"] = content;
    value
}

fn with_compat(mut model: Value, compat: Value) -> Value {
    model["compat"] = compat;
    model
}

fn text_events(text: &str) -> Vec<Value> {
    vec![
        json!({"id": "chatcmpl-1", "model": "gpt-5",
            "choices": [{"index": 0, "delta": {"content": text}, "finish_reason": null}]}),
        json!({"id": "chatcmpl-1", "model": "gpt-5",
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]}),
        json!({"id": "chatcmpl-1", "model": "gpt-5", "choices": [],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15,
                "prompt_tokens_details": {"cached_tokens": 2, "cache_write_tokens": 1},
                "completion_tokens_details": {"reasoning_tokens": 3}}}),
    ]
}

fn sse_chunks(events: &[Value], chunk_chars: usize) -> Vec<String> {
    let mut payload = String::new();
    for event in events {
        payload.push_str("data: ");
        payload.push_str(&event.to_string());
        payload.push_str("\n\n");
    }
    payload.push_str("data: [DONE]\n\n");
    payload
        .chars()
        .collect::<Vec<_>>()
        .chunks(chunk_chars.max(1))
        .map(|chunk| chunk.iter().collect())
        .collect()
}

fn provider_headers(value: Option<&Value>) -> Option<ProviderHeaders> {
    value.and_then(Value::as_object).map(|headers| {
        headers
            .iter()
            .map(|(name, value)| {
                (
                    name.clone(),
                    match value {
                        Value::Null => None,
                        Value::String(value) => Some(pi_js_compat::js_json::JsString::from(value)),
                        other => Some(pi_js_compat::js_json::JsString::from(other.to_string())),
                    },
                )
            })
            .collect()
    })
}

fn stream_options(stream: &Value, extra: &Value, fetch: Arc<dyn Fetch>, signal: Option<AbortSignal>) -> StreamOptions {
    let mut request = ProviderRequestOptions {
        api_key: Some(
            (stream
                .get("apiKey")
                .and_then(Value::as_str)
                .unwrap_or("test-key")
                .to_owned())
            .into(),
        ),
        fetch: Some(fetch),
        signal,
        headers: provider_headers(stream.get("headers")),
        timeout_ms: stream.get("timeoutMs").and_then(Value::as_f64),
        max_retries: stream.get("maxRetries").and_then(Value::as_f64),
        max_retry_delay_ms: stream.get("maxRetryDelayMs").and_then(Value::as_f64),
        ..Default::default()
    };
    if let Some(env) = stream.get("env").and_then(Value::as_object) {
        request.env = Some(
            env.iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        pi_js_compat::js_json::JsString::from(value.as_str().unwrap_or("")),
                    )
                })
                .collect::<indexmap::IndexMap<_, _>>(),
        );
    }
    let mut extra_map = extra.as_object().cloned().unwrap_or_default();
    let known = [
        "apiKey",
        "headers",
        "timeoutMs",
        "maxRetries",
        "maxRetryDelayMs",
        "temperature",
        "maxTokens",
        "cacheRetention",
        "sessionId",
        "metadata",
        "env",
        "transport",
        "websocketConnectTimeoutMs",
    ];
    for (key, value) in stream.as_object().into_iter().flatten() {
        if !known.contains(&key.as_str()) {
            extra_map.insert(key.clone(), value.clone());
        }
    }
    StreamOptions {
        request,
        temperature: stream.get("temperature").and_then(Value::as_f64),
        max_tokens: stream.get("maxTokens").and_then(Value::as_f64),
        cache_retention: stream
            .get("cacheRetention")
            .and_then(Value::as_str)
            .map(|retention| match retention {
                "none" => CacheRetention::None,
                "long" => CacheRetention::Long,
                _ => CacheRetention::Short,
            }),
        session_id: (stream.get("sessionId").and_then(Value::as_str).map(str::to_owned))
            .map(pi_js_compat::js_json::JsString::from),
        metadata: stream
            .get("metadata")
            .and_then(|value| pi_js_compat::js_json::JsValue::from(value.clone()).as_object().cloned()),
        extra: extra_map,
        ..Default::default()
    }
}

fn request_value(request: &HttpRequest, configured_headers: &BTreeSet<String>) -> Value {
    let headers = request
        .headers
        .iter()
        .map(|(name, value)| {
            let value = if ENV_HEADERS.contains(&name.as_str()) && !configured_headers.contains(name) {
                "<env>"
            } else {
                value
            };
            (name.clone(), Value::String(value.to_owned()))
        })
        .collect::<serde_json::Map<_, _>>();
    let body = request
        .body
        .as_deref()
        .and_then(|body| serde_json::from_slice::<Value>(body).ok())
        .unwrap_or(Value::Null);
    json!({"url": request.url, "method": request.method, "headers": headers, "body": body})
}

fn assert_json_eq(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(actual), Value::Number(expected)) => {
            assert_eq!(actual.as_f64(), expected.as_f64(), "number mismatch at {path}");
        }
        (Value::Array(actual), Value::Array(expected)) => {
            assert_eq!(actual.len(), expected.len(), "array length mismatch at {path}");
            for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
                assert_json_eq(actual, expected, &format!("{path}[{index}]"));
            }
        }
        (Value::Object(actual), Value::Object(expected)) => {
            assert_eq!(
                actual.keys().collect::<BTreeSet<_>>(),
                expected.keys().collect::<BTreeSet<_>>(),
                "object keys mismatch at {path}"
            );
            for (key, expected) in expected {
                assert_json_eq(&actual[key], expected, &format!("{path}.{key}"));
            }
        }
        _ => assert_eq!(actual, expected, "value mismatch at {path}"),
    }
}

fn normalize_timestamps(value: &mut Value) {
    provider_normalize::normalize_capture_clocks(value);
}

async fn collect_stream(mut stream: AssistantMessageEventStream) -> (Vec<Value>, Value) {
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(serde_json::to_value(event.expect("event")).expect("event json"));
    }
    let result = match stream.result().await {
        Ok(message) => serde_json::to_value(message).expect("result json"),
        Err(failure) => json!({"error": failure.message()}),
    };
    (events, result)
}

struct Scenario {
    id: i64,
    model: Value,
    context: Value,
    stream: Value,
    extra: Value,
    responses: Vec<ScriptedResponse>,
    reasoning: Option<String>,
    simple: bool,
    abort_after_chunks: Option<usize>,
    ordered_body_keys: Vec<String>,
}

impl Scenario {
    fn new(id: i64, model: Value, events: &[Value]) -> Self {
        Self {
            id,
            model,
            context: json!({"messages": [{"role": "user", "content": "hi", "timestamp": 1}]}),
            stream: json!({}),
            extra: json!({}),
            responses: vec![ScriptedResponse::sse(
                &sse_chunks(events, 7).iter().map(String::as_str).collect::<Vec<_>>(),
            )],
            reasoning: None,
            simple: false,
            abort_after_chunks: None,
            ordered_body_keys: Vec::new(),
        }
    }

    fn context(mut self, context: Value) -> Self {
        self.context = context;
        self
    }

    fn stream(mut self, stream: Value) -> Self {
        self.stream = stream;
        self
    }

    fn extra(mut self, extra: Value) -> Self {
        self.extra = extra;
        self
    }

    fn responses(mut self, responses: Vec<ScriptedResponse>) -> Self {
        self.responses = responses;
        self
    }

    fn reasoning(mut self, reasoning: &str) -> Self {
        self.reasoning = Some(reasoning.to_owned());
        self.simple = true;
        self
    }

    fn abort_after(mut self, chunks: usize) -> Self {
        self.abort_after_chunks = Some(chunks);
        self
    }

    fn ordered_body_keys(mut self, keys: &[&str]) -> Self {
        self.ordered_body_keys = keys.iter().map(|key| (*key).to_owned()).collect();
        self
    }
}

async fn run_scenario(oracle: &mut TsOracle, scenario: Scenario) {
    let mut request = json!({
        "id": scenario.id,
        "operation": if scenario.simple { "streamSimple" } else { "stream" },
        "model": scenario.model,
        "context": scenario.context,
        "stream": scenario.stream,
        "extra": scenario.extra,
        "responses": scenario.responses.iter().map(|response| json!({
            "status": response.status,
            "headers": response.headers.iter().cloned().collect::<indexmap::IndexMap<_, _>>(),
            "chunks": response.chunks.iter().map(|chunk| String::from_utf8_lossy(chunk).into_owned()).collect::<Vec<_>>()
        })).collect::<Vec<_>>(),
    });
    if let Some(reasoning) = &scenario.reasoning {
        request["reasoning"] = Value::String(reasoning.clone());
    }
    if let Some(chunks) = scenario.abort_after_chunks {
        request["abortAfterChunks"] = json!(chunks);
    }
    let expected = oracle.request(&request);
    assert_eq!(expected["ok"], Value::Bool(true), "oracle failed: {expected}");
    let expected = &expected["output"];

    let model: Model = serde_json::from_value(scenario.model.clone()).expect("model");
    let context: Context = serde_json::from_value(scenario.context.clone()).expect("context");
    let controller = scenario
        .abort_after_chunks
        .map(|_| pi_js_compat::abort::AbortController::new());
    let signal = controller.as_ref().map(|controller| controller.signal());
    let fetch: Arc<RecordingFetch> = match (scenario.abort_after_chunks, &controller) {
        (Some(chunks), Some(controller)) => {
            RecordingFetch::with_abort_after_chunks(scenario.responses.clone(), chunks, controller.clone())
        }
        _ => RecordingFetch::new(scenario.responses.clone()),
    };
    let options = stream_options(&scenario.stream, &scenario.extra, fetch.clone(), signal.clone());

    let stream = if scenario.simple {
        let reasoning = scenario
            .reasoning
            .as_deref()
            .and_then(|reasoning| serde_json::from_value(Value::String(reasoning.to_owned())).ok());
        stream_simple(
            model,
            context,
            SimpleStreamOptions {
                stream: options,
                reasoning,
                ..Default::default()
            },
        )
        .expect("simple acquisition")
    } else {
        stream(model, context, options)
    };
    let (events, result) = collect_stream(stream).await;
    let configured_headers = provider_normalize::configured_headers(&scenario.model, &scenario.stream);
    let requests = fetch
        .requests()
        .iter()
        .map(|request| request_value(request, &configured_headers))
        .collect::<Vec<_>>();
    let mut actual = json!({"requests": requests, "events": events, "result": result});
    normalize_timestamps(&mut actual);
    assert_json_eq(
        &actual,
        expected,
        &format!("openai-completions scenario {}", scenario.id),
    );
    for (index, (actual, expected)) in actual["requests"]
        .as_array()
        .unwrap()
        .iter()
        .zip(expected["requests"].as_array().unwrap())
        .enumerate()
    {
        json_compare::assert_json_eq_ordered(
            &actual["body"],
            &expected["body"],
            &format!("scenario {} request {index} body", scenario.id),
        );
    }
    for key in &scenario.ordered_body_keys {
        let actual_field = actual["requests"][0]["body"].get(key);
        let expected_field = expected["requests"][0]["body"].get(key);
        assert!(
            actual_field.is_some(),
            "scenario {} body.{key} must be present",
            scenario.id
        );
        assert_eq!(
            actual_field.map(Value::to_string),
            expected_field.map(Value::to_string),
            "openai-completions scenario {} body.{key} order",
            scenario.id
        );
    }
}

fn compat(value: Value) -> Value {
    value
}

#[tokio::test]
async fn differential_openai_completions() {
    let mut oracle = TsOracle::spawn(
        "openai-responses-oracle.ts",
        "packages/ai/src/api/openai-completions.ts",
    );
    let text = text_events("Hello");
    let openai = model("openai", "gpt-5", "https://api.openai.com/v1", true);

    run_scenario(
        &mut oracle,
        Scenario::new(0, openai.clone(), &text).stream(json!({"sessionId": "session-123", "maxTokens": 100})),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(1, openai.clone(), &text).extra(json!({"reasoningEffort": "high"})),
    )
    .await;

    run_scenario(&mut oracle, Scenario::new(2, openai.clone(), &text).reasoning("high")).await;

    let openrouter = model("openrouter", "openai/gpt-5", "https://openrouter.ai/api/v1", true);
    run_scenario(
        &mut oracle,
        Scenario::new(3, openrouter.clone(), &text).extra(json!({"reasoningEffort": "high"})),
    )
    .await;
    run_scenario(&mut oracle, Scenario::new(4, openrouter, &text)).await;

    let deepseek = model("deepseek", "deepseek-chat", "https://api.deepseek.com", true);
    run_scenario(
        &mut oracle,
        Scenario::new(5, deepseek.clone(), &text).extra(json!({"reasoningEffort": "low"})),
    )
    .await;
    run_scenario(
        &mut oracle,
        Scenario::new(6, deepseek, &text).stream(json!({"maxTokens":123,"sessionId":"session"})),
    )
    .await;

    let together = model("together", "together-1", "https://api.together.xyz/v1", true);
    run_scenario(
        &mut oracle,
        Scenario::new(7, together, &text).extra(json!({"reasoningEffort": "medium"})),
    )
    .await;

    let baseten = with_compat(
        model("baseten", "baseten-1", "https://inference.baseten.co/v1", true),
        compat(json!({"thinkingFormat": "baseten", "supportsReasoningEffort": true,
            "chatTemplateArgs": {"thinking": {"$var": "thinking.effort", "omitWhenOff": true}}})),
    );
    run_scenario(
        &mut oracle,
        Scenario::new(8, baseten, &text).extra(json!({"reasoningEffort": "high"})),
    )
    .await;

    let chat_template = with_compat(
        model("chat-template", "chat-template-1", "https://example.invalid/v1", true),
        compat(
            json!({"thinkingFormat": "chat-template", "supportsReasoningEffort": true,
            "chatTemplateKwargs": {"zeta": "Z", "alpha": true, "middle": 3}}),
        ),
    );
    run_scenario(
        &mut oracle,
        Scenario::new(40, chat_template, &text)
            .extra(json!({"reasoningEffort": "high"}))
            .ordered_body_keys(&["chat_template_kwargs"]),
    )
    .await;

    let zai = with_compat(
        model("zai", "glm-4.6", "https://api.z.ai/api/paas/v4", true),
        compat(json!({"thinkingFormat": "zai", "supportsReasoningEffort": true,
            "zaiToolStream": true, "supportsStrictMode": true})),
    );
    let zai_context = json!({
        "messages": [{"role": "user", "content": "hi", "timestamp": 1}],
        "tools": [{"name": "tool", "description": "Tool",
            "parameters": {"type": "object", "properties": {"a": {"type": "string"}}}}]
    });
    run_scenario(
        &mut oracle,
        Scenario::new(9, zai, &text)
            .context(zai_context)
            .extra(json!({"reasoningEffort": "high"})),
    )
    .await;

    let qwen = with_compat(
        model("qwen-token-plan", "qwen3", "https://qwen.example.com/v1", true),
        compat(json!({"thinkingFormat": "qwen", "supportsReasoningEffort": true})),
    );
    run_scenario(
        &mut oracle,
        Scenario::new(10, qwen, &text).extra(json!({"reasoningEffort": "low"})),
    )
    .await;

    let chat_template = with_compat(
        model("custom", "template-1", "https://example.com/v1", true),
        compat(json!({"thinkingFormat": "chat-template", "chatTemplateKwargs": {
            "enable_thinking": {"$var": "thinking.enabled"},
            "effort": {"$var": "thinking.effort", "omitWhenOff": true},
            "static": "value"
        }})),
    );
    run_scenario(
        &mut oracle,
        Scenario::new(11, chat_template.clone(), &text).extra(json!({"reasoningEffort": "medium"})),
    )
    .await;
    run_scenario(&mut oracle, Scenario::new(12, chat_template, &text)).await;

    let qwen_template = with_compat(
        model("qwen-token-plan", "qwen3", "https://qwen.example.com/v1", true),
        compat(json!({"thinkingFormat": "qwen-chat-template"})),
    );
    run_scenario(&mut oracle, Scenario::new(13, qwen_template, &text)).await;

    let string_thinking = with_compat(
        model("custom", "string-1", "https://example.com/v1", true),
        compat(json!({"thinkingFormat": "string-thinking"})),
    );
    run_scenario(
        &mut oracle,
        Scenario::new(14, string_thinking, &text).extra(json!({"reasoningEffort": "high"})),
    )
    .await;

    let ant_ling = with_compat(
        model("ant-ling", "ling-1", "https://api.ant-ling.com/v1", true),
        compat(json!({"thinkingFormat": "ant-ling"})),
    );
    run_scenario(
        &mut oracle,
        Scenario::new(15, ant_ling, &text).extra(json!({"reasoningEffort": "high"})),
    )
    .await;

    let tool_events = vec![
        json!({"id": "chatcmpl-2", "model": "gpt-5", "choices": [{"index": 0, "finish_reason": null,
            "delta": {"tool_calls": [{"index": 0, "id": "call_1", "type": "function",
                "function": {"name": "edit", "arguments": ""}}]}}]}),
        json!({"id": "chatcmpl-2", "model": "gpt-5", "choices": [{"index": 0, "finish_reason": null,
            "delta": {"tool_calls": [{"index": 0, "function": {"arguments": "{\"path\""}}]}}]}),
        json!({"id": "chatcmpl-2", "model": "gpt-5", "choices": [{"index": 0, "finish_reason": null,
            "delta": {"tool_calls": [{"index": 0, "function": {"arguments": ":\"README.md\"}"}}]}}]}),
        json!({"id": "chatcmpl-2", "model": "gpt-5", "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]}),
    ];
    run_scenario(&mut oracle, Scenario::new(16, openai.clone(), &tool_events)).await;

    let grammar_model = with_compat(
        openai.clone(),
        compat(json!({"supportsOpenAIGrammarTools": true, "supportsStrictMode": true})),
    );
    let grammar_context = json!({
        "messages": [{"role": "user", "content": "hi", "timestamp": 1}],
        "tools": [
            {"name": "ordinary", "description": "Ordinary",
             "parameters": {"type": "object", "properties": {"a": {"type": "string"}}}},
            {"name": "query", "description": "Query",
             "parameters": {"type": "object", "properties": {"input": {"type": "string"}}, "required": ["input"]},
             "constrainedSampling": {"type": "grammar", "variants": {"openai_lark": "start: /[a-z]+/"}}}
        ]
    });
    run_scenario(
        &mut oracle,
        Scenario::new(17, grammar_model, &text).context(grammar_context),
    )
    .await;

    run_scenario(
        &mut oracle,
        Scenario::new(18, openai.clone(), &text)
            .stream(json!({"sessionId": "session-123", "cacheRetention": "long", "samplingParams": {"top_p": 0.9}})),
    )
    .await;

    let affinity_model = with_compat(
        openai.clone(),
        compat(json!({"sendSessionAffinityHeaders": true, "sessionAffinityFormat": "openai"})),
    );
    run_scenario(
        &mut oracle,
        Scenario::new(19, affinity_model, &text).stream(json!({"sessionId": "session-affinity"})),
    )
    .await;

    let openrouter_affinity = with_compat(
        openai.clone(),
        compat(json!({"sendSessionAffinityHeaders": true, "sessionAffinityFormat": "openrouter"})),
    );
    run_scenario(
        &mut oracle,
        Scenario::new(20, openrouter_affinity, &text).stream(json!({"sessionId": "session-or"})),
    )
    .await;

    let routing_model = with_compat(
        openai.clone(),
        compat(json!({"openRouterRouting": {"only": ["openai"], "order": ["openai"]},
            "vercelGatewayRouting": {"only": ["anthropic"], "order": ["anthropic"]}})),
    );
    run_scenario(&mut oracle, Scenario::new(21, routing_model, &text)).await;

    run_scenario(
        &mut oracle,
        Scenario::new(22, openai.clone(), &text).responses(vec![ScriptedResponse::json(
            400,
            r#"{"error":{"message":"bad request","type":"invalid_request_error"}}"#,
        )]),
    )
    .await;

    let retry_chunks = sse_chunks(&text, 11);
    run_scenario(
        &mut oracle,
        Scenario::new(23, openai.clone(), &text)
            .stream(json!({"maxRetries": 1}))
            .responses(vec![
                ScriptedResponse::json(429, r#"{"error":{"message":"rate limited"}}"#)
                    .with_header("retry-after", "0.001"),
                ScriptedResponse::sse(&retry_chunks.iter().map(String::as_str).collect::<Vec<_>>()),
            ]),
    )
    .await;

    let abort_chunks = sse_chunks(&text, 5);
    run_scenario(
        &mut oracle,
        Scenario::new(24, openai.clone(), &text).abort_after(abort_chunks.len().min(3)),
    )
    .await;

    let mut malformed = sse_chunks(&text, 9);
    malformed.insert(1, "data: {not json}\n\n".to_owned());
    run_scenario(
        &mut oracle,
        Scenario::new(25, openai.clone(), &text).responses(vec![ScriptedResponse::sse(
            &malformed.iter().map(String::as_str).collect::<Vec<_>>(),
        )]),
    )
    .await;

    let image_context = json!({
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "look"},
                {"type": "image", "data": "AAAA", "mimeType": "image/png"}], "timestamp": 1},
            {"role": "assistant",
             "content": [{"type": "toolCall", "id": "call_img", "name": "shot", "arguments": {}}],
             "api": "openai-completions", "provider": "openai", "model": "gpt-5",
             "usage": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0,
                "totalTokens": 0.0,
                "cost": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0, "total": 0.0}},
             "stopReason": "toolUse", "timestamp": 2},
            {"role": "toolResult", "toolCallId": "call_img", "toolName": "shot",
             "content": [{"type": "text", "text": "text"},
                {"type": "image", "data": "BBBB", "mimeType": "image/png"}],
             "isError": false, "timestamp": 3}
        ]
    });
    run_scenario(
        &mut oracle,
        Scenario::new(26, openai.clone(), &text).context(image_context),
    )
    .await;

    let kimi_model = with_compat(openai.clone(), compat(json!({"deferredToolsMode": "kimi"})));
    let kimi_context = json!({
        "messages": [
            {"role": "user", "content": "run", "timestamp": 1},
            {"role": "assistant",
             "content": [{"type": "toolCall", "id": "call_1", "name": "first", "arguments": {}}],
             "api": "openai-completions", "provider": "openai", "model": "gpt-5",
             "usage": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0,
                "totalTokens": 0.0,
                "cost": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0, "total": 0.0}},
             "stopReason": "toolUse", "timestamp": 2},
            {"role": "toolResult", "toolCallId": "call_1", "toolName": "first",
             "content": [{"type": "text", "text": "ok"}], "isError": false, "timestamp": 3,
             "addedToolNames": ["later"]}
        ],
        "tools": [
            {"name": "first", "description": "First",
             "parameters": {"type": "object", "properties": {}}},
            {"name": "later", "description": "Later",
             "parameters": {"type": "object", "properties": {"value": {"type": "string"}}}}
        ]
    });
    run_scenario(&mut oracle, Scenario::new(27, kimi_model, &text).context(kimi_context)).await;

    let encrypted_events = vec![
        json!({"id": "chatcmpl-3", "model": "gpt-5", "choices": [{"index": 0, "finish_reason": null,
            "delta": {"tool_calls": [{"index": 0, "id": "call_enc", "type": "function",
                "function": {"name": "edit", "arguments": "{}"}}],
                "reasoning_details": [{"type": "reasoning.encrypted", "id": "call_enc", "data": "opaque"}]}}]}),
        json!({"id": "chatcmpl-3", "model": "gpt-5", "choices": [{"index": 0, "delta": {}, "finish_reason": "tool_calls"}]}),
    ];
    run_scenario(&mut oracle, Scenario::new(28, openai.clone(), &encrypted_events)).await;

    let cache_model = with_compat(
        model(
            "openrouter",
            "anthropic/claude-sonnet",
            "https://openrouter.ai/api/v1",
            false,
        ),
        compat(json!({"cacheControlFormat": "anthropic", "supportsLongCacheRetention": true})),
    );
    let cache_context = json!({
        "systemPrompt": "system",
        "messages": [
            {"role": "user", "content": "hi", "timestamp": 1},
            {"role": "assistant", "content": [{"type": "text", "text": "hello"}],
             "api": "openai-completions", "provider": "openrouter", "model": "anthropic/claude-sonnet",
             "usage": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0,
                "totalTokens": 0.0,
                "cost": {"input": 0.0, "output": 0.0, "cacheRead": 0.0, "cacheWrite": 0.0, "total": 0.0}},
             "stopReason": "stop", "timestamp": 2}
        ],
        "tools": [{"name": "tool", "description": "Tool",
            "parameters": {"type": "object", "properties": {"a": {"type": "string"}}}}]
    });
    run_scenario(
        &mut oracle,
        Scenario::new(29, cache_model, &text)
            .context(cache_context)
            .stream(json!({"cacheRetention": "long"})),
    )
    .await;

    let no_developer = with_compat(openai, compat(json!({"supportsDeveloperRole": false})));
    run_scenario(&mut oracle, Scenario::new(30, no_developer, &text)).await;

    let budget_model = with_compat(
        model("custom", "budget-1", "https://example.com/v1", true),
        compat(json!({"supportsThinkingTokenBudget": true, "thinkingFormat": "openai",
            "supportsReasoningEffort": true})),
    );
    run_scenario(
        &mut oracle,
        Scenario::new(31, budget_model.clone(), &text)
            .stream(json!({"maxTokens": 2000}))
            .extra(json!({"reasoningEffort": "high", "thinkingBudgets": {"high": 500}})),
    )
    .await;
    run_scenario(
        &mut oracle,
        Scenario::new(32, budget_model, &text)
            .stream(json!({"maxTokens": 2000}))
            .extra(json!({"reasoningEffort": "high", "thinkingBudgets": {"high": 5000}})),
    )
    .await;

    let no_usage = with_compat(
        model("openai", "gpt-5", "https://api.openai.com/v1", true),
        compat(json!({"supportsUsageInStreaming": false, "supportsStore": false})),
    );
    run_scenario(&mut oracle, Scenario::new(33, no_usage, &text)).await;

    run_scenario(
        &mut oracle,
        Scenario::new(34, model("openai", "gpt-5", "https://api.openai.com/v1", true), &text).responses(
            vec![ScriptedResponse::json(
                400,
                r#"{"error":{"message":"This model's maximum context length is 128000 tokens.","code":"context_length_exceeded"}}"#,
            )],
        ),
    )
    .await;
}

#[tokio::test]
async fn tool_scratch_preserves_numeric_indices_and_failure_boundaries() {
    let mut oracle = TsOracle::spawn(
        "openai-responses-oracle.ts",
        "packages/ai/src/api/openai-completions.ts",
    );
    let opening = json!({"id":"chat_open","choices":[{"index":0,"delta":{"tool_calls":[
        {"index":0.5,"id":"call_a","type":"function","function":{"name":"a","arguments":"{\"n\":"}},
        {"index":0.75,"id":"call_b","type":"function","function":{"name":"b","arguments":"{\"n\":"}}
    ]}}]});
    let next = json!({"id":"chat_open","choices":[{"index":0,"delta":{"tool_calls":[
        {"index":0.75,"function":{"arguments":"2}"}},
        {"index":0.5,"function":{"arguments":"1}"}}
    ]}}]});
    let finish = json!({"id":"chat_open","choices":[{"index":0,"delta":{},"finish_reason":"tool_calls"}]});
    let cases = [
        vec![opening.clone(), next, finish.clone()],
        vec![opening.clone(), json!({"error":{"message":"open tool failed"}})],
        vec![opening],
        vec![
            json!({"id":"chat_open","choices":[{"index":0,"delta":{"tool_calls":[{
                "index":0,"id":"call_custom","type":"custom","custom":{"name":"old","input":"retired"}
            }]}}]}),
            finish,
        ],
    ];
    for (index, events) in cases.into_iter().enumerate() {
        run_scenario(
            &mut oracle,
            Scenario::new(
                100 + index as i64,
                model("openai", "gpt-5", "https://api.openai.com/v1", true),
                &events,
            ),
        )
        .await;
    }
}

#[tokio::test]
async fn completions_replay_ids_empty_prompt_and_empty_tool_result_match_frozen() {
    let mut source = TsOracle::spawn(
        "openai-responses-oracle.ts",
        "packages/ai/src/api/openai-completions.ts",
    );
    let base = model("openai", "test", "https://api.openai.com/v1", false);
    let text = text_events("replay");
    for (index, id) in [
        "call_123|fc_456".to_owned(),
        "call🙈end|fc_tail".to_owned(),
        format!("call{}", "🙈".repeat(20)),
    ]
    .into_iter()
    .enumerate()
    {
        let mut call = assistant(
            json!([{"type":"toolCall","id":id,"name":"read","arguments":{}}]),
            "openai",
            "test",
        );
        call["api"] = json!("openai-completions");
        let result = json!({"role":"toolResult","toolCallId":id,"toolName":"read","content":[{"type":"text","text":"done"}],"isError":false,"timestamp":1234});
        run_scenario(
            &mut source,
            Scenario::new(200 + index as i64, base.clone(), &text).context(json!({"messages":[call,result]})),
        )
        .await;
    }
}

#[tokio::test]
async fn completions_empty_prompt_and_tool_result_match_literal_source_placeholder() {
    let mut source = TsOracle::spawn(
        "openai-responses-oracle.ts",
        "packages/ai/src/api/openai-completions.ts",
    );
    let base = model("openai", "test", "https://api.openai.com/v1", false);
    let text = text_events("empty");
    let call = assistant(
        json!([{"type":"toolCall","id":"call_empty","name":"read","arguments":{}}]),
        "openai",
        "test",
    );
    let result = json!({"role":"toolResult","toolCallId":"call_empty","toolName":"read","content":[],"isError":false,"timestamp":1234});
    run_scenario(
        &mut source,
        Scenario::new(210, base, &text).context(json!({"systemPrompt":"","messages":[call,result]})),
    )
    .await;
}

#[tokio::test]
async fn completions_simple_tool_choice_and_sdk_retry_ignore_old_budget_and_delay_options() {
    let mut source = TsOracle::spawn(
        "openai-responses-oracle.ts",
        "packages/ai/src/api/openai-completions.ts",
    );
    let base = model("openai", "test", "https://api.openai.com/v1", true);
    let text = text_events("options");
    run_scenario(
        &mut source,
        Scenario::new(220, base.clone(), &text)
            .extra(json!({"toolChoice":"required"}))
            .reasoning("medium"),
    )
    .await;
    let chunks = sse_chunks(&text, 100);
    run_scenario(
        &mut source,
        Scenario::new(221, base, &text)
            .stream(json!({"maxRetries":1,"maxRetryDelayMs":1}))
            .responses(vec![
                ScriptedResponse::json(429, r#"{"error":{"message":"retry"}}"#).with_header("retry-after-ms", "2"),
                ScriptedResponse::sse(&chunks.iter().map(String::as_str).collect::<Vec<_>>()),
            ]),
    )
    .await;
}
