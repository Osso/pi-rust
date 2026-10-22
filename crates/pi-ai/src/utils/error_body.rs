use pi_js_compat::js_json::{JsString, JsValue as Value};

use pi_js_compat::{js_json, number::js_number_to_string};

pub const MAX_PROVIDER_ERROR_BODY_CHARS: usize = 4000;

#[derive(Debug, Clone, PartialEq)]
pub struct NormalizedProviderError {
    pub status: Option<f64>,
    pub body: Option<JsString>,
    pub message: JsString,
    pub message_carries_body: bool,
}

/// Raw HTTP body shape carried by an SDK error.
///
/// Callers pass text or already-parsed JSON. JSON strings have the same
/// selection semantics as `Text`; JSON arrays and objects retain their shape.
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorBody {
    Text(JsString),
    Json(Value),
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ProviderError {
    pub message: JsString,
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

fn extract_body(error: &ProviderError) -> Option<JsString> {
    let body_text = pick_body_text(error)?;
    let trimmed = body_text.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(truncate_error_text(&trimmed, MAX_PROVIDER_ERROR_BODY_CHARS))
}

fn pick_body_text(error: &ProviderError) -> Option<JsString> {
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

pub fn format_provider_error(normalized: &NormalizedProviderError, prefix: Option<&str>) -> JsString {
    let carries_body = normalized.message_carries_body || normalized.status.is_none() || normalized.body.is_none();

    if carries_body {
        return match (prefix, normalized.status) {
            (Some(prefix), Some(status)) => JsString::join(
                [
                    JsString::from(format!("{prefix} ({}): ", js_number_to_string(status))),
                    normalized.message.clone(),
                ],
                "",
            ),
            _ => normalized.message.clone(),
        };
    }

    let status = normalized.status.expect("status is present when the body is surfaced");
    let body = normalized.body.as_ref().expect("body is present when it is surfaced");
    match prefix {
        Some(prefix) => JsString::join(
            [
                JsString::from(format!("{prefix} ({}): ", js_number_to_string(status))),
                body.clone(),
            ],
            "",
        ),
        None => JsString::join(
            [
                JsString::from(format!("{}: ", js_number_to_string(status))),
                body.clone(),
            ],
            "",
        ),
    }
}

pub fn truncate_error_text(text: &JsString, max_chars: usize) -> JsString {
    if text.len() <= max_chars {
        return text.clone();
    }
    JsString::join(
        [
            text.slice(0, max_chars),
            format!("... [truncated {} chars]", text.len() - max_chars).into(),
        ],
        "",
    )
}

pub fn safe_json_stringify(value: &Value) -> JsString {
    js_json::stringify_lossless(value).into()
}
