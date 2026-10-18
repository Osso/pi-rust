use super::{ImageContent, JsonValue, ModelRef, SchemaValueError, TextContent, ThinkingContent, Usage};

/// A tool invocation embedded in assistant content.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolCallContent {
    tool_call_id: String,
    tool_name: String,
    input: JsonValue,
}

impl ToolCallContent {
    pub fn new(tool_call_id: String, tool_name: String, input: JsonValue) -> Result<Self, SchemaValueError> {
        validate_non_empty(&tool_call_id, SchemaValueError::EmptyToolCallId)?;
        validate_non_empty(&tool_name, SchemaValueError::EmptyToolName)?;
        Ok(Self {
            tool_call_id,
            tool_name,
            input,
        })
    }

    pub fn tool_call_id(&self) -> &str {
        &self.tool_call_id
    }

    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    pub const fn input(&self) -> &JsonValue {
        &self.input
    }
}

/// Content accepted in user transcript items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserContent {
    Text(TextContent),
    Image(ImageContent),
}

impl UserContent {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Image(_) => "image",
        }
    }
}

/// Content accepted in assistant transcript items.
#[derive(Debug, Clone, PartialEq)]
pub enum AssistantContent {
    Text(TextContent),
    Thinking(ThinkingContent),
    ToolCall(ToolCallContent),
}

impl AssistantContent {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Thinking(_) => "thinking",
            Self::ToolCall(_) => "toolCall",
        }
    }
}

/// Content accepted in tool transcript items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolContent {
    Text(TextContent),
    Image(ImageContent),
}

impl ToolContent {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Image(_) => "image",
        }
    }
}

/// A user message in a session transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserTranscriptItem {
    id: String,
    content: Vec<UserContent>,
    timestamp: u64,
}

impl UserTranscriptItem {
    pub fn new(id: String, content: Vec<UserContent>, timestamp: u64) -> Result<Self, SchemaValueError> {
        validate_non_empty(&id, SchemaValueError::EmptyId)?;
        Ok(Self { id, content, timestamp })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn content(&self) -> &[UserContent] {
        &self.content
    }

    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }
}

/// Stop reasons accepted by complete assistant items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssistantStopReason {
    Stop,
    Length,
    ToolUse,
}

impl AssistantStopReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Length => "length",
            Self::ToolUse => "toolUse",
        }
    }
}

/// Status-specific assistant transcript fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssistantTranscriptState {
    Streaming,
    Complete { stop_reason: AssistantStopReason },
    Error { error_message: Option<String> },
    Aborted { error_message: Option<String> },
}

impl AssistantTranscriptState {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Streaming => "streaming",
            Self::Complete { .. } => "complete",
            Self::Error { .. } => "error",
            Self::Aborted { .. } => "aborted",
        }
    }

    pub const fn stop_reason(&self) -> Option<&'static str> {
        match self {
            Self::Streaming => None,
            Self::Complete { stop_reason } => Some(stop_reason.as_str()),
            Self::Error { .. } => Some("error"),
            Self::Aborted { .. } => Some("aborted"),
        }
    }

    pub fn error_message(&self) -> Option<&str> {
        match self {
            Self::Error { error_message } | Self::Aborted { error_message } => error_message.as_deref(),
            Self::Streaming | Self::Complete { .. } => None,
        }
    }

    pub const fn is_terminal(&self) -> bool {
        !matches!(self, Self::Streaming)
    }
}

/// Values used to construct an assistant transcript item.
#[derive(Debug, Clone, PartialEq)]
pub struct AssistantTranscriptItemInput {
    pub id: String,
    pub content: Vec<AssistantContent>,
    pub model: ModelRef,
    pub response_model: Option<String>,
    pub usage: Option<Usage>,
    pub timestamp: u64,
    pub state: AssistantTranscriptState,
}

/// An assistant message in a session transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct AssistantTranscriptItem {
    id: String,
    content: Vec<AssistantContent>,
    model: ModelRef,
    response_model: Option<String>,
    usage: Option<Usage>,
    timestamp: u64,
    state: AssistantTranscriptState,
}

impl AssistantTranscriptItem {
    pub fn new(input: AssistantTranscriptItemInput) -> Result<Self, SchemaValueError> {
        validate_non_empty(&input.id, SchemaValueError::EmptyId)?;
        if let Some(response_model) = &input.response_model {
            validate_non_empty(response_model, SchemaValueError::EmptyResponseModel)?;
        }
        if let AssistantTranscriptState::Error {
            error_message: Some(error_message),
        } = &input.state
        {
            validate_non_empty(error_message, SchemaValueError::EmptyErrorMessage)?;
        }
        Ok(Self {
            id: input.id,
            content: input.content,
            model: input.model,
            response_model: input.response_model,
            usage: input.usage,
            timestamp: input.timestamp,
            state: input.state,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn content(&self) -> &[AssistantContent] {
        &self.content
    }

    pub const fn model(&self) -> &ModelRef {
        &self.model
    }

    pub fn response_model(&self) -> Option<&str> {
        self.response_model.as_deref()
    }

    pub const fn usage(&self) -> Option<Usage> {
        self.usage
    }

    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    pub const fn state(&self) -> &AssistantTranscriptState {
        &self.state
    }
}

/// Status-specific tool transcript fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolTranscriptState {
    Running,
    Complete,
    Error,
}

impl ToolTranscriptState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Complete => "complete",
            Self::Error => "error",
        }
    }

    pub const fn is_error(self) -> bool {
        matches!(self, Self::Error)
    }

    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Running)
    }
}

/// Values used to construct a tool transcript item.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolTranscriptItemInput {
    pub id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub input: JsonValue,
    pub content: Vec<ToolContent>,
    pub details: Option<JsonValue>,
    pub usage: Option<Usage>,
    pub timestamp: u64,
    pub state: ToolTranscriptState,
}

/// A tool result in a session transcript.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolTranscriptItem {
    id: String,
    tool_call_id: String,
    tool_name: String,
    input: JsonValue,
    content: Vec<ToolContent>,
    details: Option<JsonValue>,
    usage: Option<Usage>,
    timestamp: u64,
    state: ToolTranscriptState,
}

impl ToolTranscriptItem {
    pub fn new(input: ToolTranscriptItemInput) -> Result<Self, SchemaValueError> {
        validate_non_empty(&input.id, SchemaValueError::EmptyId)?;
        validate_non_empty(&input.tool_call_id, SchemaValueError::EmptyToolCallId)?;
        validate_non_empty(&input.tool_name, SchemaValueError::EmptyToolName)?;
        Ok(Self {
            id: input.id,
            tool_call_id: input.tool_call_id,
            tool_name: input.tool_name,
            input: input.input,
            content: input.content,
            details: input.details,
            usage: input.usage,
            timestamp: input.timestamp,
            state: input.state,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn tool_call_id(&self) -> &str {
        &self.tool_call_id
    }

    pub fn tool_name(&self) -> &str {
        &self.tool_name
    }

    pub const fn input(&self) -> &JsonValue {
        &self.input
    }

    pub fn content(&self) -> &[ToolContent] {
        &self.content
    }

    pub const fn details(&self) -> Option<&JsonValue> {
        self.details.as_ref()
    }

    pub const fn usage(&self) -> Option<Usage> {
        self.usage
    }

    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    pub const fn state(&self) -> ToolTranscriptState {
        self.state
    }

    pub const fn is_error(&self) -> bool {
        self.state.is_error()
    }
}

/// One durable transcript entry.
#[derive(Debug, Clone, PartialEq)]
pub enum TranscriptItem {
    User(UserTranscriptItem),
    Assistant(AssistantTranscriptItem),
    Tool(ToolTranscriptItem),
}

impl TranscriptItem {
    pub const fn role(&self) -> &'static str {
        match self {
            Self::User(_) => "user",
            Self::Assistant(_) => "assistant",
            Self::Tool(_) => "tool",
        }
    }
}

/// Kinds reported by assistant content deltas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TranscriptDeltaKind {
    Text,
    Thinking,
    ToolCall,
}

impl TranscriptDeltaKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Thinking => "thinking",
            Self::ToolCall => "toolCall",
        }
    }
}

/// Items accepted by the `item_updated` progress event.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdatedTranscriptItem {
    Assistant(AssistantTranscriptItem),
    Tool(ToolTranscriptItem),
}

/// Items accepted by the `item_finished` progress event.
#[derive(Debug, Clone, PartialEq)]
pub enum FinishedTranscriptItem {
    Assistant(AssistantTranscriptItem),
    Tool(ToolTranscriptItem),
}

impl FinishedTranscriptItem {
    /// Creates a finished item, rejecting nonterminal assistant or tool states.
    pub fn new(item: TranscriptItem) -> Result<Self, SchemaValueError> {
        match item {
            TranscriptItem::Assistant(item) if item.state().is_terminal() => Ok(Self::Assistant(item)),
            TranscriptItem::Tool(item) if item.state().is_terminal() => Ok(Self::Tool(item)),
            TranscriptItem::User(_) | TranscriptItem::Assistant(_) | TranscriptItem::Tool(_) => {
                Err(SchemaValueError::InvalidFinishedItem)
            }
        }
    }
}

/// Normalized incremental activity. Snapshots remain authoritative.
#[derive(Debug, Clone, PartialEq)]
pub enum TranscriptProgress {
    ItemStarted {
        item: TranscriptItem,
    },
    AssistantDelta {
        message_id: String,
        content_index: u64,
        kind: TranscriptDeltaKind,
        delta: String,
    },
    ItemUpdated {
        item: UpdatedTranscriptItem,
    },
    ItemFinished {
        item: FinishedTranscriptItem,
    },
}

impl TranscriptProgress {
    pub const fn item_started(item: TranscriptItem) -> Self {
        Self::ItemStarted { item }
    }

    pub fn assistant_delta(
        message_id: String,
        content_index: u64,
        kind: TranscriptDeltaKind,
        delta: String,
    ) -> Result<Self, SchemaValueError> {
        validate_non_empty(&message_id, SchemaValueError::EmptyId)?;
        Ok(Self::AssistantDelta {
            message_id,
            content_index,
            kind,
            delta,
        })
    }

    pub fn item_updated(item: TranscriptItem) -> Result<Self, SchemaValueError> {
        match item {
            TranscriptItem::Assistant(item) => Ok(Self::ItemUpdated {
                item: UpdatedTranscriptItem::Assistant(item),
            }),
            TranscriptItem::Tool(item) => Ok(Self::ItemUpdated {
                item: UpdatedTranscriptItem::Tool(item),
            }),
            TranscriptItem::User(_) => Err(SchemaValueError::InvalidUpdatedItem),
        }
    }

    pub fn item_finished(item: TranscriptItem) -> Result<Self, SchemaValueError> {
        Ok(Self::ItemFinished {
            item: FinishedTranscriptItem::new(item)?,
        })
    }

    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::ItemStarted { .. } => "item_started",
            Self::AssistantDelta { .. } => "assistant_delta",
            Self::ItemUpdated { .. } => "item_updated",
            Self::ItemFinished { .. } => "item_finished",
        }
    }
}

fn validate_non_empty(value: &str, error: SchemaValueError) -> Result<(), SchemaValueError> {
    if value.is_empty() {
        return Err(error);
    }
    Ok(())
}
