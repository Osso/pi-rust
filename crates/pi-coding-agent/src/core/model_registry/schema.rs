//! TypeBox schema from the frozen registry module.
use serde_json::{Value, json};

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

fn cost_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "input": {"type": "number"},
            "output": {"type": "number"},
            "cacheRead": {"type": "number"},
            "cacheWrite": {"type": "number"}
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
            "cacheWrite": {"type": "number"}
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
            "max": nullable_string(),
            "ultra": nullable_string()
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
                string_literal("deepseek"), string_literal("zai"),
                string_literal("qwen"), string_literal("chat-template"), string_literal("qwen-chat-template"),
                string_literal("string-thinking"), string_literal("ant-ling")
            ]},
            "chatTemplateKwargs": record(chat_template_kwarg_schema()),
            "cacheControlFormat": string_literal("anthropic"),
            "openRouterRouting": openrouter_routing_schema(),
            "vercelGatewayRouting": {"type": "object", "properties": {
                "only": {"type": "array", "items": {"type": "string"}},
                "order": {"type": "array", "items": {"type": "string"}}
            }},
            "supportsStrictMode": {"type": "boolean"},
            "supportsLongCacheRetention": {"type": "boolean"}
        }
    })
}

fn openai_responses_compat_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "supportsDeveloperRole": {"type": "boolean"},
            "sendSessionIdHeader": {"type": "boolean"},
            "supportsLongCacheRetention": {"type": "boolean"}
        }
    })
}

fn anthropic_messages_compat_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "supportsEagerToolInputStreaming": {"type": "boolean"},
            "supportsLongCacheRetention": {"type": "boolean"},
            "supportsCacheControlOnTools": {"type": "boolean"},
            "forceAdaptiveThinking": {"type": "boolean"}
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
            "headers": record(json!({"type": "string"})),
            "compat": provider_compat_schema(),
            "authHeader": {"type": "boolean"},
            "models": {"type": "array", "items": model_definition_schema()},
            "modelOverrides": record(model_override_schema())
        }
    })
}

/// `ModelsConfigSchema`: `{ providers: Record<string, ProviderConfig> }`.
pub(super) fn models_config_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "providers": record(provider_config_schema())
        },
        "required": ["providers"]
    })
}
