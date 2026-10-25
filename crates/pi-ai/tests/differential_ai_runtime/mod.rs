use crate::shared_oracle as oracle;

use pi_js_compat::js_json::JsString;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use pi_ai::auth::{
    ApiKeyAuth, ApiKeyResolveInput, AuthContext, AuthError, Credential, CredentialStore, InMemoryCredentialStore,
    ModelAuth, OAuthAuth, OAuthCredential, ProviderAuth, env_api_key_auth, resolve_provider_auth,
};
use pi_ai::models::{
    CreateModelsOptions, CreateProviderOptions, Models, MutableModels, ProviderApi, calculate_cost,
    clamp_thinking_level, create_models, create_provider, get_supported_thinking_levels,
};
use pi_ai::providers::faux::{
    FauxAssistantMessageOptions, FauxContent, FauxModelDefinition, FauxResponseStep, FauxTokenSize,
    RegisterFauxProviderOptions, create_faux_core, faux_assistant_message,
};
use pi_ai::request::{
    BoxFuture, CallbackError, ProviderRequestOptions, ProviderStreams, SimpleStreamOptions, StreamOptions,
};
use pi_ai::types::{
    AssistantContent, AssistantMessageEvent, Context, DoneReason, Model, ModelThinkingLevel, StopReason,
};
use pi_ai::utils::event_stream::{AssistantMessageEventStream, create_assistant_message_event_stream};
use serde_json::{Value, json};

pub(crate) fn run() {
    let mut oracle = oracle::TsOracle::spawn("ai-runtime-oracle.ts", "packages/ai/src");
    compare_cost(&mut oracle);
    compare_thinking(&mut oracle);
    compare_equality(&mut oracle);
    compare_auth_resolution(&mut oracle);
    compare_faux_sequence(&mut oracle);
    compare_lazy_stream_error(&mut oracle);
    compare_refresh(&mut oracle);
    compare_apply_auth(&mut oracle);
}

fn request(oracle: &mut oracle::TsOracle, value: Value) -> Value {
    let response = oracle.request(&value);
    assert_eq!(response["ok"], json!(true), "oracle failed: {response}");
    response["output"].clone()
}

fn model_json(provider: &str, id: &str) -> Value {
    json!({
        "id": id,
        "name": id,
        "api": "test-api",
        "provider": provider,
        "baseUrl": "https://example.test/v1",
        "reasoning": false,
        "input": ["text"],
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": 10000,
        "maxTokens": 1000
    })
}

fn compare_cost(oracle: &mut oracle::TsOracle) {
    let model = json!({
        "id": "gpt-5.6-sol",
        "name": "gpt-5.6-sol",
        "api": "test-api",
        "provider": "openai",
        "baseUrl": "https://example.test/v1",
        "reasoning": false,
        "input": ["text"],
        "cost": {
            "input": 5,
            "output": 30,
            "cacheRead": 0.5,
            "cacheWrite": 6.25,
            "tiers": [{"inputTokensAbove": 272000, "input": 10, "output": 45, "cacheRead": 1, "cacheWrite": 12.5}]
        },
        "contextWindow": 10000,
        "maxTokens": 1000
    });
    let usage = json!({
        "input": 200000,
        "output": 100000,
        "cacheRead": 72000,
        "cacheWrite": 1,
        "totalTokens": 372001,
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0, "total": 0}
    });
    let mut cases = vec![json!({"model":model,"usage":usage})];
    for (cache_write, long_write) in [(0.0, None), (10.0, Some(0.0)), (10.0, Some(4.0)), (3.5, Some(8.25))] {
        let mut usage = cases[0]["usage"].clone();
        usage["cacheWrite"] = json!(cache_write);
        if let Some(value) = long_write {
            usage["cacheWrite1h"] = json!(value);
        }
        cases.push(json!({"model":model,"usage":usage}));
    }
    let output = request(oracle, json!({"id":1,"operation":"cost","cases":cases}));
    let actual: Vec<_> = cases
        .into_iter()
        .map(|case| {
            let model: Model = serde_json::from_value(case["model"].clone()).unwrap();
            let usage: pi_ai::types::Usage = serde_json::from_value(case["usage"].clone()).unwrap();
            serde_json::to_value(calculate_cost(&model, &usage)).unwrap()
        })
        .collect();
    assert_eq!(output["results"], json!(actual));
}

fn compare_thinking(oracle: &mut oracle::TsOracle) {
    let mut model = model_json("p", "m");
    model["reasoning"] = json!(true);
    model["thinkingLevelMap"] = json!({"xhigh": "x", "max": null, "high": null});
    let levels = ["off", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"];
    let mut cases = vec![json!({"model":model,"levels":levels})];
    for (reasoning, map) in [
        (false, None),
        (true, None),
        (true, Some(json!({}))),
        (
            true,
            Some(
                json!({"off":null,"minimal":null,"low":null,"medium":null,"high":null,"xhigh":null,"max":null,"ultra":null}),
            ),
        ),
        (true, Some(json!({"high":null,"max":"max","ultra":"ultra"}))),
    ] {
        let mut model = model_json("p", "m");
        model["reasoning"] = json!(reasoning);
        if let Some(map) = map {
            model["thinkingLevelMap"] = map;
        }
        cases.push(json!({"model":model,"levels":levels}));
    }
    let output = request(oracle, json!({"id":2,"operation":"thinking","cases":cases}));
    let actual: Vec<_> = cases
        .into_iter()
        .map(|case| {
            let model: Model = serde_json::from_value(case["model"].clone()).unwrap();
            let clamped: Vec<_> = levels
                .iter()
                .map(|level| {
                    let level: ModelThinkingLevel = serde_json::from_value(json!(level)).unwrap();
                    clamp_thinking_level(&model, level)
                })
                .collect();
            json!({"supported":get_supported_thinking_levels(&model),"clamped":clamped})
        })
        .collect();
    assert_eq!(output["results"], json!(actual));
}

fn compare_equality(oracle: &mut oracle::TsOracle) {
    let a = model_json("p", "m");
    let mut other_api = a.clone();
    other_api["api"] = json!("other-api");
    let cases = json!([
        {"a":null,"b":null},{"a":a,"b":null},{"a":null,"b":a},
        {"a":a,"b":other_api},{"a":a,"b":model_json("other","m")},
        {"a":a,"b":model_json("p","different")},
        {"a":model_json("p","e\u{301}"),"b":model_json("p","é")},
    ]);
    let output = request(oracle, json!({"id":8,"operation":"equality","cases":cases}));
    let parse = |value: &Value| {
        if value.is_null() {
            None
        } else {
            Some(serde_json::from_value::<Model>(value.clone()).unwrap())
        }
    };
    let actual: Vec<_> = cases
        .as_array()
        .unwrap()
        .iter()
        .map(|case| pi_ai::models::models_are_equal(parse(&case["a"]).as_ref(), parse(&case["b"]).as_ref()))
        .collect();
    assert_eq!(output["results"], json!(actual));
}

struct TestAuthContext {
    env: BTreeMap<String, String>,
}

impl AuthContext for TestAuthContext {
    fn env(
        &self,
        name: &str,
    ) -> BoxFuture<'_, Result<Option<pi_js_compat::js_json::JsString>, pi_ai::auth::AuthError>> {
        let value = self.env.get(name).cloned();
        Box::pin(async move { Ok(value.map(pi_js_compat::js_json::JsString::from)) })
    }
    fn file_exists(
        &self,
        _path: &pi_js_compat::js_json::JsString,
    ) -> BoxFuture<'_, Result<bool, pi_ai::auth::AuthError>> {
        Box::pin(async { Ok(false) })
    }
}

struct IdentityOAuth;

impl OAuthAuth for IdentityOAuth {
    fn name(&self) -> JsString {
        "Test OAuth".into()
    }
    fn login(
        &self,
        _interaction: Arc<dyn pi_ai::auth::AuthInteraction>,
    ) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        Box::pin(async { Err(AuthError::failed("not used")) })
    }
    fn refresh(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        Box::pin(async move { Ok(credential) })
    }
    fn to_auth(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<ModelAuth, AuthError>> {
        Box::pin(async move {
            Ok(ModelAuth {
                api_key: Some(credential.access),
                headers: None,
                base_url: None,
            })
        })
    }
}

fn auth_cases() -> Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    json!([
        {
            "name": "env_order",
            "providerId": "p1",
            "apiKeyEnv": ["FIRST_KEY", "SECOND_KEY"],
            "env": {"SECOND_KEY": "second"}
        },
        {
            "name": "stored_api_key",
            "providerId": "p1",
            "apiKeyEnv": ["FIRST_KEY"],
            "credentials": [{"type": "api_key", "key": "stored"}],
            "env": {"FIRST_KEY": "env"}
        },
        {
            "name": "stored_oauth_valid",
            "providerId": "p1",
            "oauth": true,
            "credentials": [{"type": "oauth", "refresh": "r", "access": "oauth-token", "expires": now + 600000}]
        },
        {
            "name": "stored_without_handler",
            "providerId": "p1",
            "apiKeyEnv": ["FIRST_KEY"],
            "credentials": [{"type": "oauth", "refresh": "r", "access": "stale", "expires": now + 600000}]
        },
        {
            "name": "explicit",
            "providerId": "p1",
            "apiKeyEnv": ["FIRST_KEY"],
            "env": {"FIRST_KEY": "env"},
            "overrides": {"apiKey": "explicit"}
        },
        {
            "name": "missing",
            "providerId": "p1",
            "apiKeyEnv": ["FIRST_KEY"]
        }
    ])
}

fn compare_auth_resolution(oracle: &mut oracle::TsOracle) {
    let cases = auth_cases();
    let output = request(oracle, json!({"id": 3, "operation": "auth_resolve", "cases": cases}));
    let expected = output["results"].as_array().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    for (index, case) in cases.as_array().unwrap().iter().enumerate() {
        let actual = runtime.block_on(resolve_auth_case(case));
        assert_eq!(&actual, &expected[index], "auth case {index} mismatch");
    }
}

async fn resolve_auth_case(case: &Value) -> Value {
    let name = case["name"].as_str().unwrap();
    let provider_id = case["providerId"].as_str().unwrap();
    let credentials = Arc::new(InMemoryCredentialStore::new());
    for credential in case["credentials"].as_array().into_iter().flatten() {
        let credential: Credential = serde_json::from_value(credential.clone()).unwrap();
        credentials
            .modify(
                provider_id,
                Arc::new(move |_| {
                    let credential = credential.clone();
                    Box::pin(async move { Ok(Some(credential)) })
                }),
            )
            .await
            .unwrap();
    }
    let env_vars = case["apiKeyEnv"]
        .as_array()
        .map(|vars| vars.iter().map(|value| value.as_str().unwrap()).collect::<Vec<_>>())
        .unwrap_or_default();
    let provider_auth = ProviderAuth {
        api_key: (!env_vars.is_empty())
            .then(|| Arc::new(env_api_key_auth("Test key", &env_vars)) as Arc<dyn ApiKeyAuth>),
        oauth: case["oauth"]
            .as_bool()
            .unwrap_or(false)
            .then(|| Arc::new(IdentityOAuth) as Arc<dyn OAuthAuth>),
    };
    let env = case["env"]
        .as_object()
        .map(|env| {
            env.iter()
                .map(|(key, value)| (key.clone(), value.as_str().unwrap().to_owned()))
                .collect()
        })
        .unwrap_or_default();
    let overrides = pi_ai::auth::AuthResolutionOverrides {
        api_key: (case["overrides"]["apiKey"].as_str().map(str::to_owned)).map(pi_js_compat::js_json::JsString::from),
        ..Default::default()
    };
    let model = serde_json::from_value(model_json(provider_id, "auth-model")).unwrap();
    let result = resolve_provider_auth(
        provider_id,
        &provider_auth,
        pi_ai::auth::AuthModel::Chat(model),
        Arc::clone(&credentials) as Arc<dyn CredentialStore>,
        Arc::new(TestAuthContext { env }),
        overrides,
    )
    .await;
    match result {
        Ok(Some(result)) => json!({"name": name, "ok": true, "result": result}),
        Ok(None) => json!({"name": name, "ok": true}),
        Err(error) => json!({
            "name": name,
            "ok": false,
            "error": {"name": "ModelsError", "code": error.code.as_str(), "message": error.message}
        }),
    }
}

fn faux_cases() -> Value {
    let context_one = json!({"messages": [{"role": "user", "content": "hello", "timestamp": 1}]});
    let context_two = json!({"messages": [
        {"role": "user", "content": "hello", "timestamp": 1},
        {"role": "user", "content": "follow up", "timestamp": 2}
    ]});
    json!([
        {
            "name": "fixed_chunks",
            "tokenSize": {"min": 1, "max": 1},
            "response": {
                "content": [
                    {"type": "thinking", "thinking": "go"},
                    {"type": "text", "text": "ok"},
                    {"type": "toolCall", "id": "tool-1", "name": "echo", "arguments": {"text": "hi"}}
                ],
                "stopReason": "toolUse"
            },
            "steps": [{"context": context_one, "options": {}}]
        },
        {
            "name": "session_cache",
            "tokenSize": {"min": 3, "max": 3},
            "response": {"content": [{"type": "text", "text": "answer"}], "stopReason": "stop"},
            "steps": [
                {"context": context_one, "options": {"sessionId": "session-1", "cacheRetention": "short"}},
                {"context": context_two, "options": {"sessionId": "session-1", "cacheRetention": "short"}}
            ]
        },
        {
            "name": "cache_none",
            "tokenSize": {"min": 3, "max": 3},
            "response": {"content": [{"type": "text", "text": "answer"}], "stopReason": "stop"},
            "steps": [
                {"context": context_one, "options": {"sessionId": "session-1", "cacheRetention": "none"}},
                {"context": context_two, "options": {"sessionId": "session-1", "cacheRetention": "none"}}
            ]
        }
    ])
}

fn compare_faux_sequence(oracle: &mut oracle::TsOracle) {
    let cases = faux_cases();
    let output = request(oracle, json!({"id": 4, "operation": "faux_sequence", "cases": cases}));
    let expected = output["results"].as_array().unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    for (index, case) in cases.as_array().unwrap().iter().enumerate() {
        let actual = runtime.block_on(run_faux_case(case));
        assert_eq!(&actual, &expected[index], "faux case {index} mismatch");
    }
}

async fn run_faux_case(case: &Value) -> Value {
    let token_size = case["tokenSize"].as_object().unwrap();
    let core = create_faux_core(RegisterFauxProviderOptions {
        api: Some("faux:test".to_owned()),
        provider: Some("faux".to_owned()),
        models: Some(vec![FauxModelDefinition {
            id: "faux-1".to_owned(),
            ..Default::default()
        }]),
        token_size: Some(FauxTokenSize {
            min: token_size["min"].as_f64(),
            max: token_size["max"].as_f64(),
        }),
        ..Default::default()
    });
    let blocks = case["response"]["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|block| serde_json::from_value::<AssistantContent>(block.clone()).unwrap())
        .collect::<Vec<_>>();
    let message = faux_assistant_message(
        FauxContent::Blocks(blocks),
        FauxAssistantMessageOptions {
            stop_reason: case["response"]["stopReason"]
                .as_str()
                .map(|reason| serde_json::from_value(json!(reason)).unwrap()),
            error_message: (case["response"]["errorMessage"].as_str().map(str::to_owned))
                .map(pi_js_compat::js_json::JsString::from),
            ..Default::default()
        },
    );
    core.set_responses(vec![FauxResponseStep::message(message)]);

    let mut steps = Vec::new();
    for step in case["steps"].as_array().unwrap() {
        let context: Context = serde_json::from_value(step["context"].clone()).unwrap();
        let options = step["options"].as_object().unwrap();
        let session_id = options.get("sessionId").and_then(Value::as_str).map(str::to_owned);
        let cache_retention = options
            .get("cacheRetention")
            .and_then(Value::as_str)
            .map(|retention| serde_json::from_value(json!(retention)).unwrap());
        let mut stream = core
            .stream_simple(
                core.get_model(None).unwrap(),
                context,
                SimpleStreamOptions {
                    stream: StreamOptions {
                        session_id: session_id.map(pi_js_compat::js_json::JsString::from),
                        cache_retention,
                        ..Default::default()
                    },
                    ..Default::default()
                },
            )
            .expect("acquisition");
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event_type(&event.unwrap()));
        }
        let result = stream.result().await.unwrap();
        let mut result = serde_json::to_value(result).unwrap();
        result.as_object_mut().unwrap().remove("timestamp");
        steps.push(json!({"events": events, "result": result}));
    }
    json!({"name": case["name"], "steps": steps})
}

fn event_type(event: &AssistantMessageEvent) -> &'static str {
    match event {
        AssistantMessageEvent::Start { .. } => "start",
        AssistantMessageEvent::TextStart { .. } => "text_start",
        AssistantMessageEvent::TextDelta { .. } => "text_delta",
        AssistantMessageEvent::TextEnd { .. } => "text_end",
        AssistantMessageEvent::ThinkingStart { .. } => "thinking_start",
        AssistantMessageEvent::ThinkingDelta { .. } => "thinking_delta",
        AssistantMessageEvent::ThinkingEnd { .. } => "thinking_end",
        AssistantMessageEvent::ToolCallStart { .. } => "toolcall_start",
        AssistantMessageEvent::ToolCallDelta { .. } => "toolcall_delta",
        AssistantMessageEvent::ToolCallEnd { .. } => "toolcall_end",
        AssistantMessageEvent::Done { .. } => "done",
        AssistantMessageEvent::Error { .. } => "error",
    }
}

fn compare_lazy_stream_error(oracle: &mut oracle::TsOracle) {
    let output = request(oracle, json!({"id": 5, "operation": "lazy_stream_error"}));
    let model = serde_json::from_value::<Model>(model_json("lazy", "m1")).unwrap();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let (events, result) = runtime.block_on(async move {
        let mut stream = pi_ai::api::lazy::lazy_stream(model, move || {
            Box::pin(async move { Err(CallbackError::new("setup boom")) })
        });
        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event_type(&event.unwrap()));
        }
        let result = stream.result().await.unwrap();
        (events, result)
    });
    let mut result = serde_json::to_value(result).unwrap();
    result.as_object_mut().unwrap().remove("timestamp");
    assert_eq!(output["events"], json!(events));
    assert_eq!(output["result"], result);
}

struct EmptyStreams;

impl ProviderStreams for EmptyStreams {
    fn stream(
        &self,
        _model: Model,
        _context: Context,
        _options: StreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        Ok(create_assistant_message_event_stream().1)
    }
    fn stream_simple(
        &self,
        _model: Model,
        _context: Context,
        _options: SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        Ok(create_assistant_message_event_stream().1)
    }
}

fn compare_refresh(oracle: &mut oracle::TsOracle) {
    let output = request(oracle, json!({"id": 6, "operation": "refresh"}));
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(create_provider(CreateProviderOptions {
        id: "dynamic".to_owned(),
        name: None,
        base_url: None,
        headers: None,
        auth: ProviderAuth::api_key(Arc::new(AmbientAuth)),
        models: vec![serde_json::from_value(model_json("dynamic", "static")).unwrap()],
        refresh_models: Some(Arc::new(|| {
            Box::pin(async {
                Ok::<Vec<Model>, CallbackError>(vec![serde_json::from_value(model_json("dynamic", "fetched")).unwrap()])
            })
        })),
        api: ProviderApi::Single(Arc::new(EmptyStreams)),
    }));
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(models.refresh(Some("dynamic"))).unwrap();
    let actual = json!({
        "models": models.get_models(Some("dynamic")).iter().map(|model| model.id.clone()).collect::<Vec<_>>(),
    });
    assert_eq!(output, actual);
}

struct AmbientAuth;

impl ApiKeyAuth for AmbientAuth {
    fn name(&self) -> JsString {
        "Ambient".into()
    }
    fn resolve(
        &self,
        _input: ApiKeyResolveInput,
    ) -> BoxFuture<'static, Result<Option<pi_ai::auth::AuthResult>, AuthError>> {
        Box::pin(async {
            Ok(Some(pi_ai::auth::AuthResult {
                auth: ModelAuth::default(),
                env: None,
                source: None,
            }))
        })
    }
}

struct HeaderAuth;

impl ApiKeyAuth for HeaderAuth {
    fn name(&self) -> JsString {
        "Test".into()
    }
    fn resolve(
        &self,
        _input: ApiKeyResolveInput,
    ) -> BoxFuture<'static, Result<Option<pi_ai::auth::AuthResult>, AuthError>> {
        Box::pin(async {
            Ok(Some(pi_ai::auth::AuthResult {
                auth: ModelAuth {
                    api_key: Some(("provider-key".to_owned()).into()),
                    headers: Some(indexmap::IndexMap::from([
                        ("Authorization".into(), Some("Bearer provider".into())),
                        ("X-Shared".into(), Some("provider".into())),
                    ])),
                    base_url: Some(("https://resolved.test/v1".to_owned()).into()),
                },
                env: Some(indexmap::IndexMap::from([
                    ("PROVIDER_ONLY".into(), "provider".into()),
                    ("SHARED".into(), "provider".into()),
                ])),
                source: None,
            }))
        })
    }
}

struct CapturingStreams {
    captured: Arc<Mutex<Option<Value>>>,
}

impl ProviderStreams for CapturingStreams {
    fn stream(
        &self,
        model: Model,
        _context: Context,
        options: StreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        *self.captured.lock().unwrap() = Some(json!({
            "baseUrl": model.base_url,
            "apiKey": options.request.api_key,
            "headers": options.request.headers,
            "env": options.request.env,
        }));
        let (sender, stream) = create_assistant_message_event_stream();
        let mut message = pi_ai::types::AssistantMessage::empty(&model.api, &model.provider, &model.id, 1.0);
        message.content = vec![AssistantContent::Text(pi_ai::types::TextContent::new("ok"))];
        message.stop_reason = StopReason::Stop;
        sender.push(AssistantMessageEvent::Start {
            partial: message.clone(),
        });
        sender.push(AssistantMessageEvent::Done {
            reason: DoneReason::Stop,
            message: message.clone(),
        });
        sender.end(Some(message));
        Ok(stream)
    }
    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        self.stream(model, context, options.stream)
    }
}

fn compare_apply_auth(oracle: &mut oracle::TsOracle) {
    let output = request(oracle, json!({"id": 7, "operation": "apply_auth"}));
    let captured = Arc::new(Mutex::new(None));
    let mut model: Model = serde_json::from_value(model_json("p1", "m1")).unwrap();
    model.headers = Some(indexmap::IndexMap::from([
        ("x-model".into(), "model".into()),
        ("x-shared".into(), "model".into()),
    ]));
    let provider = create_provider(CreateProviderOptions {
        id: "p1".to_owned(),
        name: None,
        base_url: None,
        headers: None,
        auth: ProviderAuth::api_key(Arc::new(HeaderAuth)),
        models: vec![model.clone()],
        refresh_models: None,
        api: ProviderApi::Single(Arc::new(CapturingStreams {
            captured: Arc::clone(&captured),
        })),
    });
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(provider);
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let result = runtime
        .block_on(async {
            models
                .complete_simple(
                    model,
                    Context {
                        system_prompt: None,
                        messages: Vec::new(),
                        tools: None,
                    },
                    SimpleStreamOptions {
                        stream: StreamOptions {
                            request: ProviderRequestOptions {
                                api_key: Some(("request-key".to_owned()).into()),
                                headers: Some(indexmap::IndexMap::from([
                                    ("X-Request".into(), Some("request".into())),
                                    ("x-shared".into(), Some("request".into())),
                                ])),
                                env: Some(indexmap::IndexMap::from([
                                    ("REQUEST_ONLY".into(), "request".into()),
                                    ("SHARED".into(), "request".into()),
                                ])),
                                ..Default::default()
                            },
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                )
                .await
        })
        .unwrap();
    let actual = json!({
        "captured": captured.lock().unwrap().clone().unwrap(),
        "stopReason": result.stop_reason,
    });
    assert_eq!(output, actual);
}
