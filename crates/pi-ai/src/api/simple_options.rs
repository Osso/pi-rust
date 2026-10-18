//! Port of `packages/ai/src/api/simple-options.ts`.

use crate::request::{ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use crate::types::{Context, Model, ModelThinkingLevel, ThinkingBudgets, ThinkingLevel};
use crate::utils::estimate::estimate_context_tokens;

pub const CONTEXT_SAFETY_TOKENS: f64 = 4096.0;
pub const MIN_MAX_TOKENS: f64 = 1.0;
/// Tokens always left for the answer when a thinking budget shares the response ceiling.
pub const MIN_ANSWER_TOKENS: f64 = 1024.0;

pub fn clamp_max_tokens_to_context(model: &Model, context: &Context, max_tokens: f64) -> f64 {
    if model.context_window <= 0.0 {
        return max_tokens.max(MIN_MAX_TOKENS);
    }
    let available = model.context_window - estimate_context_tokens(context).tokens - CONTEXT_SAFETY_TOKENS;
    max_tokens.min(available.max(MIN_MAX_TOKENS))
}

pub fn build_base_options(
    model: &Model,
    context: &Context,
    options: Option<&SimpleStreamOptions>,
    api_key: Option<&str>,
) -> StreamOptions {
    let sampling_params = merge_sampling_params(model, options);
    let max_tokens = clamp_max_tokens_to_context(
        model,
        context,
        options
            .and_then(|options| options.stream.max_tokens)
            .unwrap_or(model.max_tokens),
    );
    let request = options.map(|options| &options.stream.request);
    StreamOptions {
        request: ProviderRequestOptions {
            signal: request.and_then(|request| request.signal.clone()),
            api_key: non_empty(api_key)
                .map(str::to_owned)
                .or_else(|| request.and_then(|request| request.api_key.clone())),
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
        max_tokens: Some(max_tokens),
        transport: options.and_then(|options| options.stream.transport),
        cache_retention: options.and_then(|options| options.stream.cache_retention),
        session_id: options.and_then(|options| options.stream.session_id.clone()),
        on_retry: options.and_then(|options| options.stream.on_retry.clone()),
        websocket_connect_timeout_ms: options.and_then(|options| options.stream.websocket_connect_timeout_ms),
        metadata: options.and_then(|options| options.stream.metadata.clone()),
        extra: options.map(|options| options.stream.extra.clone()).unwrap_or_default(),
    }
}

fn merge_sampling_params(model: &Model, options: Option<&SimpleStreamOptions>) -> Option<crate::types::JsonObject> {
    let option_params = options.and_then(|options| options.stream.sampling_params.as_ref());
    match (model.sampling_params.as_ref(), option_params) {
        (None, None) => None,
        (model_params, option_params) => {
            let mut merged = model_params.cloned().unwrap_or_default();
            if let Some(option_params) = option_params {
                merged.extend(option_params.clone());
            }
            Some(merged)
        }
    }
}

fn non_empty(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.is_empty())
}

pub fn clamp_reasoning(effort: Option<ThinkingLevel>) -> Option<ThinkingLevel> {
    match effort {
        Some(ThinkingLevel::Xhigh | ThinkingLevel::Max | ThinkingLevel::Ultra) => Some(ThinkingLevel::High),
        other => other,
    }
}

pub fn clamp_standard_reasoning_level(level: Option<ModelThinkingLevel>) -> Option<ModelThinkingLevel> {
    match level {
        None | Some(ModelThinkingLevel::Off) => None,
        Some(ModelThinkingLevel::Max | ModelThinkingLevel::Ultra) => Some(ModelThinkingLevel::High),
        other => other,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdjustedMaxTokens {
    pub max_tokens: f64,
    pub thinking_budget: f64,
}

pub fn adjust_max_tokens_for_thinking(
    base_max_tokens: Option<f64>,
    model_max_tokens: f64,
    reasoning_level: ThinkingLevel,
    custom_budgets: Option<&ThinkingBudgets>,
) -> AdjustedMaxTokens {
    let budgets = merged_budgets(custom_budgets);
    let level = clamp_reasoning(Some(reasoning_level)).unwrap_or(ThinkingLevel::High);
    let mut thinking_budget = budget_for(&budgets, level);
    let max_tokens = match base_max_tokens {
        None => model_max_tokens,
        Some(base_max_tokens) => (base_max_tokens + thinking_budget).min(model_max_tokens),
    };
    if max_tokens <= thinking_budget {
        thinking_budget = (max_tokens - MIN_ANSWER_TOKENS).max(0.0);
    }
    AdjustedMaxTokens {
        max_tokens,
        thinking_budget,
    }
}

fn merged_budgets(custom: Option<&ThinkingBudgets>) -> ThinkingBudgets {
    let mut budgets = ThinkingBudgets {
        minimal: Some(1024.0),
        low: Some(2048.0),
        medium: Some(8192.0),
        high: Some(16384.0),
    };
    if let Some(custom) = custom {
        budgets.minimal = custom.minimal.or(budgets.minimal);
        budgets.low = custom.low.or(budgets.low);
        budgets.medium = custom.medium.or(budgets.medium);
        budgets.high = custom.high.or(budgets.high);
    }
    budgets
}

fn budget_for(budgets: &ThinkingBudgets, level: ThinkingLevel) -> f64 {
    let value = match level {
        ThinkingLevel::Minimal => budgets.minimal,
        ThinkingLevel::Low => budgets.low,
        ThinkingLevel::Medium => budgets.medium,
        ThinkingLevel::High | ThinkingLevel::Xhigh | ThinkingLevel::Max | ThinkingLevel::Ultra => budgets.high,
    };
    value.unwrap_or(0.0)
}
