//! Native catalog maintenance counterpart of `scripts/generate-models.ts`.
//! Provider payloads remain lossless JSON, not the runtime's narrower typed models.

use pi_js_compat::js_json::{JsString, JsValue, object_entries_lossless};
use serde_json::json;

use crate::models_dev_reasoning_options::{ModelsDevReasoningOption, get_effort_thinking_level_map};

fn field<'a>(value: &'a JsValue, key: &str) -> &'a JsValue {
    match value {
        JsValue::Object(object) => object.get(&JsString::from(key)).unwrap_or(&JsValue::Null),
        _ => &JsValue::Null,
    }
}

fn truthy(value: &JsValue) -> bool {
    match value {
        JsValue::Null => false,
        JsValue::Bool(value) => *value,
        JsValue::Number(value) => value.as_f64().is_some_and(|value| value != 0.0 && !value.is_nan()),
        JsValue::String(value) => !value.units().is_empty(),
        _ => true,
    }
}

fn fallback(value: &JsValue, default: JsValue) -> JsValue {
    if truthy(value) { value.clone() } else { default }
}
fn number(value: u64) -> JsValue {
    JsValue::from(&json!(value))
}
fn text(value: &str) -> JsValue {
    JsValue::String(JsString::from(value))
}
fn object(entries: impl IntoIterator<Item = (&'static str, JsValue)>) -> JsValue {
    JsValue::Object(
        entries
            .into_iter()
            .map(|(key, value)| (JsString::from(key), value))
            .collect(),
    )
}
fn literal(value: serde_json::Value) -> JsValue {
    JsValue::from(&value)
}
fn set(value: &mut JsValue, key: &str, item: JsValue) {
    if let JsValue::Object(map) = value {
        map.insert(JsString::from(key), item);
    }
}
fn includes(value: &JsValue, needle: &str) -> bool {
    match value {
        JsValue::Array(values) => values.contains(&text(needle)),
        JsValue::String(value) => value
            .units()
            .windows(needle.encode_utf16().count())
            .any(|part| part.iter().copied().eq(needle.encode_utf16())),
        _ => false,
    }
}
fn entries(value: &JsValue) -> Vec<(&JsString, &JsValue)> {
    match value {
        JsValue::Object(values) => object_entries_lossless(values),
        _ => vec![],
    }
}
fn equal(value: &JsValue, expected: &str) -> bool {
    value == &text(expected)
}

/// models.dev costs retain source values and only context-size tiers, in source order.
pub fn get_models_dev_cost(cost: &JsValue) -> JsValue {
    let mut result = plain_cost(cost);
    if let JsValue::Array(tiers) = field(cost, "tiers") {
        let tiers: Vec<_> = tiers
            .iter()
            .filter_map(|tier| {
                let context = field(tier, "tier");
                if !equal(field(context, "type"), "context") {
                    return None;
                }
                let JsValue::Object(context_object) = context else {
                    return None;
                };
                let size = context_object.get(&JsString::from("size"))?;
                let mut rates = object([("inputTokensAbove", size.clone())]);
                if let (JsValue::Object(rates), JsValue::Object(cost)) = (&mut rates, plain_cost(tier)) {
                    rates.extend(cost);
                }
                Some(rates)
            })
            .collect();
        if !tiers.is_empty() {
            set(&mut result, "tiers", JsValue::Array(tiers));
        }
    }
    result
}

fn plain_cost(cost: &JsValue) -> JsValue {
    object([
        ("input", fallback(field(cost, "input"), number(0))),
        ("output", fallback(field(cost, "output"), number(0))),
        ("cacheRead", fallback(field(cost, "cache_read"), number(0))),
        ("cacheWrite", fallback(field(cost, "cache_write"), number(0))),
    ])
}

fn common_model(id: &JsString, model: &JsValue, provider: &str) -> JsValue {
    let id = JsValue::String(id.clone());
    object([
        ("id", id.clone()),
        ("name", fallback(field(model, "name"), id)),
        ("provider", text(provider)),
        (
            "reasoning",
            JsValue::Bool(field(model, "reasoning") == &JsValue::Bool(true)),
        ),
        (
            "input",
            if includes(field(field(model, "modalities"), "input"), "image") {
                literal(json!(["text", "image"]))
            } else {
                literal(json!(["text"]))
            },
        ),
        ("cost", plain_cost(field(model, "cost"))),
        (
            "contextWindow",
            fallback(field(field(model, "limit"), "context"), number(4096)),
        ),
        (
            "maxTokens",
            fallback(field(field(model, "limit"), "output"), number(4096)),
        ),
    ])
}

/// Baseten accepts non-tool models and excludes only deprecated entries.
pub fn process_baseten_models(provider: &JsValue) -> JsValue {
    let mut models = Vec::new();
    for (id, model) in entries(field(provider, "models")) {
        if equal(field(model, "status"), "deprecated") {
            continue;
        }
        let is_glm = ["zai-org/GLM-5.2", "zai-org/GLM-5.2-Fast"]
            .iter()
            .any(|name| id == &JsString::from(*name));
        let options = field(model, "reasoning_options");
        let has_option = |kind| match options {
            JsValue::Array(options) => options.iter().any(|option| equal(field(option, "type"), kind)),
            _ => false,
        };
        let toggle = has_option("toggle") || is_glm;
        let effort = has_option("effort") || is_glm;
        let mut compat = literal(
            json!({"supportsStore":false,"supportsDeveloperRole":false,"supportsReasoningEffort":effort,"supportsUsageInStreaming":true,"maxTokensField":"max_tokens","supportsStrictMode":true,"supportsLongCacheRetention":false}),
        );
        if effort {
            set(&mut compat, "thinkingFormat", text("openai"));
        }
        if toggle {
            set(&mut compat, "thinkingFormat", text("baseten"));
            set(
                &mut compat,
                "chatTemplateArgs",
                literal(json!({"enable_thinking":{"$var":"thinking.enabled"}})),
            );
        }
        let map = if is_glm {
            Some(literal(
                json!({"off":"none","minimal":null,"low":null,"medium":null,"high":"high","xhigh":null,"max":"max"}),
            ))
        } else if toggle {
            Some(literal(
                json!({"off":"off","minimal":null,"low":null,"medium":null,"high":"high","xhigh":null,"max":null}),
            ))
        } else {
            reasoning_map(options)
        };
        // Preserve source insertion order, including its API/baseUrl placement.
        let common = common_model(id, model, "baseten");
        let mut result = object([
            ("id", field(&common, "id").clone()),
            ("name", field(&common, "name").clone()),
            ("api", text("openai-completions")),
            ("provider", text("baseten")),
            ("baseUrl", text("https://inference.baseten.co/v1")),
            ("reasoning", field(&common, "reasoning").clone()),
        ]);
        if let Some(map) = map {
            set(&mut result, "thinkingLevelMap", map);
        }
        for key in ["input", "cost"] {
            set(&mut result, key, field(&common, key).clone());
        }
        set(&mut result, "compat", compat);
        for key in ["contextWindow", "maxTokens"] {
            set(&mut result, key, field(&common, key).clone());
        }
        models.push(result);
    }
    JsValue::Array(models)
}

fn reasoning_map(options: &JsValue) -> Option<JsValue> {
    // Invalid JS option shapes remain outside the approved typed reasoning API boundary.
    // Do not convert the provider payload itself through lossy UTF-8.
    let options: Vec<ModelsDevReasoningOption> = match options {
        JsValue::Array(options) => options
            .iter()
            .filter_map(|option| {
                let kind = field(option, "type");
                if equal(kind, "effort") {
                    let JsValue::Array(values) = field(option, "values") else {
                        return None;
                    };
                    let values = values
                        .iter()
                        .filter_map(|value| match value {
                            JsValue::Null => Some(None),
                            JsValue::String(value) => value
                                .to_utf8()
                                .ok()
                                .and_then(|value| serde_json::from_value(json!(value)).ok())
                                .map(Some),
                            _ => None,
                        })
                        .collect();
                    Some(ModelsDevReasoningOption::Effort { values })
                } else {
                    None
                }
            })
            .collect(),
        _ => vec![],
    };
    get_effort_thinking_level_map(&options)
        .map(|value| literal(serde_json::to_value(value).expect("reasoning map serializes")))
}

/// Fireworks uses Anthropic except GLM-5p2/Kimi-K3 completions families.
pub fn process_fireworks_models(provider: &JsValue) -> JsValue {
    let mut models = Vec::new();
    for (id, model) in entries(field(provider, "models")) {
        if field(model, "tool_call") != &JsValue::Bool(true) {
            continue;
        }
        let mut result = common_model(id, model, "fireworks");
        let id = JsValue::String(id.clone());
        let completions = includes(&id, "glm-5p2") || includes(&id, "kimi-k3");
        set(
            &mut result,
            "api",
            text(if completions {
                "openai-completions"
            } else {
                "anthropic-messages"
            }),
        );
        set(
            &mut result,
            "baseUrl",
            text(if completions {
                "https://api.fireworks.ai/inference/v1"
            } else {
                "https://api.fireworks.ai/inference"
            }),
        );
        let mut compat = if completions {
            literal(
                json!({"supportsStore":false,"supportsDeveloperRole":false,"sendSessionAffinityHeaders":true,"supportsLongCacheRetention":false}),
            )
        } else {
            literal(
                json!({"sendSessionAffinityHeaders":true,"supportsEagerToolInputStreaming":false,"supportsCacheControlOnTools":false,"supportsLongCacheRetention":false}),
            )
        };
        if !includes(&id, "glm-5p2") && includes(&id, "kimi-k3") {
            set(
                &mut compat,
                "requiresReasoningContentOnAssistantMessages",
                JsValue::Bool(true),
            );
            set(&mut compat, "thinkingFormat", text("openai"));
            set(&mut compat, "deferredToolsMode", text("kimi"));
        }
        set(&mut result, "compat", compat);
        models.push(result);
    }
    JsValue::Array(models)
}
