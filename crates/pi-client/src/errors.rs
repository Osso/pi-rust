use std::error::Error;
use std::fmt::{self, Display, Formatter};

use pi_protocol::{JsonValue, ProtocolError, ProtocolErrorCode, ProtocolValidationError};

/// Error raised for a typed protocol error returned by the server.
#[derive(Debug, Clone, PartialEq)]
pub struct PiServerError {
    code: ProtocolErrorCode,
    message: String,
    details: Option<JsonValue>,
}

impl PiServerError {
    pub fn new(error: ProtocolError) -> Self {
        Self {
            code: error.code(),
            message: error.message().to_string(),
            details: error.details().cloned(),
        }
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

impl Display for PiServerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for PiServerError {}

/// Error raised when a client operation needs a connected transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiDisconnectedError {
    message: String,
}

impl PiDisconnectedError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Default for PiDisconnectedError {
    fn default() -> Self {
        Self::new("Pi client is disconnected")
    }
}

impl Display for PiDisconnectedError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for PiDisconnectedError {}

/// Error raised when a disposed client is used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PiClientDisposedError;

impl Display for PiClientDisposedError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("Pi client is disposed")
    }
}

impl Error for PiClientDisposedError {}

/// Error raised when a session lease mode conflicts with an existing lease.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiSessionOwnershipError {
    session_id: String,
    message: String,
}

impl PiSessionOwnershipError {
    pub fn new(session_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            message: message.into(),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for PiSessionOwnershipError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for PiSessionOwnershipError {}

/// Error raised when a session handle is no longer attached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PiSessionDetachedError {
    session_id: String,
}

impl PiSessionDetachedError {
    pub fn new(session_id: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

impl Display for PiSessionDetachedError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "Session {} is not attached", self.session_id)
    }
}

impl Error for PiSessionDetachedError {}

/// A typed failure surfaced by the client.
#[derive(Debug, Clone, PartialEq)]
pub enum PiError {
    Server(PiServerError),
    Disconnected(PiDisconnectedError),
    Disposed(PiClientDisposedError),
    SessionOwnership(PiSessionOwnershipError),
    SessionDetached(PiSessionDetachedError),
    Protocol(ProtocolValidationError),
    Transport(String),
    Io { message: String, raw_os_error: Option<i32> },
    InvalidArgument(String),
    Listener(String),
}

impl PiError {
    /// Returns the source-compatible error message.
    pub fn message(&self) -> String {
        match self {
            Self::Server(error) => error.message().to_string(),
            Self::Disconnected(error) => error.message().to_string(),
            Self::Disposed(error) => error.to_string(),
            Self::SessionOwnership(error) => error.message().to_string(),
            Self::SessionDetached(error) => error.to_string(),
            Self::Protocol(error) => error.to_string(),
            Self::Transport(message) | Self::InvalidArgument(message) | Self::Listener(message) => message.clone(),
            Self::Io { message, .. } => message.clone(),
        }
    }

    /// Returns the operating-system error number for I/O failures.
    pub const fn raw_os_error(&self) -> Option<i32> {
        match self {
            Self::Io { raw_os_error, .. } => *raw_os_error,
            _ => None,
        }
    }

    pub(crate) fn disconnected(message: impl Into<String>) -> Self {
        Self::Disconnected(PiDisconnectedError::new(message))
    }
}

impl Display for PiError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Server(error) => Display::fmt(error, formatter),
            Self::Disconnected(error) => Display::fmt(error, formatter),
            Self::Disposed(error) => Display::fmt(error, formatter),
            Self::SessionOwnership(error) => Display::fmt(error, formatter),
            Self::SessionDetached(error) => Display::fmt(error, formatter),
            Self::Protocol(error) => Display::fmt(error, formatter),
            Self::Transport(message) | Self::InvalidArgument(message) | Self::Listener(message) => {
                formatter.write_str(message)
            }
            Self::Io { message, .. } => formatter.write_str(message),
        }
    }
}

impl Error for PiError {}

impl From<ProtocolValidationError> for PiError {
    fn from(error: ProtocolValidationError) -> Self {
        Self::Protocol(error)
    }
}

impl From<std::io::Error> for PiError {
    fn from(error: std::io::Error) -> Self {
        Self::Io {
            message: error.to_string(),
            raw_os_error: error.raw_os_error(),
        }
    }
}

/// Converts any failure into the client's disconnected error while preserving an existing one.
pub fn to_disconnected_error(error: PiError) -> PiError {
    match error {
        PiError::Disconnected(_) => error,
        other => PiError::Disconnected(PiDisconnectedError::new(other.to_string())),
    }
}
