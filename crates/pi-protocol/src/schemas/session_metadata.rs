use std::error::Error;
use std::fmt::{self, Display, Formatter};

/// Values used to construct listed-session metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMetadataInput {
    pub id: String,
    pub created_at: u64,
    pub updated_at: Option<u64>,
    pub parent_session_id: Option<String>,
    pub session_name: Option<String>,
    pub cwd: Option<String>,
}

/// Durable metadata for a listed session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionMetadata {
    id: String,
    created_at: u64,
    updated_at: Option<u64>,
    parent_session_id: Option<String>,
    session_name: Option<String>,
    cwd: Option<String>,
}

impl SessionMetadata {
    /// Creates session metadata after validating its representable string constraints.
    pub fn new(input: SessionMetadataInput) -> Result<Self, SessionMetadataError> {
        validate_session_metadata_text(&input.id, SessionMetadataError::EmptyId)?;
        if let Some(parent_session_id) = &input.parent_session_id {
            validate_session_metadata_text(parent_session_id, SessionMetadataError::EmptyParentSessionId)?;
        }
        if let Some(cwd) = &input.cwd {
            validate_session_metadata_text(cwd, SessionMetadataError::EmptyCwd)?;
        }

        Ok(Self {
            id: input.id,
            created_at: input.created_at,
            updated_at: input.updated_at,
            parent_session_id: input.parent_session_id,
            session_name: input.session_name,
            cwd: input.cwd,
        })
    }

    /// Session identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Session creation timestamp.
    pub const fn created_at(&self) -> u64 {
        self.created_at
    }

    /// Last-update timestamp when present.
    pub const fn updated_at(&self) -> Option<u64> {
        self.updated_at
    }

    /// Parent session identifier when present.
    pub fn parent_session_id(&self) -> Option<&str> {
        self.parent_session_id.as_deref()
    }

    /// Session name when present. Empty names remain distinct from omission.
    pub fn session_name(&self) -> Option<&str> {
        self.session_name.as_deref()
    }

    /// Session working directory when present.
    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }
}

/// Identifies the first invalid string in typed session metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMetadataError {
    EmptyId,
    EmptyParentSessionId,
    EmptyCwd,
}

impl Display for SessionMetadataError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let field = match self {
            Self::EmptyId => "id",
            Self::EmptyParentSessionId => "parentSessionId",
            Self::EmptyCwd => "cwd",
        };
        write!(formatter, "Session metadata field {field} must not be empty")
    }
}

impl Error for SessionMetadataError {}

fn validate_session_metadata_text(value: &str, error: SessionMetadataError) -> Result<(), SessionMetadataError> {
    if value.is_empty() {
        return Err(error);
    }
    Ok(())
}
