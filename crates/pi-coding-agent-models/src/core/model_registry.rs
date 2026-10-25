//! Frozen `core/model-registry.ts`: synchronous catalog and live AuthStorage queries.

mod registered_providers;
mod schema;
use registered_providers::RegisteredProviders;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use indexmap::IndexMap;
use pi_ai::compat::{self, ApiProvider};
use pi_ai::oauth::{OAuthCredentials, OAuthLoginCallbacks, OAuthProviderInterface};
use pi_ai::request::{BoxFuture, CallbackError, SimpleStreamOptions};
use pi_ai::types::{
    Context, Modality, Model, ModelCompat, ModelCost, ProviderEnv, ProviderHeaders, ThinkingLevelMap,
    spread_compat_properties,
};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use pi_ai::utils::headers::validate_opaque_header_names;
use pi_js_compat::abort::AbortSignal;
use pi_js_compat::js_format;
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue};
use serde::{Deserialize, Serialize};

use crate::core::auth_storage::{AuthStatus, AuthStorage, GetApiKeyOptions, StorageError, stringify_auth_rejection};
use crate::core::config_value::{
    get_config_value_env_var_names, is_command_config_value, is_config_value_configured, resolve_config_value_or_throw,
    resolve_config_value_uncached,
};

pub use crate::core::config_value::clear_config_value_cache as clear_api_key_cache;

pub type StreamSimpleFn = Arc<
    dyn Fn(Model, Context, Option<SimpleStreamOptions>) -> Result<AssistantMessageEventStream, CallbackError>
        + Send
        + Sync,
>;
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
pub type ModifyModelsFn = Arc<dyn Fn(Vec<Model>, &OAuthCredentials) -> Result<Vec<Model>, CallbackError> + Send + Sync>;

pub type ExtensionOAuthApiKeyFn = Arc<dyn Fn(&OAuthCredentials) -> Result<JsString, CallbackError> + Send + Sync>;

#[derive(Clone)]
pub struct ExtensionOAuthConfig {
    pub name: String,
    pub uses_callback_server: Option<bool>,
    pub login: ExtensionOAuthLoginFn,
    pub refresh_token: ExtensionOAuthRefreshFn,
    pub get_api_key: ExtensionOAuthApiKeyFn,
    pub modify_models: Option<ModifyModelsFn>,
}

/// Frozen ProviderConfigInput.models inline object. Absence and an explicit
/// empty endpoint remain distinct; fields outside this object are not carried.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelInput {
    pub id: String,
    pub name: JsString,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<JsString>,
    pub reasoning: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level_map: Option<ThinkingLevelMap>,
    pub input: Vec<Modality>,
    pub cost: ModelCost,
    pub context_window: f64,
    pub max_tokens: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<IndexMap<String, JsString>>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "js_json::deserialize_present"
    )]
    pub compat: Option<JsValue>,
}

impl From<Model> for ProviderModelInput {
    fn from(model: Model) -> Self {
        // Frozen936-950 copies only these fields, not the input Model's root
        // properties, samplingParams or autoCompactionThreshold. No JSON copy.
        Self {
            id: model.id,
            name: model.name,
            api: Some(model.api),
            base_url: Some(model.base_url),
            reasoning: model.reasoning,
            thinking_level_map: model.thinking_level_map,
            input: model.input,
            cost: model.cost,
            context_window: model.context_window,
            max_tokens: model.max_tokens,
            headers: model.headers,
            compat: model.compat.map(|compat| compat.to_raw_value()),
        }
    }
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
    pub models: Option<Vec<ProviderModelInput>>,
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
    registered: RegisteredProviders,
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
    pub fn create(auth_storage: AuthStorage, models_path: Option<JsString>) -> Result<Self, CallbackError> {
        let path = match models_path {
            Some(path) => path,
            None => crate::utils::paths::node_join(crate::config::get_agent_dir()?, "models.json"),
        };
        let path = if path.is_empty() {
            None
        } else {
            Some(crate::utils::paths::normalize_path(path, &Default::default())?)
        };
        Self::with_path(auth_storage, path)
    }
    pub fn in_memory(auth_storage: AuthStorage) -> Result<Self, CallbackError> {
        Self::with_path(auth_storage, None)
    }
    fn with_path(auth_storage: AuthStorage, models_path: Option<JsString>) -> Result<Self, CallbackError> {
        let registry = Self {
            auth_storage,
            models_path,
            state: Arc::new(Mutex::new(RegistryState::default())),
        };
        registry.load_models()?;
        Ok(registry)
    }
    pub fn auth_storage(&self) -> &AuthStorage {
        &self.auth_storage
    }
    pub fn refresh(&self) -> Result<(), CallbackError> {
        self.auth_storage.reload();
        {
            let mut state = lock(&self.state);
            state.provider_requests.clear();
            state.model_headers.clear();
            state.error = None;
        }
        compat::reset_api_providers();
        pi_ai::oauth::reset_oauth_providers();
        self.load_models()?;
        // Frozen393 walks live Map entries; modifiers can insert/delete slots.
        let mut cursor = None;
        loop {
            let next = lock(&self.state).registered.next_after(cursor);
            let Some((slot, provider, config)) = next else {
                break;
            };
            cursor = Some(slot);
            self.apply_provider_config(&provider, &config)?;
        }
        Ok(())
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
        let Some(key) = key.filter(|key| !key.is_empty()) else {
            return Ok(None);
        };
        let env = self.provider_env(provider)?;
        Ok(resolve_config_value_uncached(&key, env.as_ref()))
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
        let config = lock(&self.state)
            .provider_requests
            .get(&model.provider)
            .cloned()
            .unwrap_or_default();
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
        // Frozen712 awaits the key before732 reads model headers. A key
        // callback can replace the model headers without replacing this config.
        let model_headers = lock(&self.state)
            .model_headers
            .get(&format!("{}:{}", model.provider, model.id))
            .cloned();
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
    fn load_models(&self) -> Result<(), CallbackError> {
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
                    store_model_headers(&mut state, &provider, id, value.get("headers"));
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
                if provider == "openrouter"
                    && let Err(error) = validate_cached_compat(&value)
                {
                    state.error = Some(js_format!(
                        "Invalid OpenRouter catalog model \"{}\": {}",
                        model_id,
                        error
                    ));
                    continue;
                }
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
                if let Err(error) = validate_model_header_names(&provider, &model_id, value.get("headers")) {
                    state.error = Some(error.into());
                    continue;
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
        drop(state);
        for oauth in self.auth_storage.get_oauth_providers() {
            if let Some(credential) = oauth_credential(&self.auth_storage, oauth.id()) {
                models = oauth.modify_models(models, &credential)?;
            }
        }
        lock(&self.state).models = models;
        Ok(())
    }
    fn load_custom_config(&self) -> Result<JsObject, JsString> {
        let Some(path) = self.models_path.as_ref() else {
            return Ok(JsObject::new());
        };
        let os_path = js_json::encode_node_utf8(path);
        if !std::path::Path::new(&os_path).exists() {
            return Ok(JsObject::new());
        }
        let bytes = std::fs::read(os_path)
            .map_err(|error| self.load_error(pi_js_compat::node_fs::node_read_error(&error, path)))?;
        // Node's UTF-8 file ingress repairs malformed bytes before JSON.parse.
        let content = String::from_utf8_lossy(&bytes);
        let value = js_json::parse_lossless(crate::utils::json::strip_json_comments(&content))
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
    pub fn register_provider(&self, provider: &str, config: ProviderConfigInput) -> Result<(), CallbackError> {
        validate_provider_config(provider, &config)?;
        self.apply_provider_config(provider, &config)?;
        lock(&self.state).registered.upsert(provider, config);
        Ok(())
    }
    pub fn unregister_provider(&self, provider: &str) -> Result<(), CallbackError> {
        if lock(&self.state).registered.remove(provider) {
            self.refresh()?;
        }
        Ok(())
    }
    fn apply_provider_config(&self, provider: &str, config: &ProviderConfigInput) -> Result<(), CallbackError> {
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
                state.models.push(create_registered_model(provider, model, config));
            }
            let Some(modify) = config.oauth.as_ref().and_then(|oauth| oauth.modify_models.as_ref()) else {
                return Ok(());
            };
            let Some(credential) = oauth_credential(&self.auth_storage, provider) else {
                return Ok(());
            };
            // Frozen957 keeps the newly installed models visible during reentry.
            // Owned callback arguments follow the approved value-copy boundary.
            let models = state.models.clone();
            drop(state);
            let models = modify(models, &credential)?;
            lock(&self.state).models = models;
        } else if config.base_url.as_ref().is_some_and(|url| !url.is_empty()) || config.headers.is_some() {
            // Frozen960 gates on truthiness; an empty URL only applies when
            // a present headers object enters this branch, then966 uses ??.
            for model in &mut state.models {
                if model.provider == provider
                    && let Some(url) = &config.base_url
                {
                    model.base_url = url.clone();
                }
            }
        }
        Ok(())
    }
}

fn create_registered_model(provider: &str, input: &ProviderModelInput, config: &ProviderConfigInput) -> Model {
    // Frozen933 uses || for api,941 uses ?? for endpoint,936-950 builds a new
    // Model rather than cloning the registration input's additional fields.
    let api = input
        .api
        .as_deref()
        .filter(|api| !api.is_empty())
        .or(config.api.as_deref())
        .expect("validated api");
    Model {
        id: input.id.clone(),
        name: input.name.clone(),
        api: api.into(),
        provider: provider.into(),
        base_url: input
            .base_url
            .clone()
            .unwrap_or_else(|| config.base_url.clone().expect("validated baseUrl")),
        reasoning: input.reasoning,
        thinking_level_map: input.thinking_level_map.clone(),
        input: input.input.clone(),
        cost: input.cost.clone(),
        context_window: input.context_window,
        max_tokens: input.max_tokens,
        auto_compaction_threshold: None,
        headers: None,
        compat: input
            .compat
            .as_ref()
            .map(|value| ModelCompat::parse(api, value.clone()).expect("validated compatibility object")),
        properties: Default::default(),
    }
}

fn validate_cached_compat(model: &JsValue) -> Result<(), String> {
    let Some(compat) = model.get("compat") else {
        return Ok(());
    };
    let Some(api) = model.get("api").and_then(JsValue::as_string) else {
        return Ok(());
    };
    let api = api
        .to_utf8()
        .map_err(|_| "model.api contains an unpaired UTF-16 surrogate".to_owned())?;
    pi_ai::types::ModelCompat::validate_catalog(&api, compat).map_err(|error| error.to_string())
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
fn merge_compat_properties(base: Option<&JsValue>, overrides: Option<&JsValue>) -> JsObject {
    let mut merged = JsObject::new();
    for value in base.into_iter().chain(overrides) {
        let properties = spread_compat_properties(value);
        for (key, value) in js_json::object_entries_lossless(&properties) {
            merged.insert(key.clone(), value.clone());
        }
    }
    merged
}

fn merge_compat(base: Option<&JsValue>, overrides: Option<&JsValue>) -> Option<JsValue> {
    // Frozen282 preserves the base when there is no truthy override. With an
    // override, :286/:292-310 spread both values, even if one is absent.
    if !overrides.is_some_and(js_json::is_truthy) {
        return base.cloned();
    }
    let mut merged = merge_compat_properties(base, overrides);
    for key in ["openRouterRouting", "vercelGatewayRouting", "chatTemplateKwargs"] {
        let base = base.and_then(|base| base.get(key));
        let overrides = overrides.and_then(|overrides| overrides.get(key));
        if base.is_some_and(js_json::is_truthy) || overrides.is_some_and(js_json::is_truthy) {
            merged.insert(key.into(), JsValue::Object(merge_compat_properties(base, overrides)));
        }
    }
    Some(JsValue::Object(merged))
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
fn validate_model_header_names(provider: &str, id: &JsString, headers: Option<&JsValue>) -> Result<(), String> {
    let id = js_json::stringify_lossless(&JsValue::String(id.clone()));
    validate_opaque_header_names(headers, &format!("provider {provider}, model {id}"))
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
        validate_opaque_header_names(config.get("headers"), &format!("provider {provider_id}"))?;
        if let Some(overrides) = config.get("modelOverrides").and_then(JsValue::as_object) {
            for (id, model) in js_json::object_entries_lossless(overrides) {
                validate_model_header_names(&provider_id, id, model.get("headers"))?;
            }
        }
        for model in models {
            let id = model.get("id").and_then(JsValue::as_string).expect("validated id");
            if id.to_utf8().is_err() {
                return Err("model ID contains an unpaired UTF-16 unit".into());
            }
            validate_model_header_names(&provider_id, id, model.get("headers"))?;
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
fn validate_provider_config(provider: &str, config: &ProviderConfigInput) -> Result<(), CallbackError> {
    if config.stream_simple.is_some() && config.api.as_ref().is_none_or(|api| api.is_empty()) {
        return Err(CallbackError::new(js_format!(
            "Provider {}: \"api\" is required when registering streamSimple.",
            provider
        )));
    }
    let Some(models) = config.models.as_ref().filter(|models| !models.is_empty()) else {
        return Ok(());
    };
    if config.base_url.as_ref().is_none_or(JsString::is_empty) {
        return Err(CallbackError::new(js_format!(
            "Provider {}: \"baseUrl\" is required when defining models.",
            provider
        )));
    }
    if config.api_key.as_ref().is_none_or(JsString::is_empty) && config.oauth.is_none() {
        return Err(CallbackError::new(js_format!(
            "Provider {}: \"apiKey\" or \"oauth\" is required when defining models.",
            provider
        )));
    }
    for model in models {
        if model.api.as_ref().is_none_or(|api| api.is_empty()) && config.api.as_ref().is_none_or(|api| api.is_empty()) {
            return Err(CallbackError::new(js_format!(
                "Provider {}, model {}: no \"api\" specified.",
                provider,
                model.id
            )));
        }
        if model.compat.as_ref().is_some_and(|value| !value.is_object()) {
            return Err(CallbackError::new(js_format!(
                "Provider {}, model {}: compat must be an object.",
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
    fn modify_models(&self, models: Vec<Model>, credentials: &OAuthCredentials) -> Result<Vec<Model>, CallbackError> {
        match &self.config.modify_models {
            Some(modify) => modify(models, credentials),
            None => Ok(models),
        }
    }
}
