//! Port of `packages/coding-agent/src/core/model-runtime.ts`.
//!
//! Configured pi-ai `Models` collection used by coding-agent and SDK
//! consumers: built-in providers, `models.json` overlays, extension
//! registrations, availability snapshots, and serialized credential flows.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use pi_ai::auth::resolve::{AuthResolutionOverrides, ModelsError, ModelsErrorCode, ResolveError};
use pi_ai::auth::types::{
    AuthCheck, AuthInteraction, AuthOperationOptions, AuthResult, AuthType, Credential, CredentialInfo, CredentialStore,
};
use pi_ai::http::default_fetch;
use pi_ai::models::{
    CreateModelsOptions, Models, ModelsRefreshOptions, ModelsRefreshResult, MutableModels, Provider, create_models,
    merge_headers,
};
use pi_ai::providers::all::{builtin_providers, get_builtin_model_data_generated_at};
use pi_ai::request::{
    BoxFuture, CallbackError, DeferredCancelOptions, DeferredFetchOptions, SimpleStreamOptions, StreamOptions,
};
use pi_ai::types::{AssistantMessage, Context, DeferredHandle, Model, ProviderEnv, ProviderHeaders, ProviderId};
use pi_ai::utils::event_stream::{AssistantMessageEventStream, StreamFailure};
use pi_js_compat::js_json::{JsString, encode_node_utf8};
use serde_json::Map;
use tokio::sync::Mutex as AsyncMutex;
use tokio_util::sync::CancellationToken;

use crate::config::get_agent_dir;
use crate::core::auth_storage::AuthStorage;
use crate::core::model_config::ModelConfig;
use crate::core::model_resolver::ModelResolverRuntime;
use crate::core::models_store::{FileModelsStore, InMemoryCodingAgentModelsStore};
use crate::core::provider_composer::{
    AuthStatus, AuthStatusSource, CompatibilityRequestConfig, ModelsJsonProvider, ProviderConfigInput,
    compose_model_provider, configured_request_auth_status, resolve_compatibility_request_config,
    resolve_configured_model_headers, validate_extension_provider,
};
use crate::core::remote_catalog_provider::with_remote_catalog;
use crate::core::runtime_credentials::RuntimeCredentials;

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Credential synchronization operations named by the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSynchronizationOperation {
    Login,
    Logout,
    SetRuntimeApiKey,
    RemoveRuntimeApiKey,
}

impl CredentialSynchronizationOperation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Login => "login",
            Self::Logout => "logout",
            Self::SetRuntimeApiKey => "setRuntimeApiKey",
            Self::RemoveRuntimeApiKey => "removeRuntimeApiKey",
        }
    }
}

/// Credentials changed successfully, but the local snapshot could not be synchronized.
#[derive(Debug, Clone, PartialEq)]
pub struct CredentialSynchronizationError {
    pub provider_id: String,
    pub operation: CredentialSynchronizationOperation,
    pub credential: Option<Credential>,
    pub cause: JsString,
}

impl std::fmt::Display for CredentialSynchronizationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Credential {} committed for {}, but local synchronization failed",
            self.operation.as_str(),
            self.provider_id
        )
    }
}

impl std::error::Error for CredentialSynchronizationError {}

/// Failure from [`ModelRuntime`] operations.
#[derive(Debug, Clone, PartialEq)]
pub enum ModelRuntimeError {
    Aborted,
    Resolve(ResolveError),
    CredentialSync(Box<CredentialSynchronizationError>),
    Message(JsString),
}

impl std::fmt::Display for ModelRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Aborted => formatter.write_str("The operation was aborted"),
            Self::Resolve(error) => write!(formatter, "{error}"),
            Self::CredentialSync(error) => write!(formatter, "{error}"),
            Self::Message(message) => formatter.write_str(&encode_node_utf8(message)),
        }
    }
}

impl ModelRuntimeError {
    pub fn message(&self) -> JsString {
        match self {
            Self::Aborted => "The operation was aborted".into(),
            Self::Resolve(error) => resolve_error_message(error),
            Self::CredentialSync(error) => format!(
                "Credential {} committed for {}, but local synchronization failed",
                error.operation.as_str(),
                error.provider_id
            )
            .into(),
            Self::Message(message) => message.clone(),
        }
    }
}

pub fn resolve_error_message(error: &ResolveError) -> JsString {
    match error {
        ResolveError::Aborted => "The operation was aborted".into(),
        ResolveError::Models(error) => error.message.clone(),
        ResolveError::Callback(error) => error.message.clone(),
    }
}

impl std::error::Error for ModelRuntimeError {}

impl From<ResolveError> for ModelRuntimeError {
    fn from(error: ResolveError) -> Self {
        Self::Resolve(error)
    }
}

fn resolve_error(code: ModelsErrorCode, message: impl Into<JsString>) -> ResolveError {
    ResolveError::models(code, message)
}

/// Options for [`ModelRuntime::create`].
#[derive(Default, Clone)]
pub struct CreateModelRuntimeOptions {
    pub credentials: Option<Arc<dyn CredentialStore>>,
    pub auth_path: Option<JsString>,
    /// `None` uses the agent dir; `Some(None)` disables `models.json`.
    pub models_path: Option<Option<JsString>>,
    pub models_store: Option<Arc<dyn pi_ai::models_store::ModelsStore>>,
    pub models_store_path: Option<JsString>,
    /// Allow create() to refresh catalogs over the network. Defaults to false.
    pub allow_model_network: bool,
    pub model_refresh_timeout_ms: Option<f64>,
    pub catalog_base_url: Option<String>,
    pub signal: Option<CancellationToken>,
    /// Skip the initial catalog/availability refresh.
    pub refresh_on_create: Option<bool>,
}

#[derive(Clone, Default)]
struct ModelRuntimeSnapshot {
    all: Vec<Model>,
    available: Vec<Model>,
    configured_providers: BTreeSet<String>,
    stored_providers: BTreeSet<String>,
    auth: BTreeMap<String, AuthCheck>,
}

struct PreparedRequest {
    provider: Arc<dyn Provider>,
    model: Model,
    options: StreamOptions,
}

struct RuntimeInner {
    models: pi_ai::models::ModelsCollection,
    credentials: Arc<RuntimeCredentials>,
    default_builtins: Vec<(String, Arc<dyn Provider>)>,
    builtins: Mutex<Vec<(String, Arc<dyn Provider>)>>,
    native_extension_providers: Mutex<Vec<(String, Arc<dyn Provider>)>>,
    extension_providers: Mutex<Vec<(String, ProviderConfigInput)>>,
    composition_errors: Mutex<BTreeMap<String, String>>,
    models_path: Option<JsString>,
    auth_path: Option<JsString>,
    model_network_enabled: bool,
    config: Mutex<ModelConfig>,
    snapshot: Mutex<ModelRuntimeSnapshot>,
    availability_refresh_seq: AtomicU64,
    availability_error_seq: AtomicU64,
    provider_availability_seq: Mutex<HashMap<String, u64>>,
    availability_error: Mutex<Option<JsString>>,
    credential_gates: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

/// Configured pi-ai `Models` collection used by coding-agent and SDK consumers.
#[derive(Clone)]
pub struct ModelRuntime {
    inner: Arc<RuntimeInner>,
}

impl ModelRuntime {
    pub async fn create(options: CreateModelRuntimeOptions) -> Result<Self, ModelRuntimeError> {
        let credential_store = options.credentials.clone();
        let default_auth_path = options
            .auth_path
            .clone()
            .unwrap_or_else(|| get_agent_dir().join("auth.json").display().to_string().into());
        let credentials = Arc::new(RuntimeCredentials::new(
            credential_store.unwrap_or_else(|| Arc::new(AuthStorage::create(Some(&default_auth_path)))),
        ));
        let reload_auth_path = if options.credentials.is_none() {
            Some(default_auth_path)
        } else {
            None
        };
        let models_path = match &options.models_path {
            Some(None) => None,
            Some(Some(path)) => Some(path.clone()),
            None => Some(get_agent_dir().join("models.json").display().to_string().into()),
        };
        let config = ModelConfig::load(models_path.as_ref());
        let models_store: Arc<dyn pi_ai::models_store::ModelsStore> =
            options.models_store.unwrap_or_else(|| match &models_path {
                Some(path) => {
                    let store_path = options.models_store_path.clone().unwrap_or_else(|| {
                        crate::utils::paths::node_join(crate::utils::paths::node_dirname(path), "models-store.json")
                    });
                    Arc::new(FileModelsStore::new(Some(&store_path)))
                }
                None => Arc::new(InMemoryCodingAgentModelsStore::new()),
            });
        let generated_at = get_builtin_model_data_generated_at();
        let fetch = default_fetch();
        let providers: Vec<Arc<dyn Provider>> = builtin_providers()
            .into_iter()
            .map(|provider| {
                if provider.id() == "radius" {
                    provider
                } else {
                    with_remote_catalog(
                        provider,
                        Arc::clone(&fetch),
                        options.catalog_base_url.as_deref(),
                        generated_at,
                    )
                }
            })
            .collect();
        let model_network_enabled = pi_js_compat::process_env::get("PI_OFFLINE").is_none();
        let models = create_models(CreateModelsOptions {
            credentials: Some(credentials.clone() as Arc<dyn CredentialStore>),
            models_store: Some(models_store),
            auth_context: None,
        });
        let runtime = Self {
            inner: Arc::new(RuntimeInner {
                models,
                credentials,
                default_builtins: providers
                    .into_iter()
                    .map(|provider| (provider.id().to_owned(), provider))
                    .collect(),
                builtins: Mutex::new(Vec::new()),
                native_extension_providers: Mutex::new(Vec::new()),
                extension_providers: Mutex::new(Vec::new()),
                composition_errors: Mutex::new(BTreeMap::new()),
                models_path,
                auth_path: reload_auth_path,
                model_network_enabled,
                config: Mutex::new(config),
                snapshot: Mutex::new(ModelRuntimeSnapshot::default()),
                availability_refresh_seq: AtomicU64::new(0),
                availability_error_seq: AtomicU64::new(0),
                provider_availability_seq: Mutex::new(HashMap::new()),
                availability_error: Mutex::new(None),
                credential_gates: Mutex::new(HashMap::new()),
            }),
        };
        runtime.reset_builtin_providers();
        runtime.rebuild_providers();
        let refresh_from_network = runtime.inner.model_network_enabled && options.allow_model_network;
        let signal = options.signal.clone();
        let timeout_signal = match (refresh_from_network, options.model_refresh_timeout_ms) {
            (true, Some(timeout_ms)) => {
                let controller = CancellationToken::new();
                let timer = controller.clone();
                let duration = std::time::Duration::from_millis(timeout_ms.max(0.0) as u64);
                tokio::spawn(async move {
                    tokio::time::sleep(duration).await;
                    timer.cancel();
                });
                Some(controller)
            }
            _ => None,
        };
        let effective_signal = match (signal, timeout_signal) {
            (Some(signal), Some(timeout)) => {
                let combined = CancellationToken::new();
                let child = combined.clone();
                tokio::spawn(async move {
                    tokio::select! {
                        _ = signal.cancelled() => child.cancel(),
                        _ = timeout.cancelled() => child.cancel(),
                    }
                });
                Some(combined)
            }
            (signal, None) => signal,
            (None, timeout) => timeout,
        };
        if options.refresh_on_create.unwrap_or(true) {
            let _ = runtime
                .refresh(ModelsRefreshOptions {
                    allow_network: Some(refresh_from_network),
                    signal: effective_signal,
                    ..Default::default()
                })
                .await?;
        }
        Ok(runtime)
    }

    fn reset_builtin_providers(&self) {
        let mut builtins = lock(&self.inner.builtins);
        builtins.clear();
        builtins.extend(self.inner.default_builtins.iter().cloned());
    }

    fn provider_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for (id, _) in lock(&self.inner.builtins).iter() {
            push_unique_id(&mut ids, id);
        }
        for (id, _) in lock(&self.inner.native_extension_providers).iter() {
            push_unique_id(&mut ids, id);
        }
        for id in lock(&self.inner.config).get_provider_ids() {
            push_unique_id(&mut ids, &id);
        }
        for (id, _) in lock(&self.inner.extension_providers).iter() {
            push_unique_id(&mut ids, id);
        }
        ids
    }

    fn recompose_provider(&self, provider_id: &str) {
        let base = find_provider(&lock(&self.inner.native_extension_providers), provider_id)
            .or_else(|| find_provider(&lock(&self.inner.builtins), provider_id));
        let extension = lock(&self.inner.extension_providers)
            .iter()
            .find(|(id, _)| id == provider_id)
            .map(|(_, config)| config.clone());
        let has_config = lock(&self.inner.config).get_provider(provider_id).is_some();
        if base.is_none() && !has_config && extension.is_none() {
            self.inner.models.delete_provider(provider_id);
            lock(&self.inner.composition_errors).remove(provider_id);
            return;
        }
        if let Some(base) = &base
            && !has_config
            && extension.is_none()
        {
            self.inner.models.set_provider(Arc::clone(base));
            lock(&self.inner.composition_errors).remove(provider_id);
            return;
        }
        let model_config = lock(&self.inner.config).clone();
        match compose_model_provider(provider_id, base.clone(), &model_config, extension.as_ref()) {
            Ok(provider) => {
                self.inner.models.set_provider(provider);
                lock(&self.inner.composition_errors).remove(provider_id);
            }
            Err(error) => {
                lock(&self.inner.composition_errors).insert(provider_id.to_owned(), error.to_string());
                match base {
                    Some(base) => self.inner.models.set_provider(base),
                    None => self.inner.models.delete_provider(provider_id),
                }
            }
        }
    }

    fn rebuild_providers(&self) {
        self.inner.models.clear_providers();
        lock(&self.inner.composition_errors).clear();
        for provider_id in self.provider_ids() {
            self.recompose_provider(&provider_id);
        }
        self.update_model_snapshot();
    }

    fn update_model_snapshot(&self) {
        let all = self.inner.models.get_models(None);
        let mut snapshot = lock(&self.inner.snapshot);
        snapshot.available = all
            .iter()
            .filter(|model| snapshot.configured_providers.contains(&model.provider))
            .cloned()
            .collect();
        snapshot.all = all;
    }

    async fn run_availability_refresh(
        &self,
        seq: u64,
        error_seq: u64,
        signal: CancellationToken,
    ) -> Result<(), ResolveError> {
        let providers = self.inner.models.get_providers();
        let available = self
            .inner
            .models
            .get_available(None, AuthOperationOptions::new(Some(signal.clone())))
            .await?;
        let mut join_set = tokio::task::JoinSet::new();
        for provider in providers {
            let models = self.inner.models.clone();
            let signal = signal.clone();
            join_set.spawn(async move {
                let provider_id = provider.id().to_owned();
                let check = models
                    .check_auth(&provider_id, AuthOperationOptions::new(Some(signal)))
                    .await;
                (provider_id, check)
            });
        }
        let mut checks: Vec<(String, Option<AuthCheck>)> = Vec::new();
        while let Some(result) = join_set.join_next().await {
            let (provider_id, check) = result
                .map_err(|error| resolve_error(ModelsErrorCode::Auth, format!("provider check failed: {error}")))?;
            checks.push((provider_id, check?));
        }
        let credentials = self
            .inner
            .credentials
            .list(AuthOperationOptions::new(Some(signal)))
            .await
            .map_err(|error| match error {
                pi_ai::auth::types::AuthError::Aborted => ResolveError::Aborted,
                pi_ai::auth::types::AuthError::Failed(error) => ResolveError::Callback(error),
            })?;
        if seq != self.inner.availability_refresh_seq.load(Ordering::SeqCst) {
            return Ok(());
        }
        let mut auth = BTreeMap::new();
        let mut configured_providers = BTreeSet::new();
        for (provider_id, check) in checks {
            if let Some(check) = check {
                configured_providers.insert(provider_id.clone());
                auth.insert(provider_id, check);
            }
        }
        let mut snapshot = lock(&self.inner.snapshot);
        snapshot.all = self.inner.models.get_models(None);
        snapshot.available = available;
        snapshot.configured_providers = configured_providers;
        snapshot.stored_providers = credentials.into_iter().map(|entry| entry.provider_id).collect();
        snapshot.auth = auth;
        if error_seq == self.inner.availability_error_seq.load(Ordering::SeqCst) {
            *lock(&self.inner.availability_error) = None;
        }
        Ok(())
    }

    async fn queue_availability_refresh(&self, signal: Option<CancellationToken>) -> Result<(), ResolveError> {
        let seq = self.inner.availability_refresh_seq.fetch_add(1, Ordering::SeqCst) + 1;
        {
            let mut provider_seq = lock(&self.inner.provider_availability_seq);
            for value in provider_seq.values_mut() {
                *value += 1;
            }
        }
        let error_seq = self.inner.availability_error_seq.fetch_add(1, Ordering::SeqCst) + 1;
        let effective_signal = signal.unwrap_or_default();
        let result = self
            .run_availability_refresh(seq, error_seq, effective_signal.clone())
            .await;
        if let Err(error) = &result
            && error_seq == self.inner.availability_error_seq.load(Ordering::SeqCst)
            && !effective_signal.is_cancelled()
        {
            *lock(&self.inner.availability_error) = Some(resolve_error_message(error));
        }
        result
    }

    async fn refresh_provider_availability(
        &self,
        provider_id: &str,
        signal: CancellationToken,
    ) -> Result<(), ResolveError> {
        self.inner.availability_refresh_seq.fetch_add(1, Ordering::SeqCst);
        let provider_seq = {
            let mut sequences = lock(&self.inner.provider_availability_seq);
            let entry = sequences.entry(provider_id.to_owned()).or_insert(0);
            *entry += 1;
            *entry
        };
        let error_seq = self.inner.availability_error_seq.fetch_add(1, Ordering::SeqCst) + 1;
        let available = self
            .inner
            .models
            .get_available(Some(provider_id), AuthOperationOptions::new(Some(signal.clone())))
            .await;
        let auth = self
            .inner
            .models
            .check_auth(provider_id, AuthOperationOptions::new(Some(signal.clone())))
            .await;
        let credential = self
            .inner
            .credentials
            .read(provider_id, AuthOperationOptions::new(Some(signal.clone())))
            .await;
        let (available, auth, credential) = match (available, auth, credential) {
            (Ok(available), Ok(auth), Ok(credential)) => (available, auth, credential),
            (Err(error), _, _) | (_, Err(error), _) => return Err(error),
            (_, _, Err(error)) => {
                return Err(match error {
                    pi_ai::auth::types::AuthError::Aborted => ResolveError::Aborted,
                    pi_ai::auth::types::AuthError::Failed(error) => ResolveError::Callback(error),
                });
            }
        };
        if signal.is_cancelled() {
            return Err(ResolveError::Aborted);
        }
        if lock(&self.inner.provider_availability_seq).get(provider_id).copied() != Some(provider_seq) {
            return Ok(());
        }
        let mut snapshot = lock(&self.inner.snapshot);
        if auth.is_some() {
            snapshot.configured_providers.insert(provider_id.to_owned());
        } else {
            snapshot.configured_providers.remove(provider_id);
        }
        match &auth {
            Some(check) => {
                snapshot.auth.insert(provider_id.to_owned(), check.clone());
            }
            None => {
                snapshot.auth.remove(provider_id);
            }
        }
        if credential.is_some() {
            snapshot.stored_providers.insert(provider_id.to_owned());
        } else {
            snapshot.stored_providers.remove(provider_id);
        }
        let all = self.inner.models.get_models(None);
        let mut available_by_id: HashMap<String, Model> = HashMap::new();
        for model in snapshot
            .available
            .iter()
            .filter(|model| model.provider != provider_id)
            .chain(available.iter())
        {
            available_by_id.insert(format!("{}\0{}", model.provider, model.id), model.clone());
        }
        snapshot.available = all
            .iter()
            .filter_map(|model| {
                available_by_id
                    .get(&format!("{}\0{}", model.provider, model.id))
                    .cloned()
            })
            .collect();
        snapshot.all = all;
        if error_seq == self.inner.availability_error_seq.load(Ordering::SeqCst) {
            *lock(&self.inner.availability_error) = None;
        }
        Ok(())
    }

    pub fn get_providers(&self) -> Vec<Arc<dyn Provider>> {
        self.inner.models.get_providers()
    }

    pub fn get_provider(&self, provider_id: &str) -> Option<Arc<dyn Provider>> {
        self.inner.models.get_provider(provider_id)
    }

    pub fn get_models(&self, provider_id: Option<&str>) -> Vec<Model> {
        self.inner.models.get_models(provider_id)
    }

    pub fn get_model(&self, provider_id: impl Into<JsString>, model_id: impl Into<JsString>) -> Option<Model> {
        let provider_id = provider_id.into();
        let model_id = model_id.into();
        let provider = self
            .inner
            .models
            .get_providers()
            .into_iter()
            .find(|provider| JsString::from(provider.id()) == provider_id)?;
        self.inner
            .models
            .get_models(Some(provider.id()))
            .into_iter()
            .find(|model| JsString::from(&model.id) == model_id)
    }

    pub async fn check_auth(
        &self,
        provider_id: &str,
        options: AuthOperationOptions,
    ) -> Result<Option<AuthCheck>, ResolveError> {
        self.inner.models.check_auth(provider_id, options).await
    }

    pub async fn get_available(
        &self,
        provider_id: Option<&str>,
        options: AuthOperationOptions,
    ) -> Result<Vec<Model>, ResolveError> {
        if let Some(provider_id) = provider_id {
            let error_seq = self.inner.availability_error_seq.fetch_add(1, Ordering::SeqCst) + 1;
            let result = self
                .inner
                .models
                .get_available(Some(provider_id), options.clone())
                .await;
            match result {
                Ok(available) => {
                    if error_seq == self.inner.availability_error_seq.load(Ordering::SeqCst) {
                        *lock(&self.inner.availability_error) = None;
                    }
                    Ok(available)
                }
                Err(error) => {
                    if error_seq == self.inner.availability_error_seq.load(Ordering::SeqCst)
                        && !options.signal.as_ref().is_some_and(CancellationToken::is_cancelled)
                    {
                        *lock(&self.inner.availability_error) = Some(resolve_error_message(&error));
                    }
                    Err(error)
                }
            }
        } else {
            self.queue_availability_refresh(options.signal).await?;
            Ok(lock(&self.inner.snapshot).available.clone())
        }
    }

    pub fn get_available_snapshot(&self) -> Vec<Model> {
        lock(&self.inner.snapshot).available.clone()
    }

    pub fn get_error(&self) -> Option<JsString> {
        let mut errors: Vec<JsString> = Vec::new();
        if let Some(config_error) = lock(&self.inner.config).get_error() {
            errors.push(config_error.into());
        }
        for (provider_id, error) in lock(&self.inner.composition_errors).iter() {
            errors.push(format!("Provider \"{provider_id}\": {error}").into());
        }
        if let Some(error) = lock(&self.inner.availability_error).as_ref() {
            errors.push(JsString::from("Availability refresh: ") + error);
        }
        (!errors.is_empty()).then(|| JsString::join(errors, "\n\n"))
    }

    pub fn get_registered_provider_config(&self, provider_id: &str) -> Option<ProviderConfigInput> {
        lock(&self.inner.extension_providers)
            .iter()
            .find(|(id, _)| id == provider_id)
            .map(|(_, config)| config.clone())
    }

    pub fn get_registered_provider_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = Vec::new();
        for (id, _) in lock(&self.inner.extension_providers).iter() {
            push_unique_id(&mut ids, id);
        }
        for (id, _) in lock(&self.inner.native_extension_providers).iter() {
            push_unique_id(&mut ids, id);
        }
        ids
    }

    pub fn get_registered_native_provider(&self, provider_id: &str) -> Option<Arc<dyn Provider>> {
        find_provider(&lock(&self.inner.native_extension_providers), provider_id)
    }

    fn provider_config(&self, provider_id: &str) -> Option<ModelsJsonProvider> {
        crate::core::provider_composer::models_json_provider(&lock(&self.inner.config), provider_id)
            .ok()
            .flatten()
    }

    fn extension_config(&self, provider_id: &str) -> Option<ProviderConfigInput> {
        lock(&self.inner.extension_providers)
            .iter()
            .find(|(id, _)| id == provider_id)
            .map(|(_, config)| config.clone())
    }

    /// Compatibility fallback for `ModelRegistry` when provider auth is unconfigured.
    pub fn get_compatibility_request_config(
        &self,
        model: &Model,
    ) -> Result<CompatibilityRequestConfig, ModelRuntimeError> {
        let config = self.provider_config(&model.provider);
        let extension = self.extension_config(&model.provider);
        resolve_compatibility_request_config(model, config.as_ref(), extension.as_ref())
            .map_err(|error| ModelRuntimeError::Message(error.to_string().into()))
    }

    pub fn is_using_oauth(&self, provider_id: &str) -> bool {
        lock(&self.inner.snapshot)
            .auth
            .get(provider_id)
            .is_some_and(|check| check.auth_type == AuthType::Oauth)
    }

    pub fn is_using_subscription(&self, provider_id: &str) -> bool {
        self.is_using_oauth(provider_id)
            && self
                .inner
                .models
                .get_provider(provider_id)
                .and_then(|provider| provider.auth().oauth.clone())
                .is_some_and(|oauth| oauth.is_subscription())
    }

    pub fn has_configured_auth(&self, provider_id: &str) -> bool {
        lock(&self.inner.snapshot).configured_providers.contains(provider_id)
    }

    pub async fn get_auth(
        &self,
        provider_id: &str,
        overrides: AuthResolutionOverrides,
    ) -> Result<Option<AuthResult>, ResolveError> {
        self.inner.models.get_auth(provider_id, overrides).await
    }

    pub async fn get_auth_for_model(
        &self,
        model: &Model,
        overrides: AuthResolutionOverrides,
    ) -> Result<Option<AuthResult>, ResolveError> {
        let resolution = self.inner.models.get_auth_for_model(model, overrides.clone()).await?;
        let Some(resolution) = resolution else {
            return Ok(None);
        };
        let config = self.provider_config(&model.provider);
        let extension = self.extension_config(&model.provider);
        let mut env = resolution.env.clone().unwrap_or_default();
        if let Some(overrides_env) = &overrides.env {
            env.extend(overrides_env.clone());
        }
        let env_map: HashMap<String, JsString> = env.iter().map(|(key, value)| (key.clone(), value.clone())).collect();
        let configured = resolve_configured_model_headers(
            model,
            config.as_ref(),
            extension.as_ref(),
            (!env_map.is_empty()).then_some(&env_map),
        )
        .map_err(|error| resolve_error(ModelsErrorCode::Auth, error.to_string()))?;
        let configured_headers = configured.map(|headers| {
            headers
                .into_iter()
                .map(|(name, value)| (name, Some(value)))
                .collect::<ProviderHeaders>()
        });
        Ok(Some(AuthResult {
            auth: pi_ai::auth::types::ModelAuth {
                headers: merge_headers(resolution.auth.headers, configured_headers),
                ..resolution.auth
            },
            ..resolution
        }))
    }

    fn credential_gate(&self, provider_id: &str) -> Arc<AsyncMutex<()>> {
        let mut gates = lock(&self.inner.credential_gates);
        Arc::clone(
            gates
                .entry(provider_id.to_owned())
                .or_insert_with(|| Arc::new(AsyncMutex::new(()))),
        )
    }

    async fn enqueue_credential_operation<T, F>(
        &self,
        provider_id: &str,
        signal: CancellationToken,
        task: F,
    ) -> Result<T, ModelRuntimeError>
    where
        F: FnOnce() -> BoxFuture<'static, Result<T, ModelRuntimeError>>,
    {
        let gate = self.credential_gate(provider_id);
        let guard = tokio::select! {
            guard = gate.lock() => guard,
            _ = signal.cancelled() => return Err(ModelRuntimeError::Aborted),
        };
        if signal.is_cancelled() {
            return Err(ModelRuntimeError::Aborted);
        }
        let result = task().await;
        drop(guard);
        result
    }

    async fn synchronize_credential_state(
        &self,
        provider_id: &str,
        operation: CredentialSynchronizationOperation,
        credential: Option<Credential>,
        signal: CancellationToken,
    ) -> Result<(), ModelRuntimeError> {
        let result = self
            .synchronize_credential_state_inner(provider_id, signal.clone())
            .await;
        match result {
            Ok(()) => Ok(()),
            Err(cause) => Err(ModelRuntimeError::CredentialSync(Box::new(
                CredentialSynchronizationError {
                    provider_id: provider_id.to_owned(),
                    operation,
                    credential,
                    cause: cause.message(),
                },
            ))),
        }
    }

    async fn synchronize_credential_state_inner(
        &self,
        provider_id: &str,
        signal: CancellationToken,
    ) -> Result<(), ModelRuntimeError> {
        if signal.is_cancelled() {
            return Err(ModelRuntimeError::Aborted);
        }
        self.recompose_provider(provider_id);
        if let Some(error) = lock(&self.inner.composition_errors).get(provider_id) {
            return Err(ModelRuntimeError::Message(error.clone().into()));
        }
        let result = self
            .inner
            .models
            .refresh(ModelsRefreshOptions {
                allow_network: Some(false),
                providers: Some(vec![provider_id.to_owned()]),
                force: None,
                signal: Some(signal.clone()),
            })
            .await;
        if result.aborted {
            return Err(ModelRuntimeError::Aborted);
        }
        if let Some(error) = result.errors.get(provider_id) {
            return Err(ModelRuntimeError::Message(error.message.clone()));
        }
        self.update_model_snapshot();
        self.refresh_provider_availability(provider_id, signal)
            .await
            .map_err(ModelRuntimeError::Resolve)
    }

    pub async fn set_runtime_api_key(
        &self,
        provider_id: &str,
        api_key: &JsString,
        options: AuthOperationOptions,
    ) -> Result<(), ModelRuntimeError> {
        let signal = options.signal.unwrap_or_default();
        let provider_id = provider_id.to_owned();
        let api_key = api_key.to_owned();
        let runtime = self.clone();
        let task_provider_id = provider_id.clone();
        self.enqueue_credential_operation(&provider_id, signal.clone(), move || {
            let runtime = runtime.clone();
            let provider_id = task_provider_id;
            Box::pin(async move {
                runtime.inner.credentials.set_runtime_api_key(&provider_id, &api_key);
                runtime
                    .synchronize_credential_state(
                        &provider_id,
                        CredentialSynchronizationOperation::SetRuntimeApiKey,
                        Some(Credential::ApiKey(pi_ai::auth::types::ApiKeyCredential {
                            key: Some(api_key),
                            env: None,
                        })),
                        signal,
                    )
                    .await
            })
        })
        .await
    }

    pub async fn remove_runtime_api_key(
        &self,
        provider_id: &str,
        options: AuthOperationOptions,
    ) -> Result<(), ModelRuntimeError> {
        let signal = options.signal.unwrap_or_default();
        let provider_id = provider_id.to_owned();
        let runtime = self.clone();
        let task_provider_id = provider_id.clone();
        self.enqueue_credential_operation(&provider_id, signal.clone(), move || {
            let runtime = runtime.clone();
            let provider_id = task_provider_id;
            Box::pin(async move {
                runtime.inner.credentials.remove_runtime_api_key(&provider_id);
                runtime
                    .synchronize_credential_state(
                        &provider_id,
                        CredentialSynchronizationOperation::RemoveRuntimeApiKey,
                        None,
                        signal,
                    )
                    .await
            })
        })
        .await
    }

    pub async fn list_credentials(
        &self,
        options: AuthOperationOptions,
    ) -> Result<Vec<CredentialInfo>, pi_ai::auth::types::AuthError> {
        self.inner.credentials.list(options).await
    }

    pub fn get_provider_auth_status(&self, provider_id: &str) -> AuthStatus {
        if self.inner.credentials.has_runtime_api_key(provider_id) {
            return AuthStatus {
                configured: true,
                source: Some(AuthStatusSource::Runtime),
                label: None,
            };
        }
        if lock(&self.inner.snapshot).stored_providers.contains(provider_id) {
            return AuthStatus {
                configured: true,
                source: Some(AuthStatusSource::Stored),
                label: None,
            };
        }
        let config = self.provider_config(provider_id);
        let extension = self.extension_config(provider_id);
        if let Some(status) = configured_request_auth_status(config.as_ref(), extension.as_ref()) {
            return status;
        }
        match lock(&self.inner.snapshot).auth.get(provider_id) {
            Some(check) => AuthStatus {
                configured: true,
                source: Some(AuthStatusSource::Environment),
                label: check.source.clone(),
            },
            None => AuthStatus {
                configured: false,
                source: None,
                label: None,
            },
        }
    }

    async fn prepare_request(&self, model: &Model, options: StreamOptions) -> Result<PreparedRequest, ResolveError> {
        let provider = self.inner.models.get_provider(&model.provider).ok_or_else(|| {
            resolve_error(
                ModelsErrorCode::Provider,
                format!("Unknown provider: {}", model.provider),
            )
        })?;
        let overrides = AuthResolutionOverrides {
            api_key: options.request.api_key.clone(),
            env: options.request.env.clone(),
            min_oauth_validity_ms: None,
            signal: options.request.signal.clone(),
        };
        let resolution = self.get_auth_for_model(model, overrides).await?.ok_or_else(|| {
            resolve_error(
                ModelsErrorCode::Auth,
                format!("Provider is not configured: {}", model.provider),
            )
        })?;
        let mut headers = merge_headers(resolution.auth.headers.clone(), options.request.headers.clone());
        if let Some(transform) = options.request.transform_headers.clone() {
            headers = Some(
                transform(headers.unwrap_or_default())
                    .await
                    .map_err(ResolveError::Callback)?,
            );
        }
        let env: Option<ProviderEnv> = match (&resolution.env, &options.request.env) {
            (None, None) => None,
            (resolution_env, options_env) => {
                let mut merged = resolution_env.clone().unwrap_or_default();
                if let Some(options_env) = options_env {
                    merged.extend(options_env.clone());
                }
                Some(merged)
            }
        };
        let mut request_model = model.clone();
        if let Some(base_url) = &resolution.auth.base_url {
            request_model.base_url = base_url.clone();
        }
        let mut prepared_options = options;
        prepared_options.request.transform_headers = None;
        prepared_options.request.api_key = prepared_options
            .request
            .api_key
            .or_else(|| resolution.auth.api_key.clone());
        prepared_options.request.headers = headers;
        prepared_options.request.env = env;
        Ok(PreparedRequest {
            provider,
            model: request_model,
            options: prepared_options,
        })
    }

    pub fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        let runtime = self.clone();
        pi_ai::api::lazy::lazy_stream(model.clone(), move || {
            Box::pin(async move {
                let prepared = runtime
                    .prepare_request(&model, options)
                    .await
                    .map_err(|error| CallbackError::new(resolve_error_message(&error)))?;
                Ok(prepared.provider.stream(prepared.model, context, prepared.options))
            })
        })
    }

    pub async fn complete(
        &self,
        model: Model,
        context: Context,
        options: StreamOptions,
    ) -> Result<AssistantMessage, StreamFailure> {
        self.stream(model, context, options).result().await
    }

    pub fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        let runtime = self.clone();
        pi_ai::api::lazy::lazy_stream(model.clone(), move || {
            Box::pin(async move {
                let prepared = runtime
                    .prepare_request(&model, options.stream.clone())
                    .await
                    .map_err(|error| CallbackError::new(resolve_error_message(&error)))?;
                let prepared_options = SimpleStreamOptions {
                    stream: prepared.options,
                    ..options
                };
                Ok(prepared
                    .provider
                    .stream_simple(prepared.model, context, prepared_options))
            })
        })
    }

    pub async fn complete_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> Result<AssistantMessage, StreamFailure> {
        self.stream_simple(model, context, options).result().await
    }

    pub async fn fetch_deferred(
        &self,
        model: Model,
        handle: DeferredHandle,
        options: DeferredFetchOptions,
    ) -> Result<AssistantMessage, StreamFailure> {
        let runtime = self.clone();
        let request_model = model.clone();
        pi_ai::api::lazy::lazy_stream(model, move || {
            Box::pin(async move {
                let prepared = runtime
                    .prepare_request(
                        &request_model,
                        StreamOptions {
                            request: options.request.clone(),
                            ..Default::default()
                        },
                    )
                    .await
                    .map_err(|error| CallbackError::new(resolve_error_message(&error)))?;
                if !prepared.provider.supports_fetch_deferred() {
                    return Err(CallbackError::new(format!(
                        "Provider {} does not support deferred responses",
                        request_model.provider
                    )));
                }
                Ok(prepared.provider.fetch_deferred(prepared.model, handle, options))
            })
        })
        .result()
        .await
    }

    pub async fn cancel_deferred(
        &self,
        model: Model,
        handle: DeferredHandle,
        options: DeferredCancelOptions,
    ) -> Result<(), ModelRuntimeError> {
        let prepared = self
            .prepare_request(
                &model,
                StreamOptions {
                    request: options.clone(),
                    ..Default::default()
                },
            )
            .await
            .map_err(ModelRuntimeError::Resolve)?;
        if !prepared.provider.supports_cancel_deferred() {
            return Err(ModelRuntimeError::Message(
                format!("Provider {} does not support deferred responses", model.provider).into(),
            ));
        }
        prepared
            .provider
            .cancel_deferred(prepared.model, handle, options)
            .await
            .map_err(|error| ModelRuntimeError::Resolve(ResolveError::Callback(error)))
    }

    pub async fn login(
        &self,
        provider_id: &str,
        auth_type: AuthType,
        interaction: Arc<dyn AuthInteraction>,
    ) -> Result<Credential, ModelRuntimeError> {
        let signal = interaction.signal();
        let provider_id_owned = provider_id.to_owned();
        let runtime = self.clone();
        self.enqueue_credential_operation(provider_id, signal.clone(), move || {
            let runtime = runtime.clone();
            let provider_id = provider_id_owned.clone();
            Box::pin(async move {
                let credential = runtime
                    .inner
                    .models
                    .login(&provider_id, auth_type, interaction)
                    .await
                    .map_err(ModelRuntimeError::Resolve)?;
                runtime
                    .synchronize_credential_state(
                        &provider_id,
                        CredentialSynchronizationOperation::Login,
                        Some(credential.clone()),
                        signal,
                    )
                    .await?;
                Ok(credential)
            })
        })
        .await
    }

    pub async fn logout(&self, provider_id: &str, options: AuthOperationOptions) -> Result<(), ModelRuntimeError> {
        let signal = options.signal.unwrap_or_default();
        let provider_id_owned = provider_id.to_owned();
        let runtime = self.clone();
        self.enqueue_credential_operation(provider_id, signal.clone(), move || {
            let runtime = runtime.clone();
            let provider_id = provider_id_owned.clone();
            Box::pin(async move {
                runtime
                    .inner
                    .models
                    .logout(&provider_id, AuthOperationOptions::new(Some(signal.clone())))
                    .await
                    .map_err(ModelRuntimeError::Resolve)?;
                runtime
                    .synchronize_credential_state(
                        &provider_id,
                        CredentialSynchronizationOperation::Logout,
                        None,
                        signal,
                    )
                    .await
            })
        })
        .await
    }

    pub async fn reload_config(&self) -> Result<(), ModelRuntimeError> {
        if let Some(path) = &self.inner.auth_path {
            AuthStorage::create(Some(path)).reload();
        }
        let config = ModelConfig::load(self.inner.models_path.as_ref());
        *lock(&self.inner.config) = config;
        self.reset_builtin_providers();
        self.rebuild_providers();
        self.refresh(ModelsRefreshOptions {
            allow_network: Some(false),
            ..Default::default()
        })
        .await?;
        Ok(())
    }

    pub async fn refresh(&self, options: ModelsRefreshOptions) -> Result<ModelsRefreshResult, ModelRuntimeError> {
        let config = ModelConfig::load(self.inner.models_path.as_ref());
        *lock(&self.inner.config) = config;
        self.reset_builtin_providers();
        if let Some(providers) = &options.providers {
            let mut seen: Vec<String> = Vec::new();
            for provider_id in providers {
                if !seen.contains(provider_id) {
                    seen.push(provider_id.clone());
                }
            }
            for provider_id in seen {
                self.recompose_provider(&provider_id);
            }
            self.update_model_snapshot();
        } else {
            self.rebuild_providers();
        }
        let refresh_options = ModelsRefreshOptions {
            allow_network: Some(options.allow_network.unwrap_or(self.inner.model_network_enabled)),
            ..options.clone()
        };
        let result = self.inner.models.refresh(refresh_options.clone()).await;
        let mut errors = result.errors;
        self.update_model_snapshot();
        match &options.providers {
            Some(providers) => {
                let mut seen: Vec<String> = Vec::new();
                for provider_id in providers {
                    if seen.contains(provider_id) {
                        continue;
                    }
                    seen.push(provider_id.clone());
                    if let Err(error) = self
                        .refresh_provider_availability(provider_id, refresh_options.signal.clone().unwrap_or_default())
                        .await
                        && !options.signal.as_ref().is_some_and(CancellationToken::is_cancelled)
                    {
                        errors.insert(provider_id.clone(), CallbackError::new(resolve_error_message(&error)));
                    }
                }
            }
            None => {
                let _ = self.queue_availability_refresh(options.signal.clone()).await;
            }
        }
        Ok(ModelsRefreshResult {
            aborted: result.aborted || options.signal.as_ref().is_some_and(CancellationToken::is_cancelled),
            errors,
        })
    }

    pub fn register_native_provider(&self, provider: Arc<dyn Provider>) {
        let provider_id = provider.id().to_owned();
        if provider_id.trim().is_empty() {
            return;
        }
        lock(&self.inner.extension_providers).retain(|(id, _)| id != &provider_id);
        upsert_provider(
            &mut lock(&self.inner.native_extension_providers),
            provider_id.clone(),
            provider,
        );
        self.recompose_provider(&provider_id);
        self.update_model_snapshot();
        self.spawn_refresh();
    }

    pub fn register_provider(&self, provider_id: &str, config: ProviderConfigInput) -> Result<(), ModelRuntimeError> {
        let base = find_provider(&lock(&self.inner.builtins), provider_id);
        let model_config = lock(&self.inner.config).clone();
        let models_config = crate::core::provider_composer::models_json_provider(&model_config, provider_id)
            .ok()
            .flatten();
        validate_extension_provider(provider_id, base.as_deref(), models_config.as_ref(), &config)
            .map_err(|error| ModelRuntimeError::Message(error.to_string().into()))?;
        lock(&self.inner.native_extension_providers).retain(|(id, _)| id != provider_id);
        let previous = lock(&self.inner.extension_providers)
            .iter()
            .find(|(id, _)| id == provider_id)
            .map(|(_, config)| config.clone());
        let effective = merge_provider_config(previous, config);
        {
            let mut providers = lock(&self.inner.extension_providers);
            match providers.iter_mut().find(|(id, _)| id == provider_id) {
                Some(entry) => entry.1 = effective.clone(),
                None => providers.push((provider_id.to_owned(), effective.clone())),
            }
        }
        self.recompose_provider(provider_id);
        self.update_model_snapshot();
        let config = self.provider_config(provider_id);
        let configured = lock(&self.inner.snapshot).stored_providers.contains(provider_id)
            || configured_request_auth_status(config.as_ref(), Some(&effective))
                .is_some_and(|status| status.configured);
        if configured {
            let mut snapshot = lock(&self.inner.snapshot);
            snapshot.configured_providers.insert(provider_id.to_owned());
            if !snapshot.auth.contains_key(provider_id) {
                snapshot.auth.insert(
                    provider_id.to_owned(),
                    AuthCheck {
                        source: Some("configured provider".into()),
                        auth_type: if effective.oauth.is_some() && effective.api_key.is_none() {
                            AuthType::Oauth
                        } else {
                            AuthType::ApiKey
                        },
                    },
                );
            }
            let configured = snapshot.configured_providers.clone();
            snapshot.available = snapshot
                .all
                .iter()
                .filter(|model| configured.contains(&model.provider))
                .cloned()
                .collect();
        }
        self.spawn_refresh();
        Ok(())
    }

    pub fn unregister_provider(&self, provider_id: &str) {
        lock(&self.inner.extension_providers).retain(|(id, _)| id != provider_id);
        lock(&self.inner.native_extension_providers).retain(|(id, _)| id != provider_id);
        self.recompose_provider(provider_id);
        self.update_model_snapshot();
        self.spawn_refresh();
    }

    fn spawn_refresh(&self) {
        let runtime = self.clone();
        if tokio::runtime::Handle::try_current().is_ok() {
            tokio::spawn(async move {
                let _ = runtime
                    .refresh(ModelsRefreshOptions {
                        allow_network: Some(false),
                        ..Default::default()
                    })
                    .await;
            });
        }
    }
}

impl ModelResolverRuntime for ModelRuntime {
    fn get_models(&self) -> Vec<Model> {
        ModelRuntime::get_models(self, None)
    }

    fn get_available_snapshot(&self) -> Vec<Model> {
        ModelRuntime::get_available_snapshot(self)
    }

    fn get_model(&self, provider: &str, model_id: &str) -> Option<Model> {
        ModelRuntime::get_model(self, provider, model_id)
    }

    fn has_configured_auth(&self, provider: &str) -> bool {
        ModelRuntime::has_configured_auth(self, provider)
    }

    fn available_models(&self, options: AuthOperationOptions) -> BoxFuture<'_, Vec<Model>> {
        Box::pin(async move { self.get_available(None, options).await.unwrap_or_default() })
    }
}

fn push_unique_id(ids: &mut Vec<String>, id: &str) {
    if !ids.iter().any(|existing| existing == id) {
        ids.push(id.to_owned());
    }
}

fn find_provider(providers: &[(String, Arc<dyn Provider>)], provider_id: &str) -> Option<Arc<dyn Provider>> {
    providers
        .iter()
        .find(|(id, _)| id == provider_id)
        .map(|(_, provider)| Arc::clone(provider))
}

fn upsert_provider(providers: &mut Vec<(String, Arc<dyn Provider>)>, provider_id: String, provider: Arc<dyn Provider>) {
    match providers.iter_mut().find(|(id, _)| *id == provider_id) {
        Some(entry) => entry.1 = provider,
        None => providers.push((provider_id, provider)),
    }
}

fn merge_provider_config(previous: Option<ProviderConfigInput>, incoming: ProviderConfigInput) -> ProviderConfigInput {
    let mut merged = previous.unwrap_or_default();
    if incoming.name.is_some() {
        merged.name = incoming.name;
    }
    if incoming.base_url.is_some() {
        merged.base_url = incoming.base_url;
    }
    if incoming.api_key.is_some() {
        merged.api_key = incoming.api_key;
    }
    if incoming.api.is_some() {
        merged.api = incoming.api;
    }
    if incoming.stream_simple.is_some() {
        merged.stream_simple = incoming.stream_simple;
    }
    if incoming.headers.is_some() {
        merged.headers = incoming.headers;
    }
    if incoming.auth_header.is_some() {
        merged.auth_header = incoming.auth_header;
    }
    if incoming.oauth.is_some() {
        merged.oauth = incoming.oauth;
    }
    if incoming.models.is_some() {
        merged.models = incoming.models;
    }
    if incoming.refresh_models.is_some() {
        merged.refresh_models = incoming.refresh_models;
    }
    merged
}

/// Resolves `models.json` providers into typed values (used by registry tests).
pub fn typed_provider_config(config: &ModelConfig, provider_id: &ProviderId) -> Option<ModelsJsonProvider> {
    crate::core::provider_composer::models_json_provider(config, provider_id)
        .ok()
        .flatten()
}

/// Empty JSON object helper for callers that build `models.json` fixtures.
pub fn empty_json_object() -> Map<String, serde_json::Value> {
    Map::new()
}

/// Models error constructor used by compatibility call sites.
pub fn models_error(code: ModelsErrorCode, message: impl Into<JsString>) -> ModelsError {
    ModelsError::new(code, message)
}
