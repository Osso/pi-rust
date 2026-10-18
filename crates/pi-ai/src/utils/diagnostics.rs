use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize, Serializer};
use serde_json::{Map, Value};

use super::js_number::js_number_to_string;

/// Error fields extracted from a JavaScript `Error`-shaped throw.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticErrorInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<DiagnosticCode>,
}

/// The `string | number` subset of `Error.code` that diagnostics preserve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DiagnosticCode {
    String(String),
    Number(#[serde(serialize_with = "serialize_js_number")] f64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantMessageDiagnostic {
    #[serde(rename = "type")]
    pub r#type: String,
    pub timestamp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<DiagnosticErrorInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Map<String, Value>>,
}

/// A thrown value in the shapes diagnostics can observe.
///
/// JavaScript class instances, symbols, functions, and `undefined` are not
/// representable; they are outside the typed Rust core API.
#[derive(Debug, Clone, PartialEq)]
pub enum ThrownValue {
    Error(DiagnosticErrorInfo),
    Text(String),
    Json(Value),
}

pub fn format_thrown_value(value: &ThrownValue) -> String {
    match value {
        ThrownValue::Error(error) => format_error_message(error),
        ThrownValue::Text(text) => text.clone(),
        ThrownValue::Json(json) => js_string(json),
    }
}

pub fn extract_diagnostic_error(error: &ThrownValue) -> DiagnosticErrorInfo {
    match error {
        ThrownValue::Error(info) => DiagnosticErrorInfo {
            name: info.name.clone().filter(|name| !name.is_empty()),
            message: if info.message.is_empty() {
                info.name.clone().unwrap_or_default()
            } else {
                info.message.clone()
            },
            stack: info.stack.clone(),
            code: info.code.clone(),
        },
        _ => DiagnosticErrorInfo {
            name: Some("ThrownValue".to_owned()),
            message: format_thrown_value(error),
            stack: None,
            code: None,
        },
    }
}

pub fn create_assistant_message_diagnostic(
    r#type: impl Into<String>,
    error: &ThrownValue,
    details: Option<Map<String, Value>>,
) -> AssistantMessageDiagnostic {
    AssistantMessageDiagnostic {
        r#type: r#type.into(),
        timestamp: now_millis(),
        error: Some(extract_diagnostic_error(error)),
        details,
    }
}

pub fn append_assistant_message_diagnostic(
    diagnostics: &mut Option<Vec<AssistantMessageDiagnostic>>,
    diagnostic: AssistantMessageDiagnostic,
) {
    diagnostics.get_or_insert_with(Vec::new).push(diagnostic);
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn format_error_message(error: &DiagnosticErrorInfo) -> String {
    if error.message.is_empty() {
        error.name.clone().unwrap_or_default()
    } else {
        error.message.clone()
    }
}

fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_owned(),
        Value::Bool(boolean) => boolean.to_string(),
        Value::Number(number) => match number.as_f64() {
            Some(float) => js_number_to_string(float),
            None => number.to_string(),
        },
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => js_string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_owned(),
    }
}

fn serialize_js_number<S: Serializer>(value: &f64, serializer: S) -> Result<S::Ok, S::Error> {
    if value.fract() == 0.0 && value.abs() <= i64::MAX as f64 {
        serializer.serialize_i64(*value as i64)
    } else {
        serializer.serialize_f64(*value)
    }
}
