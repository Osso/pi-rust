//! Port of the `buildBaseOptions` and context-clamp portion of
//! `packages/ai/src/api/simple-options.ts`.
//!
//! Only the helpers required by provider API modules are ported here; the
//! thinking-budget helpers remain tracked by the `simple-options.ts` coverage row.

use crate::request::{ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use crate::types::{Context, JsonObject, Model};
use crate::utils::estimate::estimate_context_tokens;

const CONTEXT_SAFETY_TOKENS: f64 = 4096.0;
const MIN_MAX_TOKENS: f64 = 1.0;

/// `clampMaxTokensToContext()`.
pub fn clamp_max_tokens_to_context(model: &Model, context: &Context, max_tokens: f64) -> f64 {
    if model.context_window <= 0.0 {
        return max_tokens.max(MIN_MAX_TOKENS);
    }
    let available = model.context_window - estimate_context_tokens(context).tokens - CONTEXT_SAFETY_TOKENS;
    max_tokens.min(available.max(MIN_MAX_TOKENS))
}

/// `buildBaseOptions()`.
///
/// `api_key` is the resolved key; TypeScript's `apiKey || options?.apiKey`
/// fallback is the caller's responsibility because a missing key is an error
/// before this point.
pub fn build_base_options(
    model: &Model,
    context: &Context,
    options: Option<&SimpleStreamOptions>,
    api_key: Option<&str>,
) -> StreamOptions {
    let sampling_params = merge_sampling_params(model, options);
    let request = options.map(|options| &options.stream.request);
    StreamOptions {
        request: ProviderRequestOptions {
            signal: request.and_then(|request| request.signal.clone()),
            api_key: api_key
                .map(str::to_owned)
                .or_else(|| request.and_then(|request| request.api_key.clone().filter(|key| !key.is_empty()))),
            fetch: request.and_then(|request| request.fetch.clone()),
            env: request.and_then(|request| request.env.clone()),
            on_payload: request.and_then(|request| request.on_payload.clone()),
            on_response: request.and_then(|request| request.on_response.clone()),
            headers: request.and_then(|request| request.headers.clone()),
            timeout_ms: request.and_then(|request| request.timeout_ms),
            max_retries: request.and_then(|request| request.max_retries),
            max_retry_delay_ms: request.and_then(|request| request.max_retry_delay_ms),
            transform_headers: None,
        },
        temperature: options.and_then(|options| options.stream.temperature),
        sampling_params,
        max_tokens: Some(clamp_max_tokens_to_context(
            model,
            context,
            options
                .and_then(|options| options.stream.max_tokens)
                .unwrap_or(model.max_tokens),
        )),
        transport: options.and_then(|options| options.stream.transport),
        cache_retention: options.and_then(|options| options.stream.cache_retention),
        session_id: options.and_then(|options| options.stream.session_id.clone()),
        on_retry: options.and_then(|options| options.stream.on_retry.clone()),
        websocket_connect_timeout_ms: options.and_then(|options| options.stream.websocket_connect_timeout_ms),
        metadata: options.and_then(|options| options.stream.metadata.clone()),
        extra: JsonObject::new(),
    }
}

fn merge_sampling_params(model: &Model, options: Option<&SimpleStreamOptions>) -> Option<JsonObject> {
    let option_params = options.and_then(|options| options.stream.sampling_params.as_ref());
    if model.sampling_params.is_none() && option_params.is_none() {
        return None;
    }
    let mut merged = model.sampling_params.clone().unwrap_or_default();
    if let Some(option_params) = option_params {
        merged.extend(option_params.clone());
    }
    Some(merged)
}
