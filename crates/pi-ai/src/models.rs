//! Port of `packages/ai/src/models.ts`.
//!
//! `Models` owns auth resolution, refresh generation/supersession, and
//! serialized model publication. Providers own stream behavior.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

use indexmap::IndexMap;
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;

use crate::api::lazy::lazy_stream;
use crate::auth::context::default_provider_auth_context;
use crate::auth::credential_store::InMemoryCredentialStore;
use crate::auth::resolve::{AuthResolutionOverrides, ResolveError, resolve_provider_auth};
pub use crate::auth::resolve::{ModelsError, ModelsErrorCode};
use crate::auth::types::{
    ApiKeyCheckInput, ApiKeyCredential, ApiKeyResolveInput, AuthCheck, AuthContext, AuthError, AuthInteraction,
    AuthOperationOptions, AuthResult, AuthType, Credential, CredentialStore, ProviderAuth,
};
use crate::models_store::{InMemoryModelsStore, ModelsStore, ModelsStoreEntry, ModelsStoreOperationOptions};
use crate::request::{
    BoxFuture, CallbackError, DeferredCancelOptions, DeferredFetchOptions, HeaderTransform, ProviderStreams,
    SimpleStreamOptions, StreamOptions,
};
use crate::types::{
    AssistantMessage, Context, DeferredHandle, Model, ModelCostRates, ModelThinkingLevel, ProviderHeaders, Usage,
    UsageCost,
};
use crate::utils::abort::{operation_signal, race_with_abort_signal};
use crate::utils::event_stream::{AssistantMessageEventStream, StreamFailure};

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

/// `ModelsPublication`.
#[derive(Clone)]
pub struct ModelsPublication {
    pub persist: PersistAction,
    pub update: Option<Arc<dyn Fn() + Send + Sync>>,
}

impl Default for ModelsPublication {
    fn default() -> Self {
        Self {
            persist: PersistAction::Unchanged,
            update: None,
        }
    }
}

impl ModelsPublication {
    pub fn new(persist: PersistAction) -> Self {
        Self { persist, update: None }
    }

    pub fn with_update(mut self, update: impl Fn() + Send + Sync + 'static) -> Self {
        self.update = Some(Arc::new(update));
        self
    }
}

#[derive(Clone, Default)]
pub enum PersistAction {
    #[default]
    Unchanged,
    Delete,
    Write(ModelsStoreEntry),
}

pub type PublishFn = Arc<dyn Fn(ModelsPublication) -> BoxFuture<'static, Result<bool, CallbackError>> + Send + Sync>;

/// `RefreshModelsContext`.
#[derive(Clone)]
pub struct RefreshModelsContext {
    pub credential: Option<Credential>,
    pub stored: Option<ModelsStoreEntry>,
    pub publish: PublishFn,
    pub allow_network: bool,
    pub force: Option<bool>,
    pub signal: CancellationToken,
}

#[derive(Debug, Clone, Default)]
pub struct ModelsRefreshOptions {
    pub allow_network: Option<bool>,
    pub providers: Option<Vec<String>>,
    pub force: Option<bool>,
    pub signal: Option<CancellationToken>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelsRefreshResult {
    pub aborted: bool,
    pub errors: IndexMap<String, CallbackError>,
}

/// A provider: id/name/base metadata, auth methods, model listing, and streams.
pub trait Provider: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;

    fn base_url(&self) -> Option<&str> {
        None
    }

    fn headers(&self) -> Option<&ProviderHeaders> {
        None
    }

    fn auth(&self) -> &ProviderAuth;

    /// Must not throw; callers treat failures as an empty catalog.
    fn get_models(&self) -> Vec<Model>;

    /// Whether the provider implements `refreshModels` (method presence in TS).
    fn supports_refresh_models(&self) -> bool {
        false
    }

    fn refresh_models(&self, _context: RefreshModelsContext) -> Option<BoxFuture<'static, Result<(), CallbackError>>> {
        None
    }

    fn filter_models(&self, models: Vec<Model>, _credential: Option<&Credential>) -> Vec<Model> {
        models
    }

    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream;

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream;

    fn supports_fetch_deferred(&self) -> bool {
        false
    }

    fn fetch_deferred(
        &self,
        model: Model,
        _handle: DeferredHandle,
        _options: DeferredFetchOptions,
    ) -> AssistantMessageEventStream {
        crate::request::error_stream(&model, "Provider does not support deferred responses")
    }

    fn supports_cancel_deferred(&self) -> bool {
        false
    }

    fn cancel_deferred(
        &self,
        _model: Model,
        _handle: DeferredHandle,
        _options: DeferredCancelOptions,
    ) -> BoxFuture<'static, Result<(), CallbackError>> {
        Box::pin(async { Err(CallbackError::new("Provider does not support deferred responses")) })
    }
}

/// Runtime collection of providers plus auth application and stream convenience.
pub trait Models: Send + Sync {
    fn get_providers(&self) -> Vec<Arc<dyn Provider>>;
    fn get_provider(&self, id: &str) -> Option<Arc<dyn Provider>>;
    fn get_models(&self, provider: Option<&str>) -> Vec<Model>;
    fn get_model(&self, provider: &str, id: &str) -> Option<Model>;

    fn refresh(&self, options: ModelsRefreshOptions) -> BoxFuture<'static, ModelsRefreshResult>;

    fn check_auth(
        &self,
        provider_id: &str,
        options: AuthOperationOptions,
    ) -> BoxFuture<'static, Result<Option<AuthCheck>, ResolveError>>;

    fn get_available(
        &self,
        provider_id: Option<&str>,
        options: AuthOperationOptions,
    ) -> BoxFuture<'static, Result<Vec<Model>, ResolveError>>;

    fn get_auth(
        &self,
        provider_id: &str,
        overrides: AuthResolutionOverrides,
    ) -> BoxFuture<'static, Result<Option<AuthResult>, ResolveError>>;

    fn get_auth_for_model(
        &self,
        model: &Model,
        overrides: AuthResolutionOverrides,
    ) -> BoxFuture<'static, Result<Option<AuthResult>, ResolveError>>;

    fn login(
        &self,
        provider_id: &str,
        auth_type: AuthType,
        interaction: Arc<dyn AuthInteraction>,
    ) -> BoxFuture<'static, Result<Credential, ResolveError>>;

    fn logout(&self, provider_id: &str, options: AuthOperationOptions) -> BoxFuture<'static, Result<(), ResolveError>>;

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

    fn fetch_deferred(
        &self,
        model: Model,
        handle: DeferredHandle,
        options: DeferredFetchOptions,
    ) -> BoxFuture<'static, Result<AssistantMessage, StreamFailure>>;

    fn cancel_deferred(
        &self,
        model: Model,
        handle: DeferredHandle,
        options: DeferredCancelOptions,
    ) -> BoxFuture<'static, Result<(), ResolveError>>;
}

pub trait MutableModels: Models {
    /// Upsert/replace by provider id.
    fn set_provider(&self, provider: Arc<dyn Provider>);
    fn delete_provider(&self, id: &str);
    fn clear_providers(&self);
}

#[derive(Default)]
pub struct CreateModelsOptions {
    pub credentials: Option<Arc<dyn CredentialStore>>,
    pub models_store: Option<Arc<dyn ModelsStore>>,
    pub auth_context: Option<Arc<dyn AuthContext>>,
}

/// `mergeHeaders`: override wins case-insensitively; `None` suppresses a header.
pub fn merge_headers(
    base: Option<ProviderHeaders>,
    override_headers: Option<ProviderHeaders>,
) -> Option<ProviderHeaders> {
    if base.is_none() && override_headers.is_none() {
        return None;
    }
    let mut merged = base.unwrap_or_default();
    for (name, value) in override_headers.unwrap_or_default() {
        let lower_name = name.to_lowercase();
        merged.retain(|existing, _| existing.to_lowercase() != lower_name);
        merged.insert(name, value);
    }
    Some(merged)
}

#[derive(Clone)]
struct Publisher {
    store: Arc<dyn ModelsStore>,
    generations: Arc<Mutex<HashMap<String, u64>>>,
    tails: Arc<Mutex<HashMap<String, Arc<Notify>>>>,
}

impl Publisher {
    fn publish(
        &self,
        provider_id: String,
        generation: u64,
        signal: CancellationToken,
        publication: ModelsPublication,
    ) -> BoxFuture<'static, Result<bool, CallbackError>> {
        let (previous, mine) = {
            let mine = Arc::new(Notify::new());
            let previous = lock(&self.tails).insert(provider_id.clone(), Arc::clone(&mine));
            (previous, mine)
        };
        let store = Arc::clone(&self.store);
        let generations = Arc::clone(&self.generations);
        let tails = Arc::clone(&self.tails);
        let task_signal = signal.clone();
        let task = tokio::spawn(async move {
            if let Some(previous) = previous {
                previous.notified().await;
            }
            let result = publish_queued(
                store,
                generations,
                provider_id.clone(),
                generation,
                task_signal,
                publication,
            )
            .await;
            mine.notify_one();
            let mut tails = lock(&tails);
            if tails.get(&provider_id).is_some_and(|tail| Arc::ptr_eq(tail, &mine)) {
                tails.remove(&provider_id);
            }
            result
        });
        Box::pin(async move {
            let result = race_with_abort_signal(
                async move { task.await.map_err(|error| CallbackError::new(error.to_string())) },
                &signal,
            )
            .await
            .map_err(|_| CallbackError::new("The operation was aborted"))???;
            Ok(result)
        })
    }
}

async fn publish_queued(
    store: Arc<dyn ModelsStore>,
    generations: Arc<Mutex<HashMap<String, u64>>>,
    provider_id: String,
    generation: u64,
    signal: CancellationToken,
    publication: ModelsPublication,
) -> Result<bool, CallbackError> {
    if signal.is_cancelled() || current_generation(&generations, &provider_id) != generation {
        return Ok(false);
    }
    let options = ModelsStoreOperationOptions {
        signal: Some(signal.clone()),
    };
    match publication.persist {
        PersistAction::Delete => store
            .delete(&provider_id, options)
            .await
            .map_err(|error| CallbackError::new(error.to_string()))?,
        PersistAction::Write(entry) => store
            .write(&provider_id, entry, options)
            .await
            .map_err(|error| CallbackError::new(error.to_string()))?,
        PersistAction::Unchanged => {}
    }
    if signal.is_cancelled() || current_generation(&generations, &provider_id) != generation {
        return Ok(false);
    }
    if let Some(update) = publication.update {
        update();
    }
    Ok(true)
}

fn current_generation(generations: &Mutex<HashMap<String, u64>>, provider_id: &str) -> u64 {
    lock(generations).get(provider_id).copied().unwrap_or(0)
}

struct Inner {
    providers: Mutex<Vec<(String, Arc<dyn Provider>)>>,
    credentials: Arc<dyn CredentialStore>,
    models_store: Arc<dyn ModelsStore>,
    auth_context: Arc<dyn AuthContext>,
    generations: Arc<Mutex<HashMap<String, u64>>>,
    controllers: Mutex<HashMap<String, CancellationToken>>,
    publisher: Publisher,
}

/// Concrete `MutableModels` returned by [`create_models`].
#[derive(Clone)]
pub struct ModelsCollection {
    inner: Arc<Inner>,
}

/// `createModels()`.
pub fn create_models(options: CreateModelsOptions) -> ModelsCollection {
    let generations = Arc::new(Mutex::new(HashMap::new()));
    let publisher = Publisher {
        store: options
            .models_store
            .clone()
            .unwrap_or_else(|| Arc::new(InMemoryModelsStore::new())),
        generations: Arc::clone(&generations),
        tails: Arc::new(Mutex::new(HashMap::new())),
    };
    ModelsCollection {
        inner: Arc::new(Inner {
            providers: Mutex::new(Vec::new()),
            credentials: options
                .credentials
                .unwrap_or_else(|| Arc::new(InMemoryCredentialStore::new())),
            models_store: options
                .models_store
                .unwrap_or_else(|| Arc::new(InMemoryModelsStore::new())),
            auth_context: options.auth_context.unwrap_or_else(default_provider_auth_context),
            generations,
            controllers: Mutex::new(HashMap::new()),
            publisher,
        }),
    }
}

impl ModelsCollection {
    fn provider_snapshot(&self) -> Vec<Arc<dyn Provider>> {
        lock(&self.inner.providers)
            .iter()
            .map(|(_, provider)| Arc::clone(provider))
            .collect()
    }

    fn provider(&self, id: &str) -> Option<Arc<dyn Provider>> {
        lock(&self.inner.providers)
            .iter()
            .find(|(provider_id, _)| provider_id == id)
            .map(|(_, provider)| Arc::clone(provider))
    }

    fn supersede_provider_refresh(&self, provider_id: &str) -> u64 {
        let generation = {
            let mut generations = lock(&self.inner.generations);
            let generation = generations.get(provider_id).copied().unwrap_or(0) + 1;
            generations.insert(provider_id.to_owned(), generation);
            generation
        };
        if let Some(previous) = lock(&self.inner.controllers).remove(provider_id) {
            previous.cancel();
        }
        generation
    }

    fn begin_provider_refresh(&self, provider_id: &str) -> (u64, CancellationToken) {
        let generation = self.supersede_provider_refresh(provider_id);
        let controller = CancellationToken::new();
        lock(&self.inner.controllers).insert(provider_id.to_owned(), controller.clone());
        (generation, controller)
    }

    fn finish_provider_refresh(&self, provider_id: &str, generation: u64) {
        if current_generation(&self.inner.generations, provider_id) == generation {
            lock(&self.inner.controllers).remove(provider_id);
        }
    }

    async fn read_credential(
        &self,
        provider_id: &str,
        signal: CancellationToken,
    ) -> Result<Option<Credential>, ResolveError> {
        match self
            .inner
            .credentials
            .read(provider_id, AuthOperationOptions { signal: Some(signal) })
            .await
        {
            Ok(credential) => Ok(credential),
            Err(AuthError::Aborted) => Err(ResolveError::Aborted),
            Err(AuthError::Failed(error)) => Err(ResolveError::with_cause(
                ModelsErrorCode::Auth,
                format!("Credential store read failed for {provider_id}"),
                Some(&error.message),
            )),
        }
    }

    async fn check_provider_auth(
        &self,
        provider: &Arc<dyn Provider>,
        credential: Option<&Credential>,
        signal: CancellationToken,
    ) -> Result<Option<AuthCheck>, ResolveError> {
        if let Some(Credential::OAuth(_)) = credential {
            return Ok(provider.auth().oauth.as_ref().map(|_| AuthCheck {
                source: Some("OAuth".into()),
                auth_type: AuthType::Oauth,
            }));
        }
        let Some(api_key) = provider.auth().api_key.as_ref() else {
            return Ok(None);
        };
        if let Some(check) = api_key.check(ApiKeyCheckInput {
            ctx: Arc::clone(&self.inner.auth_context),
            credential: credential.and_then(|credential| credential.as_api_key().cloned()),
            signal: signal.clone(),
        }) {
            return match check.await {
                Ok(result) => Ok(result),
                Err(AuthError::Aborted) => Err(ResolveError::Aborted),
                Err(AuthError::Failed(error)) => Err(ResolveError::with_cause(
                    ModelsErrorCode::Auth,
                    format!("API key auth check failed for provider {}", provider.id()),
                    Some(&error.message),
                )),
            };
        }
        let resolution = resolve_provider_auth(
            provider.id(),
            provider.auth(),
            Arc::clone(&self.inner.credentials),
            Arc::clone(&self.inner.auth_context),
            AuthResolutionOverrides {
                signal: Some(signal),
                ..AuthResolutionOverrides::default()
            },
        )
        .await?;
        Ok(resolution.map(|resolution| AuthCheck {
            source: resolution.source,
            auth_type: AuthType::ApiKey,
        }))
    }

    async fn get_auth_inner(
        &self,
        provider_id: &str,
        overrides: AuthResolutionOverrides,
    ) -> Result<Option<AuthResult>, ResolveError> {
        let signal = operation_signal(overrides.signal.as_ref());
        if signal.is_cancelled() {
            return Err(ResolveError::Aborted);
        }
        let Some(provider) = self.provider(provider_id) else {
            return Ok(None);
        };
        resolve_provider_auth(
            provider.id(),
            provider.auth(),
            Arc::clone(&self.inner.credentials),
            Arc::clone(&self.inner.auth_context),
            AuthResolutionOverrides {
                signal: Some(signal),
                ..overrides
            },
        )
        .await
    }

    async fn check_auth_inner(
        &self,
        provider_id: &str,
        signal: CancellationToken,
    ) -> Result<Option<AuthCheck>, ResolveError> {
        if signal.is_cancelled() {
            return Err(ResolveError::Aborted);
        }
        let Some(provider) = self.provider(provider_id) else {
            return Ok(None);
        };
        let credential = self.read_credential(provider_id, signal.clone()).await?;
        self.check_provider_auth(&provider, credential.as_ref(), signal).await
    }

    async fn apply_auth(&self, model: &Model, parts: AuthRequestParts) -> Result<AppliedAuth, ResolveError> {
        self.provider(&model.provider).ok_or_else(|| {
            ResolveError::models(
                ModelsErrorCode::Provider,
                format!("Unknown provider: {}", model.provider),
            )
        })?;
        let resolution = self
            .get_auth_for_model_inner(
                model,
                AuthResolutionOverrides {
                    api_key: parts.api_key.clone(),
                    env: parts.env.clone(),
                    signal: parts.signal.clone(),
                    ..AuthResolutionOverrides::default()
                },
            )
            .await?;
        let Some(resolution) = resolution else {
            return Err(ResolveError::models(
                ModelsErrorCode::Auth,
                format!("Provider is not configured: {}", model.provider),
            ));
        };
        let auth = resolution.auth;
        let api_key = parts.api_key.or(auth.api_key);
        let mut headers = merge_headers(auth.headers, parts.headers);
        if let Some(transform) = parts.transform_headers {
            let assembled = headers.take().unwrap_or_default();
            headers = Some(transform(assembled).await.map_err(ResolveError::Callback)?);
        }
        let env = if resolution.env.is_some() || parts.env.is_some() {
            let mut merged = resolution.env.unwrap_or_default();
            merged.extend(parts.env.unwrap_or_default());
            Some(merged)
        } else {
            None
        };
        let request_model = match auth.base_url {
            Some(base_url) => Model {
                base_url,
                ..model.clone()
            },
            None => model.clone(),
        };
        Ok(AppliedAuth {
            model: request_model,
            api_key,
            headers,
            env,
        })
    }

    async fn get_auth_for_model_inner(
        &self,
        model: &Model,
        overrides: AuthResolutionOverrides,
    ) -> Result<Option<AuthResult>, ResolveError> {
        let result = self.get_auth_inner(&model.provider, overrides).await?;
        let Some(mut result) = result else {
            return Ok(None);
        };
        let Some(model_headers) = &model.headers else {
            return Ok(Some(result));
        };
        let model_headers: ProviderHeaders = model_headers
            .iter()
            .map(|(name, value)| (name.clone(), Some(value.clone().into())))
            .collect();
        result.auth.headers = merge_headers(result.auth.headers, Some(model_headers));
        Ok(Some(result))
    }

    async fn resolve_refresh_credential(
        &self,
        provider: &Arc<dyn Provider>,
        stored: Option<Credential>,
        signal: CancellationToken,
    ) -> Result<Option<Credential>, CallbackError> {
        if let Some(Credential::OAuth(stored)) = stored {
            let Some(oauth) = provider.auth().oauth.as_ref() else {
                return Ok(None);
            };
            if (crate::request::now_millis() as f64) < stored.expires {
                return Ok(Some(Credential::OAuth(stored)));
            }
            if signal.is_cancelled() {
                return Ok(None);
            }
            let oauth = Arc::clone(oauth);
            let modify_signal = signal.clone();
            let modifier: crate::auth::types::CredentialModifier = Arc::new(move |current| {
                let oauth = Arc::clone(&oauth);
                let signal = signal.clone();
                Box::pin(async move {
                    let Some(Credential::OAuth(current)) = current else {
                        return Ok(None);
                    };
                    if (crate::request::now_millis() as f64) < current.expires {
                        return Ok(None);
                    }
                    let refreshed = oauth.refresh(current, signal).await.map_err(|error| match error {
                        AuthError::Aborted => AuthError::Aborted,
                        AuthError::Failed(error) => error.into(),
                    })?;
                    Ok(Some(Credential::OAuth(refreshed)))
                })
            });
            let post = self
                .inner
                .credentials
                .modify(
                    provider.id(),
                    modifier,
                    AuthOperationOptions {
                        signal: Some(modify_signal),
                    },
                )
                .await
                .map_err(|error| CallbackError::new(error.to_string()))?;
            return Ok(post.and_then(Credential::into_oauth).map(Credential::OAuth));
        }

        let Some(api_key) = provider.auth().api_key.as_ref() else {
            return Ok(None);
        };
        let credential = stored.and_then(|credential| match credential {
            Credential::ApiKey(credential) => Some(credential),
            Credential::OAuth(_) => None,
        });
        let result = api_key
            .resolve(ApiKeyResolveInput {
                ctx: Arc::clone(&self.inner.auth_context),
                credential,
                signal,
            })
            .await
            .map_err(|error| CallbackError::new(error.to_string()))?;
        Ok(result.map(|result| {
            Credential::ApiKey(ApiKeyCredential {
                key: result.auth.api_key,
                env: result.env,
            })
        }))
    }

    async fn run_provider_refresh_phase(
        &self,
        provider: &Arc<dyn Provider>,
        credential: Option<Credential>,
        allow_network: bool,
        force: Option<bool>,
        generation: u64,
        signal: CancellationToken,
    ) -> Result<(), CallbackError> {
        let stored = self
            .inner
            .models_store
            .read(
                provider.id(),
                ModelsStoreOperationOptions {
                    signal: Some(signal.clone()),
                },
            )
            .await
            .map_err(|error| CallbackError::new(error.to_string()))?;
        let Some(refresh_models) = provider.refresh_models(RefreshModelsContext {
            credential,
            stored,
            publish: self.publisher_closure(provider.id(), generation, signal.clone()),
            allow_network,
            force: if allow_network { force } else { None },
            signal,
        }) else {
            return Ok(());
        };
        refresh_models.await
    }

    fn publisher_closure(&self, provider_id: &str, generation: u64, signal: CancellationToken) -> PublishFn {
        let publisher = self.inner.publisher.clone();
        let provider_id = provider_id.to_owned();
        Arc::new(move |publication| publisher.publish(provider_id.clone(), generation, signal.clone(), publication))
    }

    async fn refresh_provider(
        &self,
        provider: Arc<dyn Provider>,
        allow_network: bool,
        force: Option<bool>,
        caller_signal: CancellationToken,
        errors: Arc<Mutex<IndexMap<String, CallbackError>>>,
    ) {
        let provider_id = provider.id().to_owned();
        let (generation, controller) = self.begin_provider_refresh(&provider_id);
        let signal = caller_signal.child_token();
        let watcher = {
            let signal = signal.clone();
            let controller = controller.clone();
            tokio::spawn(async move {
                controller.cancelled().await;
                signal.cancel();
            })
        };

        let operation = {
            let collection = self.clone();
            let provider = Arc::clone(&provider);
            let signal = signal.clone();
            async move {
                let mut credential_error = None;
                let stored_credential = match collection
                    .inner
                    .credentials
                    .read(
                        provider.id(),
                        AuthOperationOptions {
                            signal: Some(signal.clone()),
                        },
                    )
                    .await
                {
                    Ok(credential) => credential,
                    Err(error) => {
                        credential_error = Some(CallbackError::new(error.to_string()));
                        None
                    }
                };

                collection
                    .run_provider_refresh_phase(
                        &provider,
                        stored_credential.clone(),
                        false,
                        None,
                        generation,
                        signal.clone(),
                    )
                    .await?;
                if let Some(error) = credential_error {
                    return Err(error);
                }
                if !allow_network || signal.is_cancelled() {
                    return Ok(());
                }

                let credential = collection
                    .resolve_refresh_credential(&provider, stored_credential, signal.clone())
                    .await?;
                let Some(credential) = credential else {
                    return Ok(());
                };
                collection
                    .run_provider_refresh_phase(&provider, Some(credential), true, force, generation, signal)
                    .await
            }
        };

        let handle = tokio::spawn(operation);
        let outcome = race_with_abort_signal(
            async move { handle.await.map_err(|error| CallbackError::new(error.to_string())) },
            &signal,
        )
        .await;
        match outcome {
            Ok(Ok(Ok(()))) => {}
            Ok(Ok(Err(error))) => {
                if !signal.is_cancelled() {
                    lock(&errors).insert(provider_id.clone(), error);
                }
            }
            Ok(Err(error)) => {
                if !signal.is_cancelled() {
                    lock(&errors).insert(provider_id.clone(), error);
                }
            }
            Err(_) => {}
        }
        self.finish_provider_refresh(&provider_id, generation);
        watcher.abort();
    }

    async fn refresh_inner(&self, options: ModelsRefreshOptions) -> ModelsRefreshResult {
        let allow_network = options.allow_network.unwrap_or(true);
        let caller_signal = operation_signal(options.signal.as_ref());
        let errors = Arc::new(Mutex::new(IndexMap::new()));
        if caller_signal.is_cancelled() {
            return ModelsRefreshResult {
                aborted: true,
                errors: IndexMap::new(),
            };
        }
        let selected: Option<HashSet<String>> = options
            .providers
            .as_ref()
            .map(|providers| providers.iter().cloned().collect());
        let refreshable: Vec<Arc<dyn Provider>> = self
            .provider_snapshot()
            .into_iter()
            .filter(|provider| {
                provider.supports_refresh_models()
                    && selected
                        .as_ref()
                        .is_none_or(|selected| selected.contains(provider.id()))
            })
            .collect();

        let mut handles = Vec::with_capacity(refreshable.len());
        for provider in refreshable {
            let collection = self.clone();
            let caller = caller_signal.clone();
            let errors = Arc::clone(&errors);
            handles.push(tokio::spawn(async move {
                collection
                    .refresh_provider(provider, allow_network, options.force, caller, errors)
                    .await;
            }));
        }

        let wait_all = async {
            for handle in handles {
                let _ = handle.await;
            }
        };
        let _ = race_with_abort_signal(wait_all, &caller_signal).await;

        let errors = lock(&errors).clone();
        ModelsRefreshResult {
            aborted: caller_signal.is_cancelled(),
            errors,
        }
    }
}

struct AuthRequestParts {
    api_key: Option<pi_js_compat::js_json::JsString>,
    env: Option<crate::types::ProviderEnv>,
    headers: Option<ProviderHeaders>,
    transform_headers: Option<HeaderTransform>,
    signal: Option<CancellationToken>,
}

struct AppliedAuth {
    model: Model,
    api_key: Option<pi_js_compat::js_json::JsString>,
    headers: Option<ProviderHeaders>,
    env: Option<crate::types::ProviderEnv>,
}

fn resolve_error_to_callback(error: ResolveError) -> CallbackError {
    CallbackError::new(error.to_string())
}

impl Models for ModelsCollection {
    fn get_providers(&self) -> Vec<Arc<dyn Provider>> {
        self.provider_snapshot()
    }

    fn get_provider(&self, id: &str) -> Option<Arc<dyn Provider>> {
        self.provider(id)
    }

    fn get_models(&self, provider: Option<&str>) -> Vec<Model> {
        match provider {
            Some(provider_id) => self
                .provider(provider_id)
                .map(|provider| provider.get_models())
                .unwrap_or_default(),
            None => self
                .provider_snapshot()
                .into_iter()
                .flat_map(|provider| provider.get_models())
                .collect(),
        }
    }

    fn get_model(&self, provider: &str, id: &str) -> Option<Model> {
        self.get_models(Some(provider)).into_iter().find(|model| model.id == id)
    }

    fn refresh(&self, options: ModelsRefreshOptions) -> BoxFuture<'static, ModelsRefreshResult> {
        let collection = self.clone();
        Box::pin(async move { collection.refresh_inner(options).await })
    }

    fn check_auth(
        &self,
        provider_id: &str,
        options: AuthOperationOptions,
    ) -> BoxFuture<'static, Result<Option<AuthCheck>, ResolveError>> {
        let collection = self.clone();
        let provider_id = provider_id.to_owned();
        let signal = operation_signal(options.signal.as_ref());
        Box::pin(async move {
            let operation = collection.check_auth_inner(&provider_id, signal.clone());
            race_with_abort_signal(operation, &signal)
                .await
                .map_err(|_| ResolveError::Aborted)?
        })
    }

    fn get_available(
        &self,
        provider_id: Option<&str>,
        options: AuthOperationOptions,
    ) -> BoxFuture<'static, Result<Vec<Model>, ResolveError>> {
        let collection = self.clone();
        let provider_id = provider_id.map(str::to_owned);
        let signal = operation_signal(options.signal.as_ref());
        Box::pin(async move {
            let operation = async {
                if signal.is_cancelled() {
                    return Err(ResolveError::Aborted);
                }
                let providers: Vec<Arc<dyn Provider>> = match &provider_id {
                    Some(provider_id) => collection.provider(provider_id).into_iter().collect(),
                    None => collection.provider_snapshot(),
                };
                let mut available = Vec::new();
                for provider in providers {
                    let credential = collection.read_credential(provider.id(), signal.clone()).await?;
                    let auth = collection
                        .check_provider_auth(&provider, credential.as_ref(), signal.clone())
                        .await?;
                    if auth.is_none() {
                        continue;
                    }
                    let models = provider.get_models();
                    available.extend(provider.filter_models(models, credential.as_ref()));
                }
                Ok(available)
            };
            race_with_abort_signal(operation, &signal)
                .await
                .map_err(|_| ResolveError::Aborted)?
        })
    }

    fn get_auth(
        &self,
        provider_id: &str,
        overrides: AuthResolutionOverrides,
    ) -> BoxFuture<'static, Result<Option<AuthResult>, ResolveError>> {
        let collection = self.clone();
        let provider_id = provider_id.to_owned();
        let signal = operation_signal(overrides.signal.as_ref());
        Box::pin(async move {
            let operation = collection.get_auth_inner(
                &provider_id,
                AuthResolutionOverrides {
                    signal: Some(signal.clone()),
                    ..overrides
                },
            );
            race_with_abort_signal(operation, &signal)
                .await
                .map_err(|_| ResolveError::Aborted)?
        })
    }

    fn get_auth_for_model(
        &self,
        model: &Model,
        overrides: AuthResolutionOverrides,
    ) -> BoxFuture<'static, Result<Option<AuthResult>, ResolveError>> {
        let collection = self.clone();
        let model = model.clone();
        let signal = operation_signal(overrides.signal.as_ref());
        Box::pin(async move {
            let operation = collection.get_auth_for_model_inner(
                &model,
                AuthResolutionOverrides {
                    signal: Some(signal.clone()),
                    ..overrides
                },
            );
            race_with_abort_signal(operation, &signal)
                .await
                .map_err(|_| ResolveError::Aborted)?
        })
    }

    fn login(
        &self,
        provider_id: &str,
        auth_type: AuthType,
        interaction: Arc<dyn AuthInteraction>,
    ) -> BoxFuture<'static, Result<Credential, ResolveError>> {
        let collection = self.clone();
        let provider_id = provider_id.to_owned();
        let signal = interaction.signal();
        Box::pin(async move {
            let operation = async {
                if signal.is_cancelled() {
                    return Err(ResolveError::Aborted);
                }
                let Some(provider) = collection.provider(&provider_id) else {
                    return Err(ResolveError::models(
                        ModelsErrorCode::Provider,
                        format!("Unknown provider: {provider_id}"),
                    ));
                };
                let login: Option<BoxFuture<'static, Result<Credential, AuthError>>> = match auth_type {
                    AuthType::Oauth => provider.auth().oauth.as_ref().map(|oauth| {
                        let login = oauth.login(Arc::clone(&interaction));
                        Box::pin(async move { login.await.map(Credential::OAuth) })
                            as BoxFuture<'static, Result<Credential, AuthError>>
                    }),
                    AuthType::ApiKey => provider.auth().api_key.as_ref().and_then(|api_key| {
                        api_key.login(Arc::clone(&interaction)).map(|login| {
                            Box::pin(async move { login.await.map(Credential::ApiKey) })
                                as BoxFuture<'static, Result<Credential, AuthError>>
                        })
                    }),
                };
                let Some(login) = login else {
                    return Err(ResolveError::models(
                        ModelsErrorCode::Auth,
                        format!("{} does not support {} login", provider.name(), auth_type.as_str()),
                    ));
                };
                let credential = race_with_abort_signal(login, &signal)
                    .await
                    .map_err(|_| ResolveError::Aborted)?
                    .map_err(|error| match error {
                        AuthError::Aborted => ResolveError::Aborted,
                        AuthError::Failed(error) => ResolveError::Callback(error),
                    })?;

                let mutation_started = Arc::new(std::sync::atomic::AtomicBool::new(false));
                let started = Arc::new(Notify::new());
                let modifier: crate::auth::types::CredentialModifier = {
                    let credential = credential.clone();
                    let mutation_started = Arc::clone(&mutation_started);
                    let started = Arc::clone(&started);
                    Arc::new(move |_current| {
                        mutation_started.store(true, std::sync::atomic::Ordering::SeqCst);
                        started.notify_one();
                        let credential = credential.clone();
                        Box::pin(async move { Ok(Some(credential)) })
                    })
                };
                let mut mutation = collection.inner.credentials.modify(
                    &provider_id,
                    modifier,
                    AuthOperationOptions {
                        signal: Some(signal.clone()),
                    },
                );
                let mutation_result = tokio::select! {
                    _ = started.notified() => mutation.await,
                    result = &mut mutation => result,
                    _ = signal.cancelled() => {
                        if !mutation_started.load(std::sync::atomic::Ordering::SeqCst) {
                            return Err(ResolveError::Aborted);
                        }
                        mutation.await
                    }
                };
                mutation_result.map_err(|error| {
                    if signal.is_cancelled() {
                        ResolveError::Aborted
                    } else {
                        ResolveError::with_cause(
                            ModelsErrorCode::Auth,
                            format!("Credential store modify failed for {provider_id}"),
                            error.detail(),
                        )
                    }
                })?;
                Ok(credential)
            };
            operation.await
        })
    }

    fn logout(&self, provider_id: &str, options: AuthOperationOptions) -> BoxFuture<'static, Result<(), ResolveError>> {
        let collection = self.clone();
        let provider_id = provider_id.to_owned();
        let signal = operation_signal(options.signal.as_ref());
        Box::pin(async move {
            if signal.is_cancelled() {
                return Err(ResolveError::Aborted);
            }
            match collection
                .inner
                .credentials
                .delete(
                    &provider_id,
                    AuthOperationOptions {
                        signal: Some(signal.clone()),
                    },
                )
                .await
            {
                Ok(()) => Ok(()),
                Err(error) if signal.is_cancelled() => {
                    let _ = error;
                    Err(ResolveError::Aborted)
                }
                Err(error) => Err(ResolveError::with_cause(
                    ModelsErrorCode::Auth,
                    format!("Credential store delete failed for {provider_id}"),
                    error.detail(),
                )),
            }
        })
    }

    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        let collection = self.clone();
        lazy_stream(model.clone(), move || {
            Box::pin(async move {
                let provider = collection
                    .provider(&model.provider)
                    .ok_or_else(|| CallbackError::new(format!("Unknown provider: {}", model.provider)))?;
                let mut options = options;
                let transform_headers = options.request.transform_headers.take();
                let applied = collection
                    .apply_auth(
                        &model,
                        AuthRequestParts {
                            api_key: options.request.api_key.clone(),
                            env: options.request.env.clone(),
                            headers: options.request.headers.clone(),
                            transform_headers,
                            signal: options.request.signal.clone(),
                        },
                    )
                    .await
                    .map_err(resolve_error_to_callback)?;
                options.request.api_key = applied.api_key;
                options.request.headers = applied.headers;
                options.request.env = applied.env;
                Ok(provider.stream(applied.model, context, options))
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
                    .provider(&model.provider)
                    .ok_or_else(|| CallbackError::new(format!("Unknown provider: {}", model.provider)))?;
                let mut options = options;
                let transform_headers = options.stream.request.transform_headers.take();
                let applied = collection
                    .apply_auth(
                        &model,
                        AuthRequestParts {
                            api_key: options.stream.request.api_key.clone(),
                            env: options.stream.request.env.clone(),
                            headers: options.stream.request.headers.clone(),
                            transform_headers,
                            signal: options.stream.request.signal.clone(),
                        },
                    )
                    .await
                    .map_err(resolve_error_to_callback)?;
                options.stream.request.api_key = applied.api_key;
                options.stream.request.headers = applied.headers;
                options.stream.request.env = applied.env;
                Ok(provider.stream_simple(applied.model, context, options))
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

    fn fetch_deferred(
        &self,
        model: Model,
        handle: DeferredHandle,
        options: DeferredFetchOptions,
    ) -> BoxFuture<'static, Result<AssistantMessage, StreamFailure>> {
        let collection = self.clone();
        let stream = lazy_stream(model.clone(), move || {
            Box::pin(async move {
                let provider = collection
                    .provider(&model.provider)
                    .ok_or_else(|| CallbackError::new(format!("Unknown provider: {}", model.provider)))?;
                if !provider.supports_fetch_deferred() {
                    return Err(CallbackError::new(format!(
                        "Provider {} does not support deferred responses",
                        model.provider
                    )));
                }
                let mut options = options;
                let transform_headers = options.request.transform_headers.take();
                let applied = collection
                    .apply_auth(
                        &model,
                        AuthRequestParts {
                            api_key: options.request.api_key.clone(),
                            env: options.request.env.clone(),
                            headers: options.request.headers.clone(),
                            transform_headers,
                            signal: options.request.signal.clone(),
                        },
                    )
                    .await
                    .map_err(resolve_error_to_callback)?;
                options.request.api_key = applied.api_key;
                options.request.headers = applied.headers;
                options.request.env = applied.env;
                Ok(provider.fetch_deferred(applied.model, handle, options))
            })
        });
        Box::pin(async move { stream.result().await })
    }

    fn cancel_deferred(
        &self,
        model: Model,
        handle: DeferredHandle,
        options: DeferredCancelOptions,
    ) -> BoxFuture<'static, Result<(), ResolveError>> {
        let collection = self.clone();
        Box::pin(async move {
            let provider = collection.provider(&model.provider).ok_or_else(|| {
                ResolveError::models(
                    ModelsErrorCode::Provider,
                    format!("Unknown provider: {}", model.provider),
                )
            })?;
            if !provider.supports_cancel_deferred() {
                return Err(ResolveError::models(
                    ModelsErrorCode::Provider,
                    format!("Provider {} does not support deferred responses", model.provider),
                ));
            }
            let mut options = options;
            let transform_headers = options.transform_headers.take();
            let applied = collection
                .apply_auth(
                    &model,
                    AuthRequestParts {
                        api_key: options.api_key.clone(),
                        env: options.env.clone(),
                        headers: options.headers.clone(),
                        transform_headers,
                        signal: options.signal.clone(),
                    },
                )
                .await?;
            options.api_key = applied.api_key;
            options.headers = applied.headers;
            options.env = applied.env;
            provider
                .cancel_deferred(applied.model, handle, options)
                .await
                .map_err(ResolveError::Callback)
        })
    }
}

impl MutableModels for ModelsCollection {
    fn set_provider(&self, provider: Arc<dyn Provider>) {
        self.supersede_provider_refresh(provider.id());
        let mut providers = lock(&self.inner.providers);
        match providers
            .iter_mut()
            .find(|(provider_id, _)| provider_id == provider.id())
        {
            Some(entry) => entry.1 = provider,
            None => providers.push((provider.id().to_owned(), provider)),
        }
    }

    fn delete_provider(&self, id: &str) {
        self.supersede_provider_refresh(id);
        lock(&self.inner.providers).retain(|(provider_id, _)| provider_id != id);
    }

    fn clear_providers(&self) {
        let mut ids: HashSet<String> = lock(&self.inner.providers).iter().map(|(id, _)| id.clone()).collect();
        ids.extend(lock(&self.inner.controllers).keys().cloned());
        for id in ids {
            self.supersede_provider_refresh(&id);
        }
        lock(&self.inner.providers).clear();
    }
}

// --- createProvider -------------------------------------------------------

pub type FetchModelsFn =
    Arc<dyn Fn(RefreshModelsContext) -> BoxFuture<'static, Result<Vec<Model>, CallbackError>> + Send + Sync>;
pub type FilterModelsFn = Arc<dyn Fn(Vec<Model>, Option<&Credential>) -> Vec<Model> + Send + Sync>;

/// Single implementation or per-`model.api` map.
#[derive(Clone)]
pub enum ProviderApi {
    Single(Arc<dyn ProviderStreams>),
    Map(IndexMap<String, Arc<dyn ProviderStreams>>),
}

pub struct CreateProviderOptions {
    pub id: String,
    /// Default: `id`.
    pub name: Option<String>,
    pub base_url: Option<String>,
    pub headers: Option<ProviderHeaders>,
    pub auth: ProviderAuth,
    /// Static baseline model list.
    pub models: Vec<Model>,
    /// Fetch a dynamic model overlay.
    pub fetch_models: Option<FetchModelsFn>,
    pub filter_models: Option<FilterModelsFn>,
    pub api: ProviderApi,
}

struct ConfiguredProvider {
    id: String,
    name: String,
    base_url: Option<String>,
    headers: Option<ProviderHeaders>,
    auth: ProviderAuth,
    baseline_models: Vec<Model>,
    dynamic_models: Arc<Mutex<Vec<Model>>>,
    fetch_models: Option<FetchModelsFn>,
    filter_models: Option<FilterModelsFn>,
    api: ProviderApi,
}

impl ConfiguredProvider {
    fn current_models(&self) -> Vec<Model> {
        let mut merged = self.baseline_models.clone();
        for model in lock(&self.dynamic_models).iter() {
            match merged.iter_mut().find(|entry| entry.id == model.id) {
                Some(entry) => *entry = model.clone(),
                None => merged.push(model.clone()),
            }
        }
        merged
    }

    fn api_for(&self, model: &Model) -> Option<Arc<dyn ProviderStreams>> {
        match &self.api {
            ProviderApi::Single(streams) => Some(Arc::clone(streams)),
            ProviderApi::Map(map) => map.get(&model.api).map(Arc::clone),
        }
    }

    fn dispatch(
        &self,
        model: Model,
        run: impl FnOnce(&Arc<dyn ProviderStreams>) -> AssistantMessageEventStream,
    ) -> AssistantMessageEventStream {
        match self.api_for(&model) {
            Some(streams) => run(&streams),
            None => {
                let provider_id = self.id.clone();
                let message = format!("Provider {provider_id} has no API implementation for \"{}\"", model.api);
                lazy_stream(model, move || Box::pin(async move { Err(CallbackError::new(message)) }))
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

    fn base_url(&self) -> Option<&str> {
        self.base_url.as_deref()
    }

    fn headers(&self) -> Option<&ProviderHeaders> {
        self.headers.as_ref()
    }

    fn auth(&self) -> &ProviderAuth {
        &self.auth
    }

    fn get_models(&self) -> Vec<Model> {
        self.current_models()
    }

    fn supports_refresh_models(&self) -> bool {
        self.fetch_models.is_some()
    }

    fn refresh_models(&self, context: RefreshModelsContext) -> Option<BoxFuture<'static, Result<(), CallbackError>>> {
        let fetch_models = Arc::clone(self.fetch_models.as_ref()?);
        let provider_id = self.id.clone();
        let dynamic_models = Arc::clone(&self.dynamic_models);
        Some(Box::pin(async move {
            if let Some(stored) = &context.stored {
                let restored: Vec<Model> = stored
                    .models
                    .iter()
                    .filter(|model| model.provider == provider_id)
                    .cloned()
                    .collect();
                let published = (context.publish)(ModelsPublication::new(PersistAction::Unchanged).with_update({
                    let dynamic_models = Arc::clone(&dynamic_models);
                    move || *lock(&dynamic_models) = restored.clone()
                }))
                .await?;
                if !published {
                    return Ok(());
                }
            }
            if !context.allow_network || context.signal.is_cancelled() {
                return Ok(());
            }
            let refreshed = fetch_models(context.clone()).await?;
            if context.signal.is_cancelled() {
                return Ok(());
            }
            (context.publish)(
                ModelsPublication::new(PersistAction::Write(ModelsStoreEntry {
                    models: refreshed.clone(),
                    last_modified: None,
                    checked_at: Some(crate::request::now_millis() as f64),
                    etag: None,
                }))
                .with_update({
                    let dynamic_models = Arc::clone(&dynamic_models);
                    move || *lock(&dynamic_models) = refreshed.clone()
                }),
            )
            .await?;
            Ok(())
        }))
    }

    fn filter_models(&self, models: Vec<Model>, credential: Option<&Credential>) -> Vec<Model> {
        match &self.filter_models {
            Some(filter) => filter(models, credential),
            None => models,
        }
    }

    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        let request_model = model.clone();
        self.dispatch(model, move |streams| streams.stream(request_model, context, options))
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        let request_model = model.clone();
        self.dispatch(model, move |streams| {
            streams.stream_simple(request_model, context, options)
        })
    }

    fn supports_fetch_deferred(&self) -> bool {
        self.api
            .streams()
            .iter()
            .any(|streams| streams.supports_fetch_deferred())
    }

    fn fetch_deferred(
        &self,
        model: Model,
        handle: DeferredHandle,
        options: DeferredFetchOptions,
    ) -> AssistantMessageEventStream {
        let provider_id = self.id.clone();
        let api = self.api.clone();
        lazy_stream(model.clone(), move || {
            Box::pin(async move {
                let streams = match &api {
                    ProviderApi::Single(streams) => Some(Arc::clone(streams)),
                    ProviderApi::Map(map) => map.get(&model.api).map(Arc::clone),
                };
                let Some(streams) = streams.filter(|streams| streams.supports_fetch_deferred()) else {
                    return Err(CallbackError::new(format!(
                        "Provider {provider_id} does not support deferred responses for \"{}\"",
                        model.api
                    )));
                };
                Ok(streams.fetch_deferred(model, handle, options))
            })
        })
    }

    fn supports_cancel_deferred(&self) -> bool {
        self.api
            .streams()
            .iter()
            .any(|streams| streams.supports_cancel_deferred())
    }

    fn cancel_deferred(
        &self,
        model: Model,
        handle: DeferredHandle,
        options: DeferredCancelOptions,
    ) -> BoxFuture<'static, Result<(), CallbackError>> {
        let provider_id = self.id.clone();
        let api = self.api.clone();
        Box::pin(async move {
            let streams = match &api {
                ProviderApi::Single(streams) => Some(Arc::clone(streams)),
                ProviderApi::Map(map) => map.get(&model.api).map(Arc::clone),
            };
            let Some(streams) = streams.filter(|streams| streams.supports_cancel_deferred()) else {
                return Err(CallbackError::new(format!(
                    "Provider {provider_id} cannot cancel deferred responses for \"{}\"",
                    model.api
                )));
            };
            streams.cancel_deferred(model, handle, options).await
        })
    }
}

impl ProviderApi {
    fn streams(&self) -> Vec<Arc<dyn ProviderStreams>> {
        match self {
            Self::Single(streams) => vec![Arc::clone(streams)],
            Self::Map(map) => map.values().map(Arc::clone).collect(),
        }
    }
}

/// `createProvider()`.
pub fn create_provider(input: CreateProviderOptions) -> Arc<dyn Provider> {
    Arc::new(ConfiguredProvider {
        name: input.name.unwrap_or_else(|| input.id.clone()),
        id: input.id,
        base_url: input.base_url,
        headers: input.headers,
        auth: input.auth,
        baseline_models: input.models,
        dynamic_models: Arc::new(Mutex::new(Vec::new())),
        fetch_models: input.fetch_models,
        filter_models: input.filter_models,
        api: input.api,
    })
}

/// Runtime-checked `model.api` comparison.
pub fn has_api(model: &Model, api: &str) -> bool {
    model.api == api
}

/// `calculateCost()`; pure variant of the mutating TypeScript function.
pub fn calculate_cost(model: &Model, usage: &Usage) -> UsageCost {
    let input_tokens = usage.input + usage.cache_read + usage.cache_write;
    let mut rates: &ModelCostRates = &model.cost.rates;
    let mut matched_threshold = -1.0;
    for tier in model.cost.tiers.iter().flatten() {
        if input_tokens > tier.input_tokens_above && tier.input_tokens_above > matched_threshold {
            rates = &tier.rates;
            matched_threshold = tier.input_tokens_above;
        }
    }

    let long_write = usage.cache_write_1h.unwrap_or(0.0);
    let short_write = usage.cache_write - long_write;
    let input = (rates.input / 1_000_000.0) * usage.input;
    let output = (rates.output / 1_000_000.0) * usage.output;
    let cache_read = (rates.cache_read / 1_000_000.0) * usage.cache_read;
    let cache_write = (rates.cache_write * short_write + rates.input * 2.0 * long_write) / 1_000_000.0;
    UsageCost {
        input,
        output,
        cache_read,
        cache_write,
        total: input + output + cache_read + cache_write,
    }
}

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
    let Some(requested_index) = EXTENDED_THINKING_LEVELS.iter().position(|entry| *entry == level) else {
        return available.first().copied().unwrap_or(ModelThinkingLevel::Off);
    };
    for candidate in &EXTENDED_THINKING_LEVELS[requested_index..] {
        if available.contains(candidate) {
            return *candidate;
        }
    }
    for candidate in EXTENDED_THINKING_LEVELS[..requested_index].iter().rev() {
        if available.contains(candidate) {
            return *candidate;
        }
    }
    available.first().copied().unwrap_or(ModelThinkingLevel::Off)
}

/// Models are equal when both id and provider match.
pub fn models_are_equal(a: Option<&Model>, b: Option<&Model>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.id == b.id && a.provider == b.provider,
        _ => false,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
