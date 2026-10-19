//! Port of `packages/ai/src/providers/faux.ts`.
//!
//! Deterministic fake provider used to exercise `Models` without network
//! access. Token chunking and generated ids are not bit-identical to the
//! TypeScript `Math.random` values; tests fix `tokenSize` and tool-call ids.

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::models::{CreateProviderOptions, Provider, ProviderApi, create_provider};
use crate::request::{
    BoxFuture, CallbackError, DeferredCancelOptions, DeferredFetchOptions, ProviderStreams, SimpleStreamOptions,
    StreamOptions, now_millis,
};
use crate::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, CacheRetention, Context, DeferredHandle, DoneReason,
    ErrorReason, JsonObject, Message, Modality, Model, ModelCostRates, ProviderResponse, StopReason, TextContent,
    TextOrImageContent, ThinkingContent, ToolCall, Usage, UsageCost, UserContent,
};
use crate::utils::event_stream::{
    AssistantMessageEventSender, AssistantMessageEventStream, create_assistant_message_event_stream,
};

const DEFAULT_API: &str = "faux";
const DEFAULT_PROVIDER: &str = "faux";
const DEFAULT_MODEL_ID: &str = "faux-1";
const DEFAULT_MODEL_NAME: &str = "Faux Model";
const DEFAULT_BASE_URL: &str = "http://localhost:0";
const DEFAULT_MIN_TOKEN_SIZE: usize = 3;
const DEFAULT_MAX_TOKEN_SIZE: usize = 5;

fn default_usage() -> Usage {
    Usage::default()
}

#[derive(Debug, Clone, Default)]
pub struct FauxModelDefinition {
    pub id: String,
    pub name: Option<String>,
    pub reasoning: Option<bool>,
    pub input: Option<Vec<Modality>>,
    pub cost: Option<ModelCostRates>,
    pub context_window: Option<f64>,
    pub max_tokens: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FauxTokenSize {
    pub min: Option<f64>,
    pub max: Option<f64>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FauxDeferredOptions {
    /// Fetches that return the original handle before the scripted response is ready.
    pub pending_fetches: Option<f64>,
    pub poll_after_ms: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct RegisterFauxProviderOptions {
    pub api: Option<String>,
    pub provider: Option<String>,
    pub models: Option<Vec<FauxModelDefinition>>,
    pub deferred: Option<FauxDeferredOptions>,
    pub tokens_per_second: Option<f64>,
    pub token_size: Option<FauxTokenSize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FauxContent {
    Text(String),
    Block(AssistantContent),
    Blocks(Vec<AssistantContent>),
}

impl From<&str> for FauxContent {
    fn from(text: &str) -> Self {
        Self::Text(text.to_owned())
    }
}

impl From<String> for FauxContent {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl From<AssistantContent> for FauxContent {
    fn from(block: AssistantContent) -> Self {
        Self::Block(block)
    }
}

impl From<Vec<AssistantContent>> for FauxContent {
    fn from(blocks: Vec<AssistantContent>) -> Self {
        Self::Blocks(blocks)
    }
}

pub fn faux_text(text: impl Into<String>) -> AssistantContent {
    AssistantContent::Text(TextContent::new(text))
}

pub fn faux_thinking(thinking: impl Into<String>) -> AssistantContent {
    AssistantContent::Thinking(ThinkingContent::new(thinking))
}

pub fn faux_tool_call(name: impl Into<String>, arguments: JsonObject, id: Option<String>) -> AssistantContent {
    AssistantContent::ToolCall(ToolCall::new(id.unwrap_or_else(|| random_id("tool")), name, arguments))
}

#[derive(Debug, Clone, Default)]
pub struct FauxAssistantMessageOptions {
    pub stop_reason: Option<StopReason>,
    pub deferred: Option<DeferredHandle>,
    pub error_message: Option<String>,
    pub response_id: Option<String>,
    pub timestamp: Option<i64>,
}

/// `fauxAssistantMessage()`.
pub fn faux_assistant_message(
    content: impl Into<FauxContent>,
    options: FauxAssistantMessageOptions,
) -> AssistantMessage {
    let content = match content.into() {
        FauxContent::Text(text) => vec![faux_text(text)],
        FauxContent::Block(block) => vec![block],
        FauxContent::Blocks(blocks) => blocks,
    };
    let mut message = AssistantMessage::empty(DEFAULT_API, DEFAULT_PROVIDER, DEFAULT_MODEL_ID, 0);
    message.content = content;
    message.usage = default_usage();
    message.stop_reason = options.stop_reason.unwrap_or(StopReason::Stop);
    message.deferred = options.deferred;
    message.error_message = options.error_message;
    message.response_id = options.response_id;
    message.timestamp = options.timestamp.unwrap_or_else(now_millis);
    message
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FauxProviderState {
    pub call_count: u64,
    pub deferred_fetch_count: u64,
    pub cancelled_deferred: Vec<DeferredHandle>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FauxProviderStateSnapshot {
    pub call_count: u64,
    pub deferred_fetch_count: u64,
}

pub type FauxResponseFactory = Arc<
    dyn Fn(
            Context,
            Option<SimpleStreamOptions>,
            FauxProviderStateSnapshot,
            Model,
        ) -> BoxFuture<'static, Result<AssistantMessage, CallbackError>>
        + Send
        + Sync,
>;

#[derive(Clone)]
pub enum FauxResponseStep {
    Message(Box<AssistantMessage>),
    Factory(FauxResponseFactory),
}

impl FauxResponseStep {
    pub fn message(message: AssistantMessage) -> Self {
        Self::Message(Box::new(message))
    }
}

impl From<AssistantMessage> for FauxResponseStep {
    fn from(message: AssistantMessage) -> Self {
        Self::Message(Box::new(message))
    }
}

impl From<FauxResponseFactory> for FauxResponseStep {
    fn from(factory: FauxResponseFactory) -> Self {
        Self::Factory(factory)
    }
}

#[derive(Clone)]
struct DeferredEntry {
    handle: DeferredHandle,
    step: FauxResponseStep,
    context: Context,
    options: Option<SimpleStreamOptions>,
    model: Model,
    pending_fetches: usize,
    cancelled: bool,
    final_message: Option<AssistantMessage>,
}

pub struct FauxCore {
    api: String,
    provider: String,
    models: Vec<Model>,
    min_token_size: usize,
    max_token_size: usize,
    tokens_per_second: Option<f64>,
    pending_responses: Arc<Mutex<Vec<FauxResponseStep>>>,
    state: Arc<Mutex<FauxProviderState>>,
    prompt_cache: Arc<Mutex<HashMap<String, String>>>,
    deferred_responses: Arc<Mutex<HashMap<String, DeferredEntry>>>,
    deferred: FauxDeferredOptions,
}

/// `createFauxCore()`.
pub fn create_faux_core(options: RegisterFauxProviderOptions) -> FauxCore {
    let api = options.api.unwrap_or_else(|| random_id(DEFAULT_API));
    let provider = options.provider.unwrap_or_else(|| DEFAULT_PROVIDER.to_owned());
    let min_token_size = options
        .token_size
        .and_then(|size| size.min)
        .unwrap_or(DEFAULT_MIN_TOKEN_SIZE as f64)
        .min(
            options
                .token_size
                .and_then(|size| size.max)
                .unwrap_or(DEFAULT_MAX_TOKEN_SIZE as f64),
        )
        .max(1.0) as usize;
    let max_token_size = options
        .token_size
        .and_then(|size| size.max)
        .unwrap_or(DEFAULT_MAX_TOKEN_SIZE as f64)
        .max(min_token_size as f64) as usize;

    let definitions = options.models.unwrap_or_else(|| {
        vec![FauxModelDefinition {
            id: DEFAULT_MODEL_ID.to_owned(),
            name: Some(DEFAULT_MODEL_NAME.to_owned()),
            reasoning: Some(false),
            input: Some(vec![Modality::Text, Modality::Image]),
            cost: Some(ModelCostRates::default()),
            context_window: Some(128_000.0),
            max_tokens: Some(16_384.0),
        }]
    });
    let models = definitions
        .into_iter()
        .map(|definition| Model {
            id: definition.id.clone(),
            name: definition.name.unwrap_or_else(|| definition.id.clone()),
            api: api.clone(),
            provider: provider.clone(),
            base_url: DEFAULT_BASE_URL.to_owned(),
            reasoning: definition.reasoning.unwrap_or(false),
            thinking_level_map: None,
            input: definition
                .input
                .unwrap_or_else(|| vec![Modality::Text, Modality::Image]),
            cost: crate::types::ModelCost {
                rates: definition.cost.unwrap_or_default(),
                tiers: None,
            },
            context_window: definition.context_window.unwrap_or(128_000.0),
            auto_compaction_threshold: None,
            max_tokens: definition.max_tokens.unwrap_or(16_384.0),
            sampling_params: None,
            headers: None,
            compat: None,
        })
        .collect();

    FauxCore {
        api,
        provider,
        models,
        min_token_size,
        max_token_size,
        tokens_per_second: options.tokens_per_second,
        pending_responses: Arc::new(Mutex::new(Vec::new())),
        state: Arc::new(Mutex::new(FauxProviderState::default())),
        prompt_cache: Arc::new(Mutex::new(HashMap::new())),
        deferred_responses: Arc::new(Mutex::new(HashMap::new())),
        deferred: options.deferred.unwrap_or_default(),
    }
}

impl FauxCore {
    pub fn api(&self) -> &str {
        &self.api
    }

    pub fn provider_id(&self) -> &str {
        &self.provider
    }

    pub fn models(&self) -> &[Model] {
        &self.models
    }

    pub fn state(&self) -> Arc<Mutex<FauxProviderState>> {
        Arc::clone(&self.state)
    }

    pub fn state_snapshot(&self) -> FauxProviderState {
        lock(&self.state).clone()
    }

    pub fn set_responses(&self, responses: Vec<FauxResponseStep>) {
        *lock(&self.pending_responses) = responses;
    }

    pub fn append_responses(&self, responses: Vec<FauxResponseStep>) {
        lock(&self.pending_responses).extend(responses);
    }

    pub fn get_pending_response_count(&self) -> usize {
        lock(&self.pending_responses).len()
    }

    pub fn get_model(&self, id: Option<&str>) -> Option<Model> {
        match id {
            None => self.models.first().cloned(),
            Some(id) => self.models.iter().find(|model| model.id == id).cloned(),
        }
    }

    fn stream_with_options(
        &self,
        request_model: Model,
        context: Context,
        options: Option<SimpleStreamOptions>,
    ) -> AssistantMessageEventStream {
        let (sender, stream) = create_assistant_message_event_stream();
        let step = {
            let mut pending = lock(&self.pending_responses);
            if pending.is_empty() {
                None
            } else {
                Some(pending.remove(0))
            }
        };
        lock(&self.state).call_count += 1;

        let min_token_size = self.min_token_size;
        let max_token_size = self.max_token_size;
        let tokens_per_second = self.tokens_per_second;
        let deferred_options = self.deferred;
        let resolver = FauxResolver {
            deferred_responses: Arc::clone(&self.deferred_responses),
            prompt_cache: Arc::clone(&self.prompt_cache),
            state: Arc::clone(&self.state),
            api: self.api.clone(),
            provider: self.provider.clone(),
        };

        tokio::spawn(async move {
            if let Some(on_response) = options
                .as_ref()
                .and_then(|options| options.stream.request.on_response.clone())
                && let Err(error) = on_response(
                    ProviderResponse {
                        status: 200.0,
                        headers: BTreeMap::new(),
                    },
                    request_model.clone(),
                )
                .await
            {
                let message =
                    create_error_message(&error.message, &resolver.api, &resolver.provider, &request_model.id);
                sender.push(AssistantMessageEvent::Error {
                    reason: ErrorReason::Error,
                    error: message.clone(),
                });
                sender.end(Some(message));
                return;
            }

            let Some(step) = step else {
                let mut message = create_error_message(
                    "No more faux responses queued",
                    &resolver.api,
                    &resolver.provider,
                    &request_model.id,
                );
                message = with_usage_estimate(
                    message,
                    &context,
                    options.as_ref().map(|options| &options.stream),
                    &resolver.prompt_cache,
                );
                sender.push(AssistantMessageEvent::Error {
                    reason: ErrorReason::Error,
                    error: message.clone(),
                });
                sender.end(Some(message));
                return;
            };

            let signal = options
                .as_ref()
                .and_then(|options| options.stream.request.signal.clone());

            if options.as_ref().is_some_and(|options| options.deferred.is_some()) {
                let handle = DeferredHandle {
                    provider: request_model.provider.clone(),
                    model_id: request_model.id.clone(),
                    api: request_model.api.clone(),
                    id: random_id("deferred"),
                    expires_at: None,
                    poll_after_ms: deferred_options.poll_after_ms,
                    data: None,
                };
                let pending_fetches = deferred_options.pending_fetches.unwrap_or(0.0).max(0.0) as usize;
                lock(&resolver.deferred_responses).insert(
                    handle.id.clone(),
                    DeferredEntry {
                        handle: handle.clone(),
                        step,
                        context: context.clone(),
                        options: options.clone(),
                        model: request_model.clone(),
                        pending_fetches,
                        cancelled: false,
                        final_message: None,
                    },
                );
                if let Err(error) = stream_with_deltas(
                    &sender,
                    create_deferred_message(&request_model, handle),
                    min_token_size,
                    max_token_size,
                    tokens_per_second,
                    signal.as_ref(),
                )
                .await
                {
                    let message =
                        create_error_message(&error.message, &resolver.api, &resolver.provider, &request_model.id);
                    sender.push(AssistantMessageEvent::Error {
                        reason: ErrorReason::Error,
                        error: message.clone(),
                    });
                    sender.end(Some(message));
                }
                return;
            }

            let result = resolver.resolve(step, &context, options.as_ref(), &request_model).await;
            match result {
                Ok(message) => {
                    if let Err(error) = stream_with_deltas(
                        &sender,
                        message,
                        min_token_size,
                        max_token_size,
                        tokens_per_second,
                        signal.as_ref(),
                    )
                    .await
                    {
                        let message =
                            create_error_message(&error.message, &resolver.api, &resolver.provider, &request_model.id);
                        sender.push(AssistantMessageEvent::Error {
                            reason: ErrorReason::Error,
                            error: message.clone(),
                        });
                        sender.end(Some(message));
                    }
                }
                Err(error) => {
                    let message =
                        create_error_message(&error.message, &resolver.api, &resolver.provider, &request_model.id);
                    sender.push(AssistantMessageEvent::Error {
                        reason: ErrorReason::Error,
                        error: message.clone(),
                    });
                    sender.end(Some(message));
                }
            }
        });

        stream
    }

    fn fetch_deferred_stream(
        &self,
        request_model: Model,
        handle: DeferredHandle,
        options: DeferredFetchOptions,
    ) -> AssistantMessageEventStream {
        let (sender, stream) = create_assistant_message_event_stream();
        lock(&self.state).deferred_fetch_count += 1;

        let min_token_size = self.min_token_size;
        let max_token_size = self.max_token_size;
        let tokens_per_second = self.tokens_per_second;
        let resolver = FauxResolver {
            deferred_responses: Arc::clone(&self.deferred_responses),
            prompt_cache: Arc::clone(&self.prompt_cache),
            state: Arc::clone(&self.state),
            api: self.api.clone(),
            provider: self.provider.clone(),
        };

        tokio::spawn(async move {
            if let Some(on_response) = options.request.on_response.clone()
                && let Err(error) = on_response(
                    ProviderResponse {
                        status: 200.0,
                        headers: BTreeMap::new(),
                    },
                    request_model.clone(),
                )
                .await
            {
                let message =
                    create_error_message(&error.message, &resolver.api, &resolver.provider, &request_model.id);
                sender.push(AssistantMessageEvent::Error {
                    reason: ErrorReason::Error,
                    error: message.clone(),
                });
                sender.end(Some(message));
                return;
            }
            let result = resolver.fetch(&request_model, &handle).await;
            match result {
                Ok(message) => {
                    if let Err(error) = stream_with_deltas(
                        &sender,
                        message,
                        min_token_size,
                        max_token_size,
                        tokens_per_second,
                        options.request.signal.as_ref(),
                    )
                    .await
                    {
                        let message =
                            create_error_message(&error.message, &resolver.api, &resolver.provider, &request_model.id);
                        sender.push(AssistantMessageEvent::Error {
                            reason: ErrorReason::Error,
                            error: message.clone(),
                        });
                        sender.end(Some(message));
                    }
                }
                Err(error) => {
                    let message =
                        create_error_message(&error.message, &resolver.api, &resolver.provider, &request_model.id);
                    sender.push(AssistantMessageEvent::Error {
                        reason: ErrorReason::Error,
                        error: message.clone(),
                    });
                    sender.end(Some(message));
                }
            }
        });

        stream
    }
}

struct FauxResolver {
    deferred_responses: Arc<Mutex<HashMap<String, DeferredEntry>>>,
    prompt_cache: Arc<Mutex<HashMap<String, String>>>,
    state: Arc<Mutex<FauxProviderState>>,
    api: String,
    provider: String,
}

impl FauxResolver {
    async fn fetch(&self, request_model: &Model, handle: &DeferredHandle) -> Result<AssistantMessage, CallbackError> {
        let entry = lock(&self.deferred_responses).get(&handle.id).cloned();
        let Some(entry) = entry else {
            return Err(CallbackError::new(format!(
                "Unknown faux deferred response: {}",
                handle.id
            )));
        };
        if entry.handle.provider != handle.provider
            || entry.handle.model_id != handle.model_id
            || entry.handle.api != handle.api
        {
            return Err(CallbackError::new(format!(
                "Unknown faux deferred response: {}",
                handle.id
            )));
        }
        if entry.cancelled {
            return Err(CallbackError::new(format!(
                "Faux deferred response was cancelled: {}",
                handle.id
            )));
        }
        if entry.pending_fetches > 0 {
            if let Some(stored) = lock(&self.deferred_responses).get_mut(&handle.id) {
                stored.pending_fetches -= 1;
            }
            return Ok(create_deferred_message(request_model, entry.handle));
        }
        if let Some(final_message) = entry.final_message {
            return Ok(final_message);
        }

        let mut submission_options = entry.options.clone();
        if let Some(options) = submission_options.as_mut() {
            options.deferred = None;
            options.stream.request.signal = None;
            options.stream.request.on_response = None;
        }
        let resolved = self
            .resolve(
                entry.step.clone(),
                &entry.context,
                submission_options.as_ref(),
                &entry.model,
            )
            .await
            .unwrap_or_else(|error| create_error_message(&error.message, &self.api, &self.provider, &entry.model.id));
        if let Some(stored) = lock(&self.deferred_responses).get_mut(&handle.id) {
            stored.final_message = Some(resolved.clone());
        }
        Ok(resolved)
    }

    async fn resolve(
        &self,
        step: FauxResponseStep,
        context: &Context,
        options: Option<&SimpleStreamOptions>,
        request_model: &Model,
    ) -> Result<AssistantMessage, CallbackError> {
        let resolved = match step {
            FauxResponseStep::Message(message) => *message,
            FauxResponseStep::Factory(factory) => {
                let snapshot = {
                    let state = lock(&self.state);
                    FauxProviderStateSnapshot {
                        call_count: state.call_count,
                        deferred_fetch_count: state.deferred_fetch_count,
                    }
                };
                factory(context.clone(), options.cloned(), snapshot, request_model.clone()).await?
            }
        };
        Ok(with_usage_estimate(
            clone_message(resolved, &self.api, &self.provider, &request_model.id),
            context,
            options.map(|options| &options.stream),
            &self.prompt_cache,
        ))
    }
}

impl ProviderStreams for FauxCore {
    fn stream(&self, model: Model, context: Context, options: StreamOptions) -> AssistantMessageEventStream {
        self.stream_with_options(
            model,
            context,
            Some(SimpleStreamOptions {
                stream: options,
                ..SimpleStreamOptions::default()
            }),
        )
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> AssistantMessageEventStream {
        self.stream_with_options(model, context, Some(options))
    }

    fn supports_fetch_deferred(&self) -> bool {
        true
    }

    fn fetch_deferred(
        &self,
        model: Model,
        handle: DeferredHandle,
        options: DeferredFetchOptions,
    ) -> AssistantMessageEventStream {
        self.fetch_deferred_stream(model, handle, options)
    }

    fn supports_cancel_deferred(&self) -> bool {
        true
    }

    fn cancel_deferred(
        &self,
        model: Model,
        handle: DeferredHandle,
        options: DeferredCancelOptions,
    ) -> BoxFuture<'_, Result<(), CallbackError>> {
        let state = Arc::clone(&self.state);
        let deferred_responses = Arc::clone(&self.deferred_responses);
        Box::pin(async move {
            lock(&state).cancelled_deferred.push(handle.clone());
            if let Some(entry) = lock(&deferred_responses).get_mut(&handle.id) {
                entry.cancelled = true;
            }
            if let Some(on_response) = options.on_response {
                on_response(
                    ProviderResponse {
                        status: 200.0,
                        headers: BTreeMap::new(),
                    },
                    model,
                )
                .await?;
            }
            Ok(())
        })
    }
}

/// `fauxProvider()` handle.
pub struct FauxProviderHandle {
    pub provider: Arc<dyn Provider>,
    pub api: String,
    pub models: Vec<Model>,
    core: Arc<FauxCore>,
}

impl FauxProviderHandle {
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
}

struct FauxApiKeyAuth;

impl crate::auth::types::ApiKeyAuth for FauxApiKeyAuth {
    fn name(&self) -> &str {
        "Faux"
    }

    fn supports_login(&self) -> bool {
        false
    }

    fn resolve(
        &self,
        _input: crate::auth::types::ApiKeyResolveInput,
    ) -> BoxFuture<'static, Result<Option<crate::auth::types::AuthResult>, crate::auth::types::AuthError>> {
        Box::pin(async {
            Ok(Some(crate::auth::types::AuthResult {
                auth: crate::auth::types::ModelAuth::default(),
                env: None,
                source: None,
            }))
        })
    }
}

/// `fauxProvider()`.
pub fn faux_provider(options: RegisterFauxProviderOptions) -> FauxProviderHandle {
    let core = Arc::new(create_faux_core(options));
    let provider = create_provider(CreateProviderOptions {
        id: core.provider.clone(),
        name: None,
        base_url: None,
        headers: None,
        auth: crate::auth::types::ProviderAuth::api_key(Arc::new(FauxApiKeyAuth)),
        models: core.models.clone(),
        fetch_models: None,
        filter_models: None,
        api: ProviderApi::Single(Arc::clone(&core) as Arc<dyn ProviderStreams>),
    });
    FauxProviderHandle {
        provider,
        api: core.api.clone(),
        models: core.models.clone(),
        core,
    }
}

fn clone_message(mut message: AssistantMessage, api: &str, provider: &str, model_id: &str) -> AssistantMessage {
    message.api = api.to_owned();
    message.provider = provider.to_owned();
    message.model = model_id.to_owned();
    if message.timestamp == 0 {
        message.timestamp = now_millis();
    }
    message
}

fn create_deferred_message(model: &Model, handle: DeferredHandle) -> AssistantMessage {
    let mut message = AssistantMessage::empty(&model.api, &model.provider, &model.id, now_millis());
    message.usage = default_usage();
    message.stop_reason = StopReason::Deferred;
    message.deferred = Some(handle);
    message
}

fn create_error_message(error: &str, api: &str, provider: &str, model_id: &str) -> AssistantMessage {
    let mut message = AssistantMessage::empty(api, provider, model_id, now_millis());
    message.usage = default_usage();
    message.stop_reason = StopReason::Error;
    message.error_message = Some(error.to_owned());
    message
}

fn create_aborted_message(partial: &AssistantMessage) -> AssistantMessage {
    let mut message = partial.clone();
    message.stop_reason = StopReason::Aborted;
    message.error_message = Some("Request was aborted".to_owned());
    message.timestamp = now_millis();
    message
}

async fn stream_with_deltas(
    sender: &AssistantMessageEventSender,
    message: AssistantMessage,
    min_token_size: usize,
    max_token_size: usize,
    tokens_per_second: Option<f64>,
    signal: Option<&CancellationToken>,
) -> Result<(), CallbackError> {
    let mut partial = message.clone();
    partial.content = Vec::new();
    partial.stop_reason = StopReason::Pending;

    if signal.is_some_and(CancellationToken::is_cancelled) {
        let aborted = create_aborted_message(&partial);
        sender.push(AssistantMessageEvent::Error {
            reason: ErrorReason::Aborted,
            error: aborted.clone(),
        });
        sender.end(Some(aborted));
        return Ok(());
    }

    sender.push(AssistantMessageEvent::Start {
        partial: partial.clone(),
    });

    for (index, block) in message.content.iter().enumerate() {
        if signal.is_some_and(CancellationToken::is_cancelled) {
            let aborted = create_aborted_message(&partial);
            sender.push(AssistantMessageEvent::Error {
                reason: ErrorReason::Aborted,
                error: aborted.clone(),
            });
            sender.end(Some(aborted));
            return Ok(());
        }
        match block {
            AssistantContent::Thinking(block) => {
                partial
                    .content
                    .push(AssistantContent::Thinking(ThinkingContent::new("")));
                sender.push(AssistantMessageEvent::ThinkingStart {
                    content_index: index,
                    partial: partial.clone(),
                });
                for chunk in split_string_by_token_size(&block.thinking, min_token_size, max_token_size) {
                    schedule_chunk(&chunk, tokens_per_second).await;
                    if signal.is_some_and(CancellationToken::is_cancelled) {
                        let aborted = create_aborted_message(&partial);
                        sender.push(AssistantMessageEvent::Error {
                            reason: ErrorReason::Aborted,
                            error: aborted.clone(),
                        });
                        sender.end(Some(aborted));
                        return Ok(());
                    }
                    append_thinking(&mut partial, index, &chunk);
                    sender.push(AssistantMessageEvent::ThinkingDelta {
                        content_index: index,
                        delta: chunk,
                        partial: partial.clone(),
                    });
                }
                sender.push(AssistantMessageEvent::ThinkingEnd {
                    content_index: index,
                    content: block.thinking.clone(),
                    partial: partial.clone(),
                });
            }
            AssistantContent::Text(block) => {
                partial.content.push(AssistantContent::Text(TextContent::new("")));
                sender.push(AssistantMessageEvent::TextStart {
                    content_index: index,
                    partial: partial.clone(),
                });
                for chunk in split_string_by_token_size(&block.text, min_token_size, max_token_size) {
                    schedule_chunk(&chunk, tokens_per_second).await;
                    if signal.is_some_and(CancellationToken::is_cancelled) {
                        let aborted = create_aborted_message(&partial);
                        sender.push(AssistantMessageEvent::Error {
                            reason: ErrorReason::Aborted,
                            error: aborted.clone(),
                        });
                        sender.end(Some(aborted));
                        return Ok(());
                    }
                    append_text(&mut partial, index, &chunk);
                    sender.push(AssistantMessageEvent::TextDelta {
                        content_index: index,
                        delta: chunk,
                        partial: partial.clone(),
                    });
                }
                sender.push(AssistantMessageEvent::TextEnd {
                    content_index: index,
                    content: block.text.clone(),
                    partial: partial.clone(),
                });
            }
            AssistantContent::ToolCall(block) => {
                partial.content.push(AssistantContent::ToolCall(ToolCall::new(
                    block.id.clone(),
                    block.name.clone(),
                    JsonObject::new(),
                )));
                sender.push(AssistantMessageEvent::ToolCallStart {
                    content_index: index,
                    partial: partial.clone(),
                });
                let arguments = serde_json::to_string(&block.arguments).unwrap_or_default();
                let chunks = split_string_by_token_size(&arguments, min_token_size, max_token_size);
                let last_chunk = chunks.len().saturating_sub(1);
                for (chunk_index, chunk) in chunks.iter().enumerate() {
                    schedule_chunk(chunk, tokens_per_second).await;
                    if signal.is_some_and(CancellationToken::is_cancelled) {
                        let aborted = create_aborted_message(&partial);
                        sender.push(AssistantMessageEvent::Error {
                            reason: ErrorReason::Aborted,
                            error: aborted.clone(),
                        });
                        sender.end(Some(aborted));
                        return Ok(());
                    }
                    // TypeScript assigns the final arguments synchronously right after
                    // the last delta; listeners observe them on that delta's partial.
                    if chunk_index == last_chunk
                        && let Some(AssistantContent::ToolCall(partial_call)) = partial.content.get_mut(index)
                    {
                        partial_call.arguments = block.arguments.clone();
                    }
                    sender.push(AssistantMessageEvent::ToolCallDelta {
                        content_index: index,
                        delta: chunk.clone(),
                        partial: partial.clone(),
                    });
                }
                if let Some(AssistantContent::ToolCall(partial_call)) = partial.content.get_mut(index) {
                    partial_call.arguments = block.arguments.clone();
                }
                sender.push(AssistantMessageEvent::ToolCallEnd {
                    content_index: index,
                    tool_call: block.clone(),
                    partial: partial.clone(),
                });
            }
        }
    }

    match message.stop_reason {
        StopReason::Pending => Err(CallbackError::new("Faux response ended without a stop reason")),
        StopReason::Error => {
            sender.push(AssistantMessageEvent::Error {
                reason: ErrorReason::Error,
                error: message.clone(),
            });
            sender.end(Some(message));
            Ok(())
        }
        StopReason::Aborted => {
            sender.push(AssistantMessageEvent::Error {
                reason: ErrorReason::Aborted,
                error: message.clone(),
            });
            sender.end(Some(message));
            Ok(())
        }
        reason => {
            let done_reason = match reason {
                StopReason::Stop => DoneReason::Stop,
                StopReason::Length => DoneReason::Length,
                StopReason::ToolUse => DoneReason::ToolUse,
                StopReason::Deferred => DoneReason::Deferred,
                StopReason::Pending | StopReason::Error | StopReason::Aborted => unreachable!(),
            };
            sender.push(AssistantMessageEvent::Done {
                reason: done_reason,
                message: message.clone(),
            });
            sender.end(Some(message));
            Ok(())
        }
    }
}

fn append_text(partial: &mut AssistantMessage, index: usize, chunk: &str) {
    if let Some(AssistantContent::Text(block)) = partial.content.get_mut(index) {
        block.text.push_str(chunk);
    }
}

fn append_thinking(partial: &mut AssistantMessage, index: usize, chunk: &str) {
    if let Some(AssistantContent::Thinking(block)) = partial.content.get_mut(index) {
        block.thinking.push_str(chunk);
    }
}

async fn schedule_chunk(chunk: &str, tokens_per_second: Option<f64>) {
    match tokens_per_second {
        Some(tokens_per_second) if tokens_per_second > 0.0 => {
            let delay_ms = (estimate_tokens(chunk) / tokens_per_second) * 1000.0;
            tokio::time::sleep(Duration::from_secs_f64(delay_ms / 1000.0)).await;
        }
        _ => tokio::task::yield_now().await,
    }
}

fn split_string_by_token_size(text: &str, min_token_size: usize, max_token_size: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let chars: Vec<char> = text.chars().collect();
    let mut index = 0;
    while index < chars.len() {
        let span = max_token_size.saturating_sub(min_token_size) + 1;
        let token_size = min_token_size + (next_random() as usize % span);
        let char_size = (token_size * 4).max(1);
        let end = (index + char_size).min(chars.len());
        chunks.push(chars[index..end].iter().collect());
        index = end;
    }
    if chunks.is_empty() {
        chunks.push(String::new());
    }
    chunks
}

fn with_usage_estimate(
    mut message: AssistantMessage,
    context: &Context,
    options: Option<&StreamOptions>,
    prompt_cache: &Mutex<HashMap<String, String>>,
) -> AssistantMessage {
    let prompt_text = serialize_context(context);
    let prompt_tokens = estimate_tokens(&prompt_text);
    let output_tokens = estimate_tokens(&assistant_content_to_text(&message.content));
    let mut input = prompt_tokens;
    let mut cache_read = 0.0;
    let mut cache_write = 0.0;
    let session_id = options.and_then(|options| options.session_id.clone());
    let caching_enabled = options
        .and_then(|options| options.cache_retention)
        .is_none_or(|retention| retention != CacheRetention::None);

    if let Some(session_id) = session_id
        && caching_enabled
    {
        let previous_prompt = lock(prompt_cache).get(&session_id).cloned();
        if let Some(previous_prompt) = previous_prompt {
            let cached_chars = common_prefix_length(&previous_prompt, &prompt_text);
            cache_read = estimate_tokens(&previous_prompt[..cached_chars]);
            cache_write = estimate_tokens(&prompt_text[cached_chars..]);
            input = (prompt_tokens - cache_read).max(0.0);
        } else {
            cache_write = prompt_tokens;
        }
        lock(prompt_cache).insert(session_id, prompt_text);
    }

    message.usage = Usage {
        input,
        output: output_tokens,
        cache_read,
        cache_write,
        cache_write_1h: None,
        reasoning: None,
        total_tokens: input + output_tokens + cache_read + cache_write,
        cost: UsageCost::default(),
    };
    message
}

fn estimate_tokens(text: &str) -> f64 {
    let units = text.encode_utf16().count();
    (units as f64 / 4.0).ceil()
}

fn common_prefix_length(a: &str, b: &str) -> usize {
    let mut length = 0;
    for (left, right) in a.chars().zip(b.chars()) {
        if left != right {
            break;
        }
        length += left.len_utf8();
    }
    length
}

fn content_to_text(content: &UserContent) -> String {
    match content {
        UserContent::Text(text) => text.clone(),
        UserContent::Blocks(blocks) => blocks
            .iter()
            .map(|block| match block {
                TextOrImageContent::Text(block) => block.text.clone(),
                TextOrImageContent::Image(block) => {
                    format!("[image:{}:{}]", block.mime_type, block.data.encode_utf16().count())
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn assistant_content_to_text(content: &[AssistantContent]) -> String {
    content
        .iter()
        .map(|block| match block {
            AssistantContent::Text(block) => block.text.clone(),
            AssistantContent::Thinking(block) => block.thinking.clone(),
            AssistantContent::ToolCall(block) => format!(
                "{}:{}",
                block.name,
                serde_json::to_string(&block.arguments).unwrap_or_default()
            ),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn tool_result_to_text(message: &crate::types::ToolResultMessage) -> String {
    let mut parts = vec![message.tool_name.clone()];
    for block in &message.content {
        parts.push(match block {
            TextOrImageContent::Text(block) => block.text.clone(),
            TextOrImageContent::Image(block) => {
                format!("[image:{}:{}]", block.mime_type, block.data.encode_utf16().count())
            }
        });
    }
    parts.join("\n")
}

fn message_to_text(message: &Message) -> String {
    match message {
        Message::User(message) => content_to_text(&message.content),
        Message::Assistant(message) => assistant_content_to_text(&message.content),
        Message::ToolResult(message) => tool_result_to_text(message),
    }
}

fn serialize_context(context: &Context) -> String {
    let mut parts = Vec::new();
    if let Some(system_prompt) = &context.system_prompt {
        parts.push(format!("system:{system_prompt}"));
    }
    for message in &context.messages {
        let role = match message {
            Message::User(_) => "user",
            Message::Assistant(_) => "assistant",
            Message::ToolResult(_) => "toolResult",
        };
        parts.push(format!("{role}:{}", message_to_text(message)));
    }
    if let Some(tools) = &context.tools
        && !tools.is_empty()
    {
        parts.push(format!("tools:{}", serde_json::to_string(tools).unwrap_or_default()));
    }
    parts.join("\n\n")
}

fn random_id(prefix: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}:{}:{counter}", now_millis())
}

fn next_random() -> u64 {
    static STATE: AtomicU64 = AtomicU64::new(0x9e37_79b9_7f4a_7c15);
    let mut current = STATE.load(Ordering::Relaxed);
    current ^= current << 13;
    current ^= current >> 7;
    current ^= current << 17;
    STATE.store(current, Ordering::Relaxed);
    current
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
