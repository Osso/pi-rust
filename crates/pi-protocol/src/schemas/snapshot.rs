use super::{
    ModelMetadata, ModelRef, PROTOCOL_VERSION, SchemaValueError, SessionMetadata, SessionPhase, ThinkingLevel,
    TranscriptItem, UserTranscriptItem,
};

/// Values used to construct a session snapshot.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSnapshotInput {
    pub id: String,
    pub name: Option<String>,
    pub cwd: String,
    pub created_at: u64,
    pub updated_at: u64,
    pub phase: SessionPhase,
    pub model: ModelRef,
    pub thinking_level: ThinkingLevel,
    pub attached: bool,
    pub locked: bool,
    pub revision: u64,
    pub transcript: Vec<TranscriptItem>,
    pub queued_steer: Vec<UserTranscriptItem>,
    pub queued_steer_count: u64,
}

/// A complete session state published to clients.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSnapshot {
    id: String,
    name: Option<String>,
    cwd: String,
    created_at: u64,
    updated_at: u64,
    phase: SessionPhase,
    model: ModelRef,
    thinking_level: ThinkingLevel,
    attached: bool,
    locked: bool,
    revision: u64,
    transcript: Vec<TranscriptItem>,
    queued_steer: Vec<UserTranscriptItem>,
    queued_steer_count: u64,
}

impl SessionSnapshot {
    pub fn new(input: SessionSnapshotInput) -> Result<Self, SchemaValueError> {
        validate_non_empty(&input.id, SchemaValueError::EmptyId)?;
        validate_non_empty(&input.cwd, SchemaValueError::EmptyCwd)?;
        Ok(Self {
            id: input.id,
            name: input.name,
            cwd: input.cwd,
            created_at: input.created_at,
            updated_at: input.updated_at,
            phase: input.phase,
            model: input.model,
            thinking_level: input.thinking_level,
            attached: input.attached,
            locked: input.locked,
            revision: input.revision,
            transcript: input.transcript,
            queued_steer: input.queued_steer,
            queued_steer_count: input.queued_steer_count,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub fn cwd(&self) -> &str {
        &self.cwd
    }

    pub const fn created_at(&self) -> u64 {
        self.created_at
    }

    pub const fn updated_at(&self) -> u64 {
        self.updated_at
    }

    pub const fn phase(&self) -> SessionPhase {
        self.phase
    }

    pub const fn model(&self) -> &ModelRef {
        &self.model
    }

    pub const fn thinking_level(&self) -> ThinkingLevel {
        self.thinking_level
    }

    pub const fn attached(&self) -> bool {
        self.attached
    }

    pub const fn locked(&self) -> bool {
        self.locked
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn transcript(&self) -> &[TranscriptItem] {
        &self.transcript
    }

    pub fn queued_steer(&self) -> &[UserTranscriptItem] {
        &self.queued_steer
    }

    pub const fn queued_steer_count(&self) -> u64 {
        self.queued_steer_count
    }
}

/// The server-wide state published in a handshake.
#[derive(Debug, Clone, PartialEq)]
pub struct ServerSnapshot {
    server_id: String,
    revision: u64,
    sessions: Vec<SessionMetadata>,
    models: Vec<ModelMetadata>,
}

impl ServerSnapshot {
    pub fn new(
        server_id: String,
        revision: u64,
        sessions: Vec<SessionMetadata>,
        models: Vec<ModelMetadata>,
    ) -> Result<Self, SchemaValueError> {
        validate_non_empty(&server_id, SchemaValueError::EmptyServerId)?;
        Ok(Self {
            server_id,
            revision,
            sessions,
            models,
        })
    }

    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub const fn protocol_version(&self) -> u64 {
        PROTOCOL_VERSION
    }

    pub const fn revision(&self) -> u64 {
        self.revision
    }

    pub fn sessions(&self) -> &[SessionMetadata] {
        &self.sessions
    }

    pub fn models(&self) -> &[ModelMetadata] {
        &self.models
    }
}

fn validate_non_empty(value: &str, error: SchemaValueError) -> Result<(), SchemaValueError> {
    if value.is_empty() {
        return Err(error);
    }
    Ok(())
}
