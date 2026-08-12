use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

/// Error codes exposed by the protocol schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolErrorCode {
    Version,
    Busy,
    SessionLocked,
    NotFound,
    InvalidRequest,
    NotImplemented,
    InternalError,
}

impl ProtocolErrorCode {
    /// Returns the exact protocol literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Version => "version",
            Self::Busy => "busy",
            Self::SessionLocked => "session_locked",
            Self::NotFound => "not_found",
            Self::InvalidRequest => "invalid_request",
            Self::NotImplemented => "not_implemented",
            Self::InternalError => "internal_error",
        }
    }
}

/// Returned when a string is not a protocol error-code literal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseProtocolErrorCodeError;

impl Display for ParseProtocolErrorCodeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("unknown protocol error code")
    }
}

impl Error for ParseProtocolErrorCodeError {}

impl FromStr for ProtocolErrorCode {
    type Err = ParseProtocolErrorCodeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "version" => Ok(Self::Version),
            "busy" => Ok(Self::Busy),
            "session_locked" => Ok(Self::SessionLocked),
            "not_found" => Ok(Self::NotFound),
            "invalid_request" => Ok(Self::InvalidRequest),
            "not_implemented" => Ok(Self::NotImplemented),
            "internal_error" => Ok(Self::InternalError),
            _ => Err(ParseProtocolErrorCodeError),
        }
    }
}
