//! Port of `packages/coding-agent/src/core/auth-storage.ts`.
//!
//! `auth.json` is shared with the TypeScript runtime, so the file format, the `0600` mode, and the
//! `proper-lockfile` protocol (`<auth.json>.lock`) are part of the contract.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime};

use pi_ai::auth::types::{
    AuthError, AuthOperationOptions, Credential, CredentialInfo, CredentialModifier, CredentialStore,
};
use pi_ai::request::BoxFuture;
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use crate::config::get_agent_dir;
use crate::core::config_value::{is_command_config_value, resolve_config_value};
use crate::utils::lockfile::{LockErrorCode, LockOptions, acquire, acquire_sync};
use crate::utils::paths::{get_file_revision, normalize_path};
use pi_js_compat::js_json;

const AUTH_FILE_MODE: u32 = 0o600;
const SYNC_LOCK_ATTEMPTS: usize = 10;
const SYNC_LOCK_DELAY: Duration = Duration::from_millis(20);
const AUTH_LOCK_STALE_MS: u64 = 30_000;

/// Failure of a storage backend or of parsing `auth.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageError {
    message: String,
    aborted: bool,
    cause: Option<Arc<StorageError>>,
}

impl StorageError {
    /// Creates a non-abort failure with the given message.
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            aborted: false,
            cause: None,
        }
    }

    fn aborted() -> Self {
        Self {
            message: "The operation was aborted".to_owned(),
            aborted: true,
            cause: None,
        }
    }

    pub fn is_aborted(&self) -> bool {
        self.aborted
    }

    fn into_auth_error(self) -> AuthError {
        if self.aborted {
            AuthError::Aborted
        } else {
            AuthError::failed(self.message)
        }
    }
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.cause.as_deref().map(|error| error as &dyn std::error::Error)
    }
}

/// The backend writes `next` (when `Some`) after the operation returns.
pub type LockOperation<'a> = Box<dyn FnOnce(Option<&str>) -> Result<Option<String>, StorageError> + Send + 'a>;

/// Async variant of [`LockOperation`]; receives the current file content.
pub type AsyncLockOperation =
    Box<dyn FnOnce(Option<String>) -> BoxFuture<'static, Result<Option<String>, StorageError>> + Send>;

/// Credential storage backend (`AuthStorageBackend`).
pub trait AuthStorageBackend: Send + Sync {
    /// Runs `operation` while holding the file lock.
    fn with_lock(&self, operation: LockOperation<'_>) -> Result<(), StorageError>;

    /// Runs `operation` while holding the file lock, honoring cancellation.
    fn with_lock_async(
        &self,
        operation: AsyncLockOperation,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<(), StorageError>>;
}

fn check_aborted(signal: Option<&CancellationToken>) -> Result<(), StorageError> {
    if signal.is_some_and(CancellationToken::is_cancelled) {
        Err(StorageError::aborted())
    } else {
        Ok(())
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `FileAuthStorageBackend`: JSON file plus `proper-lockfile`.
pub struct FileAuthStorageBackend {
    auth_path: String,
}

impl FileAuthStorageBackend {
    pub fn new(auth_path: &str) -> Self {
        Self {
            auth_path: normalize_path(auth_path, &Default::default()).unwrap_or_else(|_| auth_path.to_owned()),
        }
    }

    pub fn auth_path(&self) -> &str {
        &self.auth_path
    }

    fn ensure_parent_dir(&self) -> Result<(), StorageError> {
        let Some(directory) = Path::new(&self.auth_path).parent() else {
            return Ok(());
        };
        if directory.exists() {
            return Ok(());
        }
        let mut builder = std::fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(directory).map_err(|error| {
            StorageError::new(pi_js_compat::node_fs::node_fs_error(
                &error,
                "mkdir",
                &directory.to_string_lossy(),
            ))
        })
    }

    fn ensure_file_exists(&self) -> Result<(), StorageError> {
        if Path::new(&self.auth_path).exists() {
            return Ok(());
        }
        write_auth_file(&self.auth_path, "{}")?;
        Ok(())
    }

    fn read_current(&self) -> Result<Option<String>, StorageError> {
        if !Path::new(&self.auth_path).exists() {
            return Ok(None);
        }
        std::fs::read(&self.auth_path)
            .map(|bytes| Some(String::from_utf8_lossy(&bytes).into_owned()))
            .map_err(|error| StorageError::new(pi_js_compat::node_fs::node_read_error(&error, &self.auth_path)))
    }

    fn write_next(&self, next: Option<String>) -> Result<(), StorageError> {
        match next {
            Some(content) => write_auth_file(&self.auth_path, &content),
            None => Ok(()),
        }
    }

    fn acquire_sync_with_retry(&self) -> Result<crate::utils::lockfile::LockGuard, StorageError> {
        let options = LockOptions {
            realpath: false,
            stale_ms: AUTH_LOCK_STALE_MS,
            ..LockOptions::default()
        };
        let mut last_error = None;
        for attempt in 1..=SYNC_LOCK_ATTEMPTS {
            match acquire_sync(Path::new(&self.auth_path), &options) {
                Ok(guard) => return Ok(guard),
                Err(error) if error.code == LockErrorCode::Locked && attempt < SYNC_LOCK_ATTEMPTS => {
                    last_error = Some(error);
                    std::thread::sleep(SYNC_LOCK_DELAY);
                }
                Err(error) => return Err(StorageError::new(error.to_string())),
            }
        }
        Err(StorageError::new(last_error.map_or_else(
            || "Failed to acquire auth storage lock".to_owned(),
            |error| error.to_string(),
        )))
    }

    /// Frozen `proper-lockfile` options: ten retries, randomized 100ms–10s exponential delays.
    async fn acquire_async(
        &self,
        signal: Option<CancellationToken>,
    ) -> Result<(crate::utils::lockfile::LockGuard, Arc<Mutex<Option<String>>>), StorageError> {
        let compromised: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let options = LockOptions {
            realpath: false,
            retries: 0,
            stale_ms: AUTH_LOCK_STALE_MS,
            on_compromised: Some({
                let compromised = Arc::clone(&compromised);
                Arc::new(move |error| {
                    *compromised.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(error.to_string());
                })
            }),
            ..LockOptions::default()
        };
        let mut retry = 0u32;
        loop {
            check_aborted(signal.as_ref())?;
            match acquire(Path::new(&self.auth_path), &options).await {
                Ok(guard) => {
                    if signal.as_ref().is_some_and(CancellationToken::is_cancelled) {
                        let _ = guard.release();
                        return Err(StorageError::aborted());
                    }
                    if let Some(message) = compromised.lock().unwrap().clone() {
                        return Err(StorageError::new(message));
                    }
                    return Ok((guard, compromised));
                }
                Err(error) if error.code == LockErrorCode::Locked && retry < 10 => {
                    let base_delay = 100u64.saturating_mul(1u64 << retry);
                    retry += 1;
                    let delay = jittered_delay(base_delay).min(10_000);
                    if let Some(signal) = &signal {
                        tokio::select! {
                            () = tokio::time::sleep(Duration::from_millis(delay)) => {}
                            () = signal.cancelled() => return Err(StorageError::aborted()),
                        }
                    } else {
                        tokio::time::sleep(Duration::from_millis(delay)).await;
                    }
                }
                Err(error) => {
                    if let Some(message) = compromised.lock().unwrap().clone() {
                        return Err(StorageError::new(message));
                    }
                    return Err(StorageError::new(error.to_string()));
                }
            }
        }
    }
}

/// `Math.round(baseDelay * (1 + Math.random()))`; caller applies the retry cap.
fn jittered_delay(base_ms: u64) -> u64 {
    let entropy = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.subsec_nanos() as u64);
    (base_ms as f64 * (1.0 + (entropy % 1_000_000) as f64 / 1_000_000.0)).round() as u64
}

impl AuthStorageBackend for FileAuthStorageBackend {
    fn with_lock(&self, operation: LockOperation<'_>) -> Result<(), StorageError> {
        self.ensure_parent_dir()?;
        self.ensure_file_exists()?;
        let guard = self.acquire_sync_with_retry()?;
        let current = self.read_current()?;
        let next = operation(current.as_deref())?;
        self.write_next(next)?;
        guard.release().map_err(|error| StorageError::new(error.to_string()))
    }

    fn with_lock_async(
        &self,
        operation: AsyncLockOperation,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<(), StorageError>> {
        let signal = options.signal.clone();
        Box::pin(async move {
            check_aborted(signal.as_ref())?;
            self.ensure_parent_dir()?;
            self.ensure_file_exists()?;
            let (guard, compromised) = self.acquire_async(signal.clone()).await?;
            let check_compromised = || match lock(&compromised).clone() {
                Some(message) => Err(StorageError::new(message)),
                None => Ok(()),
            };
            let outcome = async {
                check_aborted(signal.as_ref())?;
                check_compromised()?;
                let current = self.read_current()?;
                let next = operation(current).await?;
                check_compromised()?;
                check_aborted(signal.as_ref())?;
                self.write_next(next)?;
                check_compromised()
            }
            .await;
            let _ = guard.release();
            outcome
        })
    }
}

fn write_auth_file(path: &str, content: &str) -> Result<(), StorageError> {
    use std::io::Write;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(AUTH_FILE_MODE);
    }
    options
        .open(path)
        .and_then(|mut file| file.write_all(content.as_bytes()))
        .map_err(|error| StorageError::new(pi_js_compat::node_fs::node_fs_error(&error, "open", path)))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(AUTH_FILE_MODE))
            .map_err(|error| StorageError::new(pi_js_compat::node_fs::node_fs_error(&error, "chmod", path)))?;
    }
    Ok(())
}

/// `InMemoryAuthStorageBackend`: one JSON string guarded by an async chain.
#[derive(Default)]
pub struct InMemoryAuthStorageBackend {
    value: Mutex<Option<String>>,
    chain: tokio::sync::Mutex<()>,
}

impl InMemoryAuthStorageBackend {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_value(&self, value: Option<String>) {
        *self.value.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = value;
    }
}

impl AuthStorageBackend for InMemoryAuthStorageBackend {
    fn with_lock(&self, operation: LockOperation<'_>) -> Result<(), StorageError> {
        let current = self
            .value
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let next = operation(current.as_deref())?;
        if let Some(next) = next {
            self.set_value(Some(next));
        }
        Ok(())
    }

    fn with_lock_async(
        &self,
        operation: AsyncLockOperation,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<(), StorageError>> {
        Box::pin(async move {
            let _guard = self.chain.lock().await;
            check_aborted(options.signal.as_ref())?;
            let current = self
                .value
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            let next = operation(current).await?;
            check_aborted(options.signal.as_ref())?;
            if let Some(next) = next {
                self.set_value(Some(next));
            }
            Ok(())
        })
    }
}

pub type AuthStorageData = Map<String, Value>;

fn parse_auth_data(content: Option<&str>) -> Result<AuthStorageData, StorageError> {
    let Some(content) = content.filter(|content| !content.is_empty()) else {
        return Ok(Map::new());
    };
    match js_json::parse(content).map_err(|error| StorageError::new(error.to_string()))? {
        Value::Object(object) => Ok(object),
        _ => Ok(Map::new()),
    }
}

/// Merge raw UTF-16 JSON before encoding; untouched providers/keys must not cross serde's lossy string boundary.
fn serialize_provider_change(
    current: Option<&str>,
    provider: &str,
    credential: Option<&Value>,
) -> Result<String, StorageError> {
    let parsed = match current.filter(|text| !text.is_empty()) {
        Some(text) => js_json::parse_lossless(text).map_err(|error| StorageError::new(error.to_string()))?,
        None => js_json::JsValue::Object(js_json::JsObject::new()),
    };
    let mut object = match parsed {
        js_json::JsValue::Object(object) => object,
        _ => js_json::JsObject::new(),
    };
    let key = js_json::JsString::from(provider);
    if let Some(credential) = credential {
        let encoded = serde_json::to_string(credential).map_err(|error| StorageError::new(error.to_string()))?;
        object.insert(
            key,
            js_json::parse_lossless(&encoded).map_err(|error| StorageError::new(error.to_string()))?,
        );
    } else {
        object.shift_remove(&key);
    }
    Ok(js_json::stringify_lossless_pretty(&js_json::JsValue::Object(object)))
}

fn source_now_millis() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as f64
}
fn oauth_value(credential: pi_ai::oauth::OAuthCredentials) -> Result<Value, StorageError> {
    serde_json::to_value(Credential::OAuth(credential)).map_err(|error| StorageError::new(error.to_string()))
}
fn json_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(n) => n.as_f64().is_some_and(|n| n != 0.0),
        Value::String(s) => !s.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn credential_type(value: &Value) -> Option<&str> {
    value.get("type").and_then(Value::as_str)
}

/// Strict validation used by [`ReadOnlyAuthStorage`], mirroring the source messages.
fn validate_credential(value: &Value, provider_id: &str) -> Result<(), String> {
    let Some(object) = value.as_object() else {
        return Err(format!("Invalid auth.json credential for provider \"{provider_id}\""));
    };
    match object.get("type").and_then(Value::as_str) {
        Some("api_key") => {
            let valid_key = object.get("key").is_none_or(Value::is_string);
            let valid_env = object.get("env").is_none_or(|env| {
                env.as_object()
                    .is_some_and(|values| values.values().all(Value::is_string))
            });
            if valid_key && valid_env {
                Ok(())
            } else {
                Err(format!("Invalid auth.json credential for provider \"{provider_id}\""))
            }
        }
        Some("oauth") => {
            let access = object.get("access").is_some_and(Value::is_string);
            let refresh = object.get("refresh").is_some_and(Value::is_string);
            let expires = object
                .get("expires")
                .and_then(Value::as_f64)
                .is_some_and(f64::is_finite);
            if access && refresh && expires {
                Ok(())
            } else {
                Err(format!("Invalid auth.json credential for provider \"{provider_id}\""))
            }
        }
        _ => Err(format!("Invalid auth.json credential for provider \"{provider_id}\"")),
    }
}

fn to_credential(value: &Value) -> Result<Credential, StorageError> {
    serde_json::from_value(value.clone())
        .map_err(|error| StorageError::new(format!("Invalid auth.json credential: {error}")))
}

/// Credential storage that cannot modify `auth.json`.
pub struct ReadOnlyAuthStorage {
    auth_path: String,
    data: Mutex<Option<AuthStorageData>>,
}

impl ReadOnlyAuthStorage {
    pub fn new(auth_path: Option<&str>) -> Self {
        let auth_path = auth_path
            .map(str::to_owned)
            .unwrap_or_else(|| get_agent_dir().join("auth.json").display().to_string());
        Self {
            auth_path: normalize_path(&auth_path, &Default::default()).unwrap_or(auth_path),
            data: Mutex::new(None),
        }
    }

    fn load(&self) -> Result<AuthStorageData, StorageError> {
        let mut data = self.data.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(data) = data.as_ref() {
            return Ok(data.clone());
        }
        let parsed = match std::fs::read_to_string(&self.auth_path) {
            Ok(content) => js_json::parse(&content)
                .map_err(|error| StorageError::new(format!("Failed to read auth.json: {error}")))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Value::Object(Map::new()),
            Err(error) => {
                return Err(StorageError::new(format!("Failed to read auth.json: {error}")));
            }
        };
        let Value::Object(object) = parsed else {
            return Err(StorageError::new("Invalid auth.json: expected an object"));
        };
        for (provider_id, credential) in &object {
            validate_credential(credential, provider_id).map_err(StorageError::new)?;
        }
        *data = Some(object.clone());
        Ok(object)
    }
}

impl CredentialStore for ReadOnlyAuthStorage {
    fn read(
        &self,
        provider_id: &str,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        let provider_id = provider_id.to_owned();
        Box::pin(async move {
            check_aborted(options.signal.as_ref()).map_err(StorageError::into_auth_error)?;
            let data = self.load().map_err(StorageError::into_auth_error)?;
            check_aborted(options.signal.as_ref()).map_err(StorageError::into_auth_error)?;
            data.get(&provider_id)
                .map(to_credential)
                .transpose()
                .map(|credential| credential.map(|credential| resolved_read_only_credential(&credential)))
                .map_err(StorageError::into_auth_error)
        })
    }

    fn list(&self, options: AuthOperationOptions) -> BoxFuture<'_, Result<Vec<CredentialInfo>, AuthError>> {
        Box::pin(async move {
            check_aborted(options.signal.as_ref()).map_err(StorageError::into_auth_error)?;
            let data = self.load().map_err(StorageError::into_auth_error)?;
            check_aborted(options.signal.as_ref()).map_err(StorageError::into_auth_error)?;
            Ok(js_json::object_entries(&data)
                .into_iter()
                .filter_map(|(provider_id, credential)| {
                    credential_type(credential).map(|credential_type| CredentialInfo {
                        provider_id: provider_id.clone(),
                        credential_type: if credential_type == "oauth" {
                            pi_ai::auth::types::CredentialType::Oauth
                        } else {
                            pi_ai::auth::types::CredentialType::ApiKey
                        },
                    })
                })
                .collect())
        })
    }

    fn modify(
        &self,
        _provider_id: &str,
        _modifier: CredentialModifier,
        _options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        Box::pin(async move {
            Err(AuthError::failed(
                "Read-only credential storage cannot modify auth.json",
            ))
        })
    }

    fn delete(&self, _provider_id: &str, _options: AuthOperationOptions) -> BoxFuture<'_, Result<(), AuthError>> {
        Box::pin(async move {
            Err(AuthError::failed(
                "Read-only credential storage cannot modify auth.json",
            ))
        })
    }
}

struct Reload {
    signal: CancellationToken,
    readers: AtomicUsize,
    outcome: Arc<Mutex<Option<Result<AuthStorageData, StorageError>>>>,
    notify: Arc<tokio::sync::Notify>,
}

struct ReadState {
    data: AuthStorageData,
    revision: Option<String>,
    reload: Option<Arc<Reload>>,
}

type SharedReadState = Option<(String, Arc<Mutex<ReadState>>)>;

static SHARED_READ_STATE: OnceLock<Mutex<SharedReadState>> = OnceLock::new();

/// Credential storage backed by a JSON file (`AuthStorage`).
#[derive(Default)]
struct SourceAuthState {
    data: AuthStorageData,
    runtime_overrides: std::collections::HashMap<String, String>,
    load_error: Option<StorageError>,
    errors: Vec<StorageError>,
}

#[derive(Clone, Debug)]
pub struct GetApiKeyOptions {
    pub include_fallback: bool,
    pub throw_on_refresh_failure: bool,
    /// Native cancellation boundary; frozen TS AuthStorage has no cancellation option.
    pub signal: Option<CancellationToken>,
}
impl Default for GetApiKeyOptions {
    fn default() -> Self {
        Self {
            include_fallback: true,
            throw_on_refresh_failure: false,
            signal: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct AuthStatus {
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

pub struct AuthStorage {
    source_state: Arc<Mutex<SourceAuthState>>,
    storage: Arc<dyn AuthStorageBackend>,
    auth_path: Option<String>,
    read_state: Arc<Mutex<ReadState>>,
}

impl AuthStorage {
    pub fn create(auth_path: Option<&str>) -> Self {
        let path = auth_path
            .map(str::to_owned)
            .unwrap_or_else(|| get_agent_dir().join("auth.json").display().to_string());
        let normalized = normalize_path(&path, &Default::default()).unwrap_or(path);
        Self::from_storage_at(Arc::new(FileAuthStorageBackend::new(&normalized)), Some(normalized))
    }

    pub fn from_storage(storage: Arc<dyn AuthStorageBackend>) -> Self {
        Self::from_storage_at(storage, None)
    }

    /// Builds a store over `storage` whose shared read state is keyed by `auth_path`.
    pub fn from_storage_at(storage: Arc<dyn AuthStorageBackend>, auth_path: Option<String>) -> Self {
        let read_state = match &auth_path {
            Some(path) => {
                let shared = SHARED_READ_STATE.get_or_init(|| Mutex::new(None));
                let mut guard = shared.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                match guard.as_ref() {
                    Some((shared_path, state)) if shared_path == path => Arc::clone(state),
                    _ => {
                        let state = Arc::new(Mutex::new(ReadState {
                            data: Map::new(),
                            revision: None,
                            reload: None,
                        }));
                        *guard = Some((path.clone(), Arc::clone(&state)));
                        state
                    }
                }
            }
            None => Arc::new(Mutex::new(ReadState {
                data: Map::new(),
                revision: None,
                reload: None,
            })),
        };
        let source_data = lock(&read_state).data.clone();
        let store = Self {
            source_state: Arc::new(Mutex::new(SourceAuthState {
                data: source_data,
                ..Default::default()
            })),
            storage,
            auth_path,
            read_state,
        };
        if store.auth_path.is_some() {
            let revision = store.current_revision();
            let state = store.read_state.clone();
            let already_current = revision_matches(&state, revision.as_deref());
            if !already_current {
                store.reload();
            }
        } else {
            store.reload();
        }
        store
    }

    pub fn in_memory(data: AuthStorageData) -> Self {
        let backend = Arc::new(InMemoryAuthStorageBackend::new());
        backend.set_value(Some(js_json::stringify_pretty(&Value::Object(data))));
        Self::from_storage(backend)
    }

    /// Frozen public API: raw stored credential, without config resolution or refresh.
    pub fn get(&self, provider: &str) -> Option<Value> {
        lock(&self.source_state).data.get(provider).cloned()
    }
    pub fn get_all(&self) -> AuthStorageData {
        lock(&self.source_state).data.clone()
    }
    /// Named separately until obsolete CredentialStore::list(options) consumers are removed.
    pub fn list_providers(&self) -> Vec<String> {
        ordered_provider_ids(&lock(&self.source_state).data)
    }
    pub fn has(&self, provider: &str) -> bool {
        lock(&self.source_state).data.contains_key(provider)
    }
    pub fn get_provider_env(&self, provider: &str) -> Option<std::collections::HashMap<String, String>> {
        let credential = self.get(provider)?;
        if credential_type(&credential) != Some("api_key") {
            return None;
        }
        serde_json::from_value(credential.get("env")?.clone()).ok()
    }
    pub fn set_runtime_api_key(&self, provider: &str, api_key: &str) {
        lock(&self.source_state)
            .runtime_overrides
            .insert(provider.to_owned(), api_key.to_owned());
    }
    pub fn remove_runtime_api_key(&self, provider: &str) {
        lock(&self.source_state).runtime_overrides.remove(provider);
    }
    pub fn has_auth(&self, provider: &str) -> bool {
        let source = lock(&self.source_state);
        source.runtime_overrides.contains_key(provider)
            || source.data.get(provider).is_some_and(json_truthy)
            || pi_ai::env_api_keys::get_env_api_key(provider, None).is_some()
    }
    pub fn get_auth_status(&self, provider: &str) -> AuthStatus {
        let source = lock(&self.source_state);
        if source.data.get(provider).is_some_and(json_truthy) {
            return AuthStatus {
                configured: true,
                source: Some("stored".to_owned()),
                label: None,
            };
        }
        if source.runtime_overrides.contains_key(provider) {
            return AuthStatus {
                configured: false,
                source: Some("runtime".to_owned()),
                label: Some("--api-key".to_owned()),
            };
        }
        if let Some(label) = pi_ai::env_api_keys::find_env_keys(provider, None).and_then(|keys| keys.into_iter().next())
        {
            return AuthStatus {
                configured: false,
                source: Some("environment".to_owned()),
                label: Some(label),
            };
        }
        AuthStatus {
            configured: false,
            source: None,
            label: None,
        }
    }
    fn record_error(&self, error: StorageError) {
        lock(&self.source_state).errors.push(error);
    }
    pub fn drain_errors(&self) -> Vec<StorageError> {
        std::mem::take(&mut lock(&self.source_state).errors)
    }
    fn persist_provider_change(
        &self,
        provider: &str,
        credential: Option<Value>,
    ) -> Result<AuthStorageData, StorageError> {
        if lock(&self.source_state).load_error.is_some() {
            self.reload();
        }
        let load_error = lock(&self.source_state).load_error.clone();
        if let Some(error) = load_error {
            let error = StorageError::new(format!(
                "Cannot update auth storage because it could not be loaded: {error}"
            ));
            self.record_error(error.clone());
            return Err(error);
        }
        let mut persisted = None;
        let result = self.storage.with_lock(Box::new(|current| {
            let content = serialize_provider_change(current, provider, credential.as_ref())?;
            let mut merged = parse_auth_data(current)?;
            if let Some(credential) = credential {
                merged.insert(provider.to_owned(), credential);
            } else {
                merged.shift_remove(provider);
            }
            persisted = Some(merged);
            Ok(Some(content))
        }));
        if let Err(error) = result {
            self.record_error(error.clone());
            return Err(error);
        }
        lock(&self.source_state).load_error = None;
        Ok(persisted.expect("successful backend must call operation"))
    }
    pub fn set(&self, provider: &str, credential: Value) -> Result<(), StorageError> {
        let persisted = self.persist_provider_change(provider, Some(credential))?;
        lock(&self.source_state).data = persisted;
        Ok(())
    }
    pub fn remove(&self, provider: &str) -> Result<(), StorageError> {
        let persisted = self.persist_provider_change(provider, None)?;
        lock(&self.source_state).data = persisted;
        Ok(())
    }
    pub fn logout(&self, provider: &str) -> Result<(), StorageError> {
        self.remove(provider)
    }
    pub async fn login(
        &self,
        provider_id: &str,
        callbacks: Arc<dyn pi_ai::oauth::OAuthLoginCallbacks>,
    ) -> Result<(), StorageError> {
        let provider = pi_ai::oauth::get_oauth_provider(provider_id)
            .ok_or_else(|| StorageError::new(format!("Unknown OAuth provider: {provider_id}")))?;
        let credential = provider
            .login(callbacks)
            .await
            .map_err(|error| StorageError::new(error.to_string()))?;
        self.set(provider_id, oauth_value(credential)?)
    }
    pub fn get_oauth_providers(&self) -> Vec<Arc<dyn pi_ai::oauth::OAuthProviderInterface>> {
        pi_ai::oauth::get_oauth_providers()
    }
    async fn refresh_oauth_token_with_lock(
        &self,
        provider_id: &str,
        options: &GetApiKeyOptions,
    ) -> Result<Option<pi_ai::oauth::OAuthApiKey>, StorageError> {
        let Some(provider) = pi_ai::oauth::get_oauth_provider(provider_id) else {
            return Ok(None);
        };
        let rotated = Arc::new(Mutex::new(None));
        let captured = rotated.clone();
        let result = Arc::new(Mutex::new(None));
        let captured_result = result.clone();
        let source = Arc::clone(&self.source_state);
        let id = provider_id.to_owned();
        let refresh = self
            .storage
            .with_lock_async(
                Box::new(move |current| {
                    Box::pin(async move {
                        let mut data = parse_auth_data(current.as_deref())?;
                        {
                            let mut source = lock(&source);
                            source.data = data.clone();
                            source.load_error = None;
                        }
                        let Some(value) = data.get(&id) else { return Ok(None) };
                        if credential_type(value) != Some("oauth") {
                            return Ok(None);
                        }
                        let credential = to_credential(value)?.into_oauth().expect("checked OAuth");
                        if source_now_millis() < credential.expires {
                            *lock(&captured_result) = Some(pi_ai::oauth::OAuthApiKey {
                                api_key: provider.get_api_key(&credential),
                                new_credentials: credential,
                            });
                            return Ok(None);
                        }
                        let credentials = data
                            .iter()
                            .filter(|(_, v)| credential_type(v) == Some("oauth"))
                            .map(|(id, v)| {
                                to_credential(v).map(|c| (id.clone(), c.into_oauth().expect("checked OAuth")))
                            })
                            .collect::<Result<std::collections::HashMap<_, _>, _>>()?;
                        let Some(refreshed) = pi_ai::oauth::get_oauth_api_key(&id, &credentials)
                            .await
                            .map_err(|error| StorageError::new(error.to_string()))?
                        else {
                            return Ok(None);
                        };
                        *lock(&captured) = Some(refreshed.clone());
                        let next_credential = oauth_value(refreshed.new_credentials.clone())?;
                        let content = serialize_provider_change(current.as_deref(), &id, Some(&next_credential))?;
                        data.insert(id, next_credential);
                        {
                            let mut source = lock(&source);
                            source.data = data.clone();
                            source.load_error = None;
                        }
                        *lock(&captured_result) = Some(refreshed);
                        Ok(Some(content))
                    })
                }),
                AuthOperationOptions {
                    signal: options.signal.clone(),
                },
            )
            .await;
        match refresh {
            Ok(()) => Ok(lock(&result).take()),
            Err(error) => {
                let Some(rotated) = lock(&rotated).take() else {
                    return Err(error);
                };
                self.record_error(error);
                self.set(provider_id, oauth_value(rotated.new_credentials.clone())?)?;
                Ok(Some(rotated))
            }
        }
    }
    pub async fn get_api_key(
        &self,
        provider_id: &str,
        options: GetApiKeyOptions,
    ) -> Result<Option<String>, StorageError> {
        let runtime = lock(&self.source_state).runtime_overrides.get(provider_id).cloned();
        if runtime.as_deref().is_some_and(|key| !key.is_empty()) {
            return Ok(runtime);
        }
        if let Some(credential) = self.get(provider_id) {
            if credential_type(&credential) == Some("api_key") {
                let key = credential.get("key").and_then(Value::as_str);
                return Ok(key.and_then(|key| resolve_config_value(key, self.get_provider_env(provider_id).as_ref())));
            }
            if credential_type(&credential) == Some("oauth") {
                let Some(provider) = pi_ai::oauth::get_oauth_provider(provider_id) else {
                    return Ok(None);
                };
                let credential = to_credential(&credential)?.into_oauth().expect("checked OAuth");
                if source_now_millis() < credential.expires {
                    return Ok(Some(provider.get_api_key(&credential)));
                }
                match self.refresh_oauth_token_with_lock(provider_id, &options).await {
                    Ok(Some(result)) => return Ok(Some(result.api_key)),
                    Ok(None) => {}
                    Err(error) => {
                        self.record_error(error.clone());
                        self.reload();
                        let reloaded = lock(&self.source_state).load_error.is_none();
                        if let Some(updated) = self.get(provider_id).filter(|_| reloaded) {
                            if credential_type(&updated) == Some("oauth") {
                                let updated = to_credential(&updated)?.into_oauth().expect("checked OAuth");
                                if source_now_millis() < updated.expires {
                                    return Ok(Some(provider.get_api_key(&updated)));
                                }
                            }
                        }
                        if options.throw_on_refresh_failure {
                            let mut wrapped = StorageError::new(format!(
                                "Authentication failed for \"{provider_id}\": {error}. Run '/login {provider_id}' to re-authenticate."
                            ));
                            wrapped.cause = Some(Arc::new(error));
                            return Err(wrapped);
                        }
                        return Ok(None);
                    }
                }
            }
        }
        if !options.include_fallback {
            return Ok(None);
        }
        Ok(pi_ai::env_api_keys::get_env_api_key(provider_id, None))
    }

    fn current_revision(&self) -> Option<String> {
        self.auth_path.as_deref().and_then(get_file_revision)
    }

    /// Reloads credentials from storage, preserving the last valid snapshot on failure.
    pub fn reload(&self) {
        let state = Arc::clone(&self.read_state);
        let auth_path = self.auth_path.clone();
        let storage = Arc::clone(&self.storage);
        let outcome = Arc::new(Mutex::new(None));
        let captured = Arc::clone(&outcome);
        let result = storage.with_lock(Box::new(move |current| {
            let revision = auth_path.as_deref().and_then(get_file_revision);
            let parsed = parse_auth_data(current);
            *captured.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((parsed, revision));
            Ok(None)
        }));
        if let Err(error) = result {
            let mut source = lock(&self.source_state);
            source.load_error = Some(error.clone());
            source.errors.push(error);
            return;
        }
        let captured = outcome.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
        if let Some((parsed, revision)) = captured {
            match parsed {
                Ok(data) => {
                    let mut source = lock(&self.source_state);
                    source.data = data.clone();
                    source.load_error = None;
                    let mut state = lock(&state);
                    state.data = data;
                    state.revision = revision;
                }
                Err(error) => {
                    let mut source = lock(&self.source_state);
                    source.load_error = Some(error.clone());
                    source.errors.push(error);
                }
            }
        }
    }

    fn reload_from_storage_async(
        &self,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<AuthStorageData, StorageError>> {
        let state = Arc::clone(&self.read_state);
        let auth_path = self.auth_path.clone();
        let storage = Arc::clone(&self.storage);
        let outcome: Arc<Mutex<Option<Result<AuthStorageData, StorageError>>>> = Arc::new(Mutex::new(None));
        let captured = Arc::clone(&outcome);
        Box::pin(async move {
            storage
                .with_lock_async(
                    Box::new(move |current| {
                        let revision = auth_path.as_deref().and_then(get_file_revision);
                        let parsed = parse_auth_data(current.as_deref());
                        *captured.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(parsed.clone());
                        Box::pin(async move {
                            let mut state = lock(&state);
                            if let Ok(data) = &parsed {
                                state.data = data.clone();
                                state.revision = revision;
                            }
                            Ok(None)
                        })
                    }),
                    options,
                )
                .await?;
            outcome
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take()
                .unwrap_or_else(|| Ok(Map::new()))
        })
    }

    async fn read_latest_data(&self, options: AuthOperationOptions) -> Result<AuthStorageData, StorageError> {
        check_aborted(options.signal.as_ref())?;
        let Some(_) = &self.auth_path else {
            let reload = self.reload_from_storage_async(options.clone());
            return match &options.signal {
                Some(_) => reload.await,
                None => reload.await.or_else(|_| Ok(lock(&self.read_state).data.clone())),
            };
        };
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
                    start_reload(self, &reload);
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
}

fn revision_matches(state: &Arc<Mutex<ReadState>>, revision: Option<&str>) -> bool {
    match revision {
        Some(revision) => lock(state).revision.as_deref() == Some(revision),
        None => false,
    }
}

fn start_reload(store: &AuthStorage, reload: &Arc<Reload>) {
    let state = Arc::clone(&store.read_state);
    let auth_path = store.auth_path.clone();
    let storage = Arc::clone(&store.storage);
    let reload = Arc::clone(reload);
    let signal = reload.signal.clone();
    let outcome_state = Arc::clone(&state);
    tokio::spawn(async move {
        let outcome = storage
            .with_lock_async(
                Box::new(move |current| {
                    let revision = auth_path.as_deref().and_then(get_file_revision);
                    let parsed = parse_auth_data(current.as_deref());
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
        *reload.outcome.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(parsed);
        reload.notify.notify_waiters();
    });
}

async fn wait_for_reload(
    reload: &Arc<Reload>,
    signal: Option<&CancellationToken>,
) -> Result<AuthStorageData, StorageError> {
    loop {
        if let Some(outcome) = reload
            .outcome
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
        {
            return outcome;
        }
        let notified = reload.notify.notified();
        if let Some(signal) = signal {
            tokio::select! {
                () = notified => {}
                () = signal.cancelled() => return Err(StorageError::aborted()),
            }
        } else {
            notified.await;
        }
    }
}

fn resolved_credential(credential: &Credential) -> Credential {
    let Credential::ApiKey(api_key) = credential else {
        return credential.clone();
    };
    let Some(key) = &api_key.key else {
        return credential.clone();
    };
    let env: std::collections::HashMap<String, String> = api_key
        .env
        .as_ref()
        .map(|env| env.iter().map(|(name, value)| (name.clone(), value.clone())).collect())
        .unwrap_or_default();
    Credential::ApiKey(pi_ai::auth::types::ApiKeyCredential {
        key: resolve_config_value(key, Some(&env)),
        env: api_key.env.clone(),
    })
}

/// `ReadOnlyAuthStorage.read` resolves templates but never executes command references.
fn resolved_read_only_credential(credential: &Credential) -> Credential {
    let Credential::ApiKey(api_key) = credential else {
        return credential.clone();
    };
    let Some(key) = &api_key.key else {
        return credential.clone();
    };
    if is_command_config_value(key) {
        return credential.clone();
    }
    resolved_credential(credential)
}

impl CredentialStore for AuthStorage {
    fn read(
        &self,
        provider_id: &str,
        options: AuthOperationOptions,
    ) -> BoxFuture<'_, Result<Option<Credential>, AuthError>> {
        let provider_id = provider_id.to_owned();
        Box::pin(async move {
            let data = self
                .read_latest_data(options.clone())
                .await
                .map_err(StorageError::into_auth_error)?;
            check_aborted(options.signal.as_ref()).map_err(StorageError::into_auth_error)?;
            let Some(value) = data.get(&provider_id) else {
                return Ok(None);
            };
            let credential = to_credential(value).map_err(StorageError::into_auth_error)?;
            Ok(Some(resolved_credential(&credential)))
        })
    }

    fn list(&self, options: AuthOperationOptions) -> BoxFuture<'_, Result<Vec<CredentialInfo>, AuthError>> {
        Box::pin(async move {
            let data = self
                .read_latest_data(options.clone())
                .await
                .map_err(StorageError::into_auth_error)?;
            check_aborted(options.signal.as_ref()).map_err(StorageError::into_auth_error)?;
            Ok(js_json::object_entries(&data)
                .into_iter()
                .filter_map(|(provider_id, credential)| {
                    credential_type(credential).map(|credential_type| CredentialInfo {
                        provider_id: provider_id.clone(),
                        credential_type: if credential_type == "oauth" {
                            pi_ai::auth::types::CredentialType::Oauth
                        } else {
                            pi_ai::auth::types::CredentialType::ApiKey
                        },
                    })
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
        let provider_id = provider_id.to_owned();
        let storage = Arc::clone(&self.storage);
        let state = Arc::clone(&self.read_state);
        let auth_path = self.auth_path.clone();
        Box::pin(async move {
            type ModifyOutcome = Option<Result<Option<Credential>, StorageError>>;
            let outcome: Arc<Mutex<ModifyOutcome>> = Arc::new(Mutex::new(None));
            let latest: Arc<Mutex<Option<AuthStorageData>>> = Arc::new(Mutex::new(None));
            let captured_outcome = Arc::clone(&outcome);
            let captured_latest = Arc::clone(&latest);
            let provider = provider_id.clone();
            let revision: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
            let captured_revision = Arc::clone(&revision);
            storage
                .with_lock_async(
                    Box::new(move |current| {
                        let provider = provider.clone();
                        let modifier = Arc::clone(&modifier);
                        let captured_outcome = Arc::clone(&captured_outcome);
                        let captured_latest = Arc::clone(&captured_latest);
                        let captured_revision = Arc::clone(&captured_revision);
                        let auth_path = auth_path.clone();
                        Box::pin(async move {
                            let mut current_data = parse_auth_data(current.as_deref())?;
                            let current_credential = current_data.get(&provider).map(to_credential).transpose()?;
                            let next = modifier(current_credential.clone())
                                .await
                                .map_err(|error| match error {
                                    AuthError::Aborted => StorageError::aborted(),
                                    AuthError::Failed(error) => StorageError::new(error.message),
                                })?;
                            let Some(next) = next else {
                                *captured_latest.lock().unwrap() = Some(current_data);
                                *captured_revision.lock().unwrap() = auth_path.as_deref().and_then(get_file_revision);
                                *captured_outcome.lock().unwrap() = Some(Ok(current_credential));
                                return Ok(None);
                            };
                            let next_value =
                                serde_json::to_value(&next).map_err(|error| StorageError::new(error.to_string()))?;
                            current_data.insert(provider, next_value);
                            *captured_latest.lock().unwrap() = Some(current_data.clone());
                            *captured_outcome.lock().unwrap() = Some(Ok(Some(next)));
                            Ok(Some(js_json::stringify_pretty(&Value::Object(current_data))))
                        })
                    }),
                    options.clone(),
                )
                .await
                .map_err(StorageError::into_auth_error)?;
            if let Some(latest) = latest.lock().unwrap().clone() {
                let mut state = lock(&state);
                state.data = latest;
                state.revision = revision.lock().unwrap().clone();
            }
            outcome
                .lock()
                .unwrap()
                .take()
                .unwrap_or(Ok(None))
                .map_err(StorageError::into_auth_error)
        })
    }

    fn delete(&self, provider_id: &str, options: AuthOperationOptions) -> BoxFuture<'_, Result<(), AuthError>> {
        let provider_id = provider_id.to_owned();
        let storage = Arc::clone(&self.storage);
        let state = Arc::clone(&self.read_state);
        Box::pin(async move {
            let latest: Arc<Mutex<Option<AuthStorageData>>> = Arc::new(Mutex::new(None));
            let captured_latest = Arc::clone(&latest);
            storage
                .with_lock_async(
                    Box::new(move |current| {
                        let provider = provider_id.clone();
                        let captured_latest = Arc::clone(&captured_latest);
                        Box::pin(async move {
                            let mut current_data = parse_auth_data(current.as_deref())?;
                            current_data.shift_remove(&provider);
                            *captured_latest.lock().unwrap() = Some(current_data.clone());
                            Ok(Some(js_json::stringify_pretty(&Value::Object(current_data))))
                        })
                    }),
                    options.clone(),
                )
                .await
                .map_err(StorageError::into_auth_error)?;
            if let Some(latest) = latest.lock().unwrap().clone() {
                lock(&state).data = latest;
            }
            Ok(())
        })
    }
}

/// One-off synchronous read of a stored credential without resolving configured values.
pub fn read_stored_credential(provider_id: &str, auth_path: Option<&str>) -> Option<Credential> {
    let path = auth_path
        .map(str::to_owned)
        .unwrap_or_else(|| get_agent_dir().join("auth.json").display().to_string());
    let normalized = normalize_path(&path, &Default::default()).unwrap_or(path);
    let content = std::fs::read_to_string(normalized).ok()?;
    let parsed = js_json::parse(&content).ok()?;
    let object = parsed.as_object()?;
    object.get(provider_id).and_then(|value| to_credential(value).ok())
}

/// Provider ids in JavaScript object order (`Object.entries`).
pub fn ordered_provider_ids(data: &AuthStorageData) -> Vec<String> {
    js_json::object_entries(data)
        .into_iter()
        .map(|(provider_id, _)| provider_id.clone())
        .collect()
}

/// Credential map from `auth.json` content.
pub fn parse_auth_storage_data(content: &str) -> Result<AuthStorageData, StorageError> {
    parse_auth_data(Some(content))
}

/// Path helper used by callers that need the default `auth.json` location.
pub fn default_auth_path() -> PathBuf {
    get_agent_dir().join("auth.json")
}
