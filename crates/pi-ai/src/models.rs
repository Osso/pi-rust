//! Frozen `packages/ai/src/models.ts`: provider collection, auth and stream dispatch.
use futures_util::future::{FutureExt, Shared, join_all};
use indexmap::IndexMap;
use pi_js_compat::js_json::JsString;
use std::sync::{Arc, Mutex, MutexGuard};

use crate::api::lazy::lazy_stream;
use crate::auth::context::default_provider_auth_context;
use crate::auth::credential_store::InMemoryCredentialStore;
use crate::auth::resolve::{AuthResolutionOverrides, resolve_provider_auth};
pub use crate::auth::resolve::{ModelsError, ModelsErrorCode};
use crate::auth::types::{AuthContext, AuthModel, AuthResult, CredentialStore, ProviderAuth};
use crate::request::{
    BoxFuture, CallbackError, ProviderRequestOptions, ProviderStreams, SimpleStreamOptions, StreamOptions,
};
use crate::types::{AssistantMessage, Context, Model, ModelThinkingLevel, ProviderHeaders, Usage, UsageCost};
use crate::utils::event_stream::{AssistantMessageEventStream, StreamFailure};

pub type RefreshModelsFn = Arc<dyn Fn() -> BoxFuture<'static, Result<(), CallbackError>> + Send + Sync>;
pub type FetchModelsFn = Arc<dyn Fn() -> BoxFuture<'static, Result<Vec<Model>, CallbackError>> + Send + Sync>;

/// Frozen Provider: optional refresh is represented by method presence, not a no-op implementation.
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn base_url(&self) -> Option<&JsString>;
    fn headers(&self) -> Option<&ProviderHeaders>;
    fn auth(&self) -> &ProviderAuth;
    fn get_models(&self) -> Result<Vec<Model>, CallbackError>;
    fn refresh_models(&self) -> Option<RefreshModelsFn>;
    fn stream(
        &self,
        model: Model,
        context: Context,
        options: StreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError>;
    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError>;
}

pub trait Models: Send + Sync {
    fn get_providers(&self) -> Vec<Arc<dyn Provider>>;
    fn get_provider(&self, id: &str) -> Option<Arc<dyn Provider>>;
    fn get_models(&self, provider: Option<&str>) -> Vec<Model>;
    fn get_model(&self, provider: &str, id: &str) -> Option<Model>;
    fn refresh(&self, provider: Option<&str>) -> BoxFuture<'static, Result<(), ModelsError>>;
    fn get_auth(&self, model: &Model) -> BoxFuture<'static, Result<Option<AuthResult>, ModelsError>>;
    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream;
    fn complete(
        &self,
        model: Model,
        context: Context,
        options: StreamOptions,
    ) -> BoxFuture<'static, Result<AssistantMessage, StreamFailure>>;
    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream;
    fn complete_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> BoxFuture<'static, Result<AssistantMessage, StreamFailure>>;
}

pub trait MutableModels: Models {
    fn set_provider(&self, provider: Arc<dyn Provider>);
    fn delete_provider(&self, id: &str);
    fn clear_providers(&self);
}

#[derive(Default)]
pub struct CreateModelsOptions {
    pub credentials: Option<Arc<dyn CredentialStore>>,
    pub auth_context: Option<Arc<dyn AuthContext>>,
}

struct Inner {
    providers: Mutex<IndexMap<String, Arc<dyn Provider>>>,
    credentials: Arc<dyn CredentialStore>,
    auth_context: Arc<dyn AuthContext>,
}

#[derive(Clone)]
pub struct ModelsCollection {
    inner: Arc<Inner>,
}

pub fn create_models(options: CreateModelsOptions) -> ModelsCollection {
    ModelsCollection {
        inner: Arc::new(Inner {
            providers: Mutex::new(IndexMap::new()),
            credentials: options
                .credentials
                .unwrap_or_else(|| Arc::new(InMemoryCredentialStore::new())),
            auth_context: options.auth_context.unwrap_or_else(default_provider_auth_context),
        }),
    }
}

impl ModelsCollection {
    fn require_provider(&self, model: &Model) -> Result<Arc<dyn Provider>, ModelsError> {
        self.get_provider(&model.provider).ok_or_else(|| {
            ModelsError::new(
                ModelsErrorCode::Provider,
                format!("Unknown provider: {}", model.provider),
            )
        })
    }

    async fn resolve_auth(
        &self,
        model: &Model,
        overrides: AuthResolutionOverrides,
    ) -> Result<Option<AuthResult>, ModelsError> {
        let Some(provider) = self.get_provider(&model.provider) else {
            return Ok(None);
        };
        resolve_provider_auth(
            provider.id(),
            provider.auth(),
            AuthModel::Chat(model.clone()),
            Arc::clone(&self.inner.credentials),
            Arc::clone(&self.inner.auth_context),
            overrides,
        )
        .await
    }

    /// Frozen :233–256: unconfigured auth leaves the request unchanged, not rejected.
    async fn apply_auth(
        &self,
        model: Model,
        mut options: ProviderRequestOptions,
    ) -> Result<(Model, ProviderRequestOptions), ModelsError> {
        let resolution = self
            .resolve_auth(
                &model,
                AuthResolutionOverrides {
                    api_key: options.api_key.clone(),
                    env: options.env.clone(),
                },
            )
            .await?;
        let Some(resolution) = resolution else {
            return Ok((model, options));
        };
        let auth = resolution.auth;
        let model = match auth.base_url.filter(|url| !url.is_empty()) {
            Some(base_url) => Model { base_url, ..model },
            None => model,
        };
        options.api_key = options.api_key.or(auth.api_key);
        // Object spread is case-sensitive; HTTP header normalization belongs to the adapter.
        options.headers = merge_request_maps(auth.headers, options.headers);
        options.env = merge_request_maps(resolution.env, options.env);
        Ok((model, options))
    }
}

fn merge_request_maps<K: std::hash::Hash + Eq, V>(
    base: Option<IndexMap<K, V>>,
    overrides: Option<IndexMap<K, V>>,
) -> Option<IndexMap<K, V>> {
    if base.is_none() && overrides.is_none() {
        return None;
    }
    let mut merged = base.unwrap_or_default();
    merged.extend(overrides.unwrap_or_default());
    Some(merged)
}

pub(crate) fn wrap_refresh_error(provider: &str, error: CallbackError) -> ModelsError {
    // Frozen :205 tests the thrown error itself, never a ModelsError in its cause chain.
    if let Some(existing) = error
        .original
        .as_deref()
        .and_then(|source| source.downcast_ref::<ModelsError>())
    {
        return existing.clone();
    }
    ModelsError::with_cause(
        ModelsErrorCode::ModelSource,
        format!("Model refresh failed for {provider}"),
        Some(Box::new(error.thrown_value())),
    )
}

impl Models for ModelsCollection {
    fn get_providers(&self) -> Vec<Arc<dyn Provider>> {
        lock(&self.inner.providers).values().cloned().collect()
    }
    fn get_provider(&self, id: &str) -> Option<Arc<dyn Provider>> {
        lock(&self.inner.providers).get(id).cloned()
    }
    fn get_models(&self, provider: Option<&str>) -> Vec<Model> {
        let providers = match provider {
            Some(id) => self.get_provider(id).into_iter().collect(),
            None => self.get_providers(),
        };
        // Frozen models.ts:175–194 catches only getModels failures, per provider.
        providers
            .into_iter()
            .flat_map(|provider| provider.get_models().unwrap_or_default())
            .collect()
    }
    fn get_model(&self, provider: &str, id: &str) -> Option<Model> {
        self.get_models(Some(provider)).into_iter().find(|model| model.id == id)
    }
    fn refresh(&self, provider: Option<&str>) -> BoxFuture<'static, Result<(), ModelsError>> {
        if let Some(id) = provider {
            let hook = self.get_provider(id).and_then(|provider| provider.refresh_models());
            let id = id.to_owned();
            return Box::pin(async move {
                match hook {
                    Some(refresh) => refresh().await.map_err(|error| wrap_refresh_error(&id, error)),
                    None => Ok(()),
                }
            });
        }
        let providers = self.get_providers();
        Box::pin(async move {
            // Frozen :213: allSettled captures every provider's independent failure.
            join_all(providers.into_iter().map(|provider| async move {
                if let Some(refresh) = provider.refresh_models() {
                    let _ = refresh().await;
                }
            }))
            .await;
            Ok(())
        })
    }
    fn get_auth(&self, model: &Model) -> BoxFuture<'static, Result<Option<AuthResult>, ModelsError>> {
        let collection = self.clone();
        let model = model.clone();
        Box::pin(async move { collection.resolve_auth(&model, Default::default()).await })
    }
    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        let collection = self.clone();
        lazy_stream(model.clone(), move || {
            Box::pin(async move {
                let provider = collection
                    .require_provider(&model)
                    .map_err(ModelsError::into_callback)?;
                let mut options = options;
                let (model, request) = collection
                    .apply_auth(model, options.request)
                    .await
                    .map_err(ModelsError::into_callback)?;
                options.request = request;
                provider.stream(model, context, options)
            })
        })
    }
    fn complete(
        &self,
        model: Model,
        context: Context,
        options: StreamOptions,
    ) -> BoxFuture<'static, Result<AssistantMessage, StreamFailure>> {
        let stream = self.stream(model, context, options);
        Box::pin(async move { stream.result().await })
    }
    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        let collection = self.clone();
        lazy_stream(model.clone(), move || {
            Box::pin(async move {
                let provider = collection
                    .require_provider(&model)
                    .map_err(ModelsError::into_callback)?;
                let mut options = options;
                let (model, request) = collection
                    .apply_auth(model, options.stream.request)
                    .await
                    .map_err(ModelsError::into_callback)?;
                options.stream.request = request;
                provider.stream_simple(model, context, options)
            })
        })
    }
    fn complete_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> BoxFuture<'static, Result<AssistantMessage, StreamFailure>> {
        let stream = self.stream_simple(model, context, options);
        Box::pin(async move { stream.result().await })
    }
}

impl MutableModels for ModelsCollection {
    fn set_provider(&self, provider: Arc<dyn Provider>) {
        lock(&self.inner.providers).insert(provider.id().to_owned(), provider);
    }
    fn delete_provider(&self, id: &str) {
        lock(&self.inner.providers).shift_remove(id);
    }
    fn clear_providers(&self) {
        lock(&self.inner.providers).clear();
    }
}

#[derive(Clone)]
pub enum ProviderApi {
    Single(Arc<dyn ProviderStreams>),
    Map(IndexMap<String, Arc<dyn ProviderStreams>>),
}

pub struct CreateProviderOptions {
    pub id: String,
    pub name: Option<String>,
    pub base_url: Option<JsString>,
    pub headers: Option<ProviderHeaders>,
    pub auth: ProviderAuth,
    pub models: Vec<Model>,
    pub refresh_models: Option<FetchModelsFn>,
    pub api: ProviderApi,
}

type InflightRefresh = Shared<BoxFuture<'static, Result<(), CallbackError>>>;
struct ProviderModelState {
    models: Vec<Model>,
    inflight: Option<InflightRefresh>,
}
struct ConfiguredProvider {
    id: String,
    name: String,
    base_url: Option<JsString>,
    headers: Option<ProviderHeaders>,
    auth: ProviderAuth,
    state: Arc<Mutex<ProviderModelState>>,
    refresh: Option<RefreshModelsFn>,
    api: ProviderApi,
}

impl ConfiguredProvider {
    fn dispatch(
        &self,
        model: Model,
        run: impl FnOnce(&Arc<dyn ProviderStreams>) -> Result<AssistantMessageEventStream, CallbackError>,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        let streams = match &self.api {
            ProviderApi::Single(api) => Some(api),
            ProviderApi::Map(apis) => apis.get(&model.api),
        };
        match streams {
            Some(streams) => run(streams),
            None => {
                let error = ModelsError::new(
                    ModelsErrorCode::Stream,
                    format!("Provider {} has no API implementation for \"{}\"", self.id, model.api),
                )
                .into_callback();
                Ok(lazy_stream(model, move || Box::pin(async move { Err(error) })))
            }
        }
    }
}
impl Provider for ConfiguredProvider {
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
    fn get_models(&self) -> Result<Vec<Model>, CallbackError> {
        Ok(lock(&self.state).models.clone())
    }
    fn refresh_models(&self) -> Option<RefreshModelsFn> {
        self.refresh.clone()
    }
    fn stream(
        &self,
        model: Model,
        context: Context,
        options: StreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        self.dispatch(model.clone(), move |streams| streams.stream(model, context, options))
    }
    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        self.dispatch(model.clone(), move |streams| {
            streams.stream_simple(model, context, options)
        })
    }
}

fn build_refresh_hook(state: Arc<Mutex<ProviderModelState>>, fetch: FetchModelsFn) -> RefreshModelsFn {
    Arc::new(move || {
        let mut guard = lock(&state);
        if let Some(inflight) = &guard.inflight {
            return Box::pin(inflight.clone());
        }
        let state = Arc::clone(&state);
        let fetch = Arc::clone(&fetch);
        let task = tokio::spawn(async move {
            let result = fetch().await;
            let mut guard = lock(&state);
            let result = result.map(|models| {
                guard.models = models;
            });
            guard.inflight = None;
            result
        });
        let inflight = async move {
            task.await
                .map_err(|error| CallbackError::from_error(error.to_string().into(), error))?
        }
        .boxed()
        .shared();
        guard.inflight = Some(inflight.clone());
        Box::pin(inflight)
    })
}

pub fn create_provider(input: CreateProviderOptions) -> Arc<dyn Provider> {
    let state = Arc::new(Mutex::new(ProviderModelState {
        models: input.models,
        inflight: None,
    }));
    let refresh = input
        .refresh_models
        .map(|fetch| build_refresh_hook(Arc::clone(&state), fetch));
    Arc::new(ConfiguredProvider {
        name: input.name.unwrap_or_else(|| input.id.clone()),
        id: input.id,
        base_url: input.base_url,
        headers: input.headers,
        auth: input.auth,
        state,
        refresh,
        api: input.api,
    })
}

pub fn has_api(model: &Model, api: &str) -> bool {
    model.api == api
}

/// Owned-value form of frozen calculateCost; built-in consumers assign the returned cost.
pub fn calculate_cost(model: &Model, usage: &Usage) -> UsageCost {
    let rates = &model.cost.rates;
    let long_write = usage.cache_write_1h.unwrap_or(0.0);
    let short_write = usage.cache_write - long_write;
    let input = rates.input / 1_000_000.0 * usage.input;
    let output = rates.output / 1_000_000.0 * usage.output;
    let cache_read = rates.cache_read / 1_000_000.0 * usage.cache_read;
    let cache_write = (rates.cache_write * short_write + rates.input * 2.0 * long_write) / 1_000_000.0;
    UsageCost {
        input,
        output,
        cache_read,
        cache_write,
        total: input + output + cache_read + cache_write,
    }
}

const EXTENDED_THINKING_LEVELS: [ModelThinkingLevel; 8] = [
    ModelThinkingLevel::Off,
    ModelThinkingLevel::Minimal,
    ModelThinkingLevel::Low,
    ModelThinkingLevel::Medium,
    ModelThinkingLevel::High,
    ModelThinkingLevel::Xhigh,
    ModelThinkingLevel::Max,
    ModelThinkingLevel::Ultra,
];

pub fn get_supported_thinking_levels(model: &Model) -> Vec<ModelThinkingLevel> {
    if !model.reasoning {
        return vec![ModelThinkingLevel::Off];
    }
    EXTENDED_THINKING_LEVELS
        .iter()
        .copied()
        .filter(|level| {
            let mapped = model.thinking_level_map.as_ref().and_then(|map| map.get(level));
            match mapped {
                Some(None) => false,
                None => !matches!(
                    level,
                    ModelThinkingLevel::Xhigh | ModelThinkingLevel::Max | ModelThinkingLevel::Ultra
                ),
                Some(Some(_)) => true,
            }
        })
        .collect()
}
pub fn clamp_thinking_level(model: &Model, level: ModelThinkingLevel) -> ModelThinkingLevel {
    let available = get_supported_thinking_levels(model);
    if available.contains(&level) {
        return level;
    }
    let Some(index) = EXTENDED_THINKING_LEVELS
        .iter()
        .position(|candidate| *candidate == level)
    else {
        return available.first().copied().unwrap_or(ModelThinkingLevel::Off);
    };
    EXTENDED_THINKING_LEVELS[index..]
        .iter()
        .chain(EXTENDED_THINKING_LEVELS[..index].iter().rev())
        .find(|candidate| available.contains(candidate))
        .copied()
        .unwrap_or(ModelThinkingLevel::Off)
}
pub fn models_are_equal(a: Option<&Model>, b: Option<&Model>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.id == b.id && a.provider == b.provider,
        _ => false,
    }
}
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}
