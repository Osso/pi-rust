//! Declared RPC client model shapes, with exact unknown properties and key order.

use pi_ai::types::ModelProperties;
use pi_js_compat::js_json::{JsString, JsValue};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct ModelReference {
    pub provider: JsString,
    pub id: JsString,
    pub properties: ModelProperties,
}

#[derive(Serialize, Deserialize)]
struct ReferenceFields {
    provider: JsString,
    id: JsString,
}

impl Serialize for ModelReference {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.properties.serialize(
            &ReferenceFields {
                provider: self.provider.clone(),
                id: self.id.clone(),
            },
            serializer,
        )
    }
}

impl<'de> Deserialize<'de> for ModelReference {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = JsValue::deserialize(deserializer)?;
        let (fields, properties) =
            ModelProperties::decode::<ReferenceFields>(raw, &["provider", "id"]).map_err(serde::de::Error::custom)?;
        Ok(Self {
            provider: fields.provider,
            id: fields.id,
            properties,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelInfo {
    pub provider: JsString,
    pub id: JsString,
    pub context_window: f64,
    pub reasoning: bool,
    pub properties: ModelProperties,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InfoFields {
    provider: JsString,
    id: JsString,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    context_window: f64,
    reasoning: bool,
}

impl Serialize for ModelInfo {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.properties.serialize(
            &InfoFields {
                provider: self.provider.clone(),
                id: self.id.clone(),
                context_window: self.context_window,
                reasoning: self.reasoning,
            },
            serializer,
        )
    }
}

impl<'de> Deserialize<'de> for ModelInfo {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = JsValue::deserialize(deserializer)?;
        let (fields, properties) =
            ModelProperties::decode::<InfoFields>(raw, &["provider", "id", "contextWindow", "reasoning"])
                .map_err(serde::de::Error::custom)?;
        Ok(Self {
            provider: fields.provider,
            id: fields.id,
            context_window: fields.context_window,
            reasoning: fields.reasoning,
            properties,
        })
    }
}
