//! Frozen `core/model-registry.ts`: synchronous catalog and live AuthStorage queries.

mod schema;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use indexmap::IndexMap;
use pi_ai::compat::{self, ApiProvider};
use pi_ai::oauth::{OAuthCredentials, OAuthLoginCallbacks, OAuthProviderInterface};
use pi_ai::request::{BoxFuture, CallbackError, SimpleStreamOptions};
use pi_ai::types::{Context, Model, ProviderEnv, ProviderHeaders};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue};

use crate::core::auth_storage::{AuthStatus, AuthStorage, GetApiKeyOptions, StorageError, stringify_auth_rejection};
use crate::core::config_value::{
    get_config_value_env_var_names, is_command_config_value, is_config_value_configured, resolve_config_value_or_throw,
    resolve_config_value_uncached,
};

pub use crate::core::config_value::clear_config_value_cache as clear_api_key_cache;

pub type StreamSimpleFn =
    Arc<dyn Fn(Model, Context, Option<SimpleStreamOptions>) -> AssistantMessageEventStream + Send + Sync>;
pub type ExtensionOAuthLoginFn = Arc<
    dyn Fn(Arc<dyn OAuthLoginCallbacks>) -> BoxFuture<'static, Result<OAuthCredentials, pi_ai::auth::AuthError>>
        + Send
        + Sync,
>;
pub type ExtensionOAuthRefreshFn = Arc<
    dyn Fn(OAuthCredentials, AbortSignal) -> BoxFuture<'static, Result<OAuthCredentials, pi_ai::auth::AuthError>>
        + Send
        + Sync,
>;
pub type ModifyModelsFn = Arc<dyn Fn(Vec<Model>, &OAuthCredentials) -> Vec<Model> + Send + Sync>;

#[derive(Clone)]
pub struct ExtensionOAuthConfig {
    pub name: String,
    pub uses_callback_server: Option<bool>,
    pub login: ExtensionOAuthLoginFn,
    pub refresh_token: ExtensionOAuthRefreshFn,
    pub get_api_key: Arc<dyn Fn(&OAuthCredentials) -> Result<JsString, CallbackError> + Send + Sync>,
    pub modify_models: Option<ModifyModelsFn>,
}

/// Native representation of the frozen extension registration object.
#[derive(Clone, Default)]
pub struct ProviderConfigInput {
    pub name: Option<JsString>,
    pub base_url: Option<JsString>,
    pub api_key: Option<JsString>,
    pub api: Option<String>,
    pub stream_simple: Option<StreamSimpleFn>,
    pub headers: Option<IndexMap<String, JsString>>,
    pub auth_header: Option<bool>,
    pub oauth: Option<ExtensionOAuthConfig>,
    pub models: Option<Vec<Model>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedRequestAuth {
    Ok {
        api_key: Option<JsString>,
        headers: Option<ProviderHeaders>,
        env: Option<ProviderEnv>,
    },
    Err {
        error: JsString,
    },
}
impl ResolvedRequestAuth {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok { .. })
    }
    pub fn error(&self) -> Option<&JsString> {
        match self {
            Self::Ok { .. } => None,
            Self::Err { error } => Some(error),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRegistryError(JsString);
impl ModelRegistryError {
    pub fn message(&self) -> &JsString {
        &self.0
    }
}
impl std::fmt::Display for ModelRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&js_json::encode_node_utf8(&self.0))
    }
}
impl std::error::Error for ModelRegistryError {}

#[derive(Clone, Default)]
struct ProviderRequestConfig {
    api_key: Option<JsString>,
    headers: Option<IndexMap<String, JsString>>,
    auth_header: bool,
}
#[derive(Default)]
struct RegistryState {
    models: Vec<Model>,
    provider_requests: IndexMap<String, ProviderRequestConfig>,
    model_headers: IndexMap<String, IndexMap<String, JsString>>,
    registered: IndexMap<String, ProviderConfigInput>,
    error: Option<JsString>,
}

#[derive(Clone)]
pub struct ModelRegistry {
    auth_storage: AuthStorage,
    models_path: Option<JsString>,
    state: Arc<Mutex<RegistryState>>,
}
fn lock<T>(value: &Mutex<T>) -> MutexGuard<'_, T> {
    value.lock().expect("model registry lock poisoned")
}

impl ModelRegistry {
    pub fn create(
        auth_storage: AuthStorage,
        models_path: Option<JsString>,
    ) -> Result<Self, crate::config::ConfigError> {
        let path = match models_path {
            Some(path) => path,
            None => crate::utils::paths::node_join(crate::config::get_agent_dir()?, "models.json"),
        };
        Ok(Self::with_path(auth_storage, Some(path)))
    }
    pub fn in_memory(auth_storage: AuthStorage) -> Self {
        Self::with_path(auth_storage, None)
    }
    fn with_path(auth_storage: AuthStorage, models_path: Option<JsString>) -> Self {
        let registry = Self {
            auth_storage,
            models_path,
            state: Arc::new(Mutex::new(RegistryState::default())),
        };
        registry.load_models();
        registry
    }
    pub fn auth_storage(&self) -> &AuthStorage {
        &self.auth_storage
    }
    pub fn refresh(&self) {
        self.auth_storage.reload();
        {
            let mut state = lock(&self.state);
            state.provider_requests.clear();
            state.model_headers.clear();
            state.error = None;
        }
        compat::reset_api_providers();
        pi_ai::oauth::reset_oauth_providers();
        self.load_models();
        let registered = lock(&self.state).registered.clone();
        for (provider, config) in registered {
            self.apply_provider_config(&provider, &config);
        }
    }
    pub fn get_error(&self) -> Option<JsString> {
        lock(&self.state).error.clone()
    }
    pub fn get_all(&self) -> Vec<Model> {
        lock(&self.state).models.clone()
    }
    pub fn get_available(&self) -> Result<Vec<Model>, pi_js_compat::node_error::NativeNodeError> {
        let mut available = Vec::new();
        for model in self.get_all() {
            if self.has_configured_auth(&model)? {
                available.push(model);
            }
        }
        Ok(available)
    }
    pub fn find(&self, provider: impl Into<JsString>, id: impl Into<JsString>) -> Option<Model> {
        let (provider, id) = (provider.into(), id.into());
        lock(&self.state)
            .models
            .iter()
            .find(|model| provider == model.provider.as_str() && id == model.id.as_str())
            .cloned()
    }
    pub fn has_configured_auth(&self, model: &Model) -> Result<bool, pi_js_compat::node_error::NativeNodeError> {
        Ok(self.auth_storage.has_auth(&model.provider)?
            || lock(&self.state)
                .provider_requests
                .get(&model.provider)
                .and_then(|config| config.api_key.as_ref())
                .is_some_and(|key| is_config_value_configured(key, None)))
    }
    pub fn is_using_oauth(&self, model: &Model) -> bool {
        self.auth_storage
            .get(model.provider.as_str())
            .and_then(|credential| credential.get("type").cloned())
            .as_ref()
            .and_then(JsValue::as_string)
            .is_some_and(|kind| kind == "oauth")
    }
    pub fn get_provider_auth_status(&self, provider: &str) -> AuthStatus {
        let status = self.auth_storage.get_auth_status(provider);
        if status.source.is_some() {
            return status;
        }
        let key = lock(&self.state)
            .provider_requests
            .get(provider)
            .and_then(|config| config.api_key.clone());
        let Some(key) = key.filter(|key| !key.is_empty()) else {
            return status;
        };
        if is_command_config_value(&key) {
            return AuthStatus {
                configured: true,
                source: Some("models_json_command".into()),
                label: None,
            };
        }
        let names = get_config_value_env_var_names(&key);
        if names.is_empty() {
            return AuthStatus {
                configured: true,
                source: Some("models_json_key".into()),
                label: None,
            };
        }
        if is_config_value_configured(&key, None) {
            AuthStatus {
                configured: true,
                source: Some("environment".into()),
                label: Some(names.join(", ").into()),
            }
        } else {
            AuthStatus {
                configured: false,
                source: None,
                label: None,
            }
        }
    }
    pub fn get_provider_display_name(&self, provider: &str) -> JsString {
        if let Some(config) = lock(&self.state).registered.get(provider) {
            if let Some(name) = &config.name {
                return name.clone();
            }
            if let Some(oauth) = &config.oauth {
                return oauth.name.as_str().into();
            }
        }
        if let Some(oauth) = self
            .auth_storage
            .get_oauth_providers()
            .iter()
            .find(|oauth| oauth.id() == provider)
        {
            return oauth.name().into();
        }
        crate::core::provider_display_names::built_in_provider_display_name(provider)
            .unwrap_or(provider)
            .into()
    }
    pub async fn get_api_key_for_provider(&self, provider: &str) -> Result<Option<JsString>, StorageError> {
        let key = self
            .auth_storage
            .get_api_key(provider, GetApiKeyOptions::default())
            .await?;
        if key.is_some() {
            return Ok(key);
        }
        let key = lock(&self.state)
            .provider_requests
            .get(provider)
            .and_then(|config| config.api_key.clone());
        let env = self.provider_env(provider)?;
        Ok(key
            .as_ref()
            .and_then(|key| resolve_config_value_uncached(key, env.as_ref())))
    }
    fn provider_env(&self, provider: &str) -> Result<Option<HashMap<String, JsString>>, StorageError> {
        self.auth_storage
            .get_provider_env(provider)
            .map(|value| js_json::from_value(&value).map_err(|error| StorageError::new(error.to_string())))
            .transpose()
    }
    pub async fn get_api_key_and_headers(&self, model: &Model) -> Result<ResolvedRequestAuth, CallbackError> {
        match self.resolve_request_auth(model).await {
            Ok(result) => Ok(result),
            Err(error) => Ok(ResolvedRequestAuth::Err {
                error: stringify_auth_rejection(&error)?,
            }),
        }
    }
    async fn resolve_request_auth(&self, model: &Model) -> Result<ResolvedRequestAuth, CallbackError> {
        let (config, model_headers) = {
            let state = lock(&self.state);
            (
                state
                    .provider_requests
                    .get(&model.provider)
                    .cloned()
                    .unwrap_or_default(),
                state
                    .model_headers
                    .get(&format!("{}:{}", model.provider, model.id))
                    .cloned(),
            )
        };
        let env = self
            .provider_env(&model.provider)
            .map_err(StorageError::into_callback)?;
        let key = self
            .auth_storage
            .get_api_key(
                &model.provider,
                GetApiKeyOptions {
                    include_fallback: false,
                    throw_on_refresh_failure: true,
                    ..Default::default()
                },
            )
            .await
            .map_err(StorageError::into_callback)?;
        let key = match key {
            Some(key) => Some(key),
            None => config
                .api_key
                .as_ref()
                .filter(|key| !key.is_empty())
                .map(|key| {
                    resolve_config_value_or_throw(
                        key,
                        &js_format!("API key for provider \"{}\"", model.provider),
                        env.as_ref(),
                    )
                })
                .transpose()
                .map_err(|error| CallbackError::new(error.message().clone()))?,
        };
        let mut headers: ProviderHeaders = model
            .headers
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(|(name, value)| (name, Some(value)))
            .collect();
        for (raw, description) in [
            (config.headers.as_ref(), js_format!("provider \"{}\"", model.provider)),
            (
                model_headers.as_ref(),
                js_format!("model \"{}/{}\"", model.provider, model.id),
            ),
        ] {
            if let Some(raw) = raw {
                for (name, value) in raw {
                    let resolved = resolve_config_value_or_throw(
                        value,
                        &js_format!("{} header \"{}\"", description, name),
                        env.as_ref(),
                    )
                    .map_err(|error| CallbackError::new(error.message().clone()))?;
                    headers.insert(name.clone(), Some(resolved));
                }
            }
        }
        if config.auth_header {
            let Some(key) = key.as_ref().filter(|key| !key.is_empty()) else {
                return Ok(ResolvedRequestAuth::Err {
                    error: js_format!("No API key found for \"{}\"", model.provider),
                });
            };
            headers.insert("Authorization".into(), Some(JsString::from("Bearer ") + key));
        }
        Ok(ResolvedRequestAuth::Ok {
            api_key: key,
            headers: (!headers.is_empty()).then_some(headers),
            env: env.filter(|env| !env.is_empty()).map(|env| env.into_iter().collect()),
        })
    }
    fn load_models(&self) {
        let config = self.load_custom_config();
        let mut state = lock(&self.state);
        let providers = match config {
            Ok(providers) => providers,
            Err(error) => {
                state.error = Some(error);
                JsObject::new()
            }
        };
        let providers = &providers;
        for (provider, config) in js_json::object_entries_lossless(providers) {
            let provider = provider.to_utf8().expect("provider identity validated");
            store_request_config(&mut state, &provider, config);
            if let Some(overrides) = config.get("modelOverrides").and_then(JsValue::as_object) {
                for (id, value) in js_json::object_entries_lossless(overrides) {
                    store_model_headers(&mut state, &provider, &id, value.get("headers"));
                }
            }
        }
        let mut models = Vec::new();
        for provider in compat::get_providers() {
            let config = providers.get(&JsString::from(provider.as_str()));
            let provider_models = if provider == "openrouter" {
                crate::core::model_catalog_cache::get_openrouter_catalog_models()
            } else {
                compat::get_models(&provider)
                    .iter()
                    .map(|model| js_json::to_value(model).expect("typed model serializes"))
                    .collect()
            };
            for mut value in provider_models {
                let model_id = value
                    .get("id")
                    .and_then(JsValue::as_string)
                    .expect("catalog model id")
                    .clone();
                if let Some(config) = config {
                    if config.get("baseUrl").is_some() || config.get("compat").is_some() {
                        let object = value.as_object_mut().expect("model object");
                        if let Some(base_url) = config.get("baseUrl") {
                            object.insert("baseUrl".into(), base_url.clone());
                        }
                        merge_model_compat(object, config.get("compat"));
                    }
                    if let Some(model_override) = config
                        .get("modelOverrides")
                        .and_then(JsValue::as_object)
                        .and_then(|overrides| overrides.get(&model_id))
                    {
                        apply_model_override(&mut value, model_override);
                    }
                }
                match js_json::from_value(&value) {
                    Ok(model) => models.push(model),
                    Err(error) => {
                        state.error = Some(if provider == "openrouter" {
                            js_format!(
                                "Invalid OpenRouter catalog model \"{}\": {}",
                                model_id,
                                error.to_string()
                            )
                        } else {
                            error.to_string().into()
                        });
                    }
                }
            }
        }
        for (provider, config) in js_json::object_entries_lossless(providers) {
            let provider = provider.to_utf8().expect("provider identity validated");
            let Some(definitions) = config.get("models").and_then(JsValue::as_array) else {
                continue;
            };
            let defaults = compat::get_models(&provider).into_iter().next();
            for definition in definitions {
                match parse_custom_model(&provider, definition, config, defaults.as_ref()) {
                    Ok(Some(model)) => {
                        store_model_headers(
                            &mut state,
                            &provider,
                            &JsString::from(model.id.as_str()),
                            definition.get("headers"),
                        );
                        if let Some(index) = models
                            .iter()
                            .position(|existing: &Model| existing.provider == provider && existing.id == model.id)
                        {
                            models[index] = model;
                        } else {
                            models.push(model);
                        }
                    }
                    Ok(None) => {}
                    Err(error) => state.error = Some(self.load_error(error)),
                }
            }
        }
        for oauth in self.auth_storage.get_oauth_providers() {
            if let Some(credential) = oauth_credential(&self.auth_storage, oauth.id()) {
                models = oauth.modify_models(models, &credential);
            }
        }
        state.models = models;
    }
    fn load_custom_config(&self) -> Result<JsObject, JsString> {
        let Some(path) = self.models_path.as_ref().filter(|path| !path.is_empty()) else {
            return Ok(JsObject::new());
        };
        let path = crate::utils::paths::normalize_path(path, &Default::default())
            .map_err(|error| self.load_error(error.message().clone()))?;
        let os_path = js_json::encode_node_utf8(&path);
        if !std::path::Path::new(&os_path).exists() {
            return Ok(JsObject::new());
        }
        let bytes = std::fs::read(os_path)
            .map_err(|error| self.load_error(pi_js_compat::node_fs::node_read_error(&error, &path)))?;
        // Node's UTF-8 file ingress repairs malformed bytes before JSON.parse.
        let content = String::from_utf8_lossy(&bytes);
        let value = js_json::parse_lossless(&crate::utils::json::strip_json_comments(&content))
            .map_err(|error| js_format!("Failed to parse models.json: {}\n\nFile: {}", error.to_string(), path))?;
        let errors = pi_ai::utils::validation::schema_error_lines(&schema::models_config_schema().into(), &value);
        if !errors.is_empty() {
            return Err(js_format!(
                "Invalid models.json schema:\n{}\n\nFile: {}",
                JsString::join(errors, "\n"),
                path
            ));
        }
        let providers = value
            .get("providers")
            .and_then(JsValue::as_object)
            .expect("validated providers")
            .clone();
        validate_config(&providers).map_err(|error| self.load_error(error))?;
        Ok(providers)
    }
    fn load_error(&self, error: JsString) -> JsString {
        js_format!(
            "Failed to load models.json: {}\n\nFile: {}",
            error,
            self.models_path.as_ref().expect("custom models have a path")
        )
    }
    pub fn register_provider(&self, provider: &str, config: ProviderConfigInput) -> Result<(), ModelRegistryError> {
        validate_provider_config(provider, &config)?;
        self.apply_provider_config(provider, &config);
        let mut state = lock(&self.state);
        if let Some(existing) = state.registered.get_mut(provider) {
            upsert_provider_config(existing, config);
        } else {
            state.registered.insert(provider.into(), config);
        }
        Ok(())
    }
    pub fn unregister_provider(&self, provider: &str) {
        if lock(&self.state).registered.shift_remove(provider).is_some() {
            self.refresh();
        }
    }
    fn apply_provider_config(&self, provider: &str, config: &ProviderConfigInput) {
        if let Some(oauth) = &config.oauth {
            pi_ai::oauth::register_oauth_provider(Arc::new(RegisteredOAuth {
                id: provider.into(),
                config: oauth.clone(),
            }));
        }
        if let Some(stream) = &config.stream_simple {
            let raw_stream = Arc::clone(stream);
            let simple_stream = Arc::clone(stream);
            compat::register_api_provider(
                ApiProvider::new(
                    config.api.as_deref().expect("validated api"),
                    move |model, context, options| {
                        raw_stream(
                            model,
                            context,
                            Some(SimpleStreamOptions {
                                stream: options,
                                ..Default::default()
                            }),
                        )
                    },
                    move |model, context, options| simple_stream(model, context, Some(options)),
                ),
                Some(format!("provider:{provider}")),
            );
        }
        let mut state = lock(&self.state);
        if config.api_key.as_ref().is_some_and(|key| !key.is_empty())
            || config.headers.is_some()
            || config.auth_header == Some(true)
        {
            state.provider_requests.insert(
                provider.into(),
                ProviderRequestConfig {
                    api_key: config.api_key.clone(),
                    headers: config.headers.clone(),
                    auth_header: config.auth_header.unwrap_or(false),
                },
            );
        }
        if let Some(models) = config.models.as_ref().filter(|models| !models.is_empty()) {
            state.models.retain(|model| model.provider != provider);
            for model in models {
                let headers = model.headers.as_ref().map(|headers| {
                    headers
                        .iter()
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect()
                });
                store_typed_model_headers(&mut state, provider, &model.id, headers);
                let mut model = model.clone();
                model.provider = provider.into();
                model.base_url = if model.base_url.is_empty() {
                    config.base_url.clone().expect("validated baseUrl")
                } else {
                    model.base_url
                };
                if model.api.is_empty() {
                    model.api = config.api.clone().expect("validated api");
                }
                model.headers = None;
                state.models.push(model);
            }
            if let Some(oauth) = &config.oauth {
                if let Some(modify) = &oauth.modify_models {
                    if let Some(credential) = oauth_credential(&self.auth_storage, provider) {
                        state.models = modify(std::mem::take(&mut state.models), &credential);
                    }
                }
            }
        } else if config.base_url.is_some() || config.headers.is_some() {
            for model in &mut state.models {
                if model.provider == provider {
                    if let Some(url) = &config.base_url {
                        model.base_url = url.clone();
                    }
                }
            }
        }
    }
}

fn oauth_credential(storage: &AuthStorage, provider: &str) -> Option<OAuthCredentials> {
    let value = storage.get(provider)?;
    if value.get("type")?.as_string()? != "oauth" {
        return None;
    }
    OAuthCredentials::from_js_value(&value).ok()
}

fn store_request_config(state: &mut RegistryState, provider: &str, config: &JsValue) {
    if config.get("apiKey").is_none()
        && config.get("headers").is_none()
        && config.get("authHeader").and_then(JsValue::as_bool) != Some(true)
    {
        return;
    }
    state.provider_requests.insert(
        provider.into(),
        ProviderRequestConfig {
            api_key: config.get("apiKey").and_then(JsValue::as_string).cloned(),
            headers: config
                .get("headers")
                .map(|value| js_json::from_value(value).expect("validated headers")),
            auth_header: config.get("authHeader").and_then(JsValue::as_bool).unwrap_or(false),
        },
    );
}
fn store_model_headers(state: &mut RegistryState, provider: &str, id: &JsString, headers: Option<&JsValue>) {
    let key = js_format!("{}:{}", provider, id);
    // Model and provider identities use the approved scalar domain.
    let key = key.to_utf8().expect("model identity must be scalar");
    let headers: Option<IndexMap<String, JsString>> =
        headers.map(|value| js_json::from_value(value).expect("validated headers"));
    if let Some(headers) = headers.filter(|headers| !headers.is_empty()) {
        state.model_headers.insert(key, headers);
    } else {
        state.model_headers.shift_remove(&key);
    }
}
fn store_typed_model_headers(
    state: &mut RegistryState,
    provider: &str,
    id: &str,
    headers: Option<IndexMap<String, JsString>>,
) {
    let key = format!("{provider}:{id}");
    if let Some(headers) = headers.filter(|headers| !headers.is_empty()) {
        state.model_headers.insert(key, headers);
    } else {
        state.model_headers.shift_remove(&key);
    }
}
fn merge_objects(base: Option<&JsValue>, overrides: Option<&JsValue>) -> Option<JsValue> {
    if overrides.is_none() {
        return base.cloned();
    }
    let mut result = base.and_then(JsValue::as_object).cloned().unwrap_or_default();
    if let Some(overrides) = overrides.and_then(JsValue::as_object) {
        for (key, value) in js_json::object_entries_lossless(overrides) {
            result.insert(key.clone(), value.clone());
        }
    }
    Some(JsValue::Object(result))
}
fn merge_compat(base: Option<&JsValue>, overrides: Option<&JsValue>) -> Option<JsValue> {
    let mut merged = merge_objects(base, overrides)?;
    if overrides.is_some() {
        for key in ["openRouterRouting", "vercelGatewayRouting", "chatTemplateKwargs"] {
            if let Some(value) = merge_objects(
                base.and_then(|base| base.get(key)),
                overrides.and_then(|overrides| overrides.get(key)),
            ) {
                merged.as_object_mut().unwrap().insert(key.into(), value);
            }
        }
    }
    Some(merged)
}
fn merge_model_compat(model: &mut JsObject, overrides: Option<&JsValue>) {
    let compat = merge_compat(model.get(&JsString::from("compat")), overrides);
    if let Some(compat) = compat {
        model.insert("compat".into(), compat);
    }
}
fn apply_model_override(model: &mut JsValue, overrides: &JsValue) {
    let object = model.as_object_mut().expect("model object");
    for key in ["name", "reasoning", "input", "contextWindow", "maxTokens"] {
        if let Some(value) = overrides.get(key) {
            object.insert(key.into(), value.clone());
        }
    }
    if let Some(value) = merge_objects(
        object.get(&JsString::from("thinkingLevelMap")),
        overrides.get("thinkingLevelMap"),
    ) {
        object.insert("thinkingLevelMap".into(), value);
    }
    if let Some(cost) = overrides.get("cost") {
        let base = object.get(&JsString::from("cost")).expect("model cost");
        let merged: JsObject = ["input", "output", "cacheRead", "cacheWrite"]
            .into_iter()
            .map(|key| {
                (
                    key.into(),
                    cost.get(key).or_else(|| base.get(key)).expect("cost field").clone(),
                )
            })
            .collect();
        object.insert("cost".into(), JsValue::Object(merged));
    }
    merge_model_compat(object, overrides.get("compat"));
}
fn parse_custom_model(
    provider: &str,
    definition: &JsValue,
    config: &JsValue,
    defaults: Option<&Model>,
) -> Result<Option<Model>, JsString> {
    let api = definition
        .get("api")
        .or_else(|| config.get("api"))
        .cloned()
        .or_else(|| defaults.map(|model| JsValue::String(model.api.as_str().into())));
    let url = definition
        .get("baseUrl")
        .or_else(|| config.get("baseUrl"))
        .cloned()
        .or_else(|| defaults.map(|model| JsValue::String(model.base_url.clone())));
    let (Some(api), Some(url)) = (api, url) else {
        return Ok(None);
    };
    let id = definition.get("id").expect("validated id");
    let mut object: JsObject = [
        ("id", id.clone()),
        ("name", definition.get("name").unwrap_or(id).clone()),
        ("api", api),
        ("provider", JsValue::String(provider.into())),
        ("baseUrl", url),
        (
            "reasoning",
            definition.get("reasoning").cloned().unwrap_or(JsValue::Bool(false)),
        ),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), value))
    .collect();
    if let Some(value) = definition.get("thinkingLevelMap") {
        object.insert("thinkingLevelMap".into(), value.clone());
    }
    for (key, default) in [
        ("input", serde_json::json!(["text"])),
        (
            "cost",
            serde_json::json!({"input":0,"output":0,"cacheRead":0,"cacheWrite":0}),
        ),
        ("contextWindow", serde_json::json!(128000)),
        ("maxTokens", serde_json::json!(16384)),
    ] {
        object.insert(key.into(), definition.get(key).cloned().unwrap_or(default.into()));
    }
    if let Some(compat) = merge_compat(config.get("compat"), definition.get("compat")) {
        object.insert("compat".into(), compat);
    }
    js_json::from_value(&JsValue::Object(object))
        .map(Some)
        .map_err(|error| error.to_string().into())
}
fn validate_config(providers: &JsObject) -> Result<(), JsString> {
    let builtins = compat::get_providers();
    for (provider, config) in js_json::object_entries_lossless(providers) {
        let provider_id = provider
            .to_utf8()
            .map_err(|_| JsString::from("models.json provider ID contains an unpaired UTF-16 unit"))?;
        let models = config
            .get("models")
            .and_then(JsValue::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        if models.is_empty() {
            if ["baseUrl", "headers", "compat"]
                .iter()
                .all(|key| config.get(key).is_none())
                && config
                    .get("modelOverrides")
                    .and_then(JsValue::as_object)
                    .is_none_or(|value| value.is_empty())
            {
                return Err(js_format!(
                    "Provider {}: must specify \"baseUrl\", \"headers\", \"compat\", \"modelOverrides\", or \"models\".",
                    provider
                ));
            }
        } else if !builtins.contains(&provider_id) && config.get("baseUrl").is_none() {
            return Err(js_format!(
                "Provider {}: \"baseUrl\" is required when defining custom models.",
                provider
            ));
        }
        for model in models {
            let id = model.get("id").and_then(JsValue::as_string).expect("validated id");
            if id.to_utf8().is_err() {
                return Err("model ID contains an unpaired UTF-16 unit".into());
            }
            if config.get("api").is_none() && model.get("api").is_none() && !builtins.contains(&provider_id) {
                return Err(js_format!(
                    "Provider {}, model {}: no \"api\" specified. Set at provider or model level.",
                    provider,
                    id
                ));
            }
            for field in ["contextWindow", "maxTokens"] {
                if model
                    .get(field)
                    .and_then(JsValue::as_f64)
                    .is_some_and(|value| value <= 0.0)
                {
                    return Err(js_format!("Provider {}, model {}: invalid {}", provider, id, field));
                }
            }
        }
    }
    Ok(())
}
fn validate_provider_config(provider: &str, config: &ProviderConfigInput) -> Result<(), ModelRegistryError> {
    if config.stream_simple.is_some() && config.api.as_ref().is_none_or(|api| api.is_empty()) {
        return Err(ModelRegistryError(js_format!(
            "Provider {}: \"api\" is required when registering streamSimple.",
            provider
        )));
    }
    let Some(models) = config.models.as_ref().filter(|models| !models.is_empty()) else {
        return Ok(());
    };
    if config.base_url.as_ref().is_none_or(JsString::is_empty) {
        return Err(ModelRegistryError(js_format!(
            "Provider {}: \"baseUrl\" is required when defining models.",
            provider
        )));
    }
    if config.api_key.as_ref().is_none_or(JsString::is_empty) && config.oauth.is_none() {
        return Err(ModelRegistryError(js_format!(
            "Provider {}: \"apiKey\" or \"oauth\" is required when defining models.",
            provider
        )));
    }
    for model in models {
        if model.api.is_empty() && config.api.as_ref().is_none_or(|api| api.is_empty()) {
            return Err(ModelRegistryError(js_format!(
                "Provider {}, model {}: no \"api\" specified.",
                provider,
                model.id
            )));
        }
    }
    Ok(())
}
fn upsert_provider_config(existing: &mut ProviderConfigInput, incoming: ProviderConfigInput) {
    macro_rules! update { ($($field:ident),*) => { $(if incoming.$field.is_some() { existing.$field = incoming.$field; })* }; }
    update!(
        name,
        base_url,
        api_key,
        api,
        stream_simple,
        headers,
        auth_header,
        oauth,
        models
    );
}
struct RegisteredOAuth {
    id: String,
    config: ExtensionOAuthConfig,
}
impl OAuthProviderInterface for RegisteredOAuth {
    fn id(&self) -> &str {
        &self.id
    }
    fn name(&self) -> &str {
        &self.config.name
    }
    fn uses_callback_server(&self) -> Option<bool> {
        self.config.uses_callback_server
    }
    fn login(
        &self,
        callbacks: Arc<dyn OAuthLoginCallbacks>,
    ) -> BoxFuture<'static, Result<OAuthCredentials, pi_ai::auth::AuthError>> {
        (self.config.login)(callbacks)
    }
    fn refresh_token(
        &self,
        credentials: OAuthCredentials,
    ) -> BoxFuture<'static, Result<OAuthCredentials, pi_ai::auth::AuthError>> {
        (self.config.refresh_token)(credentials, AbortSignal::default())
    }
    fn get_api_key(&self, credentials: &OAuthCredentials) -> Result<JsString, CallbackError> {
        (self.config.get_api_key)(credentials)
    }
    fn modify_models(&self, models: Vec<Model>, credentials: &OAuthCredentials) -> Vec<Model> {
        self.config
            .modify_models
            .as_ref()
            .map_or_else(|| models.clone(), |modify| modify(models.clone(), credentials))
    }
}
