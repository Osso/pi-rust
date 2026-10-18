use std::error::Error;
use std::fmt::{self, Display, Formatter};

use pi_protocol::{JsonValue, ProtocolErrorCode};

/// Service/runtime error codes that can safely cross the protocol boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PiServerOperationErrorCode {
    Busy,
    SessionLocked,
    NotFound,
    InvalidRequest,
    NotImplemented,
}

impl PiServerOperationErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Busy => "busy",
            Self::SessionLocked => "session_locked",
            Self::NotFound => "not_found",
            Self::InvalidRequest => "invalid_request",
            Self::NotImplemented => "not_implemented",
        }
    }

    pub const fn protocol_code(self) -> ProtocolErrorCode {
        match self {
            Self::Busy => ProtocolErrorCode::Busy,
            Self::SessionLocked => ProtocolErrorCode::SessionLocked,
            Self::NotFound => ProtocolErrorCode::NotFound,
            Self::InvalidRequest => ProtocolErrorCode::InvalidRequest,
            Self::NotImplemented => ProtocolErrorCode::NotImplemented,
        }
    }
}

pub const INTERNAL_SERVER_ERROR_MESSAGE: &str = "Internal server error";
pub const NOT_IMPLEMENTED_MESSAGE: &str = "Operation is not implemented";

/// A service/runtime error that can safely cross the protocol boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct PiServerError {
    code: PiServerOperationErrorCode,
    message: String,
    details: Option<JsonValue>,
}

impl PiServerError {
    pub fn new(code: PiServerOperationErrorCode, message: impl Into<String>, details: Option<JsonValue>) -> Self {
        Self {
            code,
            message: message.into(),
            details,
        }
    }

    pub const fn code(&self) -> PiServerOperationErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub const fn details(&self) -> Option<&JsonValue> {
        self.details.as_ref()
    }

    pub fn session_busy(message: impl Into<String>, details: Option<JsonValue>) -> Self {
        Self::new(PiServerOperationErrorCode::Busy, message, details)
    }

    pub fn session_locked(message: impl Into<String>, details: Option<JsonValue>) -> Self {
        Self::new(PiServerOperationErrorCode::SessionLocked, message, details)
    }

    pub fn session_not_found(message: impl Into<String>, details: Option<JsonValue>) -> Self {
        Self::new(PiServerOperationErrorCode::NotFound, message, details)
    }

    pub fn invalid_request(message: impl Into<String>, details: Option<JsonValue>) -> Self {
        Self::new(PiServerOperationErrorCode::InvalidRequest, message, details)
    }

    pub fn not_implemented() -> Self {
        Self::new(
            PiServerOperationErrorCode::NotImplemented,
            NOT_IMPLEMENTED_MESSAGE,
            None,
        )
    }
}

impl Display for PiServerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for PiServerError {}

/// An unsafe failure whose cause is retained for reporting but never serialized.
#[derive(Debug)]
pub struct InternalServerError {
    cause: Box<dyn Error + Send + Sync>,
}

impl InternalServerError {
    pub fn new(cause: Box<dyn Error + Send + Sync>) -> Self {
        Self { cause }
    }

    pub fn cause(&self) -> &(dyn Error + Send + Sync + 'static) {
        self.cause.as_ref()
    }
}

impl Display for InternalServerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(INTERNAL_SERVER_ERROR_MESSAGE)
    }
}

impl Error for InternalServerError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.cause.as_ref())
    }
}
