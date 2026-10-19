//! Port of `packages/coding-agent/src/extensions/llama/provider.ts`.
//!
//! Registers the `llama.cpp` provider, resolves its router URL from credentials
//! or `LLAMA_BASE_URL`, and refreshes the loaded-model catalog.

use std::sync::{Arc, Mutex};

use pi_ai::auth::types::{
    ApiKeyAuth, ApiKeyCheckInput, ApiKeyCredential, ApiKeyResolveInput, AuthCheck, AuthContext, AuthError,
    AuthInteraction, AuthPrompt, AuthPromptKind, AuthResult, AuthType, Credential, ModelAuth, ProviderAuth,
};
use pi_ai::models::{ModelsPublication, PersistAction, Provider, PublishFn, RefreshModelsContext};
use pi_ai::models_store::ModelsStoreEntry;
use pi_ai::request::{BoxFuture, CallbackError, Fetch, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{
    Context, MaxTokensField, Modality, Model, ModelCompat, ModelCost, ModelCostRates, OpenAICompletionsCompat,
    ProviderEnv,
};
use pi_ai::utils::event_stream::AssistantMessageEventStream;
use pi_js_compat::string::js_trim;

use crate::bundled_extensions::llama::client::{
    LlamaClient, LlamaListOptions, LlamaModelInfo, LlamaResult, STATUS_LOADED, llama_inference_url,
    normalize_llama_server_url,
};

pub const LLAMA_PROVIDER_ID: &str = "llama.cpp";
pub const DEFAULT_LLAMA_SERVER_URL: &str = "http://127.0.0.1:8080";

/// `credentialServerUrl`: only a non-empty credential env URL is configured.
fn credential_server_url(credential: Option<&ApiKeyCredential>) -> LlamaResult<Option<String>> {
    let value = credential
        .and_then(|credential| credential.env.as_ref())
        .and_then(|env| env.get("LLAMA_BASE_URL"))
        .map(|value| js_trim(value));
    match value {
        Some(value) if !value.is_empty() => Ok(Some(normalize_llama_server_url(value)?)),
        _ => Ok(None),
    }
}

async fn resolve_server_url(
    ctx: &dyn AuthContext,
    credential: Option<&ApiKeyCredential>,
) -> LlamaResult<Option<String>> {
    let configured = match credential_server_url(credential)? {
        Some(url) => Some(url),
        None => ctx
            .env("LLAMA_BASE_URL")
            .await
            .map(|value| js_trim(&value).to_owned())
            .filter(|value| !value.is_empty()),
    };
    configured.map(|value| normalize_llama_server_url(&value)).transpose()
}

fn model_accepts_images(model: &LlamaModelInfo) -> bool {
    model
        .architecture
        .as_ref()
        .and_then(|architecture| architecture.input_modalities.as_ref())
        .is_some_and(|modalities| modalities.iter().any(|modality| modality == "image"))
}

fn to_pi_model(model: &LlamaModelInfo, server_url: &str) -> LlamaResult<Model> {
    let reported_context_window = model.meta.as_ref().and_then(|meta| meta.n_ctx.or(meta.n_ctx_train));
    let context_window = reported_context_window
        .filter(|value| *value > 0.0)
        .unwrap_or(128_000.0);
    Ok(Model {
        id: model.id.clone(),
        name: model.id.clone(),
        api: "openai-completions".to_owned(),
        provider: LLAMA_PROVIDER_ID.to_owned(),
        base_url: llama_inference_url(server_url)?,
        reasoning: false,
        thinking_level_map: None,
        input: if model_accepts_images(model) {
            vec![Modality::Text, Modality::Image]
        } else {
            vec![Modality::Text]
        },
        cost: ModelCost {
            rates: ModelCostRates {
                input: 0.0,
                output: 0.0,
                cache_read: 0.0,
                cache_write: 0.0,
            },
            tiers: None,
        },
        context_window,
        auto_compaction_threshold: None,
        max_tokens: context_window,
        sampling_params: None,
        headers: None,
        compat: Some(ModelCompat::OpenAICompletions(Box::new(OpenAICompletionsCompat {
            supports_store: Some(false),
            supports_developer_role: Some(false),
            supports_reasoning_effort: Some(false),
            supports_usage_in_streaming: Some(true),
            supports_strict_mode: Some(false),
            max_tokens_field: Some(MaxTokensField::MaxTokens),
            ..OpenAICompletionsCompat::default()
        }))),
    })
}

fn credential_source(credential: Option<&ApiKeyCredential>) -> String {
    if credential.is_some() {
        "stored credential".to_owned()
    } else {
        "LLAMA_BASE_URL".to_owned()
    }
}

fn loaded_models(catalog: &[LlamaModelInfo], server_url: &str) -> LlamaResult<Vec<Model>> {
    catalog
        .iter()
        .filter(|model| model.status.value == STATUS_LOADED)
        .map(|model| to_pi_model(model, server_url))
        .collect()
}

struct LlamaState {
    models: Arc<Mutex<Vec<Model>>>,
    fetch: Arc<dyn Fetch>,
}

impl LlamaState {
    fn current_models(&self) -> Vec<Model> {
        self.models
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    async fn publish_update(&self, publish: &PublishFn, models: Vec<Model>) -> Result<bool, CallbackError> {
        let update = Arc::clone(&self.models);
        publish(ModelsPublication::default().with_update(move || {
            *update.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = models.clone();
        }))
        .await
    }

    async fn publish_persist(&self, publish: &PublishFn, models: Vec<Model>) -> Result<bool, CallbackError> {
        let update = Arc::clone(&self.models);
        let entry = ModelsStoreEntry {
            models: models.clone(),
            last_modified: None,
            checked_at: Some(pi_ai::request::now_millis() as f64),
            etag: None,
        };
        publish(
            ModelsPublication::new(PersistAction::Write(entry)).with_update(move || {
                *update.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = models.clone();
            }),
        )
        .await
    }

    async fn refresh(&self, context: RefreshModelsContext) -> Result<(), CallbackError> {
        if let Some(stored) = context.stored.as_ref() {
            let restored = stored
                .models
                .iter()
                .filter(|model| model.provider == LLAMA_PROVIDER_ID && model.api == "openai-completions")
                .cloned()
                .collect();
            if !self.publish_update(&context.publish, restored).await? {
                return Ok(());
            }
        }
        if !context.allow_network || context.signal.is_cancelled() {
            return Ok(());
        }
        let Some(credential) = context.credential.as_ref().and_then(Credential::as_api_key) else {
            return Ok(());
        };
        let Some(server_url) = credential_server_url(Some(credential))? else {
            return Ok(());
        };
        let client = LlamaClient::with_fetch(&server_url, credential.key.as_deref(), Arc::clone(&self.fetch))?;
        let catalog = client
            .list(LlamaListOptions {
                reload: false,
                signal: Some(context.signal.clone()),
            })
            .await?;
        if context.signal.is_cancelled() {
            return Ok(());
        }
        let refreshed = loaded_models(&catalog, &server_url)?;
        self.publish_persist(&context.publish, refreshed).await?;
        Ok(())
    }
}

struct LlamaApiKeyAuth {
    fetch: Arc<dyn Fetch>,
}

impl ApiKeyAuth for LlamaApiKeyAuth {
    fn name(&self) -> &str {
        "llama.cpp server"
    }

    fn login(
        &self,
        interaction: Arc<dyn AuthInteraction>,
    ) -> Option<BoxFuture<'static, Result<ApiKeyCredential, AuthError>>> {
        let fetch = Arc::clone(&self.fetch);
        Some(Box::pin(async move {
            let placeholder = std::env::var("LLAMA_BASE_URL").unwrap_or_else(|_| DEFAULT_LLAMA_SERVER_URL.to_owned());
            let entered_url = interaction
                .prompt(AuthPrompt {
                    signal: None,
                    kind: AuthPromptKind::Text {
                        message: "llama.cpp server URL".to_owned(),
                        placeholder: Some(placeholder),
                    },
                })
                .await?;
            let fallback = std::env::var("LLAMA_BASE_URL")
                .ok()
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| DEFAULT_LLAMA_SERVER_URL.to_owned());
            let entered_url = js_trim(&entered_url);
            let server_url = normalize_llama_server_url(if entered_url.is_empty() { &fallback } else { entered_url })?;
            let api_key = js_trim(
                &interaction
                    .prompt(AuthPrompt {
                        signal: None,
                        kind: AuthPromptKind::Secret {
                            message: "API key (optional)".to_owned(),
                            placeholder: None,
                        },
                    })
                    .await?,
            )
            .to_owned();
            let client =
                LlamaClient::with_fetch(&server_url, (!api_key.is_empty()).then_some(api_key.as_str()), fetch)?;
            client
                .list(LlamaListOptions {
                    reload: false,
                    signal: Some(interaction.signal()),
                })
                .await?;
            let mut env = ProviderEnv::new();
            env.insert("LLAMA_BASE_URL".to_owned(), server_url);
            Ok(ApiKeyCredential {
                key: (!api_key.is_empty()).then_some(api_key),
                env: Some(env),
            })
        }))
    }

    fn check(&self, input: ApiKeyCheckInput) -> Option<BoxFuture<'static, Result<Option<AuthCheck>, AuthError>>> {
        Some(Box::pin(async move {
            let server_url = resolve_server_url(input.ctx.as_ref(), input.credential.as_ref()).await?;
            Ok(server_url.map(|_| AuthCheck {
                source: Some(credential_source(input.credential.as_ref())),
                auth_type: AuthType::ApiKey,
            }))
        }))
    }

    fn resolve(&self, input: ApiKeyResolveInput) -> BoxFuture<'static, Result<Option<AuthResult>, AuthError>> {
        Box::pin(async move {
            let Some(server_url) = resolve_server_url(input.ctx.as_ref(), input.credential.as_ref()).await? else {
                return Ok(None);
            };
            let api_key = match input.credential.as_ref().and_then(|credential| credential.key.clone()) {
                Some(key) => key,
                None => input
                    .ctx
                    .env("LLAMA_API_KEY")
                    .await
                    .unwrap_or_else(|| "local".to_owned()),
            };
            let mut env = input
                .credential
                .as_ref()
                .and_then(|credential| credential.env.clone())
                .unwrap_or_default();
            env.insert("LLAMA_BASE_URL".to_owned(), server_url.clone());
            Ok(Some(AuthResult {
                auth: ModelAuth {
                    api_key: Some(api_key),
                    headers: None,
                    base_url: Some(llama_inference_url(&server_url)?),
                },
                env: Some(env),
                source: Some(credential_source(input.credential.as_ref())),
            }))
        })
    }
}

struct LlamaProvider {
    base_url: String,
    auth: ProviderAuth,
    state: Arc<LlamaState>,
}

impl Provider for LlamaProvider {
    fn id(&self) -> &str {
        LLAMA_PROVIDER_ID
    }

    fn name(&self) -> &str {
        "llama.cpp"
    }

    fn base_url(&self) -> Option<&str> {
        Some(&self.base_url)
    }

    fn auth(&self) -> &ProviderAuth {
        &self.auth
    }

    fn get_models(&self) -> Vec<Model> {
        self.state.current_models()
    }

    fn supports_refresh_models(&self) -> bool {
        true
    }

    fn refresh_models(&self, context: RefreshModelsContext) -> Option<BoxFuture<'static, Result<(), CallbackError>>> {
        let state = Arc::clone(&self.state);
        Some(Box::pin(async move { state.refresh(context).await }))
    }

    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        pi_ai::compat::stream(model, context, options)
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        pi_ai::compat::stream_simple(model, context, options)
    }
}

/// `LlamaProviderController`: the provider plus its mutable catalog setter.
pub struct LlamaProviderController {
    provider: Arc<dyn Provider>,
    models: Arc<Mutex<Vec<Model>>>,
}

impl LlamaProviderController {
    pub fn provider(&self) -> Arc<dyn Provider> {
        Arc::clone(&self.provider)
    }

    /// `setCatalog`: replaces the catalog with the currently loaded models.
    pub fn set_catalog(&self, catalog: &[LlamaModelInfo], server_url: &str) -> LlamaResult<()> {
        let models = loaded_models(catalog, server_url)?;
        *self.models.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = models;
        Ok(())
    }
}

/// `createLlamaProvider()`.
pub fn create_llama_provider() -> LlamaProviderController {
    create_llama_provider_with_fetch(pi_ai::http::default_fetch())
}

/// [`create_llama_provider`] with an injected HTTP implementation.
pub fn create_llama_provider_with_fetch(fetch: Arc<dyn Fetch>) -> LlamaProviderController {
    let models = Arc::new(Mutex::new(Vec::new()));
    let state = Arc::new(LlamaState {
        models: Arc::clone(&models),
        fetch: Arc::clone(&fetch),
    });
    let base_url = llama_inference_url(DEFAULT_LLAMA_SERVER_URL).expect("the default llama.cpp server URL is valid");
    let auth = ProviderAuth::api_key(Arc::new(LlamaApiKeyAuth { fetch }));
    let provider: Arc<dyn Provider> = Arc::new(LlamaProvider { base_url, auth, state });
    LlamaProviderController { provider, models }
}
