use crate::shared_oracle as oracle;

use std::sync::{Arc, Mutex};

use pi_ai::request::{
    BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse, ProviderRequestOptions, SimpleStreamOptions,
    StreamOptions,
};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::model_registry::{ModelRegistry, ResolvedRequestAuth};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::{js_json, js_json::JsValue};
use serde_json::Value;

struct RequestCapture {
    requests: Mutex<Vec<JsValue>>,
    response: String,
    status: u16,
}

struct FixtureBody(Option<Vec<u8>>);

impl HttpBody for FixtureBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        let chunk = self.0.take();
        Box::pin(async move { chunk.map(Ok) })
    }
}

impl Fetch for RequestCapture {
    fn fetch(
        &self,
        request: HttpRequest,
        _signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        let body = request.body.as_ref().expect("completion request has JSON body");
        let body = js_json::parse_lossless(std::str::from_utf8(body).unwrap()).unwrap();
        let headers = ["session_id", "x-client-request-id", "x-session-affinity"]
            .into_iter()
            .map(|name| {
                let value = request
                    .headers
                    .iter()
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
                    .map(|(_, value)| value);
                (name, js_json::to_value(&value).unwrap())
            })
            .collect::<Vec<_>>();
        self.requests
            .lock()
            .unwrap()
            .push(js_json!({"body":body,"headers":JsValue::object(headers)}));
        let body = self.response.as_bytes().to_vec();
        let status = self.status;
        Box::pin(async move {
            Ok(HttpResponse {
                status,
                status_text: String::new(),
                headers: vec![("content-type".into(), "text/event-stream".into())],
                body: Box::new(FixtureBody(Some(body))),
            })
        })
    }
}

fn response_fixture(api: &str, finish_reason: bool) -> String {
    let events = match api {
        "openai-responses" | "azure-openai-responses" => vec![
            serde_json::json!({"type":"response.created","response":{"id":"fixture","model":"fixture","status":"in_progress","output":[]}}),
            serde_json::json!({"type":"response.completed","response":{"id":"fixture","status":"completed","output":[],"usage":{"input_tokens":1,"output_tokens":0,"total_tokens":1}}}),
        ],
        "anthropic-messages" => vec![
            serde_json::json!({"type":"message_start","message":{"id":"fixture","type":"message","role":"assistant","model":"fixture","content":[],"stop_reason":null,"usage":{"input_tokens":1,"output_tokens":0}}}),
            serde_json::json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":0}}),
            serde_json::json!({"type":"message_stop"}),
        ],
        "openai-completions" | "mistral-conversations" => vec![
            serde_json::json!({"id":"fixture","object":"chat.completion.chunk","created":1,"model":"fixture","choices":[{"index":0,"delta":{"content":"Hello"},"finish_reason":if finish_reason {serde_json::json!("stop")} else {Value::Null}}]}),
        ],
        _ => unreachable!("fixture API must be named explicitly"),
    };
    let mut response = events
        .iter()
        .map(|event| {
            let prefix = event
                .get("type")
                .and_then(Value::as_str)
                .map(|name| format!("event: {name}\n"))
                .unwrap_or_default();
            format!("{prefix}data: {event}\n\n")
        })
        .collect::<String>();
    if api == "openai-completions" {
        response.push_str("data: [DONE]\n\n");
    }
    response
}

fn compatibility_request_cases() -> Vec<(&'static str, &'static str)> {
    vec![
        ("thinking-map-wire-unit", r#"{}"#),
        ("responses-thinking-map-unit", r#"{}"#),
        ("azure-thinking-map-unit", r#"{}"#),
        ("anthropic-thinking-map-unit", r#"{"forceAdaptiveThinking":true}"#),
        ("anthropic-thinking-map-empty", r#"{"forceAdaptiveThinking":true}"#),
        ("anthropic-thinking-map-null", r#"{"forceAdaptiveThinking":true}"#),
        ("anthropic-thinking-map-custom", r#"{"forceAdaptiveThinking":true}"#),
        ("mistral-thinking-map-unit", r#"{}"#),
        ("mistral-thinking-map-empty", r#"{}"#),
        ("mistral-thinking-map-null", r#"{}"#),
        ("mistral-thinking-map-custom", r#"{}"#),
        ("store-null", r#"{"supportsStore":null}"#),
        ("store-zero", r#"{"supportsStore":0}"#),
        ("store-false", r#"{"supportsStore":false}"#),
        ("usage-zero", r#"{"supportsUsageInStreaming":0}"#),
        ("developer-null", r#"{"supportsDeveloperRole":null}"#),
        ("max-tokens-number", r#"{"maxTokensField":17}"#),
        ("max-tokens-object", r#"{"maxTokensField":{"toString":null}}"#),
        ("strict-zero", r#"{"supportsStrictMode":0}"#),
        (
            "kwargs-raw",
            r#"{"thinkingFormat":"chat-template","chatTemplateKwargs":{"literal":"\ud800","unknownVar":{},"boolean":false}}"#,
        ),
        (
            "kwargs-string",
            r#"{"thinkingFormat":"chat-template","chatTemplateKwargs":"\ud800A"}"#,
        ),
        (
            "routing-raw",
            r#"{"openRouterRouting":{"unknown":{"z":1,"a":2}},"vercelGatewayRouting":{"only":0,"order":["fixture"]}}"#,
        ),
        ("routing-zero", r#"{"openRouterRouting":0}"#),
        ("responses-old-reasoning", r#"{"supportsReasoningEffort":false}"#),
        (
            "responses-old-session-format",
            r#"{"sessionAffinityFormat":"openrouter"}"#,
        ),
        ("responses-old-strict", r#"{"supportsStrictMode":true}"#),
        ("responses-old-grammar", r#"{"supportsOpenAIGrammarTools":true}"#),
        ("responses-old-additional", r#"{"supportsAdditionalTools":true}"#),
        ("responses-old-search", r#"{"supportsToolSearch":true}"#),
        (
            "responses-old-cache-mode",
            r#"{"supportsExplicitPromptCacheMode":true}"#,
        ),
        ("responses-developer-zero", r#"{"supportsDeveloperRole":0}"#),
        ("responses-session-zero", r#"{"sendSessionIdHeader":0}"#),
        ("responses-session-null", r#"{"sendSessionIdHeader":null}"#),
        ("anthropic-old-strict", r#"{"supportsStrictTools":true}"#),
        ("anthropic-old-references", r#"{"supportsToolReferences":true}"#),
        ("anthropic-temp-zero", r#"{"supportsTemperature":0}"#),
        ("anthropic-temp-null", r#"{"supportsTemperature":null}"#),
        ("anthropic-eager-zero", r#"{"supportsEagerToolInputStreaming":0}"#),
        ("anthropic-tools-cache-zero", r#"{"supportsCacheControlOnTools":0}"#),
        ("anthropic-adaptive-zero", r#"{"forceAdaptiveThinking":0}"#),
        ("anthropic-adaptive-true", r#"{"forceAdaptiveThinking":true}"#),
        ("anthropic-signature-zero", r#"{"allowEmptySignature":0}"#),
        ("anthropic-signature-true", r#"{"allowEmptySignature":true}"#),
        ("old-finish-reason", r#"{"supportsFinishReason":false}"#),
        (
            "old-chat-template-args",
            r#"{"thinkingFormat":"chat-template","chatTemplateArgs":{"legacy":true}}"#,
        ),
        (
            "old-token-budget",
            r#"{"thinkingFormat":"zai","supportsThinkingTokenBudget":true}"#,
        ),
        ("old-grammar-tools", r#"{"supportsOpenAIGrammarTools":true}"#),
        ("old-deferred-tools", r#"{"deferredToolsMode":"kimi"}"#),
        (
            "old-session-format",
            r#"{"sessionAffinityFormat":"openrouter","sendSessionAffinityHeaders":true}"#,
        ),
    ]
}

#[tokio::test]
async fn schema_accepted_compat_values_reach_the_exact_frozen_provider_request() {
    // Frozen completions1271/566: nullish detection then truthiness.
    // Frozen562: !== false. Frozen571-575: strict max_tokens equality; all
    // other values select max_completion_tokens without invoking ToString.
    let mut oracle = oracle::TsOracle::spawn(
        "model-registry-oracle.ts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let directory = tempfile::tempdir().unwrap();
    let mut failures = Vec::new();
    for (name, raw) in compatibility_request_cases() {
        let api = if name.starts_with("responses-") {
            "openai-responses"
        } else if name.starts_with("azure-") {
            "azure-openai-responses"
        } else if name.starts_with("mistral-") {
            "mistral-conversations"
        } else if name.starts_with("anthropic-") {
            "anthropic-messages"
        } else {
            "openai-completions"
        };
        let model_id = if api == "mistral-conversations" {
            "mistral-small-latest"
        } else {
            "fixture-model"
        };
        let response = response_fixture(api, name != "old-finish-reason");
        let compat = js_json::parse_lossless(raw).unwrap();
        let reasoning = name.contains("thinking-map")
            || matches!(
                name,
                "developer-null"
                    | "thinking-map-wire-unit"
                    | "responses-thinking-map-unit"
                    | "azure-thinking-map-unit"
                    | "anthropic-thinking-map-unit"
                    | "old-chat-template-args"
                    | "old-token-budget"
                    | "kwargs-raw"
                    | "kwargs-string"
                    | "responses-old-reasoning"
                    | "responses-developer-zero"
                    | "anthropic-adaptive-zero"
                    | "anthropic-adaptive-true"
            );
        let tools=(name.starts_with("responses-") || name.starts_with("anthropic-") || matches!(name,"old-grammar-tools"|"old-deferred-tools"|"strict-zero")).then(||js_json!([{"name":"echo","description":"fixture tool","parameters":{"type":"object","properties":{"value":{"type":"string"}}},"constrainedSampling":{"type":"grammar","variants":{"openai_regex":".*"}}}]));
        let messages = if name.starts_with("anthropic-signature-") {
            js_json!([
                {"role":"user","content":"Hello","timestamp":1},
                {"role":"assistant","api":api,"provider":"local","model":"fixture-model","content":[{"type":"thinking","thinking":"fixture thought","thinkingSignature":""}],"usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"stop","timestamp":1}
            ])
        } else if name == "old-grammar-tools" {
            js_json!([
                {"role":"user","content":"Hello","timestamp":1},
                {"role":"assistant","api":"openai-completions","provider":"local","model":"fixture-model","content":[{"type":"toolCall","id":"call-1","name":"echo","arguments":{"value":"fixture"}}],"usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"toolUse","timestamp":1},
                {"role":"toolResult","toolCallId":"call-1","toolName":"echo","content":[{"type":"text","text":"fixture"}],"isError":false,"timestamp":2}
            ])
        } else {
            js_json!([{"role":"user","content":"Hello","timestamp":1}])
        };
        let stream_options = if name.contains("thinking-map") {
            serde_json::json!({"reasoningEffort":"high","sessionId":"registry-fixture"})
        } else if name == "old-session-format" || name.starts_with("responses-") {
            serde_json::json!({"sessionId":"registry-fixture"})
        } else if matches!(
            name,
            "old-chat-template-args" | "old-token-budget" | "kwargs-raw" | "kwargs-string" | "thinking-map-wire-unit"
        ) {
            serde_json::json!({"reasoningEffort":"high"})
        } else if name.starts_with("anthropic-adaptive-") {
            serde_json::json!({"thinkingEnabled":true})
        } else if name.starts_with("anthropic-temp-") {
            serde_json::json!({"temperature":0.25})
        } else {
            serde_json::json!({})
        };
        let mut definition = js_json!({"id":model_id,"reasoning":reasoning,"compat":compat});
        if name.contains("thinking-map") {
            definition["thinkingLevelMap"] = if name.ends_with("-empty") {
                js_json!({"high":""})
            } else if name.ends_with("-null") {
                js_json!({"high":null})
            } else if name.ends_with("-custom") {
                js_json!({"high":"provider-new-effort"})
            } else {
                js_json::parse_lossless(r#"{"high":"\ud800"}"#).unwrap()
            };
        }
        let config = js_json!({"providers":{"local":{
            "api":api,"apiKey":"private-fixture-key","baseUrl":"https://fixture.invalid",
            "models":[definition]
        }}});
        let path = directory.path().join(format!("{name}.json"));
        std::fs::write(&path, js_json::stringify_lossless(&config)).unwrap();
        let expected: JsValue =
            oracle.request(&js_json!({"id":name,"operation":"config_request","modelsPath":path.to_str().unwrap(),"api":api,"modelId":model_id,"simple":name.starts_with("anthropic-thinking-map-") || name.starts_with("mistral-thinking-map-"),"response":response,"streamOptions":stream_options,"tools":tools,"messages":messages}));
        assert_eq!(expected["ok"], JsValue::Bool(true), "{name}: {expected:?}");
        if name.starts_with("max-tokens-") {
            assert_eq!(expected["output"]["errorMessage"], JsValue::Null);
            assert_eq!(
                expected["output"]["requests"][0]["body"]["max_completion_tokens"],
                JsValue::Number(11.0)
            );
        }
        let registry = ModelRegistry::create(
            AuthStorage::in_memory(Vec::<(String, Value)>::new()),
            Some(path.to_str().unwrap().into()),
        )
        .unwrap();
        let capture = Arc::new(RequestCapture {
            requests: Mutex::new(Vec::new()),
            response,
            status: 200,
        });
        let actual = if let Some(model) = registry.find("local", model_id) {
            let ResolvedRequestAuth::Ok { api_key, headers, env } =
                registry.get_api_key_and_headers(&model).await.unwrap()
            else {
                panic!("fixture auth must resolve");
            };
            let mut context_value = js_json!({"systemPrompt":"fixture system","messages":messages});
            if let Some(tools) = &tools {
                context_value["tools"] = tools.clone();
            }
            let context = js_json::from_value(&context_value).unwrap();
            let options = StreamOptions {
                request: ProviderRequestOptions {
                    api_key,
                    fetch: Some(capture.clone()),
                    headers,
                    env,
                    ..Default::default()
                },
                max_tokens: Some(11.0),
                temperature: stream_options.get("temperature").and_then(Value::as_f64),
                session_id: stream_options.get("sessionId").and_then(Value::as_str).map(Into::into),
                extra: stream_options.as_object().unwrap().clone(),
                ..Default::default()
            };
            let mut stream = match api {
                "openai-responses" => pi_ai::api::openai_responses::stream(model, context, options),
                "azure-openai-responses" => pi_ai::api::azure_openai_responses::stream(model, context, options),
                "anthropic-messages" if name.starts_with("anthropic-thinking-map-") => {
                    pi_ai::api::anthropic_messages::stream_simple(
                        model,
                        context,
                        SimpleStreamOptions {
                            stream: options,
                            reasoning: Some(pi_ai::types::ThinkingLevel::High),
                            ..Default::default()
                        },
                    )
                    .expect("simple acquisition")
                }
                "anthropic-messages" => pi_ai::api::anthropic_messages::stream(model, context, options),
                "mistral-conversations" => pi_ai::api::mistral_conversations::stream_simple(
                    model,
                    context,
                    SimpleStreamOptions {
                        stream: options,
                        reasoning: Some(pi_ai::types::ThinkingLevel::High),
                        ..Default::default()
                    },
                )
                .expect("simple acquisition"),
                "openai-completions" => pi_ai::api::openai_completions::stream(model, context, options),
                _ => unreachable!("fixture API must be named explicitly"),
            };
            while let Some(event) = stream.next().await {
                event.unwrap();
            }
            let result = stream.result().await.unwrap();
            js_json!({"error":registry.get_error(),"requests":capture.requests.lock().unwrap().clone(),"errorMessage":result.error_message,"stopReason":result.stop_reason})
        } else {
            js_json!({"error":registry.get_error(),"requests":[],"modelMissing":true})
        };
        if actual != expected["output"] {
            eprintln!("compat consumer mismatch {name}");
            failures.push(js_json!({"case":name,"actual":actual,"expected":expected["output"]}));
        }
    }
    let log = std::env::temp_dir().join(format!("q15e-registry-compat-requests-{}.json", std::process::id()));
    std::fs::write(&log, js_json::stringify_lossless(&JsValue::Array(failures.clone()))).unwrap();
    assert!(
        failures.is_empty(),
        "{} compat request differences; {}",
        failures.len(),
        log.display()
    );
}

async fn capture_bedrock_models_request(definition: JsValue, reasoning: Option<&str>) -> JsValue {
    // Both real SDK/native transports reject after full HTTP capture. This
    // proves request construction, not event-stream/result parity.
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("bedrock.json");
    let config = js_json!({"providers":{"local":{"api":"bedrock-converse-stream","apiKey":"fixture","baseUrl":"https://fixture.invalid","models":[definition]}}});
    std::fs::write(&path, js_json::stringify_lossless(&config)).unwrap();
    let registry = ModelRegistry::create(
        AuthStorage::in_memory(Vec::<(String, Value)>::new()),
        Some(path.to_str().unwrap().into()),
    )
    .unwrap();
    let model = registry.find("local", "fixture-model").unwrap();
    let context_value = js_json!({"messages":[{"role":"user","content":"fixture","timestamp":1}],"tools":[{"name":"echo","description":"fixture tool","parameters":{"type":"object","properties":{"value":{"type":"string"}}},"constrainedSampling":{"type":"json_schema","strict":"require"}}]});
    let mut options = serde_json::json!({"region":"us-east-1","bearerToken":"fixture","maxRetries":0});
    if let Some(reasoning) = reasoning {
        options["reasoning"] = serde_json::json!(reasoning);
    }
    let mut oracle = oracle::TsOracle::spawn("bedrock-converse-stream-oracle.ts", "packages/ai/src");
    let expected:JsValue=oracle.request(&js_json!({"id":"bedrock-model-request","modelsPath":path.to_str().unwrap(),"context":context_value,"options":options,"script":[{"status":400,"headers":[["content-type","application/json"]],"text":"{\"message\":\"fixture request boundary\"}"}]}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let source_requests = expected["output"]["requests"].as_array().unwrap();
    assert_eq!(source_requests.len(), 1, "{expected:?}");
    let source_body =
        js_json::parse_lossless(source_requests[0]["body"].as_string().unwrap().to_utf8().unwrap()).unwrap();
    let capture = Arc::new(RequestCapture {
        requests: Mutex::new(Vec::new()),
        response: r#"{"message":"fixture request boundary"}"#.into(),
        status: 400,
    });
    let mut stream = pi_ai::api::bedrock_converse_stream::stream(
        model,
        js_json::from_value(&context_value).unwrap(),
        StreamOptions {
            request: ProviderRequestOptions {
                fetch: Some(capture.clone()),
                max_retries: Some(0.0),
                ..Default::default()
            },
            extra: options.as_object().unwrap().clone(),
            ..Default::default()
        },
    );
    while let Some(event) = stream.next().await {
        event.unwrap();
    }
    let result = stream.result().await.unwrap();
    assert_eq!(result.stop_reason, pi_ai::types::StopReason::Error);
    let requests = capture.requests.lock().unwrap();
    assert_eq!(requests.len(), 1, "native error {:?}", result.error_message);
    assert_eq!(requests[0]["body"], source_body);
    source_body
}

#[tokio::test]
async fn bedrock_models_json_strict_metadata_never_changes_the_frozen_sdk_request() {
    // Frozen906-910 copies parameters verbatim; unknown strict metadata is inert.
    let body = capture_bedrock_models_request(
        js_json!({"id":"fixture-model","compat":{"supportsStrictMode":true}}),
        None,
    )
    .await;
    assert_eq!(body["toolConfig"]["tools"][0]["toolSpec"].get("strict"), None);
}

#[tokio::test]
async fn bedrock_models_json_thinking_map_values_match_the_real_frozen_sdk_request() {
    // Frozen594-595/1017-1025 casts every string into JSON output_config.
    for raw in [r#""\ud800""#, r#""""#, "null", r#""provider-new-effort""#] {
        let high = js_json::parse_lossless(raw).unwrap();
        let expected = if high == JsValue::Null {
            JsValue::String("high".into())
        } else {
            high.clone()
        };
        let body = capture_bedrock_models_request(
            js_json!({"id":"fixture-model","name":"Claude Opus 4.7","reasoning":true,"thinkingLevelMap":{"high":high}}),
            Some("high"),
        )
        .await;
        assert_eq!(
            body["additionalModelRequestFields"]["output_config"]["effort"],
            expected
        );
    }
}
