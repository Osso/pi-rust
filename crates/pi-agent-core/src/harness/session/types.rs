//! Port of `packages/agent/src/harness/session/types.ts`.
//!
//! Entry and record bodies are internally tagged enums flattened into the
//! storage record, which reproduces the TypeScript key order: the discriminant
//! first, the variant fields next, and storage-assigned fields last.

use std::error::Error;
use std::fmt::{self, Display, Formatter};

use indexmap::IndexMap;

use pi_ai::request::BoxFuture;
use pi_ai::types::Usage;
use pi_js_compat::js_json::{JsObject, JsString, JsValue as Value};
use pi_js_compat::number as js_number;
use serde::{Deserialize, Serialize};

use crate::agent::AgentMessage;

/// JSON value accepted by durable payloads.
pub type JsonValue = Value;

/// `Exclude<StopReason, "pending"> | "deferred"`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionStopReason {
    Stop,
    Length,
    ToolUse,
    Error,
    Aborted,
    Deferred,
}

/// Entry discriminants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryType {
    Message,
    ModelChange,
    ThinkingLevelChange,
    ActiveToolsChange,
    Compaction,
    BranchSummary,
    Custom,
}

impl EntryType {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::ModelChange => "model_change",
            Self::ThinkingLevelChange => "thinking_level_change",
            Self::ActiveToolsChange => "active_tools_change",
            Self::Compaction => "compaction",
            Self::BranchSummary => "branch_summary",
            Self::Custom => "custom",
        }
    }
}

/// Record discriminants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordType {
    OperationStarted,
    AbortRequested,
    OperationFinished,
    StepAttempt,
    ToolStarted,
    QueueEnqueued,
    QueueCancelled,
    WriteDeferred,
    Usage,
}

impl RecordType {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OperationStarted => "operation_started",
            Self::AbortRequested => "abort_requested",
            Self::OperationFinished => "operation_finished",
            Self::StepAttempt => "step_attempt",
            Self::ToolStarted => "tool_started",
            Self::QueueEnqueued => "queue_enqueued",
            Self::QueueCancelled => "queue_cancelled",
            Self::WriteDeferred => "write_deferred",
            Self::Usage => "usage",
        }
    }
}

/// Distinguishes an absent optional JSON value from an explicit `null`.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum OptionalValue {
    #[default]
    Missing,
    Present(JsonValue),
}

impl OptionalValue {
    pub fn from_option(value: Option<JsonValue>) -> Self {
        match value {
            Some(value) => Self::Present(value),
            None => Self::Missing,
        }
    }

    pub fn as_value(&self) -> Option<&JsonValue> {
        match self {
            Self::Missing => None,
            Self::Present(value) => Some(value),
        }
    }

    pub fn into_value(self) -> Option<JsonValue> {
        match self {
            Self::Missing => None,
            Self::Present(value) => Some(value),
        }
    }

    pub const fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
}

impl Serialize for OptionalValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Missing => serializer.serialize_none(),
            Self::Present(value) => value.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for OptionalValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::Present(Value::deserialize(deserializer)?))
    }
}

impl From<Option<JsonValue>> for OptionalValue {
    fn from(value: Option<JsonValue>) -> Self {
        Self::from_option(value)
    }
}

impl From<JsonValue> for OptionalValue {
    fn from(value: JsonValue) -> Self {
        Self::Present(value)
    }
}

/// `ProvisionedEntry`: an entry before storage assigns `parentId`, `seq`, and
/// `timestamp`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum EntryBody {
    Message {
        id: String,
        message: AgentMessage,
        #[serde(skip_serializing_if = "Option::is_none")]
        terminate: Option<bool>,
    },
    ModelChange {
        id: String,
        provider: String,
        model_id: String,
    },
    ThinkingLevelChange {
        id: String,
        thinking_level: String,
    },
    ActiveToolsChange {
        id: String,
        active_tool_names: Vec<String>,
    },
    Compaction {
        id: String,
        summary: JsString,
        #[serde(serialize_with = "js_number::serialize")]
        tokens_before: f64,
        retained_tail: Vec<AgentMessage>,
        #[serde(
            skip_serializing_if = "Option::is_none",
            serialize_with = "js_number::serialize_option"
        )]
        duration_ms: Option<f64>,
        #[serde(default, skip_serializing_if = "OptionalValue::is_missing")]
        details: OptionalValue,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
    },
    BranchSummary {
        id: String,
        from_id: String,
        summary: JsString,
        #[serde(default, skip_serializing_if = "OptionalValue::is_missing")]
        details: OptionalValue,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Usage>,
    },
    Custom {
        id: String,
        custom_type: String,
        #[serde(default, skip_serializing_if = "OptionalValue::is_missing")]
        data: OptionalValue,
    },
}

impl EntryBody {
    pub const fn entry_type(&self) -> EntryType {
        match self {
            Self::Message { .. } => EntryType::Message,
            Self::ModelChange { .. } => EntryType::ModelChange,
            Self::ThinkingLevelChange { .. } => EntryType::ThinkingLevelChange,
            Self::ActiveToolsChange { .. } => EntryType::ActiveToolsChange,
            Self::Compaction { .. } => EntryType::Compaction,
            Self::BranchSummary { .. } => EntryType::BranchSummary,
            Self::Custom { .. } => EntryType::Custom,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::Message { id, .. }
            | Self::ModelChange { id, .. }
            | Self::ThinkingLevelChange { id, .. }
            | Self::ActiveToolsChange { id, .. }
            | Self::Compaction { id, .. }
            | Self::BranchSummary { id, .. }
            | Self::Custom { id, .. } => id,
        }
    }

    pub fn custom_type(&self) -> Option<&str> {
        match self {
            Self::Custom { custom_type, .. } => Some(custom_type),
            _ => None,
        }
    }

    pub fn message(&self) -> Option<&AgentMessage> {
        match self {
            Self::Message { message, .. } => Some(message),
            _ => None,
        }
    }

    pub fn usage(&self) -> Option<&Usage> {
        match self {
            Self::Compaction { usage, .. } | Self::BranchSummary { usage, .. } => usage.as_ref(),
            _ => None,
        }
    }
}

/// One stored entry with storage-assigned fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    #[serde(flatten)]
    pub body: EntryBody,
    #[serde(rename = "parentId")]
    pub parent_id: Option<String>,
    pub seq: i64,
    pub timestamp: i64,
}

impl Entry {
    pub const fn entry_type(&self) -> EntryType {
        self.body.entry_type()
    }

    pub fn id(&self) -> &str {
        self.body.id()
    }
}

/// Operation intent kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Run,
    Compaction,
    Navigation,
}

impl OperationKind {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Run => "run",
            Self::Compaction => "compaction",
            Self::Navigation => "navigation",
        }
    }
}

/// Operation intent payload, tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum OperationIntent {
    Run {
        original_prompt: Vec<AgentMessage>,
        initial_messages: Vec<EntryBody>,
        #[serde(skip_serializing_if = "Option::is_none")]
        system_prompt_override: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        resume_data: Option<IndexMap<String, JsonValue>>,
    },
    Compaction {
        #[serde(skip_serializing_if = "Option::is_none")]
        custom_instructions: Option<String>,
        result_entry_id: String,
    },
    Navigation {
        target_id: Option<String>,
        summarize: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        custom_instructions: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        summary_entry_id: Option<String>,
    },
}

impl OperationIntent {
    pub const fn kind(&self) -> OperationKind {
        match self {
            Self::Run { .. } => OperationKind::Run,
            Self::Compaction { .. } => OperationKind::Compaction,
            Self::Navigation { .. } => OperationKind::Navigation,
        }
    }
}

/// Operation outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OperationOutcome {
    Completed,
    Aborted,
    Failed,
    Declined,
}

/// Persisted operation failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationError {
    pub code: String,
    pub message: JsString,
}

/// Why compaction summary generation started.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompactionReason {
    Manual,
    Threshold,
    Overflow,
}

/// Step kind for a step attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepKind {
    Assistant,
    BranchSummary,
    Compaction,
}

/// Tool replay safety.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReplaySafety {
    Never,
    Safe,
}

/// Queue identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum QueueKind {
    Steer,
    FollowUp,
    NextRun,
}

/// Usage cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageCause {
    Assistant,
    Compaction,
    BranchSummary,
    DeferredFetch,
    Tool,
    Hook,
    Adjustment,
}

/// `NewRecord`: a record before storage assigns `seq` and `timestamp`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum RecordBody {
    OperationStarted {
        id: String,
        lane: String,
        source_leaf_id: Option<String>,
        intent: OperationIntent,
    },
    AbortRequested {
        id: String,
        lane: String,
        run_id: String,
    },
    OperationFinished {
        id: String,
        lane: String,
        run_id: String,
        outcome: OperationOutcome,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<OperationError>,
    },
    StepAttempt {
        id: String,
        lane: String,
        run_id: String,
        step: StepKind,
        #[serde(serialize_with = "js_number::serialize")]
        attempt: f64,
        result_entry_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        compaction_reason: Option<CompactionReason>,
    },
    ToolStarted {
        id: String,
        lane: String,
        run_id: String,
        assistant_entry_id: String,
        #[serde(serialize_with = "js_number::serialize")]
        tool_index: f64,
        tool_call_id: String,
        tool_name: String,
        #[serde(
            serialize_with = "pi_js_compat::js_json::serialize_object",
            deserialize_with = "pi_js_compat::js_json::deserialize_object"
        )]
        effective_args: JsObject,
        result_entry_id: String,
        replay: ReplaySafety,
    },
    QueueEnqueued {
        id: String,
        lane: String,
        queue: QueueKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        run_id: Option<String>,
        target: EntryBody,
    },
    QueueCancelled {
        id: String,
        lane: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        run_id: Option<String>,
        entry_id: String,
    },
    WriteDeferred {
        id: String,
        lane: String,
        run_id: String,
        target: EntryBody,
    },
    Usage {
        id: String,
        lane: String,
        cause: UsageCause,
        #[serde(skip_serializing_if = "Option::is_none")]
        run_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        entry_id: Option<String>,
        #[serde(
            skip_serializing_if = "Option::is_none",
            serialize_with = "js_number::serialize_option"
        )]
        attempt: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stop_reason: Option<SessionStopReason>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_call_id: Option<String>,
        #[serde(default, skip_serializing_if = "OptionalValue::is_missing")]
        details: OptionalValue,
        usage: Usage,
    },
}

impl RecordBody {
    pub const fn record_type(&self) -> RecordType {
        match self {
            Self::OperationStarted { .. } => RecordType::OperationStarted,
            Self::AbortRequested { .. } => RecordType::AbortRequested,
            Self::OperationFinished { .. } => RecordType::OperationFinished,
            Self::StepAttempt { .. } => RecordType::StepAttempt,
            Self::ToolStarted { .. } => RecordType::ToolStarted,
            Self::QueueEnqueued { .. } => RecordType::QueueEnqueued,
            Self::QueueCancelled { .. } => RecordType::QueueCancelled,
            Self::WriteDeferred { .. } => RecordType::WriteDeferred,
            Self::Usage { .. } => RecordType::Usage,
        }
    }

    pub fn id(&self) -> &str {
        match self {
            Self::OperationStarted { id, .. }
            | Self::AbortRequested { id, .. }
            | Self::OperationFinished { id, .. }
            | Self::StepAttempt { id, .. }
            | Self::ToolStarted { id, .. }
            | Self::QueueEnqueued { id, .. }
            | Self::QueueCancelled { id, .. }
            | Self::WriteDeferred { id, .. }
            | Self::Usage { id, .. } => id,
        }
    }

    pub fn lane(&self) -> &str {
        match self {
            Self::OperationStarted { lane, .. }
            | Self::AbortRequested { lane, .. }
            | Self::OperationFinished { lane, .. }
            | Self::StepAttempt { lane, .. }
            | Self::ToolStarted { lane, .. }
            | Self::QueueEnqueued { lane, .. }
            | Self::QueueCancelled { lane, .. }
            | Self::WriteDeferred { lane, .. }
            | Self::Usage { lane, .. } => lane,
        }
    }

    /// Operation identity used by `RecordQuery::run_id`.
    pub fn run_id(&self) -> Option<&str> {
        match self {
            Self::OperationStarted { id, .. } => Some(id),
            Self::AbortRequested { run_id, .. }
            | Self::OperationFinished { run_id, .. }
            | Self::StepAttempt { run_id, .. }
            | Self::ToolStarted { run_id, .. }
            | Self::WriteDeferred { run_id, .. } => Some(run_id),
            Self::QueueEnqueued { run_id, .. } | Self::QueueCancelled { run_id, .. } | Self::Usage { run_id, .. } => {
                run_id.as_deref()
            }
        }
    }

    pub fn usage(&self) -> Option<&Usage> {
        match self {
            Self::Usage { usage, .. } => Some(usage),
            _ => None,
        }
    }
}

/// One stored lane record with storage-assigned fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LaneRecord {
    #[serde(flatten)]
    pub body: RecordBody,
    pub seq: i64,
    pub timestamp: i64,
}

impl LaneRecord {
    pub const fn record_type(&self) -> RecordType {
        self.body.record_type()
    }

    pub fn id(&self) -> &str {
        self.body.id()
    }

    pub fn lane(&self) -> &str {
        self.body.lane()
    }
}

/// Storage mutation applied by replay and by live writes.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum SessionMutation {
    Entry {
        #[serde(skip_serializing_if = "Option::is_none")]
        lane: Option<String>,
        #[serde(flatten)]
        entry: Entry,
    },
    Record {
        #[serde(flatten)]
        record: LaneRecord,
    },
    Lane {
        seq: i64,
        lane: String,
        leaf_id: Option<String>,
    },
    Fact {
        seq: i64,
        #[serde(flatten)]
        fact: FactItem,
    },
}

impl SessionMutation {
    pub const fn seq(&self) -> i64 {
        match self {
            Self::Entry { entry, .. } => entry.seq,
            Self::Record { record } => record.seq,
            Self::Lane { seq, .. } | Self::Fact { seq, .. } => *seq,
        }
    }
}

/// Global fact mutation payload.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "fact", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum FactItem {
    Name {
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
    },
    Label {
        target_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
}

/// Query ordering.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryOrder {
    NewestFirst,
    OldestFirst,
}

/// Exclusive chronological cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntryCursor {
    pub after_seq: i64,
}

/// Entry query filters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EntryQuery {
    pub entry_type: Option<EntryType>,
    pub custom_type: Option<String>,
    pub order: Option<EntryOrder>,
    pub limit: Option<usize>,
    pub cursor: Option<EntryCursor>,
}

/// Bounds of a branch scan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchBounds {
    pub start: Option<String>,
    pub stop_at_type: Option<EntryType>,
    pub stop_at_id: Option<String>,
}

/// Branch query combining entry filters and bounds.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BranchQuery {
    pub entry: EntryQuery,
    pub bounds: BranchBounds,
}

/// Branch query whose start is mandatory at the storage boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageBranchQuery {
    pub entry: EntryQuery,
    pub start: String,
    pub stop_at_type: Option<EntryType>,
    pub stop_at_id: Option<String>,
}

/// Record query filters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecordQuery {
    pub lane: Option<String>,
    pub record_type: Option<RecordType>,
    pub run_id: Option<String>,
    pub operation_kind: Option<OperationKind>,
    pub after_seq: Option<i64>,
    pub order: Option<EntryOrder>,
    pub limit: Option<usize>,
}

/// Log query options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogOptions {
    pub after_seq: Option<i64>,
    pub limit: Option<usize>,
}

/// Base session metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMetadata {
    pub id: String,
    pub created_at: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
}

/// Metadata value carried by a generic session.
pub trait SessionMetadataValue: Clone + Send + Sync + 'static {
    fn id(&self) -> &str;
}

impl SessionMetadataValue for SessionMetadata {
    fn id(&self) -> &str {
        &self.id
    }
}

/// Aggregate ledger statistics.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    pub message_count: u64,
    #[serde(serialize_with = "js_number::serialize")]
    pub cached_tokens: f64,
    #[serde(serialize_with = "js_number::serialize")]
    pub uncached_tokens: f64,
    #[serde(serialize_with = "js_number::serialize")]
    pub total_tokens: f64,
    #[serde(serialize_with = "js_number::serialize")]
    pub cost_total: f64,
}

/// Lane name and current leaf.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanePointer {
    pub lane: String,
    pub leaf_id: Option<String>,
}

/// One ordered log item.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum LogItem {
    Entry {
        seq: i64,
        entry: Entry,
    },
    Record {
        seq: i64,
        record: LaneRecord,
    },
    Lane {
        seq: i64,
        lane: String,
        leaf_id: Option<String>,
    },
    Fact {
        seq: i64,
        #[serde(flatten)]
        fact: FactItem,
    },
}

impl LogItem {
    pub const fn seq(&self) -> i64 {
        match self {
            Self::Entry { seq, .. } | Self::Record { seq, .. } | Self::Lane { seq, .. } | Self::Fact { seq, .. } => {
                *seq
            }
        }
    }
}

/// Session error codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionErrorCode {
    NotFound,
    AlreadyExists,
    InvalidEntry,
    InvalidPayload,
    InvalidLane,
    InvalidQuery,
    InvalidForkTarget,
    Storage,
}

impl SessionErrorCode {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotFound => "not_found",
            Self::AlreadyExists => "already_exists",
            Self::InvalidEntry => "invalid_entry",
            Self::InvalidPayload => "invalid_payload",
            Self::InvalidLane => "invalid_lane",
            Self::InvalidQuery => "invalid_query",
            Self::InvalidForkTarget => "invalid_fork_target",
            Self::Storage => "storage",
        }
    }
}

/// Structured cause retained by [`SessionError`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionErrorCause {
    File {
        code: crate::harness::types::FileErrorCode,
        message: JsString,
        path: Option<JsString>,
    },
    Decode {
        kind: JsonlDecodeKind,
        message: String,
    },
    Session(Box<SessionError>),
}

/// Failure surfaced by session APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionError {
    code: SessionErrorCode,
    message: JsString,
    cause: Option<SessionErrorCause>,
}

impl SessionError {
    pub fn new(code: SessionErrorCode, message: impl Into<JsString>) -> Self {
        Self {
            code,
            message: message.into(),
            cause: None,
        }
    }

    pub fn with_cause(code: SessionErrorCode, message: impl Into<JsString>, cause: SessionErrorCause) -> Self {
        Self {
            code,
            message: message.into(),
            cause: Some(cause),
        }
    }

    pub const fn code(&self) -> SessionErrorCode {
        self.code
    }

    pub fn message(&self) -> &JsString {
        &self.message
    }

    pub fn cause(&self) -> Option<&SessionErrorCause> {
        self.cause.as_ref()
    }
}

impl Display for SessionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&pi_js_compat::js_json::encode_node_utf8(&self.message))
    }
}

impl Error for SessionError {}

/// JSONL decode error kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonlDecodeKind {
    Syntax,
    Schema,
}

impl JsonlDecodeKind {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Syntax => "syntax",
            Self::Schema => "schema",
        }
    }
}

/// Failure to decode one JSONL line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonlDecodeError {
    pub kind: JsonlDecodeKind,
    pub message: String,
}

impl JsonlDecodeError {
    pub fn new(kind: JsonlDecodeKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl Display for JsonlDecodeError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for JsonlDecodeError {}

/// Identifier generator.
pub trait IdGenerator: Send + Sync {
    fn next(&self) -> String;
}

/// Default UUIDv7 generator.
#[derive(Debug, Clone, Copy, Default)]
pub struct UuidV7IdGenerator;

impl IdGenerator for UuidV7IdGenerator {
    fn next(&self) -> String {
        crate::uuidv7()
    }
}

/// Session creation options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionCreateOptions {
    pub id: Option<String>,
    pub parent_session_id: Option<String>,
}

/// Fork scope and position.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ForkOptions {
    /// Default target: the main lane leaf, position `at`.
    #[default]
    Branch,
    /// Default target: the main lane leaf, position `before`.
    BranchDefaultBefore,
    BranchAt {
        entry_id: String,
    },
    BranchBefore {
        entry_id: String,
    },
    Tree,
}

/// Storage operations shared by memory and JSONL backends.
pub trait SessionStorage<M: SessionMetadataValue>: Send + Sync {
    fn get_metadata<'a>(&'a self) -> BoxFuture<'a, Result<M, SessionError>>;
    fn get_lanes<'a>(&'a self) -> BoxFuture<'a, Result<Vec<LanePointer>, SessionError>>;
    fn create_lane<'a>(&'a self, lane: &'a str, at: Option<&'a str>) -> BoxFuture<'a, Result<(), SessionError>>;
    fn move_lane<'a>(&'a self, lane: &'a str, to: Option<&'a str>) -> BoxFuture<'a, Result<(), SessionError>>;
    fn append_entry<'a>(&'a self, entry: EntryBody, lane: &'a str) -> BoxFuture<'a, Result<Entry, SessionError>>;
    fn append_record<'a>(&'a self, record: RecordBody) -> BoxFuture<'a, Result<LaneRecord, SessionError>>;
    fn get_entry<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Option<Entry>, SessionError>>;
    fn find_entries<'a>(&'a self, query: EntryQuery) -> BoxFuture<'a, Result<Vec<Entry>, SessionError>>;
    fn find_entries_on_branch<'a>(
        &'a self,
        query: StorageBranchQuery,
    ) -> BoxFuture<'a, Result<Vec<Entry>, SessionError>>;
    fn find_records<'a>(&'a self, query: RecordQuery) -> BoxFuture<'a, Result<Vec<LaneRecord>, SessionError>>;
    fn find_open_operations<'a>(
        &'a self,
        lane: &'a str,
        limit: Option<usize>,
    ) -> BoxFuture<'a, Result<Vec<LaneRecord>, SessionError>>;
    fn get_log<'a>(&'a self, options: LogOptions) -> BoxFuture<'a, Result<Vec<LogItem>, SessionError>>;
    fn get_name<'a>(&'a self) -> BoxFuture<'a, Result<Option<String>, SessionError>>;
    fn set_name<'a>(&'a self, name: Option<&'a str>) -> BoxFuture<'a, Result<(), SessionError>>;
    fn get_label<'a>(&'a self, id: &'a str) -> BoxFuture<'a, Result<Option<String>, SessionError>>;
    fn set_label<'a>(&'a self, id: &'a str, label: Option<&'a str>) -> BoxFuture<'a, Result<(), SessionError>>;
    fn get_stats<'a>(&'a self) -> BoxFuture<'a, Result<SessionStats, SessionError>>;
}

/// Injectable Unix-millisecond clock.
pub type Clock = std::sync::Arc<dyn Fn() -> i64 + Send + Sync>;

/// Default wall-clock source.
pub fn system_clock() -> Clock {
    std::sync::Arc::new(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis() as i64)
            .unwrap_or(0)
    })
}
