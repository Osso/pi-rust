use crate::shared_oracle as oracle;

use pi_ai::{
    compat,
    request::SimpleStreamOptions,
    types::{
        AssistantContent, AssistantMessage, AssistantMessageEvent, Context, DoneReason, Model, ProviderHeaders,
        TextContent, ThinkingLevel,
    },
    utils::event_stream::create_assistant_message_event_stream,
};
use pi_coding_agent::core::{
    auth_storage::{AuthStatus, AuthStorage},
    model_registry::{
        ExtensionOAuthConfig, ModelRegistry, ProviderConfigInput, ProviderModelInput, ResolvedRequestAuth,
        StreamSimpleFn,
    },
};
use pi_js_compat::{
    js_json,
    js_json::{JsString, JsValue},
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[derive(Default)]
struct NoFetch(AtomicUsize);
impl pi_ai::request::Fetch for NoFetch {
    fn fetch(
        &self,
        _: pi_ai::request::HttpRequest,
        _: Option<pi_js_compat::abort::AbortSignal>,
    ) -> pi_ai::request::BoxFuture<'_, Result<pi_ai::request::HttpResponse, pi_ai::request::CallbackError>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Box::pin(async { Err(pi_ai::request::CallbackError::new("Fixture must not issue HTTP")) })
    }
}

const PROVIDER: &str = "registry-dispatch-lifecycle";
const OUTSIDE: &str = "registry-dispatch-outside";

fn oauth(name: &str) -> ExtensionOAuthConfig {
    ExtensionOAuthConfig {
        name: name.into(),
        uses_callback_server: None,
        login: Arc::new(|_| unreachable!("unused login")),
        refresh_token: Arc::new(|_, _| unreachable!("unused refresh")),
        get_api_key: Arc::new(|_| unreachable!("unused OAuth key")),
        modify_models: None,
    }
}
fn callback(label: &str, calls: Arc<Mutex<Vec<JsValue>>>) -> StreamSimpleFn {
    let label = label.to_owned();
    Arc::new(move |model, context, options| {
        let options = options.expect("dispatch options");
        calls.lock().unwrap().push(js_json!({"label":label,"model":model,"context":context,"options":{
            "apiKey":options.stream.request.api_key,"temperature":options.stream.temperature,"maxTokens":options.stream.max_tokens,
            "headers":options.stream.request.headers,"reasoning":options.reasoning
        }}));
        let mut message = AssistantMessage::empty(model.api, model.provider, model.id, 0.0);
        message.content = vec![AssistantContent::Text(TextContent::new(label.clone()))];
        let (sender, stream) = create_assistant_message_event_stream();
        sender.push(AssistantMessageEvent::Start {
            partial: message.clone(),
        });
        sender.push(AssistantMessageEvent::Done {
            reason: DoneReason::Stop,
            message: message.clone(),
        });
        sender.end(Some(message));
        Ok(stream)
    })
}
fn headers(name: &str, value: JsString) -> ProviderHeaders {
    [(name.to_owned(), Some(value))].into_iter().collect()
}
fn outside_present(registry: &ModelRegistry) -> JsValue {
    js_json!({"api":compat::get_api_provider("registry-outside-api").is_some(),"oauth":registry.auth_storage().get_oauth_providers().iter().any(|item|item.id()==OUTSIDE)})
}
fn status_value(status: AuthStatus) -> JsValue {
    let mut value = js_json!({"configured":status.configured});
    if let Some(source) = status.source {
        value["source"] = js_json::to_value(&source).unwrap();
    }
    if let Some(label) = status.label {
        value["label"] = js_json::to_value(&label).unwrap();
    }
    value
}
fn auth_value(auth: ResolvedRequestAuth) -> JsValue {
    match auth {
        ResolvedRequestAuth::Err { error } => js_json!({"ok":false,"error":error}),
        ResolvedRequestAuth::Ok { api_key, headers, env } => {
            let mut value = js_json!({"ok":true});
            if let Some(key) = api_key {
                value["apiKey"] = js_json::to_value(&key).unwrap();
            }
            if let Some(headers) = headers {
                value["headers"] = js_json::to_value(&headers).unwrap();
            }
            if let Some(env) = env {
                value["env"] = js_json::to_value(&env).unwrap();
            }
            value
        }
    }
}
async fn snapshot(registry: &ModelRegistry, model: &Model, phase: &str) -> JsValue {
    let oauth_name = registry
        .auth_storage()
        .get_oauth_providers()
        .iter()
        .find(|item| item.id() == PROVIDER)
        .map(|item| item.name().to_owned());
    js_json!({"phase":phase,"models":registry.get_all().into_iter().filter(|item|item.provider==PROVIDER).collect::<Vec<_>>(),
        "status":status_value(registry.get_provider_auth_status(PROVIDER)),"display":registry.get_provider_display_name(PROVIDER),
        "key":registry.get_api_key_for_provider(PROVIDER).await.unwrap(),"requestAuth":auth_value(registry.get_api_key_and_headers(model).await.unwrap()),
        "configured":registry.has_configured_auth(model).unwrap(),"outside":outside_present(registry),"oauthName":oauth_name})
}
fn context() -> Context {
    js_json::from_value(&js_json!({"systemPrompt":"","messages":[{"role":"user","content":JsString::from("request-")+JsString::from_utf16(vec![0xd800]),"timestamp":1}]})).unwrap()
}
async fn dispatch(model: &Model, simple: bool) -> JsValue {
    let mut options = SimpleStreamOptions::default();
    options.stream.request.api_key = Some("wire-key".into());
    options.stream.request.headers = Some(headers(
        "X-Call",
        JsString::from("caller-") + JsString::from_utf16(vec![0xd800]),
    ));
    options.stream.temperature = Some(0.5);
    options.stream.max_tokens = Some(7.0);
    let mut stream = if simple {
        options.reasoning = Some(ThinkingLevel::High);
        compat::stream_simple(model.clone(), context(), options)
    } else {
        compat::stream(model.clone(), context(), options.stream)
    }
    .expect("acquisition");
    let mut events = Vec::new();
    while let Some(event) = stream.next().await {
        events.push(event.unwrap());
    }
    js_json!({"events":events,"result":stream.result().await.unwrap()})
}
async fn native_lifecycle() -> JsValue {
    let registry = ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::new())).unwrap();
    let outside = ModelRegistry::in_memory(AuthStorage::in_memory(serde_json::Map::new())).unwrap();
    let calls = Arc::new(Mutex::new(Vec::new()));
    outside
        .register_provider(
            OUTSIDE,
            ProviderConfigInput {
                api: Some("registry-outside-api".into()),
                stream_simple: Some(callback("outside", calls.clone())),
                oauth: Some(oauth("Outside")),
                ..Default::default()
            },
        )
        .unwrap();
    registry.unregister_provider("never-registered").unwrap();
    let unknown_noop = outside_present(&registry);
    let definition: ProviderModelInput = js_json::from_value(&js_json!({"id":"one","name":"One","reasoning":false,"input":["text"],"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},"contextWindow":128,"maxTokens":32})).unwrap();
    registry
        .register_provider(
            PROVIDER,
            ProviderConfigInput {
                api: Some("openai-responses".into()),
                base_url: Some("https://fixture.invalid".into()),
                api_key: Some("config-key".into()),
                headers: Some([("X-Phase".into(), "first".into())].into_iter().collect()),
                models: Some(vec![definition]),
                stream_simple: Some(callback("first", calls.clone())),
                oauth: Some(oauth("OAuth First")),
                ..Default::default()
            },
        )
        .unwrap();
    let model = registry.find(PROVIDER, "one").unwrap();
    let mut states = vec![snapshot(&registry, &model, "registered").await];
    let mut dispatches = vec![dispatch(&model, false).await, dispatch(&model, true).await];
    registry
        .register_provider(
            PROVIDER,
            ProviderConfigInput {
                api: Some("openai-responses".into()),
                stream_simple: Some(callback("second", calls.clone())),
                headers: Some([("X-Phase".into(), "second".into())].into_iter().collect()),
                ..Default::default()
            },
        )
        .unwrap();
    states.push(snapshot(&registry, &model, "partial-upsert").await);
    dispatches.push(dispatch(&model, false).await);
    dispatches.push(dispatch(&model, true).await);
    registry
        .register_provider(
            PROVIDER,
            ProviderConfigInput {
                oauth: Some(oauth("OAuth Second")),
                ..Default::default()
            },
        )
        .unwrap();
    states.push(snapshot(&registry, &model, "oauth-upsert").await);
    registry.refresh().unwrap();
    states.push(snapshot(&registry, &model, "refreshed").await);
    dispatches.push(dispatch(&model, false).await);
    dispatches.push(dispatch(&model, true).await);
    registry.unregister_provider(PROVIDER).unwrap();
    states.push(snapshot(&registry, &model, "unregistered").await);
    let fetch = Arc::new(NoFetch::default());
    let mut options = SimpleStreamOptions::default();
    options.stream.request.env = Some(Default::default());
    options.stream.request.fetch = Some(fetch.clone());
    let restored = compat::stream_simple(model.clone(), context(), options)
        .expect("restored acquisition")
        .result()
        .await
        .unwrap();
    let restored_summary = js_json!({"stopReason":restored.stop_reason,"errorMessage":restored.error_message});
    registry.unregister_provider(PROVIDER).unwrap();
    states.push(snapshot(&registry, &model, "unregistered-again").await);
    js_json!({"unknownNoop":unknown_noop,"states":states,"dispatches":dispatches,"calls":*calls.lock().unwrap(),"restoredSummary":restored_summary,"fetchCount":fetch.0.load(Ordering::SeqCst)})
}

#[tokio::test]
async fn raw_simple_dispatch_and_upsert_refresh_unregister_match_frozen() {
    let mut source = oracle::TsOracle::spawn(
        "model-registry-lifecycle-oracle.mts",
        "packages/coding-agent/src/core/model-registry.ts",
    );
    let response = source.request(&js_json!({}));
    assert_eq!(response["ok"], JsValue::Bool(true), "{response:?}");
    let expected = &response["output"];
    assert_eq!(expected["unknownNoop"], js_json!({"api":true,"oauth":true}));
    assert_eq!(expected["states"][3]["outside"], js_json!({"api":false,"oauth":false}));
    assert_eq!(expected["states"][1]["key"], JsValue::Null);
    assert_eq!(expected["states"][3]["key"], JsValue::String("config-key".into()));
    assert_eq!(expected["fetchCount"], js_json!(0));
    assert_eq!(expected["calls"].as_array().unwrap().len(), 6);
    assert_eq!(native_lifecycle().await, *expected);
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
