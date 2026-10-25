//! Port of `packages/ai/src/compat.ts` plus `compat/` helpers.
//!
//! Legacy global API: the api registry, api-dispatch `stream()`/`complete()`
//! with env API-key injection, generated catalog reads, and faux providers.
//!
//! Missing APIs, API mismatches and registered callback throws reject before a
//! stream exists. Only the frozen lazy/forwarding consumers encode caught
//! rejections in an error stream.

pub mod extension_oauth_types;

use std::sync::{Arc, LazyLock, Mutex, MutexGuard, OnceLock};

use crate::models::{CreateModelsOptions, Models as ModelsTrait, ModelsCollection, Provider};
use crate::providers::all::{builtin_models, get_builtin_model, get_builtin_models, get_builtin_providers};
use crate::providers::faux::{
    FauxCore, FauxProviderState, FauxResponseStep, RegisterFauxProviderOptions, create_faux_core,
};
use crate::request::{CallbackError, ProviderStreams, SimpleStreamOptions, StreamOptions};
use crate::types::{AssistantMessage, Context, Model};
use crate::utils::event_stream::AssistantMessageEventStream;

pub use crate::api::anthropic_messages_lazy::anthropic_messages_api;
pub use crate::api::azure_openai_responses::azure_open_ai_responses_api;
pub use crate::api::google_generative_ai::google_generative_ai_api;
pub use crate::api::google_vertex::google_vertex_api;
pub use crate::api::mistral_conversations::mistral_conversations_api;
pub use crate::api::openai_codex_responses::openai_codex_responses_api;
pub use crate::api::openai_completions::open_ai_completions_api;
pub use crate::api::openai_responses::open_ai_responses_api;
pub use crate::bedrock_provider::bedrock_converse_stream_api;
pub use crate::env_api_keys::*;
pub use crate::image_models::*;
pub use crate::images::*;
pub use crate::images_api_registry::*;
pub use crate::legacy_api_aliases::*;
pub use crate::providers::all::BuiltinProvider;
pub use crate::utils::retry::*;

/// Deprecated static catalog read (`getModel`).
pub fn get_model(provider: &str, model_id: &str) -> Option<Model> {
    get_builtin_model(provider, model_id)
}

/// Deprecated static catalog read (`getModels`).
pub fn get_models(provider: &str) -> Vec<Model> {
    get_builtin_models(provider)
}

/// Deprecated static catalog read (`getProviders`).
pub fn get_providers() -> Vec<String> {
    get_builtin_providers()
}

/// `ApiStreamFunction`: wrapped `stream` with a runtime api check.
pub type ApiStreamFunction =
    Arc<dyn Fn(Model, Context, StreamOptions) -> Result<AssistantMessageEventStream, CallbackError> + Send + Sync>;
/// `ApiStreamSimpleFunction`: wrapped `streamSimple` with a runtime api check.
pub type ApiStreamSimpleFunction = Arc<
    dyn Fn(Model, Context, SimpleStreamOptions) -> Result<AssistantMessageEventStream, CallbackError> + Send + Sync,
>;

/// One registered api implementation (`ApiProviderInternal`).
///
/// The stream closures reject a model whose `api` differs from the registered
/// api with `Mismatched api: <model.api> expected <api>`, as TypeScript's
/// `wrapStream`/`wrapStreamSimple` do.
pub struct ApiProvider {
    api: String,
    stream: ApiStreamFunction,
    stream_simple: ApiStreamSimpleFunction,
}

impl ApiProvider {
    /// Builds a provider from raw stream functions, applying the api check.
    pub fn new(
        api: impl Into<String>,
        stream: impl Fn(Model, Context, StreamOptions) -> Result<AssistantMessageEventStream, CallbackError>
        + Send
        + Sync
        + 'static,
        stream_simple: impl Fn(Model, Context, SimpleStreamOptions) -> Result<AssistantMessageEventStream, CallbackError>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        let api = api.into();
        let stream_api = api.clone();
        let simple_api = api.clone();
        Self {
            api,
            stream: Arc::new(move |model, context, options| {
                if model.api != stream_api {
                    return Err(mismatch_error(&model, &stream_api));
                }
                stream(model, context, options)
            }),
            stream_simple: Arc::new(move |model, context, options| {
                if model.api != simple_api {
                    return Err(mismatch_error(&model, &simple_api));
                }
                stream_simple(model, context, options)
            }),
        }
    }

    /// Wraps an api implementation module (`ProviderStreams`) with the api check.
    pub fn from_streams(api: impl Into<String>, streams: Arc<dyn ProviderStreams>) -> Self {
        let stream_streams = Arc::clone(&streams);
        let simple_streams = Arc::clone(&streams);
        Self::new(
            api,
            move |model, context, options| stream_streams.stream(model, context, options),
            move |model, context, options| simple_streams.stream_simple(model, context, options),
        )
    }

    pub fn api(&self) -> &str {
        &self.api
    }
}

impl ProviderStreams for ApiProvider {
    fn stream(
        &self,
        model: Model,
        context: Context,
        options: StreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        (self.stream)(model, context, options)
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, CallbackError> {
        (self.stream_simple)(model, context, options)
    }
}

fn mismatch_error(model: &Model, expected: &str) -> CallbackError {
    pi_js_compat::node_error::NativeNodeError::new(
        "Error",
        format!("Mismatched api: {} expected {expected}", model.api),
    )
    .into()
}

struct RegisteredApiProvider {
    provider: Arc<ApiProvider>,
    source_id: Option<String>,
}

#[derive(Default)]
struct Registry {
    providers: Vec<(String, RegisteredApiProvider)>,
    builtins: Vec<(String, Option<Arc<ApiProvider>>)>,
}

static REGISTRY: LazyLock<Mutex<Registry>> = LazyLock::new(|| Mutex::new(Registry::default()));

fn lock() -> MutexGuard<'static, Registry> {
    REGISTRY.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The built-in api implementations in `compat.ts` order.
fn builtin_apis() -> Vec<(&'static str, Arc<dyn ProviderStreams>)> {
    vec![
        ("anthropic-messages", Arc::new(anthropic_messages_api())),
        ("openai-completions", Arc::new(open_ai_completions_api())),
        ("openai-responses", Arc::new(open_ai_responses_api())),
        ("openai-codex-responses", Arc::new(openai_codex_responses_api())),
        ("azure-openai-responses", Arc::new(azure_open_ai_responses_api())),
        ("google-generative-ai", Arc::new(google_generative_ai_api())),
        ("google-vertex", Arc::new(google_vertex_api())),
        ("mistral-conversations", Arc::new(mistral_conversations_api())),
        ("bedrock-converse-stream", Arc::new(bedrock_converse_stream_api())),
    ]
}

/// `registerApiProvider()`; re-registering an api keeps its registry position.
pub fn register_api_provider(provider: ApiProvider, source_id: Option<String>) {
    ensure_initialized();
    insert_provider(Arc::new(provider), source_id);
}

fn insert_provider(provider: Arc<ApiProvider>, source_id: Option<String>) {
    let mut registry = lock();
    let api = provider.api().to_owned();
    let entry = RegisteredApiProvider { provider, source_id };
    match registry
        .providers
        .iter_mut()
        .find(|(registered_api, _)| *registered_api == api)
    {
        Some((_, existing)) => *existing = entry,
        None => registry.providers.push((api, entry)),
    }
}

/// `getApiProvider(api)`.
pub fn get_api_provider(api: &str) -> Option<Arc<ApiProvider>> {
    ensure_initialized();
    lock()
        .providers
        .iter()
        .find(|(registered_api, _)| registered_api == api)
        .map(|(_, entry)| Arc::clone(&entry.provider))
}

/// `getApiProviders()` in registration order.
pub fn get_api_providers() -> Vec<Arc<ApiProvider>> {
    ensure_initialized();
    lock()
        .providers
        .iter()
        .map(|(_, entry)| Arc::clone(&entry.provider))
        .collect()
}

/// `unregisterApiProviders(sourceId)`.
pub fn unregister_api_providers(source_id: &str) {
    ensure_initialized();
    lock()
        .providers
        .retain(|(_, entry)| entry.source_id.as_deref() != Some(source_id));
}

/// `registerBuiltInApiProviders()`: never clobbers an existing api entry.
pub fn register_built_in_api_providers() {
    for (api, streams) in builtin_apis() {
        if get_registered(api).is_none() {
            insert_provider(Arc::new(ApiProvider::from_streams(api, streams)), None);
        }
        let instance = get_registered(api);
        let mut registry = lock();
        match registry.builtins.iter_mut().find(|(registered, _)| registered == api) {
            Some((_, builtin)) => *builtin = instance,
            None => registry.builtins.push((api.to_owned(), instance)),
        }
    }
}

/// `resetApiProviders()`.
pub fn reset_api_providers() {
    {
        let mut registry = lock();
        registry.providers.clear();
        registry.builtins.clear();
    }
    register_built_in_api_providers();
}

fn get_registered(api: &str) -> Option<Arc<ApiProvider>> {
    lock()
        .providers
        .iter()
        .find(|(registered_api, _)| registered_api == api)
        .map(|(_, entry)| Arc::clone(&entry.provider))
}

fn ensure_initialized() {
    static INITIALIZED: OnceLock<()> = OnceLock::new();
    INITIALIZED.get_or_init(register_built_in_api_providers);
}

/// `registerFauxProvider()`.
pub struct FauxProviderRegistration {
    core: Arc<FauxCore>,
    source_id: String,
}

impl FauxProviderRegistration {
    pub fn api(&self) -> &str {
        self.core.api()
    }

    pub fn models(&self) -> &[Model] {
        self.core.models()
    }

    pub fn get_model(&self, id: Option<&str>) -> Option<Model> {
        self.core.get_model(id)
    }

    pub fn state(&self) -> Arc<Mutex<FauxProviderState>> {
        self.core.state()
    }

    pub fn set_responses(&self, responses: Vec<FauxResponseStep>) {
        self.core.set_responses(responses);
    }

    pub fn append_responses(&self, responses: Vec<FauxResponseStep>) {
        self.core.append_responses(responses);
    }

    pub fn get_pending_response_count(&self) -> usize {
        self.core.get_pending_response_count()
    }

    pub fn unregister(&self) {
        unregister_api_providers(&self.source_id);
    }
}

/// `registerFauxProvider(options?)`.
pub fn register_faux_provider(options: RegisterFauxProviderOptions) -> FauxProviderRegistration {
    let core = Arc::new(create_faux_core(options));
    let source_id = format!("faux-provider-{}", random_base36(8));
    let api = core.api().to_owned();
    let streams: Arc<dyn ProviderStreams> = Arc::clone(&core) as Arc<dyn ProviderStreams>;
    register_api_provider(ApiProvider::from_streams(api, streams), Some(source_id.clone()));
    FauxProviderRegistration { core, source_id }
}

fn random_base36(length: usize) -> String {
    const ALPHABET: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    let mut bytes = vec![0_u8; length];
    getrandom::fill(&mut bytes).expect("OS random source must be available for faux provider ids");
    bytes
        .into_iter()
        .map(|byte| char::from(ALPHABET[usize::from(byte) % ALPHABET.len()]))
        .collect()
}

static COMPAT_MODELS: LazyLock<ModelsCollection> = LazyLock::new(|| builtin_models(CreateModelsOptions::default()));

const AMBIENT_AUTH_MARKER: &str = "<authenticated>";

fn has_explicit_api_key(options: &StreamOptions) -> bool {
    options
        .request
        .api_key
        .as_ref()
        .is_some_and(|api_key| !api_key.trim().is_empty())
}

fn with_env_api_key(
    model: &Model,
    mut options: StreamOptions,
) -> Result<StreamOptions, pi_js_compat::node_error::NativeNodeError> {
    if has_explicit_api_key(&options) {
        return Ok(options);
    }
    match get_env_api_key(&model.provider, options.request.env.as_ref())? {
        Some(api_key) if api_key != AMBIENT_AUTH_MARKER => options.request.api_key = Some(api_key),
        _ => {}
    }
    Ok(options)
}

fn with_env_api_key_simple(
    model: &Model,
    mut options: SimpleStreamOptions,
) -> Result<SimpleStreamOptions, pi_js_compat::node_error::NativeNodeError> {
    if has_explicit_api_key(&options.stream) {
        return Ok(options);
    }
    match get_env_api_key(&model.provider, options.stream.request.env.as_ref())? {
        Some(api_key) if api_key != AMBIENT_AUTH_MARKER => options.stream.request.api_key = Some(api_key),
        _ => {}
    }
    Ok(options)
}

fn builtin_api_instance(api: &str) -> Option<Arc<ApiProvider>> {
    lock()
        .builtins
        .iter()
        .find(|(builtin_api, _)| builtin_api == api)
        .and_then(|(_, instance)| instance.clone())
}

fn get_builtin_provider_for_model(model: &Model) -> Option<Arc<dyn Provider>> {
    let registered = get_api_provider(&model.api)?;
    let builtin = builtin_api_instance(&model.api)?;
    if !Arc::ptr_eq(&registered, &builtin) {
        return None;
    }
    let provider = COMPAT_MODELS.get_provider(&model.provider)?;
    COMPAT_MODELS
        .get_model(&model.provider, &model.id)
        .is_some_and(|candidate| candidate.api == model.api)
        .then_some(provider)
}

fn resolve_api_provider(model: &Model) -> Result<Arc<ApiProvider>, pi_js_compat::node_error::NativeNodeError> {
    get_api_provider(&model.api).ok_or_else(|| {
        pi_js_compat::node_error::NativeNodeError::from_constructor(
            pi_js_compat::node_error::NativeErrorClass::Error,
            format!("No API provider registered for api: {}", model.api),
        )
    })
}

/// `stream()`: dispatch through the builtin provider or the api registry.
pub fn stream(
    model: Model,
    context: Context,
    options: StreamOptions,
) -> Result<AssistantMessageEventStream, CallbackError> {
    if get_builtin_provider_for_model(&model).is_some() {
        return Ok(COMPAT_MODELS.stream(model, context, options));
    }
    let provider = resolve_api_provider(&model)?;
    let options = with_env_api_key(&model, options)?;
    provider.stream(model, context, options)
}

/// `complete()`.
pub async fn complete(
    model: Model,
    context: Context,
    options: StreamOptions,
) -> Result<AssistantMessage, crate::request::CallbackError> {
    stream(model, context, options)?
        .result()
        .await
        .map_err(|error| crate::request::CallbackError::new(error.message().clone()))
}

/// `streamSimple()`.
pub fn stream_simple(
    model: Model,
    context: Context,
    options: SimpleStreamOptions,
) -> Result<AssistantMessageEventStream, CallbackError> {
    if get_builtin_provider_for_model(&model).is_some() {
        return Ok(COMPAT_MODELS.stream_simple(model, context, options));
    }
    let provider = resolve_api_provider(&model)?;
    let options = with_env_api_key_simple(&model, options)?;
    provider.stream_simple(model, context, options)
}

/// `completeSimple()`.
pub async fn complete_simple(
    model: Model,
    context: Context,
    options: SimpleStreamOptions,
) -> Result<AssistantMessage, crate::request::CallbackError> {
    stream_simple(model, context, options)?
        .result()
        .await
        .map_err(|error| crate::request::CallbackError::new(error.message().clone()))
}
