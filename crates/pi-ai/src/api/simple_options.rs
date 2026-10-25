//! Port of `packages/ai/src/api/simple-options.ts`.

use crate::request::{SimpleStreamOptions, StreamOptions};
use crate::types::{Context, Model, ModelThinkingLevel, ThinkingBudgets, ThinkingLevel};
use crate::utils::estimate::estimate_context_tokens;

const CONTEXT_SAFETY_TOKENS: f64 = 4096.0;
const MIN_MAX_TOKENS: f64 = 1.0;

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
    api_key: Option<&pi_js_compat::js_json::JsString>,
) -> StreamOptions {
    let stream = options.map(|options| &options.stream).cloned().unwrap_or_default();
    let sampling_params = merge_sampling_params(model, &stream);
    StreamOptions {
        cwd: stream.cwd.clone(),
        request: crate::request::ProviderRequestOptions {
            signal: stream.request.signal.clone(),
            api_key: api_key
                .filter(|key| !key.is_empty())
                .cloned()
                .or_else(|| stream.request.api_key.clone().filter(|key| !key.is_empty())),
            fetch: stream.request.fetch.clone(),
            env: stream.request.env.clone(),
            on_payload: stream.request.on_payload.clone(),
            on_response: stream.request.on_response.clone(),
            headers: stream.request.headers.clone(),
            timeout_ms: stream.request.timeout_ms,
            max_retries: stream.request.max_retries,
            max_retry_delay_ms: stream.request.max_retry_delay_ms,
        },
        temperature: stream.temperature,
        sampling_params,
        max_tokens: Some(clamp_max_tokens_to_context(
            model,
            context,
            stream.max_tokens.unwrap_or(model.max_tokens),
        )),
        transport: stream.transport,
        cache_retention: stream.cache_retention,
        session_id: stream.session_id.clone(),
        on_retry: stream.on_retry.clone(),
        websocket_connect_timeout_ms: stream.websocket_connect_timeout_ms,
        metadata: stream.metadata.clone(),
        extra: Default::default(),
    }
}

fn merge_sampling_params(model: &Model, stream: &StreamOptions) -> Option<crate::types::JsonObject> {
    if model.sampling_params.is_none() && stream.sampling_params.is_none() {
        return None;
    }
    let mut merged = model.sampling_params.clone().unwrap_or_default();
    for (key, value) in stream.sampling_params.clone().unwrap_or_default() {
        merged.insert(key, value);
    }
    Some(merged)
}

pub fn clamp_reasoning(effort: Option<ThinkingLevel>) -> Option<ThinkingLevel> {
    effort.map(|effort| match effort {
        ThinkingLevel::Xhigh | ThinkingLevel::Max | ThinkingLevel::Ultra => ThinkingLevel::High,
        other => other,
    })
}

pub fn model_thinking_level(level: ThinkingLevel) -> ModelThinkingLevel {
    match level {
        ThinkingLevel::Minimal => ModelThinkingLevel::Minimal,
        ThinkingLevel::Low => ModelThinkingLevel::Low,
        ThinkingLevel::Medium => ModelThinkingLevel::Medium,
        ThinkingLevel::High => ModelThinkingLevel::High,
        ThinkingLevel::Xhigh => ModelThinkingLevel::Xhigh,
        ThinkingLevel::Max => ModelThinkingLevel::Max,
        ThinkingLevel::Ultra => ModelThinkingLevel::Ultra,
    }
}

/// `clampStandardReasoningLevel`: `off` becomes absent, `max`/`ultra` become `high`.
pub fn clamp_standard_reasoning_level(level: Option<ModelThinkingLevel>) -> Option<String> {
    let level = level?;
    match level {
        ModelThinkingLevel::Off => None,
        ModelThinkingLevel::Max | ModelThinkingLevel::Ultra => Some("high".to_owned()),
        ModelThinkingLevel::Minimal => Some("minimal".to_owned()),
        ModelThinkingLevel::Low => Some("low".to_owned()),
        ModelThinkingLevel::Medium => Some("medium".to_owned()),
        ModelThinkingLevel::High => Some("high".to_owned()),
        ModelThinkingLevel::Xhigh => Some("xhigh".to_owned()),
    }
}

pub fn adjust_max_tokens_for_thinking(
    base_max_tokens: Option<f64>,
    model_max_tokens: f64,
    reasoning_level: ThinkingLevel,
    custom_budgets: Option<&ThinkingBudgets>,
) -> (f64, f64) {
    let default_budgets = ThinkingBudgets {
        minimal: Some(1024.0),
        low: Some(2048.0),
        medium: Some(8192.0),
        high: Some(16384.0),
    };
    let budgets = merge_budgets(&default_budgets, custom_budgets);
    let level = clamp_reasoning(Some(reasoning_level)).expect("reasoning level");
    let budget = match level {
        ThinkingLevel::Minimal => budgets.minimal,
        ThinkingLevel::Low => budgets.low,
        ThinkingLevel::Medium => budgets.medium,
        ThinkingLevel::High => budgets.high,
        ThinkingLevel::Xhigh | ThinkingLevel::Max | ThinkingLevel::Ultra => budgets.high,
    }
    .unwrap_or(0.0);

    let max_tokens = match base_max_tokens {
        None => model_max_tokens,
        Some(base) => (base + budget).min(model_max_tokens),
    };
    let thinking_budget = if max_tokens <= budget {
        (max_tokens - MIN_ANSWER_TOKENS).max(0.0)
    } else {
        budget
    };
    (max_tokens, thinking_budget)
}

fn merge_budgets(defaults: &ThinkingBudgets, custom: Option<&ThinkingBudgets>) -> ThinkingBudgets {
    let Some(custom) = custom else {
        return defaults.clone();
    };
    ThinkingBudgets {
        minimal: custom.minimal.or(defaults.minimal),
        low: custom.low.or(defaults.low),
        medium: custom.medium.or(defaults.medium),
        high: custom.high.or(defaults.high),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{JsonObject, Modality, ModelCost};
    use serde_json::json;

    fn model() -> Model {
        Model {
            properties: Default::default(),
            id: "test".to_owned(),
            name: "test".into(),
            api: "openai-completions".to_owned(),
            provider: "openai".to_owned(),
            base_url: ("https://example.com".to_owned()).into(),
            reasoning: true,
            thinking_level_map: None,
            input: vec![Modality::Text],
            cost: ModelCost::default(),
            context_window: 10_000.0,
            auto_compaction_threshold: None,
            max_tokens: 4096.0,
            sampling_params: None,
            headers: None,
            compat: None,
        }
    }

    #[test]
    fn context_clamp_reserves_safety_tokens() {
        let context = Context {
            system_prompt: None,
            messages: Vec::new(),
            tools: None,
        };
        assert_eq!(clamp_max_tokens_to_context(&model(), &context, 4096.0), 4096.0);
        assert_eq!(clamp_max_tokens_to_context(&model(), &context, 100_000.0), 5904.0);
    }

    #[test]
    fn reasoning_clamps_match_standard_levels() {
        assert_eq!(clamp_reasoning(Some(ThinkingLevel::Max)), Some(ThinkingLevel::High));
        assert_eq!(clamp_reasoning(Some(ThinkingLevel::Low)), Some(ThinkingLevel::Low));
        assert_eq!(clamp_standard_reasoning_level(Some(ModelThinkingLevel::Off)), None);
        assert_eq!(
            clamp_standard_reasoning_level(Some(ModelThinkingLevel::Ultra)),
            Some("high".to_owned())
        );
    }

    #[test]
    fn thinking_budget_leaves_answer_room() {
        let (max_tokens, budget) = adjust_max_tokens_for_thinking(None, 2000.0, ThinkingLevel::High, None);
        assert_eq!(max_tokens, 2000.0);
        assert_eq!(budget, 976.0);
    }

    #[test]
    fn base_options_merge_sampling_params() {
        let mut model = model();
        model.sampling_params = Some(JsonObject::from_iter([
            ("top_p".to_owned(), json!(0.9)),
            ("seed".to_owned(), json!(1)),
        ]));
        let options = SimpleStreamOptions {
            stream: StreamOptions {
                sampling_params: Some(JsonObject::from_iter([("seed".to_owned(), json!(2))])),
                ..Default::default()
            },
            ..Default::default()
        };
        let base = build_base_options(
            &model,
            &Context {
                system_prompt: None,
                messages: Vec::new(),
                tools: None,
            },
            Some(&options),
            Some(&pi_js_compat::js_json::JsString::from("key")),
        );
        assert_eq!(
            base.request.api_key.as_ref(),
            Some(&pi_js_compat::js_json::JsString::from("key"))
        );
        assert_eq!(
            base.sampling_params,
            Some(JsonObject::from_iter([
                ("top_p".to_owned(), json!(0.9)),
                ("seed".to_owned(), json!(2)),
            ]))
        );
    }
}
