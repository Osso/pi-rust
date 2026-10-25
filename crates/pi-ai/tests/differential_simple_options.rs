#[path = "support/json_compare.rs"]
mod json_compare;
#[path = "support/oracle.rs"]
mod oracle;

use pi_ai::api::simple_options::{
    adjust_max_tokens_for_thinking, build_base_options, clamp_reasoning, clamp_standard_reasoning_level,
};
use pi_ai::request::{SimpleStreamOptions, StreamOptions};
use pi_ai::types::{Context, Model, ThinkingBudgets, ThinkingLevel};
use serde_json::{Value, json};

fn model() -> Value {
    json!({"id":"test", "name":"test", "api":"openai-completions", "provider":"openai",
        "baseUrl":"https://example.test/v1", "reasoning":true, "input":["text"],
        "cost":{"input":0,"output":0,"cacheRead":0,"cacheWrite":0},
        "contextWindow":10000, "maxTokens":4096, "samplingParams":{"top_p":0.9}})
}

fn observe_base(model: &Value, context: &Value, options: &Value, api_key: Option<&str>) -> Value {
    let mut stream = StreamOptions::default();
    stream.cwd = options.get("cwd").and_then(Value::as_str).map(Into::into);
    stream.temperature = options.get("temperature").and_then(Value::as_f64);
    stream.max_tokens = options.get("maxTokens").and_then(Value::as_f64);
    stream.request.api_key = options.get("apiKey").and_then(Value::as_str).map(Into::into);
    if let Some(value) = options.get("samplingParams") {
        stream.extra.insert("samplingParams".into(), value.clone());
    }
    stream.extra.insert("oldOnly".into(), json!(true));
    let options = SimpleStreamOptions {
        stream,
        ..Default::default()
    };
    let model: Model = serde_json::from_value(model.clone()).unwrap();
    let context: Context = serde_json::from_value(context.clone()).unwrap();
    let base = build_base_options(&model, &context, Some(&options), api_key.map(Into::into).as_ref());
    let mut output = json!({});
    for (key, value) in [
        ("cwd", base.cwd.map(|value| json!(value))),
        ("temperature", base.temperature.map(|value| json!(value))),
        ("maxTokens", base.max_tokens.map(|value| json!(value))),
        ("apiKey", base.request.api_key.map(|value| json!(value))),
    ] {
        if let Some(value) = value {
            output[key] = value;
        }
    }
    for (key, value) in base.extra {
        output[key] = value;
    }
    output
}

#[test]
fn base_options_match_frozen_field_selection_context_clamps_and_key_precedence() {
    let mut oracle = oracle::TsOracle::spawn("simple-options-oracle.ts", "packages/ai/src/api/simple-options.ts");
    for window in [0, -1, 4096, 5000, 10000] {
        for options in [
            json!({}),
            json!({"cwd":"/tmp/work", "apiKey":"", "maxTokens":8000.5}),
            json!({"apiKey":"caller", "temperature":0.25,"samplingParams":{"seed":7},"oldOnly":true}),
        ] {
            for api_key in [None, Some(""), Some("override")] {
                let mut model = model();
                model["contextWindow"] = json!(window);
                let context = json!({"systemPrompt":"a short prompt", "messages":[]});
                let mut request = json!({"operation":"base", "model":model,"context":context,"options":options});
                if let Some(api_key) = api_key {
                    request["apiKey"] = json!(api_key);
                }
                json_compare::assert_json_eq_ordered(
                    &observe_base(&model, &context, &options, api_key),
                    &oracle.request(&request),
                    &request.to_string(),
                );
            }
        }
    }
}

#[test]
fn thinking_budgets_and_clamps_match_all_frozen_levels() {
    let mut oracle = oracle::TsOracle::spawn("simple-options-oracle.ts", "packages/ai/src/api/simple-options.ts");
    for level in ["minimal", "low", "medium", "high", "xhigh", "max", "ultra"] {
        let reasoning: ThinkingLevel = serde_json::from_value(json!(level)).unwrap();
        let standard = serde_json::from_value(json!(level)).unwrap();
        assert_eq!(
            json!({"reasoning":clamp_reasoning(Some(reasoning)), "standard":clamp_standard_reasoning_level(Some(standard))}),
            oracle.request(&json!({"operation":"clamps","level":level}))
        );
        for base in [None, Some(0.0), Some(1024.5), Some(9000.0)] {
            for cap in [512.0, 2000.0, 32768.0] {
                for budgets in [None, Some(json!({"high":0,"low":333.5}))] {
                    let custom: Option<ThinkingBudgets> =
                        budgets.clone().map(|value| serde_json::from_value(value).unwrap());
                    let (max_tokens, thinking_budget) =
                        adjust_max_tokens_for_thinking(base, cap, reasoning, custom.as_ref());
                    let mut request = json!({"operation":"budget","modelMaxTokens":cap,"level":level});
                    if let Some(base) = base {
                        request["baseMaxTokens"] = json!(base);
                    }
                    if let Some(budgets) = budgets {
                        request["budgets"] = budgets;
                    }
                    json_compare::assert_json_eq_ordered(
                        &json!({"maxTokens":max_tokens,"thinkingBudget":thinking_budget}),
                        &oracle.request(&request),
                        &request.to_string(),
                    );
                }
            }
        }
    }
    for level in [None, Some("off")] {
        let standard = level.map(|level| serde_json::from_value(json!(level)).unwrap());
        let mut request = json!({"operation":"clamps"});
        if let Some(level) = level {
            request["level"] = json!(level);
        }
        assert_eq!(clamp_standard_reasoning_level(standard), None);
        if level.is_none() {
            assert_eq!(clamp_reasoning(None), None);
        }
        assert_eq!(oracle.request(&request)["standard"], Value::Null);
    }
}
