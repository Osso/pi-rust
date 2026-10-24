//! Port of `packages/ai/src/auth/credential-store.ts`.

use super::types::{AuthError, Credential, CredentialModifier, CredentialStore};
use crate::request::BoxFuture;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::Mutex as AsyncMutex;

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
    fn read(&self, provider_id: &str) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        let credentials = Arc::clone(&self.credentials);
        let provider_id = provider_id.to_owned();
        Box::pin(async move {
            Ok(lock(&credentials)
                .iter()
                .find(|(id, _)| id == &provider_id)
                .map(|(_, credential)| credential.clone()))
        })
    }

    fn modify(
        &self,
        provider_id: &str,
        modifier: CredentialModifier,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        let credentials = Arc::clone(&self.credentials);
        let provider_id = provider_id.to_owned();
        let provider_lock = self.provider_lock(&provider_id);
        Box::pin(async move {
            let _guard = provider_lock.lock().await;
            let current = lock(&credentials)
                .iter()
                .find(|(id, _)| id == &provider_id)
                .map(|(_, credential)| credential.clone());
            let next = modifier(current.clone()).await?;
            if let Some(credential) = &next {
                let mut stored = lock(&credentials);
                match stored.iter_mut().find(|(id, _)| id == &provider_id) {
                    Some(entry) => entry.1 = credential.clone(),
                    None => stored.push((provider_id, credential.clone())),
                }
            }
            Ok(next.or(current))
        })
    }

    fn delete(&self, provider_id: &str) -> BoxFuture<'_, Result<(), AuthError>> {
        let credentials = Arc::clone(&self.credentials);
        let provider_id = provider_id.to_owned();
        let provider_lock = self.provider_lock(&provider_id);
        Box::pin(async move {
            let _guard = provider_lock.lock().await;
            lock(&credentials).retain(|(id, _)| id != &provider_id);
            Ok(())
        })
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
