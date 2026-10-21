use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::{self, Display, Formatter};

use pi_ai::types::{AssistantMessage, DeferredHandle, StopReason, ToolCall};
use pi_js_compat::number::js_number_to_string;
use serde::{Deserialize, Serialize};

use crate::agent::AgentMessage;
use crate::harness::session::{
    CompactionReason, Entry, EntryBody, EntryType, LaneRecord, OperationIntent, OperationKind, QueueKind, RecordBody,
    StepKind, UsageCause,
};

/// Machine-readable category for a contradiction in a lane's durable recovery slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordLogCorruptionReason {
    MultipleOpenOperations,
    UnknownOperation,
    RecordAfterFinish,
    NonConsecutiveAttempt,
    InvalidCompactionReason,
    QueueAfterAbort,
    InvalidQueueCancellation,
    InconsistentStep,
    ToolCallMismatch,
    DuplicateToolInvocation,
    ProvisionedEntryMismatch,
    InvalidDeferredHandle,
}

impl RecordLogCorruptionReason {
    /// Returns the exact source reason literal.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MultipleOpenOperations => "multiple_open_operations",
            Self::UnknownOperation => "unknown_operation",
            Self::RecordAfterFinish => "record_after_finish",
            Self::NonConsecutiveAttempt => "non_consecutive_attempt",
            Self::InvalidCompactionReason => "invalid_compaction_reason",
            Self::QueueAfterAbort => "queue_after_abort",
            Self::InvalidQueueCancellation => "invalid_queue_cancellation",
            Self::InconsistentStep => "inconsistent_step",
            Self::ToolCallMismatch => "tool_call_mismatch",
            Self::DuplicateToolInvocation => "duplicate_tool_invocation",
            Self::ProvisionedEntryMismatch => "provisioned_entry_mismatch",
            Self::InvalidDeferredHandle => "invalid_deferred_handle",
        }
    }
}

/// Contradiction in a lane's durable recovery slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordLogCorruption {
    reason: RecordLogCorruptionReason,
    message: String,
}

impl RecordLogCorruption {
    fn new(reason: RecordLogCorruptionReason, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
        }
    }

    /// Stable corruption category.
    pub const fn reason(&self) -> RecordLogCorruptionReason {
        self.reason
    }

    /// Human-readable detail.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl Display for RecordLogCorruption {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for RecordLogCorruption {}

/// Bounded lane recovery slice.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordLogSlice {
    pub lane: String,
    pub open_operations: Vec<LaneRecord>,
    pub records: Vec<LaneRecord>,
    pub entries: Vec<Entry>,
}

/// Effective model/thinking/tool configuration for a lane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectiveLaneConfiguration {
    pub model: LaneModel,
    pub thinking_level: String,
    pub active_tool_names: Vec<String>,
}

/// Provider and model identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneModel {
    pub provider: String,
    pub model_id: String,
}

/// Provenance of a terminal assistant failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalFailureSource {
    Step,
    DeferredFetch,
}

/// Terminal assistant failure detected at the operation tail.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalFailureState {
    pub entry_id: String,
    pub source: TerminalFailureSource,
    pub message: AssistantMessage,
}

/// One tool call in a reduced tool batch.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolBatchCall {
    pub tool_index: usize,
    pub tool_call: ToolCall,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started: Option<LaneRecord>,
    pub result_exists: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminate: Option<bool>,
}

/// Reduced tool-batch state for the newest assistant tool-call message.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolBatchState {
    pub assistant_entry_id: String,
    pub calls: Vec<ToolBatchCall>,
    pub truncated: bool,
    pub unresolved: bool,
}

/// Unfinished step attempt for an open operation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneStepState {
    pub kind: StepKind,
    #[serde(serialize_with = "pi_js_compat::number::serialize")]
    pub attempts: f64,
    pub result_entry_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compaction_reason: Option<CompactionReason>,
}

/// Newest entry appended by the open operation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NewestOwnState {
    pub entry_id: String,
    #[serde(rename = "type")]
    pub entry_type: EntryType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<pi_js_compat::js_json::JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
}

/// Structural result targets of the open operation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneTargets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<bool>,
}

/// Reduced state of the open operation.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneOperationState {
    pub id: String,
    pub kind: OperationKind,
    pub intent: OperationIntent,
    pub aborting: bool,
    pub step: Option<LaneStepState>,
    pub tool_batch: Option<ToolBatchState>,
    pub missing_initial_messages: Vec<EntryBody>,
    pub pending_steer: Vec<EntryBody>,
    pub pending_follow_up: Vec<EntryBody>,
    pub pending_writes: Vec<EntryBody>,
    pub deferred: Option<DeferredHandle>,
    pub overflow_recovery_used: bool,
    pub newest_own: Option<NewestOwnState>,
    pub targets: LaneTargets,
}

/// Reduced orchestration state for one lane.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneState {
    pub lane: String,
    pub leaf_id: Option<String>,
    pub operation: Option<LaneOperationState>,
    pub pending_next_run: Vec<EntryBody>,
}

/// Result of [`reduce_lane_state`].
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneReductionResult {
    pub lane_state: LaneState,
    pub effective_configuration: EffectiveLaneConfiguration,
    pub terminal_failure: Option<TerminalFailureState>,
}

/// Input for [`reduce_lane_state`].
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaneReductionInput {
    pub lane: String,
    pub open_operations: Vec<LaneRecord>,
    pub records: Vec<LaneRecord>,
    pub entries: Vec<Entry>,
    pub leaf_id: Option<String>,
    /// Entries appended by the open operation, oldest first.
    pub own_entries: Vec<Entry>,
    /// Bounded effective-state lookups at the operation anchor or idle leaf.
    pub configuration_entries: Vec<Entry>,
    /// Harness option fallbacks used when no persisted value exists.
    pub defaults: EffectiveLaneConfiguration,
}

impl LaneReductionInput {
    fn slice(&self) -> RecordLogSlice {
        RecordLogSlice {
            lane: self.lane.clone(),
            open_operations: self.open_operations.clone(),
            records: self.records.clone(),
            entries: self.entries.clone(),
        }
    }
}

/// Validates a bounded lane recovery slice without reading or mutating session state.
pub fn validate_record_log(input: &RecordLogSlice) -> Result<(), RecordLogCorruption> {
    if input.open_operations.len() > 1 {
        return Err(corrupt(
            RecordLogCorruptionReason::MultipleOpenOperations,
            format!("Lane {} has at least two open operations", input.lane),
        ));
    }

    let entries_by_id = entries_by_id(&input.entries);
    validate_deferred_handles(&input.entries)?;
    let mut state = ValidationState::default();
    for record in by_sequence(&input.records) {
        validate_record(record, &mut state, &entries_by_id)?;
    }
    Ok(())
}

#[derive(Default)]
struct ValidationState<'a> {
    starts: HashSet<&'a str>,
    finished_at: HashMap<&'a str, i64>,
    aborted_at: HashMap<&'a str, i64>,
    queue_enqueues: HashMap<&'a str, &'a LaneRecord>,
    latest_attempt: HashMap<&'a str, &'a LaneRecord>,
    tool_invocations: HashSet<String>,
}

fn validate_record<'a>(
    record: &'a LaneRecord,
    state: &mut ValidationState<'a>,
    entries_by_id: &HashMap<&str, &Entry>,
) -> Result<(), RecordLogCorruption> {
    if let RecordBody::OperationStarted { id, intent, .. } = &record.body {
        state.starts.insert(id);
        return validate_operation_result(entries_by_id, intent);
    }
    validate_operation_reference(record, state)?;
    validate_record_body(record, state, entries_by_id)
}

fn validate_operation_reference(record: &LaneRecord, state: &ValidationState<'_>) -> Result<(), RecordLogCorruption> {
    let Some(run_id) = record_run_id(&record.body) else {
        return Ok(());
    };
    if !state.starts.contains(run_id) {
        return Err(corrupt(
            RecordLogCorruptionReason::UnknownOperation,
            format!("Record {} references unknown operation {run_id}", record.id()),
        ));
    }
    if let Some(finish_seq) = state.finished_at.get(run_id)
        && record.seq > *finish_seq
    {
        return Err(corrupt(
            RecordLogCorruptionReason::RecordAfterFinish,
            format!("Record {} follows the finish of operation {run_id}", record.id()),
        ));
    }
    Ok(())
}

fn validate_record_body<'a>(
    record: &'a LaneRecord,
    state: &mut ValidationState<'a>,
    entries_by_id: &HashMap<&str, &Entry>,
) -> Result<(), RecordLogCorruption> {
    match &record.body {
        RecordBody::OperationStarted { .. } => Ok(()),
        RecordBody::OperationFinished { run_id, .. } => {
            state.finished_at.insert(run_id, record.seq);
            Ok(())
        }
        RecordBody::AbortRequested { run_id, .. } => {
            state.aborted_at.insert(run_id, record.seq);
            Ok(())
        }
        RecordBody::StepAttempt { .. } => validate_step_attempt(record, state, entries_by_id),
        RecordBody::ToolStarted { .. } => validate_tool_start(record, entries_by_id, &mut state.tool_invocations),
        RecordBody::QueueEnqueued {
            queue, run_id, target, ..
        } => validate_queue_enqueued(record, *queue, run_id.as_deref(), target, state, entries_by_id),
        RecordBody::QueueCancelled {
            id, entry_id, run_id, ..
        } => validate_queue_cancelled(record, id, entry_id, run_id.as_deref(), state, entries_by_id),
        RecordBody::WriteDeferred { target, .. } => validate_exact_provisioned_entry(entries_by_id, target),
        RecordBody::Usage { .. } => Ok(()),
    }
}

fn validate_step_attempt<'a>(
    record: &'a LaneRecord,
    state: &mut ValidationState<'a>,
    entries_by_id: &HashMap<&str, &Entry>,
) -> Result<(), RecordLogCorruption> {
    let RecordBody::StepAttempt {
        id,
        run_id,
        step,
        result_entry_id,
        compaction_reason,
        ..
    } = &record.body
    else {
        return Ok(());
    };
    validate_attempt_reason(id, *step, *compaction_reason)?;
    validate_attempt_sequence(
        record,
        state.latest_attempt.get(run_id.as_str()).copied(),
        entries_by_id,
    )?;
    validate_attempt_result(entries_by_id, *step, result_entry_id)?;
    state.latest_attempt.insert(run_id, record);
    Ok(())
}

fn validate_queue_enqueued<'a>(
    record: &'a LaneRecord,
    queue: QueueKind,
    run_id: Option<&str>,
    target: &'a EntryBody,
    state: &mut ValidationState<'a>,
    entries_by_id: &HashMap<&str, &Entry>,
) -> Result<(), RecordLogCorruption> {
    let aborted_after_enqueue = run_id
        .and_then(|run_id| state.aborted_at.get(run_id))
        .is_some_and(|aborted_seq| record.seq > *aborted_seq);
    if queue != QueueKind::NextRun && aborted_after_enqueue {
        return Err(corrupt(
            RecordLogCorruptionReason::QueueAfterAbort,
            format!("{} item {} was enqueued after abort", queue_literal(queue), target.id()),
        ));
    }
    state.queue_enqueues.insert(target.id(), record);
    validate_exact_provisioned_entry(entries_by_id, target)
}

fn validate_queue_cancelled(
    record: &LaneRecord,
    id: &str,
    entry_id: &str,
    run_id: Option<&str>,
    state: &ValidationState<'_>,
    entries_by_id: &HashMap<&str, &Entry>,
) -> Result<(), RecordLogCorruption> {
    let enqueue = state.queue_enqueues.get(entry_id);
    let has_pending_enqueue = enqueue.is_some_and(|enqueue| {
        enqueue.seq < record.seq && record_run_id(&enqueue.body) == run_id && !entries_by_id.contains_key(entry_id)
    });
    if has_pending_enqueue {
        return Ok(());
    }
    Err(corrupt(
        RecordLogCorruptionReason::InvalidQueueCancellation,
        format!("Queue cancellation {id} has no pending matching enqueue"),
    ))
}

/// Purely reconstructs one lane's orchestration state from its bounded recovery inputs.
pub fn reduce_lane_state(input: &LaneReductionInput) -> Result<LaneReductionResult, RecordLogCorruption> {
    validate_record_log(&input.slice())?;

    let records = by_sequence(&input.records);
    let own_entries = by_sequence(&input.own_entries);
    let entries_by_id = entries_by_id_with_own(&input.entries, &input.own_entries);
    let cancelled_ids = cancelled_queue_ids(&records);
    let queue_enqueues = pending_queue_enqueues(&records, &entries_by_id, &cancelled_ids);
    let started = input.open_operations.first();
    let captured_initial_ids = started
        .and_then(operation_started)
        .map_or_else(HashSet::new, |(_, intent)| captured_initial_ids(intent));
    let next_run_inputs = pending_next_run(&queue_enqueues, &captured_initial_ids);
    let effective_configuration = derive_effective_configuration(input);

    let Some(started) = started else {
        return Ok(idle_lane_result(input, next_run_inputs, effective_configuration));
    };
    let Some((operation_id, intent)) = operation_started(started) else {
        return Err(corrupt(
            RecordLogCorruptionReason::UnknownOperation,
            "Open operation is not started",
        ));
    };

    let operation_log = operation_records(&records, operation_id);
    let aborting = operation_log
        .iter()
        .any(|record| matches!(record.body, RecordBody::AbortRequested { .. }));
    let steer_inputs = pending_queue_for(&queue_enqueues, QueueKind::Steer, operation_id, aborting);
    let follow_up_inputs = pending_queue_for(&queue_enqueues, QueueKind::FollowUp, operation_id, aborting);
    let write_entries = pending_writes(&operation_log, &entries_by_id);
    let missing_inputs = missing_initial_messages(intent, &entries_by_id);
    let step = derive_step(&operation_log, &entries_by_id);
    let overflow_recovery_used = overflow_recovery_used(&operation_log, intent, &entries_by_id);
    let newest_own_entry = own_entries.last().copied();
    let newest_own = newest_own_entry.and_then(derive_newest_own);
    let deferred = newest_own_entry.and_then(deferred_handle);
    let targets = derive_targets(intent, &entries_by_id);
    let write_deferred_ids = deferred_write_ids(&operation_log);
    let terminal_failure = derive_terminal_failure(newest_own_entry, &own_entries, &operation_log, &write_deferred_ids);
    let tool_batch = derive_tool_batch(
        operation_id,
        &operation_log,
        &own_entries,
        &entries_by_id,
        &write_deferred_ids,
    );

    Ok(LaneReductionResult {
        lane_state: LaneState {
            lane: input.lane.clone(),
            leaf_id: input.leaf_id.clone(),
            operation: Some(LaneOperationState {
                id: operation_id.to_owned(),
                kind: intent.kind(),
                intent: intent.clone(),
                aborting,
                step,
                tool_batch,
                missing_initial_messages: missing_inputs,
                pending_steer: steer_inputs,
                pending_follow_up: follow_up_inputs,
                pending_writes: write_entries,
                deferred,
                overflow_recovery_used,
                newest_own,
                targets,
            }),
            pending_next_run: next_run_inputs,
        },
        effective_configuration,
        terminal_failure,
    })
}

fn idle_lane_result(
    input: &LaneReductionInput,
    pending_next_run: Vec<EntryBody>,
    effective_configuration: EffectiveLaneConfiguration,
) -> LaneReductionResult {
    LaneReductionResult {
        lane_state: LaneState {
            lane: input.lane.clone(),
            leaf_id: input.leaf_id.clone(),
            operation: None,
            pending_next_run,
        },
        effective_configuration,
        terminal_failure: None,
    }
}

fn cancelled_queue_ids<'a>(records: &[&'a LaneRecord]) -> HashSet<&'a str> {
    records
        .iter()
        .filter_map(|record| match &record.body {
            RecordBody::QueueCancelled { entry_id, .. } => Some(entry_id.as_str()),
            _ => None,
        })
        .collect()
}

fn pending_queue_enqueues<'a>(
    records: &[&'a LaneRecord],
    entries_by_id: &HashMap<&str, &Entry>,
    cancelled_queue_ids: &HashSet<&str>,
) -> Vec<&'a LaneRecord> {
    records
        .iter()
        .copied()
        .filter(|record| match &record.body {
            RecordBody::QueueEnqueued { target, .. } => {
                !entries_by_id.contains_key(target.id()) && !cancelled_queue_ids.contains(target.id())
            }
            _ => false,
        })
        .collect()
}

fn captured_initial_ids(intent: &OperationIntent) -> HashSet<&str> {
    match intent {
        OperationIntent::Run { initial_messages, .. } => initial_messages.iter().map(EntryBody::id).collect(),
        _ => HashSet::new(),
    }
}

fn pending_next_run(records: &[&LaneRecord], captured_initial_ids: &HashSet<&str>) -> Vec<EntryBody> {
    records
        .iter()
        .filter_map(|record| match &record.body {
            RecordBody::QueueEnqueued { queue, target, .. }
                if *queue == QueueKind::NextRun && !captured_initial_ids.contains(target.id()) =>
            {
                Some(target.clone())
            }
            _ => None,
        })
        .collect()
}

fn operation_records<'a>(records: &[&'a LaneRecord], operation_id: &str) -> Vec<&'a LaneRecord> {
    records
        .iter()
        .copied()
        .filter(|record| match &record.body {
            RecordBody::OperationStarted { id, .. } => id == operation_id,
            _ => record_run_id(&record.body) == Some(operation_id),
        })
        .collect()
}

fn pending_queue_for(records: &[&LaneRecord], queue: QueueKind, operation_id: &str, aborting: bool) -> Vec<EntryBody> {
    if aborting {
        return Vec::new();
    }
    pending_queue(records, queue, operation_id)
}

fn pending_writes(operation_records: &[&LaneRecord], entries_by_id: &HashMap<&str, &Entry>) -> Vec<EntryBody> {
    operation_records
        .iter()
        .filter_map(|record| match &record.body {
            RecordBody::WriteDeferred { target, .. } if !entries_by_id.contains_key(target.id()) => {
                Some(target.clone())
            }
            _ => None,
        })
        .collect()
}

fn missing_initial_messages(intent: &OperationIntent, entries_by_id: &HashMap<&str, &Entry>) -> Vec<EntryBody> {
    match intent {
        OperationIntent::Run { initial_messages, .. } => initial_messages
            .iter()
            .filter(|target| !entries_by_id.contains_key(target.id()))
            .cloned()
            .collect(),
        _ => Vec::new(),
    }
}

fn derive_step(operation_records: &[&LaneRecord], entries_by_id: &HashMap<&str, &Entry>) -> Option<LaneStepState> {
    let newest_attempt = operation_records
        .iter()
        .rev()
        .find(|record| matches!(record.body, RecordBody::StepAttempt { .. }))?;
    let RecordBody::StepAttempt {
        step,
        attempt,
        result_entry_id,
        compaction_reason,
        ..
    } = &newest_attempt.body
    else {
        return None;
    };
    if entries_by_id.contains_key(result_entry_id.as_str()) {
        return None;
    }
    Some(LaneStepState {
        kind: *step,
        attempts: *attempt,
        result_entry_id: result_entry_id.clone(),
        compaction_reason: if *step == StepKind::Compaction {
            *compaction_reason
        } else {
            None
        },
    })
}

fn overflow_recovery_used(
    operation_records: &[&LaneRecord],
    intent: &OperationIntent,
    entries_by_id: &HashMap<&str, &Entry>,
) -> bool {
    let newest_consumed = newest_consumed_input_sequence(operation_records, intent, entries_by_id);
    operation_records.iter().any(|record| match &record.body {
        RecordBody::StepAttempt {
            step,
            compaction_reason,
            ..
        } => {
            *step == StepKind::Compaction
                && *compaction_reason == Some(CompactionReason::Overflow)
                && record.seq as f64 > newest_consumed
        }
        _ => false,
    })
}

fn newest_consumed_input_sequence(
    operation_records: &[&LaneRecord],
    intent: &OperationIntent,
    entries_by_id: &HashMap<&str, &Entry>,
) -> f64 {
    let mut consumed_input_ids = captured_initial_ids(intent);
    for record in operation_records {
        if let RecordBody::QueueEnqueued { queue, target, .. } = &record.body
            && *queue != QueueKind::NextRun
        {
            consumed_input_ids.insert(target.id());
        }
    }
    consumed_input_ids
        .into_iter()
        .filter_map(|id| entries_by_id.get(id))
        .filter(|entry| entry.entry_type() == EntryType::Message)
        .map(|entry| entry.seq as f64)
        .fold(f64::NEG_INFINITY, f64::max)
}

fn deferred_write_ids<'a>(operation_records: &[&'a LaneRecord]) -> HashSet<&'a str> {
    operation_records
        .iter()
        .filter_map(|record| match &record.body {
            RecordBody::WriteDeferred { target, .. } => Some(target.id()),
            _ => None,
        })
        .collect()
}

fn corrupt(reason: RecordLogCorruptionReason, message: impl Into<String>) -> RecordLogCorruption {
    RecordLogCorruption::new(reason, message)
}

fn operation_started(record: &LaneRecord) -> Option<(&str, &OperationIntent)> {
    match &record.body {
        RecordBody::OperationStarted { id, intent, .. } => Some((id, intent)),
        _ => None,
    }
}

fn record_run_id(body: &RecordBody) -> Option<&str> {
    match body {
        RecordBody::OperationStarted { .. } => None,
        RecordBody::AbortRequested { run_id, .. }
        | RecordBody::OperationFinished { run_id, .. }
        | RecordBody::StepAttempt { run_id, .. }
        | RecordBody::ToolStarted { run_id, .. }
        | RecordBody::WriteDeferred { run_id, .. } => Some(run_id),
        RecordBody::QueueEnqueued { run_id, .. }
        | RecordBody::QueueCancelled { run_id, .. }
        | RecordBody::Usage { run_id, .. } => run_id.as_deref(),
    }
}

fn by_sequence<T: HasSeq>(values: &[T]) -> Vec<&T> {
    let mut sorted: Vec<&T> = values.iter().collect();
    sorted.sort_by_key(|value| value.seq());
    sorted
}

trait HasSeq {
    fn seq(&self) -> i64;
}

impl HasSeq for LaneRecord {
    fn seq(&self) -> i64 {
        self.seq
    }
}

impl HasSeq for Entry {
    fn seq(&self) -> i64 {
        self.seq
    }
}

fn entries_by_id(entries: &[Entry]) -> HashMap<&str, &Entry> {
    entries.iter().map(|entry| (entry.id(), entry)).collect()
}

fn entries_by_id_with_own<'a>(entries: &'a [Entry], own_entries: &'a [Entry]) -> HashMap<&'a str, &'a Entry> {
    let mut by_id = entries_by_id(entries);
    for entry in own_entries {
        by_id.insert(entry.id(), entry);
    }
    by_id
}

fn matches_provisioned_entry(entry: &Entry, target: &EntryBody) -> bool {
    let Ok(mut entry_value) = serde_json::to_value(entry) else {
        return false;
    };
    let Some(object) = entry_value.as_object_mut() else {
        return false;
    };
    object.remove("parentId");
    object.remove("seq");
    object.remove("timestamp");
    serde_json::to_value(target).is_ok_and(|target_value| target_value == entry_value)
}

fn validate_exact_provisioned_entry(
    entries_by_id: &HashMap<&str, &Entry>,
    target: &EntryBody,
) -> Result<(), RecordLogCorruption> {
    if let Some(entry) = entries_by_id.get(target.id())
        && !matches_provisioned_entry(entry, target)
    {
        return Err(corrupt(
            RecordLogCorruptionReason::ProvisionedEntryMismatch,
            format!(
                "Provisioned entry {} exists with content different from its intent",
                target.id()
            ),
        ));
    }
    Ok(())
}

fn validate_result_entry(
    entries_by_id: &HashMap<&str, &Entry>,
    result_entry_id: &str,
    matches: impl Fn(&Entry) -> bool,
    description: &str,
) -> Result<(), RecordLogCorruption> {
    if let Some(entry) = entries_by_id.get(result_entry_id)
        && !matches(entry)
    {
        return Err(corrupt(
            RecordLogCorruptionReason::ProvisionedEntryMismatch,
            format!("Provisioned {description} entry {result_entry_id} exists with different content"),
        ));
    }
    Ok(())
}

fn validate_attempt_reason(
    id: &str,
    step: StepKind,
    compaction_reason: Option<CompactionReason>,
) -> Result<(), RecordLogCorruption> {
    if step == StepKind::Compaction {
        if compaction_reason.is_none() {
            return Err(corrupt(
                RecordLogCorruptionReason::InvalidCompactionReason,
                format!("Compaction attempt {id} has no valid compaction reason"),
            ));
        }
    } else if compaction_reason.is_some() {
        return Err(corrupt(
            RecordLogCorruptionReason::InvalidCompactionReason,
            format!("{} attempt {id} has a compaction reason", step_literal(step)),
        ));
    }
    Ok(())
}

fn validate_attempt_sequence(
    record: &LaneRecord,
    previous: Option<&LaneRecord>,
    entries_by_id: &HashMap<&str, &Entry>,
) -> Result<(), RecordLogCorruption> {
    let RecordBody::StepAttempt {
        id,
        step,
        attempt,
        result_entry_id,
        compaction_reason,
        ..
    } = &record.body
    else {
        return Ok(());
    };
    let previous_parts = previous.and_then(step_attempt_parts);
    let previous_result =
        previous_parts.and_then(|(_, _, previous_result_id, _)| entries_by_id.get(previous_result_id).copied());
    let continues = previous_parts.is_some_and(|(previous_step, _, _, _)| {
        previous_step == *step
            && (previous_result.is_none() || previous_result.is_some_and(|result| result.seq >= record.seq))
    });
    let expected_attempt = match previous_parts {
        Some((_, previous_attempt, _, _)) if continues => previous_attempt + 1.0,
        _ => 1.0,
    };
    if *attempt != expected_attempt {
        return Err(corrupt(
            RecordLogCorruptionReason::NonConsecutiveAttempt,
            format!(
                "{} attempt {id} is {}; expected {}",
                step_literal(*step),
                js_number_to_string(*attempt),
                js_number_to_string(expected_attempt)
            ),
        ));
    }
    if !continues || *step == StepKind::Assistant {
        return Ok(());
    }
    let Some((_, _, previous_result_id, previous_reason)) = previous_parts else {
        return Ok(());
    };
    if result_entry_id != previous_result_id {
        return Err(corrupt(
            RecordLogCorruptionReason::InconsistentStep,
            format!("{} attempts disagree on their result entry id", step_literal(*step)),
        ));
    }
    if *compaction_reason != previous_reason {
        return Err(corrupt(
            RecordLogCorruptionReason::InconsistentStep,
            format!("{} attempts disagree on their compaction reason", step_literal(*step)),
        ));
    }
    Ok(())
}

fn step_attempt_parts(record: &LaneRecord) -> Option<(StepKind, f64, &str, Option<CompactionReason>)> {
    match &record.body {
        RecordBody::StepAttempt {
            step,
            attempt,
            result_entry_id,
            compaction_reason,
            ..
        } => Some((*step, *attempt, result_entry_id, *compaction_reason)),
        _ => None,
    }
}

fn validate_attempt_result(
    entries_by_id: &HashMap<&str, &Entry>,
    step: StepKind,
    result_entry_id: &str,
) -> Result<(), RecordLogCorruption> {
    match step {
        StepKind::Assistant => validate_result_entry(
            entries_by_id,
            result_entry_id,
            |entry| {
                matches!(
                    &entry.body,
                    EntryBody::Message {
                        message: AgentMessage::Assistant(_),
                        ..
                    }
                )
            },
            "assistant result",
        ),
        StepKind::Compaction => validate_result_entry(
            entries_by_id,
            result_entry_id,
            |entry| entry.entry_type() == EntryType::Compaction,
            "compaction result",
        ),
        StepKind::BranchSummary => validate_result_entry(
            entries_by_id,
            result_entry_id,
            |entry| entry.entry_type() == EntryType::BranchSummary,
            "branch-summary result",
        ),
    }
}

fn validate_tool_start(
    record: &LaneRecord,
    entries_by_id: &HashMap<&str, &Entry>,
    invocations: &mut HashSet<String>,
) -> Result<(), RecordLogCorruption> {
    let RecordBody::ToolStarted {
        id,
        assistant_entry_id,
        tool_index,
        tool_call_id,
        tool_name,
        result_entry_id,
        ..
    } = &record.body
    else {
        return Ok(());
    };
    let invocation = format!("{assistant_entry_id}\u{0}{}", js_number_to_string(*tool_index));
    if !invocations.insert(invocation) {
        return Err(corrupt(
            RecordLogCorruptionReason::DuplicateToolInvocation,
            format!(
                "Tool invocation {assistant_entry_id}:{} is duplicated",
                js_number_to_string(*tool_index)
            ),
        ));
    }

    let assistant_entry = entries_by_id.get(assistant_entry_id.as_str());
    let Some(Entry {
        body: EntryBody::Message {
            message: AgentMessage::Assistant(assistant),
            ..
        },
        ..
    }) = assistant_entry
    else {
        return Err(corrupt(
            RecordLogCorruptionReason::ToolCallMismatch,
            format!("Tool start {id} does not reference an assistant entry"),
        ));
    };
    let tool_calls: Vec<&ToolCall> = assistant
        .content
        .iter()
        .filter_map(|content| match content {
            pi_ai::types::AssistantContent::ToolCall(tool_call) => Some(tool_call),
            _ => None,
        })
        .collect();
    let tool_call = tool_index_ordinal(*tool_index).and_then(|index| tool_calls.get(index));
    if tool_call.is_none_or(|tool_call| tool_call.id != *tool_call_id || tool_call.name != *tool_name) {
        return Err(corrupt(
            RecordLogCorruptionReason::ToolCallMismatch,
            format!("Tool start {id} does not match its assistant tool-call ordinal"),
        ));
    }

    validate_result_entry(
        entries_by_id,
        result_entry_id,
        |entry| {
            matches!(
                &entry.body,
                EntryBody::Message {
                    message: AgentMessage::ToolResult(result),
                    ..
                } if result.tool_call_id == *tool_call_id && result.tool_name == *tool_name
            )
        },
        "tool result",
    )
}

fn validate_deferred_handles(entries: &[Entry]) -> Result<(), RecordLogCorruption> {
    for entry in entries {
        if let EntryBody::Message {
            message: AgentMessage::Assistant(assistant),
            ..
        } = &entry.body
            && assistant.stop_reason == StopReason::Deferred
            && assistant.deferred.is_none()
        {
            return Err(corrupt(
                RecordLogCorruptionReason::InvalidDeferredHandle,
                format!("Deferred assistant entry {} does not carry a handle", entry.id()),
            ));
        }
    }
    Ok(())
}

fn validate_operation_result(
    entries_by_id: &HashMap<&str, &Entry>,
    intent: &OperationIntent,
) -> Result<(), RecordLogCorruption> {
    match intent {
        OperationIntent::Run { initial_messages, .. } => {
            for target in initial_messages {
                validate_exact_provisioned_entry(entries_by_id, target)?;
            }
        }
        OperationIntent::Compaction { result_entry_id, .. } => validate_result_entry(
            entries_by_id,
            result_entry_id,
            |entry| entry.entry_type() == EntryType::Compaction,
            "manual compaction",
        )?,
        OperationIntent::Navigation {
            summary_entry_id: Some(summary_entry_id),
            ..
        } => validate_result_entry(
            entries_by_id,
            summary_entry_id,
            |entry| entry.entry_type() == EntryType::BranchSummary,
            "navigation summary",
        )?,
        OperationIntent::Navigation {
            summary_entry_id: None, ..
        } => {}
    }
    Ok(())
}

fn derive_effective_configuration(input: &LaneReductionInput) -> EffectiveLaneConfiguration {
    let mut configuration = input.defaults.clone();
    let mut order: Vec<&str> = Vec::new();
    let mut by_id: HashMap<&str, &Entry> = HashMap::new();
    for entry in input.configuration_entries.iter().chain(input.own_entries.iter()) {
        if by_id.insert(entry.id(), entry).is_none() {
            order.push(entry.id());
        }
    }
    let mut values: Vec<&Entry> = order.into_iter().map(|id| by_id[id]).collect();
    values.sort_by_key(|entry| entry.seq);
    for entry in values {
        match &entry.body {
            EntryBody::ModelChange { provider, model_id, .. } => {
                configuration.model = LaneModel {
                    provider: provider.clone(),
                    model_id: model_id.clone(),
                };
            }
            EntryBody::ThinkingLevelChange { thinking_level, .. } => {
                configuration.thinking_level = thinking_level.clone();
            }
            EntryBody::ActiveToolsChange { active_tool_names, .. } => {
                configuration.active_tool_names = active_tool_names.clone();
            }
            EntryBody::Message {
                message: AgentMessage::Assistant(assistant),
                ..
            } => {
                configuration.model = LaneModel {
                    provider: assistant.provider.clone(),
                    model_id: assistant.model.clone(),
                };
            }
            _ => {}
        }
    }
    configuration
}

fn derive_newest_own(entry: &Entry) -> Option<NewestOwnState> {
    if entry.entry_type() != EntryType::Message {
        return Some(NewestOwnState {
            entry_id: entry.id().to_owned(),
            entry_type: entry.entry_type(),
            role: None,
            stop_reason: None,
        });
    }
    let message = entry.body.message()?;
    let mut state = NewestOwnState {
        entry_id: entry.id().to_owned(),
        entry_type: entry.entry_type(),
        role: Some(message.role().to_owned()),
        stop_reason: None,
    };
    if let AgentMessage::Assistant(assistant) = message {
        state.stop_reason = Some(assistant.stop_reason);
    }
    Some(state)
}

fn deferred_handle(entry: &Entry) -> Option<DeferredHandle> {
    match &entry.body {
        EntryBody::Message {
            message: AgentMessage::Assistant(assistant),
            ..
        } if assistant.stop_reason == StopReason::Deferred => assistant.deferred.clone(),
        _ => None,
    }
}

fn derive_targets(intent: &OperationIntent, entries_by_id: &HashMap<&str, &Entry>) -> LaneTargets {
    let mut targets = LaneTargets::default();
    match intent {
        OperationIntent::Compaction { result_entry_id, .. } => {
            targets.result = Some(entries_by_id.contains_key(result_entry_id.as_str()));
        }
        OperationIntent::Navigation {
            summary_entry_id: Some(summary_entry_id),
            ..
        } => {
            targets.summary = Some(entries_by_id.contains_key(summary_entry_id.as_str()));
        }
        _ => {}
    }
    targets
}

fn derive_terminal_failure(
    newest_own_entry: Option<&Entry>,
    own_entries: &[&Entry],
    operation_records: &[&LaneRecord],
    deferred_write_ids: &HashSet<&str>,
) -> Option<TerminalFailureState> {
    let entry = newest_own_entry?;
    let EntryBody::Message {
        message: AgentMessage::Assistant(assistant),
        ..
    } = &entry.body
    else {
        return None;
    };
    if assistant.stop_reason != StopReason::Error || deferred_write_ids.contains(entry.id()) {
        return None;
    }
    let produced_by_step = operation_records.iter().any(|record| {
        matches!(
            &record.body,
            RecordBody::StepAttempt { result_entry_id, .. } if result_entry_id == entry.id()
        )
    });
    let produced_by_deferred_fetch = operation_records.iter().any(|record| {
        matches!(
            &record.body,
            RecordBody::Usage {
                cause: UsageCause::DeferredFetch,
                entry_id: Some(entry_id),
                ..
            } if entry_id == entry.id()
        )
    }) || own_entries
        .len()
        .checked_sub(2)
        .and_then(|index| own_entries.get(index))
        .is_some_and(|previous| {
            matches!(
                &previous.body,
                EntryBody::Message {
                    message: AgentMessage::Assistant(previous_assistant),
                    ..
                } if previous_assistant.stop_reason == StopReason::Deferred
            )
        });
    if !produced_by_step && !produced_by_deferred_fetch {
        return None;
    }
    Some(TerminalFailureState {
        entry_id: entry.id().to_owned(),
        source: if produced_by_step {
            TerminalFailureSource::Step
        } else {
            TerminalFailureSource::DeferredFetch
        },
        message: assistant.as_ref().clone(),
    })
}

fn derive_tool_batch(
    operation_id: &str,
    operation_records: &[&LaneRecord],
    own_entries: &[&Entry],
    entries_by_id: &HashMap<&str, &Entry>,
    deferred_write_ids: &HashSet<&str>,
) -> Option<ToolBatchState> {
    let assistant_entry = newest_tool_call_assistant(own_entries)?;
    let assistant_message = assistant_message_of(assistant_entry)?;
    let tool_calls = collect_tool_calls(assistant_message);
    let starts = collect_tool_starts(operation_records, operation_id, assistant_entry.id());
    let calls = tool_calls
        .iter()
        .enumerate()
        .map(|(tool_index, tool_call)| {
            derive_tool_batch_call(
                tool_index,
                tool_call,
                &starts,
                own_entries,
                assistant_entry,
                entries_by_id,
                deferred_write_ids,
            )
        })
        .collect::<Vec<_>>();

    Some(ToolBatchState {
        assistant_entry_id: assistant_entry.id().to_owned(),
        truncated: assistant_message.stop_reason == StopReason::Length,
        unresolved: calls.iter().any(|call| !call.result_exists),
        calls,
    })
}

fn newest_tool_call_assistant<'a>(own_entries: &[&'a Entry]) -> Option<&'a Entry> {
    own_entries.iter().copied().rev().find(|entry| {
        matches!(
            &entry.body,
            EntryBody::Message {
                message: AgentMessage::Assistant(assistant),
                ..
            } if assistant
                .content
                .iter()
                .any(|content| matches!(content, pi_ai::types::AssistantContent::ToolCall(_)))
        )
    })
}

fn assistant_message_of(entry: &Entry) -> Option<&AssistantMessage> {
    match &entry.body {
        EntryBody::Message {
            message: AgentMessage::Assistant(assistant),
            ..
        } => Some(assistant),
        _ => None,
    }
}

fn collect_tool_calls(assistant: &AssistantMessage) -> Vec<&ToolCall> {
    assistant
        .content
        .iter()
        .filter_map(|content| match content {
            pi_ai::types::AssistantContent::ToolCall(tool_call) => Some(tool_call),
            _ => None,
        })
        .collect()
}

fn collect_tool_starts<'a>(
    operation_records: &[&'a LaneRecord],
    operation_id: &str,
    assistant_entry_id: &str,
) -> HashMap<usize, &'a LaneRecord> {
    let mut starts = HashMap::new();
    for record in operation_records {
        if let RecordBody::ToolStarted {
            run_id,
            assistant_entry_id: started_for,
            tool_index,
            ..
        } = &record.body
            && run_id == operation_id
            && started_for == assistant_entry_id
            && let Some(index) = tool_index_ordinal(*tool_index)
        {
            starts.insert(index, *record);
        }
    }
    starts
}

fn derive_tool_batch_call(
    tool_index: usize,
    tool_call: &ToolCall,
    starts: &HashMap<usize, &LaneRecord>,
    own_entries: &[&Entry],
    assistant_entry: &Entry,
    entries_by_id: &HashMap<&str, &Entry>,
    deferred_write_ids: &HashSet<&str>,
) -> ToolBatchCall {
    let started = starts.get(&tool_index).copied();
    let started_result = started.and_then(|record| match &record.body {
        RecordBody::ToolStarted { result_entry_id, .. } => entries_by_id.get(result_entry_id.as_str()).copied(),
        _ => None,
    });
    let blocked_result = find_blocked_tool_result(own_entries, assistant_entry.seq, &tool_call.id, deferred_write_ids);
    let result = started_result.or(blocked_result);

    ToolBatchCall {
        tool_index,
        tool_call: tool_call.clone(),
        started: started.cloned(),
        result_exists: result.is_some(),
        terminate: result.and_then(terminated),
    }
}

fn find_blocked_tool_result<'a>(
    own_entries: &[&'a Entry],
    assistant_seq: i64,
    tool_call_id: &str,
    deferred_write_ids: &HashSet<&str>,
) -> Option<&'a Entry> {
    own_entries.iter().copied().find(|entry| {
        entry.seq > assistant_seq
            && !deferred_write_ids.contains(entry.id())
            && matches!(
                &entry.body,
                EntryBody::Message {
                    message: AgentMessage::ToolResult(result),
                    ..
                } if result.tool_call_id == tool_call_id
            )
    })
}

fn terminated(entry: &Entry) -> Option<bool> {
    match &entry.body {
        EntryBody::Message {
            terminate: Some(true), ..
        } => Some(true),
        _ => None,
    }
}

fn pending_queue(records: &[&LaneRecord], queue: QueueKind, operation_id: &str) -> Vec<EntryBody> {
    records
        .iter()
        .filter_map(|record| match &record.body {
            RecordBody::QueueEnqueued {
                queue: record_queue,
                run_id,
                target,
                ..
            } if *record_queue == queue && run_id.as_deref() == Some(operation_id) => Some(target.clone()),
            _ => None,
        })
        .collect()
}

fn step_literal(step: StepKind) -> &'static str {
    match step {
        StepKind::Assistant => "assistant",
        StepKind::BranchSummary => "branch_summary",
        StepKind::Compaction => "compaction",
    }
}

fn queue_literal(queue: QueueKind) -> &'static str {
    match queue {
        QueueKind::Steer => "steer",
        QueueKind::FollowUp => "followUp",
        QueueKind::NextRun => "nextRun",
    }
}

fn tool_index_ordinal(value: f64) -> Option<usize> {
    if value.is_finite() && value >= 0.0 && value.fract() == 0.0 && value <= usize::MAX as f64 {
        Some(value as usize)
    } else {
        None
    }
}
