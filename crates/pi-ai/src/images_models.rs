//! Port of `packages/ai/src/images-models.ts`.

use std::sync::{Arc, Mutex};

use tokio::sync::{Mutex as AsyncMutex, Notify};
use tokio::task::JoinSet;

use crate::auth::{
    AuthContext, AuthModel, AuthResolutionOverrides, AuthResult, CredentialStore, ModelsError, ModelsErrorCode,
    ProviderAuth, default_provider_auth_context, resolve_provider_auth,
};
use crate::models::CreateModelsOptions;
use crate::request::{BoxFuture, CallbackError, ImagesOptions, ProviderImages, now_millis};
use crate::types::{AssistantImages, ImagesContext, ImagesModel, ImagesStopReason, ProviderEnv, ProviderHeaders};

/// An image-generation provider: the image-side counterpart of `Provider`.
pub trait ImagesProvider: Send + Sync {
    fn id(&self) -> &str;
    fn name(&self) -> &str;
    fn auth(&self) -> &ProviderAuth;

    /// Must not throw; callers treat failures as an empty catalog.
    fn get_models(&self) -> Vec<ImagesModel>;

    /// Whether the provider implements `refreshModels` (method presence in TS).
    fn supports_refresh_models(&self) -> bool {
        false
    }

    fn refresh_models(&self) -> Option<BoxFuture<'static, Result<(), CallbackError>>> {
        None
    }

    fn generate_images(
        &self,
        model: ImagesModel,
        context: ImagesContext,
        options: ImagesOptions,
    ) -> BoxFuture<'static, Result<AssistantImages, CallbackError>>;
}

/// Runtime collection of image-generation providers plus auth application and
/// generation convenience: the image-side counterpart of `Models`.
pub trait ImagesModels: Send + Sync {
    fn get_providers(&self) -> Vec<Arc<dyn ImagesProvider>>;
    fn get_provider(&self, id: &str) -> Option<Arc<dyn ImagesProvider>>;
    fn get_models(&self, provider: Option<&str>) -> Vec<ImagesModel>;
    fn get_model(&self, provider: &str, id: &str) -> Option<ImagesModel>;
    fn refresh(&self, provider: Option<&str>) -> BoxFuture<'static, Result<(), ModelsError>>;
    fn get_auth(&self, model: &ImagesModel) -> BoxFuture<'static, Result<Option<AuthResult>, ModelsError>>;
    fn generate_images(
        &self,
        model: ImagesModel,
        context: ImagesContext,
        options: ImagesOptions,
    ) -> BoxFuture<'static, AssistantImages>;
}

pub trait MutableImagesModels: ImagesModels {
    /// Upsert/replace by provider id.
    fn set_provider(&self, provider: Arc<dyn ImagesProvider>);
    fn delete_provider(&self, id: &str);
    fn clear_providers(&self);
}

struct Inner {
    providers: Mutex<Vec<(String, Arc<dyn ImagesProvider>)>>,
    credentials: Arc<dyn CredentialStore>,
    auth_context: Arc<dyn AuthContext>,
}

/// Concrete `MutableImagesModels` returned by [`create_images_models`].
#[derive(Clone)]
pub struct ImagesModelsCollection {
    inner: Arc<Inner>,
}

/// `createImagesModels()`.
pub fn create_images_models(options: CreateModelsOptions) -> ImagesModelsCollection {
    ImagesModelsCollection {
        inner: Arc::new(Inner {
            providers: Mutex::new(Vec::new()),
            credentials: options
                .credentials
                .unwrap_or_else(|| Arc::new(crate::auth::InMemoryCredentialStore::default())),
            auth_context: options.auth_context.unwrap_or_else(default_provider_auth_context),
        }),
    }
}

impl Inner {
    fn provider(&self, id: &str) -> Option<Arc<dyn ImagesProvider>> {
        lock(&self.providers)
            .iter()
            .find(|(provider_id, _)| provider_id == id)
            .map(|(_, provider)| Arc::clone(provider))
    }

    fn providers(&self) -> Vec<Arc<dyn ImagesProvider>> {
        lock(&self.providers)
            .iter()
            .map(|(_, provider)| Arc::clone(provider))
            .collect()
    }
}

impl ImagesModels for ImagesModelsCollection {
    fn get_providers(&self) -> Vec<Arc<dyn ImagesProvider>> {
        self.inner.providers()
    }

    fn get_provider(&self, id: &str) -> Option<Arc<dyn ImagesProvider>> {
        self.inner.provider(id)
    }

    fn get_models(&self, provider: Option<&str>) -> Vec<ImagesModel> {
        match provider {
            Some(id) => self
                .inner
                .provider(id)
                .map_or_else(Vec::new, |provider| provider.get_models()),
            None => self
                .inner
                .providers()
                .into_iter()
                .flat_map(|provider| provider.get_models())
                .collect(),
        }
    }

    fn get_model(&self, provider: &str, id: &str) -> Option<ImagesModel> {
        self.get_models(Some(provider)).into_iter().find(|model| model.id == id)
    }

    fn refresh(&self, provider: Option<&str>) -> BoxFuture<'static, Result<(), ModelsError>> {
        match provider {
            Some(id) => {
                let provider = self.inner.provider(id);
                let id = id.to_owned();
                Box::pin(async move {
                    let Some(provider) = provider else {
                        return Ok(());
                    };
                    let Some(refresh) = provider.refresh_models() else {
                        return Ok(());
                    };
                    refresh.await.map_err(|error| {
                        ModelsError::with_cause(
                            ModelsErrorCode::ModelSource,
                            format!("Model refresh failed for {id}"),
                            Some(Box::new(error.thrown_value())),
                        )
                    })
                })
            }
            None => {
                let providers = self.inner.providers();
                Box::pin(async move {
                    refresh_all(providers).await;
                    Ok(())
                })
            }
        }
    }

    fn get_auth(&self, model: &ImagesModel) -> BoxFuture<'static, Result<Option<AuthResult>, ModelsError>> {
        let provider = self.inner.provider(&model.provider);
        let model = model.clone();
        let credentials = Arc::clone(&self.inner.credentials);
        let context = Arc::clone(&self.inner.auth_context);
        Box::pin(async move {
            let Some(provider) = provider else {
                return Ok(None);
            };
            resolve_provider_auth(
                provider.id(),
                provider.auth(),
                AuthModel::Images(model),
                credentials,
                context,
                AuthResolutionOverrides::default(),
            )
            .await
        })
    }

    fn generate_images(
        &self,
        model: ImagesModel,
        context: ImagesContext,
        options: ImagesOptions,
    ) -> BoxFuture<'static, AssistantImages> {
        let provider = self.inner.provider(&model.provider);
        let credentials = Arc::clone(&self.inner.credentials);
        let auth_context = Arc::clone(&self.inner.auth_context);
        Box::pin(async move {
            let Some(provider) = provider else {
                return unknown_provider_result(&model);
            };
            let resolution = resolve_auth_for_request(&provider, &model, credentials, auth_context, &options).await;
            let resolution = match resolution {
                Ok(resolution) => resolution,
                Err(error) => return error_result(&model, error.message),
            };
            let (request_model, request_options) = request_with_resolution(&model, &options, resolution);
            match provider.generate_images(request_model, context, request_options).await {
                Ok(result) => result,
                Err(error) => error_result(&model, error.message),
            }
        })
    }
}

fn unknown_provider_result(model: &ImagesModel) -> AssistantImages {
    error_result(
        model,
        ModelsError::new(
            ModelsErrorCode::Provider,
            format!("Unknown provider: {}", model.provider),
        )
        .to_string(),
    )
}

async fn resolve_auth_for_request(
    provider: &Arc<dyn ImagesProvider>,
    model: &ImagesModel,
    credentials: Arc<dyn CredentialStore>,
    auth_context: Arc<dyn AuthContext>,
    options: &ImagesOptions,
) -> Result<Option<AuthResult>, ModelsError> {
    resolve_provider_auth(
        provider.id(),
        provider.auth(),
        AuthModel::Images(model.clone()),
        credentials,
        auth_context,
        AuthResolutionOverrides {
            api_key: options.request.api_key.clone(),
            env: options.request.env.clone(),
        },
    )
    .await
}

fn request_with_resolution(
    model: &ImagesModel,
    options: &ImagesOptions,
    resolution: Option<AuthResult>,
) -> (ImagesModel, ImagesOptions) {
    let mut request_model = model.clone();
    let mut request_options = options.clone();
    if let Some(resolution) = resolution {
        apply_resolution(&mut request_model, &mut request_options, resolution);
    }
    (request_model, request_options)
}

impl MutableImagesModels for ImagesModelsCollection {
    fn set_provider(&self, provider: Arc<dyn ImagesProvider>) {
        let mut providers = lock(&self.inner.providers);
        if let Some(entry) = providers.iter_mut().find(|(id, _)| id == provider.id()) {
            entry.1 = provider;
        } else {
            providers.push((provider.id().to_owned(), provider));
        }
    }

    fn delete_provider(&self, id: &str) {
        lock(&self.inner.providers).retain(|(provider_id, _)| provider_id != id);
    }

    fn clear_providers(&self) {
        lock(&self.inner.providers).clear();
    }
}

async fn refresh_all(providers: Vec<Arc<dyn ImagesProvider>>) {
    let mut tasks = JoinSet::new();
    for provider in providers {
        if !provider.supports_refresh_models() {
            continue;
        }
        tasks.spawn(async move {
            if let Some(refresh) = provider.refresh_models() {
                let _ = refresh.await;
            }
        });
    }
    while tasks.join_next().await.is_some() {}
}

fn apply_resolution(request_model: &mut ImagesModel, request_options: &mut ImagesOptions, resolution: AuthResult) {
    let AuthResult { auth, env, .. } = resolution;
    if let Some(base_url) = auth.base_url {
        request_model.base_url = base_url;
    }
    request_options.request.api_key = request_options.request.api_key.clone().or(auth.api_key);
    request_options.request.headers = merge_headers(auth.headers, request_options.request.headers.clone());
    request_options.request.env = merge_env(env, request_options.request.env.clone());
}

fn merge_headers(
    auth_headers: Option<ProviderHeaders>,
    option_headers: Option<ProviderHeaders>,
) -> Option<ProviderHeaders> {
    match (auth_headers, option_headers) {
        (None, None) => None,
        (auth_headers, option_headers) => {
            let mut merged = auth_headers.unwrap_or_default();
            merged.extend(option_headers.unwrap_or_default());
            Some(merged)
        }
    }
}

fn merge_env(resolution_env: Option<ProviderEnv>, option_env: Option<ProviderEnv>) -> Option<ProviderEnv> {
    match (resolution_env, option_env) {
        (None, None) => None,
        (resolution_env, option_env) => {
            let mut merged: ProviderEnv = resolution_env.unwrap_or_default();
            merged.extend(option_env.unwrap_or_default());
            Some(merged)
        }
    }
}

fn error_result(model: &ImagesModel, message: impl Into<pi_js_compat::js_json::JsString>) -> AssistantImages {
    AssistantImages {
        api: model.api.clone(),
        provider: model.provider.clone(),
        model: model.id.clone(),
        output: Vec::new(),
        response_id: None,
        usage: None,
        stop_reason: ImagesStopReason::Error,
        error_message: Some(message.into()),
        timestamp: now_millis() as f64,
    }
}

/// `refreshModels` callback for dynamic providers.
pub type ImagesRefreshModels =
    Arc<dyn Fn() -> BoxFuture<'static, Result<Vec<ImagesModel>, CallbackError>> + Send + Sync>;

pub struct CreateImagesProviderOptions {
    pub id: String,
    /// Display name. Default: `id`.
    pub name: Option<String>,
    pub auth: ProviderAuth,
    pub models: Vec<ImagesModel>,
    pub refresh_models: Option<ImagesRefreshModels>,
    pub api: Arc<dyn ProviderImages>,
}

struct ConfiguredImagesProvider {
    id: String,
    name: String,
    auth: ProviderAuth,
    models: Arc<Mutex<Vec<ImagesModel>>>,
    refresh_models: Option<ImagesRefreshModels>,
    inflight: Arc<AsyncMutex<Option<Arc<RefreshState>>>>,
    api: Arc<dyn ProviderImages>,
}

#[derive(Default)]
struct RefreshState {
    done: Notify,
    outcome: Mutex<Option<Result<(), CallbackError>>>,
}

impl RefreshState {
    async fn wait(&self) -> Result<(), CallbackError> {
        loop {
            let notified = self.done.notified();
            if let Some(outcome) = lock(&self.outcome).clone() {
                return outcome;
            }
            notified.await;
        }
    }

    fn finish(&self, outcome: Result<(), CallbackError>) {
        *lock(&self.outcome) = Some(outcome);
        self.done.notify_waiters();
    }
}

impl ImagesProvider for ConfiguredImagesProvider {
    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn auth(&self) -> &ProviderAuth {
        &self.auth
    }

    fn get_models(&self) -> Vec<ImagesModel> {
        lock(&self.models).clone()
    }

    fn supports_refresh_models(&self) -> bool {
        self.refresh_models.is_some()
    }

    fn refresh_models(&self) -> Option<BoxFuture<'static, Result<(), CallbackError>>> {
        let refresh = Arc::clone(self.refresh_models.as_ref()?);
        let models = Arc::clone(&self.models);
        let inflight = Arc::clone(&self.inflight);
        Some(Box::pin(refresh_once(refresh, models, inflight)))
    }

    fn generate_images(
        &self,
        model: ImagesModel,
        context: ImagesContext,
        options: ImagesOptions,
    ) -> BoxFuture<'static, Result<AssistantImages, CallbackError>> {
        let api = Arc::clone(&self.api);
        Box::pin(async move { api.generate_images(model, context, options).await })
    }
}

async fn refresh_once(
    refresh: ImagesRefreshModels,
    models: Arc<Mutex<Vec<ImagesModel>>>,
    inflight: Arc<AsyncMutex<Option<Arc<RefreshState>>>>,
) -> Result<(), CallbackError> {
    let existing = join_or_start_refresh(&inflight).await;
    if let Some(state) = existing {
        return state.wait().await;
    }

    let outcome = run_refresh(refresh, &models).await;
    let state = inflight.lock().await.take();
    if let Some(state) = state {
        state.finish(outcome.clone());
    }
    outcome
}

async fn join_or_start_refresh(inflight: &AsyncMutex<Option<Arc<RefreshState>>>) -> Option<Arc<RefreshState>> {
    let mut current = inflight.lock().await;
    match current.as_ref() {
        Some(state) => Some(Arc::clone(state)),
        None => {
            *current = Some(Arc::new(RefreshState::default()));
            None
        }
    }
}

async fn run_refresh(refresh: ImagesRefreshModels, models: &Mutex<Vec<ImagesModel>>) -> Result<(), CallbackError> {
    match refresh().await {
        Ok(refreshed) => {
            *lock(models) = refreshed;
            Ok(())
        }
        Err(error) => Err(error),
    }
}

/// `createImagesProvider()`.
pub fn create_images_provider(input: CreateImagesProviderOptions) -> Arc<dyn ImagesProvider> {
    let models = Arc::new(Mutex::new(input.models));
    let inflight = Arc::new(AsyncMutex::new(None));
    Arc::new(ConfiguredImagesProvider {
        name: input.name.unwrap_or_else(|| input.id.clone()),
        id: input.id,
        auth: input.auth,
        models,
        refresh_models: input.refresh_models,
        inflight,
        api: input.api,
    })
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
