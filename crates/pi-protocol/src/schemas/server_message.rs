use super::{CommandResult, ProtocolError, SchemaValueError, ServerSnapshot, SessionSnapshot, TranscriptProgress};

/// A session-progress event.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionProgress {
    session_id: String,
    progress: TranscriptProgress,
}

impl SessionProgress {
    pub fn new(session_id: String, progress: TranscriptProgress) -> Result<Self, SchemaValueError> {
        validate_session_id(&session_id)?;
        Ok(Self { session_id, progress })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub const fn progress(&self) -> &TranscriptProgress {
        &self.progress
    }
}

/// A session-removal event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionRemoved {
    session_id: String,
}

impl SessionRemoved {
    pub fn new(session_id: String) -> Result<Self, SchemaValueError> {
        validate_session_id(&session_id)?;
        Ok(Self { session_id })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

/// An event pushed by the server.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerEvent {
    ServerSnapshot(ServerSnapshot),
    SessionSnapshot(SessionSnapshot),
    SessionProgress(SessionProgress),
    SessionRemoved(SessionRemoved),
}

impl ServerEvent {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ServerSnapshot(_) => "server_snapshot",
            Self::SessionSnapshot(_) => "session_snapshot",
            Self::SessionProgress(_) => "session_progress",
            Self::SessionRemoved(_) => "session_removed",
        }
    }
}

/// The first server message on a connection.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerHello {
    connection_id: String,
    snapshot: ServerSnapshot,
}

impl ServerHello {
    pub fn new(connection_id: String, snapshot: ServerSnapshot) -> Result<Self, SchemaValueError> {
        if connection_id.is_empty() {
            return Err(SchemaValueError::EmptyConnectionId);
        }
        Ok(Self {
            connection_id,
            snapshot,
        })
    }

    pub fn connection_id(&self) -> &str {
        &self.connection_id
    }

    pub const fn snapshot(&self) -> &ServerSnapshot {
        &self.snapshot
    }
}

/// A failed server handshake.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerHelloError {
    error: ProtocolError,
}

impl ServerHelloError {
    pub const fn new(error: ProtocolError) -> Self {
        Self { error }
    }

    pub const fn error(&self) -> &ProtocolError {
        &self.error
    }
}

/// A successful command response.
#[derive(Debug, Clone, PartialEq)]
pub struct SuccessResponse {
    id: String,
    result: CommandResult,
}

impl SuccessResponse {
    pub fn new(id: String, result: CommandResult) -> Result<Self, SchemaValueError> {
        if id.is_empty() {
            return Err(SchemaValueError::EmptyId);
        }
        Ok(Self { id, result })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub const fn result(&self) -> &CommandResult {
        &self.result
    }
}

/// A failed command response.
#[derive(Debug, Clone, PartialEq)]
pub struct ErrorResponse {
    id: String,
    error: ProtocolError,
}

impl ErrorResponse {
    pub fn new(id: String, error: ProtocolError) -> Result<Self, SchemaValueError> {
        if id.is_empty() {
            return Err(SchemaValueError::EmptyId);
        }
        Ok(Self { id, error })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub const fn error(&self) -> &ProtocolError {
        &self.error
    }
}

/// A response envelope with either a result or an error.
#[derive(Debug, Clone, PartialEq)]
pub enum ResponseEnvelope {
    Success(SuccessResponse),
    Error(ErrorResponse),
}

/// An event envelope.
#[derive(Debug, Clone, PartialEq)]
pub struct EventEnvelope {
    event: ServerEvent,
}

impl EventEnvelope {
    pub const fn new(event: ServerEvent) -> Self {
        Self { event }
    }

    pub const fn event(&self) -> &ServerEvent {
        &self.event
    }
}

/// A validated message sent by a protocol server.
#[derive(Debug, Clone, PartialEq)]
pub enum ServerMessage {
    Hello(ServerHello),
    HelloError(ServerHelloError),
    Response(ResponseEnvelope),
    Event(EventEnvelope),
}

impl ServerMessage {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Hello(_) => "hello",
            Self::HelloError(_) => "hello_error",
            Self::Response(_) => "response",
            Self::Event(_) => "event",
        }
    }
}

fn validate_session_id(session_id: &str) -> Result<(), SchemaValueError> {
    if session_id.is_empty() {
        return Err(SchemaValueError::EmptySessionId);
    }
    Ok(())
}
