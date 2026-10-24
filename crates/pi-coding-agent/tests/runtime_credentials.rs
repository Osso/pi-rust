//! Ports `packages/coding-agent/test/runtime-credentials.test.ts`.

use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use pi_ai::auth::types::{
    ApiKeyCredential, AuthError, AuthOperationOptions, Credential, CredentialInfo, CredentialModifier, CredentialStore,
    CredentialType, OAuthCredential,
};
use pi_ai::request::BoxFuture;
use pi_coding_agent::core::auth_storage::AuthStorage;
use pi_coding_agent::core::runtime_credentials::RuntimeCredentials;
use pi_js_compat::abort::AbortSignal;
use serde_json::{Map, json};

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

fn api_key(key: &str) -> Credential {
    Credential::ApiKey(ApiKeyCredential {
        key: Some(key.to_owned().into()),
        env: None,
    })
}

fn storage_with(entries: &[(&str, serde_json::Value)]) -> Arc<AuthStorage> {
    let mut data = Map::new();
    for (provider, credential) in entries {
        data.insert((*provider).to_owned(), credential.clone());
    }
    Arc::new(AuthStorage::in_memory(data))
}

#[test]
fn runtime_overrides_mask_stored_credentials_without_persisting() {
    let storage = storage_with(&[("anthropic", json!({"type": "api_key", "key": "stored-key"}))]);
    let credentials = RuntimeCredentials::new(Arc::clone(&storage) as Arc<dyn CredentialStore>);

    credentials.set_runtime_api_key("anthropic", &JsString::from("runtime-key"));
    runtime().block_on(async {
        assert_eq!(
            credentials
                .read("anthropic", AuthOperationOptions::default())
                .await
                .unwrap(),
            Some(api_key("runtime-key"))
        );
        assert_eq!(
            storage
                .read("anthropic", AuthOperationOptions::default())
                .await
                .unwrap(),
            Some(api_key("stored-key"))
        );
    });

    credentials.remove_runtime_api_key("anthropic");
    assert!(!credentials.has_runtime_api_key("anthropic"));
    runtime().block_on(async {
        assert_eq!(
            credentials
                .read("anthropic", AuthOperationOptions::default())
                .await
                .unwrap(),
            Some(api_key("stored-key"))
        );
    });
}

#[test]
fn enumeration_merges_overrides_without_exposing_keys() {
    let storage = storage_with(&[(
        "anthropic",
        json!({"type": "oauth", "access": "access", "refresh": "refresh", "expires": 1_760_000_000_000.0}),
    )]);
    let credentials = RuntimeCredentials::new(Arc::clone(&storage) as Arc<dyn CredentialStore>);
    credentials.set_runtime_api_key("anthropic", &JsString::from("runtime-key"));
    credentials.set_runtime_api_key("openai", &JsString::from("other-runtime-key"));

    let listed = runtime()
        .block_on(credentials.list(AuthOperationOptions::default()))
        .unwrap();
    assert_eq!(
        listed
            .into_iter()
            .map(|entry| (entry.provider_id, entry.credential_type))
            .collect::<Vec<_>>(),
        [
            ("anthropic".to_owned(), CredentialType::ApiKey),
            ("openai".to_owned(), CredentialType::ApiKey)
        ]
    );
}

struct RecordingStore {
    signals: std::sync::Mutex<Vec<Option<AbortSignal>>>,
}

impl CredentialStore for RecordingStore {
    fn read(
        &self,
        _provider_id: &str,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.signals.lock().unwrap().push(options.signal);
        Box::pin(async { Ok(None) })
    }

    fn list(&self, options: AuthOperationOptions) -> BoxFuture<'_, Result<Vec<CredentialInfo>, AuthError>> {
        self.signals.lock().unwrap().push(options.signal);
        Box::pin(async { Ok(Vec::new()) })
    }

    fn modify(
        &self,
        _provider_id: &str,
        _modifier: CredentialModifier,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.signals.lock().unwrap().push(options.signal);
        Box::pin(async { Ok(None) })
    }

    fn delete(&self, _provider_id: &str, options: AuthOperationOptions) -> BoxFuture<'_, Result<(), AuthError>> {
        self.signals.lock().unwrap().push(options.signal);
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn forwards_operation_signals_to_the_persistent_store() {
    let controller_signal = pi_js_compat::abort::AbortController::new();
    let signal = controller_signal.signal();
    let store = Arc::new(RecordingStore {
        signals: std::sync::Mutex::new(Vec::new()),
    });
    let credentials = RuntimeCredentials::new(Arc::clone(&store) as Arc<dyn CredentialStore>);
    let options = || AuthOperationOptions::new(Some(signal.clone()));

    runtime().block_on(async {
        credentials.read("anthropic", options()).await.unwrap();
        credentials.list(options()).await.unwrap();
        credentials
            .modify(
                "anthropic",
                Arc::new(|_current| Box::pin(async { Ok(None) })),
                options(),
            )
            .await
            .unwrap();
        credentials.delete("anthropic", options()).await.unwrap();
    });

    let signals = store.signals.lock().unwrap();
    assert_eq!(signals.len(), 4);
    assert!(signals.iter().all(|received| received.is_some()));
}

struct FailingDeleteStore {
    inner: Arc<AuthStorage>,
    signal: AbortSignal,
}

impl CredentialStore for FailingDeleteStore {
    fn read(
        &self,
        provider_id: &str,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.inner.read(provider_id, options)
    }

    fn list(&self, options: AuthOperationOptions) -> BoxFuture<'_, Result<Vec<CredentialInfo>, AuthError>> {
        self.inner.list(options)
    }

    fn modify(
        &self,
        provider_id: &str,
        modifier: CredentialModifier,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.inner.modify(provider_id, modifier, options)
    }

    fn delete(&self, _provider_id: &str, _options: AuthOperationOptions) -> BoxFuture<'_, Result<(), AuthError>> {
        let signal = self.signal.clone();
        Box::pin(async move { Err(AuthError::from_abort_reason(signal.wait().await)) })
    }
}

#[test]
fn keeps_a_runtime_override_when_persistent_deletion_aborted() {
    let storage = storage_with(&[("anthropic", json!({"type": "api_key", "key": "stored-key"}))]);
    let controller = pi_js_compat::abort::AbortController::new();
    controller.abort(Some(pi_js_compat::abort::AbortReason::Value(
        pi_js_compat::js_json::JsValue::Bool(false),
    )));
    let failing = Arc::new(FailingDeleteStore {
        inner: Arc::clone(&storage),
        signal: controller.signal(),
    });
    let credentials = RuntimeCredentials::new(failing as Arc<dyn CredentialStore>);
    credentials.set_runtime_api_key("anthropic", &JsString::from("runtime-key"));

    let error = runtime()
        .block_on(credentials.delete("anthropic", AuthOperationOptions::default()))
        .unwrap_err();
    assert_eq!(error.thrown_value(), controller.signal().reason().unwrap());
    runtime().block_on(async {
        assert_eq!(
            credentials
                .read("anthropic", AuthOperationOptions::default())
                .await
                .unwrap(),
            Some(api_key("runtime-key"))
        );
    });
}

#[test]
fn delete_clears_both_the_override_and_the_persisted_credential() {
    let storage = storage_with(&[("anthropic", json!({"type": "api_key", "key": "stored-key"}))]);
    let credentials = RuntimeCredentials::new(Arc::clone(&storage) as Arc<dyn CredentialStore>);
    credentials.set_runtime_api_key("anthropic", &JsString::from("runtime-key"));

    runtime()
        .block_on(credentials.delete("anthropic", AuthOperationOptions::default()))
        .unwrap();

    runtime().block_on(async {
        assert_eq!(
            credentials
                .read("anthropic", AuthOperationOptions::default())
                .await
                .unwrap(),
            None
        );
        assert!(
            credentials
                .list(AuthOperationOptions::default())
                .await
                .unwrap()
                .is_empty()
        );
    });
    let _ = OAuthCredential::new("r", "a", 1.0);
}
