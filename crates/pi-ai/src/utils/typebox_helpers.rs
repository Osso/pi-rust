//! Port of `packages/ai/src/utils/typebox-helpers.ts`.
//!
//! TypeScript's TypeBox schemas carry a hidden `~kind` marker (and
//! `Symbol.for("TypeBox.Kind")` in older TypeBox releases). A
//! [`crate::types::Tool`] records that origin as `typebox_kind`, because
//! `Tool::parameters` is a plain `serde_json::Value` and cannot hold hidden
//! properties. [`string_enum`] returns [`TypeBoxSchema`] so a schema built by
//! the TypeBox-equivalent helper cannot lose its origin by accident.

use serde_json::{Map, Value, json};

use crate::types::TypeBoxSchema;

/// `StringEnum(values, options?)` options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StringEnumOptions {
    pub description: Option<String>,
    pub default: Option<String>,
}

/// Creates a string enum schema compatible with Google's API and other
/// providers that don't support `anyOf`/`const` patterns.
pub fn string_enum(values: &[&str], options: StringEnumOptions) -> TypeBoxSchema {
    let mut schema = Map::new();
    schema.insert("type".to_owned(), json!("string"));
    schema.insert(
        "enum".to_owned(),
        Value::Array(values.iter().map(|value| json!(value)).collect()),
    );
    // TypeScript spreads `...(options?.description && { description })`, so
    // empty strings are omitted.
    if let Some(description) = options.description.filter(|value| !value.is_empty()) {
        schema.insert("description".to_owned(), Value::String(description));
    }
    if let Some(default) = options.default.filter(|value| !value.is_empty()) {
        schema.insert("default".to_owned(), Value::String(default));
    }
    TypeBoxSchema::new(Value::Object(schema))
}
