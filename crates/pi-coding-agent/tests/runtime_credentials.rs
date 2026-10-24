//! Ports `packages/coding-agent/test/runtime-credentials.test.ts`.

use pi_js_compat::js_json::JsString;
use std::sync::Arc;

use pi_ai::auth::types::{ApiKeyCredential, AuthError, Credential, CredentialModifier, CredentialStore};
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
            credentials.read("anthropic").await.unwrap(),
            Some(api_key("runtime-key"))
        );
        assert_eq!(storage.read("anthropic").await.unwrap(), Some(api_key("stored-key")));
    });

    credentials.remove_runtime_api_key("anthropic");
    assert!(!credentials.has_runtime_api_key("anthropic"));
    runtime().block_on(async {
        assert_eq!(
            credentials.read("anthropic").await.unwrap(),
            Some(api_key("stored-key"))
        );
    });
}

struct FailingDeleteStore {
    inner: Arc<AuthStorage>,
    signal: AbortSignal,
}

impl CredentialStore for FailingDeleteStore {
    fn read(&self, provider_id: &str) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.inner.read(provider_id)
    }

    fn modify(
        &self,
        provider_id: &str,
        modifier: CredentialModifier,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.inner.modify(provider_id, modifier)
    }

    fn delete(&self, _provider_id: &str) -> BoxFuture<'_, Result<(), AuthError>> {
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

    let error = runtime().block_on(credentials.delete("anthropic")).unwrap_err();
    assert_eq!(error.thrown_value(), controller.signal().reason().unwrap());
    runtime().block_on(async {
        assert_eq!(
            credentials.read("anthropic").await.unwrap(),
            Some(api_key("runtime-key"))
        );
    });
}

#[test]
fn delete_clears_both_the_override_and_the_persisted_credential() {
    let storage = storage_with(&[("anthropic", json!({"type": "api_key", "key": "stored-key"}))]);
    let credentials = RuntimeCredentials::new(Arc::clone(&storage) as Arc<dyn CredentialStore>);
    credentials.set_runtime_api_key("anthropic", &JsString::from("runtime-key"));

    runtime().block_on(credentials.delete("anthropic")).unwrap();

    runtime().block_on(async {
        assert_eq!(credentials.read("anthropic").await.unwrap(), None);
    });
}
