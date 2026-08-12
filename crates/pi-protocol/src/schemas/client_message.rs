use std::error::Error;
use std::fmt::{self, Display, Formatter};

use super::{ClientHello, Command};

/// A validated command request sent by a client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestEnvelope {
    id: String,
    request: Command,
}

impl RequestEnvelope {
    /// Creates a request envelope with a non-empty correlation identifier.
    pub fn new(id: String, request: Command) -> Result<Self, RequestEnvelopeError> {
        if id.is_empty() {
            return Err(RequestEnvelopeError::EmptyId);
        }
        Ok(Self { id, request })
    }

    /// Request correlation identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Validated protocol command.
    pub const fn request(&self) -> &Command {
        &self.request
    }
}

/// Returned when a request envelope violates its typed schema constraint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestEnvelopeError {
    EmptyId,
}

impl Display for RequestEnvelopeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyId => formatter.write_str("Request envelope id must not be empty"),
        }
    }
}

impl Error for RequestEnvelopeError {}

/// A validated message sent by a protocol client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientMessage {
    Hello(ClientHello),
    Request(RequestEnvelope),
}

impl ClientMessage {
    /// Returns the exact protocol message tag.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Hello(_) => "hello",
            Self::Request(_) => "request",
        }
    }
}
