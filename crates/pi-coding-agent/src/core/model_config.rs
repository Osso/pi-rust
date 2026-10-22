//! Port of `packages/coding-agent/src/core/model-config.ts`.
//!
//! `models.json` is credential-blind: providers, model definitions, overrides, and compatibility
//! flags. Validation messages must match TypeBox's compiled validator, so the schema below mirrors
//! the TypeBox definitions and the issues come from the shared validation engine.

use std::path::Path;

use pi_ai::utils::validation::schema_error_lines;
use serde_json::{Value, json};

use crate::utils::paths::normalize_path;
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue, encode_node_utf8};

/// One immutable load of `models.json`.
#[derive(Debug, Clone, Default)]
pub struct ModelConfig {
    providers: JsObject,
    error: Option<JsString>,
}

impl ModelConfig {
    /// Loads `models.json`; a missing file yields an empty configuration.
    pub fn load(models_json_path: Option<&JsString>) -> Self {
        let Some(path) = models_json_path else {
            return Self::default();
        };
        let path = normalize_path(path, &Default::default()).unwrap_or_else(|_| path.to_owned());
        let content = match std::fs::read_to_string(encode_node_utf8(&path)) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Self::default(),
            Err(error) => {
                return Self {
                    providers: JsObject::new(),
                    error: Some(JsString::from(format!("Failed to load models.json: {error}\n\nFile: ")) + &path),
                };
            }
        };

        let parsed = match js_json::parse_lossless(&crate::utils::json::strip_json_comments(&content)) {
            Ok(parsed) => parsed,
            Err(error) => {
                return Self {
                    providers: JsObject::new(),
                    error: Some(JsString::from(format!("Failed to parse models.json: {error}\n\nFile: ")) + &path),
                };
            }
        };

        let errors = schema_error_lines(&models_config_schema().into(), &parsed);
        if !errors.is_empty() {
            return Self {
                providers: JsObject::new(),
                error: Some(
                    JsString::from("Invalid models.json schema:\n")
                        + JsString::join(errors, "\n")
                        + "\n\nFile: "
                        + &path,
                ),
            };
        }

        let providers = parsed
            .get("providers")
            .and_then(JsValue::as_object)
            .cloned()
            .unwrap_or_default();
        if providers.keys().any(|name| name.to_utf8().is_err()) {
            return Self {
                providers: JsObject::new(),
                error: Some("models.json provider ID contains an unpaired UTF-16 unit".into()),
            };
        }
        Self { providers, error: None }
    }

    pub fn get_provider(&self, provider_id: &str) -> Option<&JsValue> {
        self.providers.get(&JsString::from(provider_id))
    }

    pub fn get_provider_ids(&self) -> Vec<String> {
        js_json::object_entries_lossless(&self.providers)
            .into_iter()
            .map(|(name, _)| name.to_utf8().expect("provider IDs checked at models.json load"))
            .collect()
    }

    pub fn get_error(&self) -> Option<&JsString> {
        self.error.as_ref()
    }

    /// Provider map (insertion ordered), for consumers that need all entries.
    pub fn providers(&self) -> &JsObject {
        &self.providers
    }
}

/// `Type.Record(Type.String(), schema)`.
fn record(schema: Value) -> Value {
    json!({"type": "object", "patternProperties": {"^.*$": schema}})
}

/// `Type.Literal(value)` for string literals.
fn string_literal(value: &str) -> Value {
    json!({"type": "string", "const": value})
}

/// `Type.Union([Type.String(), Type.Null()])`.
fn nullable_string() -> Value {
    json!({"anyOf": [{"type": "string"}, {"type": "null"}]})
}

fn cost_tier_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "inputTokensAbove": {"type": "number"},
            "input": {"type": "number"},
            "output": {"type": "number"},
            "cacheRead": {"type": "number"},
            "cacheWrite": {"type": "number"}
        },
        "required": ["inputTokensAbove", "input", "output", "cacheRead", "cacheWrite"]
    })
}

fn cost_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "input": {"type": "number"},
            "output": {"type": "number"},
            "cacheRead": {"type": "number"},
            "cacheWrite": {"type": "number"},
            "tiers": {"type": "array", "items": cost_tier_schema()}
        },
        "required": ["input", "output", "cacheRead", "cacheWrite"]
    })
}

fn optional_cost_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "input": {"type": "number"},
            "output": {"type": "number"},
            "cacheRead": {"type": "number"},
            "cacheWrite": {"type": "number"},
            "tiers": {"type": "array", "items": cost_tier_schema()}
        }
    })
}

fn thinking_level_map_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "off": nullable_string(),
            "minimal": nullable_string(),
            "low": nullable_string(),
            "medium": nullable_string(),
            "high": nullable_string(),
            "xhigh": nullable_string(),
            "max": nullable_string()
        }
    })
}

fn percentile_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "p50": {"type": "number"},
            "p75": {"type": "number"},
            "p90": {"type": "number"},
            "p99": {"type": "number"}
        }
    })
}

fn openrouter_routing_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "allow_fallbacks": {"type": "boolean"},
            "require_parameters": {"type": "boolean"},
            "data_collection": {"anyOf": [string_literal("deny"), string_literal("allow")]},
            "zdr": {"type": "boolean"},
            "enforce_distillable_text": {"type": "boolean"},
            "order": {"type": "array", "items": {"type": "string"}},
            "only": {"type": "array", "items": {"type": "string"}},
            "ignore": {"type": "array", "items": {"type": "string"}},
            "quantizations": {"type": "array", "items": {"type": "string"}},
            "sort": {"anyOf": [
                {"type": "string"},
                {"type": "object", "properties": {
                    "by": {"type": "string"},
                    "partition": nullable_string()
                }}
            ]},
            "max_price": {"type": "object", "properties": {
                "prompt": {"anyOf": [{"type": "number"}, {"type": "string"}]},
                "completion": {"anyOf": [{"type": "number"}, {"type": "string"}]},
                "image": {"anyOf": [{"type": "number"}, {"type": "string"}]},
                "audio": {"anyOf": [{"type": "number"}, {"type": "string"}]},
                "request": {"anyOf": [{"type": "number"}, {"type": "string"}]}
            }},
            "preferred_min_throughput": {"anyOf": [{"type": "number"}, percentile_schema()]},
            "preferred_max_latency": {"anyOf": [{"type": "number"}, percentile_schema()]}
        }
    })
}

fn chat_template_kwarg_schema() -> Value {
    json!({
        "anyOf": [
            {"type": "string"},
            {"type": "number"},
            {"type": "boolean"},
            {"type": "null"},
            {
                "type": "object",
                "properties": {
                    "$var": {"anyOf": [string_literal("thinking.enabled"), string_literal("thinking.effort")]},
                    "omitWhenOff": {"type": "boolean"}
                },
                "required": ["$var"]
            }
        ]
    })
}

fn openai_completions_compat_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "supportsStore": {"type": "boolean"},
            "supportsDeveloperRole": {"type": "boolean"},
            "supportsReasoningEffort": {"type": "boolean"},
            "supportsUsageInStreaming": {"type": "boolean"},
            "maxTokensField": {"anyOf": [string_literal("max_completion_tokens"), string_literal("max_tokens")]},
            "requiresToolResultName": {"type": "boolean"},
            "requiresAssistantAfterToolResult": {"type": "boolean"},
            "requiresThinkingAsText": {"type": "boolean"},
            "requiresReasoningContentOnAssistantMessages": {"type": "boolean"},
            "thinkingFormat": {"anyOf": [
                string_literal("openai"), string_literal("openrouter"), string_literal("together"),
                string_literal("baseten"), string_literal("deepseek"), string_literal("zai"),
                string_literal("qwen"), string_literal("chat-template"), string_literal("qwen-chat-template"),
                string_literal("string-thinking"), string_literal("ant-ling")
            ]},
            "chatTemplateKwargs": record(chat_template_kwarg_schema()),
            "chatTemplateArgs": record(chat_template_kwarg_schema()),
            "cacheControlFormat": string_literal("anthropic"),
            "openRouterRouting": openrouter_routing_schema(),
            "vercelGatewayRouting": {"type": "object", "properties": {
                "only": {"type": "array", "items": {"type": "string"}},
                "order": {"type": "array", "items": {"type": "string"}}
            }},
            "supportsOpenAIGrammarTools": {"type": "boolean"},
            "supportsStrictMode": {"type": "boolean"},
            "sendSessionAffinityHeaders": {"type": "boolean"},
            "deferredToolsMode": string_literal("kimi"),
            "sessionAffinityFormat": {"anyOf": [
                string_literal("openai"), string_literal("openai-nosession"), string_literal("openrouter")
            ]},
            "supportsLongCacheRetention": {"type": "boolean"}
        }
    })
}

fn openai_responses_compat_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "supportsDeveloperRole": {"type": "boolean"},
            "sessionAffinityFormat": {"anyOf": [
                string_literal("openai"), string_literal("openai-nosession"), string_literal("openrouter")
            ]},
            "supportsLongCacheRetention": {"type": "boolean"},
            "supportsStrictMode": {"type": "boolean"},
            "supportsOpenAIGrammarTools": {"type": "boolean"},
            "supportsAdditionalTools": {"type": "boolean"},
            "supportsToolSearch": {"type": "boolean"}
        }
    })
}

fn anthropic_messages_compat_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "supportsEagerToolInputStreaming": {"type": "boolean"},
            "supportsLongCacheRetention": {"type": "boolean"},
            "sendSessionAffinityHeaders": {"type": "boolean"},
            "supportsCacheControlOnTools": {"type": "boolean"},
            "supportsTemperature": {"type": "boolean"},
            "forceAdaptiveThinking": {"type": "boolean"},
            "allowEmptySignature": {"type": "boolean"},
            "supportsStrictTools": {"type": "boolean"},
            "supportsToolReferences": {"type": "boolean"}
        }
    })
}

fn provider_compat_schema() -> Value {
    json!({
        "anyOf": [
            openai_completions_compat_schema(),
            openai_responses_compat_schema(),
            anthropic_messages_compat_schema()
        ]
    })
}

fn model_definition_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {"type": "string", "minLength": 1},
            "name": {"type": "string", "minLength": 1},
            "api": {"type": "string", "minLength": 1},
            "baseUrl": {"type": "string", "minLength": 1},
            "reasoning": {"type": "boolean"},
            "thinkingLevelMap": thinking_level_map_schema(),
            "input": {"type": "array", "items": {"anyOf": [string_literal("text"), string_literal("image")]}},
            "cost": cost_schema(),
            "contextWindow": {"type": "number"},
            "maxTokens": {"type": "number"},
            "samplingParams": {"type": "object"},
            "headers": record(json!({"type": "string"})),
            "compat": provider_compat_schema()
        },
        "required": ["id"]
    })
}

fn model_override_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {"type": "string", "minLength": 1},
            "reasoning": {"type": "boolean"},
            "thinkingLevelMap": thinking_level_map_schema(),
            "input": {"type": "array", "items": {"anyOf": [string_literal("text"), string_literal("image")]}},
            "cost": optional_cost_schema(),
            "contextWindow": {"type": "number"},
            "maxTokens": {"type": "number"},
            "samplingParams": {"type": "object"},
            "headers": record(json!({"type": "string"})),
            "compat": provider_compat_schema()
        }
    })
}

fn provider_config_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "name": {"type": "string", "minLength": 1},
            "baseUrl": {"type": "string", "minLength": 1},
            "apiKey": {"type": "string", "minLength": 1},
            "api": {"type": "string", "minLength": 1},
            "oauth": string_literal("radius"),
            "headers": record(json!({"type": "string"})),
            "compat": provider_compat_schema(),
            "authHeader": {"type": "boolean"},
            "models": {"type": "array", "items": model_definition_schema()},
            "modelOverrides": record(model_override_schema())
        }
    })
}

/// `ModelsConfigSchema`: `{ providers: Record<string, ProviderConfig> }`.
pub fn models_config_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "providers": record(provider_config_schema())
        },
        "required": ["providers"]
    })
}

/// Path helper for callers that need the default `models.json` location.
pub fn default_models_path() -> std::path::PathBuf {
    crate::config::get_models_path()
}

/// Whether a path points at an existing `models.json`.
pub fn models_json_exists(path: &str) -> bool {
    Path::new(path).exists()
}
