//! Port of `packages/coding-agent/src/core/provider-composer.ts`.
//!
//! Composes a provider from three layers: the built-in provider, `models.json`
//! overlays, and an extension registration. Composition never reads credentials;
//! it only resolves configured values and wraps auth methods.

use pi_js_compat::{
    js_format,
    js_json::{self, JsString, encode_node_utf8},
};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use pi_ai::api::lazy::lazy_stream;
use pi_ai::auth::types::{
    ApiKeyAuth, ApiKeyCheckInput, ApiKeyCredential, ApiKeyResolveInput, AuthCheck, AuthContext, AuthError, AuthEvent,
    AuthInteraction, AuthPrompt, AuthPromptKind, AuthResult, AuthSelectOption, AuthType, Credential, ModelAuth,
    OAuthAuth, OAuthCredential, ProviderAuth,
};
use pi_ai::compat::extension_oauth_types::{
    OAuthAuthInfo, OAuthDeviceCodeInfo, OAuthLoginCallbacks, OAuthPrompt, OAuthSelectPrompt,
};
use pi_ai::models::{Provider, RefreshModelsContext};
use pi_ai::request::{
    BoxFuture, CallbackError, DeferredCancelOptions, DeferredFetchOptions, ProviderStreams, SimpleStreamOptions,
    StreamOptions,
};
use pi_ai::types::{
    Context, DeferredHandle, JsonObject, Modality, Model, ModelCost, ModelCostRates, ModelCostTier, ModelThinkingLevel,
    ProviderEnv, ProviderHeaders, ThinkingLevelMap,
};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use tokio_util::sync::CancellationToken;

use crate::core::config_value::{
    clear_config_value_cache, get_config_value_env_var_names, is_command_config_value, is_config_value_configured,
    resolve_config_value_or_throw, resolve_headers_or_throw,
};
use crate::core::model_config::ModelConfig;

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Composition failure carrying the TypeScript error message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCompositionError(JsString);

impl ProviderCompositionError {
    pub fn new(message: impl Into<JsString>) -> Self {
        Self(message.into())
    }

    pub fn message(&self) -> &JsString {
        &self.0
    }
}

impl std::fmt::Display for ProviderCompositionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&encode_node_utf8(&self.0))
    }
}

impl std::error::Error for ProviderCompositionError {}

impl From<crate::core::ConfigValueError> for ProviderCompositionError {
    fn from(error: crate::core::ConfigValueError) -> Self {
        Self::new(error.message().clone())
    }
}

/// `models.json` model definition.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsJsonModel {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub api: Option<String>,
    #[serde(default)]
    pub base_url: Option<JsString>,
    #[serde(default)]
    pub reasoning: Option<bool>,
    #[serde(default)]
    pub thinking_level_map: Option<ThinkingLevelMap>,
    #[serde(default)]
    pub input: Option<Vec<Modality>>,
    #[serde(default)]
    pub cost: Option<ModelCost>,
    #[serde(default)]
    pub context_window: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<f64>,
    #[serde(default)]
    pub sampling_params: Option<JsonObject>,
    #[serde(default)]
    pub headers: Option<HashMap<String, JsString>>,
    #[serde(default)]
    pub compat: Option<Value>,
}

/// `models.json` per-model cost override.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsJsonCostOverride {
    #[serde(default)]
    pub input: Option<f64>,
    #[serde(default)]
    pub output: Option<f64>,
    #[serde(default)]
    pub cache_read: Option<f64>,
    #[serde(default)]
    pub cache_write: Option<f64>,
    #[serde(default)]
    pub tiers: Option<Vec<ModelCostTier>>,
}

/// `models.json` model override.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsJsonModelOverride {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub reasoning: Option<bool>,
    #[serde(default)]
    pub thinking_level_map: Option<ThinkingLevelMap>,
    #[serde(default)]
    pub input: Option<Vec<Modality>>,
    #[serde(default)]
    pub cost: Option<ModelsJsonCostOverride>,
    #[serde(default)]
    pub context_window: Option<f64>,
    #[serde(default)]
    pub max_tokens: Option<f64>,
    #[serde(default)]
    pub sampling_params: Option<JsonObject>,
    #[serde(default)]
    pub headers: Option<HashMap<String, JsString>>,
    #[serde(default)]
    pub compat: Option<Value>,
}

/// `models.json` provider configuration.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelsJsonProvider {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub base_url: Option<JsString>,
    #[serde(default)]
    pub api_key: Option<JsString>,
    #[serde(default)]
    pub api: Option<String>,
    #[serde(default)]
    pub oauth: Option<String>,
    #[serde(default)]
    pub headers: Option<HashMap<String, JsString>>,
    #[serde(default)]
    pub compat: Option<Value>,
    #[serde(default)]
    pub auth_header: Option<bool>,
    #[serde(default)]
    pub models: Option<Vec<ModelsJsonModel>>,
    #[serde(default)]
    pub model_overrides: Option<HashMap<String, ModelsJsonModelOverride>>,
}

fn parse_provider_config(
    config: &ModelConfig,
    provider_id: &str,
) -> Result<Option<ModelsJsonProvider>, ProviderCompositionError> {
    let Some(value) = config.get_provider(provider_id) else {
        return Ok(None);
    };
    js_json::from_value(value)
        .map(Some)
        .map_err(|error| ProviderCompositionError::new(error.to_string()))
}

/// Extension OAuth configuration (`ExtensionOAuthConfig`).
#[derive(Clone)]
pub struct ExtensionOAuthConfig {
    pub name: String,
    pub is_subscription: Option<bool>,
    pub uses_callback_server: Option<bool>,
    pub login: ExtensionOAuthLoginFn,
    pub refresh_token: ExtensionOAuthRefreshFn,
    pub get_api_key: Arc<dyn Fn(&OAuthCredential) -> JsString + Send + Sync>,
    pub modify_models: Option<ModifyModelsFn>,
}

/// `streamSimple` override accepted by `registerProvider`.
pub type StreamSimpleFn =
    Arc<dyn Fn(Model, Context, Option<SimpleStreamOptions>) -> AssistantMessageEventStream + Send + Sync>;

/// `refreshModels` override accepted by `registerProvider`.
pub type RefreshModelsFn =
    Arc<dyn Fn(RefreshModelsContext) -> BoxFuture<'static, Result<Vec<Model>, CallbackError>> + Send + Sync>;

pub type ExtensionOAuthLoginFn =
    Arc<dyn Fn(Arc<dyn OAuthLoginCallbacks>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> + Send + Sync>;
pub type ExtensionOAuthRefreshFn = Arc<
    dyn Fn(OAuthCredential, CancellationToken) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> + Send + Sync,
>;
pub type ModifyModelsFn = Arc<dyn Fn(Vec<Model>, &OAuthCredential) -> Vec<Model> + Send + Sync>;
type ComposedRefreshFn =
    Arc<dyn Fn(RefreshModelsContext) -> BoxFuture<'static, Result<(), CallbackError>> + Send + Sync>;
type FilterModelsFn = Arc<dyn Fn(Vec<Model>, Option<&Credential>) -> Vec<Model> + Send + Sync>;
type FetchDeferredFn =
    Arc<dyn Fn(Model, DeferredHandle, DeferredFetchOptions) -> AssistantMessageEventStream + Send + Sync>;
type CancelDeferredFn = Arc<
    dyn Fn(Model, DeferredHandle, DeferredCancelOptions) -> BoxFuture<'static, Result<(), CallbackError>> + Send + Sync,
>;

/// Input type for the extension `registerProvider` API.
#[derive(Clone, Default)]
pub struct ProviderConfigInput {
    pub name: Option<String>,
    pub base_url: Option<JsString>,
    pub api_key: Option<JsString>,
    pub api: Option<String>,
    pub stream_simple: Option<StreamSimpleFn>,
    pub headers: Option<HashMap<String, JsString>>,
    pub auth_header: Option<bool>,
    pub oauth: Option<ExtensionOAuthConfig>,
    /// Rust models carry their full shape; composition overrides provider/baseUrl.
    pub models: Option<Vec<Model>>,
    pub refresh_models: Option<RefreshModelsFn>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthStatusSource {
    Stored,
    Runtime,
    Environment,
    Fallback,
    ModelsJsonKey,
    ModelsJsonCommand,
}

impl AuthStatusSource {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stored => "stored",
            Self::Runtime => "runtime",
            Self::Environment => "environment",
            Self::Fallback => "fallback",
            Self::ModelsJsonKey => "models_json_key",
            Self::ModelsJsonCommand => "models_json_command",
        }
    }
}

/// Provider auth status shown by auth pickers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub configured: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<AuthStatusSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<JsString>,
}

/// Re-exported cache reset (`clearApiKeyCache`).
pub fn clear_api_key_cache() {
    clear_config_value_cache();
}

fn provider_headers_from_map(headers: impl IntoIterator<Item = (String, JsString)>) -> ProviderHeaders {
    headers.into_iter().map(|(name, value)| (name, Some(value))).collect()
}

fn merge_compat(api: &str, base: Option<&Value>, override_compat: Option<&Value>) -> Option<pi_ai::types::ModelCompat> {
    let merged = match (base, override_compat) {
        (None, None) => return None,
        (Some(base), None) => base.clone(),
        (None, Some(override_compat)) => override_compat.clone(),
        (Some(base), Some(override_compat)) => {
            let mut merged = match base {
                Value::Object(object) => object.clone(),
                _ => Map::new(),
            };
            if let Value::Object(override_object) = override_compat {
                for (key, value) in override_object {
                    merged.insert(key.clone(), value.clone());
                }
                for key in [
                    "openRouterRouting",
                    "vercelGatewayRouting",
                    "chatTemplateKwargs",
                    "chatTemplateArgs",
                ] {
                    let base_value = base.get(key);
                    let override_value = override_object.get(key);
                    if base_value.is_some_and(Value::is_object) || override_value.is_some_and(Value::is_object) {
                        let mut nested = match base_value {
                            Some(Value::Object(object)) => object.clone(),
                            _ => Map::new(),
                        };
                        if let Some(Value::Object(object)) = override_value {
                            for (nested_key, nested_value) in object {
                                nested.insert(nested_key.clone(), nested_value.clone());
                            }
                        }
                        merged.insert(key.to_owned(), Value::Object(nested));
                    }
                }
            }
            Value::Object(merged)
        }
    };
    pi_ai::types::ModelCompat::parse(api, merged).ok().flatten()
}

fn apply_model_override(model: &Model, model_override: &ModelsJsonModelOverride) -> Model {
    let mut updated = model.clone();
    if let Some(name) = &model_override.name {
        updated.name = name.clone();
    }
    if let Some(reasoning) = model_override.reasoning {
        updated.reasoning = reasoning;
    }
    if let Some(thinking_level_map) = &model_override.thinking_level_map {
        let mut merged = updated.thinking_level_map.clone().unwrap_or_default();
        for (level, value) in thinking_level_map {
            merged.insert(*level, value.clone());
        }
        updated.thinking_level_map = Some(merged);
    }
    if let Some(input) = &model_override.input {
        updated.input = input.clone();
    }
    if let Some(cost) = &model_override.cost {
        updated.cost = ModelCost {
            rates: ModelCostRates {
                input: cost.input.unwrap_or(model.cost.rates.input),
                output: cost.output.unwrap_or(model.cost.rates.output),
                cache_read: cost.cache_read.unwrap_or(model.cost.rates.cache_read),
                cache_write: cost.cache_write.unwrap_or(model.cost.rates.cache_write),
            },
            tiers: cost.tiers.clone().or_else(|| model.cost.tiers.clone()),
        };
    }
    if let Some(context_window) = model_override.context_window {
        updated.context_window = context_window;
    }
    if let Some(max_tokens) = model_override.max_tokens {
        updated.max_tokens = max_tokens;
    }
    if let Some(sampling_params) = &model_override.sampling_params {
        let mut merged = updated.sampling_params.clone().unwrap_or_default();
        for (key, value) in sampling_params {
            merged.insert(key.clone(), value.clone());
        }
        updated.sampling_params = Some(merged);
    }
    let base_compat = updated
        .compat
        .as_ref()
        .and_then(|compat| serde_json::to_value(compat).ok());
    updated.compat = merge_compat(&updated.api, base_compat.as_ref(), model_override.compat.as_ref());
    updated
}

fn model_from_json(
    provider_id: &str,
    definition: &ModelsJsonModel,
    provider_config: &ModelsJsonProvider,
    defaults: Option<&Model>,
) -> Result<Model, ProviderCompositionError> {
    let Some(api) = definition
        .api
        .clone()
        .or_else(|| provider_config.api.clone())
        .or_else(|| defaults.map(|model| model.api.clone()))
    else {
        return Err(ProviderCompositionError::new(format!(
            "Provider {provider_id}, model {}: no \"api\" specified. Set at provider or model level.",
            definition.id
        )));
    };
    let Some(base_url) = definition
        .base_url
        .clone()
        .or_else(|| provider_config.base_url.clone())
        .or_else(|| defaults.map(|model| model.base_url.clone()))
    else {
        return Err(ProviderCompositionError::new(format!(
            "Provider {provider_id}: \"baseUrl\" is required when defining custom models."
        )));
    };
    if definition.context_window.is_some_and(|value| value <= 0.0) {
        return Err(ProviderCompositionError::new(format!(
            "Provider {provider_id}, model {}: invalid contextWindow",
            definition.id
        )));
    }
    if definition.max_tokens.is_some_and(|value| value <= 0.0) {
        return Err(ProviderCompositionError::new(format!(
            "Provider {provider_id}, model {}: invalid maxTokens",
            definition.id
        )));
    }
    Ok(Model {
        id: definition.id.clone(),
        name: definition.name.clone().unwrap_or_else(|| definition.id.clone()),
        api,
        provider: provider_id.to_owned(),
        base_url,
        reasoning: definition.reasoning.unwrap_or(false),
        thinking_level_map: definition.thinking_level_map.clone(),
        input: definition.input.clone().unwrap_or_else(|| vec![Modality::Text]),
        cost: definition.cost.clone().unwrap_or_default(),
        context_window: definition.context_window.unwrap_or(128_000.0),
        auto_compaction_threshold: None,
        max_tokens: definition.max_tokens.unwrap_or(16_384.0),
        sampling_params: definition.sampling_params.clone(),
        headers: None,
        compat: merge_compat(
            &definition
                .api
                .clone()
                .or_else(|| provider_config.api.clone())
                .or_else(|| defaults.map(|model| model.api.clone()))
                .unwrap_or_default(),
            provider_config.compat.as_ref(),
            definition.compat.as_ref(),
        ),
    })
}

fn apply_models_json(
    provider_id: &str,
    base_models: &[Model],
    config: Option<&ModelsJsonProvider>,
) -> Result<Vec<Model>, ProviderCompositionError> {
    let Some(config) = config else {
        return Ok(base_models.to_vec());
    };
    if config.oauth.is_some() && config.base_url.is_none() {
        return Err(ProviderCompositionError::new(format!(
            "Provider {provider_id}: \"baseUrl\" is required when \"oauth\" is set."
        )));
    }
    let has_overrides = config
        .model_overrides
        .as_ref()
        .is_some_and(|overrides| !overrides.is_empty());
    if config.models.as_ref().is_none_or(Vec::is_empty)
        && config.base_url.is_none()
        && config.headers.is_none()
        && config.compat.is_none()
        && !has_overrides
        && config.api_key.is_none()
        && config.oauth.is_none()
        && config.auth_header.is_none()
    {
        return Err(ProviderCompositionError::new(format!(
            "Provider {provider_id}: must specify \"baseUrl\", \"headers\", \"compat\", \"modelOverrides\", or \"models\"."
        )));
    }

    let mut models: Vec<Model> = base_models
        .iter()
        .map(|model| {
            let base_compat = model
                .compat
                .as_ref()
                .and_then(|compat| serde_json::to_value(compat).ok());
            let mut updated = model.clone();
            if config.oauth.as_deref() != Some("radius")
                && let Some(base_url) = &config.base_url
            {
                updated.base_url = base_url.clone();
            }
            updated.compat = merge_compat(&updated.api, base_compat.as_ref(), config.compat.as_ref());
            updated
        })
        .collect();

    for definition in config.models.as_deref().unwrap_or_default() {
        let existing_index = models.iter().position(|model| model.id == definition.id);
        let defaults = match existing_index {
            Some(index) => Some(models[index].clone()),
            None => models.first().cloned(),
        };
        let model = model_from_json(provider_id, definition, config, defaults.as_ref())?;
        match existing_index {
            Some(index) => models[index] = model,
            None => models.push(model),
        }
    }
    Ok(models)
}

fn apply_extension(
    provider_id: &str,
    models: &[Model],
    config: Option<&ProviderConfigInput>,
) -> Result<Vec<Model>, ProviderCompositionError> {
    let Some(config) = config else {
        return Ok(models.to_vec());
    };
    let Some(definitions) = &config.models else {
        return Ok(match &config.base_url {
            Some(base_url) => models
                .iter()
                .map(|model| {
                    let mut updated = model.clone();
                    updated.base_url = base_url.clone();
                    updated
                })
                .collect(),
            None => models.to_vec(),
        });
    };
    definitions
        .iter()
        .map(|definition| {
            let defaults = models
                .iter()
                .find(|model| model.id == definition.id)
                .or_else(|| models.first());
            let api = if definition.api.is_empty() {
                config
                    .api
                    .clone()
                    .or_else(|| defaults.map(|model| model.api.clone()))
                    .ok_or_else(|| {
                        ProviderCompositionError::new(format!(
                            "Provider {provider_id}, model {}: no \"api\" specified. Set at provider or model level.",
                            definition.id
                        ))
                    })?
            } else {
                definition.api.clone()
            };
            let base_url = if definition.base_url.is_empty() {
                config
                    .base_url
                    .clone()
                    .or_else(|| defaults.map(|model| model.base_url.clone()))
                    .ok_or_else(|| {
                        ProviderCompositionError::new(format!(
                            "Provider {provider_id}: \"baseUrl\" is required when defining custom models."
                        ))
                    })?
            } else {
                definition.base_url.clone()
            };
            let mut model = definition.clone();
            model.api = api;
            model.provider = provider_id.to_owned();
            model.base_url = base_url;
            model.headers = None;
            Ok(model)
        })
        .collect()
}

fn configured_api_key(
    config: Option<&ModelsJsonProvider>,
    extension: Option<&ProviderConfigInput>,
) -> Option<JsString> {
    extension
        .and_then(|extension| extension.api_key.clone())
        .or_else(|| config.and_then(|config| config.api_key.clone()))
}

fn configured_headers(
    config: Option<&ModelsJsonProvider>,
    extension: Option<&ProviderConfigInput>,
) -> Option<HashMap<String, JsString>> {
    let config_headers = config.and_then(|config| config.headers.clone());
    let extension_headers = extension.and_then(|extension| extension.headers.clone());
    if config_headers.is_none() && extension_headers.is_none() {
        return None;
    }
    let mut merged = config_headers.unwrap_or_default();
    merged.extend(extension_headers.unwrap_or_default());
    Some(merged)
}

async fn config_context_env(
    values: &[JsString],
    ctx: &dyn AuthContext,
    explicit: Option<&ProviderEnv>,
) -> Option<HashMap<String, JsString>> {
    let mut env: HashMap<String, JsString> = explicit
        .map(|explicit| {
            explicit
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
        .unwrap_or_default();
    let mut names: Vec<String> = Vec::new();
    for value in values {
        for name in get_config_value_env_var_names(value) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    for name in names {
        if env.contains_key(&name) {
            continue;
        }
        if let Some(value) = ctx.env(&name).await {
            env.insert(name, value);
        }
    }
    (!env.is_empty()).then_some(env)
}

fn with_configured_auth(
    auth: ModelAuth,
    headers: Option<HashMap<String, JsString>>,
    auth_header: bool,
) -> Result<ModelAuth, AuthError> {
    let mut merged: Option<ProviderHeaders> = if auth.headers.is_some() || headers.is_some() {
        let mut merged = auth.headers.clone().unwrap_or_default();
        for (name, value) in headers.unwrap_or_default() {
            merged.insert(name, Some(value));
        }
        Some(merged)
    } else {
        None
    };
    if auth_header {
        let Some(api_key) = &auth.api_key else {
            return Err(AuthError::failed("authHeader requires a resolved API key"));
        };
        merged
            .get_or_insert_with(ProviderHeaders::default)
            .insert("Authorization".to_owned(), Some(js_format!("Bearer {}", api_key)));
    }
    Ok(ModelAuth {
        headers: merged,
        ..auth
    })
}

fn raw_model_headers(
    model: &Model,
    config: Option<&ModelsJsonProvider>,
    extension: Option<&ProviderConfigInput>,
) -> Option<HashMap<String, JsString>> {
    let mut headers = HashMap::new();
    if let Some(config) = config {
        if let Some(overrides) = &config.model_overrides
            && let Some(model_override) = overrides.get(&model.id)
            && let Some(override_headers) = &model_override.headers
        {
            headers.extend(override_headers.clone());
        }
        if let Some(definition) = config
            .models
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|entry| entry.id == model.id)
            && let Some(definition_headers) = &definition.headers
        {
            headers.extend(definition_headers.clone());
        }
    }
    if let Some(extension) = extension
        && let Some(definition) = extension
            .models
            .as_deref()
            .unwrap_or_default()
            .iter()
            .find(|entry| entry.id == model.id)
        && let Some(definition_headers) = &definition.headers
    {
        headers.extend(definition_headers.clone());
    }
    (!headers.is_empty()).then_some(headers)
}

/// Validates an extension registration without touching stored config.
pub fn validate_extension_provider(
    provider_id: &str,
    base: Option<&dyn Provider>,
    models_config: Option<&ModelsJsonProvider>,
    extension: &ProviderConfigInput,
) -> Result<(), ProviderCompositionError> {
    if extension.stream_simple.is_some() && extension.api.is_none() {
        return Err(ProviderCompositionError::new(format!(
            "Provider {provider_id}: \"api\" is required when registering streamSimple."
        )));
    }
    let base_models = base.map(Provider::get_models).unwrap_or_default();
    apply_extension(
        provider_id,
        &apply_models_json(provider_id, &base_models, models_config)?,
        Some(extension),
    )?;
    Ok(())
}

struct ComposedApiKeyAuth {
    provider_id: String,
    inherited: Option<Arc<dyn ApiKeyAuth>>,
    raw_key: Option<JsString>,
    raw_headers: Option<HashMap<String, JsString>>,
    auth_header: bool,
}

impl ApiKeyAuth for ComposedApiKeyAuth {
    fn name(&self) -> &str {
        self.inherited
            .as_ref()
            .map(|inherited| inherited.name())
            .unwrap_or("API key")
    }

    fn login(
        &self,
        interaction: Arc<dyn AuthInteraction>,
    ) -> Option<BoxFuture<'static, Result<ApiKeyCredential, AuthError>>> {
        if let Some(inherited) = &self.inherited {
            return inherited.login(interaction);
        }
        Some(Box::pin(async move {
            let key = interaction
                .prompt(AuthPrompt {
                    signal: None,
                    kind: AuthPromptKind::Secret {
                        message: "Enter API key".into(),
                        placeholder: None,
                    },
                })
                .await?;
            Ok(ApiKeyCredential {
                key: Some(key),
                env: None,
            })
        }))
    }

    fn check(&self, input: ApiKeyCheckInput) -> Option<BoxFuture<'static, Result<Option<AuthCheck>, AuthError>>> {
        let inherited = self.inherited.clone();
        let raw_key = self.raw_key.clone();
        Some(Box::pin(async move {
            if let Some(credential) = &input.credential {
                if let Some(inherited) = &inherited
                    && let Some(check) = inherited.check(ApiKeyCheckInput {
                        ctx: Arc::clone(&input.ctx),
                        credential: Some(credential.clone()),
                        signal: input.signal.clone(),
                    })
                {
                    return check.await;
                }
                if credential.key.as_ref().is_some_and(|key| !key.is_empty()) {
                    return Ok(Some(AuthCheck {
                        source: Some("stored credential".into()),
                        auth_type: AuthType::ApiKey,
                    }));
                }
                if let Some(inherited) = &inherited {
                    let resolved = inherited
                        .resolve(ApiKeyResolveInput {
                            model: None, // Q15 provider-ID status check has no request model.
                            ctx: Arc::clone(&input.ctx),
                            credential: Some(credential.clone()),
                            signal: input.signal.clone(),
                        })
                        .await?;
                    return Ok(resolved.map(|resolved| AuthCheck {
                        source: resolved.source,
                        auth_type: AuthType::ApiKey,
                    }));
                }
                return Ok(None);
            }
            if let Some(raw_key) = &raw_key {
                if is_command_config_value(raw_key) {
                    return Ok(Some(AuthCheck {
                        source: Some("configured API key".into()),
                        auth_type: AuthType::ApiKey,
                    }));
                }
                for name in get_config_value_env_var_names(raw_key) {
                    if input.ctx.env(&name).await.is_none() {
                        return Ok(None);
                    }
                }
                return Ok(Some(AuthCheck {
                    source: Some("configured API key".into()),
                    auth_type: AuthType::ApiKey,
                }));
            }
            if let Some(inherited) = &inherited {
                if let Some(check) = inherited.check(ApiKeyCheckInput {
                    ctx: Arc::clone(&input.ctx),
                    credential: None,
                    signal: input.signal.clone(),
                }) {
                    return check.await;
                }
                let resolved = inherited
                    .resolve(ApiKeyResolveInput {
                        model: None, // Q15 provider-ID status check has no request model.
                        ctx: Arc::clone(&input.ctx),
                        credential: None,
                        signal: input.signal.clone(),
                    })
                    .await?;
                return Ok(resolved.map(|resolved| AuthCheck {
                    source: resolved.source,
                    auth_type: AuthType::ApiKey,
                }));
            }
            Ok(None)
        }))
    }

    fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        let provider_id = self.provider_id.clone();
        let inherited = self.inherited.clone();
        let raw_key = self.raw_key.clone();
        let raw_headers = self.raw_headers.clone();
        let auth_header = self.auth_header;
        let ApiKeyResolveInput {
            model,
            ctx,
            credential,
            signal,
        } = input;
        Box::pin(async move {
            let result = if let Some(credential) = &credential {
                match &inherited {
                    Some(inherited) => {
                        inherited
                            .resolve(ApiKeyResolveInput {
                                model: model.clone(),
                                ctx: Arc::clone(&ctx),
                                credential: Some(credential.clone()),
                                signal: signal.clone(),
                            })
                            .await?
                    }
                    None => credential
                        .key
                        .as_ref()
                        .filter(|key| !key.is_empty())
                        .map(|key| AuthResult {
                            auth: ModelAuth {
                                api_key: Some(key.to_owned()),
                                headers: None,
                                base_url: None,
                            },
                            env: credential.env.clone(),
                            source: Some("stored credential".into()),
                        }),
                }
            } else if let Some(raw_key) = &raw_key {
                let env = config_context_env(std::slice::from_ref(raw_key), ctx.as_ref(), None).await;
                let key = resolve_config_value_or_throw(
                    raw_key,
                    &js_format!("API key for provider \"{}\"", provider_id),
                    env.as_ref(),
                )
                .map_err(|error| AuthError::failed(error.message().clone()))?;
                match &inherited {
                    Some(inherited) => {
                        inherited
                            .resolve(ApiKeyResolveInput {
                                model: model.clone(),
                                ctx: Arc::clone(&ctx),
                                credential: Some(ApiKeyCredential {
                                    key: Some(key),
                                    env: None,
                                }),
                                signal: signal.clone(),
                            })
                            .await?
                    }
                    None => Some(AuthResult {
                        auth: ModelAuth {
                            api_key: Some(key),
                            headers: None,
                            base_url: None,
                        },
                        env: None,
                        source: Some("configured API key".into()),
                    }),
                }
            } else if let Some(inherited) = &inherited {
                inherited
                    .resolve(ApiKeyResolveInput {
                        model,
                        ctx: Arc::clone(&ctx),
                        credential: None,
                        signal: signal.clone(),
                    })
                    .await?
            } else {
                None
            };
            let Some(mut result) = result else {
                return Ok(None);
            };
            let mut explicit_env: Option<ProviderEnv> =
                credential.as_ref().and_then(|credential| credential.env.clone());
            if let Some(result_env) = &result.env {
                explicit_env
                    .get_or_insert_with(ProviderEnv::default)
                    .extend(result_env.clone());
            }
            let header_values: Vec<JsString> = raw_headers
                .as_ref()
                .map(|headers| headers.values().cloned().collect())
                .unwrap_or_default();
            let header_env = config_context_env(&header_values, ctx.as_ref(), explicit_env.as_ref()).await;
            let headers = resolve_headers_or_throw(
                raw_headers.as_ref(),
                &js_format!("provider \"{}\"", provider_id),
                header_env.as_ref(),
            )
            .map_err(|error| AuthError::failed(error.message().clone()))?;
            result.auth = with_configured_auth(result.auth, headers, auth_header)?;
            Ok(Some(result))
        })
    }
}

fn compose_api_key_auth(
    provider_id: &str,
    base: Option<&Arc<dyn Provider>>,
    config: Option<&ModelsJsonProvider>,
    extension: Option<&ProviderConfigInput>,
) -> Option<Arc<dyn ApiKeyAuth>> {
    let inherited = base.and_then(|base| base.auth().api_key.clone());
    let raw_key = configured_api_key(config, extension);
    let has_oauth = extension.and_then(|extension| extension.oauth.as_ref()).is_some()
        || base.is_some_and(|base| base.auth().oauth.is_some());
    if inherited.is_none() && raw_key.is_none() && has_oauth {
        return None;
    }
    Some(Arc::new(ComposedApiKeyAuth {
        provider_id: provider_id.to_owned(),
        inherited,
        raw_key,
        raw_headers: configured_headers(config, extension),
        auth_header: extension
            .and_then(|extension| extension.auth_header)
            .or_else(|| config.and_then(|config| config.auth_header))
            .unwrap_or(false),
    }))
}

struct ExtensionOAuth {
    config: ExtensionOAuthConfig,
}

impl OAuthAuth for ExtensionOAuth {
    fn name(&self) -> &str {
        &self.config.name
    }

    fn is_subscription(&self) -> bool {
        self.config.is_subscription.unwrap_or(false)
    }

    fn login(&self, interaction: Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        let callbacks = Arc::new(InteractionOAuthCallbacks { interaction });
        (self.config.login)(callbacks)
    }

    fn refresh(
        &self,
        credential: OAuthCredential,
        signal: CancellationToken,
    ) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        (self.config.refresh_token)(credential, signal)
    }

    fn to_auth(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<ModelAuth, AuthError>> {
        let api_key = (self.config.get_api_key)(&credential);
        Box::pin(async move {
            Ok(ModelAuth {
                api_key: Some(api_key),
                headers: None,
                base_url: None,
            })
        })
    }
}

struct InteractionOAuthCallbacks {
    interaction: Arc<dyn AuthInteraction>,
}

impl OAuthLoginCallbacks for InteractionOAuthCallbacks {
    fn on_auth(&self, info: OAuthAuthInfo) {
        self.interaction.notify(AuthEvent::AuthUrl {
            url: info.url,
            instructions: info.instructions,
        });
    }

    fn on_device_code(&self, info: OAuthDeviceCodeInfo) {
        self.interaction.notify(AuthEvent::DeviceCode {
            user_code: info.user_code,
            verification_uri: info.verification_uri,
            interval_seconds: info.interval_seconds,
            expires_in_seconds: info.expires_in_seconds,
        });
    }

    fn on_prompt(&self, prompt: OAuthPrompt) -> BoxFuture<'_, Result<JsString, CallbackError>> {
        Box::pin(async move {
            self.interaction
                .prompt(AuthPrompt {
                    signal: None,
                    kind: AuthPromptKind::Text {
                        message: prompt.message,
                        placeholder: prompt.placeholder,
                    },
                })
                .await
        })
    }

    fn on_progress(&self, message: &JsString) {
        self.interaction.notify(AuthEvent::Progress {
            message: message.to_owned(),
        });
    }

    fn on_manual_code_input(&self) -> Option<BoxFuture<'_, Result<JsString, CallbackError>>> {
        Some(Box::pin(async move {
            self.interaction
                .prompt(AuthPrompt {
                    signal: None,
                    kind: AuthPromptKind::ManualCode {
                        message: "Paste the authorization code".into(),
                        placeholder: None,
                    },
                })
                .await
        }))
    }

    fn on_select(&self, prompt: OAuthSelectPrompt) -> BoxFuture<'_, Result<Option<JsString>, CallbackError>> {
        Box::pin(async move {
            self.interaction
                .prompt(AuthPrompt {
                    signal: None,
                    kind: AuthPromptKind::Select {
                        message: prompt.message,
                        options: prompt
                            .options
                            .into_iter()
                            .map(|option| AuthSelectOption {
                                id: option.id,
                                label: option.label,
                                description: None,
                            })
                            .collect(),
                    },
                })
                .await
                .map(Some)
        })
    }

    fn signal(&self) -> Option<CancellationToken> {
        Some(self.interaction.signal())
    }
}

struct ComposedOAuthAuth {
    oauth: Arc<dyn OAuthAuth>,
    provider_id: String,
    raw_headers: Option<HashMap<String, JsString>>,
    auth_header: bool,
}

impl OAuthAuth for ComposedOAuthAuth {
    fn name(&self) -> &str {
        self.oauth.name()
    }

    fn is_subscription(&self) -> bool {
        self.oauth.is_subscription()
    }

    fn login_label(&self) -> Option<&str> {
        self.oauth.login_label()
    }

    fn login(&self, interaction: Arc<dyn AuthInteraction>) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        self.oauth.login(interaction)
    }

    fn refresh(
        &self,
        credential: OAuthCredential,
        signal: CancellationToken,
    ) -> BoxFuture<'static, Result<OAuthCredential, AuthError>> {
        self.oauth.refresh(credential, signal)
    }

    fn to_auth(&self, credential: OAuthCredential) -> BoxFuture<'static, Result<ModelAuth, AuthError>> {
        let oauth = Arc::clone(&self.oauth);
        let provider_id = self.provider_id.clone();
        let raw_headers = self.raw_headers.clone();
        let auth_header = self.auth_header;
        Box::pin(async move {
            let auth = oauth.to_auth(credential.clone()).await?;
            let env = credential
                .extra
                .get(&pi_js_compat::js_json::JsString::from("env"))
                .map(js_json::from_value::<HashMap<String, JsString>>)
                .transpose()
                .map_err(|error| AuthError::failed(error.to_string()))?;
            let headers = resolve_headers_or_throw(
                raw_headers.as_ref(),
                &js_format!("provider \"{}\"", provider_id),
                env.as_ref(),
            )
            .map_err(|error| AuthError::failed(error.message().clone()))?;
            with_configured_auth(auth, headers, auth_header)
        })
    }
}

fn compose_oauth_auth(
    provider_id: &str,
    base: Option<&Arc<dyn Provider>>,
    config: Option<&ModelsJsonProvider>,
    extension: Option<&ProviderConfigInput>,
) -> Option<Arc<dyn OAuthAuth>> {
    let oauth: Arc<dyn OAuthAuth> = match extension.and_then(|extension| extension.oauth.clone()) {
        Some(config) => Arc::new(ExtensionOAuth { config }),
        None => base?.auth().oauth.clone()?,
    };
    Some(Arc::new(ComposedOAuthAuth {
        oauth,
        provider_id: provider_id.to_owned(),
        raw_headers: configured_headers(config, extension),
        auth_header: extension
            .and_then(|extension| extension.auth_header)
            .or_else(|| config.and_then(|config| config.auth_header))
            .unwrap_or(false),
    }))
}

type ModelsState = Arc<Mutex<(Option<Vec<Model>>, Option<OAuthCredential>)>>;

fn current_extension(extension: &ProviderConfigInput, state: &ModelsState) -> ProviderConfigInput {
    let refreshed = lock(state).0.clone();
    match refreshed {
        Some(models) => ProviderConfigInput {
            models: Some(models),
            ..extension.clone()
        },
        None => extension.clone(),
    }
}

fn composed_models(
    provider_id: &str,
    base_models: &[Model],
    config: Option<&ModelsJsonProvider>,
    extension: &ProviderConfigInput,
    state: &ModelsState,
) -> Result<Vec<Model>, ProviderCompositionError> {
    let extension = current_extension(extension, state);
    let mut models = apply_extension(
        provider_id,
        &apply_models_json(provider_id, base_models, config)?,
        Some(&extension),
    )?;
    let oauth_credential = lock(state).1.clone();
    if let (Some(credential), Some(modify)) = (
        oauth_credential,
        extension.oauth.as_ref().and_then(|oauth| oauth.modify_models.as_ref()),
    ) {
        models = modify(models, &credential);
    }
    if let Some(overrides) = config.and_then(|config| config.model_overrides.as_ref()) {
        models = models
            .into_iter()
            .map(|model| match overrides.get(&model.id) {
                Some(model_override) => apply_model_override(&model, model_override),
                None => model,
            })
            .collect();
    }
    Ok(models)
}

/// The API-registry lookup from `pi-ai/compat`.
fn api_provider(api: &str) -> Option<Arc<dyn ProviderStreams>> {
    pi_ai::compat::get_api_provider(api).map(|provider| provider as Arc<dyn ProviderStreams>)
}

struct ComposedProvider {
    id: String,
    name: String,
    base_url: Option<JsString>,
    headers: Option<ProviderHeaders>,
    auth: ProviderAuth,
    get_models: Arc<dyn Fn() -> Vec<Model> + Send + Sync>,
    refresh_models: Option<ComposedRefreshFn>,
    filter_models: Option<FilterModelsFn>,
    stream_fn: Arc<dyn Fn(Model, Context, StreamOptions) -> AssistantMessageEventStream + Send + Sync>,
    stream_simple_fn: Arc<dyn Fn(Model, Context, SimpleStreamOptions) -> AssistantMessageEventStream + Send + Sync>,
    fetch_deferred: Option<FetchDeferredFn>,
    cancel_deferred: Option<CancelDeferredFn>,
}

impl Provider for ComposedProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn base_url(&self) -> Option<&JsString> {
        self.base_url.as_ref()
    }

    fn headers(&self) -> Option<&ProviderHeaders> {
        self.headers.as_ref()
    }

    fn auth(&self) -> &ProviderAuth {
        &self.auth
    }

    fn get_models(&self) -> Vec<Model> {
        (self.get_models)()
    }

    fn supports_refresh_models(&self) -> bool {
        self.refresh_models.is_some()
    }

    fn refresh_models(&self, context: RefreshModelsContext) -> Option<BoxFuture<'static, Result<(), CallbackError>>> {
        self.refresh_models.as_ref().map(|refresh| refresh(context))
    }

    fn filter_models(&self, models: Vec<Model>, credential: Option<&Credential>) -> Vec<Model> {
        match &self.filter_models {
            Some(filter) => filter(models, credential),
            None => models,
        }
    }

    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        (self.stream_fn)(model, context, options)
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        (self.stream_simple_fn)(model, context, options)
    }

    fn supports_fetch_deferred(&self) -> bool {
        self.fetch_deferred.is_some()
    }

    fn fetch_deferred(
        &self,
        model: Model,
        handle: DeferredHandle,
        options: DeferredFetchOptions,
    ) -> AssistantMessageEventStream {
        match &self.fetch_deferred {
            Some(fetch) => fetch(model, handle, options),
            None => pi_ai::request::error_stream(&model, "Provider does not support deferred responses"),
        }
    }

    fn supports_cancel_deferred(&self) -> bool {
        self.cancel_deferred.is_some()
    }

    fn cancel_deferred(
        &self,
        model: Model,
        handle: DeferredHandle,
        options: DeferredCancelOptions,
    ) -> BoxFuture<'static, Result<(), CallbackError>> {
        match &self.cancel_deferred {
            Some(cancel) => cancel(model, handle, options),
            None => Box::pin(async { Err(CallbackError::new("Provider cannot cancel deferred responses")) }),
        }
    }
}

/// Composes built-in, `models.json`, and extension layers without reading credentials.
pub fn compose_model_provider(
    provider_id: &str,
    base: Option<Arc<dyn Provider>>,
    model_config: &ModelConfig,
    extension: Option<&ProviderConfigInput>,
) -> Result<Arc<dyn Provider>, ProviderCompositionError> {
    let config = parse_provider_config(model_config, provider_id)?;
    let state: ModelsState = Arc::new(Mutex::new((None, None)));
    let base_models = base.as_ref().map(|base| base.get_models()).unwrap_or_default();

    // Validate eagerly so registration/reload reports structural errors immediately.
    composed_models(
        provider_id,
        &base_models,
        config.as_ref(),
        extension.unwrap_or(&ProviderConfigInput::default()),
        &state,
    )?;

    let api_key = compose_api_key_auth(provider_id, base.as_ref(), config.as_ref(), extension);
    let oauth = compose_oauth_auth(provider_id, base.as_ref(), config.as_ref(), extension);
    if api_key.is_none() && oauth.is_none() {
        return Err(ProviderCompositionError::new(format!(
            "Provider {provider_id}: no authentication method configured."
        )));
    }

    let get_models = {
        let provider_id = provider_id.to_owned();
        let config = config.clone();
        let extension = extension.cloned();
        let state = Arc::clone(&state);
        let base_models = base_models.clone();
        Arc::new(move || {
            let extension = extension.clone().unwrap_or_default();
            composed_models(&provider_id, &base_models, config.as_ref(), &extension, &state).unwrap_or_default()
        })
    };

    let name = extension
        .and_then(|extension| extension.name.clone())
        .or_else(|| config.as_ref().and_then(|config| config.name.clone()))
        .or_else(|| base.as_ref().map(|base| base.name().to_owned()))
        .or_else(|| {
            extension
                .and_then(|extension| extension.oauth.as_ref())
                .map(|oauth| oauth.name.clone())
        })
        .unwrap_or_else(|| provider_id.to_owned());
    let base_url = extension
        .and_then(|extension| extension.base_url.clone())
        .or_else(|| config.as_ref().and_then(|config| config.base_url.clone()))
        .or_else(|| base.as_ref().and_then(|base| base.base_url().cloned()));
    let headers = base.as_ref().and_then(|base| base.headers().cloned());

    let supports_base_api = {
        let base_models = base_models.clone();
        Arc::new(move |model: &Model| base_models.iter().any(|entry| entry.api == model.api))
    };

    let stream_simple_override = extension.and_then(|extension| extension.stream_simple.clone());
    let extension_api = extension.and_then(|extension| extension.api.clone());
    let base_stream = base.clone();
    let supports = Arc::clone(&supports_base_api);
    let stream_fn: Arc<dyn Fn(Model, Context, StreamOptions) -> AssistantMessageEventStream + Send + Sync> =
        Arc::new(move |model, context, options| {
            let stream_simple_override = stream_simple_override.clone();
            let extension_api = extension_api.clone();
            let base_stream = base_stream.clone();
            let supports = Arc::clone(&supports);
            lazy_stream(model.clone(), move || {
                Box::pin(async move {
                    if let (Some(stream_simple), Some(api)) = (&stream_simple_override, &extension_api)
                        && model.api == *api
                    {
                        return Ok(stream_simple(
                            model,
                            context,
                            Some(SimpleStreamOptions {
                                stream: options,
                                ..Default::default()
                            }),
                        ));
                    }
                    if let Some(base) = &base_stream
                        && supports(&model)
                    {
                        return Ok(base.stream(model, context, options));
                    }
                    let Some(api_provider) = api_provider(&model.api) else {
                        return Err(CallbackError::new(format!(
                            "No API provider registered for api: {}",
                            model.api
                        )));
                    };
                    Ok(api_provider.stream(model, context, options))
                })
            })
        });

    let stream_simple_override = extension.and_then(|extension| extension.stream_simple.clone());
    let extension_api = extension.and_then(|extension| extension.api.clone());
    let base_stream = base.clone();
    let supports = Arc::clone(&supports_base_api);
    let stream_simple_fn: Arc<
        dyn Fn(Model, Context, SimpleStreamOptions) -> AssistantMessageEventStream + Send + Sync,
    > = Arc::new(move |model, context, options| {
        let stream_simple_override = stream_simple_override.clone();
        let extension_api = extension_api.clone();
        let base_stream = base_stream.clone();
        let supports = Arc::clone(&supports);
        lazy_stream(model.clone(), move || {
            Box::pin(async move {
                if let (Some(stream_simple), Some(api)) = (&stream_simple_override, &extension_api)
                    && model.api == *api
                {
                    return Ok(stream_simple(model, context, Some(options)));
                }
                if let Some(base) = &base_stream
                    && supports(&model)
                {
                    return Ok(base.stream_simple(model, context, options));
                }
                let Some(api_provider) = api_provider(&model.api) else {
                    return Err(CallbackError::new(format!(
                        "No API provider registered for api: {}",
                        model.api
                    )));
                };
                Ok(api_provider.stream_simple(model, context, options))
            })
        })
    });

    let refresh_models = compose_refresh_models(
        provider_id,
        base.as_ref(),
        config.as_ref(),
        extension,
        &base_models,
        Arc::clone(&state),
    );
    let filter_models = base.as_ref().map(|base| {
        let base = Arc::clone(base);
        Arc::new(move |models: Vec<Model>, credential: Option<&Credential>| base.filter_models(models, credential))
            as Arc<dyn Fn(Vec<Model>, Option<&Credential>) -> Vec<Model> + Send + Sync>
    });
    let fetch_deferred = base.as_ref().filter(|base| base.supports_fetch_deferred()).map(|base| {
        let base = Arc::clone(base);
        Arc::new(
            move |model: Model, handle: DeferredHandle, options: DeferredFetchOptions| {
                base.fetch_deferred(model, handle, options)
            },
        )
            as Arc<dyn Fn(Model, DeferredHandle, DeferredFetchOptions) -> AssistantMessageEventStream + Send + Sync>
    });
    let cancel_deferred = base
        .as_ref()
        .filter(|base| base.supports_cancel_deferred())
        .map(|base| {
            let base = Arc::clone(base);
            Arc::new(
                move |model: Model, handle: DeferredHandle, options: DeferredCancelOptions| {
                    base.cancel_deferred(model, handle, options)
                },
            )
                as Arc<
                    dyn Fn(
                            Model,
                            DeferredHandle,
                            DeferredCancelOptions,
                        ) -> BoxFuture<'static, Result<(), CallbackError>>
                        + Send
                        + Sync,
                >
        });

    Ok(Arc::new(ComposedProvider {
        id: provider_id.to_owned(),
        name,
        base_url,
        headers,
        auth: ProviderAuth { api_key, oauth },
        get_models,
        refresh_models,
        filter_models,
        stream_fn,
        stream_simple_fn,
        fetch_deferred,
        cancel_deferred,
    }))
}

fn compose_refresh_models(
    provider_id: &str,
    base: Option<&Arc<dyn Provider>>,
    config: Option<&ModelsJsonProvider>,
    extension: Option<&ProviderConfigInput>,
    base_models: &[Model],
    state: ModelsState,
) -> Option<ComposedRefreshFn> {
    let has_base_refresh = base.is_some_and(|base| base.supports_refresh_models());
    let extension_refresh = extension.and_then(|extension| extension.refresh_models.clone());
    let has_modify_models = extension
        .and_then(|extension| extension.oauth.as_ref())
        .is_some_and(|oauth| oauth.modify_models.is_some());
    if !has_base_refresh && extension_refresh.is_none() && !has_modify_models {
        return None;
    }
    let base = base.cloned();
    let extension = extension.cloned();
    let config = config.cloned();
    let base_models = base_models.to_vec();
    let provider_id = provider_id.to_owned();
    Some(Arc::new(move |context: RefreshModelsContext| {
        let base = base.clone();
        let extension = extension.clone();
        let config = config.clone();
        let base_models = base_models.clone();
        let provider_id = provider_id.clone();
        let state = Arc::clone(&state);
        Box::pin(async move {
            if let Some(base) = &base
                && base.supports_refresh_models()
                && let Some(refresh) = base.refresh_models(context.clone())
            {
                refresh.await?;
            }
            let refreshed = match extension
                .as_ref()
                .and_then(|extension| extension.refresh_models.as_ref())
            {
                Some(refresh) => Some(refresh(context.clone()).await?),
                None => None,
            };
            if context.signal.is_cancelled() {
                return Ok(());
            }
            let oauth_credential = match &context.credential {
                Some(Credential::OAuth(credential)) => Some(credential.clone()),
                _ => None,
            };
            if let Some(refreshed) = refreshed {
                let extension = extension.as_ref().expect("refreshModels implies an extension");
                apply_extension(
                    &provider_id,
                    &apply_models_json(&provider_id, &base_models, config.as_ref())
                        .map_err(|error| CallbackError::new(error.to_string()))?,
                    Some(&ProviderConfigInput {
                        models: Some(refreshed.clone()),
                        ..extension.clone()
                    }),
                )
                .map_err(|error| CallbackError::new(error.to_string()))?;
                let update_state = Arc::clone(&state);
                let published = refreshed.clone();
                (context.publish)(pi_ai::models::ModelsPublication::default().with_update(move || {
                    lock(&update_state).0 = Some(published.clone());
                }))
                .await?;
            }
            let update_state = Arc::clone(&state);
            let published_credential = oauth_credential.clone();
            (context.publish)(pi_ai::models::ModelsPublication::default().with_update(move || {
                lock(&update_state).1 = published_credential.clone();
            }))
            .await?;
            Ok(())
        })
    }))
}

/// Resolves configured headers for a model request.
pub fn resolve_configured_model_headers(
    model: &Model,
    config: Option<&ModelsJsonProvider>,
    extension: Option<&ProviderConfigInput>,
    env: Option<&HashMap<String, JsString>>,
) -> Result<Option<HashMap<String, JsString>>, ProviderCompositionError> {
    let mut headers = configured_headers(config, extension).unwrap_or_default();
    if let Some(model_headers) = raw_model_headers(model, config, extension) {
        headers.extend(model_headers);
    }
    Ok(resolve_headers_or_throw(
        (!headers.is_empty()).then_some(&headers),
        &js_format!("model \"{}/{}\"", &model.provider, &model.id),
        env,
    )?)
}

/// Compatibility request auth used by `ModelRegistry` when auth is unconfigured.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompatibilityRequestConfig {
    pub headers: Option<ProviderHeaders>,
    pub auth_header: bool,
}

/// `resolveCompatibilityRequestConfig`.
pub fn resolve_compatibility_request_config(
    model: &Model,
    config: Option<&ModelsJsonProvider>,
    extension: Option<&ProviderConfigInput>,
) -> Result<CompatibilityRequestConfig, ProviderCompositionError> {
    let mut configured = configured_headers(config, extension).unwrap_or_default();
    if let Some(model_headers) = raw_model_headers(model, config, extension) {
        configured.extend(model_headers);
    }
    let configured = resolve_headers_or_throw(
        (!configured.is_empty()).then_some(&configured),
        &js_format!("model \"{}/{}\"", &model.provider, &model.id),
        None,
    )?;
    let headers = if model.headers.is_some() || configured.is_some() {
        let mut merged = model.headers.clone().unwrap_or_default();
        merged.extend(configured.unwrap_or_default());
        Some(provider_headers_from_map(merged))
    } else {
        None
    };
    Ok(CompatibilityRequestConfig {
        headers,
        auth_header: extension
            .and_then(|extension| extension.auth_header)
            .or_else(|| config.and_then(|config| config.auth_header))
            .unwrap_or(false),
    })
}

/// Configured auth status without reading credentials or running commands.
pub fn configured_request_auth_status(
    config: Option<&ModelsJsonProvider>,
    extension: Option<&ProviderConfigInput>,
) -> Option<AuthStatus> {
    let value = configured_api_key(config, extension)?;
    if is_command_config_value(&value) {
        return Some(AuthStatus {
            configured: true,
            source: Some(AuthStatusSource::ModelsJsonCommand),
            label: None,
        });
    }
    let names = get_config_value_env_var_names(&value);
    if !names.is_empty() {
        return Some(if is_config_value_configured(&value, None) {
            AuthStatus {
                configured: true,
                source: Some(AuthStatusSource::Environment),
                label: Some(JsString::join(names, ", ")),
            }
        } else {
            AuthStatus {
                configured: false,
                source: None,
                label: None,
            }
        });
    }
    Some(AuthStatus {
        configured: true,
        source: Some(
            if extension.and_then(|extension| extension.api_key.as_ref()).is_some() {
                AuthStatusSource::Fallback
            } else {
                AuthStatusSource::ModelsJsonKey
            },
        ),
        label: None,
    })
}

/// Parses a `models.json` provider entry (schema already validated by [`ModelConfig`]).
pub fn models_json_provider(
    config: &ModelConfig,
    provider_id: &str,
) -> Result<Option<ModelsJsonProvider>, ProviderCompositionError> {
    parse_provider_config(config, provider_id)
}

/// Converts a string map into [`ProviderHeaders`].
pub fn provider_headers(headers: HashMap<String, JsString>) -> ProviderHeaders {
    provider_headers_from_map(headers)
}

/// Re-exported `Modality` mapping used by `models.json` fixtures.
pub fn modality_from_str(value: &str) -> Option<Modality> {
    match value {
        "text" => Some(Modality::Text),
        "image" => Some(Modality::Image),
        _ => None,
    }
}

/// `ModelThinkingLevel` keys accepted by `thinkingLevelMap` fixtures.
pub fn thinking_level_key(value: &str) -> Option<ModelThinkingLevel> {
    Some(match value {
        "off" => ModelThinkingLevel::Off,
        "minimal" => ModelThinkingLevel::Minimal,
        "low" => ModelThinkingLevel::Low,
        "medium" => ModelThinkingLevel::Medium,
        "high" => ModelThinkingLevel::High,
        "xhigh" => ModelThinkingLevel::Xhigh,
        "max" => ModelThinkingLevel::Max,
        "ultra" => ModelThinkingLevel::Ultra,
        _ => return None,
    })
}

/// Builds an empty string-keyed header map, used by callers composing overrides.
pub fn empty_headers() -> HashMap<String, JsString> {
    HashMap::new()
}

/// Kept for API symmetry with the TypeScript `mergeCompat` nested keys.
pub const NESTED_COMPAT_KEYS: [&str; 4] = [
    "openRouterRouting",
    "vercelGatewayRouting",
    "chatTemplateKwargs",
    "chatTemplateArgs",
];

/// Model list ordering is not part of this module; expose the provider-id list
/// helper used by tests to build fixtures deterministically.
pub fn provider_models_by_id(models: &[Model]) -> BTreeMap<String, Model> {
    models.iter().map(|model| (model.id.clone(), model.clone())).collect()
}
