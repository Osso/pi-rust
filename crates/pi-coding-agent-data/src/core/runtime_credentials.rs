//! Port of `packages/coding-agent/src/core/runtime-credentials.ts`.
//!
//! Non-persistent runtime API keys mask stored credentials for one process.

use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use pi_ai::auth::types::{ApiKeyCredential, AuthError, Credential, CredentialModifier, CredentialStore};
use pi_ai::request::BoxFuture;

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Async credential-store overlay for runtime API keys (`RuntimeCredentials`).
pub struct RuntimeCredentials {
    store: Arc<dyn CredentialStore>,
    overrides: Mutex<Vec<(String, JsString)>>,
}

impl RuntimeCredentials {
    pub fn new(store: Arc<dyn CredentialStore>) -> Self {
        Self {
            store,
            overrides: Mutex::new(Vec::new()),
        }
    }

    pub fn set_runtime_api_key(&self, provider_id: &str, api_key: &JsString) {
        let mut overrides = lock(&self.overrides);
        match overrides.iter_mut().find(|(provider, _)| provider == provider_id) {
            Some((_, key)) => *key = api_key.to_owned(),
            None => overrides.push((provider_id.to_owned(), api_key.to_owned())),
        }
    }

    pub fn remove_runtime_api_key(&self, provider_id: &str) {
        lock(&self.overrides).retain(|(provider, _)| provider != provider_id);
    }

    pub fn has_runtime_api_key(&self, provider_id: &str) -> bool {
        lock(&self.overrides)
            .iter()
            .any(|(provider, _)| provider == provider_id)
    }

    fn override_for(&self, provider_id: &str) -> Option<JsString> {
        lock(&self.overrides)
            .iter()
            .find(|(provider, _)| provider == provider_id)
            .map(|(_, key)| key.clone())
    }
}

impl CredentialStore for RuntimeCredentials {
    fn read(&self, provider_id: &str) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        let provider_id = provider_id.to_owned();
        Box::pin(async move {
            match self.override_for(&provider_id) {
                Some(key) => Ok(Some(Credential::ApiKey(ApiKeyCredential {
                    key: Some(key),
                    env: None,
                }))),
                None => self.store.read(&provider_id).await,
            }
        })
    }

    fn modify(
        &self,
        provider_id: &str,
        modifier: CredentialModifier,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.store.modify(provider_id, modifier)
    }

    fn delete(&self, provider_id: &str) -> BoxFuture<'_, Result<(), AuthError>> {
        let provider_id = provider_id.to_owned();
        Box::pin(async move {
            self.store.delete(&provider_id).await?;
            self.remove_runtime_api_key(&provider_id);
            Ok(())
        })
    }
}
