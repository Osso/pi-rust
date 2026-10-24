//! Known nullable thinking values and unknown-only JSON properties share one
//! ordered wire object. No known value is stored in the unknown-property map.
use std::ops::{Deref, DerefMut};

use indexmap::IndexMap;
use pi_js_compat::js_json::{JsObject, JsString, JsValue};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{ModelProperties, ModelThinkingLevel};

const THINKING_LEVEL_FIELDS: &[&str] = &["off", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"];

type KnownThinkingValues = IndexMap<ModelThinkingLevel, Option<JsString>>;

/// `Partial<Record<ModelThinkingLevel, string | null>>`, with the additional
/// properties accepted by the frozen models.json schema. A present `None`
/// remains explicit null; an absent known key remains undefined.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThinkingLevelMap {
    values: KnownThinkingValues,
    properties: ModelProperties,
}

impl ThinkingLevelMap {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn unknown_properties(&self) -> &JsObject {
        self.properties.extra()
    }
}

impl Deref for ThinkingLevelMap {
    type Target = KnownThinkingValues;

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

impl DerefMut for ThinkingLevelMap {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.values
    }
}

impl FromIterator<(ModelThinkingLevel, Option<JsString>)> for ThinkingLevelMap {
    fn from_iter<T: IntoIterator<Item = (ModelThinkingLevel, Option<JsString>)>>(values: T) -> Self {
        Self {
            values: values.into_iter().collect(),
            properties: ModelProperties::default(),
        }
    }
}

impl Serialize for ThinkingLevelMap {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.properties.serialize(&self.values, serializer)
    }
}

impl<'de> Deserialize<'de> for ThinkingLevelMap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = JsValue::deserialize(deserializer)?;
        let (values, properties) =
            ModelProperties::decode(value, THINKING_LEVEL_FIELDS).map_err(serde::de::Error::custom)?;
        Ok(Self { values, properties })
    }
}
