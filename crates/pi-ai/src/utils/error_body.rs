use serde_json::Value;

use pi_js_compat::{js_json, number::js_number_to_string, string::js_trim};

pub const MAX_PROVIDER_ERROR_BODY_CHARS: usize = 4000;

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedProviderError {
    pub status: Option<f64>,
    pub body: Option<String>,
    pub message: String,
    pub message_carries_body: bool,
}

/// Raw HTTP body shape carried by an SDK error.
///
/// Callers pass text or already-parsed JSON. JSON strings have the same
/// selection semantics as `Text`; JSON arrays and objects retain their shape.
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorBody {
    Text(String),
    Json(Value),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProviderError {
    pub message: String,
    pub status_code: Option<f64>,
    pub status: Option<f64>,
    pub body: Option<ErrorBody>,
    pub error: Option<Value>,
    pub metadata_http_status_code: Option<f64>,
    pub response_status_code: Option<f64>,
    pub response_body: Option<ErrorBody>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProviderErrorInput {
    Error(Box<ProviderError>),
    Value(Value),
}

pub fn normalize_provider_error(input: &ProviderErrorInput) -> NormalizedProviderError {
    match input {
        ProviderErrorInput::Value(value) => NormalizedProviderError {
            status: None,
            body: None,
            message: safe_json_stringify(value),
            message_carries_body: false,
        },
        ProviderErrorInput::Error(error) => normalize_error(error),
    }
}

fn normalize_error(error: &ProviderError) -> NormalizedProviderError {
    let body = extract_body(error);
    NormalizedProviderError {
        status: extract_status(error),
        message_carries_body: body.as_ref().is_none_or(|body| error.message.contains(body)),
        body,
        message: error.message.clone(),
    }
}

fn extract_status(error: &ProviderError) -> Option<f64> {
    error
        .status_code
        .or(error.status)
        .or(error.metadata_http_status_code)
        .or(error.response_status_code)
}

fn extract_body(error: &ProviderError) -> Option<String> {
    let body_text = pick_body_text(error)?;
    let trimmed = js_trim(&body_text);
    if trimmed.is_empty() {
        return None;
    }
    Some(truncate_error_text(trimmed, MAX_PROVIDER_ERROR_BODY_CHARS))
}

fn pick_body_text(error: &ProviderError) -> Option<String> {
    if let Some(ErrorBody::Text(text) | ErrorBody::Json(Value::String(text))) = &error.body {
        return Some(text.clone());
    }
    if let Some(value) = &error.error
        && is_non_empty_object(value)
    {
        return Some(safe_json_stringify(value));
    }
    match &error.response_body {
        Some(ErrorBody::Text(text) | ErrorBody::Json(Value::String(text))) => Some(text.clone()),
        Some(ErrorBody::Json(value)) if is_non_empty_object(value) => Some(safe_json_stringify(value)),
        _ => None,
    }
}

fn is_non_empty_object(value: &Value) -> bool {
    match value {
        Value::Object(object) => !object.is_empty(),
        Value::Array(array) => !array.is_empty(),
        _ => false,
    }
}

pub fn format_provider_error(normalized: &NormalizedProviderError, prefix: Option<&str>) -> String {
    let carries_body = normalized.message_carries_body || normalized.status.is_none() || normalized.body.is_none();

    if carries_body {
        return match (prefix, normalized.status) {
            (Some(prefix), Some(status)) => {
                format!("{prefix} ({}): {}", js_number_to_string(status), normalized.message)
            }
            _ => normalized.message.clone(),
        };
    }

    let status = normalized.status.expect("status is present when the body is surfaced");
    let body = normalized.body.as_deref().expect("body is present when it is surfaced");
    match prefix {
        Some(prefix) => format!("{prefix} ({}): {body}", js_number_to_string(status)),
        None => format!("{}: {body}", js_number_to_string(status)),
    }
}

pub fn truncate_error_text(text: &str, max_chars: usize) -> String {
    let length = text.encode_utf16().count();
    if length <= max_chars {
        return text.to_owned();
    }
    let prefix = take_utf16_units(text, max_chars);
    format!("{prefix}... [truncated {} chars]", length - max_chars)
}

fn take_utf16_units(text: &str, max_units: usize) -> String {
    let mut units = 0;
    text.chars()
        .take_while(|character| {
            let width = character.len_utf16();
            if units + width > max_units {
                return false;
            }
            units += width;
            true
        })
        .collect()
}

pub fn safe_json_stringify(value: &Value) -> String {
    js_json::stringify(value)
}
