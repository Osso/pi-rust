use pi_ai::api::simple_options::{
    AdjustedMaxTokens, adjust_max_tokens_for_thinking, build_base_options, clamp_max_tokens_to_context,
    clamp_reasoning, clamp_standard_reasoning_level,
};
use pi_ai::request::{ProviderRequestOptions, SimpleStreamOptions, StreamOptions};
use pi_ai::types::{Context, JsonObject, Model, ModelThinkingLevel, ThinkingBudgets, ThinkingLevel};
use serde_json::{Value, json};

fn model(context_window: f64, max_tokens: f64) -> Model {
    serde_json::from_value(json!({
        "id": "test-model", "name": "Test Model", "api": "anthropic-messages", "provider": "anthropic",
        "baseUrl": "https://api.anthropic.com", "reasoning": false, "input": ["text"],
        "cost": {"input": 0, "output": 0, "cacheRead": 0, "cacheWrite": 0},
        "contextWindow": context_window, "maxTokens": max_tokens
    }))
    .expect("model")
}

fn context(text: &str) -> Context {
    serde_json::from_value(json!({
        "messages": [{"role": "user", "content": text, "timestamp": 1}]
    }))
    .expect("context")
}

#[test]
fn clamp_max_tokens_reserves_context_and_safety_tokens() {
    let model = model(10_000.0, 8_000.0);
    // 4000 chars -> 1000 estimated tokens; 10000 - 1000 - 4096 = 4904.
    assert_eq!(
        clamp_max_tokens_to_context(&model, &context(&"x".repeat(4_000)), 8_000.0),
        4_904.0
    );
    assert_eq!(
        clamp_max_tokens_to_context(&model, &context(&"x".repeat(4_000)), 1_000.0),
        1_000.0
    );
    assert_eq!(clamp_max_tokens_to_context(&model, &context("tiny"), 1.0), 1.0);
}

#[test]
fn clamp_max_tokens_ignores_non_positive_context_windows() {
    let model = model(0.0, 8_000.0);
    assert_eq!(clamp_max_tokens_to_context(&model, &context("hi"), 8_000.0), 8_000.0);
    assert_eq!(clamp_max_tokens_to_context(&model, &context("hi"), 0.0), 1.0);
}

#[test]
fn build_base_options_merges_sampling_and_clamps_max_tokens() {
    let mut model = model(10_000.0, 8_000.0);
    model.sampling_params = Some(JsonObject::from_iter([
        ("temperature".to_owned(), Value::from(0.1)),
        ("top_p".to_owned(), Value::from(0.9)),
    ]));
    let mut extra = JsonObject::new();
    extra.insert("thinkingEnabled".to_owned(), Value::Bool(true));
    let options = SimpleStreamOptions {
        stream: StreamOptions {
            request: ProviderRequestOptions {
                api_key: Some("option-key".to_owned()),
                ..ProviderRequestOptions::default()
            },
            temperature: Some(0.3),
            sampling_params: Some(JsonObject::from_iter([("top_p".to_owned(), Value::from(0.5))])),
            max_tokens: Some(20_000.0),
            extra,
            ..StreamOptions::default()
        },
        ..SimpleStreamOptions::default()
    };

    let base = build_base_options(&model, &context("hi"), Some(&options), Some("explicit-key"));
    assert_eq!(base.request.api_key.as_deref(), Some("explicit-key"));
    assert_eq!(base.temperature, Some(0.3));
    assert_eq!(base.max_tokens, Some(5_903.0));
    let sampling = base.sampling_params.expect("sampling");
    assert_eq!(sampling["temperature"], json!(0.1));
    assert_eq!(sampling["top_p"], json!(0.5));
    assert_eq!(base.extra["thinkingEnabled"], json!(true));

    let base = build_base_options(&model, &context("hi"), Some(&options), Some(""));
    assert_eq!(base.request.api_key.as_deref(), Some("option-key"));
}

#[test]
fn adjust_max_tokens_fits_the_thinking_budget_inside_the_response_ceiling() {
    let custom = ThinkingBudgets {
        low: Some(2_048.0),
        ..ThinkingBudgets::default()
    };
    assert_eq!(
        adjust_max_tokens_for_thinking(Some(4_096.0), 8_192.0, ThinkingLevel::Low, Some(&custom)),
        AdjustedMaxTokens {
            max_tokens: 6_144.0,
            thinking_budget: 2_048.0
        }
    );
    assert_eq!(
        adjust_max_tokens_for_thinking(None, 8_192.0, ThinkingLevel::High, None),
        AdjustedMaxTokens {
            max_tokens: 8_192.0,
            thinking_budget: 8_192.0 - 1_024.0
        }
    );
    assert_eq!(
        adjust_max_tokens_for_thinking(Some(0.0), 1_000.0, ThinkingLevel::High, None),
        AdjustedMaxTokens {
            max_tokens: 1_000.0,
            thinking_budget: 0.0
        }
    );
    assert_eq!(
        adjust_max_tokens_for_thinking(Some(1_000.0), 100_000.0, ThinkingLevel::Xhigh, None),
        AdjustedMaxTokens {
            max_tokens: 17_384.0,
            thinking_budget: 16_384.0
        }
    );
}

#[test]
fn reasoning_levels_clamp_to_supported_provider_levels() {
    assert_eq!(clamp_reasoning(Some(ThinkingLevel::Xhigh)), Some(ThinkingLevel::High));
    assert_eq!(clamp_reasoning(Some(ThinkingLevel::Max)), Some(ThinkingLevel::High));
    assert_eq!(clamp_reasoning(Some(ThinkingLevel::Ultra)), Some(ThinkingLevel::High));
    assert_eq!(
        clamp_reasoning(Some(ThinkingLevel::Minimal)),
        Some(ThinkingLevel::Minimal)
    );
    assert_eq!(clamp_reasoning(None), None);

    assert_eq!(clamp_standard_reasoning_level(None), None);
    assert_eq!(clamp_standard_reasoning_level(Some(ModelThinkingLevel::Off)), None);
    assert_eq!(
        clamp_standard_reasoning_level(Some(ModelThinkingLevel::Max)),
        Some(ModelThinkingLevel::High)
    );
    assert_eq!(
        clamp_standard_reasoning_level(Some(ModelThinkingLevel::Ultra)),
        Some(ModelThinkingLevel::High)
    );
    assert_eq!(
        clamp_standard_reasoning_level(Some(ModelThinkingLevel::Medium)),
        Some(ModelThinkingLevel::Medium)
    );
}
