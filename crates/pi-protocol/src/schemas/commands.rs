use std::error::Error;
use std::fmt::{self, Display, Formatter};

use super::{ModelRef, ThinkingLevel};

/// Lists durable sessions.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ListCommand;

impl ListCommand {
    pub const fn new() -> Self {
        Self
    }
}

/// Optional values used to create a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateCommandInput {
    pub cwd: Option<String>,
    pub name: Option<String>,
    pub model: Option<ModelRef>,
    pub thinking_level: Option<ThinkingLevel>,
}

/// Creates a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateCommand {
    cwd: Option<String>,
    name: Option<String>,
    model: Option<ModelRef>,
    thinking_level: Option<ThinkingLevel>,
}

impl CreateCommand {
    pub fn new(input: CreateCommandInput) -> Result<Self, CommandError> {
        if input.cwd.as_deref() == Some("") {
            return Err(CommandError::EmptyCwd);
        }
        Ok(Self {
            cwd: input.cwd,
            name: input.name,
            model: input.model,
            thinking_level: input.thinking_level,
        })
    }

    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub const fn model(&self) -> Option<&ModelRef> {
        self.model.as_ref()
    }

    pub const fn thinking_level(&self) -> Option<ThinkingLevel> {
        self.thinking_level
    }
}

/// Attaches a client to a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachCommand {
    session_id: String,
}

impl AttachCommand {
    pub fn new(session_id: String) -> Result<Self, CommandError> {
        Ok(Self {
            session_id: validate_session_id(session_id)?,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

/// Detaches a client from a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachCommand {
    session_id: String,
}

impl DetachCommand {
    pub fn new(session_id: String) -> Result<Self, CommandError> {
        Ok(Self {
            session_id: validate_session_id(session_id)?,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

/// Sends a prompt to a session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptCommand {
    session_id: String,
    text: String,
}

impl PromptCommand {
    pub fn new(session_id: String, text: String) -> Result<Self, CommandError> {
        Ok(Self {
            session_id: validate_session_id(session_id)?,
            text,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Steers a running session turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteerCommand {
    session_id: String,
    text: String,
}

impl SteerCommand {
    pub fn new(session_id: String, text: String) -> Result<Self, CommandError> {
        Ok(Self {
            session_id: validate_session_id(session_id)?,
            text,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Aborts a running session turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbortCommand {
    session_id: String,
}

impl AbortCommand {
    pub fn new(session_id: String) -> Result<Self, CommandError> {
        Ok(Self {
            session_id: validate_session_id(session_id)?,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }
}

/// Changes a session's model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetModelCommand {
    session_id: String,
    model: ModelRef,
}

impl SetModelCommand {
    pub fn new(session_id: String, model: ModelRef) -> Result<Self, CommandError> {
        Ok(Self {
            session_id: validate_session_id(session_id)?,
            model,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub const fn model(&self) -> &ModelRef {
        &self.model
    }
}

/// Changes a session's thinking level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetThinkingCommand {
    session_id: String,
    thinking_level: ThinkingLevel,
}

impl SetThinkingCommand {
    pub fn new(session_id: String, thinking_level: ThinkingLevel) -> Result<Self, CommandError> {
        Ok(Self {
            session_id: validate_session_id(session_id)?,
            thinking_level,
        })
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub const fn thinking_level(&self) -> ThinkingLevel {
        self.thinking_level
    }
}

/// A validated protocol command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    List(ListCommand),
    Create(CreateCommand),
    Attach(AttachCommand),
    Detach(DetachCommand),
    Prompt(PromptCommand),
    Steer(SteerCommand),
    Abort(AbortCommand),
    SetModel(SetModelCommand),
    SetThinking(SetThinkingCommand),
}

impl Command {
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

/// Identifies an invalid string in a typed protocol command.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandError {
    EmptyCwd,
    EmptySessionId,
}

impl Display for CommandError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let field = match self {
            Self::EmptyCwd => "cwd",
            Self::EmptySessionId => "sessionId",
        };
        write!(formatter, "Command field {field} must not be empty")
    }
}

impl Error for CommandError {}

fn validate_session_id(session_id: String) -> Result<String, CommandError> {
    if session_id.is_empty() {
        return Err(CommandError::EmptySessionId);
    }
    Ok(session_id)
}
