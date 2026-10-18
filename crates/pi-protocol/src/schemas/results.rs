use super::{SchemaValueError, SessionMetadata, SessionSnapshot};

/// Result of listing durable sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListResult {
    sessions: Vec<SessionMetadata>,
}

impl ListResult {
    pub const fn new(sessions: Vec<SessionMetadata>) -> Self {
        Self { sessions }
    }

    pub fn sessions(&self) -> &[SessionMetadata] {
        &self.sessions
    }
}

/// Result of detaching from a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachResult {
    session_id: String,
}

impl DetachResult {
    pub fn new(session_id: String) -> Result<Self, SchemaValueError> {
        if session_id.is_empty() {
            return Err(SchemaValueError::EmptySessionId);
        }
        Ok(Self { session_id })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

/// Result of one validated protocol command.
#[derive(Debug, Clone, PartialEq)]
pub enum CommandResult {
    List(ListResult),
    Create(SessionSnapshot),
    Attach(SessionSnapshot),
    Detach(DetachResult),
    Prompt(SessionSnapshot),
    Steer(SessionSnapshot),
    Abort(SessionSnapshot),
    SetModel(SessionSnapshot),
    SetThinking(SessionSnapshot),
}

impl CommandResult {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::List(_) => "list",
            Self::Create(_) => "create",
            Self::Attach(_) => "attach",
            Self::Detach(_) => "detach",
            Self::Prompt(_) => "prompt",
            Self::Steer(_) => "steer",
            Self::Abort(_) => "abort",
            Self::SetModel(_) => "set_model",
            Self::SetThinking(_) => "set_thinking",
        }
    }
}
