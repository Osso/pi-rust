//! Single-authority compatibility JSON. Known fields and unknown properties are
//! disjoint; accessors implement the exact JavaScript read at the consumer.
use super::{
    CacheControlFormat, ChatTemplateKwargValue, MaxTokensField, ModelProperties, OpenRouterRouting, ThinkingFormat,
    VercelGatewayRouting,
};
use indexmap::IndexMap;
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub trait CompatFields {
    fn raw(&self, key: &str) -> Option<&JsValue>;

    /// `field ?? detected`, without coercion or replacement of non-null values.
    fn nullish_default(&self, key: &str, detected: JsValue) -> JsValue {
        self.raw(key)
            .filter(|value| !matches!(value, JsValue::Null))
            .cloned()
            .unwrap_or(detected)
    }

    /// Truthiness after the frozen nullish auto-detection/default step.
    fn truthy(&self, key: &str, detected: bool) -> bool {
        js_json::is_truthy(&self.nullish_default(key, JsValue::Bool(detected)))
    }

    /// `resolved !== false`, not truthiness: numeric zero is not boolean false.
    fn not_false(&self, key: &str, detected: bool) -> bool {
        self.nullish_default(key, JsValue::Bool(detected)) != JsValue::Bool(false)
    }

    /// `field === true`, without a truthiness conversion.
    fn is_true(&self, key: &str) -> bool {
        self.raw(key) == Some(&JsValue::Bool(true))
    }

    /// Strict string equality after a nullish default; never calls String(value).
    fn string_is(&self, key: &str, expected: &str, detected: Option<&str>) -> bool {
        match self.raw(key).filter(|value| !matches!(value, JsValue::Null)) {
            Some(JsValue::String(value)) => value == expected,
            Some(_) => false,
            None => detected == Some(expected),
        }
    }
}

/// Own enumerable properties used by frozen object spread and non-null
/// Object.entries calls. Strings expose UTF16 units, not Unicode scalars.
pub fn spread_compat_properties(value: &JsValue) -> JsObject {
    match value {
        JsValue::Object(properties) => properties.clone(),
        JsValue::Array(values) => values
            .iter()
            .enumerate()
            .map(|(index, value)| (index.to_string().into(), value.clone()))
            .collect(),
        JsValue::String(text) => text
            .units()
            .iter()
            .enumerate()
            .map(|(index, unit)| {
                (
                    index.to_string().into(),
                    JsValue::String(JsString::from_utf16(vec![*unit])),
                )
            })
            .collect(),
        JsValue::Null | JsValue::Bool(_) | JsValue::Number(_) => JsObject::new(),
    }
}

macro_rules! compat_record {
    ($name:ident, $wire:ident, $catalog:ident { $( $field:ident: $kind:ty => $key:literal ),* $(,)? }) => {
        #[derive(Debug, Clone, Default, PartialEq)]
        pub struct $name {
            $(pub $field: Option<JsValue>,)*
            pub properties: ModelProperties,
        }

        #[derive(Serialize, Deserialize)]
        struct $wire {
            $(#[serde(rename = $key, default, skip_serializing_if = "Option::is_none", deserialize_with = "pi_js_compat::js_json::deserialize_present")]
            $field: Option<JsValue>,)*
        }

        #[derive(Deserialize)]
        struct $catalog {
            $(#[serde(rename = $key)] $field: Option<$kind>,)*
        }

        impl $name {
            /// In-memory projection, not JSON serialization: native numbers
            /// and units stay unchanged when registration reconstructs a model.
            pub(super) fn to_raw_value(&self) -> JsValue {
                let known: JsObject=[$(($key,self.$field.as_ref()),)*].into_iter()
                    .filter_map(|(key,value)|value.map(|value|(key.into(),value.clone()))).collect();
                JsValue::Object(self.properties.reorder(known))
            }

            /// The persisted catalog's existing wrong-known-type boundary is
            /// separate from schema-validated models.json ingress.
            pub fn validate_catalog(value: &JsValue) -> Result<(), serde_json::Error> {
                let typed: $catalog = js_json::from_value(value)?;
                let _ = ($(typed.$field,)*);
                Ok(())
            }
        }

        impl CompatFields for $name {
            fn raw(&self, key: &str) -> Option<&JsValue> {
                match key {
                    $($key => self.$field.as_ref(),)*
                    _ => self.properties.extra().get(&JsString::from(key)),
                }
            }
        }

        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                self.properties.serialize(&$wire { $($field: self.$field.clone(),)* }, serializer)
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self,D::Error> {
                let value=JsValue::deserialize(deserializer)?;
                let (wire,properties): ($wire,_)=ModelProperties::decode(value,&[$($key,)*]).map_err(serde::de::Error::custom)?;
                Ok(Self { $($field:wire.$field,)* properties })
            }
        }
    };
}

compat_record!(OpenAICompletionsCompat, CompletionsWire, CompletionsCatalog {
    supports_store: bool => "supportsStore",
    supports_developer_role: bool => "supportsDeveloperRole",
    supports_reasoning_effort: bool => "supportsReasoningEffort",
    supports_usage_in_streaming: bool => "supportsUsageInStreaming",
    max_tokens_field: MaxTokensField => "maxTokensField",
    requires_tool_result_name: bool => "requiresToolResultName",
    requires_assistant_after_tool_result: bool => "requiresAssistantAfterToolResult",
    requires_thinking_as_text: bool => "requiresThinkingAsText",
    requires_reasoning_content_on_assistant_messages: bool => "requiresReasoningContentOnAssistantMessages",
    thinking_format: ThinkingFormat => "thinkingFormat",
    chat_template_kwargs: IndexMap<String,ChatTemplateKwargValue> => "chatTemplateKwargs",
    open_router_routing: OpenRouterRouting => "openRouterRouting",
    vercel_gateway_routing: VercelGatewayRouting => "vercelGatewayRouting",
    zai_tool_stream: bool => "zaiToolStream",
    supports_strict_mode: bool => "supportsStrictMode",
    cache_control_format: CacheControlFormat => "cacheControlFormat",
    send_session_affinity_headers: bool => "sendSessionAffinityHeaders",
    supports_long_cache_retention: bool => "supportsLongCacheRetention",
});

compat_record!(OpenAIResponsesCompat, ResponsesWire, ResponsesCatalog {
    supports_developer_role: bool => "supportsDeveloperRole",
    send_session_id_header: bool => "sendSessionIdHeader",
    supports_long_cache_retention: bool => "supportsLongCacheRetention",
});

compat_record!(AnthropicMessagesCompat, AnthropicWire, AnthropicCatalog {
    supports_eager_tool_input_streaming: bool => "supportsEagerToolInputStreaming",
    supports_long_cache_retention: bool => "supportsLongCacheRetention",
    send_session_affinity_headers: bool => "sendSessionAffinityHeaders",
    supports_cache_control_on_tools: bool => "supportsCacheControlOnTools",
    supports_temperature: bool => "supportsTemperature",
    force_adaptive_thinking: bool => "forceAdaptiveThinking",
    allow_empty_signature: bool => "allowEmptySignature",
});
