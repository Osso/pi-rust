//! Port of `packages/ai/src/auth/credential-store.ts`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::Mutex as AsyncMutex;

use crate::request::BoxFuture;
use crate::utils::abort::{operation_signal, race_with_abort_signal};

use super::types::{AuthError, AuthOperationOptions, Credential, CredentialInfo, CredentialModifier, CredentialStore};

/// Default in-memory credential store. Writes are serialized per provider.
#[derive(Default)]
pub struct InMemoryCredentialStore {
    credentials: Arc<Mutex<Vec<(String, Credential)>>>,
    chains: Arc<Mutex<HashMap<String, Arc<AsyncMutex<()>>>>>,
}

impl InMemoryCredentialStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn provider_lock(&self, provider_id: &str) -> Arc<AsyncMutex<()>> {
        let mut chains = lock(&self.chains);
        Arc::clone(
            chains
                .entry(provider_id.to_owned())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        )
    }
}

impl CredentialStore for InMemoryCredentialStore {
    fn read(
        &self,
        provider_id: &str,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        let credentials = Arc::clone(&self.credentials);
        let provider_id = provider_id.to_owned();
        let signal = operation_signal(options.signal.as_ref());
        Box::pin(async move {
            AuthError::check_aborted(&signal)?;
            Ok(lock(&credentials)
                .iter()
                .find(|(id, _)| id == &provider_id)
                .map(|(_, credential)| credential.clone()))
        })
    }

    fn list(&self, options: AuthOperationOptions) -> BoxFuture<'_, Result<Vec<CredentialInfo>, AuthError>> {
        let credentials = Arc::clone(&self.credentials);
        let signal = operation_signal(options.signal.as_ref());
        Box::pin(async move {
            AuthError::check_aborted(&signal)?;
            Ok(lock(&credentials)
                .iter()
                .map(|(provider_id, credential)| CredentialInfo {
                    provider_id: provider_id.clone(),
                    credential_type: credential.credential_type(),
                })
                .collect())
        })
    }

    fn modify(
        &self,
        provider_id: &str,
        modifier: CredentialModifier,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        let credentials = Arc::clone(&self.credentials);
        let provider_id = provider_id.to_owned();
        let provider_lock = self.provider_lock(&provider_id);
        let signal = operation_signal(options.signal.as_ref());
        Box::pin(async move {
            let queued_signal = signal.clone();
            let queued = async move {
                let _guard = provider_lock.lock().await;
                AuthError::check_aborted(&queued_signal)?;
                let current = lock(&credentials)
                    .iter()
                    .find(|(id, _)| id == &provider_id)
                    .map(|(_, credential)| credential.clone());
                let next = modifier(current.clone()).await?;
                AuthError::check_aborted(&queued_signal)?;
                if let Some(credential) = &next {
                    let mut stored = lock(&credentials);
                    match stored.iter_mut().find(|(id, _)| id == &provider_id) {
                        Some(entry) => entry.1 = credential.clone(),
                        None => stored.push((provider_id, credential.clone())),
                    }
                }
                Ok(next.or(current))
            };
            race_with_abort_signal(queued, &signal)
                .await
                .map_err(AuthError::from_abort_reason)?
        })
    }

    fn delete(&self, provider_id: &str, options: AuthOperationOptions) -> BoxFuture<'_, Result<(), AuthError>> {
        let credentials = Arc::clone(&self.credentials);
        let provider_id = provider_id.to_owned();
        let provider_lock = self.provider_lock(&provider_id);
        let signal = operation_signal(options.signal.as_ref());
        Box::pin(async move {
            let queued_signal = signal.clone();
            let queued = async move {
                let _guard = provider_lock.lock().await;
                AuthError::check_aborted(&queued_signal)?;
                lock(&credentials).retain(|(id, _)| id != &provider_id);
                Ok(())
            };
            race_with_abort_signal(queued, &signal)
                .await
                .map_err(AuthError::from_abort_reason)?
        })
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
