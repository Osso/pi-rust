//! Port of `packages/coding-agent/src/core/auth-storage.ts`.
//!
//! `auth.json` is shared with the TypeScript runtime, so the file format, the `0600` mode, and the
//! `proper-lockfile` protocol (`<auth.json>.lock`) are part of the contract.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use pi_ai::auth::types::{AuthError, Credential};
use pi_ai::request::BoxFuture;
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};

use crate::config::get_agent_dir;
use crate::core::config_value::resolve_config_value;
use crate::utils::lockfile::{LockErrorCode, LockOptions, acquire, acquire_sync};
use crate::utils::paths::{node_dirname, normalize_path};
use pi_js_compat::{js_format, js_json};

const AUTH_FILE_MODE: u32 = 0o600;
const SYNC_LOCK_ATTEMPTS: usize = 10;
const SYNC_LOCK_DELAY: Duration = Duration::from_millis(20);
const AUTH_LOCK_STALE_MS: u64 = 30_000;

/// Failure of a storage backend or of parsing `auth.json`.
#[derive(Debug, Clone, PartialEq)]
pub enum StorageError {
    Failure {
        error: pi_ai::request::CallbackError,
        cause: Option<Arc<StorageError>>,
    },
    Aborted(pi_ai::request::CallbackError),
}

impl StorageError {
    pub fn new(message: impl Into<JsString>) -> Self {
        Self::from_callback(pi_ai::request::CallbackError::new(message))
    }
    fn callback(&self) -> &pi_ai::request::CallbackError {
        match self {
            Self::Failure { error, .. } | Self::Aborted(error) => error,
        }
    }
    pub fn message(&self) -> &JsString {
        &self.callback().message
    }
    pub fn thrown_value(&self) -> pi_js_compat::abort::AbortReason {
        self.callback().thrown_value()
    }
    fn from_callback(error: pi_ai::request::CallbackError) -> Self {
        Self::Failure { error, cause: None }
    }
    pub fn into_callback(self) -> pi_ai::request::CallbackError {
        match self {
            Self::Failure { error, .. } | Self::Aborted(error) => error,
        }
    }
    fn from_auth(error: AuthError) -> Self {
        match error {
            AuthError::Aborted(error) => Self::Aborted(*error),
            AuthError::Failed(error) => Self::from_callback(*error),
        }
    }
    /// Filesystem text has already crossed the Node byte boundary.
    fn filesystem(message: JsString) -> Self {
        Self::new(message)
    }
    pub fn is_aborted(&self) -> bool {
        matches!(self, Self::Aborted(_))
    }
}

impl From<crate::config::ConfigError> for StorageError {
    fn from(error: crate::config::ConfigError) -> Self {
        Self::from_callback(error.into())
    }
}

impl From<pi_js_compat::node_error::NativeNodeError> for StorageError {
    fn from(error: pi_js_compat::node_error::NativeNodeError) -> Self {
        Self::from_callback(pi_ai::request::CallbackError::from(error))
    }
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&js_json::encode_node_utf8(self.message()))
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Failure { cause, .. } => cause.as_deref().map(|error| error as &dyn std::error::Error),
            Self::Aborted(error) => Some(error),
        }
    }
}

/// Frozen catch: Error.message, otherwise JS String(value), which can itself throw.
pub(crate) fn stringify_auth_rejection(
    error: &pi_ai::request::CallbackError,
) -> Result<JsString, pi_ai::request::CallbackError> {
    error.error_or_string_message()
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

    /// Runs `operation` while holding the file lock.
    fn with_lock_async(&self, operation: AsyncLockOperation) -> BoxFuture<'_, Result<(), StorageError>>;
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// `FileAuthStorageBackend`: JSON file plus `proper-lockfile`.
pub struct FileAuthStorageBackend {
    auth_path: JsString,
}

impl FileAuthStorageBackend {
    pub fn new(auth_path: &JsString) -> Result<Self, crate::config::ConfigError> {
        Ok(Self {
            auth_path: normalize_path(auth_path, &Default::default())?,
        })
    }

    pub fn auth_path(&self) -> &JsString {
        &self.auth_path
    }

    fn ensure_parent_dir(&self) -> Result<(), StorageError> {
        let encoded = js_json::encode_node_utf8(&self.auth_path);
        let Some(directory) = Path::new(&encoded).parent() else {
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
            StorageError::filesystem(pi_js_compat::node_fs::node_fs_error(
                &error,
                "mkdir",
                &node_dirname(self.auth_path.clone()),
            ))
        })
    }

    fn ensure_file_exists(&self) -> Result<(), StorageError> {
        if Path::new(&js_json::encode_node_utf8(&self.auth_path)).exists() {
            return Ok(());
        }
        write_auth_file(&self.auth_path, "{}")?;
        Ok(())
    }

    fn read_current(&self) -> Result<Option<String>, StorageError> {
        if !Path::new(&js_json::encode_node_utf8(&self.auth_path)).exists() {
            return Ok(None);
        }
        std::fs::read(js_json::encode_node_utf8(&self.auth_path))
            .map(|bytes| Some(String::from_utf8_lossy(&bytes).into_owned()))
            .map_err(|error| StorageError::filesystem(pi_js_compat::node_fs::node_read_error(&error, &self.auth_path)))
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
            match acquire_sync(Path::new(&js_json::encode_node_utf8(&self.auth_path)), &options) {
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
            match acquire(Path::new(&js_json::encode_node_utf8(&self.auth_path)), &options).await {
                Ok(guard) => {
                    if let Some(message) = compromised.lock().unwrap().clone() {
                        return Err(StorageError::new(message));
                    }
                    return Ok((guard, compromised));
                }
                Err(error) if error.code == LockErrorCode::Locked && retry < 10 => {
                    let base_delay = 100u64.saturating_mul(1u64 << retry);
                    retry += 1;
                    let delay = jittered_delay(base_delay).min(10_000);
                    tokio::time::sleep(Duration::from_millis(delay)).await;
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

    fn with_lock_async(&self, operation: AsyncLockOperation) -> BoxFuture<'_, Result<(), StorageError>> {
        Box::pin(async move {
            self.ensure_parent_dir()?;
            self.ensure_file_exists()?;
            let (guard, compromised) = self.acquire_async().await?;
            let check_compromised = || match lock(&compromised).clone() {
                Some(message) => Err(StorageError::new(message)),
                None => Ok(()),
            };
            let outcome = async {
                check_compromised()?;
                let current = self.read_current()?;
                let next = operation(current).await?;
                check_compromised()?;

                self.write_next(next)?;
                check_compromised()
            }
            .await;
            let _ = guard.release();
            outcome
        })
    }
}

fn write_auth_file(raw_path: &JsString, content: &str) -> Result<(), StorageError> {
    let encoded = js_json::encode_node_utf8(raw_path);
    let path = encoded.as_str();
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
        .map_err(|error| StorageError::filesystem(pi_js_compat::node_fs::node_fs_error(&error, "open", raw_path)))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(AUTH_FILE_MODE)).map_err(|error| {
            StorageError::filesystem(pi_js_compat::node_fs::node_fs_error(&error, "chmod", raw_path))
        })?;
    }
    Ok(())
}

/// `InMemoryAuthStorageBackend`: callbacks independently snapshot one JSON string.
#[derive(Default)]
pub struct InMemoryAuthStorageBackend {
    value: Mutex<Option<String>>,
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

    fn with_lock_async(&self, operation: AsyncLockOperation) -> BoxFuture<'_, Result<(), StorageError>> {
        Box::pin(async move {
            let current = self
                .value
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            let next = operation(current).await?;

            if let Some(next) = next {
                self.set_value(Some(next));
            }
            Ok(())
        })
    }
}

pub type AuthStorageData = js_json::JsObject;
use js_json::{JsObject, JsString, JsValue};

fn parse_auth_data(content: Option<&str>) -> Result<AuthStorageData, StorageError> {
    let Some(content) = content.filter(|s| !s.is_empty()) else {
        return Ok(JsObject::new());
    };
    match js_json::parse_lossless(content).map_err(|e| StorageError::new(e.to_string()))? {
        JsValue::Object(data) => Ok(data),
        _ => Ok(JsObject::new()),
    }
}
fn credential_type(value: &JsValue) -> Option<JsString> {
    value.get("type")?.as_string().cloned()
}

fn parse_api_key_config(credential: &JsValue) -> Result<JsString, StorageError> {
    let message = match credential.get("key") {
        Some(JsValue::String(key)) => return Ok(key.clone()),
        Some(JsValue::Null) => "Cannot read properties of null (reading 'startsWith')",
        None => "Cannot read properties of undefined (reading 'startsWith')",
        _ => "config.startsWith is not a function",
    };
    Err(StorageError::from_callback(
        NativeNodeError::from_constructor(NativeErrorClass::TypeError, message).into(),
    ))
}
fn to_credential(value: &JsValue) -> Result<Credential, StorageError> {
    js_json::from_value(value).map_err(|error| StorageError::new(error.to_string()))
}
fn oauth_value(value: pi_ai::oauth::OAuthCredentials) -> Result<JsValue, StorageError> {
    Ok(value.to_js_value())
}
fn source_now_millis() -> f64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as f64
}
#[derive(Clone)]
pub struct GetApiKeyOptions {
    pub include_fallback: bool,
    pub throw_on_refresh_failure: bool,
}
impl Default for GetApiKeyOptions {
    fn default() -> Self {
        Self {
            include_fallback: true,
            throw_on_refresh_failure: false,
        }
    }
}
#[derive(Default)]
struct SourceAuthState {
    data: AuthStorageData,
    runtime_overrides: std::collections::HashMap<String, JsString>,
    load_error: Option<StorageError>,
    errors: Vec<StorageError>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthStatus {
    pub configured: bool,
    pub source: Option<String>,
    pub label: Option<JsString>,
}
#[derive(Clone)]
pub struct AuthStorage {
    storage: Arc<dyn AuthStorageBackend>,
    source_state: Arc<Mutex<SourceAuthState>>,
}
impl AuthStorage {
    pub fn create(auth_path: Option<&JsString>) -> Result<Self, crate::config::ConfigError> {
        let path = match auth_path {
            Some(path) => path.clone(),
            None => default_auth_path()?,
        };
        Ok(Self::from_storage(Arc::new(FileAuthStorageBackend::new(&path)?)))
    }
    pub fn from_storage(storage: Arc<dyn AuthStorageBackend>) -> Self {
        let store = Self {
            storage,
            source_state: Arc::new(Mutex::new(SourceAuthState::default())),
        };
        store.reload();
        store
    }
    pub fn in_memory<K: Into<JsString>, V: Into<JsValue>>(data: impl IntoIterator<Item = (K, V)>) -> Self {
        let data = data
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
        let backend = Arc::new(InMemoryAuthStorageBackend::new());
        backend.set_value(Some(js_json::stringify_lossless_pretty(&JsValue::Object(data))));
        Self::from_storage(backend)
    }
    pub fn get(&self, provider: impl Into<JsString>) -> Option<JsValue> {
        lock(&self.source_state).data.get(&provider.into()).cloned()
    }
    pub fn get_all(&self) -> AuthStorageData {
        lock(&self.source_state).data.clone()
    }
    pub fn list_providers(&self) -> Vec<JsString> {
        ordered_provider_ids(&lock(&self.source_state).data)
    }
    pub fn has(&self, provider: impl Into<JsString>) -> bool {
        lock(&self.source_state).data.contains_key(&provider.into())
    }
    pub fn get_provider_env(&self, provider: &str) -> Option<JsValue> {
        let credential = self.get(provider)?;
        if !credential_type(&credential).is_some_and(|tag| tag == "api_key") {
            return None;
        }
        credential.get("env").cloned()
    }
    pub fn set_runtime_api_key(&self, provider: &str, api_key: &JsString) {
        lock(&self.source_state)
            .runtime_overrides
            .insert(provider.to_owned(), api_key.to_owned());
    }
    pub fn remove_runtime_api_key(&self, provider: &str) {
        lock(&self.source_state).runtime_overrides.remove(provider);
    }
    pub fn has_auth(&self, provider: &str) -> Result<bool, pi_js_compat::node_error::NativeNodeError> {
        let state = lock(&self.source_state);
        Ok(state.runtime_overrides.contains_key(provider)
            || state
                .data
                .get(&JsString::from(provider))
                .is_some_and(js_json::is_truthy)
            || pi_ai::env_api_keys::get_env_api_key(provider, None)?.is_some())
    }
    pub fn get_auth_status(&self, provider: &str) -> AuthStatus {
        let source = lock(&self.source_state);
        if source
            .data
            .get(&JsString::from(provider))
            .is_some_and(js_json::is_truthy)
        {
            return AuthStatus {
                configured: true,
                source: Some("stored".into()),
                label: None,
            };
        }
        if source.runtime_overrides.contains_key(provider) {
            return AuthStatus {
                configured: false,
                source: Some("runtime".into()),
                label: Some("--api-key".into()),
            };
        }
        if let Some(label) = pi_ai::env_api_keys::find_env_keys(provider, None).and_then(|k| k.into_iter().next()) {
            return AuthStatus {
                configured: false,
                source: Some("environment".into()),
                label: Some(label.into()),
            };
        }
        AuthStatus {
            configured: false,
            source: None,
            label: None,
        }
    }
    fn record_error(&self, error: StorageError) -> Result<(), StorageError> {
        let normalized = match error.thrown_value() {
            pi_js_compat::abort::AbortReason::Value(_) => {
                StorageError::new(stringify_auth_rejection(error.callback()).map_err(StorageError::from_callback)?)
            }
            _ => error,
        };
        lock(&self.source_state).errors.push(normalized);
        Ok(())
    }
    pub fn drain_errors(&self) -> Vec<StorageError> {
        std::mem::take(&mut lock(&self.source_state).errors)
    }
    pub fn reload(&self) {
        let mut parsed = None;
        let outcome = self.storage.with_lock(Box::new(|current| {
            parsed = Some(parse_auth_data(current));
            Ok(None)
        }));
        let result = outcome.and_then(|()| parsed.expect("backend must invoke callback"));
        let mut state = lock(&self.source_state);
        match result {
            Ok(data) => {
                state.data = data;
                state.load_error = None;
            }
            Err(error) => {
                state.load_error = Some(error.clone());
                state.errors.push(error);
            }
        }
    }
    fn persist_provider_change(&self, provider: JsString, credential: Option<JsValue>) -> Result<(), StorageError> {
        if lock(&self.source_state).load_error.is_some() {
            self.reload();
        }
        let load_error = lock(&self.source_state).load_error.clone();
        if let Some(error) = load_error {
            let error = StorageError::new(format!(
                "Cannot update auth storage because it could not be loaded: {error}"
            ));
            self.record_error(error.clone())?;
            return Err(error);
        }
        let mut merged = None;
        let result = self.storage.with_lock(Box::new(|current| {
            let mut data = parse_auth_data(current)?;
            if let Some(value) = credential {
                data.insert(provider, value);
            } else {
                data.shift_remove(&provider);
            }
            let content = js_json::stringify_lossless_pretty(&JsValue::Object(data.clone()));
            merged = Some(data);
            Ok(Some(content))
        }));
        if let Err(error) = result {
            self.record_error(error.clone())?;
            return Err(error);
        }
        let mut state = lock(&self.source_state);
        state.data = merged.expect("backend must invoke callback");
        state.load_error = None;
        Ok(())
    }
    pub fn set(&self, provider: impl Into<JsString>, credential: impl Into<JsValue>) -> Result<(), StorageError> {
        self.persist_provider_change(provider.into(), Some(credential.into()))
    }
    pub fn remove(&self, provider: impl Into<JsString>) -> Result<(), StorageError> {
        self.persist_provider_change(provider.into(), None)
    }
    pub fn logout(&self, provider: &str) -> Result<(), StorageError> {
        self.remove(provider)
    }
    pub async fn login(
        &self,
        id: &str,
        callbacks: Arc<dyn pi_ai::oauth::OAuthLoginCallbacks>,
    ) -> Result<(), StorageError> {
        let provider = pi_ai::oauth::get_oauth_provider(id)
            .ok_or_else(|| StorageError::new(format!("Unknown OAuth provider: {id}")))?;
        let value = provider.login(callbacks).await.map_err(StorageError::from_auth)?;
        self.set(id, value.to_js_value())
    }
    pub fn get_oauth_providers(&self) -> Vec<Arc<dyn pi_ai::oauth::OAuthProviderInterface>> {
        pi_ai::oauth::get_oauth_providers()
    }
    async fn refresh_oauth_token_with_lock(
        &self,
        provider_id: &str,
    ) -> Result<Option<pi_ai::oauth::OAuthApiKey>, StorageError> {
        let Some(provider) = pi_ai::oauth::get_oauth_provider(provider_id) else {
            return Ok(None);
        };
        let rotated = Arc::new(Mutex::new(None));
        let captured = rotated.clone();
        let result = Arc::new(Mutex::new(None));
        let captured_result = result.clone();
        let source = self.source_state.clone();
        let id = provider_id.to_owned();
        let refresh = self
            .storage
            .with_lock_async(Box::new(move |current| {
                Box::pin(async move {
                    let mut data = parse_auth_data(current.as_deref())?;
                    {
                        let mut state = lock(&source);
                        state.data = data.clone();
                        state.load_error = None;
                    }
                    let Some(value) = data.get(&JsString::from(&id)) else {
                        return Ok(None);
                    };
                    if !credential_type(value).is_some_and(|tag| tag == "oauth") {
                        return Ok(None);
                    }
                    let credential = to_credential(value)?.into_oauth().expect("checked OAuth");
                    if source_now_millis() < credential.expires {
                        *lock(&captured_result) = Some(pi_ai::oauth::OAuthApiKey {
                            api_key: provider.get_api_key(&credential).map_err(StorageError::from_callback)?,
                            new_credentials: credential,
                        });
                        return Ok(None);
                    }
                    let credentials = std::collections::HashMap::from([(id.clone(), credential)]);
                    let Some(refreshed) = pi_ai::oauth::get_oauth_api_key(&id, &credentials)
                        .await
                        .map_err(StorageError::from_auth)?
                    else {
                        return Ok(None);
                    };
                    *lock(&captured) = Some(refreshed.clone());
                    data.insert(id.into(), refreshed.new_credentials.to_js_value());
                    let content = js_json::stringify_lossless_pretty(&JsValue::Object(data.clone()));
                    {
                        let mut state = lock(&source);
                        state.data = data;
                        state.load_error = None;
                    }
                    *lock(&captured_result) = Some(refreshed);
                    Ok(Some(content))
                })
            }))
            .await;
        match refresh {
            Ok(()) => Ok(lock(&result).take()),
            Err(error) => {
                let Some(rotated) = lock(&rotated).take() else {
                    return Err(error);
                };
                self.record_error(error)?;
                self.set(provider_id, oauth_value(rotated.new_credentials.clone())?)?;
                Ok(Some(rotated))
            }
        }
    }
    /// Public key lookup preserves the provider's UTF-16 result unchanged.
    pub async fn get_api_key(
        &self,
        provider_id: &str,
        options: GetApiKeyOptions,
    ) -> Result<Option<JsString>, StorageError> {
        let runtime = lock(&self.source_state).runtime_overrides.get(provider_id).cloned();
        if runtime.as_ref().is_some_and(|k| !k.is_empty()) {
            return Ok(runtime);
        }
        if let Some(value) = self.get(provider_id) {
            match credential_type(&value).as_ref() {
                Some(tag) if tag == "api_key" => {
                    let key = parse_api_key_config(&value)?;
                    let env = self
                        .get_provider_env(provider_id)
                        .map(|value| js_json::from_value::<std::collections::HashMap<String, JsString>>(&value))
                        .transpose()
                        .map_err(|e| StorageError::new(e.to_string()))?;
                    return Ok(resolve_config_value(&key, env.as_ref()));
                }
                Some(tag) if tag == "oauth" => {
                    let Some(provider) = pi_ai::oauth::get_oauth_provider(provider_id) else {
                        return Ok(None);
                    };
                    let credential = to_credential(&value)?.into_oauth().expect("checked OAuth");
                    if source_now_millis() < credential.expires {
                        return provider
                            .get_api_key(&credential)
                            .map(Some)
                            .map_err(StorageError::from_callback);
                    }
                    match self.refresh_oauth_token_with_lock(provider_id).await {
                        Ok(Some(result)) => return Ok(Some(result.api_key)),
                        Ok(None) => {}
                        Err(error) => {
                            self.record_error(error.clone())?;
                            self.reload();
                            let reloaded = lock(&self.source_state).load_error.is_none();
                            if let Some(updated) = self.get(provider_id).filter(|_| reloaded)
                                && credential_type(&updated).is_some_and(|tag| tag == "oauth")
                            {
                                let updated = to_credential(&updated)?.into_oauth().expect("checked OAuth");
                                if source_now_millis() < updated.expires {
                                    return provider
                                        .get_api_key(&updated)
                                        .map(Some)
                                        .map_err(StorageError::from_callback);
                                }
                            }
                            if options.throw_on_refresh_failure {
                                let reason =
                                    stringify_auth_rejection(error.callback()).map_err(StorageError::from_callback)?;
                                let wrapped = StorageError::Failure {
                                    error: pi_ai::request::CallbackError::new(js_format!(
                                        "Authentication failed for \"{}\": {}. Run '/login {}' to re-authenticate.",
                                        provider_id,
                                        reason,
                                        provider_id
                                    )),
                                    cause: Some(Arc::new(error)),
                                };
                                return Err(wrapped);
                            }
                            return Ok(None);
                        }
                    }
                }
                _ => {}
            }
        }
        if !options.include_fallback {
            return Ok(None);
        }
        pi_ai::env_api_keys::get_env_api_key(provider_id, None)
            .map_err(|error| StorageError::from_callback(pi_ai::request::CallbackError::from_rejection(error.into())))
    }
}

pub fn ordered_provider_ids(data: &AuthStorageData) -> Vec<JsString> {
    js_json::object_entries_lossless(data)
        .into_iter()
        .map(|(key, _)| key.clone())
        .collect()
}
pub fn parse_auth_storage_data(content: &str) -> Result<AuthStorageData, StorageError> {
    parse_auth_data(Some(content))
}
pub fn default_auth_path() -> Result<JsString, crate::config::ConfigError> {
    Ok(crate::utils::paths::node_join(get_agent_dir()?, "auth.json"))
}
