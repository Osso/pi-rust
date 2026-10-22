//! Port of `packages/coding-agent/src/core/models-store.ts`.
//!
//! `models-store.json` holds dynamically refreshed provider catalogs. It is shared with the
//! TypeScript runtime, so writes keep every other provider's bytes and use two-space JSON.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use pi_ai::auth::types::{AuthError, AuthOperationOptions};
use pi_ai::models_store::{ModelsStore, ModelsStoreEntry, ModelsStoreOperationOptions};
use pi_ai::request::BoxFuture;
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use crate::config::get_agent_dir;
use crate::core::auth_storage::{AsyncLockOperation, AuthStorageBackend, FileAuthStorageBackend, StorageError};
use crate::utils::paths::{get_file_revision, normalize_path};
use pi_js_compat::js_json::{self, JsString, encode_node_utf8};
use pi_js_compat::number::js_number_value;

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// In-memory catalog store (`InMemoryCodingAgentModelsStore`).
#[derive(Default)]
pub struct InMemoryCodingAgentModelsStore {
    entries: Mutex<HashMap<String, ModelsStoreEntry>>,
}

impl InMemoryCodingAgentModelsStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ModelsStore for InMemoryCodingAgentModelsStore {
    fn read(
        &self,
        provider_id: &str,
        options: ModelsStoreOperationOptions,
    ) -> BoxFuture<'_, Result<Option<ModelsStoreEntry>, AuthError>> {
        let provider_id = provider_id.to_owned();
        Box::pin(async move {
            if options.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                return Err(AuthError::Aborted);
            }
            Ok(lock(&self.entries).get(&provider_id).cloned())
        })
    }

    fn write(
        &self,
        provider_id: &str,
        entry: ModelsStoreEntry,
        options: ModelsStoreOperationOptions,
    ) -> BoxFuture<'_, Result<(), AuthError>> {
        let provider_id = provider_id.to_owned();
        Box::pin(async move {
            if options.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                return Err(AuthError::Aborted);
            }
            lock(&self.entries).insert(provider_id, entry);
            Ok(())
        })
    }

    fn delete(&self, provider_id: &str, options: ModelsStoreOperationOptions) -> BoxFuture<'_, Result<(), AuthError>> {
        let provider_id = provider_id.to_owned();
        Box::pin(async move {
            if options.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                return Err(AuthError::Aborted);
            }
            lock(&self.entries).remove(&provider_id);
            Ok(())
        })
    }
}

type StoredModels = Map<String, Value>;

/// `ModelsStoreEntry` as JSON in the order the catalog provider writes it.
fn entry_to_value(entry: &ModelsStoreEntry) -> Result<Value, StorageError> {
    let mut object = Map::new();
    object.insert(
        "models".to_owned(),
        serde_json::to_value(&entry.models).map_err(|error| StorageError::new(error.to_string()))?,
    );
    if let Some(checked_at) = entry.checked_at {
        object.insert("checkedAt".to_owned(), js_number_value(checked_at));
    }
    if let Some(last_modified) = entry.last_modified {
        object.insert("lastModified".to_owned(), js_number_value(last_modified));
    }
    if let Some(etag) = &entry.etag {
        object.insert("etag".to_owned(), Value::String(etag.clone()));
    }
    Ok(Value::Object(object))
}

fn value_to_entry(value: &Value) -> Result<ModelsStoreEntry, StorageError> {
    serde_json::from_value(value.clone())
        .map_err(|error| StorageError::new(format!("Invalid models-store entry: {error}")))
}

fn parse_stored_models(content: Option<&str>) -> Result<StoredModels, StorageError> {
    let Some(content) = content else {
        return Ok(Map::new());
    };
    match js_json::parse(content).map_err(|error| StorageError::new(error.to_string()))? {
        Value::Object(object) => Ok(object),
        _ => Ok(Map::new()),
    }
}

struct Reload {
    signal: CancellationToken,
    readers: AtomicUsize,
    outcome: Arc<Mutex<Option<Result<StoredModels, StorageError>>>>,
    notify: Arc<tokio::sync::Notify>,
}

struct ReadState {
    data: StoredModels,
    revision: Option<String>,
    reload: Option<Arc<Reload>>,
}

/// Locked JSON-backed store for dynamically refreshed provider catalogs (`FileModelsStore`).
pub struct FileModelsStore {
    storage: Arc<dyn AuthStorageBackend>,
    path: JsString,
    read_state: Arc<Mutex<ReadState>>,
}

impl FileModelsStore {
    pub fn new(path: Option<&JsString>) -> Self {
        let path = path
            .cloned()
            .unwrap_or_else(|| get_agent_dir().join("models-store.json").display().to_string().into());
        let normalized = normalize_path(&path, &Default::default()).unwrap_or(path);
        Self {
            storage: Arc::new(FileAuthStorageBackend::new(&normalized)),
            path: normalized,
            read_state: Arc::new(Mutex::new(ReadState {
                data: Map::new(),
                revision: None,
                reload: None,
            })),
        }
    }

    /// Builds a store over an injected backend; used to observe lock behavior.
    pub fn with_backend(path: &JsString, storage: Arc<dyn AuthStorageBackend>) -> Self {
        let normalized = normalize_path(path, &Default::default()).unwrap_or_else(|_| path.to_owned());
        Self {
            storage,
            path: normalized,
            read_state: Arc::new(Mutex::new(ReadState {
                data: Map::new(),
                revision: None,
                reload: None,
            })),
        }
    }

    pub fn path(&self) -> &JsString {
        &self.path
    }

    fn current_revision(&self) -> Option<String> {
        get_file_revision(&encode_node_utf8(&self.path))
    }

    async fn read_latest(&self, options: ModelsStoreOperationOptions) -> Result<StoredModels, StorageError> {
        if options.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
            return Err(StorageError::new("The operation was aborted"));
        }
        let revision = self.current_revision();
        {
            let state = lock(&self.read_state);
            if revision.is_some() && revision == state.revision {
                return Ok(state.data.clone());
            }
        }
        let reload = {
            let mut state = lock(&self.read_state);
            match &state.reload {
                Some(reload) => Arc::clone(reload),
                None => {
                    let reload = Arc::new(Reload {
                        signal: CancellationToken::new(),
                        readers: AtomicUsize::new(0),
                        outcome: Arc::new(Mutex::new(None)),
                        notify: Arc::new(tokio::sync::Notify::new()),
                    });
                    state.reload = Some(Arc::clone(&reload));
                    self.start_reload(&reload);
                    reload
                }
            }
        };
        reload.readers.fetch_add(1, Ordering::SeqCst);
        let outcome = wait_for_reload(&reload, options.signal.as_ref()).await;
        let remaining = reload.readers.fetch_sub(1, Ordering::SeqCst) - 1;
        if remaining == 0 {
            let mut state = lock(&self.read_state);
            if state
                .reload
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &reload))
            {
                state.reload = None;
                reload.signal.cancel();
            }
        }
        outcome
    }

    fn start_reload(&self, reload: &Arc<Reload>) {
        let state = Arc::clone(&self.read_state);
        let path = self.path.clone();
        let storage = Arc::clone(&self.storage);
        let reload = Arc::clone(reload);
        let signal = reload.signal.clone();
        let outcome_state = Arc::clone(&state);
        tokio::spawn(async move {
            let outcome = storage
                .with_lock_async(
                    Box::new(move |current| {
                        let revision = get_file_revision(&path);
                        let parsed = parse_stored_models(current.as_deref());
                        let state = Arc::clone(&state);
                        Box::pin(async move {
                            if let Ok(data) = &parsed {
                                let mut state = lock(&state);
                                state.data = data.clone();
                                state.revision = revision;
                            }
                            Ok(None)
                        })
                    }),
                    AuthOperationOptions::new(Some(signal)),
                )
                .await;
            let parsed = match outcome {
                Ok(()) => Ok(lock(&outcome_state).data.clone()),
                Err(error) => Err(error),
            };
            *lock(&reload.outcome) = Some(parsed);
            reload.notify.notify_waiters();
        });
    }

    async fn mutate(
        &self,
        operation: AsyncLockOperation,
        options: ModelsStoreOperationOptions,
    ) -> Result<Option<StoredModels>, StorageError> {
        let latest: Arc<Mutex<Option<StoredModels>>> = Arc::new(Mutex::new(None));
        let captured = Arc::clone(&latest);
        let inner = operation;
        self.storage
            .with_lock_async(
                Box::new(move |current| {
                    let inner = inner;
                    let captured = Arc::clone(&captured);
                    Box::pin(async move {
                        let next = inner(current).await?;
                        if let Some(next) = &next
                            && let Ok(parsed) = parse_stored_models(Some(next))
                        {
                            *lock(&captured) = Some(parsed);
                        }
                        Ok(next)
                    })
                }),
                AuthOperationOptions::new(options.signal.clone()),
            )
            .await?;
        Ok(latest.lock().unwrap().clone())
    }
}

async fn wait_for_reload(
    reload: &Arc<Reload>,
    signal: Option<&CancellationToken>,
) -> Result<StoredModels, StorageError> {
    loop {
        if let Some(outcome) = lock(&reload.outcome).clone() {
            return outcome;
        }
        let notified = reload.notify.notified();
        if let Some(signal) = signal {
            tokio::select! {
                () = notified => {}
                () = signal.cancelled() => return Err(StorageError::new("The operation was aborted")),
            }
        } else {
            notified.await;
        }
    }
}

impl ModelsStore for FileModelsStore {
    fn read(
        &self,
        provider_id: &str,
        options: ModelsStoreOperationOptions,
    ) -> BoxFuture<'_, Result<Option<ModelsStoreEntry>, AuthError>> {
        let provider_id = provider_id.to_owned();
        Box::pin(async move {
            let data = self
                .read_latest(options.clone())
                .await
                .map_err(|error| AuthError::failed(error.to_string()))?;
            if options.signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                return Err(AuthError::Aborted);
            }
            data.get(&provider_id)
                .map(value_to_entry)
                .transpose()
                .map_err(|error| AuthError::failed(error.to_string()))
        })
    }

    fn write(
        &self,
        provider_id: &str,
        entry: ModelsStoreEntry,
        options: ModelsStoreOperationOptions,
    ) -> BoxFuture<'_, Result<(), AuthError>> {
        let provider_id = provider_id.to_owned();
        Box::pin(async move {
            let value = entry_to_value(&entry).map_err(|error| AuthError::failed(error.to_string()))?;
            let provider = provider_id.clone();
            let latest = self
                .mutate(
                    Box::new(move |current| {
                        let provider = provider.clone();
                        let value = value.clone();
                        Box::pin(async move {
                            let mut data = parse_stored_models(current.as_deref())?;
                            data.insert(provider, value);
                            Ok(Some(js_json::stringify_pretty(&Value::Object(data))))
                        })
                    }),
                    options,
                )
                .await
                .map_err(|error| AuthError::failed(error.to_string()))?;
            if let Some(latest) = latest {
                lock(&self.read_state).data = latest;
            }
            Ok(())
        })
    }

    fn delete(&self, provider_id: &str, options: ModelsStoreOperationOptions) -> BoxFuture<'_, Result<(), AuthError>> {
        let provider_id = provider_id.to_owned();
        Box::pin(async move {
            let latest = self
                .mutate(
                    Box::new(move |current| {
                        let provider = provider_id.clone();
                        Box::pin(async move {
                            let mut data = parse_stored_models(current.as_deref())?;
                            data.shift_remove(&provider);
                            Ok(Some(js_json::stringify_pretty(&Value::Object(data))))
                        })
                    }),
                    options,
                )
                .await
                .map_err(|error| AuthError::failed(error.to_string()))?;
            if let Some(latest) = latest {
                lock(&self.read_state).data = latest;
            }
            Ok(())
        })
    }
}
