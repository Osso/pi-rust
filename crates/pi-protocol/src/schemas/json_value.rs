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
