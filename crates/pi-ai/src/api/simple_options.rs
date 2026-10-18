//! Port of `packages/ai/src/api/simple-options.ts`.

use crate::request::{SimpleStreamOptions, StreamOptions};
use crate::types::{Context, JsonObject, Model, ThinkingBudgets, ThinkingLevel};
use crate::utils::estimate::estimate_context_tokens;

const CONTEXT_SAFETY_TOKENS: f64 = 4096.0;
const MIN_MAX_TOKENS: f64 = 1.0;

/// Tokens always left for the answer when a thinking budget shares the response ceiling.
pub const MIN_ANSWER_TOKENS: f64 = 1024.0;

/// `clampMaxTokensToContext`.
pub fn clamp_max_tokens_to_context(model: &Model, context: &Context, max_tokens: f64) -> f64 {
    if model.context_window <= 0.0 {
        return max_tokens.max(MIN_MAX_TOKENS);
    }
    let available = model.context_window - estimate_context_tokens(context).tokens - CONTEXT_SAFETY_TOKENS;
    max_tokens.min(available.max(MIN_MAX_TOKENS))
}

/// `buildBaseOptions` for the `apiKey`-less call shape used by `streamSimple`.
pub fn build_base_options(model: &Model, context: &Context, options: Option<&SimpleStreamOptions>) -> StreamOptions {
    let sampling_params = merge_sampling_params(model, options);
    let stream = options.map(|options| &options.stream);
    StreamOptions {
        request: crate::request::ProviderRequestOptions {
            signal: stream.and_then(|options| options.request.signal.clone()),
            api_key: stream.and_then(|options| options.request.api_key.clone()),
            fetch: stream.and_then(|options| options.request.fetch.clone()),
            env: stream.and_then(|options| options.request.env.clone()),
            on_payload: stream.and_then(|options| options.request.on_payload.clone()),
            on_response: stream.and_then(|options| options.request.on_response.clone()),
            headers: stream.and_then(|options| options.request.headers.clone()),
            timeout_ms: stream.and_then(|options| options.request.timeout_ms),
            max_retries: stream.and_then(|options| options.request.max_retries),
            max_retry_delay_ms: stream.and_then(|options| options.request.max_retry_delay_ms),
            transform_headers: stream.and_then(|options| options.request.transform_headers.clone()),
        },
        temperature: stream.and_then(|options| options.temperature),
        sampling_params,
        max_tokens: Some(clamp_max_tokens_to_context(
            model,
            context,
            stream
                .and_then(|options| options.max_tokens)
                .unwrap_or(model.max_tokens),
        )),
        transport: stream.and_then(|options| options.transport),
        cache_retention: stream.and_then(|options| options.cache_retention),
        session_id: stream.and_then(|options| options.session_id.clone()),
        on_retry: stream.and_then(|options| options.on_retry.clone()),
        websocket_connect_timeout_ms: stream.and_then(|options| options.websocket_connect_timeout_ms),
        metadata: stream.and_then(|options| options.metadata.clone()),
        extra: stream.map(|options| options.extra.clone()).unwrap_or_default(),
    }
}

fn merge_sampling_params(model: &Model, options: Option<&SimpleStreamOptions>) -> Option<JsonObject> {
    let option_params = options.and_then(|options| options.stream.sampling_params.as_ref());
    if model.sampling_params.is_none() && option_params.is_none() {
        return None;
    }
    let mut merged = model.sampling_params.clone().unwrap_or_default();
    if let Some(option_params) = option_params {
        for (key, value) in option_params {
            merged.insert(key.clone(), value.clone());
        }
    }
    Some(merged)
}

/// `clampReasoning`: extended levels collapse to `high`.
pub fn clamp_reasoning(effort: Option<ThinkingLevel>) -> Option<ThinkingLevel> {
    match effort {
        Some(ThinkingLevel::Xhigh | ThinkingLevel::Max | ThinkingLevel::Ultra) => Some(ThinkingLevel::High),
        other => other,
    }
}

/// Result of [`adjust_max_tokens_for_thinking`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdjustedThinking {
    pub max_tokens: f64,
    pub thinking_budget: f64,
}

/// `adjustMaxTokensForThinking`.
pub fn adjust_max_tokens_for_thinking(
    base_max_tokens: Option<f64>,
    model_max_tokens: f64,
    reasoning_level: ThinkingLevel,
    custom_budgets: Option<&ThinkingBudgets>,
) -> AdjustedThinking {
    let default_budgets = ThinkingBudgets {
        minimal: Some(1024.0),
        low: Some(2048.0),
        medium: Some(8192.0),
        high: Some(16384.0),
    };
    let budgets = custom_budgets.unwrap_or(&default_budgets);
    let level = clamp_reasoning(Some(reasoning_level)).unwrap_or(ThinkingLevel::High);
    let budget = budget_for_level(budgets, level).unwrap_or_else(|| budget_for_level(&default_budgets, level).unwrap_or(0.0));
    let max_tokens = base_max_tokens.map_or(model_max_tokens, |base| (base + budget).min(model_max_tokens));
    let thinking_budget = if max_tokens <= budget {
        (max_tokens - MIN_ANSWER_TOKENS).max(0.0)
    } else {
        budget
    };
    AdjustedThinking {
        max_tokens,
        thinking_budget,
    }
}

fn budget_for_level(budgets: &ThinkingBudgets, level: ThinkingLevel) -> Option<f64> {
    match level {
        ThinkingLevel::Minimal => budgets.minimal,
        ThinkingLevel::Low => budgets.low,
        ThinkingLevel::Medium => budgets.medium,
        ThinkingLevel::High => budgets.high,
        ThinkingLevel::Xhigh | ThinkingLevel::Max | ThinkingLevel::Ultra => budgets.high,
    }
}
