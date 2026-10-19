//! Port of `packages/agent/src/harness/agent-harness.ts`.
//!
//! The pinned TypeScript source is an explicit scaffold: construction, state
//! accessors, and `close` work, while every effectful operation rejects with
//! `HarnessNotImplemented` (or `HarnessClosed` after close). The Rust port
//! preserves that observable contract.

use std::collections::BTreeMap;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use pi_ai::models::Models;
use pi_ai::request::{BoxFuture, DeferredRequest, OnRetry, ProviderRequestOptions, SimpleStreamOptions};
use pi_ai::types::{
    AssistantMessage, CacheRetention, DeferredHandle, ImageContent, JsonObject, Message, Model, ThinkingBudgets,
    ThinkingLevel as AiThinkingLevel, Transport, Usage,
};
use pi_ai::utils::retry::RetryPolicy;
use pi_telemetry::TelemetryContext;
use serde_json::Value;

use crate::agent::{AgentMessage, AgentTool, PromptInput};
use crate::harness::compaction::{CompactionSettings, DEFAULT_COMPACTION_SETTINGS};
use crate::harness::session::Session;
use crate::harness::session::types::{
    Entry, EntryBody, OperationError, OperationKind, RecordQuery, ReplaySafety, SessionError, SessionMetadata,
};
use crate::harness::types::{
    AgentHarnessResources, AgentHarnessToolContextSource, QueueMode, ThinkingLevel, ToolExecutionMode,
};

/// `Resources`: skills and prompt templates available to the harness.
pub type Resources = AgentHarnessResources;

/// `StreamOptions`: provider request options owned by the harness.
pub type StreamOptions = SimpleStreamOptions;

/// `EntryProjector`: maps a stored entry to provider messages.
pub type EntryProjector = Arc<dyn Fn(Entry) -> BoxFuture<'static, Vec<AgentMessage>> + Send + Sync>;

/// `toProviderMessages`: application-supplied provider-message conversion.
pub type ToProviderMessages = Arc<dyn Fn(Vec<AgentMessage>) -> BoxFuture<'static, Vec<Message>> + Send + Sync>;

/// Passive hook handler. The scaffold never invokes or stores handlers.
pub type HarnessHookHandler = Arc<dyn Fn(Value) -> BoxFuture<'static, Value> + Send + Sync>;

/// Passive event listener. The scaffold never invokes or stores listeners.
pub type HarnessEventListener = Arc<dyn Fn(Value) -> BoxFuture<'static, ()> + Send + Sync>;

/// Handle returned by `hooks.on` / `events.on`; invoking it unsubscribes.
pub type Unsubscribe = Box<dyn FnOnce() + Send>;

macro_rules! tagged_error {
    ($name:ident, $tag:literal, { $($field:ident: $ty:ty),* $(,)? }) => {
        #[doc = concat!("`", $tag, "` tagged error.")]
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name {
            $(pub $field: $ty,)*
        }

        impl $name {
            pub fn new($($field: $ty),*) -> Self {
                Self { $($field,)* }
            }

            /// Returns the exact TypeScript error tag.
            pub const fn tag(&self) -> &'static str {
                $tag
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                formatter.write_str(&self.message)
            }
        }

        impl Error for $name {}
    };
}

tagged_error!(LaneBusy, "LaneBusy", {
    lane: String,
    operation_id: String,
    operation_kind: OperationKind,
    message: String,
});
tagged_error!(MissingIdentities, "MissingIdentities", {
    lane: String,
    tools: Vec<String>,
    models: Vec<String>,
    message: String,
});
tagged_error!(NoActiveRun, "NoActiveRun", {
    lane: String,
    message: String,
});
tagged_error!(NoActiveOperation, "NoActiveOperation", {
    lane: String,
    message: String,
});
tagged_error!(NothingToResume, "NothingToResume", {
    lane: String,
    message: String,
});
tagged_error!(InvalidMessage, "InvalidMessage", {
    lane: String,
    reason: String,
    message: String,
});
tagged_error!(UnknownSkill, "UnknownSkill", {
    name: String,
    message: String,
});
tagged_error!(UnknownTemplate, "UnknownTemplate", {
    name: String,
    message: String,
});
tagged_error!(UnknownTarget, "UnknownTarget", {
    target_id: String,
    message: String,
});
tagged_error!(UnknownQueueItem, "UnknownQueueItem", {
    lane: String,
    entry_id: String,
    message: String,
});
tagged_error!(LaneExists, "LaneExists", {
    lane: String,
    message: String,
});
tagged_error!(InvalidLane, "InvalidLane", {
    lane: String,
    reason: String,
    message: String,
});
tagged_error!(NothingToCompact, "NothingToCompact", {
    lane: String,
    message: String,
});
tagged_error!(Closed, "Closed", {
    message: String,
});

macro_rules! rejected_union {
    ($name:ident { $($variant:ident($ty:ty)),* $(,)? }) => {
        #[doc = concat!("Rejected-result union for `", stringify!($name), "`.")]
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub enum $name {
            $($variant($ty),)*
        }

        impl $name {
            /// Returns the exact TypeScript error tag.
            pub const fn tag(&self) -> &'static str {
                match self {
                    $(Self::$variant(error) => error.tag(),)*
                }
            }
        }

        impl Display for $name {
            fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                match self {
                    $(Self::$variant(error) => Display::fmt(error, formatter),)*
                }
            }
        }

        impl Error for $name {
            fn source(&self) -> Option<&(dyn Error + 'static)> {
                match self {
                    $(Self::$variant(error) => Some(error),)*
                }
            }
        }
    };
}

rejected_union!(RunRejected {
    LaneBusy(LaneBusy),
    InvalidMessage(InvalidMessage),
    UnknownSkill(UnknownSkill),
    UnknownTemplate(UnknownTemplate),
    Closed(Closed),
});
rejected_union!(CompactionRejected {
    LaneBusy(LaneBusy),
    NothingToCompact(NothingToCompact),
    Closed(Closed),
});
rejected_union!(NavigationRejected {
    LaneBusy(LaneBusy),
    UnknownTarget(UnknownTarget),
    Closed(Closed),
});
rejected_union!(ResumeRejected {
    LaneBusy(LaneBusy),
    NothingToResume(NothingToResume),
    MissingIdentities(MissingIdentities),
    Closed(Closed),
});
rejected_union!(QueueRejected {
    NoActiveRun(NoActiveRun),
    InvalidMessage(InvalidMessage),
    Closed(Closed),
});
rejected_union!(CancelQueuedRejected {
    UnknownQueueItem(UnknownQueueItem),
    Closed(Closed),
});
rejected_union!(AbortRejected {
    NoActiveOperation(NoActiveOperation),
    Closed(Closed),
});
rejected_union!(CreateLaneRejected {
    LaneExists(LaneExists),
    InvalidLane(InvalidLane),
    UnknownTarget(UnknownTarget),
    Closed(Closed),
});

/// `HarnessFault`: unexpected harness failure with a preserved cause.
#[derive(Debug)]
pub struct HarnessFault {
    pub message: String,
    pub cause: Box<dyn Error + Send + Sync + 'static>,
}

impl HarnessFault {
    pub fn new(message: impl Into<String>, cause: Box<dyn Error + Send + Sync + 'static>) -> Self {
        Self {
            message: message.into(),
            cause,
        }
    }

    /// Returns the exact TypeScript error name.
    pub const fn name(&self) -> &'static str {
        "HarnessFault"
    }
}

impl Display for HarnessFault {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for HarnessFault {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.cause.as_ref())
    }
}

/// `HarnessClosed`: the harness was closed while an operation was active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HarnessClosed;

impl HarnessClosed {
    /// Returns the exact TypeScript error name.
    pub const fn name(&self) -> &'static str {
        "HarnessClosed"
    }
}

impl Display for HarnessClosed {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("AgentHarness was closed while the operation was active")
    }
}

impl Error for HarnessClosed {}

/// `HarnessNotImplemented`: the scaffold has not implemented this operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessNotImplemented {
    pub operation: String,
}

impl HarnessNotImplemented {
    pub fn new(operation: impl Into<String>) -> Self {
        Self {
            operation: operation.into(),
        }
    }

    /// Returns the exact TypeScript error name.
    pub const fn name(&self) -> &'static str {
        "HarnessNotImplemented"
    }
}

impl Display for HarnessNotImplemented {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "AgentHarness.{} is not implemented yet", self.operation)
    }
}

impl Error for HarnessNotImplemented {}

/// Rejected promise for every unfinished scaffold operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HarnessUnavailable {
    Closed(HarnessClosed),
    NotImplemented(HarnessNotImplemented),
}

impl HarnessUnavailable {
    /// Returns the exact TypeScript error name.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Closed(error) => error.name(),
            Self::NotImplemented(error) => error.name(),
        }
    }

    /// Returns the unavailable operation name, absent for [`HarnessClosed`].
    pub fn operation(&self) -> Option<&str> {
        match self {
            Self::Closed(_) => None,
            Self::NotImplemented(error) => Some(&error.operation),
        }
    }
}

impl Display for HarnessUnavailable {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Closed(error) => Display::fmt(error, formatter),
            Self::NotImplemented(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for HarnessUnavailable {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Closed(error) => Some(error),
            Self::NotImplemented(error) => Some(error),
        }
    }
}

/// `RunOutcome`: terminal state of one admitted run.
#[derive(Debug, Clone, PartialEq)]
pub enum RunOutcome {
    Completed {
        leaf_id: String,
        final_entry_id: String,
        final_message: AssistantMessage,
    },
    Aborted {
        leaf_id: String,
        final_entry_id: String,
        final_message: AssistantMessage,
    },
    Failed {
        leaf_id: String,
        error: OperationError,
        final_entry_id: Option<String>,
        final_message: Option<AssistantMessage>,
    },
    Suspended {
        leaf_id: String,
        final_entry_id: String,
        deferred: DeferredHandle,
    },
}

/// `CompactionOutcome`: terminal state of one manual compaction.
#[derive(Debug, Clone, PartialEq)]
pub enum CompactionOutcome {
    Completed { leaf_id: String, entry: Box<Entry> },
    Declined { leaf_id: String },
    Aborted { leaf_id: String },
    Failed { leaf_id: String, error: OperationError },
}

/// `NavigationOutcome`: terminal state of one tree navigation.
#[derive(Debug, Clone, PartialEq)]
pub enum NavigationOutcome {
    Completed {
        new_leaf_id: Option<String>,
        summary_entry: Option<Box<Entry>>,
    },
    Declined {
        leaf_id: Option<String>,
    },
    Aborted {
        leaf_id: Option<String>,
    },
    Failed {
        leaf_id: Option<String>,
        error: OperationError,
    },
}

/// `{ runId } & RunOutcome`.
#[derive(Debug, Clone, PartialEq)]
pub struct RunResultValue {
    pub run_id: String,
    pub outcome: RunOutcome,
}

/// `{ runId } & CompactionOutcome`.
#[derive(Debug, Clone, PartialEq)]
pub struct CompactionResultValue {
    pub run_id: String,
    pub outcome: CompactionOutcome,
}

/// `{ runId } & NavigationOutcome`.
#[derive(Debug, Clone, PartialEq)]
pub struct NavigationResultValue {
    pub run_id: String,
    pub outcome: NavigationOutcome,
}

/// `RunResult`.
pub type RunResult = Result<RunResultValue, RunRejected>;

/// `CompactionResult`.
pub type CompactionResult = Result<CompactionResultValue, CompactionRejected>;

/// `NavigationResult`.
pub type NavigationResult = Result<NavigationResultValue, NavigationRejected>;

/// `QueueResult`.
pub type QueueResult = Result<QueueResultValue, QueueRejected>;

/// `{ entryId }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueResultValue {
    pub entry_id: String,
}

/// `CancelQueuedResult`.
pub type CancelQueuedResult = Result<CancelQueuedValue, CancelQueuedRejected>;

/// `{ outcome: "cancelled" | "already_consumed" | "already_cleared" }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CancelQueuedValue {
    pub outcome: CancelQueuedOutcome,
}

/// Queue cancellation outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelQueuedOutcome {
    Cancelled,
    AlreadyConsumed,
    AlreadyCleared,
}

impl CancelQueuedOutcome {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::AlreadyConsumed => "already_consumed",
            Self::AlreadyCleared => "already_cleared",
        }
    }
}

/// `RecordUsageResult`.
pub type RecordUsageResult = Result<(), Closed>;

/// `AbortResult`.
pub type AbortResult = Result<AbortValue, AbortRejected>;

/// `{ runId; steer; followUp }`.
#[derive(Debug, Clone, PartialEq)]
pub struct AbortValue {
    pub run_id: String,
    pub steer: Vec<AgentMessage>,
    pub follow_up: Vec<AgentMessage>,
}

/// `ResumeOutcome`.
#[derive(Debug, Clone, PartialEq)]
pub enum ResumeOutcome {
    Run {
        run_id: String,
        outcome: Box<RunOutcome>,
    },
    Compaction {
        run_id: String,
        outcome: Box<CompactionOutcome>,
    },
    Navigation {
        run_id: String,
        outcome: Box<NavigationOutcome>,
    },
}

/// `ResumeResult`.
pub type ResumeResult = Result<ResumeOutcome, ResumeRejected>;

/// `CreateLaneResult`.
pub type CreateLaneResult = Result<Arc<dyn AgentLane>, CreateLaneRejected>;

/// `AgentLane`: nominal lane handle interface.
///
/// The pinned scaffold never constructs a lane value, so the method surface is
/// not ported; [`AgentHarness`] is the only lane implementation.
pub trait AgentLane: Send + Sync {}

impl AgentLane for AgentHarness {}

/// `NavigateOptions`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NavigateOptions {
    pub summarize: Option<bool>,
    pub custom_instructions: Option<String>,
    pub label: Option<String>,
}

/// Options accepted by [`AgentHarness::compact`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HarnessCompactOptions {
    pub custom_instructions: Option<String>,
}

/// Options accepted by [`AgentHarness::record_usage`].
#[derive(Debug, Clone, Default)]
pub struct RecordUsageOptions {
    pub entry_id: Option<String>,
    pub details: Option<Value>,
}

/// `QueueInput`: `string | AgentMessage` accepted by queue methods.
#[derive(Debug, Clone)]
pub enum QueueInput {
    Text(String),
    Message(Box<AgentMessage>),
}

impl From<&str> for QueueInput {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl From<String> for QueueInput {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<AgentMessage> for QueueInput {
    fn from(value: AgentMessage) -> Self {
        Self::Message(Box::new(value))
    }
}

/// `DriveMode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveMode {
    Automatic,
    Manual,
}

/// `systemPrompt: string | (() => string | Promise<string>)`.
pub enum SystemPromptSource {
    Static(String),
    Provider(Arc<dyn Fn() -> BoxFuture<'static, String> + Send + Sync>),
}

/// `StreamOptionsPatch`: `Partial<SimpleStreamOptions>`.
#[derive(Clone, Default)]
pub struct StreamOptionsPatch {
    pub request: Option<ProviderRequestOptions>,
    pub temperature: Option<f64>,
    pub sampling_params: Option<JsonObject>,
    pub max_tokens: Option<f64>,
    pub transport: Option<Transport>,
    pub cache_retention: Option<CacheRetention>,
    pub session_id: Option<String>,
    pub on_retry: Option<OnRetry>,
    pub websocket_connect_timeout_ms: Option<f64>,
    pub metadata: Option<JsonObject>,
    pub extra: Option<JsonObject>,
    pub reasoning: Option<AiThinkingLevel>,
    pub deferred: Option<DeferredRequest>,
    pub thinking_budgets: Option<ThinkingBudgets>,
}

/// `HarnessTool`: an agent tool plus its declared replay policy.
///
/// `prompt_snippet` and `prompt_guidelines` carry the coding-agent prompt metadata that
/// TypeScript attaches as extra properties (`CodingAgentHarnessTool`); keeping them on the
/// tool preserves them through `setTools`/`getTools`.
#[derive(Clone)]
pub struct HarnessTool {
    pub tool: Arc<dyn AgentTool>,
    pub replay: Option<ReplaySafety>,
    pub prompt_snippet: Option<String>,
    pub prompt_guidelines: Vec<String>,
}

impl HarnessTool {
    pub fn new(tool: Arc<dyn AgentTool>) -> Self {
        Self {
            tool,
            replay: None,
            prompt_snippet: None,
            prompt_guidelines: Vec::new(),
        }
    }

    pub fn with_replay(tool: Arc<dyn AgentTool>, replay: ReplaySafety) -> Self {
        Self {
            tool,
            replay: Some(replay),
            prompt_snippet: None,
            prompt_guidelines: Vec::new(),
        }
    }

    pub fn with_prompt_metadata(mut self, prompt_snippet: Option<String>, prompt_guidelines: Vec<String>) -> Self {
        self.prompt_snippet = prompt_snippet;
        self.prompt_guidelines = prompt_guidelines;
        self
    }

    pub fn name(&self) -> &str {
        self.tool.name()
    }
}

/// `HookName`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookName {
    BeforeRun,
    BeforeResume,
    BeforeRunEnd,
    TransformContext,
    BeforeRequest,
    BeforePayload,
    AfterResponse,
    BeforeTool,
    AfterTool,
    BeforeCompaction,
    BeforeNavigation,
}

impl HookName {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BeforeRun => "before_run",
            Self::BeforeResume => "before_resume",
            Self::BeforeRunEnd => "before_run_end",
            Self::TransformContext => "transform_context",
            Self::BeforeRequest => "before_request",
            Self::BeforePayload => "before_payload",
            Self::AfterResponse => "after_response",
            Self::BeforeTool => "before_tool",
            Self::AfterTool => "after_tool",
            Self::BeforeCompaction => "before_compaction",
            Self::BeforeNavigation => "before_navigation",
        }
    }
}

/// Options accepted by `hooks.on`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HookOptions {
    pub id: Option<String>,
}

#[derive(Clone)]
struct ClosedFlag(Arc<AtomicBool>);

impl ClosedFlag {
    fn new() -> Self {
        Self(Arc::new(AtomicBool::new(false)))
    }

    fn is_closed(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    fn close(&self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

fn unavailable(closed: bool, operation: &str) -> HarnessUnavailable {
    if closed {
        HarnessUnavailable::Closed(HarnessClosed)
    } else {
        HarnessUnavailable::NotImplemented(HarnessNotImplemented::new(operation))
    }
}

/// `Hooks`: hook registry. The scaffold rejects every registration.
#[derive(Clone)]
pub struct Hooks {
    closed: ClosedFlag,
}

impl Hooks {
    /// Registers a hook handler. Always rejects in the scaffold.
    pub fn on(
        &self,
        _name: HookName,
        _handler: HarnessHookHandler,
        _options: Option<HookOptions>,
    ) -> Result<Unsubscribe, HarnessUnavailable> {
        Err(unavailable(self.closed.is_closed(), "hooks.on"))
    }
}

/// `Events`: event registry. The scaffold rejects every subscription.
#[derive(Clone)]
pub struct Events {
    closed: ClosedFlag,
}

impl Events {
    /// Registers a passive event listener. Always rejects in the scaffold.
    pub fn on(
        &self,
        _event_type: &str,
        _listener: HarnessEventListener,
        _options: Option<HookOptions>,
    ) -> Result<Unsubscribe, HarnessUnavailable> {
        Err(unavailable(self.closed.is_closed(), "events.on"))
    }
}

/// `SuspendedOperation`.
#[derive(Debug, Clone, PartialEq)]
pub struct SuspendedOperation {
    pub lane: String,
    pub kind: OperationKind,
    pub id: String,
    pub started_at: f64,
    pub reason: SuspensionReason,
    pub prompt: Option<Vec<AgentMessage>>,
    pub deferred: Option<DeferredHandle>,
    pub aborting: Option<AbortQueues>,
    pub missing: MissingCapabilities,
}

/// Why a durable operation is suspended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspensionReason {
    Crash,
    Deferred,
}

impl SuspensionReason {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Crash => "crash",
            Self::Deferred => "deferred",
        }
    }
}

/// Queued messages preserved while an operation aborts.
#[derive(Debug, Clone, PartialEq)]
pub struct AbortQueues {
    pub steer: Vec<AgentMessage>,
    pub follow_up: Vec<AgentMessage>,
}

/// Capabilities missing for a suspended operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MissingCapabilities {
    pub tools: Vec<String>,
    pub models: Vec<String>,
}

/// `LaneInfo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneInfo {
    pub name: String,
    pub leaf_id: Option<String>,
    pub operation: Option<LaneOperation>,
}

/// Operation state attached to a lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneOperation {
    pub id: String,
    pub kind: OperationKind,
    pub status: LaneOperationStatus,
}

/// Lane operation status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaneOperationStatus {
    Running,
    Suspended,
    Aborting,
}

impl LaneOperationStatus {
    /// Returns the exact TypeScript source literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Suspended => "suspended",
            Self::Aborting => "aborting",
        }
    }
}

/// `QueuedItem`.
#[derive(Debug, Clone, PartialEq)]
pub struct QueuedItem {
    pub entry_id: String,
    pub message: AgentMessage,
}

/// `LaneSnapshot.queues`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct LaneQueues {
    pub steer: Vec<QueuedItem>,
    pub follow_up: Vec<QueuedItem>,
    pub next_run: Vec<QueuedItem>,
}

/// `LaneSnapshot.pendingWrites` item.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingWrite {
    pub id: String,
    pub entry: EntryBody,
}

/// `LaneSnapshot`.
#[derive(Debug, Clone, PartialEq)]
pub struct LaneSnapshot {
    pub lane: String,
    pub transcript: Vec<Entry>,
    pub leaf_id: Option<String>,
    pub operation: Option<LaneOperation>,
    pub queues: LaneQueues,
    pub pending_writes: Vec<PendingWrite>,
    pub faulted: bool,
}

/// `SessionSnapshot.lanes` item: `LaneInfo & { suspended?: SuspendedOperation }`.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionLaneInfo {
    pub info: LaneInfo,
    pub suspended: Option<SuspendedOperation>,
}

/// `SessionSnapshot`.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionSnapshot {
    pub lanes: Vec<SessionLaneInfo>,
    pub faulted: bool,
}

/// `ActionInfo`.
#[derive(Debug, Clone, PartialEq)]
pub enum ActionInfo {
    AppendEntry {
        entry_type: crate::harness::session::types::EntryType,
        entry_id: String,
    },
    AppendRecord {
        record_type: String,
    },
    MoveLane {
        to: Option<String>,
    },
    SetFact {
        fact: FactName,
    },
    TryFinishRun {
        outcome: TryFinishRunOutcome,
    },
    FinishOperation {
        outcome: FinishOperationOutcome,
    },
    CommitFollowUp,
    ConsumeQueueItem {
        queue: QueueName,
        entry_id: String,
    },
    ApplyPendingWrite {
        entry_id: String,
    },
    StreamAssistant {
        step: AssistantStep,
        attempt: u32,
    },
    ExecuteTool {
        tool_call_id: String,
        tool_name: String,
    },
    FetchDeferred {
        provider: String,
        id: String,
    },
    CancelDeferred {
        provider: String,
        id: String,
    },
    Hook {
        name: HookName,
    },
    Sleep {
        delay_ms: f64,
    },
}

/// `ActionInfo` fact name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactName {
    Name,
    Label,
}

/// `ActionInfo` run outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TryFinishRunOutcome {
    Completed,
    Failed,
}

/// `ActionInfo` operation outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishOperationOutcome {
    Completed,
    Declined,
    Failed,
    Aborted,
}

/// `ActionInfo` queue name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueueName {
    Steer,
    FollowUp,
}

/// `ActionInfo` assistant stream step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssistantStep {
    Assistant,
    Compaction,
    BranchSummary,
}

/// `WatchHandle<TSnapshot>`.
pub struct WatchHandle<TSnapshot> {
    pub snapshot: TSnapshot,
    start: Arc<dyn Fn(HarnessEventListener) + Send + Sync>,
    unsubscribe: Arc<dyn Fn() + Send + Sync>,
}

impl<TSnapshot> WatchHandle<TSnapshot> {
    pub fn new(
        snapshot: TSnapshot,
        start: Arc<dyn Fn(HarnessEventListener) + Send + Sync>,
        unsubscribe: Arc<dyn Fn() + Send + Sync>,
    ) -> Self {
        Self {
            snapshot,
            start,
            unsubscribe,
        }
    }

    pub fn start(&self, listener: HarnessEventListener) {
        (self.start)(listener);
    }

    pub fn unsubscribe(&self) {
        (self.unsubscribe)();
    }
}

/// `AgentHarnessOptions`.
pub struct AgentHarnessOptions {
    pub session: Session<SessionMetadata>,
    pub models: Arc<dyn Models>,
    pub model: Model,
    pub thinking_level: Option<ThinkingLevel>,
    pub active_tool_names: Option<Vec<String>>,
    pub tools: Option<Vec<HarnessTool>>,
    pub tool_context: Option<AgentHarnessToolContextSource<Value>>,
    pub system_prompt: Option<SystemPromptSource>,
    pub resources: Option<Resources>,
    pub stream_options: Option<StreamOptions>,
    pub retry: Option<RetryPolicy>,
    pub compaction: Option<CompactionSettings>,
    pub steering_mode: Option<QueueMode>,
    pub follow_up_mode: Option<QueueMode>,
    pub tool_execution: Option<ToolExecutionMode>,
    pub drive: Option<DriveMode>,
    pub to_provider_messages: Option<ToProviderMessages>,
    pub entry_projectors: Option<BTreeMap<String, EntryProjector>>,
    pub context: Option<Arc<dyn TelemetryContext>>,
}

impl AgentHarnessOptions {
    /// Constructs options with every optional field absent.
    pub fn new(session: Session<SessionMetadata>, models: Arc<dyn Models>, model: Model) -> Self {
        Self {
            session,
            models,
            model,
            thinking_level: None,
            active_tool_names: None,
            tools: None,
            tool_context: None,
            system_prompt: None,
            resources: None,
            stream_options: None,
            retry: None,
            compaction: None,
            steering_mode: None,
            follow_up_mode: None,
            tool_execution: None,
            drive: None,
            to_provider_messages: None,
            entry_projectors: None,
            context: None,
        }
    }
}

/// `AgentHarness.create` result.
pub struct CreatedHarness {
    pub harness: AgentHarness,
    pub suspended: Vec<SuspendedOperation>,
}

/// `AgentHarness.create` failure.
#[derive(Debug)]
pub enum CreateHarnessError {
    NotImplemented(HarnessNotImplemented),
    Session(SessionError),
}

impl Display for CreateHarnessError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotImplemented(error) => Display::fmt(error, formatter),
            Self::Session(error) => Display::fmt(error, formatter),
        }
    }
}

impl Error for CreateHarnessError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::NotImplemented(error) => Some(error),
            Self::Session(error) => Some(error),
        }
    }
}

impl From<HarnessNotImplemented> for CreateHarnessError {
    fn from(error: HarnessNotImplemented) -> Self {
        Self::NotImplemented(error)
    }
}

impl From<SessionError> for CreateHarnessError {
    fn from(error: SessionError) -> Self {
        Self::Session(error)
    }
}

/// `AgentHarness`: scaffold harness with working configuration state.
pub struct AgentHarness {
    pub session: Session<SessionMetadata>,
    pub hooks: Hooks,
    pub events: Events,
    durable_session: Session<SessionMetadata>,
    model: Mutex<Model>,
    thinking_level: Mutex<ThinkingLevel>,
    active_tool_names: Mutex<Vec<String>>,
    tools: Mutex<Vec<HarnessTool>>,
    resources: Mutex<Resources>,
    stream_options: Mutex<StreamOptions>,
    retry_policy: Mutex<RetryPolicy>,
    compaction_settings: Mutex<CompactionSettings>,
    steering_mode: Mutex<QueueMode>,
    follow_up_mode: Mutex<QueueMode>,
    closed: ClosedFlag,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().expect("harness state mutex must not be poisoned")
}

fn initial_active_tool_names(options: &AgentHarnessOptions) -> Vec<String> {
    match &options.active_tool_names {
        Some(names) => names.clone(),
        None => options.tools.as_ref().map_or_else(Vec::new, |tools| {
            tools.iter().map(|tool| tool.name().to_owned()).collect()
        }),
    }
}

impl AgentHarness {
    /// Returns the lane name.
    pub const fn name(&self) -> &'static str {
        "main"
    }

    /// `AgentHarness.create`: opens a record-free session.
    pub async fn create(options: AgentHarnessOptions) -> Result<CreatedHarness, CreateHarnessError> {
        let records = options
            .session
            .find_records(RecordQuery {
                limit: Some(1),
                ..Default::default()
            })
            .await?;
        if !records.is_empty() {
            return Err(HarnessNotImplemented::new("create.restore").into());
        }
        Ok(CreatedHarness {
            harness: Self::new(options),
            suspended: Vec::new(),
        })
    }

    fn new(options: AgentHarnessOptions) -> Self {
        let closed = ClosedFlag::new();
        let active_tool_names = initial_active_tool_names(&options);
        let resources = options.resources.unwrap_or(Resources {
            skills: None,
            prompt_templates: None,
        });
        Self {
            durable_session: options.session.clone(),
            session: options.session,
            hooks: Hooks { closed: closed.clone() },
            events: Events { closed: closed.clone() },
            model: Mutex::new(options.model),
            thinking_level: Mutex::new(options.thinking_level.unwrap_or(ThinkingLevel::Off)),
            active_tool_names: Mutex::new(active_tool_names),
            tools: Mutex::new(options.tools.unwrap_or_default()),
            resources: Mutex::new(resources),
            stream_options: Mutex::new(options.stream_options.unwrap_or_default()),
            retry_policy: Mutex::new(options.retry.unwrap_or(RetryPolicy {
                enabled: false,
                max_retries: 0,
                base_delay_ms: 1000.0,
            })),
            compaction_settings: Mutex::new(options.compaction.unwrap_or(DEFAULT_COMPACTION_SETTINGS)),
            steering_mode: Mutex::new(options.steering_mode.unwrap_or(QueueMode::OneAtATime)),
            follow_up_mode: Mutex::new(options.follow_up_mode.unwrap_or(QueueMode::OneAtATime)),
            closed,
        }
    }

    fn unavailable<T>(&self, operation: &str) -> Result<T, HarnessUnavailable> {
        Err(unavailable(self.closed.is_closed(), operation))
    }

    pub async fn get_leaf_id(&self) -> Result<Option<String>, SessionError> {
        self.durable_session.get_leaf_id().await
    }

    pub async fn prompt(
        &self,
        _input: impl Into<PromptInput>,
        _images: Option<Vec<ImageContent>>,
    ) -> Result<RunResult, HarnessUnavailable> {
        self.unavailable("prompt")
    }

    pub async fn skill(
        &self,
        _name: &str,
        _additional_instructions: Option<&str>,
    ) -> Result<RunResult, HarnessUnavailable> {
        self.unavailable("skill")
    }

    pub async fn prompt_from_template(
        &self,
        _name: &str,
        _args: Option<&[String]>,
    ) -> Result<RunResult, HarnessUnavailable> {
        self.unavailable("promptFromTemplate")
    }

    pub async fn compact(
        &self,
        _options: Option<HarnessCompactOptions>,
    ) -> Result<CompactionResult, HarnessUnavailable> {
        self.unavailable("compact")
    }

    pub async fn navigate_tree(
        &self,
        _target_id: Option<&str>,
        _options: Option<NavigateOptions>,
    ) -> Result<NavigationResult, HarnessUnavailable> {
        self.unavailable("navigateTree")
    }

    pub async fn resume(&self) -> Result<ResumeResult, HarnessUnavailable> {
        self.unavailable("resume")
    }

    pub async fn abort(&self) -> Result<AbortResult, HarnessUnavailable> {
        self.unavailable("abort")
    }

    pub async fn steer(
        &self,
        _input: QueueInput,
        _images: Option<Vec<ImageContent>>,
    ) -> Result<QueueResult, HarnessUnavailable> {
        self.unavailable("steer")
    }

    pub async fn follow_up(
        &self,
        _input: QueueInput,
        _images: Option<Vec<ImageContent>>,
    ) -> Result<QueueResult, HarnessUnavailable> {
        self.unavailable("followUp")
    }

    pub async fn next_run(
        &self,
        _input: QueueInput,
        _images: Option<Vec<ImageContent>>,
    ) -> Result<QueueResult, HarnessUnavailable> {
        self.unavailable("nextRun")
    }

    pub async fn cancel_queued(&self, _entry_id: &str) -> Result<CancelQueuedResult, HarnessUnavailable> {
        self.unavailable("cancelQueued")
    }

    pub async fn record_usage(
        &self,
        _usage: Usage,
        _options: Option<RecordUsageOptions>,
    ) -> Result<RecordUsageResult, HarnessUnavailable> {
        self.unavailable("recordUsage")
    }

    pub async fn wait_for_idle(&self) -> Result<(), HarnessUnavailable> {
        self.unavailable("waitForIdle")
    }

    pub async fn run_when_idle(&self, _callback: impl FnOnce() + Send) -> Result<(), HarnessUnavailable> {
        self.unavailable("runWhenIdle")
    }

    pub async fn peek_action(&self) -> Result<Option<ActionInfo>, HarnessUnavailable> {
        self.unavailable("peekAction")
    }

    pub async fn execute_action(&self) -> Result<Option<ActionInfo>, HarnessUnavailable> {
        self.unavailable("executeAction")
    }

    pub async fn run_to_completion(&self) -> Result<(), HarnessUnavailable> {
        self.unavailable("runToCompletion")
    }

    pub async fn watch(&self) -> Result<WatchHandle<LaneSnapshot>, HarnessUnavailable> {
        self.unavailable("watch")
    }

    pub async fn lane(&self, _name: &str) -> Result<Option<Arc<dyn AgentLane>>, HarnessUnavailable> {
        self.unavailable("lane")
    }

    pub async fn create_lane(&self, _name: &str, _at: Option<&str>) -> Result<CreateLaneResult, HarnessUnavailable> {
        self.unavailable("createLane")
    }

    pub async fn lanes(&self) -> Result<Vec<LaneInfo>, HarnessUnavailable> {
        self.unavailable("lanes")
    }

    pub async fn get_tools(&self) -> Vec<HarnessTool> {
        lock(&self.tools).clone()
    }

    pub async fn set_tools(&self, tools: Vec<HarnessTool>, active_names: Option<Vec<String>>) {
        let names = active_names.unwrap_or_else(|| tools.iter().map(HarnessTool::name).map(str::to_owned).collect());
        *lock(&self.tools) = tools;
        *lock(&self.active_tool_names) = names;
    }

    pub async fn get_resources(&self) -> Resources {
        let resources = lock(&self.resources);
        Resources {
            skills: resources.skills.clone(),
            prompt_templates: resources.prompt_templates.clone(),
        }
    }

    pub async fn set_resources(&self, resources: Resources) {
        *lock(&self.resources) = Resources {
            skills: resources.skills,
            prompt_templates: resources.prompt_templates,
        };
    }

    pub async fn get_stream_options(&self) -> StreamOptions {
        lock(&self.stream_options).clone()
    }

    pub async fn set_stream_options(&self, options: StreamOptions) {
        *lock(&self.stream_options) = options;
    }

    pub async fn get_retry_policy(&self) -> RetryPolicy {
        *lock(&self.retry_policy)
    }

    pub async fn set_retry_policy(&self, policy: RetryPolicy) {
        *lock(&self.retry_policy) = policy;
    }

    pub async fn get_compaction_settings(&self) -> CompactionSettings {
        *lock(&self.compaction_settings)
    }

    pub async fn set_compaction_settings(&self, settings: CompactionSettings) {
        *lock(&self.compaction_settings) = settings;
    }

    pub async fn get_steering_mode(&self) -> QueueMode {
        *lock(&self.steering_mode)
    }

    pub async fn set_steering_mode(&self, mode: QueueMode) {
        *lock(&self.steering_mode) = mode;
    }

    pub async fn get_follow_up_mode(&self) -> QueueMode {
        *lock(&self.follow_up_mode)
    }

    pub async fn set_follow_up_mode(&self, mode: QueueMode) {
        *lock(&self.follow_up_mode) = mode;
    }

    pub async fn watch_session(&self) -> Result<WatchHandle<SessionSnapshot>, HarnessUnavailable> {
        self.unavailable("watchSession")
    }

    pub async fn close(&self) {
        self.closed.close();
    }

    pub async fn get_model(&self) -> Model {
        lock(&self.model).clone()
    }

    pub async fn set_model(&self, model: Model) {
        *lock(&self.model) = model;
    }

    pub async fn get_thinking_level(&self) -> ThinkingLevel {
        *lock(&self.thinking_level)
    }

    pub async fn set_thinking_level(&self, level: ThinkingLevel) {
        *lock(&self.thinking_level) = level;
    }

    pub async fn get_active_tools(&self) -> Vec<String> {
        lock(&self.active_tool_names).clone()
    }

    pub async fn set_active_tools(&self, names: Vec<String>) {
        *lock(&self.active_tool_names) = names;
    }
}
