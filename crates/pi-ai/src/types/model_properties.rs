//! Unknown-only model properties and original key order; known values remain in typed fields.
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue};
use serde::{Serialize, Serializer};

#[derive(Debug, Clone, Default)]
pub struct ModelProperties {
    order: Vec<JsString>,
    extra: JsObject,
}

impl PartialEq for ModelProperties {
    fn eq(&self, other: &Self) -> bool {
        // Ordering is serialization metadata, not a second set of model values.
        self.extra == other.extra
    }
}

impl ModelProperties {
    pub fn extra(&self) -> &JsObject {
        &self.extra
    }

    pub(super) fn split(object: JsObject, fields: &[&str]) -> (JsObject, Self) {
        let order = object.keys().cloned().collect();
        let (known, extra) = object
            .into_iter()
            .partition(|(key, _)| fields.iter().any(|field| key == *field));
        (known, Self { order, extra })
    }

    pub fn decode<T: serde::de::DeserializeOwned>(
        value: JsValue,
        fields: &[&str],
    ) -> Result<(T, Self), serde_json::Error> {
        let JsValue::Object(object) = value else {
            return Err(serde::de::Error::custom("model property must be an object"));
        };
        let (known, properties) = Self::split(object, fields);
        Ok((js_json::from_value(&JsValue::Object(known))?, properties))
    }

    pub fn serialize<S: Serializer, T: Serialize>(&self, fields: &T, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = serde_json::to_string(fields).map_err(serde::ser::Error::custom)?;
        let value = js_json::parse_lossless(wire).map_err(serde::ser::Error::custom)?;
        let JsValue::Object(known) = value else {
            return Err(serde::ser::Error::custom("model fields must serialize as an object"));
        };
        JsValue::Object(self.reorder(known)).serialize(serializer)
    }

    pub(super) fn reorder(&self, mut known: JsObject) -> JsObject {
        let mut ordered = JsObject::new();
        for key in &self.order {
            if let Some(value) = known.shift_remove(key).or_else(|| self.extra.get(key).cloned()) {
                ordered.insert(key.clone(), value);
            }
        }
        ordered.extend(known);
        ordered
    }
}
