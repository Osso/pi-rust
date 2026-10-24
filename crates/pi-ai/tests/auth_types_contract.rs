//! Frozen auth/types.ts:8–182: current store, model input and optional login contract.
use std::sync::Arc;

use pi_ai::auth::{
    ApiKeyAuth, ApiKeyResolveInput, AuthError, AuthModel, AuthResult, Credential, CredentialModifier, CredentialStore,
    ModelAuth, env_api_key_auth,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::js_json::JsString;

struct FrozenStore;
impl CredentialStore for FrozenStore {
    fn read(&self, _id: &str) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        Box::pin(async { Ok(None) })
    }
    fn modify(&self, _id: &str, modifier: CredentialModifier) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        Box::pin(async move { modifier(None).await })
    }
    fn delete(&self, _id: &str) -> BoxFuture<'_, Result<(), AuthError>> {
        Box::pin(async { Ok(()) })
    }
}

struct ModelOnlyAuth;
impl ApiKeyAuth for ModelOnlyAuth {
    fn name(&self) -> JsString {
        JsString::from_utf16(vec![0x61, 0xd800])
    }
    fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        Box::pin(async move {
            let provider = match input.model {
                AuthModel::Chat(model) => model.provider,
                AuthModel::Images(model) => model.provider,
            };
            Ok(Some(AuthResult {
                auth: ModelAuth {
                    api_key: Some(provider.into()),
                    ..Default::default()
                },
                env: None,
                source: None,
            }))
        })
    }
}

#[tokio::test]
async fn store_has_only_frozen_read_modify_delete_operations() {
    let store: Arc<dyn CredentialStore> = Arc::new(FrozenStore);
    assert_eq!(store.read("p").await.unwrap(), None);
    assert_eq!(
        store
            .modify("p", Arc::new(|current| Box::pin(async move { Ok(current) })))
            .await
            .unwrap(),
        None
    );
    store.delete("p").await.unwrap();
}

#[test]
fn human_auth_names_and_optional_login_preserve_frozen_shape() {
    let expected = JsString::from_utf16(vec![0x61, 0xd800]);
    let ambient: Arc<dyn ApiKeyAuth> = Arc::new(ModelOnlyAuth);
    assert_eq!(ambient.name(), expected);
    assert!(ambient.login().is_none());
    let standard = env_api_key_auth(expected.clone(), &["KEY"]);
    assert_eq!(standard.name(), expected);
    assert!(standard.login().is_some());
}

#[path = "support/oracle.rs"]
mod oracle;
use pi_ai::auth::{AuthContext, AuthEvent, AuthInteraction, AuthPrompt, OAuthAuth};
use pi_ai::request::CallbackError;
use pi_js_compat::abort::{AbortReason, AbortSignal};
use pi_js_compat::{js_json, js_json::JsValue};
use std::sync::Mutex;

struct RejectingCallbacks {
    operation: &'static str,
    reason: AbortReason,
    calls: Mutex<Vec<JsString>>,
}
impl RejectingCallbacks {
    fn reject(&self, call: JsString) -> AuthError {
        self.calls.lock().unwrap().push(call);
        AuthError::from(CallbackError::from_rejection(self.reason.clone()))
    }
}
impl AuthContext for RejectingCallbacks {
    fn env(&self, name: &str) -> BoxFuture<'_, Result<Option<JsString>, AuthError>> {
        let call = format!("env:{name}").into();
        Box::pin(async move {
            if self.operation == "env" {
                return Err(self.reject(call));
            }
            self.calls.lock().unwrap().push(call);
            Ok(None)
        })
    }
    fn file_exists(&self, path: &JsString) -> BoxFuture<'_, Result<bool, AuthError>> {
        let call = pi_js_compat::js_format!("file:{}", path);
        Box::pin(async move { Err(self.reject(call)) })
    }
}
impl AuthInteraction for RejectingCallbacks {
    fn signal(&self) -> AbortSignal {
        AbortSignal::default()
    }
    fn prompt(&self, prompt: AuthPrompt) -> BoxFuture<'_, Result<JsString, CallbackError>> {
        if self.operation == "notify_codex_leak"
            && let pi_ai::auth::AuthPromptKind::Select { options, .. } = prompt.kind
        {
            self.calls.lock().unwrap().push("select".into());
            let method = options[0].id.clone();
            return Box::pin(async move { Ok(method) });
        }
        self.calls.lock().unwrap().push("prompt".into());
        Box::pin(async {
            Err(CallbackError::from_rejection(AbortReason::Value(JsValue::String(
                "prompt reached".into(),
            ))))
        })
    }
    fn notify(&self, event: AuthEvent) -> Result<(), CallbackError> {
        let tag = match event {
            AuthEvent::AuthUrl { .. } => "auth_url",
            AuthEvent::DeviceCode { .. } => "device_code",
            AuthEvent::Progress { .. } => "progress",
        };
        Err(self.reject(tag.into()).into_callback())
    }
}

#[tokio::test]
async fn context_and_notify_rejections_match_real_frozen_providers() {
    let mut source = oracle::TsOracle::spawn("auth-types-errors-oracle.ts", "packages/ai/src/providers/anthropic.ts");
    let reasons = [
        JsValue::Null,
        JsValue::Bool(false),
        JsValue::Number(0.0),
        JsValue::String("".into()),
        JsValue::String(JsString::from_utf16(vec![0xd800, 0x78])),
        js_json!({"message":"plain","nested":[false,3]}),
    ];
    let json_cases = reasons
        .into_iter()
        .map(|value| (value.clone(), AbortReason::Value(value), false));
    let controller = pi_js_compat::abort::AbortController::new();
    controller.abort(None); // Native omitted/undefined reason, never JSON null.
    let default_abort = (JsValue::Null, controller.signal().reason().unwrap(), true);
    let cases: Vec<_> = json_cases.chain(std::iter::once(default_abort)).collect();
    for operation in ["env", "file", "notify"] {
        for (wire_reason, reason, abort_undefined) in &cases {
            let expected = source
                .request(&js_json!({"operation":operation,"reason":wire_reason,"abortUndefined":abort_undefined}));
            assert_eq!(expected["ok"], JsValue::Bool(false), "source must actually reject");
            assert_eq!(
                expected["thrown"],
                reason.value(),
                "source must reach the intended callback"
            );
            let callbacks = Arc::new(RejectingCallbacks {
                operation,
                reason: reason.clone(),
                calls: Mutex::new(Vec::new()),
            });
            let result = if operation == "notify" {
                pi_ai::auth::oauth::anthropic::AnthropicOAuth::new()
                    .login(callbacks.clone())
                    .await
                    .map(|_| ())
            } else {
                let provider = if operation == "env" {
                    pi_ai::providers::anthropic::anthropic_provider()
                } else {
                    pi_ai::providers::google_vertex::google_vertex_provider()
                };
                let model = provider.get_models().unwrap().into_iter().next().unwrap();
                provider
                    .auth()
                    .api_key
                    .as_ref()
                    .unwrap()
                    .resolve(ApiKeyResolveInput {
                        model: AuthModel::Chat(model),
                        ctx: callbacks.clone(),
                        credential: None,
                    })
                    .await
                    .map(|_| ())
            };
            let actual = match result {
                Ok(()) => js_json!({"ok":true,"calls":callbacks.calls.lock().unwrap().clone()}),
                Err(error) => {
                    assert_eq!(&error.thrown_value(), reason, "native rejection branding must survive");
                    js_json!({"ok":false,"thrown":error.thrown_value().value(),"calls":callbacks.calls.lock().unwrap().clone()})
                }
            };
            assert_eq!(actual, expected, "operation={operation}, reason={reason:?}");
            // The synchronous oracle request must not block Tokio from disposing
            // the aborted native listener before the next login uses its port.
            tokio::task::yield_now().await;
        }
    }
}

fn codex_listener_open() -> bool {
    match std::net::TcpListener::bind(("127.0.0.1", 1455)) {
        Ok(listener) => {
            drop(listener);
            false
        }
        Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => true,
        Err(error) => panic!("cannot probe Codex listener: {error}"),
    }
}

#[test]
fn codex_listener_leak_probe_child() {
    let Some(path) = std::env::var_os("PI_AUTH_CODEX_LEAK_RESULT") else {
        return;
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let snapshot = runtime.block_on(async {
        let callbacks = Arc::new(RejectingCallbacks {
            operation: "notify_codex_leak",
            reason: AbortReason::Value(JsValue::Bool(false)),
            calls: Mutex::new(Vec::new()),
        });
        let error = pi_ai::auth::oauth::openai_codex::OpenAICodexOAuth::new()
            .login(callbacks.clone()).await.unwrap_err();
        tokio::task::yield_now().await;
        js_json!({"ok":false,"thrown":error.thrown_value().value(),"calls":callbacks.calls.lock().unwrap().clone(),"listenerOpen":codex_listener_open()})
    });
    std::fs::write(path, js_json::stringify_lossless(&snapshot)).unwrap();
    // Dropping this isolated runtime closes the deliberately leaked source-bug listener.
}

#[test]
fn codex_auth_url_throw_preserves_frozen_listener_leak() {
    assert!(
        !codex_listener_open(),
        "fixture requires the frozen callback port to be available"
    );
    let mut source = oracle::TsOracle::spawn("auth-types-errors-oracle.ts", "packages/ai/src/providers/anthropic.ts");
    let expected = source.request(&js_json!({"operation":"notify_codex_leak","reason":false}));
    assert_eq!(expected["thrown"], JsValue::Bool(false), "source must reach notify");
    assert_eq!(
        expected["listenerOpen"],
        JsValue::Bool(true),
        "source onAuth is before try/finally"
    );
    drop(source);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while codex_listener_open() {
        assert!(
            std::time::Instant::now() < deadline,
            "source oracle must release its listener on process exit"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let directory = std::env::temp_dir().join(format!(
        "auth-codex-listener-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let result = directory.join("result.json");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "codex_listener_leak_probe_child", "--nocapture"])
        .env("PI_AUTH_CODEX_LEAK_RESULT", &result)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let actual = js_json::parse_lossless(std::fs::read_to_string(result).unwrap()).unwrap();
    assert_eq!(actual, expected);
    assert!(
        !codex_listener_open(),
        "isolated native child must release its listener on exit"
    );
    std::fs::remove_dir_all(directory).unwrap();
}
