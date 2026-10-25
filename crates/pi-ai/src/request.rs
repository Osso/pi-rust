//! Request-side contracts from `packages/ai/src/types.ts`: provider request
//! options, stream options, callbacks, and the `fetch` replacement.
//!
//! TypeScript `ProviderStreamOptions = StreamOptions & Record<string, unknown>`
//! carries API-specific fields untyped through generic dispatch; Rust keeps them
//! in [`StreamOptions::extra`] and each API module parses its typed options from it.

use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::JsValue;
use pi_js_compat::node_error::{NativeErrorClass, NativeNodeError};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use pi_js_compat::abort::AbortSignal;

use crate::types::{AssistantMessage, AssistantMessageEvent, ErrorReason, StopReason};
use crate::types::{
    CacheRetention, Context, ImagesContext, ImagesModel, JsonObject, Model, ProviderEnv, ProviderHeaders,
    ProviderResponse, ProviderRetryEvent, ThinkingBudgets, ThinkingLevel, Transport,
};
use crate::utils::event_stream::{AssistantMessageEventStream, create_assistant_message_event_stream};

/// Boxed `Send` future used by dyn-compatible async contracts.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

pub use pi_js_compat::callback_error::CallbackError;

/// `onPayload(payload, model)`: `Ok(None)` keeps the payload, `Ok(Some(v))` replaces it.
pub type OnPayload =
    Arc<dyn Fn(JsValue, Model) -> BoxFuture<'static, Result<Option<JsValue>, CallbackError>> + Send + Sync>;
/// `onResponse(response, model)`, awaited before the body is consumed.
pub type OnResponse =
    Arc<dyn Fn(ProviderResponse, Model) -> BoxFuture<'static, Result<(), CallbackError>> + Send + Sync>;
/// `onRetry(event, model)`.
pub type OnRetry = Arc<dyn Fn(ProviderRetryEvent, &Model) -> Result<(), CallbackError> + Send + Sync>;

/// One HTTP request handed to a [`Fetch`] implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    /// Header pairs in send order.
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
}

/// WebIDL ByteString conversion performed when constructing HTTP headers.
/// Content remains native before this boundary; errors identify the original unit.
pub fn header_value(value: &JsString) -> Result<String, CallbackError> {
    if let Some((index, unit)) = value.units().iter().enumerate().find(|(_, unit)| **unit > 255) {
        return Err(NativeNodeError::from_constructor(
            NativeErrorClass::TypeError,
            format!("Cannot convert argument to a ByteString because the character at index {index} has a value of {unit} which is greater than 255."),
        ).into());
    }
    Ok(value.to_utf8().expect("ByteString units are Unicode scalars"))
}

/// Wire bytes of a ByteString header value: each unit 0-255 is one byte (undici/Node `http`), never UTF-8.
pub(crate) fn header_bytes(value: &JsString) -> Result<Vec<u8>, CallbackError> {
    header_value(value)?;
    Ok(value.units().iter().map(|unit| *unit as u8).collect())
}

/// WHATWG URL input conversion at HTTP request construction.
pub fn request_url(value: &JsString) -> Result<String, CallbackError> {
    let input = pi_js_compat::js_json::encode_node_utf8(value);
    url::Url::parse(&input).map(String::from).map_err(|_| {
        NativeNodeError::from_constructor(
            NativeErrorClass::TypeError,
            pi_js_compat::js_format!("Failed to parse URL from {}", value),
        )
        .into()
    })
}

/// Streaming response body.
pub trait HttpBody: Send {
    /// Next body chunk; `None` at end of body.
    fn next_chunk(&mut self) -> BoxFuture<'_, Option<Result<Vec<u8>, CallbackError>>>;
}

pub struct HttpResponse {
    pub status: u16,
    /// Fetch `statusText`. The default client reports the canonical reason phrase
    /// for the status code (HTTP/2 responses carry none).
    pub status_text: String,
    /// Header pairs with lower-case names.
    pub headers: Vec<(String, String)>,
    pub body: Box<dyn HttpBody>,
}

/// Replacement for the `fetch` option (`typeof globalThis.fetch`).
pub trait Fetch: Send + Sync {
    fn fetch(
        &self,
        request: HttpRequest,
        signal: Option<AbortSignal>,
    ) -> BoxFuture<'_, Result<HttpResponse, CallbackError>>;
}

/// Reads a streaming response body as UTF-8 text (`Response#text`).
pub async fn read_body_text(mut body: Box<dyn HttpBody>) -> Result<String, CallbackError> {
    let mut bytes = Vec::new();
    while let Some(chunk) = body.next_chunk().await {
        bytes.extend_from_slice(&chunk?);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// `ProviderRequestOptions`: auth, transport, and lifecycle callbacks shared by provider requests.
#[derive(Clone, Default)]
pub struct ProviderRequestOptions {
    pub signal: Option<AbortSignal>,
    pub api_key: Option<JsString>,
    /// Custom HTTP implementation; `None` uses the crate default client.
    pub fetch: Option<Arc<dyn Fetch>>,
    /// Provider-scoped environment values; take precedence over the process environment.
    pub env: Option<ProviderEnv>,
    pub on_payload: Option<OnPayload>,
    pub on_response: Option<OnResponse>,
    /// Merged over provider defaults; `None` values suppress a default header.
    pub headers: Option<ProviderHeaders>,
    pub timeout_ms: Option<f64>,
    pub max_retries: Option<f64>,
    /// Cap on server-requested retry delay. Default 60000; 0 disables the cap.
    pub max_retry_delay_ms: Option<f64>,
}

/// `StreamOptions` plus the untyped API-specific remainder.
#[derive(Clone, Default)]
pub struct StreamOptions {
    /// Caller working directory for local/subprocess providers, never a server payload field.
    pub cwd: Option<JsString>,
    pub request: ProviderRequestOptions,
    pub temperature: Option<f64>,
    pub max_tokens: Option<f64>,
    pub transport: Option<Transport>,
    /// Default "short".
    pub cache_retention: Option<CacheRetention>,
    pub session_id: Option<JsString>,
    pub on_retry: Option<OnRetry>,
    pub websocket_connect_timeout_ms: Option<f64>,
    pub metadata: Option<pi_js_compat::js_json::JsObject>,
    /// API-specific fields (`AnthropicOptions`, `OpenAICompletionsOptions`, ...) in camelCase JSON.
    pub extra: JsonObject,
}

/// `SimpleStreamOptions`: unified reasoning options for `streamSimple()`.
#[derive(Clone, Default)]
pub struct SimpleStreamOptions {
    pub stream: StreamOptions,
    pub reasoning: Option<ThinkingLevel>,
    pub thinking_budgets: Option<ThinkingBudgets>,
}

/// `onPayload(payload, model)` for image requests.
pub type ImagesOnPayload =
    Arc<dyn Fn(JsValue, ImagesModel) -> BoxFuture<'static, Result<Option<JsValue>, CallbackError>> + Send + Sync>;
/// `onResponse(response, model)` for image requests.
pub type ImagesOnResponse =
    Arc<dyn Fn(ProviderResponse, ImagesModel) -> BoxFuture<'static, Result<(), CallbackError>> + Send + Sync>;

/// `ProviderRequestOptions<ImagesModel>`: the image-side request contract.
///
/// TypeScript parameterizes `ProviderRequestOptions` over the model type; Rust
/// keeps a distinct struct so image callbacks receive `ImagesModel` rather than
/// the chat `Model`.
#[derive(Clone, Default)]
pub struct ImagesRequestOptions {
    pub signal: Option<AbortSignal>,
    pub api_key: Option<JsString>,
    /// Custom HTTP implementation; `None` uses the crate default client.
    pub fetch: Option<Arc<dyn Fetch>>,
    /// Provider-scoped environment values; take precedence over the process environment.
    pub env: Option<ProviderEnv>,
    pub on_payload: Option<ImagesOnPayload>,
    pub on_response: Option<ImagesOnResponse>,
    /// Merged over provider defaults; `None` values suppress a default header.
    pub headers: Option<ProviderHeaders>,
    pub timeout_ms: Option<f64>,
    pub max_retries: Option<f64>,
    /// Cap on server-requested retry delay. Default 60000; 0 disables the cap.
    pub max_retry_delay_ms: Option<f64>,
}

/// `ImagesOptions`.
#[derive(Clone, Default)]
pub struct ImagesOptions {
    pub request: ImagesRequestOptions,
    pub metadata: Option<pi_js_compat::js_json::JsObject>,
    pub extra: JsonObject,
}

/// `ProviderStreams`: the uniform stream contract of an API implementation module.
///
/// Stream methods return synchronously and preserve throws before a stream
/// exists. Lazy modules catch these rejections inside their forwarding task.
pub trait ProviderStreams: Send + Sync {
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

/// `ProviderImages`: image-generation API contract.
pub trait ProviderImages: Send + Sync {
    fn generate_images(
        &self,
        model: ImagesModel,
        context: ImagesContext,
        options: ImagesOptions,
    ) -> BoxFuture<'_, Result<crate::types::AssistantImages, CallbackError>>;
}

/// A stream that immediately terminates with an `error` event for `model`
/// (the shape `lazyStream` produces when setup fails).
pub fn error_stream(model: &Model, message: impl Into<pi_js_compat::js_json::JsString>) -> AssistantMessageEventStream {
    let (sender, stream) = create_assistant_message_event_stream();
    let mut error = AssistantMessage::empty(&model.api, &model.provider, &model.id, now_millis() as f64);
    error.stop_reason = StopReason::Error;
    error.error_message = Some(message.into());
    sender.push(AssistantMessageEvent::Error {
        reason: ErrorReason::Error,
        error: error.clone(),
    });
    sender.end(Some(error));
    stream
}

/// `Date.now()`.
pub fn now_millis() -> i64 {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
}
