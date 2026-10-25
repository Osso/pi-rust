use crate::shared_oracle as oracle;
use pi_ai::auth::{ApiKeyAuth, ApiKeyResolveInput, AuthError, AuthResult, ModelAuth, ProviderAuth};
use pi_ai::models::{
    CreateModelsOptions, CreateProviderOptions, Models, MutableModels, ProviderApi, create_models, create_provider,
};
use pi_ai::request::{BoxFuture, ProviderStreams};
use pi_ai::types::{Context, Model};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use serde_json::json;
use std::sync::Arc;

struct Ambient;
impl ApiKeyAuth for Ambient {
    fn name(&self) -> JsString {
        "Ambient".into()
    }
    fn resolve(&self, _: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        Box::pin(async {
            Ok(Some(AuthResult {
                auth: ModelAuth::default(),
                env: None,
                source: None,
            }))
        })
    }
}
struct UnusedStreams;
impl ProviderStreams for UnusedStreams {
    fn stream(
        &self,
        _: Model,
        _: Context,
        _: pi_ai::request::StreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        unreachable!("must not stream")
    }
    fn stream_simple(
        &self,
        _: Model,
        _: Context,
        _: pi_ai::request::SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        unreachable!("must not stream")
    }
}
fn model(id: &str) -> Model {
    serde_json::from_value(json!({"id":id,"name":id,"api":"fixture","provider":"dynamic","baseUrl":"http://fixture.invalid","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":4096,"maxTokens":512})).unwrap()
}
#[tokio::test]
async fn refresh_replaces_the_provider_model_list() {
    let models = create_models(CreateModelsOptions::default());
    models.set_provider(create_provider(CreateProviderOptions {
        id: "dynamic".into(),
        name: None,
        base_url: None,
        headers: None,
        auth: ProviderAuth::api_key(Arc::new(Ambient)),
        models: vec![model("old")],
        refresh_models: Some(Arc::new(|| Box::pin(async { Ok(vec![model("new")]) }))),
        api: ProviderApi::Single(Arc::new(UnusedStreams)),
    }));
    let before: Vec<_> = models
        .get_models(Some("dynamic"))
        .iter()
        .map(|model| model.id.clone())
        .collect();
    models.refresh(Some("dynamic")).await.unwrap();
    let actual = json!({"before":before,"after":models.get_models(Some("dynamic")).iter().map(|model| model.id.clone()).collect::<Vec<_>>()});
    let mut source = oracle::TsOracle::spawn("models-contract-oracle.ts", "packages/ai/src/models.ts");
    let expected = source.request(&json!({"id":1,"initial":[model("old")],"refreshed":[model("new")]}));
    assert_eq!(expected["ok"], true, "{expected}");
    assert_eq!(actual, expected["output"]);
}

use pi_ai::models::{FetchModelsFn, ModelsError, ModelsErrorCode, Provider, RefreshModelsFn};
use pi_ai::request::CallbackError;
use pi_js_compat::abort::AbortReason;
use pi_js_compat::js_json;
use pi_js_compat::js_json::{JsString, JsValue};
use std::collections::VecDeque;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

fn configured(id: &str, models: Vec<Model>, refresh: Option<FetchModelsFn>) -> Arc<dyn Provider> {
    create_provider(CreateProviderOptions {
        id: id.into(),
        name: None,
        base_url: None,
        headers: None,
        auth: ProviderAuth::api_key(Arc::new(Ambient)),
        models,
        refresh_models: refresh,
        api: ProviderApi::Single(Arc::new(UnusedStreams)),
    })
}
fn model_ids(models: Vec<Model>) -> Vec<String> {
    models.into_iter().map(|model| model.id).collect()
}
fn assert_source(operation: &str, actual: JsValue, cause: JsValue) {
    let mut source = oracle::TsOracle::spawn("models-contract-oracle.ts", "packages/ai/src/models.ts");
    let expected = source.request(&js_json!({"id":operation,"operation":operation,
        "initial":[model("old")],"refreshed":[model("new")],"cause":cause}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    assert_eq!(actual, expected["output"], "{operation}");
}

struct CatalogProvider {
    id: String,
    models: Arc<Mutex<Result<Vec<Model>, CallbackError>>>,
    refresh: Option<RefreshModelsFn>,
    auth: ProviderAuth,
}
impl Provider for CatalogProvider {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.id
    }
    fn base_url(&self) -> Option<&JsString> {
        None
    }
    fn headers(&self) -> Option<&pi_ai::types::ProviderHeaders> {
        None
    }
    fn auth(&self) -> &ProviderAuth {
        &self.auth
    }
    fn get_models(&self) -> Result<Vec<Model>, CallbackError> {
        self.models.lock().unwrap().clone()
    }
    fn refresh_models(&self) -> Option<RefreshModelsFn> {
        self.refresh.clone()
    }
    fn stream(
        &self,
        _: Model,
        _: Context,
        _: pi_ai::request::StreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        unreachable!("must not stream")
    }
    fn stream_simple(
        &self,
        _: Model,
        _: Context,
        _: pi_ai::request::SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        unreachable!("must not stream")
    }
}
#[test]
fn provider_order_lookup_and_get_models_only_catch_match_source() {
    let models = create_models(Default::default());
    models.set_provider(configured("first", vec![model("old")], None));
    models.set_provider(configured("second", vec![model("new")], None));
    models.set_provider(configured("first", vec![model("new")], None));
    models.set_provider(Arc::new(CatalogProvider {
        id: "broken".into(),
        models: Arc::new(Mutex::new(Err(CallbackError::new("catalog broken")))),
        refresh: None,
        auth: ProviderAuth::api_key(Arc::new(Ambient)),
    }));
    let before = js_json!({"providers":models.get_providers().iter().map(|p|p.id()).collect::<Vec<_>>(),
        "models":model_ids(models.get_models(None)),"first":model_ids(models.get_models(Some("first"))),
        "broken":model_ids(models.get_models(Some("broken"))),"missing":model_ids(models.get_models(Some("missing"))),
        "found":models.get_model("first","new").map(|model|model.id),"absent":models.get_model("first","absent")});
    models.delete_provider("first");
    let deleted: Vec<_> = models.get_providers().iter().map(|p| p.id().to_owned()).collect();
    models.clear_providers();
    assert_source(
        "collection",
        js_json!({"before":before,"deleted":deleted,
        "cleared":models.get_providers().iter().map(|p|p.id()).collect::<Vec<_>>()}),
        JsValue::Null,
    );
}

#[tokio::test]
async fn refresh_errors_preserve_raw_causes_existing_models_errors_and_all_settled() {
    let raw = JsValue::String(JsString::from_utf16(vec![0xd800, 100]));
    for cause in [JsValue::Null, JsValue::Bool(false), raw.clone(), js_json!({"raw":raw})] {
        let models = create_models(Default::default());
        let calls = Arc::new(AtomicUsize::new(0));
        models.set_provider(configured("static", vec![model("old")], None));
        let failed: FetchModelsFn = {
            let calls = calls.clone();
            let cause = cause.clone();
            Arc::new(move || {
                calls.fetch_add(1, Ordering::SeqCst);
                let cause = cause.clone();
                Box::pin(async move { Err(CallbackError::from_rejection(AbortReason::Value(cause))) })
            })
        };
        models.set_provider(configured("bad", vec![model("old")], Some(failed)));
        let existing: FetchModelsFn = {
            let calls = calls.clone();
            let cause = cause.clone();
            Arc::new(move || {
                calls.fetch_add(1, Ordering::SeqCst);
                let error = ModelsError::with_cause(
                    ModelsErrorCode::Oauth,
                    "existing error",
                    Some(Box::new(AbortReason::Value(cause.clone()))),
                );
                Box::pin(async move { Err(error.into_callback()) })
            })
        };
        models.set_provider(configured("existing", vec![model("old")], Some(existing)));
        let mut errors = Vec::new();
        for id in ["bad", "existing"] {
            let error = models.refresh(Some(id)).await.unwrap_err();
            errors.push(js_json!({"kind":"error","value":error.into_callback().thrown_value().value()}));
        }
        models.refresh(Some("missing")).await.unwrap();
        models.refresh(Some("static")).await.unwrap();
        models.refresh(None).await.unwrap();
        assert_source(
            "refresh_errors",
            js_json!({"calls":calls.load(Ordering::SeqCst),"errors":errors,
            "models":model_ids(models.get_models(Some("bad")))}),
            cause,
        );
    }
}

#[tokio::test]
async fn configured_provider_shares_inflight_replaces_retains_and_retries() {
    let calls = Arc::new(AtomicUsize::new(0));
    let (started, mut starts) = tokio::sync::mpsc::unbounded_channel();
    let (one, first) = tokio::sync::oneshot::channel();
    let (two, second) = tokio::sync::oneshot::channel();
    let (three, third) = tokio::sync::oneshot::channel();
    let pending = Arc::new(Mutex::new(VecDeque::from([first, second, third])));
    let fetch: FetchModelsFn = {
        let calls = calls.clone();
        Arc::new(move || {
            calls.fetch_add(1, Ordering::SeqCst);
            let reply = pending.lock().unwrap().pop_front().unwrap();
            started.send(()).unwrap();
            Box::pin(async move { reply.await.unwrap() })
        })
    };
    let provider = configured("dynamic", vec![model("old")], Some(fetch));
    let refresh = provider.refresh_models().unwrap();
    let first = refresh();
    let second = refresh();
    starts.recv().await.unwrap();
    let before = js_json!({"calls":calls.load(Ordering::SeqCst),"models":model_ids(provider.get_models().unwrap())});
    one.send(Ok(vec![model("new")])).unwrap();
    let (first, second) = tokio::join!(first, second);
    first.unwrap();
    second.unwrap();
    let after = js_json!({"calls":calls.load(Ordering::SeqCst),"models":model_ids(provider.get_models().unwrap())});
    let cause = JsValue::String(JsString::from_utf16(vec![0xd800, 100]));
    let failed = refresh();
    starts.recv().await.unwrap();
    two.send(Err(CallbackError::from_rejection(AbortReason::Value(cause.clone()))))
        .unwrap();
    let failure = failed.await.unwrap_err();
    let retained = js_json!({"calls":calls.load(Ordering::SeqCst),"models":model_ids(provider.get_models().unwrap())});
    let retried = refresh();
    starts.recv().await.unwrap();
    three.send(Ok(vec![model("old")])).unwrap();
    retried.await.unwrap();
    assert_source(
        "inflight",
        js_json!({"before":before,"after":after,
        "failure":{"kind":"value","value":failure.thrown_value().value()},"retained":retained,
        "retried":{"calls":calls.load(Ordering::SeqCst),"models":model_ids(provider.get_models().unwrap())}}),
        cause,
    );
}

#[tokio::test]
async fn all_refresh_polls_every_provider_before_waiting_and_settles_failure() {
    let models = create_models(Default::default());
    let (started, mut starts) = tokio::sync::mpsc::unbounded_channel();
    let mut replies = Vec::new();
    for id in ["one", "two"] {
        let (reply, receiver) = tokio::sync::oneshot::channel::<Result<Vec<Model>, CallbackError>>();
        replies.push(reply);
        let receiver = Arc::new(Mutex::new(Some(receiver)));
        let state = Arc::new(Mutex::new(Ok(Vec::new())));
        let refresh: RefreshModelsFn = {
            let state = state.clone();
            let started = started.clone();
            Arc::new(move || {
                started.send(id).unwrap();
                let receiver = receiver.lock().unwrap().take().unwrap();
                let state = state.clone();
                Box::pin(async move {
                    let models = receiver.await.unwrap()?;
                    *state.lock().unwrap() = Ok(models);
                    Ok(())
                })
            })
        };
        models.set_provider(Arc::new(CatalogProvider {
            id: id.into(),
            models: state,
            refresh: Some(refresh),
            auth: ProviderAuth::api_key(Arc::new(Ambient)),
        }));
    }
    let pending = tokio::spawn(models.refresh(None));
    let started = vec![starts.recv().await.unwrap(), starts.recv().await.unwrap()];
    let mut replies = replies.into_iter();
    replies.next().unwrap().send(Ok(vec![model("old")])).unwrap();
    replies
        .next()
        .unwrap()
        .send(Err(CallbackError::from_rejection(AbortReason::Value(JsValue::Bool(
            false,
        )))))
        .unwrap();
    pending.await.unwrap().unwrap();
    assert_source(
        "concurrent_all",
        js_json!({"started":started,
        "one":model_ids(models.get_models(Some("one"))),"two":model_ids(models.get_models(Some("two")))}),
        JsValue::Bool(false),
    );
}

struct FixedAuth {
    result: Option<AuthResult>,
    rejection: Option<CallbackError>,
}
impl ApiKeyAuth for FixedAuth {
    fn name(&self) -> JsString {
        "Fixture".into()
    }
    fn resolve(&self, _: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        let result = self.result.clone();
        let rejection = self.rejection.clone();
        Box::pin(async move {
            match rejection {
                Some(error) => Err(error.into()),
                None => Ok(result),
            }
        })
    }
}
struct CapturingStreams {
    captured: Arc<Mutex<Vec<JsValue>>>,
    response: pi_ai::types::AssistantMessage,
}
impl CapturingStreams {
    fn respond(
        &self,
        method: &str,
        model: Model,
        options: pi_ai::request::StreamOptions,
    ) -> AssistantMessageEventStream {
        let value = pi_js_compat::js_json::to_value(&model).unwrap();
        self.captured
            .lock()
            .unwrap()
            .push(js_json!({"method":method,"model":value,
            "serializedModel":pi_js_compat::js_json::stringify_lossless(&value),"apiKey":options.request.api_key,
            "headers":options.request.headers,"env":options.request.env,"temperature":options.temperature}));
        let (sender, stream) = pi_ai::utils::event_stream::create_assistant_message_event_stream();
        sender.push(pi_ai::types::AssistantMessageEvent::Done {
            reason: pi_ai::types::DoneReason::Stop,
            message: self.response.clone(),
        });
        sender.end(Some(self.response.clone()));
        stream
    }
}
impl ProviderStreams for CapturingStreams {
    fn stream(
        &self,
        model: Model,
        _: Context,
        options: pi_ai::request::StreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        Ok(self.respond("stream", model, options))
    }
    fn stream_simple(
        &self,
        model: Model,
        _: Context,
        options: pi_ai::request::SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        Ok(self.respond("simple", model, options.stream))
    }
}
struct StreamCase {
    name: &'static str,
    auth: Option<AuthResult>,
    options: pi_ai::request::StreamOptions,
    rejection: Option<CallbackError>,
    missing_provider: bool,
    missing_api: bool,
}
impl StreamCase {
    fn new(name: &'static str, auth: Option<AuthResult>) -> Self {
        Self {
            name,
            auth,
            options: Default::default(),
            rejection: None,
            missing_provider: false,
            missing_api: false,
        }
    }
    fn wire(&self, model: &Model) -> JsValue {
        let mut value = js_json!({"name":self.name,"model":model,"missingProvider":self.missing_provider,"missingApi":self.missing_api,
            "options":{"apiKey":self.options.request.api_key,"headers":self.options.request.headers,
                "env":self.options.request.env,"temperature":self.options.temperature}});
        // Absent options stay absent, not invalid null input values.
        let JsValue::Object(options) = &mut value["options"] else {
            unreachable!()
        };
        options.retain(|_, value| !matches!(value, JsValue::Null));
        let JsValue::Object(object) = &mut value else {
            unreachable!()
        };
        if let Some(auth) = &self.auth {
            object.insert("auth".into(), pi_js_compat::js_json::to_value(auth).unwrap());
        }
        if let Some(error) = &self.rejection {
            object.insert("rejection".into(), error.thrown_value().value());
        }
        value
    }
}
#[tokio::test]
async fn auth_stream_variants_literal_merges_and_lazy_errors_match_source() {
    use indexmap::IndexMap;
    let mut model = model("old");
    model.name = JsString::from_utf16(vec![79, 0xd800]);
    model.headers = Some(IndexMap::from([("model-only".into(), "keep on model".into())]));
    let response = pi_ai::types::AssistantMessage::empty(&model.api, &model.provider, &model.id, 1.0);
    let mut merged = StreamCase::new(
        "literal auth/options merge",
        Some(AuthResult {
            auth: ModelAuth {
                api_key: Some("resolved".into()),
                base_url: Some("http://auth.invalid".into()),
                headers: Some(IndexMap::from([
                    ("Authorization".into(), Some("auth".into())),
                    ("x-shared".into(), Some("auth".into())),
                    ("deleted".into(), Some("auth".into())),
                ])),
            },
            env: Some(IndexMap::from([
                ("AUTH_ONLY".into(), "auth".into()),
                ("SHARED".into(), "auth".into()),
            ])),
            source: Some("fixture".into()),
        }),
    );
    merged.options.request.api_key = Some("explicit".into());
    merged.options.request.headers = Some(IndexMap::from([
        ("authorization".into(), Some("explicit".into())),
        ("x-shared".into(), Some(JsString::from_utf16(vec![82, 0xd800]))),
        ("deleted".into(), None),
    ]));
    merged.options.request.env = Some(IndexMap::from([
        ("SHARED".into(), "request".into()),
        ("REQUEST_ONLY".into(), "request".into()),
    ]));
    merged.options.temperature = Some(0.0);
    let mut empty = StreamCase::new(
        "empty key/base/maps",
        Some(AuthResult {
            auth: ModelAuth {
                api_key: Some("resolved".into()),
                base_url: Some("".into()),
                headers: Some(IndexMap::new()),
            },
            env: Some(IndexMap::new()),
            source: None,
        }),
    );
    empty.options.request.api_key = Some("".into());
    let mut missing = StreamCase::new("unknown provider", None);
    missing.missing_provider = true;
    let mut no_api = StreamCase::new(
        "missing API",
        Some(AuthResult {
            auth: Default::default(),
            env: None,
            source: None,
        }),
    );
    no_api.missing_api = true;
    let mut rejected = StreamCase::new("raw false rejection", None);
    rejected.rejection = Some(CallbackError::from_rejection(AbortReason::Value(JsValue::Bool(false))));
    let cases = vec![
        StreamCase::new("unconfigured still dispatches", None),
        merged,
        empty,
        missing,
        no_api,
        rejected,
    ];
    let source_cases: Vec<_> = cases.iter().map(|case| case.wire(&model)).collect();
    let mut source = oracle::TsOracle::spawn("models-contract-oracle.ts", "packages/ai/src/models.ts");
    let expected = source.request(&js_json!({"id":1,"operation":"streams","cases":source_cases,"response":response}));
    assert_eq!(expected["ok"], JsValue::Bool(true), "{expected:?}");
    let mut actual = Vec::new();
    for case in cases {
        let models = create_models(Default::default());
        let captured = Arc::new(Mutex::new(Vec::new()));
        if !case.missing_provider {
            let api = if case.missing_api {
                ProviderApi::Map(IndexMap::new())
            } else {
                ProviderApi::Single(Arc::new(CapturingStreams {
                    captured: captured.clone(),
                    response: response.clone(),
                }))
            };
            models.set_provider(create_provider(CreateProviderOptions {
                id: "dynamic".into(),
                name: Some("metadata name".into()),
                base_url: Some("http://metadata.invalid".into()),
                headers: Some(IndexMap::from([("metadata-only".into(), Some("not injected".into()))])),
                auth: ProviderAuth::api_key(Arc::new(FixedAuth {
                    result: case.auth,
                    rejection: case.rejection,
                })),
                models: vec![model.clone()],
                refresh_models: None,
                api,
            }));
        }
        let auth = match models.get_auth(&model).await {
            Ok(value) => js_json!({"ok":true,"value":value}),
            Err(error) => {
                js_json!({"ok":false,"error":{"kind":"error","value":error.into_callback().thrown_value().value()}})
            }
        };
        let context = Context {
            system_prompt: None,
            messages: vec![],
            tools: None,
        };
        let before = pi_ai::request::now_millis() as f64;
        let messages = vec![
            models
                .stream(model.clone(), context.clone(), case.options.clone())
                .result()
                .await
                .unwrap(),
            models
                .complete(model.clone(), context.clone(), case.options.clone())
                .await
                .unwrap(),
            models
                .stream_simple(
                    model.clone(),
                    context.clone(),
                    pi_ai::request::SimpleStreamOptions {
                        stream: case.options.clone(),
                        ..Default::default()
                    },
                )
                .result()
                .await
                .unwrap(),
            models
                .complete_simple(
                    model.clone(),
                    context,
                    pi_ai::request::SimpleStreamOptions {
                        stream: case.options,
                        ..Default::default()
                    },
                )
                .await
                .unwrap(),
        ];
        let after = pi_ai::request::now_millis() as f64;
        let messages: Vec<_> = messages
            .into_iter()
            .map(|mut message| {
                if message.stop_reason == pi_ai::types::StopReason::Error {
                    assert!(message.timestamp >= before && message.timestamp <= after);
                } else {
                    assert_eq!(message.timestamp, 1.0);
                }
                message.timestamp = 0.0;
                message
            })
            .collect();
        actual.push(
            js_json!({"name":case.name,"auth":auth,"captured":captured.lock().unwrap().clone(),"messages":messages}),
        );
    }
    assert_eq!(JsValue::Array(actual), expected["output"]);
}

#[tokio::test]
async fn refresh_does_not_mistake_a_nested_models_error_for_the_thrown_error() {
    #[derive(Debug)]
    struct Outer(ModelsError);
    impl std::fmt::Display for Outer {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("outer error")
        }
    }
    impl std::error::Error for Outer {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            Some(&self.0)
        }
    }
    let inner = ModelsError::with_cause(
        ModelsErrorCode::Oauth,
        "inner error",
        Some(Box::new(AbortReason::Value(JsValue::Bool(false)))),
    );
    let outer = pi_js_compat::node_error::NativeNodeError::new("Error", "outer error")
        .with_cause(inner.clone().into_callback().thrown_value());
    let callback = CallbackError::from(outer).with_original(Arc::new(Outer(inner)));
    let models = create_models(Default::default());
    models.set_provider(configured(
        "dynamic",
        vec![model("old")],
        Some(Arc::new(move || {
            let error = callback.clone();
            Box::pin(async move { Err(error) })
        })),
    ));
    let error = models.refresh(Some("dynamic")).await.unwrap_err();
    assert_source(
        "wrapped_error",
        js_json!({"kind":"error","value":error.into_callback().thrown_value().value()}),
        JsValue::Null,
    );
}
