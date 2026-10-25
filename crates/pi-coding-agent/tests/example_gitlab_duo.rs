#[path = "../examples/extensions/custom_provider_gitlab_duo.rs"]
mod gitlab;
use crate::shared_extensions as support;
use crate::shared_oracle as oracle;
use pi_ai::request::BoxFuture;
use pi_ai::{
    auth::{
        oauth::support::{OAuthRuntime, RandomSource},
        types::*,
    },
    oauth::{OAuthAuthInfo, OAuthDeviceCodeInfo, OAuthLoginCallbacks, OAuthPrompt, OAuthSelectPrompt},
    request::*,
    types::*,
    utils::event_stream::*,
};
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::extensions::*;
use pi_coding_agent::core::model_registry::{ModelRegistry, ProviderConfigInput, ProviderModelInput};
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::{js_json, js_json::JsString};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
struct Random;
impl RandomSource for Random {
    fn fill(&self, b: &mut [u8]) -> Result<(), CallbackError> {
        for (i, x) in b.iter_mut().enumerate() {
            *x = i as u8;
        }
        Ok(())
    }
    fn uuid(&self) -> Result<String, CallbackError> {
        Ok("00000000-0000-4000-8000-000000000000".into())
    }
}
struct Body(Option<Vec<u8>>);
impl HttpBody for Body {
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>> {
        Box::pin(async { self.0.take().map(Ok) })
    }
}
struct FetchFixture {
    calls: Arc<Mutex<Vec<Value>>>,
    input: Value,
}
impl Fetch for FetchFixture {
    fn fetch(&self, r: HttpRequest, signal: Option<AbortSignal>) -> BoxFuture<'_, Result<HttpResponse, CallbackError>> {
        Box::pin(async move {
            self.calls.lock().unwrap().push(json!({"type":"fetch","url":r.url,"method":r.method,"headers":r.headers.into_iter().map(|(k,v)|(k,Value::String(v))).collect::<serde_json::Map<String,Value>>(),"body":String::from_utf8(r.body.unwrap()).unwrap(),"signal":signal.is_some()}));
            if let Some(e) = self.input["fetchError"].as_str() {
                return Err(CallbackError::new(e));
            }
            let body = if r.url.ends_with("/oauth/token") {
                json!({"access_token":"FAKE-access","refresh_token":"FAKE-refresh","created_at":1700000000,"expires_in":3600}).to_string()
            } else {
                json!({"token":"FAKE-direct","headers":{"X-Gitlab":"fixture","Authorization":"replace"}}).to_string()
            };
            Ok(HttpResponse {
                status: self.input["status"].as_u64().unwrap_or(200) as u16,
                status_text: "".into(),
                headers: vec![],
                body: Box::new(Body(Some(
                    self.input["body"].as_str().unwrap_or(&body).as_bytes().to_vec(),
                ))),
            })
        })
    }
}
struct Backend {
    calls: Arc<Mutex<Vec<Value>>>,
    name: &'static str,
    rejection: Option<CallbackError>,
}
fn backend_rejection(input: &Value) -> Option<CallbackError> {
    let kind = input["backendThrow"].as_str()?;
    let units = input["valueUnits"]
        .as_array()
        .expect("fixture UTF16 units")
        .iter()
        .map(|value| u16::try_from(value.as_u64().unwrap()).unwrap())
        .collect();
    let message = JsString::from_utf16(units);
    let reason = match kind {
        "error" => pi_js_compat::node_error::NativeNodeError::from_constructor(
            pi_js_compat::node_error::NativeErrorClass::TypeError,
            message,
        )
        .with_property("code", pi_js_compat::js_json::JsValue::String("E_BACKEND".into()))
        .into(),
        "value" => pi_js_compat::abort::AbortReason::Value(pi_js_compat::js_json::JsValue::String(message)),
        "conversion" => pi_js_compat::abort::AbortReason::Value(js_json!({"toString":null,"valueOf":null})),
        _ => panic!("unknown fixture backendThrow kind"),
    };
    Some(CallbackError::from_rejection(reason))
}
fn backend_event() -> Value {
    json!({"type":"error","reason":"aborted","error":{"role":"assistant","api":"fixture-api","provider":"fixture","model":"fixture","content":[],"usage":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":0,"cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":0}},"stopReason":"aborted","errorMessage":"backend event","timestamp":12}})
}
impl ProviderStreams for Backend {
    fn stream(&self, _: Model, _: Context, _: StreamOptions) -> Result<AssistantMessageEventStream, CallbackError> {
        panic!("simple only")
    }
    fn stream_simple(
        &self,
        m: Model,
        c: Context,
        o: SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        self.calls.lock().unwrap().push(json!({"type":"backend","backend":self.name,"model":m,"context":c,"apiKey":o.stream.request.api_key,"headers":o.stream.request.headers}));
        if let Some(error) = &self.rejection {
            return Err(error.clone());
        }
        let (tx, rx) = create_assistant_message_event_stream();
        tx.push(serde_json::from_value(backend_event()).unwrap());
        tx.end(None);
        Ok(rx)
    }
}
struct Interaction {
    calls: Arc<Mutex<Vec<Value>>>,
    code: String,
}
impl OAuthLoginCallbacks for Interaction {
    fn on_auth(&self, info: OAuthAuthInfo) -> Result<(), CallbackError> {
        self.calls.lock().unwrap().push(json!({"type":"auth","url":info.url}));
        Ok(())
    }
    fn on_device_code(&self, _: OAuthDeviceCodeInfo) -> Result<(), CallbackError> {
        panic!("unexpected device code");
    }
    fn on_select(&self, _: OAuthSelectPrompt) -> BoxFuture<'_, Result<Option<JsString>, CallbackError>> {
        panic!("unexpected selection");
    }
    fn on_prompt(&self, prompt: OAuthPrompt) -> BoxFuture<'_, Result<JsString, CallbackError>> {
        self.calls
            .lock()
            .unwrap()
            .push(json!({"type":"prompt","message":prompt.message}));
        Box::pin(async move { Ok(JsString::from(&self.code)) })
    }
}
fn stream_fixture(
    config: &ProviderConfigInput,
    model: ProviderModelInput,
    context: Context,
    options: SimpleStreamOptions,
) -> AssistantMessageEventStream {
    // Stream callbacks consume output Models, not registration definitions.
    // Use the public reconstruction path without unrelated API/OAuth callbacks.
    let id = model.id.clone();
    let registry = ModelRegistry::in_memory(AuthStorage::in_memory(Vec::<(String, Value)>::new())).unwrap();
    registry
        .register_provider(
            "gitlab-duo",
            ProviderConfigInput {
                api: config.api.clone(),
                base_url: config.base_url.clone(),
                api_key: Some("fixture-model-key".into()),
                models: Some(vec![model]),
                ..Default::default()
            },
        )
        .unwrap();
    let model = registry.find("gitlab-duo", id).unwrap();
    (config.stream_simple.as_ref().unwrap())(model, context, Some(options)).expect("acquisition")
}
async fn provider(input: Value, calls: Arc<Mutex<Vec<Value>>>, clock: Arc<Mutex<f64>>) -> ProviderConfigInput {
    let rejection = backend_rejection(&input);
    let now = clock.clone();
    let runtime = OAuthRuntime::new(
        Arc::new(FetchFixture {
            calls: calls.clone(),
            input,
        }),
        Arc::new(move || *now.lock().unwrap()),
        Arc::new(Random),
    );
    let backends = gitlab::Backends {
        anthropic: Arc::new(Backend {
            calls: calls.clone(),
            name: "anthropic",
            rejection: rejection.clone(),
        }),
        openai: Arc::new(Backend {
            calls,
            name: "openai",
            rejection,
        }),
    };
    let mut registry = ExtensionRegistry::new();
    registry.register(
        "<example:gitlab>",
        Arc::new(move |api| {
            let result = gitlab::gitlab_duo_extension_with_backends(api, runtime.clone(), backends.clone());
            Box::pin(async move { result.map_err(HandlerError::from) })
        }),
    );
    let runtime = support::runtime();
    let extension = load_extension_from_factory(
        registry.get(&JsString::from("<example:gitlab>")).unwrap().clone(),
        &JsString::from("/fixture"),
        support::event_bus(),
        runtime.clone(),
        &JsString::from("<example:gitlab>"),
    )
    .await
    .unwrap();
    let models = support::model_registry().await;
    let config = runtime.pending_provider_registrations()[0].config.clone();
    let mut runner = support::runner_for(vec![extension], "/fixture", runtime, models.clone());
    let target = models.clone();
    runner
        .bind_core(
            support::actions(),
            support::context_actions(),
            Some(ProviderActions {
                register_provider: Some(Arc::new(move |name, config| {
                    target.register_provider(name, config.clone())
                })),
                ..Default::default()
            }),
        )
        .unwrap();
    let first = &config.models.as_ref().unwrap()[0];
    assert!(
        models.find("gitlab-duo", &first.id).is_some(),
        "provider bound through real ExtensionRunner"
    );
    config
}
#[tokio::test]
async fn registration_login_refresh_and_errors_match_pinned_example() {
    let runtime = support::runtime();
    support::extension(&runtime, "/fixture", "<default:gitlab>", |api| {
        gitlab::gitlab_duo_extension(api).unwrap()
    })
    .await;
    assert_eq!(
        runtime.pending_provider_registrations()[0]
            .config
            .models
            .as_ref()
            .unwrap()
            .len(),
        9
    );
    let mut ts = oracle::TsOracle::spawn(
        "example-gitlab-duo-oracle.ts",
        "packages/coding-agent/examples/extensions/custom-provider-gitlab-duo/index.ts",
    );
    for input in [
        json!({"kind":"metadata"}),
        json!({"kind":"login"}),
        json!({"kind":"login","code":"http://localhost/callback?state=ignored"}),
        json!({"kind":"login","status":400,"body":"denied"}),
        json!({"kind":"refresh"}),
        json!({"kind":"refresh","status":401,"body":"expired"}),
    ] {
        let calls = Arc::new(Mutex::new(vec![]));
        let p = provider(input.clone(), calls.clone(), Arc::new(Mutex::new(1700000000000.0))).await;
        let expected = ts.request(&input);
        if input["kind"] == "metadata" {
            assert_eq!(
                js_json!({"id":"gitlab-duo","baseUrl":p.base_url,"apiKey":p.api_key,"api":p.api,"models":p.models,"oauthName":p.oauth.as_ref().unwrap().name}),
                pi_js_compat::js_json::to_value(&expected["metadata"]).unwrap()
            );
            continue;
        }
        let oauth = p.oauth.as_ref().unwrap();
        let result = if input["kind"] == "login" {
            (oauth.login)(Arc::new(Interaction {
                calls: calls.clone(),
                code: input["code"]
                    .as_str()
                    .unwrap_or("http://localhost/callback?code=FAKE-code&state=ignored")
                    .into(),
            }))
            .await
        } else {
            (oauth.refresh_token)(
                OAuthCredential::new("old-refresh", "old-access", 0.0),
                AbortSignal::default(),
            )
            .await
        };
        let actual = match result {
            Ok(c) => {
                let access = (oauth.get_api_key)(&c).unwrap();
                json!({"credential":{"access":c.access.to_utf8().unwrap(),"refresh":c.refresh.to_utf8().unwrap(),"expires":pi_js_compat::number::js_number_value(c.expires)},"auth":access,"calls":*calls.lock().unwrap()})
            }
            Err(e) => json!({"error":e.to_string(),"calls":*calls.lock().unwrap()}),
        };
        assert_eq!(actual, expected, "{input}");
    }
}
#[tokio::test]
async fn backend_dispatch_errors_cache_expiry_and_oauth_invalidation() {
    let mut ts = oracle::TsOracle::spawn(
        "example-gitlab-duo-oracle.ts",
        "packages/coding-agent/examples/extensions/custom-provider-gitlab-duo/index.ts",
    );
    for input in [
        json!({"kind":"streams","indices":[0,5,8]}),
        json!({"kind":"streams","indices":[0],"missingKey":true}),
        json!({"kind":"streams","indices":[0],"unknown":true}),
        json!({"kind":"streams","indices":[0],"status":403,"body":"no duo"}),
        json!({"kind":"streams","indices":[0],"status":500,"body":"failed"}),
        json!({"kind":"streams","indices":[0],"fetchError":"offline"}),
    ] {
        let calls = Arc::new(Mutex::new(vec![]));
        let p = provider(input.clone(), calls.clone(), Arc::new(Mutex::new(1700000000000.0))).await;
        let mut events = vec![];
        for index in input["indices"].as_array().unwrap() {
            let mut model = p.models.as_ref().unwrap()[index.as_u64().unwrap() as usize].clone();
            if input["unknown"] == true {
                model.id = "unknown".into();
            }
            let mut o = SimpleStreamOptions::default();
            if input["missingKey"] != true {
                o.stream.request.api_key = Some("FAKE-gitlab".into());
            }
            o.stream.request.headers = Some(
                [(String::from("ignored"), Some(JsString::from("caller")))]
                    .into_iter()
                    .collect(),
            );
            let mut stream = stream_fixture(&p, model, serde_json::from_value(json!({"messages":[]})).unwrap(), o);
            let mut list = vec![];
            while let Some(event) = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
                .await
                .expect("native stream progress")
            {
                list.push(serde_json::to_value(event.unwrap()).unwrap());
            }
            events.push(list);
        }
        let actual = json!({"calls":*calls.lock().unwrap(),"events":events});
        let expected = ts.request(&input);
        assert_eq!(actual, expected, "{input}");
    }
    let calls = Arc::new(Mutex::new(vec![]));
    let clock = Arc::new(Mutex::new(1700000000000.0));
    let p = provider(json!({}), calls.clone(), clock.clone()).await;
    for stage in 0..4 {
        if stage == 2 {
            *clock.lock().unwrap() += 25.0 * 60.0 * 1000.0;
        }
        if stage == 3 {
            (p.oauth.as_ref().unwrap().refresh_token)(OAuthCredential::new("r", "a", 0.0), AbortSignal::default())
                .await
                .unwrap();
        }
        let mut o = SimpleStreamOptions::default();
        o.stream.request.api_key = Some("fake".into());
        let mut s = stream_fixture(
            &p,
            p.models.as_ref().unwrap()[0].clone(),
            serde_json::from_value(json!({"messages":[]})).unwrap(),
            o,
        );
        while s.next().await.is_some() {}
    }
    let direct = calls
        .lock()
        .unwrap()
        .iter()
        .filter(|c| c["url"].as_str().is_some_and(|s| s.ends_with("direct_access")))
        .count();
    assert_eq!(direct, 3, "cache hit, TTL boundary, refresh invalidation");
}

#[tokio::test]
async fn backend_acquisition_rejections_keep_frozen_catch_units() {
    let mut source = oracle::TsOracle::spawn(
        "example-gitlab-duo-oracle.ts",
        "packages/coding-agent/examples/extensions/custom-provider-gitlab-duo/index.ts",
    );
    let mut actual_cases = Vec::new();
    let mut expected_cases = Vec::new();
    for index in [0, 5] {
        for kind in ["error", "value"] {
            let input = json!({"kind":"streams","indices":[index],"backendThrow":kind,"valueUnits":[66,55296]});
            let expected = source.request(&pi_js_compat::js_json::JsValue::from(&input));
            let calls = Arc::new(Mutex::new(Vec::new()));
            let config = provider(input, calls.clone(), Arc::new(Mutex::new(1700000000000.0))).await;
            let mut options = SimpleStreamOptions::default();
            options.stream.request.api_key = Some("FAKE-gitlab".into());
            options.stream.request.headers = Some([("ignored".into(), Some("caller".into()))].into_iter().collect());
            let mut stream = stream_fixture(
                &config,
                config.models.as_ref().unwrap()[index].clone(),
                serde_json::from_value(json!({"messages":[]})).unwrap(),
                options,
            );
            let mut events = Vec::new();
            while let Some(event) = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
                .await
                .expect("catch settles")
            {
                events.push(pi_js_compat::js_json::to_value(&event.unwrap()).unwrap());
            }
            assert_eq!(
                expected["events"][0][0]["error"]["errorMessage"],
                pi_js_compat::js_json::JsValue::String(JsString::from_utf16(vec![66, 55296]))
            );
            actual_cases.push((
                index,
                kind,
                js_json!({"calls":*calls.lock().unwrap(),"events":[events]}),
            ));
            expected_cases.push((index, kind, expected));
        }
    }
    assert_eq!(actual_cases, expected_cases);
}

fn wait_fixture_output(mut child: std::process::Child) -> std::process::Output {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while child.try_wait().expect("fixture status").is_none() {
        if std::time::Instant::now() >= deadline {
            let pid = rustix::process::Pid::from_raw(child.id() as i32).unwrap();
            let killed = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
            let reaped = child.wait();
            panic!("fixture timed out; kill={killed:?}, reap={reaped:?}");
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    child.wait_with_output().expect("fixture output")
}

#[test]
fn catch_conversion_rejection_is_process_uncaught_like_frozen() {
    use std::io::Write;
    use std::os::unix::process::CommandExt;
    let source_lock = oracle::read_source_lock();
    oracle::verify_source_head(&source_lock);
    let mut source = oracle::spawn_oracle_process(
        &source_lock,
        "example-gitlab-duo-oracle.ts",
        "packages/coding-agent/examples/extensions/custom-provider-gitlab-duo/index.ts",
        &[],
        std::process::Stdio::piped(),
    );
    source
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"{\"kind\":\"streams\",\"indices\":[0],\"backendThrow\":\"conversion\",\"valueUnits\":[66,55296]}\n",
        )
        .unwrap();
    let source = wait_fixture_output(source);
    assert_eq!(source.status.code(), Some(1), "{source:?}");
    assert!(source.stdout.is_empty(), "no error event/completion may be invented");
    assert!(
        std::str::from_utf8(&source.stderr)
            .expect("Node stderr UTF8")
            .contains("TypeError: Cannot convert object to primitive value"),
        "{source:?}"
    );
    let native = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "gitlab_conversion_child", "--ignored", "--nocapture"])
        .env("PI_GITLAB_CONVERSION_CHILD", "1")
        .process_group(0)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let native = wait_fixture_output(native);
    assert_eq!(native.status.code(), Some(1), "{native:?}");
    assert_eq!(native.stderr, b"TypeError: Cannot convert object to primitive value\n");
}

#[tokio::test]
#[ignore = "isolated process hook fixture"]
async fn gitlab_conversion_child() {
    assert_eq!(std::env::var("PI_GITLAB_CONVERSION_CHILD").as_deref(), Ok("1"));
    pi_coding_agent::utils::uncaught::install_uncaught_handler();
    let config = provider(
        json!({"backendThrow":"conversion","valueUnits":[66,55296]}),
        Arc::new(Mutex::new(Vec::new())),
        Arc::new(Mutex::new(1700000000000.0)),
    )
    .await;
    let mut options = SimpleStreamOptions::default();
    options.stream.request.api_key = Some("FAKE-gitlab".into());
    let mut stream = stream_fixture(
        &config,
        config.models.as_ref().unwrap()[0].clone(),
        serde_json::from_value(json!({"messages":[]})).unwrap(),
        options,
    );
    let _event = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
        .await
        .expect("fixture stream progress");
}

// Shared support is compiled once per integration target.
#[path = "support/extensions.rs"]
pub mod shared_extensions;
#[path = "support/oracle.rs"]
pub mod shared_oracle;
