#[path = "support/custom_anthropic_cases.rs"]
mod cases;
#[path = "support/custom_anthropic_lifecycle.rs"]
mod lifecycle;
use crate::shared_oracle as oracle;
#[path = "support/custom_anthropic_wire.rs"]
mod wire;

#[path = "../examples/extensions/custom_provider_anthropic.rs"]
mod custom_provider_anthropic;
use pi_ai::auth::oauth::support::{OAuthRuntime, RandomSource};
use pi_ai::auth::types::*;
use pi_ai::oauth::{OAuthAuthInfo, OAuthDeviceCodeInfo, OAuthLoginCallbacks, OAuthPrompt, OAuthSelectPrompt};
use pi_ai::request::{BoxFuture, CallbackError, Fetch, HttpBody, HttpRequest, HttpResponse};
use pi_ai::types::AssistantMessageEvent;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::event_bus::EventBusController;
use pi_coding_agent::core::extensions::{
    ExtensionFactory, ExtensionRegistry, create_extension_runtime, load_extension_from_factory,
};
use pi_coding_agent::core::model_registry::{ModelRegistry, ProviderConfigInput, ProviderModelInput};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::{js_json, js_json::JsString};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

struct Random;
impl RandomSource for Random {
    fn fill(&self, bytes: &mut [u8]) -> Result<(), CallbackError> {
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = i as u8;
        }
        Ok(())
    }
    fn uuid(&self) -> Result<String, CallbackError> {
        panic!("not a UUID flow")
    }
}
struct Body(Option<Vec<u8>>);
impl HttpBody for Body {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async { self.0.take().map(Ok) })
    }
}
struct StreamBody {
    bytes: Option<Vec<u8>>,
    failure: Option<String>,
    abort: bool,
    signal: Option<AbortSignal>,
    controller: pi_js_compat::abort::AbortController,
}
impl HttpBody for StreamBody {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async move {
            if let Some(bytes) = self.bytes.take() {
                return Some(Ok(bytes));
            }
            if self.abort && self.signal.is_some() {
                self.controller.abort(None);
            }
            self.failure.take().map(|error| Err(CallbackError::new(error)))
        })
    }
}
struct Transport {
    input: Value,
    calls: Arc<Mutex<Vec<Value>>>,
    controller: pi_js_compat::abort::AbortController,
}
impl Fetch for Transport {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(json!({"type":"fetch", "url":request.url, "method":request.method, "headers":request.headers.into_iter().map(|(k,v)|(k,Value::String(v))).collect::<serde_json::Map<String, Value>>(), "body":serde_json::from_slice::<Value>(&request.body.unwrap()).unwrap(), "signal":signal.is_some()}));
            if let Some(error) = self.input["fetchError"].as_str() {
                return Err(CallbackError::new(error));
            }
            let stream_events = self.input.get("events").and_then(Value::as_array);
            if let Some(events) = stream_events {
                let body = events
                    .iter()
                    .map(|event| format!("event: {}\ndata: {}\n\n", event["type"].as_str().unwrap(), event))
                    .collect::<String>();
                return Ok(HttpResponse {
                    status: 200,
                    status_text: "OK".into(),
                    headers: vec![],
                    body: Box::new(StreamBody {
                        bytes: Some(body.into_bytes()),
                        controller: self.controller.clone(),
                        failure: self.input.get("streamError").map(|value| {
                            if self.input["thrownValue"] == true {
                                value.to_string()
                            } else {
                                value.as_str().unwrap().into()
                            }
                        }),
                        abort: self.input["abort"] == true,
                        signal,
                    }),
                });
            }
            let body = self
                .input
                .get("body")
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    self.input
                        .get("token")
                        .cloned()
                        .unwrap_or(
                            json!({"access_token":"FAKE-access", "refresh_token":"FAKE-refresh", "expires_in":3600}),
                        )
                        .to_string()
                });
            Ok(HttpResponse {
                status: self.input["status"].as_u64().unwrap_or(200) as u16,
                status_text: "".into(),
                headers: vec![],
                body: Box::new(Body(Some(body.into_bytes()))),
            })
        })
    }
}
struct Interaction {
    input: Value,
    calls: Arc<Mutex<Vec<Value>>>,
}
impl OAuthLoginCallbacks for Interaction {
    fn on_auth(&self, info: OAuthAuthInfo) -> Result<(), CallbackError> {
        assert!(info.instructions.is_none());
        self.calls.lock().unwrap().push(json!({"type":"auth", "url":info.url}));
        Ok(())
    }
    fn on_device_code(&self, _: OAuthDeviceCodeInfo) -> Result<(), CallbackError> {
        panic!("unexpected device code");
    }
    fn on_select(&self, _: OAuthSelectPrompt) -> BoxFuture<'_, Result<Option<JsString>, CallbackError>> {
        panic!("unexpected selection");
    }
    fn on_prompt(&self, prompt: OAuthPrompt) -> BoxFuture<'_, Result<JsString, CallbackError>> {
        Box::pin(async move {
            assert!(prompt.placeholder.is_none());
            self.calls
                .lock()
                .unwrap()
                .push(json!({"type":"prompt", "message":prompt.message}));
            if let Some(error) = self.input["promptError"].as_str() {
                return Err(CallbackError::new(error));
            }
            Ok(self.input["code"]
                .as_str()
                .unwrap_or("FAKE-code#FAKE-state#ignored")
                .into())
        })
    }
}
fn stream_fixture(
    config: &ProviderConfigInput,
    model: ProviderModelInput,
    context: pi_ai::types::Context,
    options: pi_ai::request::SimpleStreamOptions,
) -> pi_ai::utils::event_stream::AssistantMessageEventStream {
    // Stream callbacks consume output Models, not registration definitions.
    // Use the public reconstruction path without unrelated API/OAuth callbacks.
    let id = model.id.clone();
    let registry = ModelRegistry::in_memory(AuthStorage::in_memory(Vec::<(String, Value)>::new())).unwrap();
    registry
        .register_provider(
            "custom-anthropic",
            ProviderConfigInput {
                api: config.api.clone(),
                base_url: config.base_url.clone(),
                api_key: Some("fixture-model-key".into()),
                models: Some(vec![model]),
                ..Default::default()
            },
        )
        .unwrap();
    let model = registry.find("custom-anthropic", id).unwrap();
    (config.stream_simple.as_ref().unwrap())(model, context, Some(options)).expect("acquisition")
}
async fn provider(
    input: &Value,
    calls: Arc<Mutex<Vec<Value>>>,
) -> (ProviderConfigInput, pi_js_compat::abort::AbortController) {
    let controller = pi_js_compat::abort::AbortController::new();
    let transport = Arc::new(Transport {
        input: input.clone(),
        calls,
        controller: controller.clone(),
    });
    let runtime = OAuthRuntime::new(transport, Arc::new(|| 1700000000000.0), Arc::new(Random));
    let factory: ExtensionFactory = Arc::new(move |api| {
        let result = custom_provider_anthropic::custom_anthropic_extension_with_runtime(api, runtime.clone());
        Box::pin(async move { result.map_err(Into::into) })
    });
    let mut registry = ExtensionRegistry::new();
    registry.register("<example:custom-anthropic>", factory);
    let extension_runtime = create_extension_runtime();
    let registered: Arc<Mutex<Option<ProviderConfigInput>>> = Arc::new(Mutex::new(None));
    let capture = registered.clone();
    extension_runtime.set_provider_handlers(pi_coding_agent::core::extensions::extension_runtime::ProviderHandlers {
        register_provider: Arc::new(move |name, config, path| {
            assert_eq!(name, "custom-anthropic");
            assert_eq!(path, Some(&JsString::from("<example:custom-anthropic>")));
            *capture.lock().unwrap() = Some(config.clone());
            Ok(())
        }),
        unregister_provider: Arc::new(|_, _| panic!("not an unregister flow")),
    });
    load_extension_from_factory(
        registry
            .get(&JsString::from("<example:custom-anthropic>"))
            .unwrap()
            .clone(),
        &JsString::from("/tmp"),
        Arc::new(EventBusController::new()),
        extension_runtime.clone(),
        &JsString::from("<example:custom-anthropic>"),
    )
    .await
    .unwrap();
    assert!(extension_runtime.pending_provider_registrations().is_empty());
    let provider = registered
        .lock()
        .unwrap()
        .clone()
        .expect("factory must register one provider config through runtime handler");
    (provider, controller)
}

#[tokio::test]
async fn default_factory_registers_exact_catalog_without_network() {
    let runtime = create_extension_runtime();
    let factory: ExtensionFactory = Arc::new(|api| {
        let result = custom_provider_anthropic::custom_anthropic_extension(api);
        Box::pin(async move { result.map_err(Into::into) })
    });
    load_extension_from_factory(
        factory,
        &JsString::from("/tmp"),
        Arc::new(EventBusController::new()),
        runtime.clone(),
        &JsString::from("<example:custom-anthropic>"),
    )
    .await
    .unwrap();
    let pending = runtime.pending_provider_registrations();
    assert_eq!(pending.len(), 1);
    let provider = &pending[0].config;
    assert_eq!(pending[0].name, "custom-anthropic");
    let mut oracle = oracle::TsOracle::spawn(
        "example-custom-anthropic-oracle.ts",
        "packages/coding-agent/examples/extensions/custom-provider-anthropic/index.ts",
    );
    let expected = oracle.request(&js_json!({"kind":"metadata"}));
    assert_eq!(
        js_json!({"id":pending[0].name,"baseUrl":provider.base_url,"apiKey":provider.api_key,"api":provider.api,"models":provider.models,"oauthName":provider.oauth.as_ref().unwrap().name}),
        expected["metadata"]
    );
}

#[tokio::test]
async fn native_factory_oauth_matches_pinned_login_refresh_and_failures() {
    let mut oracle = oracle::TsOracle::spawn(
        "example-custom-anthropic-oracle.ts",
        "packages/coding-agent/examples/extensions/custom-provider-anthropic/index.ts",
    );
    for input in [
        json!({"kind":"login"}),
        json!({"kind":"login","code":"FAKE-code"}),
        json!({"kind":"login","code":"#"}),
        json!({"kind":"login","status":400,"body":"denied"}),
        json!({"kind":"login","fetchError":"offline"}),
        json!({"kind":"login","promptError":"cancelled"}),
        json!({"kind":"refresh","credential":{"access":"FAKE-old","refresh":"FAKE-refresh","expires":0}}),
        json!({"kind":"refresh","credential":{"access":"FAKE-old","refresh":"FAKE-refresh","expires":0},"status":401,"body":"expired"}),
    ] {
        let expected = oracle.request(&input);
        let calls = Arc::new(Mutex::new(vec![]));
        let (provider, _) = provider(&input, calls.clone()).await;
        let oauth = provider.oauth.as_ref().unwrap();
        let result = if input["kind"] == "login" {
            (oauth.login)(Arc::new(Interaction {
                input: input.clone(),
                calls: calls.clone(),
            }))
            .await
        } else {
            (oauth.refresh_token)(
                OAuthCredential::new("FAKE-refresh", "FAKE-old", 0.0),
                AbortSignal::default(),
            )
            .await
        };
        let actual = match result {
            Ok(credential) => {
                json!({"calls":calls.lock().unwrap().clone(), "credential":{"access":credential.access.to_utf8().unwrap(),"refresh":credential.refresh.to_utf8().unwrap(),"expires":pi_js_compat::number::js_number_value(credential.expires)},"auth":(oauth.get_api_key)(&credential).unwrap()})
            }
            Err(error) => json!({"calls":calls.lock().unwrap().clone(), "error":error.to_string()}),
        };
        assert_eq!(actual, expected, "{input}");
    }
}

#[tokio::test]
async fn native_provider_stream_reaches_controlled_http_with_exact_example_request() {
    let input = json!({"events":[{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{}}], "context":{"messages":[{"role":"user","content":"hello 😀","timestamp":0}],"systemPrompt":"fixture"},"options":{"apiKey":"FAKE-key","reasoning":"high"}});
    let mut oracle = oracle::TsOracle::spawn(
        "example-custom-anthropic-oracle.ts",
        "packages/coding-agent/examples/extensions/custom-provider-anthropic/index.ts",
    );
    let expected = oracle.request(&input);
    let calls = Arc::new(Mutex::new(vec![]));
    let (provider, _) = provider(&input, calls.clone()).await;
    let mut options = pi_ai::request::SimpleStreamOptions::default();
    options.stream.request.api_key = Some("FAKE-key".into());
    options.stream.request.signal = Some(AbortSignal::default());
    options.reasoning = Some(pi_ai::types::ThinkingLevel::High);
    let mut stream = stream_fixture(
        &provider,
        provider.models.as_ref().unwrap()[0].clone(),
        serde_json::from_value(input["context"].clone()).unwrap(),
        options,
    );
    let mut events = vec![];
    while let Some(event) = stream.next().await {
        events.push(serde_json::to_value(event.unwrap()).unwrap());
    }
    let calls = calls.lock().unwrap();
    assert_eq!(
        calls.len(),
        1,
        "custom Provider must execute controlled HTTP instead of an inert error stream"
    );
    assert_eq!(calls[0]["body"], expected["calls"][1]["params"]);
    assert_eq!(events, expected["events"].as_array().unwrap().clone());
    assert_eq!(calls[0]["url"], "https://api.anthropic.com/v1/messages");
    let headers = calls[0]["headers"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.to_lowercase(), v.clone()))
        .collect::<serde_json::Map<String, Value>>();
    assert_eq!(headers["x-api-key"], "FAKE-key");
    assert_eq!(headers["x-stainless-helper-method"], "stream");
    for (key, value) in expected["calls"][0]["options"]["defaultHeaders"].as_object().unwrap() {
        assert_eq!(headers[key], *value);
    }
}

#[test]
fn pinned_example_valid_text_start_survives_native_event_boundary() {
    let mut oracle = oracle::TsOracle::spawn(
        "example-custom-anthropic-oracle.ts",
        "packages/coding-agent/examples/extensions/custom-provider-anthropic/index.ts",
    );
    let expected = oracle.request(&json!({"events":[
        {"type":"content_block_start","index":3,"content_block":{"type":"text","text":"ignored"}},
        {"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{}}
    ]}));
    let event = expected["events"][1].clone();
    assert_eq!(event["type"], "text_start", "{expected}");
    let native: AssistantMessageEvent = serde_json::from_value(event.clone()).unwrap();
    assert_eq!(
        serde_json::to_value(native).unwrap(),
        event,
        "valid source event must survive the Provider trait's typed output boundary unchanged"
    );
}

// Shared support is compiled once per integration target.
#[path = "support/oracle.rs"]
pub mod shared_oracle;
