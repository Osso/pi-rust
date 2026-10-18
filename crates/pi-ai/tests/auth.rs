use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use indexmap::IndexMap;
use pi_ai::auth::{
    ApiKeyAuth, AuthContext, AuthError, AuthEvent, AuthInteraction, AuthOperationOptions, AuthPrompt, AuthPromptKind,
    AuthResult, Credential, CredentialModifier, CredentialStore, CredentialType, InMemoryCredentialStore,
    LazyOAuthInput, ModelsErrorCode, OAuthAuth, OAuthCredential, ProviderAuth, ResolveError, env_api_key_auth,
    lazy_oauth, resolve_provider_auth,
};
use pi_ai::request::BoxFuture;
use pi_ai::types::ProviderEnv;
use serde_json::{Value, json};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct TestAuthContext {
    env: BTreeMap<String, String>,
    files: Vec<String>,
}

impl AuthContext for TestAuthContext {
    fn env(&self, name: &str) -> BoxFuture<'_, Option<String>> {
        let value = self.env.get(name).cloned();
        Box::pin(async move { value })
    }

    fn file_exists(&self, path: &str) -> BoxFuture<'_, bool> {
        let exists = self.files.iter().any(|file| file == path);
        Box::pin(async move { exists })
    }
}

fn auth_context(env: &[(&str, &str)], files: &[&str]) -> Arc<dyn AuthContext> {
    Arc::new(TestAuthContext {
        env: env
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect(),
        files: files.iter().map(|file| (*file).to_owned()).collect(),
    })
}

struct TestInteraction {
    signal: CancellationToken,
    answer: String,
    prompts: Arc<Mutex<Vec<AuthPrompt>>>,
    events: Arc<Mutex<Vec<AuthEvent>>>,
}

impl TestInteraction {
    fn new(signal: CancellationToken, answer: &str) -> Self {
        Self {
            signal,
            answer: answer.to_owned(),
            prompts: Arc::new(Mutex::new(Vec::new())),
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl AuthInteraction for TestInteraction {
    fn signal(&self) -> CancellationToken {
        self.signal.clone()
    }

    fn prompt(&self, prompt: AuthPrompt) -> BoxFuture<'_, Result<String, pi_ai::request::CallbackError>> {
        self.prompts
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(prompt);
        let answer = self.answer.clone();
        Box::pin(async move { Ok(answer) })
    }

    fn notify(&self, event: AuthEvent) {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(event);
    }
}

type RefreshFn = Arc<
    dyn Fn(OAuthCredential, CancellationToken) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> + Send + Sync,
>;

struct TestOAuth {
    name: String,
    refresh_impl: RefreshFn,
}

impl OAuthAuth for TestOAuth {
    fn name(&self) -> &str {
        &self.name
    }

    fn login(&self, _interaction: Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        Box::pin(async { Err(AuthError::failed("not used")) })
    }

    fn refresh(
        &self,
        credential: OAuthCredential,
        signal: CancellationToken,
    ) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        (self.refresh_impl)(credential, signal)
    }

    fn to_auth(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<pi_ai::auth::ModelAuth, AuthError>> {
        Box::pin(async move {
            Ok(pi_ai::auth::ModelAuth {
                api_key: Some(credential.access),
                headers: None,
                base_url: None,
            })
        })
    }
}

fn test_oauth(refresh_impl: RefreshFn) -> Arc<TestOAuth> {
    Arc::new(TestOAuth {
        name: "Test OAuth".to_owned(),
        refresh_impl,
    })
}

fn never_refresh() -> RefreshFn {
    Arc::new(|credential, _signal| Box::pin(async move { Ok(credential) }))
}

fn now_ms() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as f64
}

fn oauth(access: &str, expires: f64) -> Credential {
    Credential::OAuth(OAuthCredential::new("refresh", access, expires))
}

#[test]
fn credential_round_trips_and_preserves_extra_oauth_keys() {
    let value = json!({
        "type": "oauth",
        "refresh": "r1",
        "access": "a1",
        "expires": 1234,
        "accountId": "acct",
        "nested": { "scope": ["a", "b"] }
    });
    let credential: Credential = serde_json::from_value(value.clone()).unwrap();
    let oauth = credential.as_oauth().unwrap();
    assert_eq!(oauth.refresh, "r1");
    assert_eq!(oauth.access, "a1");
    assert_eq!(oauth.expires, 1234.0);
    assert_eq!(oauth.extra.get("accountId"), Some(&json!("acct")));
    assert_eq!(serde_json::to_value(&credential).unwrap(), value);

    let api_key: Credential = serde_json::from_value(json!({"type": "api_key", "key": "k"})).unwrap();
    assert_eq!(api_key.credential_type(), CredentialType::ApiKey);
    assert_eq!(api_key.as_api_key().unwrap().key.as_deref(), Some("k"));
    assert_eq!(
        serde_json::to_value(&api_key).unwrap(),
        json!({"type": "api_key", "key": "k"})
    );
}

#[tokio::test]
async fn in_memory_store_lists_metadata_and_serializes_modifications() {
    let credentials = InMemoryCredentialStore::new();
    credentials
        .modify(
            "api-provider",
            Arc::new(|_| Box::pin(async { Ok(Some(Credential::ApiKey(Default::default()))) })),
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();
    credentials
        .modify(
            "oauth-provider",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth("access", now_ms() + 60_000.0))) })),
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();

    let listed = credentials.list(AuthOperationOptions::default()).await.unwrap();
    assert_eq!(
        listed,
        vec![
            pi_ai::auth::CredentialInfo {
                provider_id: "api-provider".to_owned(),
                credential_type: CredentialType::ApiKey,
            },
            pi_ai::auth::CredentialInfo {
                provider_id: "oauth-provider".to_owned(),
                credential_type: CredentialType::Oauth,
            },
        ]
    );
    let serialized = serde_json::to_value(&listed).unwrap();
    assert_eq!(serialized[0], json!({"providerId": "api-provider", "type": "api_key"}));
    assert!(!serialized.to_string().contains("access"));
}

#[tokio::test]
async fn aborted_queued_mutation_never_runs() {
    let credentials = Arc::new(InMemoryCredentialStore::new());
    let first_blocked = Arc::new(Notify::new());
    let release_first = Arc::new(Notify::new());
    let first = {
        let first_blocked = Arc::clone(&first_blocked);
        let release_first = Arc::clone(&release_first);
        let credentials = Arc::clone(&credentials);
        async move {
            credentials
                .modify(
                    "p1",
                    Arc::new(move |_| {
                        let first_blocked = Arc::clone(&first_blocked);
                        let release_first = Arc::clone(&release_first);
                        Box::pin(async move {
                            first_blocked.notify_one();
                            release_first.notified().await;
                            Ok(Some(Credential::ApiKey(pi_ai::auth::ApiKeyCredential {
                                key: Some("first".to_owned()),
                                env: None,
                            })))
                        })
                    }),
                    AuthOperationOptions::default(),
                )
                .await
        }
    };
    let first = tokio::spawn(first);
    first_blocked.notified().await;
    let signal = CancellationToken::new();
    let second_ran = Arc::new(AtomicBool::new(false));
    let task_signal = signal.clone();
    let second = {
        let second_ran = Arc::clone(&second_ran);
        let credentials = Arc::clone(&credentials);
        async move {
            credentials
                .modify(
                    "p1",
                    Arc::new(move |_| {
                        second_ran.store(true, Ordering::SeqCst);
                        Box::pin(async { Ok(Some(Credential::ApiKey(Default::default()))) })
                    }),
                    AuthOperationOptions {
                        signal: Some(task_signal.clone()),
                    },
                )
                .await
        }
    };
    let second = tokio::spawn(second);
    tokio::task::yield_now().await;
    signal.cancel();
    assert_eq!(second.await.unwrap(), Err(AuthError::Aborted));
    release_first.notify_one();
    first.await.unwrap().unwrap();
    tokio::task::yield_now().await;

    assert!(!second_ran.load(Ordering::SeqCst));
    assert_eq!(
        credentials.read("p1", AuthOperationOptions::default()).await.unwrap(),
        Some(Credential::ApiKey(pi_ai::auth::ApiKeyCredential {
            key: Some("first".to_owned()),
            env: None,
        }))
    );
}

#[tokio::test]
async fn env_api_key_auth_prefers_stored_key_then_env_order() {
    let auth = env_api_key_auth("Test key", &["FIRST_KEY", "SECOND_KEY"]);
    let stored = auth
        .resolve(pi_ai::auth::ApiKeyResolveInput {
            ctx: auth_context(&[("FIRST_KEY", "env")], &[]),
            credential: Some(pi_ai::auth::ApiKeyCredential {
                key: Some("stored".to_owned()),
                env: None,
            }),
            signal: CancellationToken::new(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.auth.api_key.as_deref(), Some("stored"));
    assert_eq!(stored.source.as_deref(), Some("stored credential"));

    let second = auth
        .resolve(pi_ai::auth::ApiKeyResolveInput {
            ctx: auth_context(&[("SECOND_KEY", "second")], &[]),
            credential: None,
            signal: CancellationToken::new(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(second.auth.api_key.as_deref(), Some("second"));
    assert_eq!(second.source.as_deref(), Some("SECOND_KEY"));

    assert_eq!(
        auth.resolve(pi_ai::auth::ApiKeyResolveInput {
            ctx: auth_context(&[], &[]),
            credential: None,
            signal: CancellationToken::new(),
        })
        .await
        .unwrap(),
        None
    );
}

#[tokio::test]
async fn env_api_key_login_prompts_for_a_secret() {
    let auth = env_api_key_auth("Test key", &["TEST_KEY"]);
    let interaction = Arc::new(TestInteraction::new(CancellationToken::new(), "entered-key"));
    let trait_interaction: Arc<dyn AuthInteraction> = interaction.clone();
    let credential = auth.login(trait_interaction).unwrap().await.unwrap();
    assert_eq!(credential.key.as_deref(), Some("entered-key"));
    let prompts = interaction.prompts.lock().unwrap();
    assert_eq!(prompts.len(), 1);
    assert!(matches!(
        &prompts[0].kind,
        AuthPromptKind::Secret { message, .. } if message == "Enter Test key"
    ));
}

#[tokio::test]
async fn resolve_provider_auth_honors_explicit_key_stored_credentials_and_ambient_fallback() {
    let credentials = Arc::new(InMemoryCredentialStore::new());
    let api_key = Arc::new(env_api_key_auth("Test key", &["ENV_KEY"]));
    let provider_auth = ProviderAuth {
        api_key: Some(api_key),
        oauth: Some(test_oauth(never_refresh())),
    };
    let ctx = auth_context(&[("ENV_KEY", "env-key")], &[]);

    let explicit = resolve_provider_auth(
        "p1",
        &provider_auth,
        Arc::clone(&credentials) as Arc<dyn CredentialStore>,
        Arc::clone(&ctx),
        pi_ai::auth::AuthResolutionOverrides {
            api_key: Some("explicit".to_owned()),
            env: Some(IndexMap::from([("ACCOUNT_ID".to_owned(), "acct".to_owned())])),
            ..Default::default()
        },
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(explicit.auth.api_key.as_deref(), Some("explicit"));

    let ambient = resolve_provider_auth(
        "p1",
        &provider_auth,
        Arc::clone(&credentials) as Arc<dyn CredentialStore>,
        Arc::clone(&ctx),
        Default::default(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(ambient.auth.api_key.as_deref(), Some("env-key"));

    credentials
        .modify(
            "p1",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth("oauth-token", now_ms() + 600_000.0))) })),
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();
    let oauth_resolution = resolve_provider_auth(
        "p1",
        &provider_auth,
        Arc::clone(&credentials) as Arc<dyn CredentialStore>,
        Arc::clone(&ctx),
        Default::default(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(oauth_resolution.auth.api_key.as_deref(), Some("oauth-token"));
    assert_eq!(oauth_resolution.source.as_deref(), Some("OAuth"));

    // A stored credential without a matching handler blocks ambient fallback.
    credentials
        .modify(
            "p2",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth("stale", now_ms() + 600_000.0))) })),
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();
    let blocked = resolve_provider_auth(
        "p2",
        &ProviderAuth::api_key(Arc::new(env_api_key_auth("Test key", &["ENV_KEY"]))),
        Arc::clone(&credentials) as Arc<dyn CredentialStore>,
        ctx,
        Default::default(),
    )
    .await
    .unwrap();
    assert_eq!(blocked, None);
}

#[tokio::test]
async fn expired_oauth_refreshes_once_and_persists_rotated_credential() {
    let credentials = Arc::new(InMemoryCredentialStore::new());
    credentials
        .modify(
            "p1",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth("old", 0.0))) })),
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();
    let refreshes = Arc::new(AtomicUsize::new(0));
    let oauth_auth = test_oauth({
        let refreshes = Arc::clone(&refreshes);
        Arc::new(move |_credential, _signal| {
            let refreshes = Arc::clone(&refreshes);
            Box::pin(async move {
                let count = refreshes.fetch_add(1, Ordering::SeqCst) + 1;
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                Ok(OAuthCredential::new(
                    "rotated",
                    format!("new-{count}"),
                    now_ms() + 3_600_000.0,
                ))
            })
        })
    });
    let provider_auth = ProviderAuth::oauth(oauth_auth);
    let ctx = auth_context(&[], &[]);

    let (left, right) = tokio::join!(
        resolve_provider_auth(
            "p1",
            &provider_auth,
            Arc::clone(&credentials) as Arc<dyn CredentialStore>,
            Arc::clone(&ctx),
            Default::default(),
        ),
        resolve_provider_auth(
            "p1",
            &provider_auth,
            Arc::clone(&credentials) as Arc<dyn CredentialStore>,
            Arc::clone(&ctx),
            Default::default(),
        )
    );
    assert_eq!(left.unwrap().unwrap().auth.api_key.as_deref(), Some("new-1"));
    assert_eq!(right.unwrap().unwrap().auth.api_key.as_deref(), Some("new-1"));
    assert_eq!(refreshes.load(Ordering::SeqCst), 1);
    let stored = credentials.read("p1", AuthOperationOptions::default()).await.unwrap();
    assert_eq!(stored.unwrap().as_oauth().unwrap().refresh, "rotated");
}

#[tokio::test]
async fn oauth_refresh_failure_keeps_stored_credential_and_reports_oauth_code() {
    let credentials = Arc::new(InMemoryCredentialStore::new());
    credentials
        .modify(
            "p1",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth("old", 0.0))) })),
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();
    let oauth_auth = test_oauth(Arc::new(|_credential, _signal| {
        Box::pin(async { Err(AuthError::failed("token refresh failed (400): invalid_grant")) })
    }));
    let error = resolve_provider_auth(
        "p1",
        &ProviderAuth::oauth(oauth_auth),
        Arc::clone(&credentials) as Arc<dyn CredentialStore>,
        auth_context(&[], &[]),
        Default::default(),
    )
    .await
    .unwrap_err();
    let ResolveError::Models(error) = error else {
        panic!("expected ModelsError");
    };
    assert_eq!(error.code, ModelsErrorCode::Oauth);
    assert_eq!(
        error.message,
        "OAuth refresh failed for p1: token refresh failed (400): invalid_grant"
    );
    let stored = credentials.read("p1", AuthOperationOptions::default()).await.unwrap();
    assert_eq!(stored.unwrap().as_oauth().unwrap().access, "old");
}

#[tokio::test]
async fn oauth_refresh_aborts_without_writing() {
    let credentials = Arc::new(InMemoryCredentialStore::new());
    credentials
        .modify(
            "p1",
            Arc::new(|_| Box::pin(async { Ok(Some(oauth("old", 0.0))) })),
            AuthOperationOptions::default(),
        )
        .await
        .unwrap();
    let started = Arc::new(Notify::new());
    let oauth_auth = test_oauth({
        let started = Arc::clone(&started);
        Arc::new(move |_credential, _signal| {
            let started = Arc::clone(&started);
            Box::pin(async move {
                started.notify_one();
                std::future::pending::<()>().await;
                unreachable!()
            })
        })
    });
    let signal = CancellationToken::new();
    let pending = resolve_provider_auth(
        "p1",
        &ProviderAuth::oauth(oauth_auth),
        Arc::clone(&credentials) as Arc<dyn CredentialStore>,
        auth_context(&[], &[]),
        pi_ai::auth::AuthResolutionOverrides {
            signal: Some(signal.clone()),
            ..Default::default()
        },
    );
    let handle = tokio::spawn(pending);
    started.notified().await;
    signal.cancel();
    assert_eq!(handle.await.unwrap(), Err(ResolveError::Aborted));
    tokio::task::yield_now().await;
    let stored = credentials.read("p1", AuthOperationOptions::default()).await.unwrap();
    assert_eq!(stored.unwrap().as_oauth().unwrap().access, "old");
}

#[tokio::test]
async fn lazy_oauth_loads_implementation_once_and_delegates() {
    let loads = Arc::new(AtomicUsize::new(0));
    let implementation = test_oauth(never_refresh());
    let lazy = lazy_oauth(LazyOAuthInput {
        name: "Lazy".to_owned(),
        is_subscription: Some(true),
        login_label: Some("Sign in".to_owned()),
        load: {
            let loads = Arc::clone(&loads);
            let implementation = Arc::clone(&implementation);
            Arc::new(move || {
                loads.fetch_add(1, Ordering::SeqCst);
                let implementation = Arc::clone(&implementation);
                Box::pin(async move { Ok(implementation as Arc<dyn OAuthAuth>) })
            })
        },
    });
    assert_eq!(lazy.name(), "Lazy");
    assert!(lazy.is_subscription());
    assert_eq!(lazy.login_label(), Some("Sign in"));
    let auth = lazy
        .to_auth(OAuthCredential::new("r", "access", now_ms() + 1000.0))
        .await
        .unwrap();
    assert_eq!(auth.api_key.as_deref(), Some("access"));
    lazy.to_auth(OAuthCredential::new("r", "access", now_ms() + 1000.0))
        .await
        .unwrap();
    assert_eq!(loads.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn auth_result_serializes_provider_env_and_source() {
    let result = AuthResult {
        auth: pi_ai::auth::ModelAuth {
            api_key: Some("k".to_owned()),
            headers: None,
            base_url: None,
        },
        env: Some(ProviderEnv::from([("ACCOUNT".to_owned(), "a".to_owned())])),
        source: Some("stored credential".to_owned()),
    };
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        json!({
            "auth": {"apiKey": "k"},
            "env": {"ACCOUNT": "a"},
            "source": "stored credential"
        })
    );
}

#[tokio::test]
async fn custom_credential_store_failures_wrap_with_auth_code() {
    struct FailingStore;
    impl CredentialStore for FailingStore {
        fn read(
            &self,
            _provider_id: &str,
            _options: AuthOperationOptions,
        ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
            Box::pin(async { Err(AuthError::failed("disk on fire")) })
        }
        fn list(
            &self,
            _options: AuthOperationOptions,
        ) -> BoxFuture<'_, Result<Vec<pi_ai::auth::CredentialInfo>, AuthError>> {
            Box::pin(async { Ok(Vec::new()) })
        }
        fn modify(
            &self,
            _provider_id: &str,
            _modifier: CredentialModifier,
            _options: AuthOperationOptions,
        ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
            Box::pin(async { Ok(None) })
        }
        fn delete(&self, _provider_id: &str, _options: AuthOperationOptions) -> BoxFuture<'_, Result<(), AuthError>> {
            Box::pin(async { Ok(()) })
        }
    }
    let error = resolve_provider_auth(
        "p1",
        &ProviderAuth::api_key(Arc::new(env_api_key_auth("Test", &["KEY"]))),
        Arc::new(FailingStore),
        auth_context(&[("KEY", "value")], &[]),
        Default::default(),
    )
    .await
    .unwrap_err();
    let ResolveError::Models(error) = error else {
        panic!("expected ModelsError");
    };
    assert_eq!(error.code, ModelsErrorCode::Auth);
    assert_eq!(error.message, "Credential store read failed for p1: disk on fire");
}

#[test]
fn models_error_with_cause_keeps_detail_once() {
    let error =
        pi_ai::auth::ModelsError::with_cause(ModelsErrorCode::Oauth, "OAuth refresh failed for p1", Some("bad token"));
    assert_eq!(error.message, "OAuth refresh failed for p1: bad token");
    assert_eq!(error.code.as_str(), "oauth");
    let value: Value = serde_json::to_value(pi_ai::auth::AuthCheck {
        source: Some("OAuth".to_owned()),
        auth_type: pi_ai::auth::AuthType::Oauth,
    })
    .unwrap();
    assert_eq!(value, json!({"source": "OAuth", "type": "oauth"}));
}
