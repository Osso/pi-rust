//! Port of `packages/ai/src/models-store.ts`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

use crate::js_number;
use crate::request::BoxFuture;
use crate::types::Model;
use crate::utils::abort::operation_signal;

use crate::auth::types::AuthError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsStoreEntry {
    pub models: Vec<Model>,
    /// Unix timestamp from the remote catalog's Last-Modified header.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub last_modified: Option<f64>,
    /// Unix timestamp of the last completed remote check.
    #[serde(
        skip_serializing_if = "Option::is_none",
        serialize_with = "js_number::serialize_option"
    )]
    pub checked_at: Option<f64>,
    /// Opaque ETag validator, stored verbatim (quotes included).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etag: Option<String>,
}

impl ModelsStoreEntry {
    pub fn new(models: Vec<Model>) -> Self {
        Self {
            models,
            last_modified: None,
            checked_at: None,
            etag: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ModelsStoreOperationOptions {
    pub signal: Option<CancellationToken>,
}

/// Persistent model catalogs keyed by provider ID.
pub trait ModelsStore: Send + Sync {
    fn read(
        &self,
        provider_id: &str,
        options: ModelsStoreOperationOptions,
    ) -> BoxFuture<'_, Result<Option<ModelsStoreEntry>, AuthError>>;

    fn write(
        &self,
        provider_id: &str,
        entry: ModelsStoreEntry,
        options: ModelsStoreOperationOptions,
    ) -> BoxFuture<'_, Result<(), AuthError>>;

    fn delete(&self, provider_id: &str, options: ModelsStoreOperationOptions) -> BoxFuture<'_, Result<(), AuthError>>;
}

#[derive(Default)]
pub struct InMemoryModelsStore {
    entries: Arc<Mutex<HashMap<String, ModelsStoreEntry>>>,
}

impl InMemoryModelsStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ModelsStore for InMemoryModelsStore {
    fn read(
        &self,
        provider_id: &str,
        options: ModelsStoreOperationOptions,
    ) -> BoxFuture<'_, Result<Option<ModelsStoreEntry>, AuthError>> {
        let entries = Arc::clone(&self.entries);
        let provider_id = provider_id.to_owned();
        let signal = operation_signal(options.signal.as_ref());
        Box::pin(async move {
            check_aborted(&signal)?;
            Ok(lock(&entries).get(&provider_id).cloned())
        })
    }

    fn write(
        &self,
        provider_id: &str,
        entry: ModelsStoreEntry,
        options: ModelsStoreOperationOptions,
    ) -> BoxFuture<'_, Result<(), AuthError>> {
        let entries = Arc::clone(&self.entries);
        let provider_id = provider_id.to_owned();
        let signal = operation_signal(options.signal.as_ref());
        Box::pin(async move {
            check_aborted(&signal)?;
            lock(&entries).insert(provider_id, entry);
            Ok(())
        })
    }

    fn delete(&self, provider_id: &str, options: ModelsStoreOperationOptions) -> BoxFuture<'_, Result<(), AuthError>> {
        let entries = Arc::clone(&self.entries);
        let provider_id = provider_id.to_owned();
        let signal = operation_signal(options.signal.as_ref());
        Box::pin(async move {
            check_aborted(&signal)?;
            lock(&entries).remove(&provider_id);
            Ok(())
        })
    }
}

fn check_aborted(signal: &CancellationToken) -> Result<(), AuthError> {
    if signal.is_cancelled() {
        Err(AuthError::Aborted)
    } else {
        Ok(())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
