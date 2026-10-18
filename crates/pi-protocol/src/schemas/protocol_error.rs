use super::{JsonValue, ProtocolErrorCode};

/// A protocol-level error payload.
#[derive(Debug, Clone, PartialEq)]
pub struct ProtocolError {
    code: ProtocolErrorCode,
    message: String,
    details: Option<JsonValue>,
}

impl ProtocolError {
    pub const fn new(code: ProtocolErrorCode, message: String, details: Option<JsonValue>) -> Self {
        Self { code, message, details }
    }

    pub const fn code(&self) -> ProtocolErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub const fn details(&self) -> Option<&JsonValue> {
        self.details.as_ref()
    }
}
