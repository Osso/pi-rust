/// JSON-shaped data carried by unrestricted protocol fields.
///
/// Numbers must be finite before encoding; byte strings are not JSON values and are rejected by
/// the protocol parser.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

impl JsonValue {
    /// Converts a `serde_json` value into the protocol's JSON-compatible subset.
    ///
    /// Typed Rust boundaries already exclude values without a JSON representation; numbers use
    /// the JavaScript double value (`as_f64`).
    pub fn from_serde_json(value: &serde_json::Value) -> Self {
        match value {
            serde_json::Value::Null => Self::Null,
            serde_json::Value::Bool(value) => Self::Bool(*value),
            serde_json::Value::Number(value) => Self::Number(value.as_f64().unwrap_or_default()),
            serde_json::Value::String(value) => Self::String(value.clone()),
            serde_json::Value::Array(values) => Self::Array(values.iter().map(Self::from_serde_json).collect()),
            serde_json::Value::Object(values) => Self::Object(
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), Self::from_serde_json(value)))
                    .collect(),
            ),
        }
    }
}
