//! Port of `packages/coding-agent/src/core/runtime-credentials.ts`.
//!
//! Non-persistent runtime API keys mask stored credentials for one process.

use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex};

use pi_ai::auth::types::{
    ApiKeyCredential, AuthError, AuthOperationOptions, Credential, CredentialInfo, CredentialModifier, CredentialStore,
    CredentialType,
};
use pi_ai::request::BoxFuture;
use pi_js_compat::abort::AbortSignal;

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

    /// Read the backing credential without applying process-local API-key overrides.
    pub(crate) fn read_stored(
        &self,
        provider_id: &str,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.store.read(provider_id, options)
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
    fn read(
        &self,
        provider_id: &str,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        let provider_id = provider_id.to_owned();
        Box::pin(async move {
            if options.signal.as_ref().is_some_and(AbortSignal::aborted) {
                return Err(AuthError::Aborted);
            }
            match self.override_for(&provider_id) {
                Some(key) => Ok(Some(Credential::ApiKey(ApiKeyCredential {
                    key: Some(key),
                    env: None,
                }))),
                None => self.store.read(&provider_id, options).await,
            }
        })
    }

    fn list(&self, options: AuthOperationOptions) -> BoxFuture<'_, Result<Vec<CredentialInfo>, AuthError>> {
        Box::pin(async move {
            let entries = self.store.list(options.clone()).await?;
            if options.signal.as_ref().is_some_and(AbortSignal::aborted) {
                return Err(AuthError::Aborted);
            }
            let mut merged: Vec<CredentialInfo> = Vec::new();
            for entry in entries {
                match merged
                    .iter_mut()
                    .find(|existing| existing.provider_id == entry.provider_id)
                {
                    Some(existing) => *existing = entry,
                    None => merged.push(entry),
                }
            }
            for (provider_id, _) in lock(&self.overrides).iter() {
                let entry = CredentialInfo {
                    provider_id: provider_id.clone(),
                    credential_type: CredentialType::ApiKey,
                };
                match merged.iter_mut().find(|existing| &existing.provider_id == provider_id) {
                    Some(existing) => *existing = entry,
                    None => merged.push(entry),
                }
            }
            Ok(merged)
        })
    }

    fn modify(
        &self,
        provider_id: &str,
        modifier: CredentialModifier,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        self.store.modify(provider_id, modifier, options)
    }

    fn delete(&self, provider_id: &str, options: AuthOperationOptions) -> BoxFuture<'_, Result<(), AuthError>> {
        let provider_id = provider_id.to_owned();
        Box::pin(async move {
            if options.signal.as_ref().is_some_and(AbortSignal::aborted) {
                return Err(AuthError::Aborted);
            }
            self.store.delete(&provider_id, options).await?;
            self.remove_runtime_api_key(&provider_id);
            Ok(())
        })
    }
}
