//! Port of `packages/ai/src/providers/faux.ts`.
//!
//! Deterministic fake provider used to exercise `Models` without network
//! access. Token chunking and generated ids are not bit-identical to the
//! TypeScript `Math.random` values; tests fix `tokenSize` and tool-call ids.

use pi_js_compat::js_json::JsString;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use pi_js_compat::abort::AbortSignal;

use crate::models::{CreateProviderOptions, Provider, ProviderApi, create_provider};
use crate::request::{BoxFuture, CallbackError, ProviderStreams, SimpleStreamOptions, StreamOptions, now_millis};
use crate::types::{
    AssistantContent, AssistantMessage, AssistantMessageEvent, CacheRetention, Context, DoneReason, ErrorReason,
    Message, Modality, Model, ModelCostRates, ProviderResponse, StopReason, TextContent, TextOrImageContent,
    ThinkingContent, ToolCall, Usage, UsageCost, UserContent,
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
    pub name: Option<JsString>,
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

#[derive(Debug, Clone, Default)]
pub struct RegisterFauxProviderOptions {
    pub api: Option<String>,
    pub provider: Option<String>,
    pub models: Option<Vec<FauxModelDefinition>>,
    pub tokens_per_second: Option<f64>,
    pub token_size: Option<FauxTokenSize>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum FauxContent {
    Text(JsString),
    Block(Box<AssistantContent>),
    Blocks(Vec<AssistantContent>),
}

impl From<&str> for FauxContent {
    fn from(text: &str) -> Self {
        Self::Text(text.into())
    }
}

impl From<String> for FauxContent {
    fn from(text: String) -> Self {
        Self::Text(text.into())
    }
}

impl From<JsString> for FauxContent {
    fn from(text: JsString) -> Self {
        Self::Text(text)
    }
}

impl From<AssistantContent> for FauxContent {
    fn from(block: AssistantContent) -> Self {
        Self::Block(Box::new(block))
    }
}

impl From<Vec<AssistantContent>> for FauxContent {
    fn from(blocks: Vec<AssistantContent>) -> Self {
        Self::Blocks(blocks)
    }
}

pub fn faux_text(text: impl Into<JsString>) -> AssistantContent {
    AssistantContent::Text(TextContent::new(text))
}

pub fn faux_thinking(thinking: impl Into<JsString>) -> AssistantContent {
    AssistantContent::Thinking(ThinkingContent::new(thinking))
}

pub fn faux_tool_call(
    name: impl Into<String>,
    arguments: pi_js_compat::js_json::JsObject,
    id: Option<String>,
) -> AssistantContent {
    AssistantContent::ToolCall(ToolCall::new(id.unwrap_or_else(|| random_id("tool")), name, arguments))
}

#[derive(Debug, Clone, Default)]
pub struct FauxAssistantMessageOptions {
    pub stop_reason: Option<StopReason>,
    pub error_message: Option<pi_js_compat::js_json::JsString>,
    pub response_id: Option<String>,
    pub timestamp: Option<f64>,
}

/// `fauxAssistantMessage()`.
pub fn faux_assistant_message(
    content: impl Into<FauxContent>,
    options: FauxAssistantMessageOptions,
) -> AssistantMessage {
    let content = match content.into() {
        FauxContent::Text(text) => vec![faux_text(text)],
        FauxContent::Block(block) => vec![*block],
        FauxContent::Blocks(blocks) => blocks,
    };
    let mut message = AssistantMessage::empty(DEFAULT_API, DEFAULT_PROVIDER, DEFAULT_MODEL_ID, 0.0);
    message.content = content;
    message.usage = default_usage();
    message.stop_reason = options.stop_reason.unwrap_or(StopReason::Stop);
    message.error_message = options.error_message;
    message.response_id = options.response_id;
    message.timestamp = options.timestamp.unwrap_or_else(|| now_millis() as f64);
    message
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FauxProviderState {
    pub call_count: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct FauxProviderStateSnapshot {
    pub call_count: u64,
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

pub struct FauxCore {
    api: String,
    provider: String,
    models: Vec<Model>,
    min_token_size: f64,
    max_token_size: f64,
    tokens_per_second: Option<f64>,
    pending_responses: Arc<Mutex<Vec<FauxResponseStep>>>,
    state: Arc<Mutex<FauxProviderState>>,
    prompt_cache: Arc<Mutex<HashMap<JsString, JsString>>>,
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
        .max(1.0);
    let max_token_size = options
        .token_size
        .and_then(|size| size.max)
        .unwrap_or(DEFAULT_MAX_TOKEN_SIZE as f64)
        .max(min_token_size);

    let definitions = options.models.filter(|models| !models.is_empty()).unwrap_or_else(|| {
        vec![FauxModelDefinition {
            id: DEFAULT_MODEL_ID.to_owned(),
            name: Some(DEFAULT_MODEL_NAME.into()),
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
            properties: Default::default(),
            id: definition.id.clone(),
            name: definition.name.unwrap_or_else(|| definition.id.clone().into()),
            api: api.clone(),
            provider: provider.clone(),
            base_url: DEFAULT_BASE_URL.into(),
            reasoning: definition.reasoning.unwrap_or(false),
            thinking_level_map: None,
            input: definition
                .input
                .unwrap_or_else(|| vec![Modality::Text, Modality::Image]),
            cost: crate::types::ModelCost {
                properties: Default::default(),
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
            None | Some("") => self.models.first().cloned(),
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
        let resolver = FauxResolver {
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
                        headers: Default::default(),
                    },
                    request_model.clone(),
                )
                .await
            {
                let message = create_error_message(
                    callback_error_text(error),
                    &resolver.api,
                    &resolver.provider,
                    &request_model.id,
                );
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
                        let message = create_error_message(
                            callback_error_text(error),
                            &resolver.api,
                            &resolver.provider,
                            &request_model.id,
                        );
                        sender.push(AssistantMessageEvent::Error {
                            reason: ErrorReason::Error,
                            error: message.clone(),
                        });
                        sender.end(Some(message));
                    }
                }
                Err(error) => {
                    let message = create_error_message(
                        callback_error_text(error),
                        &resolver.api,
                        &resolver.provider,
                        &request_model.id,
                    );
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
    prompt_cache: Arc<Mutex<HashMap<JsString, JsString>>>,
    state: Arc<Mutex<FauxProviderState>>,
    api: String,
    provider: String,
}

impl FauxResolver {
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
    fn stream(
        &self,
        model: Model,
        context: Context,
        options: StreamOptions,
    ) -> Result<AssistantMessageEventStream, crate::request::CallbackError> {
        Ok(self.stream_with_options(
            model,
            context,
            Some(SimpleStreamOptions {
                stream: options,
                ..SimpleStreamOptions::default()
            }),
        ))
    }

    fn stream_simple(
        &self,
        model: Model,
        context: Context,
        options: SimpleStreamOptions,
    ) -> Result<AssistantMessageEventStream, crate::request::CallbackError> {
        Ok(self.stream_with_options(model, context, Some(options)))
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
    fn name(&self) -> pi_js_compat::js_json::JsString {
        "Faux".into()
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
        refresh_models: None,
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
    message
}

fn callback_error_text(error: CallbackError) -> JsString {
    error
        .error_or_string_message()
        .unwrap_or_else(|error| pi_js_compat::callback_error::throw_callback_uncaught(error))
}

fn create_error_message(
    error: impl Into<pi_js_compat::js_json::JsString>,
    api: &str,
    provider: &str,
    model_id: &str,
) -> AssistantMessage {
    let mut message = AssistantMessage::empty(api, provider, model_id, now_millis() as f64);
    message.usage = default_usage();
    message.stop_reason = StopReason::Error;
    message.error_message = Some(error.into());
    message
}

fn create_aborted_message(partial: &AssistantMessage) -> AssistantMessage {
    let mut message = partial.clone();
    message.stop_reason = StopReason::Aborted;
    message.error_message = Some("Request was aborted".to_owned().into());
    message.timestamp = now_millis() as f64;
    message
}

async fn stream_with_deltas(
    sender: &AssistantMessageEventSender,
    message: AssistantMessage,
    min_token_size: f64,
    max_token_size: f64,
    tokens_per_second: Option<f64>,
    signal: Option<&AbortSignal>,
) -> Result<(), CallbackError> {
    let mut partial = message.clone();
    partial.content = Vec::new();

    if signal.is_some_and(AbortSignal::aborted) {
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
        if signal.is_some_and(AbortSignal::aborted) {
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
                for chunk in split_text_by_token_size(&block.thinking, min_token_size, max_token_size) {
                    schedule_chunk(&chunk, tokens_per_second).await;
                    if signal.is_some_and(AbortSignal::aborted) {
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
                for chunk in split_text_by_token_size(&block.text, min_token_size, max_token_size) {
                    schedule_chunk(&chunk, tokens_per_second).await;
                    if signal.is_some_and(AbortSignal::aborted) {
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
                    pi_js_compat::js_json::JsObject::new(),
                )));
                sender.push(AssistantMessageEvent::ToolCallStart {
                    content_index: index,
                    partial: partial.clone(),
                });
                let arguments = pi_js_compat::js_json::stringify_lossless(&pi_js_compat::js_json::JsValue::Object(
                    block.arguments.clone(),
                ));
                let chunks = split_text_by_token_size(&JsString::from(arguments), min_token_size, max_token_size);
                for chunk in &chunks {
                    schedule_chunk(chunk, tokens_per_second).await;
                    if signal.is_some_and(AbortSignal::aborted) {
                        let aborted = create_aborted_message(&partial);
                        sender.push(AssistantMessageEvent::Error {
                            reason: ErrorReason::Aborted,
                            error: aborted.clone(),
                        });
                        sender.end(Some(aborted));
                        return Ok(());
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
                    tool_call: Box::new(block.clone()),
                    partial: partial.clone(),
                });
            }
        }
    }

    let event = match message.stop_reason {
        StopReason::Error => AssistantMessageEvent::Error {
            reason: ErrorReason::Error,
            error: message.clone(),
        },
        StopReason::Aborted => AssistantMessageEvent::Error {
            reason: ErrorReason::Aborted,
            error: message.clone(),
        },
        StopReason::Stop => completed_event(DoneReason::Stop, &message),
        StopReason::Length => completed_event(DoneReason::Length, &message),
        StopReason::ToolUse => completed_event(DoneReason::ToolUse, &message),
    };
    sender.push(event);
    sender.end(Some(message));
    Ok(())
}

fn completed_event(reason: DoneReason, message: &AssistantMessage) -> AssistantMessageEvent {
    AssistantMessageEvent::Done {
        reason,
        message: message.clone(),
    }
}

fn append_text(partial: &mut AssistantMessage, index: usize, chunk: &JsString) {
    if let Some(AssistantContent::Text(block)) = partial.content.get_mut(index) {
        block.text.push_str(chunk);
    }
}

fn append_thinking(partial: &mut AssistantMessage, index: usize, chunk: &JsString) {
    if let Some(AssistantContent::Thinking(block)) = partial.content.get_mut(index) {
        block.thinking.push_str(chunk);
    }
}

async fn schedule_chunk(chunk: impl Into<JsString>, tokens_per_second: Option<f64>) {
    match tokens_per_second {
        Some(tokens_per_second) if tokens_per_second > 0.0 => {
            let delay_ms = (estimate_tokens(chunk) / tokens_per_second) * 1000.0;
            tokio::time::sleep(Duration::from_secs_f64(delay_ms / 1000.0)).await;
        }
        _ => tokio::task::yield_now().await,
    }
}

fn split_text_by_token_size(text: &JsString, min_token_size: f64, max_token_size: f64) -> Vec<JsString> {
    let mut chunks = Vec::new();
    let mut index = 0.0;
    while index < text.len() as f64 {
        let random = (next_random() >> 11) as f64 / (1_u64 << 53) as f64;
        let token_size = min_token_size + (random * (max_token_size - min_token_size + 1.0)).floor();
        let end = index + (token_size * 4.0).max(1.0);
        let start_unit = (index as usize).min(text.len());
        let end_unit = (end as usize).min(text.len());
        chunks.push(JsString::from_utf16(text.units()[start_unit..end_unit].to_vec()));
        index = end;
    }
    if chunks.is_empty() {
        chunks.push(Default::default());
    }
    chunks
}

fn with_usage_estimate(
    mut message: AssistantMessage,
    context: &Context,
    options: Option<&StreamOptions>,
    prompt_cache: &Mutex<HashMap<JsString, JsString>>,
) -> AssistantMessage {
    let prompt_text = serialize_context(context);
    let prompt_tokens = estimate_tokens(&prompt_text);
    let output_tokens = estimate_tokens(assistant_content_to_text(&message.content));
    let mut input = prompt_tokens;
    let mut cache_read = 0.0;
    let mut cache_write = 0.0;
    let session_id = options.and_then(|options| options.session_id.clone());
    let caching_enabled = options
        .and_then(|options| options.cache_retention)
        .is_none_or(|retention| retention != CacheRetention::None);

    if let Some(session_id) = session_id.filter(|id| !id.is_empty())
        && caching_enabled
    {
        let previous_prompt = lock(prompt_cache).get(&session_id).cloned();
        if let Some(previous_prompt) = previous_prompt {
            let cached_chars = common_prefix_length(&previous_prompt, &prompt_text);
            cache_read = (cached_chars as f64 / 4.0).ceil();
            cache_write = ((prompt_text.len() - cached_chars) as f64 / 4.0).ceil();
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

fn estimate_tokens(text: impl Into<JsString>) -> f64 {
    let units = text.into().len();
    (units as f64 / 4.0).ceil()
}

fn common_prefix_length(a: &JsString, b: &JsString) -> usize {
    a.units().iter().zip(b.units()).take_while(|(a, b)| a == b).count()
}

fn content_to_text(content: &UserContent) -> JsString {
    match content {
        UserContent::Text(text) => text.into(),
        UserContent::Blocks(blocks) => JsString::join(blocks.iter().map(block_to_text), "\n"),
    }
}

fn block_to_text(block: &TextOrImageContent) -> JsString {
    match block {
        TextOrImageContent::Text(block) => block.text.clone(),
        TextOrImageContent::Image(block) => {
            format!("[image:{}:{}]", block.mime_type, block.data.encode_utf16().count()).into()
        }
    }
}

fn assistant_content_to_text(content: &[AssistantContent]) -> JsString {
    JsString::join(
        content.iter().map(|block| match block {
            AssistantContent::Text(block) => block.text.clone(),
            AssistantContent::Thinking(block) => block.thinking.clone(),
            AssistantContent::ToolCall(block) => format!(
                "{}:{}",
                block.name,
                pi_js_compat::js_json::stringify_lossless(&pi_js_compat::js_json::JsValue::Object(
                    block.arguments.clone()
                ))
            )
            .into(),
        }),
        "\n",
    )
}

fn tool_result_to_text(message: &crate::types::ToolResultMessage) -> JsString {
    let parts = std::iter::once(message.tool_name.clone().into()).chain(message.content.iter().map(block_to_text));
    JsString::join(parts, "\n")
}

fn message_to_text(message: &Message) -> JsString {
    match message {
        Message::User(message) => content_to_text(&message.content),
        Message::Assistant(message) => assistant_content_to_text(&message.content),
        Message::ToolResult(message) => tool_result_to_text(message),
    }
}

fn serialize_context(context: &Context) -> JsString {
    let mut parts = Vec::new();
    if let Some(system_prompt) = context.system_prompt.as_ref().filter(|prompt| !prompt.is_empty()) {
        parts.push(JsString::from("system:") + system_prompt);
    }
    for message in &context.messages {
        let role = match message {
            Message::User(_) => "user",
            Message::Assistant(_) => "assistant",
            Message::ToolResult(_) => "toolResult",
        };
        parts.push(JsString::join(
            [format!("{role}:").into(), message_to_text(message)],
            "",
        ));
    }
    if let Some(tools) = &context.tools
        && !tools.is_empty()
    {
        parts.push(format!("tools:{}", serde_json::to_string(tools).unwrap_or_default()).into());
    }
    JsString::join(parts, "\n\n")
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
