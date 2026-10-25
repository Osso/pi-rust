//! Multi-agent persistence: lifecycle mutations, runtime ownership, outbox, counters.

use pi_js_compat::js_format;
use pi_js_compat::js_json::{JsObject as Map, JsValue as Value};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::mailbox::{PreparedStoredRuntimeMailboxMessage, prepare_stored_runtime_mailbox_message};
use super::{
    ControlDbError, EnqueueStoredRuntimeMailboxMessageInput, MultiAgentRuntimeOwner, MultiAgentRuntimeOwnerIdentity,
    MultiAgentRuntimeOwnership, MultiAgentRuntimeOwnershipIdentity, MultiAgentRuntimeOwnershipRow,
    RuntimeMailboxAddress, RuntimeMailboxListenerRow, RuntimeMailboxMessageKind, get_runtime_process_instance_id,
    parse_agent_snapshot, parse_process_identity, parse_stored_json_object_lossless, process_identity_is_alive,
    read_multi_agent_runtime_ownership_row, read_runtime_mailbox_listener_row, require_native_string_field,
    runtime_mailbox_agent_id_key, serialize_process_identity, trusted_runtime_mailbox_session_path,
    validate_mailbox_payload, validate_persisted_agent_payload, with_control_db, with_immediate_transaction,
};
use crate::core::multi_agent_store::{
    AgentLifecycle, AgentMailboxMessage, AgentSnapshot, MailboxMessageKindValue, MailboxMessageStatusValue,
    SteeringCheckpoint, is_active_lifecycle,
};
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue, encode_node_utf8};

const MULTI_AGENT_CREATION_MAX_ATTEMPTS: usize = 3;
const MULTI_AGENT_STEERING_MUTATION_MAX_ATTEMPTS: usize = 3;
const MULTI_AGENT_STEERING_DELIVERY_MAX_ATTEMPTS: usize = 3;
const MULTI_AGENT_DETACH_MARK_MAX_ATTEMPTS: usize = 3;
const MULTI_AGENT_LIFECYCLE_MUTATION_MAX_ATTEMPTS: usize = 3;
const MULTI_AGENT_TERMINAL_MUTATION_MAX_ATTEMPTS: usize = 3;
const MULTI_AGENT_METADATA_UPDATE_MAX_ATTEMPTS: usize = 3;
const MULTI_AGENT_MAILBOX_UPSERT_MAX_ATTEMPTS: usize = 3;
const ATTACHED_RUNTIME_OWNERSHIP_MAX_ATTEMPTS: usize = 3;
const STEERING_MESSAGE_ID_PLACEHOLDER: &str = "__pi_generated_steering_message_id__";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateChildError {
    AgentExists,
    ParentNotFound,
}

impl CreateChildError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentExists => "agent_exists",
            Self::ParentNotFound => "parent_not_found",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateAttachmentError {
    AgentExists,
    ParentNotFound,
    PermissionBroadened,
}

impl CreateAttachmentError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentExists => "agent_exists",
            Self::ParentNotFound => "parent_not_found",
            Self::PermissionBroadened => "permission_broadened",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcquireAttachedError {
    AgentNotFound,
    InvalidAgent,
    OwnershipHeld,
    MutationMismatch,
}

impl AcquireAttachedError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentNotFound => "agent_not_found",
            Self::InvalidAgent => "invalid_agent",
            Self::OwnershipHeld => "ownership_held",
            Self::MutationMismatch => "mutation_mismatch",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleCommandError {
    AgentNotFound,
    InvalidTransition,
    MutationMismatch,
}

impl LifecycleCommandError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentNotFound => "agent_not_found",
            Self::InvalidTransition => "invalid_transition",
            Self::MutationMismatch => "mutation_mismatch",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SteeringCommandError {
    AgentNotFound,
    InvalidTransition,
    MessageNotFound,
    MutationMismatch,
}

impl SteeringCommandError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentNotFound => "agent_not_found",
            Self::InvalidTransition => "invalid_transition",
            Self::MessageNotFound => "message_not_found",
            Self::MutationMismatch => "mutation_mismatch",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CreateFailedMultiAgentChildResult {
    Ok { agent: Box<AgentSnapshot> },
    Err { error: CreateChildError },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CreateMultiAgentChildWithRuntimeOwnershipResult {
    Ok {
        agent: Box<AgentSnapshot>,
        ownership: Box<MultiAgentRuntimeOwnership>,
    },
    Err {
        error: CreateChildError,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CreateMultiAgentAttachmentResult {
    Ok { agent: Box<AgentSnapshot> },
    Err { error: CreateAttachmentError },
}

#[derive(Debug, Clone, PartialEq)]
pub enum AcquireAttachedRuntimeOwnershipResult {
    Ok {
        agent: Box<AgentSnapshot>,
        ownership: Box<MultiAgentRuntimeOwnership>,
    },
    Err {
        error: AcquireAttachedError,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommitMultiAgentSteeringMutationResult {
    Ok {
        agent: Box<AgentSnapshot>,
        message: Box<AgentMailboxMessage>,
    },
    Err {
        error: SteeringCommandError,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommitMultiAgentSteeringDeliveryResult {
    Ok {
        agent: Box<AgentSnapshot>,
        message: Box<AgentMailboxMessage>,
    },
    Err {
        error: SteeringCommandError,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommitMultiAgentLifecycleMutationResult {
    Ok { agent: JsObject },
    Err { error: LifecycleCommandError },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitMultiAgentTerminalMutationResult {
    Ok { terminal_revision: i64 },
    Err { error: LifecycleCommandError },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommitMultiAgentDetachMarkResult {
    Ok { agent: Box<AgentSnapshot> },
    Err { error: LifecycleCommandError },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorRuntimeOwnership {
    pub agent_id: Option<JsString>,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub session_id: JsString,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcquireMultiAgentRuntimeOwnershipInput {
    pub session_path: JsString,
    pub agent_id: JsString,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub now_iso: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcquireAttachedRuntimeOwnershipInput {
    pub session_path: JsString,
    pub agent_id: JsString,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub now_iso: String,
    pub supervisor: SupervisorRuntimeOwnership,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateMultiAgentChildWithRuntimeOwnershipInput {
    pub session_path: JsString,
    pub agent_id: JsString,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub now_iso: String,
    pub agent: AgentSnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateMultiAgentAttachmentInput {
    pub agent: AgentSnapshot,
    pub agent_id: JsString,
    pub now_iso: String,
    pub session_path: JsString,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateFailedMultiAgentChildInput {
    pub agent: AgentSnapshot,
    pub now_iso: String,
    pub session_path: JsString,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitMultiAgentLifecycleMutationInput {
    pub session_path: JsString,
    pub agent_id: JsString,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub requested_lifecycle: String,
    pub updated_at: String,
    pub detached_cancellation: Option<DetachedCancellation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedCancellation {
    pub output_label: JsString,
    pub reason: Option<JsString>,
    pub suppress_terminal_notification: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommitMultiAgentSteeringMutationInput {
    pub session_path: JsString,
    pub agent_id: JsString,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub requested_lifecycle: String,
    pub updated_at: String,
    pub body: JsString,
    pub file_refs: Option<JsValue>,
    pub from_agent_id: JsString,
    pub recipient: RuntimeMailboxAddress,
    pub target_checkpoint: Option<SteeringCheckpoint>,
    pub thread_id: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitMultiAgentSteeringDeliveryInput {
    pub session_path: JsString,
    pub agent_id: JsString,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub requested_lifecycle: String,
    pub updated_at: String,
    pub message_id: JsString,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitMultiAgentDetachMarkInput {
    pub agent_id: JsString,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub session_path: JsString,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AgentDetails {
    pub error: Option<JsValue>,
    pub result: Option<JsValue>,
}

impl AgentDetails {
    fn to_map(&self) -> JsObject {
        let mut map = JsObject::new();
        if let Some(error) = &self.error {
            map.insert("error".into(), error.clone());
        }
        if let Some(result) = &self.result {
            map.insert("result".into(), result.clone());
        }
        map
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommitMultiAgentTerminalMutationInput {
    pub session_path: JsString,
    pub agent_id: JsString,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub terminal_lifecycle: String,
    pub event_kind: String,
    pub agent_details: AgentDetails,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalOutboxStatus {
    Claimed,
    Pending,
    Poisoned,
}

impl TerminalOutboxStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Claimed => "claimed",
            Self::Pending => "pending",
            Self::Poisoned => "poisoned",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiAgentTerminalOutboxRecord {
    pub session_path: JsString,
    pub agent_id: JsString,
    pub terminal_revision: i64,
    pub event_kind: String,
    pub status: TerminalOutboxStatus,
    pub claim_id: Option<JsString>,
    pub attempt_count: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaimMultiAgentTerminalOutboxOptions {
    pub max_attempts: Option<i64>,
    pub session_path: Option<JsString>,
    pub stale_claim_before: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoverDeadMultiAgentRuntimeInput {
    pub expected_owner: MultiAgentRuntimeOwnershipIdentity,
    pub now_iso: String,
    pub supervisor: SupervisorRuntimeOwnership,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecoverDeadMultiAgentRuntimeResult {
    Ok { agent: Map, terminal_revision: i64 },
    Err { error: RecoverDeadRuntimeError },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoverDeadRuntimeError {
    AgentNotFound,
    InvalidTransition,
    OwnerAlive,
    MutationMismatch,
}

impl RecoverDeadRuntimeError {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AgentNotFound => "agent_not_found",
            Self::InvalidTransition => "invalid_transition",
            Self::OwnerAlive => "owner_alive",
            Self::MutationMismatch => "mutation_mismatch",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetachedArtifactAgentRecord {
    pub agent: AgentSnapshot,
    pub session_path: JsString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiAgentCounters {
    pub next_agent_number: i64,
    pub next_message_number: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiAgentPersistedState {
    pub agents: Vec<JsValue>,
    pub mailbox_messages: Vec<JsValue>,
    pub counters: MultiAgentCounters,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MultiAgentCounterName {
    Agent,
    Message,
}

impl MultiAgentCounterName {
    fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Message => "message",
        }
    }
}

struct ClaimedTerminalOutboxRow {
    agent_id: JsString,
    attempt_count: i64,
    event_kind: String,
    session_path: JsString,
    terminal_revision: i64,
}

/// `claimMultiAgentTerminalOutbox`.
pub fn claim_multi_agent_terminal_outbox(
    control_db_path: impl Into<JsString>,
    claim_id: &JsString,
    now_iso: &str,
    options: &ClaimMultiAgentTerminalOutboxOptions,
) -> Result<Option<MultiAgentTerminalOutboxRecord>, ControlDbError> {
    let control_db_path = control_db_path.into();
    let max_attempts = options.max_attempts.unwrap_or(5);
    with_control_db(&control_db_path, |db| {
        if !has_eligible_terminal_outbox_work(db, max_attempts, options)? {
            return Ok(None);
        }
        let session_filter = options.session_path.as_ref().map(encode_node_utf8);
        let row = with_immediate_transaction(db, |db| {
            if let Some(stale_claim_before) = &options.stale_claim_before {
                recover_stale_terminal_outbox_claims(
                    db,
                    max_attempts,
                    now_iso,
                    session_filter.as_deref(),
                    stale_claim_before,
                )?;
            }
            claim_next_terminal_outbox_row(db, claim_id, now_iso, max_attempts, session_filter.as_deref())
        })?;
        Ok(row.map(|row| MultiAgentTerminalOutboxRecord {
            agent_id: row.agent_id,
            attempt_count: row.attempt_count,
            claim_id: Some(claim_id.to_owned()),
            event_kind: row.event_kind,
            session_path: row.session_path,
            status: TerminalOutboxStatus::Claimed,
            terminal_revision: row.terminal_revision,
        }))
    })
}

fn has_eligible_terminal_outbox_work(
    db: &Connection,
    max_attempts: i64,
    options: &ClaimMultiAgentTerminalOutboxOptions,
) -> Result<bool, ControlDbError> {
    Ok(db
        .query_row(
            "SELECT 1 FROM multi_agent_terminal_outbox
				 WHERE (? IS NULL OR session_path = ?)
				 AND (
					(status = 'pending' AND attempt_count < ?)
					OR (? IS NOT NULL AND status = 'claimed' AND claimed_at < ?)
				 )
				 LIMIT 1",
            params![
                options.session_path,
                options.session_path,
                max_attempts,
                options.stale_claim_before,
                options.stale_claim_before
            ],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn recover_stale_terminal_outbox_claims(
    db: &Connection,
    max_attempts: i64,
    now_iso: &str,
    session_path: Option<&str>,
    stale_claim_before: &str,
) -> Result<(), ControlDbError> {
    db.execute(
        "UPDATE multi_agent_terminal_outbox
		 SET status = CASE WHEN attempt_count >= ? THEN 'poisoned' ELSE 'pending' END,
		 claim_id = NULL, claimed_at = NULL, last_error = 'claim lease expired', updated_at = ?
		 WHERE status = 'claimed' AND claimed_at < ? AND (? IS NULL OR session_path = ?)",
        params![max_attempts, now_iso, stale_claim_before, session_path, session_path],
    )?;
    Ok(())
}

fn claim_next_terminal_outbox_row(
    db: &Connection,
    claim_id: &JsString,
    now_iso: &str,
    max_attempts: i64,
    session_path: Option<&str>,
) -> Result<Option<ClaimedTerminalOutboxRow>, ControlDbError> {
    Ok(db
        .query_row(
            "UPDATE multi_agent_terminal_outbox
			 SET status = 'claimed', claim_id = ?, claimed_at = ?,
			 attempt_count = attempt_count + 1, updated_at = ?
			 WHERE rowid = (
				SELECT rowid FROM multi_agent_terminal_outbox
				WHERE status = 'pending' AND attempt_count < ? AND (? IS NULL OR session_path = ?)
				ORDER BY updated_at LIMIT 1
			 )
			 AND status = 'pending'
			 RETURNING session_path, agent_id, terminal_revision, event_kind, attempt_count",
            params![claim_id, now_iso, now_iso, max_attempts, session_path, session_path],
            |row| {
                Ok(ClaimedTerminalOutboxRow {
                    session_path: row.get(0)?,
                    agent_id: row.get(1)?,
                    terminal_revision: row.get(2)?,
                    event_kind: row.get(3)?,
                    attempt_count: row.get(4)?,
                })
            },
        )
        .optional()?)
}

/// `failMultiAgentTerminalOutbox`.
pub fn fail_multi_agent_terminal_outbox(
    control_db_path: impl Into<JsString>,
    record: &MultiAgentTerminalOutboxRecord,
    error: &JsString,
    now_iso: &str,
    max_attempts: i64,
) -> Result<bool, ControlDbError> {
    let control_db_path = control_db_path.into();
    let status = if record.attempt_count >= max_attempts {
        TerminalOutboxStatus::Poisoned
    } else {
        TerminalOutboxStatus::Pending
    };
    update_claimed_terminal_outbox(&control_db_path, record, status, now_iso, Some(error))
}

/// `deliverMultiAgentTerminalOutbox`.
pub fn deliver_multi_agent_terminal_outbox(
    control_db_path: impl Into<JsString>,
    record: &MultiAgentTerminalOutboxRecord,
) -> Result<bool, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        Ok(db.execute("DELETE FROM multi_agent_terminal_outbox WHERE session_path = ? AND agent_id = ? AND terminal_revision = ? AND event_kind = ? AND status = 'claimed' AND claim_id = ?",params![record.session_path,record.agent_id,record.terminal_revision,record.event_kind,record.claim_id])?==1)
    })
}

fn update_claimed_terminal_outbox(
    control_db_path: &JsString,
    record: &MultiAgentTerminalOutboxRecord,
    status: TerminalOutboxStatus,
    now_iso: &str,
    error: Option<&JsString>,
) -> Result<bool, ControlDbError> {
    with_control_db(control_db_path, |db| {
        Ok(db.execute(
            "UPDATE multi_agent_terminal_outbox SET status = ?, claim_id = NULL, claimed_at = NULL, last_error = ?, updated_at = ? WHERE session_path = ? AND agent_id = ? AND terminal_revision = ? AND event_kind = ? AND status = 'claimed' AND claim_id = ?",
            params![
                status.as_str(),
                error,
                now_iso,
                record.session_path,
                record.agent_id,
                record.terminal_revision,
                record.event_kind,
                record.claim_id
            ],
        )? == 1)
    })
}

/// `cleanupMultiAgentTerminalOutbox`.
pub fn cleanup_multi_agent_terminal_outbox(
    control_db_path: impl Into<JsString>,
    older_than: &str,
) -> Result<i64, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        Ok(db.execute(
            "DELETE FROM multi_agent_terminal_outbox
					 WHERE status = 'poisoned' AND updated_at < ?",
            params![older_than],
        )? as i64)
    })
}

// ---------------------------------------------------------------------------
// Steering
// ---------------------------------------------------------------------------

struct PreparedSteeringMailboxMessage {
    data_prefix: String,
    data_suffix: String,
    updated_at: String,
}

struct SteeringMutationPlan {
    agent_data: String,
    mailbox: PreparedSteeringMailboxMessage,
    process_identity_data: String,
    recipient_listener: RuntimeMailboxListenerRow,
    sender_listener: RuntimeMailboxListenerRow,
    updated_agent: AgentSnapshot,
    updated_agent_data: String,
}

enum SteeringMutationPreflight {
    Plan(Box<SteeringMutationPlan>),
    Result(Box<CommitMultiAgentSteeringMutationResult>),
}

/// `commitMultiAgentSteeringMutation`.
pub fn commit_multi_agent_steering_mutation(
    control_db_path: impl Into<JsString>,
    input: &CommitMultiAgentSteeringMutationInput,
) -> Result<CommitMultiAgentSteeringMutationResult, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        for _ in 0..MULTI_AGENT_STEERING_MUTATION_MAX_ATTEMPTS {
            let preflight = prepare_multi_agent_steering_mutation(db, input)?;
            let plan = match preflight {
                SteeringMutationPreflight::Result(result) => return Ok(*result),
                SteeringMutationPreflight::Plan(plan) => plan,
            };
            if let Some(result) = persist_multi_agent_steering_mutation(db, input, &plan)? {
                return Ok(result);
            }
        }
        Ok(CommitMultiAgentSteeringMutationResult::Err {
            error: SteeringCommandError::MutationMismatch,
        })
    })
}

fn prepare_multi_agent_steering_mutation(
    db: &Connection,
    input: &CommitMultiAgentSteeringMutationInput,
) -> Result<SteeringMutationPreflight, ControlDbError> {
    let Some((agent, agent_data)) = read_steering_mutation_agent(db, input)? else {
        return Ok(SteeringMutationPreflight::Result(Box::new(
            CommitMultiAgentSteeringMutationResult::Err {
                error: SteeringCommandError::AgentNotFound,
            },
        )));
    };
    if input.requested_lifecycle != "steering_pending"
        || !can_persist_lifecycle_transition(&agent.lifecycle, &input.requested_lifecycle)
    {
        return Ok(SteeringMutationPreflight::Result(Box::new(
            CommitMultiAgentSteeringMutationResult::Err {
                error: SteeringCommandError::InvalidTransition,
            },
        )));
    }
    let ownership = read_multi_agent_runtime_ownership_row(db, &input.session_path, &input.agent_id)?;
    let sender_listener = read_runtime_mailbox_listener_row(
        db,
        &RuntimeMailboxAddress {
            agent_id: input.owner.agent_id.clone().map(Into::into),
            session_id: input.owner.session_id.clone().into(),
        },
    )?;
    let recipient_listener = read_runtime_mailbox_listener_row(db, &input.recipient)?;
    let (Some(sender_listener), Some(recipient_listener)) = (sender_listener, recipient_listener) else {
        return Ok(SteeringMutationPreflight::Result(Box::new(
            CommitMultiAgentSteeringMutationResult::Err {
                error: SteeringCommandError::MutationMismatch,
            },
        )));
    };
    if !steering_mutation_has_authority(&agent, ownership.as_ref(), &sender_listener, &recipient_listener, input)? {
        return Ok(SteeringMutationPreflight::Result(Box::new(
            CommitMultiAgentSteeringMutationResult::Err {
                error: SteeringCommandError::MutationMismatch,
            },
        )));
    }
    let mut updated_agent = agent.clone();
    updated_agent.lifecycle = AgentLifecycle::SteeringPending;
    updated_agent.revision += 1;
    updated_agent.updated_at = input.updated_at.clone().into();
    Ok(SteeringMutationPreflight::Plan(Box::new(SteeringMutationPlan {
        agent_data,
        mailbox: prepare_steering_mailbox_message(input)?,
        process_identity_data: serialize_process_identity(&input.process_identity),
        recipient_listener,
        sender_listener,
        updated_agent_data: super::serialize_agent(&updated_agent)?,
        updated_agent,
    })))
}

fn read_steering_mutation_agent(
    db: &Connection,
    input: &CommitMultiAgentSteeringMutationInput,
) -> Result<Option<(AgentSnapshot, String)>, ControlDbError> {
    let data = read_agent_data_row(db, &input.session_path, &input.agent_id)?;
    let Some(data) = data else {
        return Ok(None);
    };
    let context = js_format!("multi_agent_agents:{}#{}", input.session_path, input.agent_id);

    let agent = parse_agent_snapshot(&data, &context)?;
    Ok(Some((agent, data)))
}

fn steering_mutation_has_authority(
    agent: &AgentSnapshot,
    ownership: Option<&MultiAgentRuntimeOwnershipRow>,
    sender_listener: &RuntimeMailboxListenerRow,
    recipient_listener: &RuntimeMailboxListenerRow,
    input: &CommitMultiAgentSteeringMutationInput,
) -> Result<bool, ControlDbError> {
    let identity = ownership_identity(
        &input.session_path,
        &input.agent_id,
        &input.process_identity,
        &input.owner,
    );
    if !super::runtime_owner_matches(ownership, &identity) {
        return Ok(false);
    }
    let sender_path_matches = input.owner.agent_id.is_some()
        || trusted_runtime_mailbox_session_path(sender_listener).is_some_and(|path| input.session_path == path);
    let expected_sender_id = input.owner.agent_id.clone().unwrap_or_else(|| "supervisor".into());
    Ok(input.from_agent_id == expected_sender_id
        && agent.parent_id == Some(input.owner.agent_id.clone().unwrap_or_else(|| "main".into()))
        && input.recipient.agent_id.as_ref() == Some(&JsString::from(&input.agent_id))
        && agent
            .transcript
            .as_ref()
            .map(|transcript| JsString::from(&transcript.session_id))
            == Some(input.recipient.session_id.clone())
        && sender_listener.pid == std::process::id()
        && sender_listener.runtime_instance_id.as_deref() == Some(get_runtime_process_instance_id()?)
        && sender_path_matches
        && runtime_listener_matches_process_identity(
            Some(recipient_listener),
            ownership.and_then(|ownership| ownership.process_identity.as_deref()),
        ))
}

fn runtime_listener_matches_process_identity(
    listener: Option<&RuntimeMailboxListenerRow>,
    serialized_identity: Option<&str>,
) -> bool {
    let (Some(listener), Some(serialized_identity)) = (listener, serialized_identity) else {
        return false;
    };
    let Some(listener_instance_id) = listener.runtime_instance_id.as_deref() else {
        return false;
    };
    let Ok(listener_identity) = parse_process_identity(listener_instance_id) else {
        return false;
    };
    let Ok(process_identity) = parse_process_identity(serialized_identity) else {
        return false;
    };
    listener.pid == process_identity.pid
        && listener_identity.pid == process_identity.pid
        && listener_identity.start_time_ticks == process_identity.start_time_ticks
        && process_identity_is_alive(&process_identity)
}

fn persist_multi_agent_steering_mutation(
    db: &Connection,
    input: &CommitMultiAgentSteeringMutationInput,
    plan: &SteeringMutationPlan,
) -> Result<Option<CommitMultiAgentSteeringMutationResult>, ControlDbError> {
    let message_number = with_immediate_transaction(db, |db| {
        if !compare_and_persist_steering_agent(db, input, plan)? {
            return Ok(None);
        }
        let allocated = allocate_multi_agent_counter_with_db(db, &input.session_path, MultiAgentCounterName::Message)?;
        let message_id = JsString::from(format!("message_{allocated}"));
        persist_prepared_steering_mailbox_message(db, &input.session_path, &message_id, &plan.mailbox)?;
        Ok(Some(allocated))
    })?;
    let Some(message_number) = message_number else {
        return Ok(None);
    };
    Ok(Some(CommitMultiAgentSteeringMutationResult::Ok {
        agent: Box::new(plan.updated_agent.clone()),
        message: Box::new(build_steering_mailbox_message(
            input,
            &JsString::from(format!("message_{message_number}")),
        )?),
    }))
}

fn compare_and_persist_steering_agent(
    db: &Connection,
    input: &CommitMultiAgentSteeringMutationInput,
    plan: &SteeringMutationPlan,
) -> Result<bool, ControlDbError> {
    Ok(db.execute(
        "UPDATE multi_agent_agents SET data = ?, updated_at = ?
				 WHERE session_path = ? AND agent_id = ? AND data = ?
				 AND EXISTS (
					SELECT 1 FROM multi_agent_runtime_owners
					WHERE session_path = ? AND agent_id = ? AND process_identity = ?
					AND owner_session_id = ? AND owner_agent_id IS ?
				 )
				 AND EXISTS (
					SELECT 1 FROM runtime_mailbox_listeners
					WHERE recipient_session_id = ? AND recipient_agent_id_key = ? AND pid = ?
					AND runtime_instance_id IS ? AND session_path IS ?
					AND session_path_asserted_at IS ? AND updated_at = ?
				 )
				 AND EXISTS (
					SELECT 1 FROM runtime_mailbox_listeners
					WHERE recipient_session_id = ? AND recipient_agent_id_key = ? AND pid = ?
					AND runtime_instance_id IS ? AND session_path IS ?
					AND session_path_asserted_at IS ? AND updated_at = ?
				 )",
        params![
            plan.updated_agent_data,
            input.updated_at,
            input.session_path,
            input.agent_id,
            plan.agent_data,
            input.session_path,
            input.agent_id,
            plan.process_identity_data,
            input.owner.session_id,
            input.owner.agent_id,
            input.owner.session_id,
            runtime_mailbox_agent_id_key(input.owner.agent_id.as_ref()),
            plan.sender_listener.pid,
            plan.sender_listener.runtime_instance_id,
            plan.sender_listener.session_path,
            plan.sender_listener.session_path_asserted_at,
            plan.sender_listener.updated_at,
            input.recipient.session_id,
            runtime_mailbox_agent_id_key(input.recipient.agent_id.as_ref()),
            plan.recipient_listener.pid,
            plan.recipient_listener.runtime_instance_id,
            plan.recipient_listener.session_path,
            plan.recipient_listener.session_path_asserted_at,
            plan.recipient_listener.updated_at
        ],
    )? == 1)
}

fn build_steering_mailbox_message(
    input: &CommitMultiAgentSteeringMutationInput,
    message_id: &JsString,
) -> Result<AgentMailboxMessage, ControlDbError> {
    Ok(AgentMailboxMessage {
        extra: js_json::JsObject::new(),
        body: Some(input.body.clone().into()),
        created_at: input.updated_at.clone().into(),
        file_refs: input.file_refs.clone(),
        from_agent_id: input.from_agent_id.clone().into(),
        id: message_id.to_owned().into(),
        kind: MailboxMessageKindValue::Steer,
        status: MailboxMessageStatusValue::Pending,
        target_checkpoint: input.target_checkpoint,
        thread_id: input.thread_id.clone(),
        to_agent_id: input.agent_id.clone().into(),
        updated_at: Some(input.updated_at.clone().into()),
        error: None,
    })
}

fn prepare_steering_mailbox_message(
    input: &CommitMultiAgentSteeringMutationInput,
) -> Result<PreparedSteeringMailboxMessage, ControlDbError> {
    let message = build_steering_mailbox_message(input, &JsString::from(STEERING_MESSAGE_ID_PLACEHOLDER))?;
    let message_value = js_json::to_value(&message).map_err(|error| ControlDbError::new(error.to_string()))?;
    let mailbox_input = EnqueueStoredRuntimeMailboxMessageInput {
        recipient_host: None,
        kind: RuntimeMailboxMessageKind::Steer,
        message: message_value.into(),
        recipient: input.recipient.clone(),
        sender: RuntimeMailboxAddress {
            agent_id: input.owner.agent_id.clone().map(Into::into),
            session_id: input.owner.session_id.clone().into(),
        },
        store_ref: super::RuntimeMailboxStoreRef {
            message_id: message.id.clone(),
            session_path: input.session_path.clone().into(),
        },
        updated_at: Some(input.updated_at.clone()),
    };
    let serialized = prepare_stored_runtime_mailbox_message(&mailbox_input)?.serialized;
    let serialized_id = js_json::stringify_lossless(&Value::String(STEERING_MESSAGE_ID_PLACEHOLDER.into()));
    let id_property = format!("\"id\":{serialized_id}");
    let id_property_index = serialized.find(&id_property).ok_or_else(|| {
        ControlDbError::new("Prepared steering mailbox payload has an invalid generated message ID field")
    })?;
    if serialized[id_property_index + id_property.len()..].contains(&id_property) {
        return Err(ControlDbError::new(
            "Prepared steering mailbox payload has an invalid generated message ID field",
        ));
    }
    let serialized_id_index = id_property_index + "\"id\":".len();
    Ok(PreparedSteeringMailboxMessage {
        data_prefix: serialized[..serialized_id_index].to_owned(),
        data_suffix: serialized[serialized_id_index + serialized_id.len()..].to_owned(),
        updated_at: input.updated_at.clone(),
    })
}

fn persist_prepared_steering_mailbox_message(
    db: &Connection,
    session_path: &JsString,
    message_id: &JsString,
    prepared: &PreparedSteeringMailboxMessage,
) -> Result<(), ControlDbError> {
    db.execute(
        "INSERT INTO multi_agent_mailbox_messages (session_path, message_id, data, updated_at)
		 VALUES (?, ?, ? || json_quote(?) || ?, ?)",
        params![
            session_path,
            message_id,
            prepared.data_prefix,
            message_id,
            prepared.data_suffix,
            prepared.updated_at
        ],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Detach mark
// ---------------------------------------------------------------------------

/// `commitMultiAgentDetachMark`.
pub fn commit_multi_agent_detach_mark(
    control_db_path: impl Into<JsString>,
    input: &CommitMultiAgentDetachMarkInput,
) -> Result<CommitMultiAgentDetachMarkResult, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        for _ in 0..MULTI_AGENT_DETACH_MARK_MAX_ATTEMPTS {
            let data = read_agent_data_row(db, &input.session_path, &input.agent_id)?;
            let Some(data) = data else {
                return Ok(CommitMultiAgentDetachMarkResult::Err {
                    error: LifecycleCommandError::AgentNotFound,
                });
            };
            let context = js_format!("multi_agent_agents:{}#{}", input.session_path, input.agent_id);
            let agent = parse_stored_json_object_lossless(&data, &context)?;
            let mut agent_snapshot = parse_agent_snapshot(&data, &context)?;
            let ownership = read_multi_agent_runtime_ownership_row(db, &input.session_path, &input.agent_id)?;
            let identity = ownership_identity(
                &input.session_path,
                &input.agent_id,
                &input.process_identity,
                &input.owner,
            );
            if !super::runtime_owner_matches(ownership.as_ref(), &identity) {
                return Ok(CommitMultiAgentDetachMarkResult::Err {
                    error: LifecycleCommandError::MutationMismatch,
                });
            }
            if !is_recoverable_runtime_lifecycle(&agent_snapshot.lifecycle) {
                return Ok(CommitMultiAgentDetachMarkResult::Err {
                    error: LifecycleCommandError::InvalidTransition,
                });
            }
            if agent_snapshot.detached == Some(true) {
                return Ok(CommitMultiAgentDetachMarkResult::Ok {
                    agent: Box::new(agent_snapshot),
                });
            }
            agent_snapshot.detached = Some(true);
            agent_snapshot.revision += 1;
            agent_snapshot.updated_at = input.updated_at.clone().into();
            validate_persisted_agent_payload(&agent, &context)?;
            let updated_data = super::serialize_agent(&agent_snapshot)?;
            let persisted = db.execute(
                "UPDATE multi_agent_agents SET data = ?, updated_at = ?
				 WHERE session_path = ? AND agent_id = ? AND data = ?
				 AND EXISTS (
					SELECT 1 FROM multi_agent_runtime_owners
					WHERE session_path = ? AND agent_id = ? AND process_identity = ?
					AND owner_session_id = ? AND owner_agent_id IS ?
				 )",
                params![
                    updated_data,
                    input.updated_at,
                    input.session_path,
                    input.agent_id,
                    data,
                    input.session_path,
                    input.agent_id,
                    serialize_process_identity(&input.process_identity),
                    input.owner.session_id,
                    input.owner.agent_id
                ],
            )? == 1;
            if persisted {
                return Ok(CommitMultiAgentDetachMarkResult::Ok {
                    agent: Box::new(agent_snapshot),
                });
            }
        }
        Ok(CommitMultiAgentDetachMarkResult::Err {
            error: LifecycleCommandError::MutationMismatch,
        })
    })
}

// ---------------------------------------------------------------------------
// Steering delivery
// ---------------------------------------------------------------------------

pub(super) struct SteeringDeliveryPlan {
    agent_data: String,
    pub(super) message_data: String,
    process_identity_data: String,
    updated_agent: AgentSnapshot,
    updated_agent_data: String,
    updated_message: AgentMailboxMessage,
}

/// `commitMultiAgentSteeringDelivery`.
pub fn commit_multi_agent_steering_delivery(
    control_db_path: impl Into<JsString>,
    input: &CommitMultiAgentSteeringDeliveryInput,
) -> Result<CommitMultiAgentSteeringDeliveryResult, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        for _ in 0..MULTI_AGENT_STEERING_DELIVERY_MAX_ATTEMPTS {
            let plan = match read_steering_delivery_plan(db, input)? {
                Ok(plan) => plan,
                Err(error) => return Ok(CommitMultiAgentSteeringDeliveryResult::Err { error }),
            };
            let persisted = persist_steering_delivery(db, input, &plan, None)?;
            if persisted {
                return Ok(CommitMultiAgentSteeringDeliveryResult::Ok {
                    agent: Box::new(plan.updated_agent),
                    message: Box::new(plan.updated_message),
                });
            }
        }
        Ok(CommitMultiAgentSteeringDeliveryResult::Err {
            error: SteeringCommandError::MutationMismatch,
        })
    })
}

pub(super) fn read_steering_delivery_plan(
    db: &Connection,
    input: &CommitMultiAgentSteeringDeliveryInput,
) -> Result<Result<SteeringDeliveryPlan, SteeringCommandError>, ControlDbError> {
    let agent_row = read_agent_data_row(db, &input.session_path, &input.agent_id)?;
    let Some(agent_data) = agent_row else {
        return Ok(Err(SteeringCommandError::AgentNotFound));
    };
    let agent_context = js_format!("multi_agent_agents:{}#{}", input.session_path, input.agent_id);
    let agent = parse_agent_snapshot(&agent_data, &agent_context)?;
    let ownership = read_multi_agent_runtime_ownership_row(db, &input.session_path, &input.agent_id)?;
    let identity = ownership_identity(
        &input.session_path,
        &input.agent_id,
        &input.process_identity,
        &input.owner,
    );
    if !super::runtime_owner_matches(ownership.as_ref(), &identity) {
        return Ok(Err(SteeringCommandError::MutationMismatch));
    }
    if !can_persist_lifecycle_transition(&agent.lifecycle, &input.requested_lifecycle) {
        return Ok(Err(SteeringCommandError::InvalidTransition));
    }
    let message_data: Option<String> = db
        .query_row(
            &format!(
                "SELECT data FROM multi_agent_mailbox_messages WHERE session_path = ? AND message_id = ? AND {} > ?",
                super::retention::MAILBOX_CREATED_AT_MS_SQL
            ),
            params![
                input.session_path,
                input.message_id,
                super::retention::mailbox_cutoff_ms()
            ],
            |row| row.get(0),
        )
        .optional()?;
    let Some(message_data) = message_data else {
        return Ok(Err(SteeringCommandError::MessageNotFound));
    };
    let message_context = js_format!(
        "multi_agent_mailbox_messages:{}#{}",
        input.session_path,
        input.message_id
    );
    let message = parse_stored_json_object_lossless(&message_data, &message_context)?;
    validate_mailbox_payload(&Value::Object(message.clone()), &message_context)?;
    let is_steering_message = message
        .get("kind")
        .and_then(Value::as_string)
        .is_some_and(|kind| kind == "steer");
    let addressed_to_agent = message
        .get("toAgentId")
        .and_then(Value::as_string)
        .is_some_and(|to_agent_id| &input.agent_id == to_agent_id);
    if !is_steering_message || !addressed_to_agent {
        return Ok(Err(SteeringCommandError::MessageNotFound));
    }
    let mut updated_agent_object = super::parse_stored_json_object_lossless(&agent_data, &agent_context)?;
    updated_agent_object.insert(
        "lifecycle".into(),
        JsValue::String(input.requested_lifecycle.clone().into()),
    );
    updated_agent_object.insert("revision".into(), JsValue::Number((agent.revision + 1) as f64));
    updated_agent_object.insert("updatedAt".into(), JsValue::String(input.updated_at.clone().into()));
    let updated_agent: AgentSnapshot = js_json::from_value(&JsValue::Object(updated_agent_object))
        .map_err(|error| ControlDbError::new(error.to_string()))?;
    let updated_message: AgentMailboxMessage = js_json::from_value(
        &js_json::parse_lossless(&message_data).map_err(|error| ControlDbError::new(error.to_string()))?,
    )
    .map_err(|error| ControlDbError::new(error.to_string()))?;
    Ok(Ok(SteeringDeliveryPlan {
        agent_data,
        message_data,
        process_identity_data: serialize_process_identity(&input.process_identity),
        updated_agent_data: super::serialize_agent(&updated_agent)?,
        updated_agent,
        updated_message,
    }))
}

pub(super) fn persist_steering_delivery(
    db: &Connection,
    input: &CommitMultiAgentSteeringDeliveryInput,
    plan: &SteeringDeliveryPlan,
    mailbox: Option<(&RuntimeMailboxAddress, &super::mailbox::RuntimeMailboxCandidate)>,
) -> Result<bool, ControlDbError> {
    with_immediate_transaction(db, |db| {
        if super::is_mailbox_message_expired(
            Some(&plan.updated_message.created_at),
            pi_js_compat::date::js_now_millis(),
        ) {
            return Ok(false);
        }
        if let Some((recipient, candidate)) = mailbox
            && !super::mailbox::candidate_authority_is_current(db, recipient, candidate)?
        {
            return Ok(false);
        }
        let agent_updated = db.execute(
            "UPDATE multi_agent_agents SET data = ?, updated_at = ?
		 WHERE session_path = ? AND agent_id = ? AND data = ?
		 AND EXISTS (
			SELECT 1 FROM multi_agent_runtime_owners
			WHERE session_path = ? AND agent_id = ? AND process_identity = ?
			AND owner_session_id = ? AND owner_agent_id IS ?
		 )
		 AND EXISTS (
			SELECT 1 FROM multi_agent_mailbox_messages
			WHERE session_path = ? AND message_id = ? AND data = ?
		 )",
            params![
                plan.updated_agent_data,
                input.updated_at,
                input.session_path,
                input.agent_id,
                plan.agent_data,
                input.session_path,
                input.agent_id,
                plan.process_identity_data,
                input.owner.session_id,
                input.owner.agent_id,
                input.session_path,
                input.message_id,
                plan.message_data
            ],
        )?;
        if agent_updated != 1 {
            return Ok(false);
        }
        let message_updated = if let Some((_, candidate)) = mailbox {
            let process_identity = super::get_runtime_process_instance_id()?;
            let serialized = super::serialize_canonical_mailbox_mutation(&candidate.row, |claimed| {
                claimed.insert(JsString::from("status"), JsValue::String(JsString::from("claimed")));
                for key in ["claimedAt", "updatedAt"] {
                    claimed.insert(
                        JsString::from(key),
                        JsValue::String(JsString::from(input.updated_at.as_str())),
                    );
                }
                claimed.insert(
                    JsString::from("claimantProcessIdentity"),
                    JsValue::String(JsString::from(process_identity)),
                );
            })?;
            usize::from(super::compare_and_write_serialized_canonical_mailbox_payload(
                db,
                &candidate.row,
                &serialized,
                &input.updated_at,
            )?)
        } else {
            db.execute(
                "DELETE FROM multi_agent_mailbox_messages WHERE session_path = ? AND message_id = ? AND data = ?",
                params![input.session_path, input.message_id, plan.message_data],
            )?
        };
        if message_updated != 1 {
            return Err(ControlDbError::new(js_format!(
                "Steering message changed during delivery {}#{}",
                input.session_path,
                input.message_id
            )));
        }
        Ok(true)
    })
}

// ---------------------------------------------------------------------------
// Lifecycle mutation
// ---------------------------------------------------------------------------

/// `commitMultiAgentLifecycleMutation`.
pub fn commit_multi_agent_lifecycle_mutation(
    control_db_path: impl Into<JsString>,
    input: &CommitMultiAgentLifecycleMutationInput,
) -> Result<CommitMultiAgentLifecycleMutationResult, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        for _ in 0..MULTI_AGENT_LIFECYCLE_MUTATION_MAX_ATTEMPTS {
            let plan = match read_lifecycle_mutation_plan(db, input)? {
                LifecycleMutationRead::Plan(plan) => plan,
                LifecycleMutationRead::Unchanged(agent) => {
                    return Ok(CommitMultiAgentLifecycleMutationResult::Ok { agent });
                }
                LifecycleMutationRead::Err(error) => {
                    return Ok(CommitMultiAgentLifecycleMutationResult::Err { error });
                }
            };
            let persisted = with_immediate_transaction(db, |db| persist_lifecycle_mutation(db, input, &plan))?;
            if persisted {
                return Ok(CommitMultiAgentLifecycleMutationResult::Ok { agent: plan.updated });
            }
        }
        Ok(CommitMultiAgentLifecycleMutationResult::Err {
            error: LifecycleCommandError::MutationMismatch,
        })
    })
}

struct LifecycleMutationPlan {
    data: String,
    updated: JsObject,
    updated_data: String,
    cancellation: Option<(JsString, String)>,
    process_identity_data: String,
}

enum LifecycleMutationRead {
    Plan(LifecycleMutationPlan),
    Unchanged(JsObject),
    Err(LifecycleCommandError),
}

fn read_lifecycle_mutation_plan(
    db: &Connection,
    input: &CommitMultiAgentLifecycleMutationInput,
) -> Result<LifecycleMutationRead, ControlDbError> {
    let data = read_agent_data_row(db, &input.session_path, &input.agent_id)?;
    let Some(data) = data else {
        return Ok(LifecycleMutationRead::Err(LifecycleCommandError::AgentNotFound));
    };
    let agent = parse_stored_json_object_lossless(
        &data,
        &js_format!("multi_agent_agents:{}#{}", input.session_path, input.agent_id),
    )?;
    let ownership = read_multi_agent_runtime_ownership_row(db, &input.session_path, &input.agent_id)?;
    let identity = ownership_identity(
        &input.session_path,
        &input.agent_id,
        &input.process_identity,
        &input.owner,
    );
    if !super::runtime_owner_matches(ownership.as_ref(), &identity) {
        return Ok(LifecycleMutationRead::Err(LifecycleCommandError::MutationMismatch));
    }
    let current_lifecycle = agent
        .get("lifecycle")
        .and_then(Value::as_string)
        .cloned()
        .unwrap_or_default();
    let lifecycle_changed = current_lifecycle != input.requested_lifecycle;
    let persist_suppression = input
        .detached_cancellation
        .as_ref()
        .and_then(|cancellation| cancellation.suppress_terminal_notification)
        == Some(true)
        && agent.get("suppressTerminalNotification").and_then(Value::as_bool) != Some(true);
    if !lifecycle_changed && !persist_suppression {
        return Ok(LifecycleMutationRead::Unchanged(agent.clone()));
    }
    if lifecycle_changed
        && !current_lifecycle
            .to_utf8()
            .ok()
            .is_some_and(|current| can_persist_lifecycle_transition_str(&current, &input.requested_lifecycle))
    {
        return Ok(LifecycleMutationRead::Err(LifecycleCommandError::InvalidTransition));
    }
    let revision = agent
        .get("revision")
        .and_then(Value::as_f64)
        .map(|value| value as i64)
        .unwrap_or(0)
        + 1;
    let mut updated = agent.clone();
    if persist_suppression {
        updated.insert("suppressTerminalNotification".into(), JsValue::Bool(true));
    }
    updated.insert(
        "lifecycle".into(),
        JsValue::String(input.requested_lifecycle.clone().into()),
    );
    updated.insert("revision".into(), JsValue::Number(revision as f64));
    updated.insert("updatedAt".into(), JsValue::String(input.updated_at.clone().into()));
    let cancellation = input
        .detached_cancellation
        .as_ref()
        .filter(|_| lifecycle_changed)
        .map(|cancellation| build_detached_cancellation_message(input, cancellation, revision))
        .transpose()?;
    let updated_data = js_json::stringify_lossless(&JsValue::Object(updated.clone()));
    Ok(LifecycleMutationRead::Plan(LifecycleMutationPlan {
        data,
        updated,
        updated_data,
        cancellation,
        process_identity_data: serialize_process_identity(&input.process_identity),
    }))
}

fn persist_lifecycle_mutation(
    db: &Connection,
    input: &CommitMultiAgentLifecycleMutationInput,
    plan: &LifecycleMutationPlan,
) -> Result<bool, ControlDbError> {
    let changed = db.execute(
        "UPDATE multi_agent_agents SET data = ?, updated_at = ?
		 WHERE session_path = ? AND agent_id = ? AND data = ?
		 AND EXISTS (
			SELECT 1 FROM multi_agent_runtime_owners
			WHERE session_path = ? AND agent_id = ? AND process_identity = ?
			AND owner_session_id = ? AND owner_agent_id IS ?
		 )",
        params![
            plan.updated_data,
            input.updated_at,
            input.session_path,
            input.agent_id,
            plan.data,
            input.session_path,
            input.agent_id,
            plan.process_identity_data,
            input.owner.session_id,
            input.owner.agent_id
        ],
    )?;
    if changed != 1 {
        return Ok(false);
    }
    if let Some((message_id, message)) = &plan.cancellation {
        let birth = JsObject::from_iter([("createdAt".into(), JsValue::String(input.updated_at.clone().into()))]);
        super::retention::assert_mailbox_message_fresh(
            &birth,
            &js_format!("multi_agent_mailbox_messages:{}#{}", input.session_path, message_id),
        )?;
        db.execute(
            "INSERT INTO multi_agent_mailbox_messages (session_path, message_id, data, updated_at)
		 VALUES (?, ?, ?, ?)",
            params![input.session_path, message_id, message, input.updated_at],
        )?;
    }
    Ok(true)
}

fn build_detached_cancellation_message(
    input: &CommitMultiAgentLifecycleMutationInput,
    cancellation: &DetachedCancellation,
    cancelling_revision: i64,
) -> Result<(JsString, String), ControlDbError> {
    let message_id = js_format!("detached-cancel:{}:{}", input.agent_id, cancelling_revision.to_string());
    let process_identity =
        js_json::to_value(&input.process_identity).map_err(|error| ControlDbError::new(error.to_string()))?;
    let mut body = pi_js_compat::js_json!({
        "command": "cancel",
        "identity": {
            "jobId": input.agent_id,
            "outputLabel": cancellation.output_label,
            "owner": {
                "sessionId": input.owner.session_id,
                "agentId": input.owner.agent_id,
            },
            "processIdentity": process_identity,
        },
    });
    if let Some(reason) = &cancellation.reason {
        body.as_object_mut()
            .expect("cancel body object")
            .insert("reason".into(), JsValue::String(reason.clone()));
    }
    let body = js_json::stringify_lossless(&body);
    let message = js_json::stringify_lossless(&pi_js_compat::js_json!({
        "body": body,
        "createdAt": input.updated_at,
        "fromAgentId": "main",
        "id": message_id,
        "kind": "system",
        "recipientAgentId": input.agent_id,
        "recipientSessionId": input.owner.session_id,
        "senderAgentId": null,
        "senderSessionId": input.owner.session_id,
        "status": "pending",
        "toAgentId": input.agent_id,
    }));
    Ok((message_id, message))
}

fn can_persist_lifecycle_transition(current: &AgentLifecycle, requested: &str) -> bool {
    can_persist_lifecycle_transition_str(lifecycle_name(*current), requested)
}

pub(crate) fn can_persist_lifecycle_transition_str(current: &str, requested: &str) -> bool {
    let transitions: [(&str, &[&str]); 7] = [
        (
            "running",
            &[
                "waiting_for_input",
                "steering_pending",
                "cancelling",
                "completed",
                "failed",
                "aborted",
            ],
        ),
        (
            "waiting_for_input",
            &["running", "steering_pending", "cancelling", "completed", "aborted"],
        ),
        ("steering_pending", &["running", "cancelling", "failed", "aborted"]),
        ("cancelling", &["aborted", "failed"]),
        ("completed", &[]),
        ("failed", &[]),
        ("aborted", &[]),
    ];
    if current == requested {
        return !matches!(current, "completed" | "failed" | "aborted");
    }
    transitions
        .iter()
        .find(|(lifecycle, _)| *lifecycle == current)
        .is_some_and(|(_, allowed)| allowed.contains(&requested))
}

fn lifecycle_name(lifecycle: AgentLifecycle) -> &'static str {
    crate::core::multi_agent_store::lifecycle_name(lifecycle)
}

pub(crate) fn is_recoverable_runtime_lifecycle(lifecycle: &AgentLifecycle) -> bool {
    matches!(
        lifecycle,
        AgentLifecycle::WaitingForInput
            | AgentLifecycle::Running
            | AgentLifecycle::SteeringPending
            | AgentLifecycle::Cancelling
    )
}

// ---------------------------------------------------------------------------
// Terminal mutation
// ---------------------------------------------------------------------------

/// `commitMultiAgentTerminalMutation`.
pub fn commit_multi_agent_terminal_mutation(
    control_db_path: impl Into<JsString>,
    input: &CommitMultiAgentTerminalMutationInput,
) -> Result<CommitMultiAgentTerminalMutationResult, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        for _ in 0..MULTI_AGENT_TERMINAL_MUTATION_MAX_ATTEMPTS {
            let plan = match read_terminal_mutation_plan(db, input)? {
                TerminalMutationRead::Plan(plan) => plan,
                TerminalMutationRead::Replay(result) => return Ok(result),
                TerminalMutationRead::Err(error) => {
                    return Ok(CommitMultiAgentTerminalMutationResult::Err { error });
                }
            };
            let persisted = with_immediate_transaction(db, |db| persist_terminal_mutation(db, input, &plan))?;
            if persisted {
                return Ok(CommitMultiAgentTerminalMutationResult::Ok {
                    terminal_revision: plan.terminal_revision,
                });
            }
        }
        Ok(CommitMultiAgentTerminalMutationResult::Err {
            error: LifecycleCommandError::MutationMismatch,
        })
    })
}

struct TerminalMutationPlan {
    agent_data: String,
    terminal_revision: i64,
    updated_data: String,
    process_identity_data: String,
}

enum TerminalMutationRead {
    Plan(TerminalMutationPlan),
    Replay(CommitMultiAgentTerminalMutationResult),
    Err(LifecycleCommandError),
}

fn read_terminal_mutation_plan(
    db: &Connection,
    input: &CommitMultiAgentTerminalMutationInput,
) -> Result<TerminalMutationRead, ControlDbError> {
    let agent_row = read_agent_data_row(db, &input.session_path, &input.agent_id)?;
    let Some(agent_data) = agent_row else {
        return Ok(TerminalMutationRead::Err(LifecycleCommandError::AgentNotFound));
    };
    let agent = parse_stored_json_object_lossless(
        &agent_data,
        &js_format!("multi_agent_agents:{}#{}", input.session_path, input.agent_id),
    )?;
    let ownership = read_multi_agent_runtime_ownership_row(db, &input.session_path, &input.agent_id)?;
    let identity = ownership_identity(
        &input.session_path,
        &input.agent_id,
        &input.process_identity,
        &input.owner,
    );
    if !super::runtime_owner_matches(ownership.as_ref(), &identity) {
        return Ok(TerminalMutationRead::Err(LifecycleCommandError::MutationMismatch));
    }
    let current_lifecycle = agent
        .get("lifecycle")
        .and_then(Value::as_string)
        .cloned()
        .unwrap_or_default();
    if current_lifecycle == input.terminal_lifecycle {
        return Ok(TerminalMutationRead::Replay(terminal_mutation_replay_result(
            input, &agent,
        )));
    }
    let terminal_revision = agent
        .get("revision")
        .and_then(Value::as_f64)
        .map(|value| value as i64)
        .unwrap_or(0)
        + 1;
    if !current_lifecycle
        .to_utf8()
        .ok()
        .is_some_and(|current| can_persist_terminal_transition(&current, &input.terminal_lifecycle))
    {
        return Ok(TerminalMutationRead::Err(LifecycleCommandError::InvalidTransition));
    }
    if has_active_persisted_descendant(db, &input.session_path, &input.agent_id)? {
        return Ok(TerminalMutationRead::Err(LifecycleCommandError::InvalidTransition));
    }
    let mut updated = agent.clone();
    updated.extend(input.agent_details.to_map());
    updated.shift_remove("currentActivity");
    updated.insert(
        "lifecycle".into(),
        JsValue::String(input.terminal_lifecycle.clone().into()),
    );
    updated.insert("revision".into(), JsValue::Number(terminal_revision as f64));
    updated.insert("updatedAt".into(), JsValue::String(input.updated_at.clone().into()));
    Ok(TerminalMutationRead::Plan(TerminalMutationPlan {
        agent_data,
        terminal_revision,
        updated_data: js_json::stringify_lossless(&JsValue::Object(updated)),
        process_identity_data: serialize_process_identity(&input.process_identity),
    }))
}

fn persist_terminal_mutation(
    db: &Connection,
    input: &CommitMultiAgentTerminalMutationInput,
    plan: &TerminalMutationPlan,
) -> Result<bool, ControlDbError> {
    if !update_terminal_agent_if_current(
        db,
        &TerminalAgentCasInput {
            agent_data: &plan.agent_data,
            agent_id: &input.agent_id,
            owner: &input.owner,
            process_identity_data: &plan.process_identity_data,
            require_unique_ownership: false,
            session_path: &encode_node_utf8(&input.session_path),
            updated_agent_data: &plan.updated_data,
            updated_at: &input.updated_at,
        },
    )? {
        return Ok(false);
    }
    db.execute(
        "INSERT INTO multi_agent_terminal_outbox (
				session_path, agent_id, terminal_revision, event_kind, status,
				claim_id, claimed_at, delivered_at, attempt_count, last_error, updated_at
			) VALUES (?, ?, ?, ?, 'pending', NULL, NULL, NULL, 0, NULL, ?)",
        params![
            input.session_path,
            input.agent_id,
            plan.terminal_revision,
            input.event_kind,
            input.updated_at
        ],
    )?;
    Ok(true)
}

pub(crate) struct TerminalAgentCasInput<'a> {
    pub agent_data: &'a str,
    pub agent_id: &'a JsString,
    pub owner: &'a MultiAgentRuntimeOwnerIdentity,
    pub process_identity_data: &'a str,
    pub require_unique_ownership: bool,
    pub session_path: &'a str,
    pub updated_agent_data: &'a str,
    pub updated_at: &'a str,
}

pub(crate) fn update_terminal_agent_if_current(
    db: &Connection,
    input: &TerminalAgentCasInput<'_>,
) -> Result<bool, ControlDbError> {
    Ok(db.execute(
        "WITH RECURSIVE descendants(agent_id) AS (
					SELECT agent_id FROM multi_agent_agents
					WHERE session_path = ? AND json_valid(data)
					AND json_extract(data, '$.parentId') = ?
					UNION
					SELECT child.agent_id FROM multi_agent_agents AS child
					JOIN descendants AS parent
						ON json_valid(child.data) AND json_extract(child.data, '$.parentId') = parent.agent_id
					WHERE child.session_path = ?
				)
				UPDATE multi_agent_agents SET data = ?, updated_at = ?
				WHERE session_path = ? AND agent_id = ? AND data = ?
				AND EXISTS (
					SELECT 1 FROM multi_agent_runtime_owners
					WHERE session_path = ? AND agent_id = ? AND process_identity = ?
					AND owner_session_id = ? AND owner_agent_id IS ?
				)
				AND (? = 0 OR (
					SELECT COUNT(*) FROM multi_agent_runtime_owners AS matching_owner
					WHERE matching_owner.agent_id = ? AND matching_owner.process_identity = ?
					AND matching_owner.owner_session_id = ? AND matching_owner.owner_agent_id IS ?
				) = 1)
				AND NOT EXISTS (
					SELECT 1 FROM multi_agent_agents
					WHERE session_path = ? AND NOT json_valid(data)
				)
				AND NOT EXISTS (
					SELECT 1 FROM multi_agent_agents AS active
					JOIN descendants ON descendants.agent_id = active.agent_id
					WHERE active.session_path = ?
					AND json_extract(active.data, '$.lifecycle') NOT IN ('completed', 'failed', 'aborted')
				)",
        params![
            input.session_path,
            input.agent_id,
            input.session_path,
            input.updated_agent_data,
            input.updated_at,
            input.session_path,
            input.agent_id,
            input.agent_data,
            input.session_path,
            input.agent_id,
            input.process_identity_data,
            input.owner.session_id,
            input.owner.agent_id,
            i64::from(input.require_unique_ownership),
            input.agent_id,
            input.process_identity_data,
            input.owner.session_id,
            input.owner.agent_id,
            input.session_path,
            input.session_path
        ],
    )? == 1)
}

pub(crate) fn has_active_persisted_descendant(
    db: &Connection,
    session_path: impl Into<JsString>,
    ancestor_id: &JsString,
) -> Result<bool, ControlDbError> {
    let session_path: &JsString = &session_path.into();
    let mut statement = db.prepare("SELECT agent_id, data FROM multi_agent_agents WHERE session_path = ?")?;
    let rows = statement
        .query_map(params![session_path], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut agents = std::collections::HashMap::new();
    for (agent_id, data) in rows {
        let agent =
            parse_stored_json_object_lossless(&data, &js_format!("multi_agent_agents:{}#{}", session_path, agent_id))?;
        agents.insert(JsString::from(agent_id), agent);
    }
    for (agent_id, agent) in &agents {
        if !is_nonterminal_lifecycle(agent.get("lifecycle")) {
            continue;
        }
        let mut parent_id = agent.get("parentId").and_then(Value::as_string).cloned();
        let mut visited = std::collections::HashSet::from([agent_id.clone()]);
        while let Some(current_parent) = parent_id {
            if !visited.insert(current_parent.clone()) {
                break;
            }
            if *ancestor_id == current_parent {
                return Ok(true);
            }
            parent_id = agents
                .get(&current_parent)
                .and_then(|parent| parent.get("parentId"))
                .and_then(Value::as_string)
                .cloned();
        }
    }
    Ok(false)
}

fn is_nonterminal_lifecycle(lifecycle: Option<&Value>) -> bool {
    lifecycle
        .and_then(Value::as_string)
        .is_some_and(|lifecycle| lifecycle != "completed" && lifecycle != "failed" && lifecycle != "aborted")
}

fn terminal_mutation_replay_result(
    input: &CommitMultiAgentTerminalMutationInput,
    agent: &JsObject,
) -> CommitMultiAgentTerminalMutationResult {
    let terminal_revision = agent.get("revision").and_then(JsValue::as_f64).unwrap_or(0.0) as i64;
    for (key, value) in input.agent_details.to_map() {
        if agent.get(&key).map(js_json::stringify_lossless) != Some(js_json::stringify_lossless(&value)) {
            return CommitMultiAgentTerminalMutationResult::Err {
                error: LifecycleCommandError::MutationMismatch,
            };
        }
    }
    CommitMultiAgentTerminalMutationResult::Ok { terminal_revision }
}

fn can_persist_terminal_transition(current: &str, requested: &str) -> bool {
    let allowed: &[&str] = if requested == "completed" {
        &["running", "waiting_for_input"]
    } else {
        &["running", "waiting_for_input", "steering_pending", "cancelling"]
    };
    allowed.contains(&current)
}

// ---------------------------------------------------------------------------
// Child creation
// ---------------------------------------------------------------------------

struct ActiveParentSnapshot {
    agent: Option<AgentSnapshot>,
    data: Option<String>,
    parent_id: JsString,
}

/// `createFailedMultiAgentChild`.
pub fn create_failed_multi_agent_child(
    control_db_path: impl Into<JsString>,
    input: &CreateFailedMultiAgentChildInput,
) -> Result<CreateFailedMultiAgentChildResult, ControlDbError> {
    let control_db_path = control_db_path.into();
    let context = js_format!("multi_agent_agents:{}#{}", input.session_path, input.agent.id);
    let agent_value = super::agent_value(&input.agent)?;
    let Value::Object(agent_object) = &agent_value else {
        return Err(ControlDbError::new(js_format!(
            "Invalid persisted agent payload at {}",
            context
        )));
    };
    validate_persisted_agent_payload(agent_object, &context)?;
    if input.agent.lifecycle != AgentLifecycle::Failed || input.agent.revision != 1 {
        return Err(ControlDbError::new("Failed child creation requires failed revision 1"));
    }
    let Some(parent_id) = input.agent.parent_id.clone().filter(|parent| !parent.is_empty()) else {
        return Ok(CreateFailedMultiAgentChildResult::Err {
            error: CreateChildError::ParentNotFound,
        });
    };
    let serialized_agent = js_json::stringify_lossless(&agent_value);
    with_control_db(&control_db_path, |db| {
        for _ in 0..MULTI_AGENT_CREATION_MAX_ATTEMPTS {
            let Some(parent) = read_active_parent_snapshot(db, &input.session_path, &parent_id)? else {
                return Ok(CreateFailedMultiAgentChildResult::Err {
                    error: CreateChildError::ParentNotFound,
                });
            };
            if multi_agent_agent_exists(db, &input.session_path, &input.agent.id)? {
                return Ok(CreateFailedMultiAgentChildResult::Err {
                    error: CreateChildError::AgentExists,
                });
            }
            let inserted =
                with_immediate_transaction(db, |db| insert_failed_child(db, input, &serialized_agent, &parent))?;
            if inserted {
                return Ok(CreateFailedMultiAgentChildResult::Ok {
                    agent: Box::new(input.agent.clone()),
                });
            }
        }
        Err(ControlDbError::new(js_format!(
            "Failed child parent changed repeatedly {}#{}",
            input.session_path,
            input.agent.id
        )))
    })
}

fn read_active_parent_snapshot(
    db: &Connection,
    session_path: &JsString,
    parent_id: &JsString,
) -> Result<Option<ActiveParentSnapshot>, ControlDbError> {
    if parent_id == "main" {
        return Ok(Some(ActiveParentSnapshot {
            agent: None,
            data: None,
            parent_id: parent_id.to_owned(),
        }));
    }
    let data = read_agent_data_row(db, session_path, parent_id)?;
    let Some(data) = data else {
        return Ok(None);
    };
    let context = js_format!("multi_agent_agents:{}#{}", session_path, parent_id);
    let parent = parse_stored_json_object_lossless(&data, &context)?;
    validate_persisted_agent_payload(&parent, &context)?;
    if !is_nonterminal_lifecycle(parent.get("lifecycle")) {
        return Ok(None);
    }
    let agent = parse_agent_snapshot(&data, &context)?;
    Ok(Some(ActiveParentSnapshot {
        agent: Some(agent),
        data: Some(data),
        parent_id: parent_id.to_owned(),
    }))
}

fn ownership_identity(
    session_path: &JsString,
    agent_id: &JsString,
    process_identity: &crate::core::runtime_process::ProcessIdentity,
    owner: &MultiAgentRuntimeOwnerIdentity,
) -> MultiAgentRuntimeOwnershipIdentity {
    MultiAgentRuntimeOwnershipIdentity {
        session_path: session_path.to_owned(),
        agent_id: agent_id.to_owned(),
        process_identity: process_identity.clone(),
        owner: owner.clone(),
    }
}

fn read_agent_data_row(
    db: &Connection,
    session_path: &JsString,
    agent_id: impl Into<JsString>,
) -> Result<Option<String>, ControlDbError> {
    Ok(db
        .query_row(
            "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![session_path, encode_node_utf8(&agent_id.into())],
            |row| row.get(0),
        )
        .optional()?)
}

fn multi_agent_agent_exists(
    db: &Connection,
    session_path: &JsString,
    agent_id: &JsString,
) -> Result<bool, ControlDbError> {
    Ok(db
        .query_row(
            "SELECT 1 FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![session_path, agent_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn insert_multi_agent_agent_if_parent_current(
    db: &Connection,
    session_path: &JsString,
    agent_id: &JsString,
    serialized_agent: &str,
    updated_at: &str,
    parent: &ActiveParentSnapshot,
) -> Result<bool, ControlDbError> {
    Ok(db.execute(
        "INSERT INTO multi_agent_agents (session_path, agent_id, data, updated_at)
				 SELECT ?, ?, ?, ?
				 WHERE (? = 'main' OR EXISTS (
					SELECT 1 FROM multi_agent_agents
					WHERE session_path = ? AND agent_id = ? AND data = ?
				 ))
				 ON CONFLICT(session_path, agent_id) DO NOTHING",
        params![
            session_path,
            agent_id,
            serialized_agent,
            updated_at,
            parent.parent_id,
            session_path,
            parent.parent_id,
            parent.data
        ],
    )? == 1)
}

fn insert_failed_child(
    db: &Connection,
    input: &CreateFailedMultiAgentChildInput,
    serialized_agent: &str,
    parent: &ActiveParentSnapshot,
) -> Result<bool, ControlDbError> {
    let inserted = insert_multi_agent_agent_if_parent_current(
        db,
        &input.session_path,
        &input.agent.id,
        serialized_agent,
        &input.now_iso,
        parent,
    )?;
    if !inserted {
        return Ok(false);
    }
    db.execute(
        "INSERT INTO multi_agent_terminal_outbox (
				session_path, agent_id, terminal_revision, event_kind, status,
				claim_id, claimed_at, delivered_at, attempt_count, last_error, updated_at
			) VALUES (?, ?, 1, 'failed', 'pending', NULL, NULL, NULL, 0, NULL, ?)",
        params![input.session_path, input.agent.id, input.now_iso],
    )?;
    Ok(true)
}

/// `createMultiAgentChildWithRuntimeOwnership`.
pub fn create_multi_agent_child_with_runtime_ownership(
    control_db_path: impl Into<JsString>,
    input: &CreateMultiAgentChildWithRuntimeOwnershipInput,
) -> Result<CreateMultiAgentChildWithRuntimeOwnershipResult, ControlDbError> {
    let control_db_path = control_db_path.into();
    let context = js_format!("multi_agent_agents:{}#{}", input.session_path, input.agent_id);
    let agent_value = super::agent_value(&input.agent)?;
    let Value::Object(agent_object) = &agent_value else {
        return Err(ControlDbError::new(js_format!(
            "Invalid persisted agent payload at {}",
            context
        )));
    };
    validate_persisted_agent_payload(agent_object, &context)?;
    if input.agent.id != input.agent_id {
        return Err(ControlDbError::new(
            "Child agent payload ID does not match runtime ownership identity",
        ));
    }
    if input.agent.lifecycle != AgentLifecycle::Running || input.agent.revision != 1 {
        return Err(ControlDbError::new(
            "Child runtime ownership requires running revision 1",
        ));
    }
    let Some(parent_id) = input.agent.parent_id.clone().filter(|parent| !parent.is_empty()) else {
        return Ok(CreateMultiAgentChildWithRuntimeOwnershipResult::Err {
            error: CreateChildError::ParentNotFound,
        });
    };
    let process_identity_data = serialize_process_identity(&input.process_identity);
    let serialized_agent = js_json::stringify_lossless(&agent_value);
    with_control_db(&control_db_path, |db| {
        for _ in 0..MULTI_AGENT_CREATION_MAX_ATTEMPTS {
            let Some(parent) = read_active_parent_snapshot(db, &input.session_path, &parent_id)? else {
                return Ok(CreateMultiAgentChildWithRuntimeOwnershipResult::Err {
                    error: CreateChildError::ParentNotFound,
                });
            };
            if multi_agent_agent_exists(db, &input.session_path, &input.agent_id)? {
                return Ok(CreateMultiAgentChildWithRuntimeOwnershipResult::Err {
                    error: CreateChildError::AgentExists,
                });
            }
            let inserted = with_immediate_transaction(db, |db| {
                insert_child_with_ownership(db, input, &serialized_agent, &process_identity_data, &parent)
            })?;
            if inserted {
                return Ok(CreateMultiAgentChildWithRuntimeOwnershipResult::Ok {
                    agent: Box::new(input.agent.clone()),
                    ownership: Box::new(ownership_from_identity_parts(
                        &input.session_path,
                        &input.agent_id,
                        &input.owner,
                        &input.process_identity,
                    )),
                });
            }
        }
        Err(ControlDbError::new(js_format!(
            "Child parent changed repeatedly {}#{}",
            input.session_path,
            input.agent_id
        )))
    })
}

fn insert_child_with_ownership(
    db: &Connection,
    input: &CreateMultiAgentChildWithRuntimeOwnershipInput,
    serialized_agent: &str,
    process_identity_data: &str,
    parent: &ActiveParentSnapshot,
) -> Result<bool, ControlDbError> {
    let inserted = insert_multi_agent_agent_if_parent_current(
        db,
        &input.session_path,
        &input.agent_id,
        serialized_agent,
        &input.now_iso,
        parent,
    )?;
    if !inserted {
        return Ok(false);
    }
    persist_acquired_runtime_ownership(
        db,
        &input.session_path,
        &input.agent_id,
        process_identity_data,
        &input.owner,
    )?;
    Ok(true)
}

/// `createMultiAgentAttachment`.
pub fn create_multi_agent_attachment(
    control_db_path: impl Into<JsString>,
    input: &CreateMultiAgentAttachmentInput,
) -> Result<CreateMultiAgentAttachmentResult, ControlDbError> {
    let control_db_path = control_db_path.into();
    let context = js_format!("multi_agent_agents:{}#{}", input.session_path, input.agent_id);
    let agent_value = super::agent_value(&input.agent)?;
    let Value::Object(agent_object) = &agent_value else {
        return Err(ControlDbError::new(js_format!(
            "Invalid persisted agent payload at {}",
            context
        )));
    };
    validate_persisted_agent_payload(agent_object, &context)?;
    if input.agent.id != input.agent_id {
        return Err(ControlDbError::new(
            "Attached agent payload ID does not match command identity",
        ));
    }
    if input.agent.origin != Some(crate::core::multi_agent_store::AgentOrigin::Attached)
        || input.agent.lifecycle != AgentLifecycle::WaitingForInput
        || input.agent.revision != 1
    {
        return Err(ControlDbError::new(
            "Attached agent creation requires waiting_for_input revision 1",
        ));
    }
    let serialized_agent = js_json::stringify_lossless(&agent_value);
    with_control_db(&control_db_path, |db| {
        for _ in 0..MULTI_AGENT_CREATION_MAX_ATTEMPTS {
            if multi_agent_agent_exists(db, &input.session_path, &input.agent_id)? {
                return Ok(CreateMultiAgentAttachmentResult::Err {
                    error: CreateAttachmentError::AgentExists,
                });
            }
            let parent = match read_attachment_parent_snapshot(db, &input.session_path, &input.agent) {
                Ok(parent) => parent,
                Err(error) => return Ok(CreateMultiAgentAttachmentResult::Err { error }),
            };
            if insert_multi_agent_agent_if_parent_current(
                db,
                &input.session_path,
                &input.agent_id,
                &serialized_agent,
                &input.now_iso,
                &parent,
            )? {
                return Ok(CreateMultiAgentAttachmentResult::Ok {
                    agent: Box::new(input.agent.clone()),
                });
            }
        }
        Err(ControlDbError::new(js_format!(
            "Attachment parent changed repeatedly {}#{}",
            input.session_path,
            input.agent_id
        )))
    })
}

fn read_attachment_parent_snapshot(
    db: &Connection,
    session_path: &JsString,
    agent: &AgentSnapshot,
) -> Result<ActiveParentSnapshot, CreateAttachmentError> {
    let Some(parent_id) = agent.parent_id.clone().filter(|parent| !parent.is_empty()) else {
        return Ok(ActiveParentSnapshot {
            agent: None,
            data: None,
            parent_id: "main".into(),
        });
    };
    if parent_id == "main" {
        return Ok(ActiveParentSnapshot {
            agent: None,
            data: None,
            parent_id,
        });
    }
    let parent =
        read_active_parent_snapshot(db, session_path, &parent_id).map_err(|_| CreateAttachmentError::ParentNotFound)?;
    let Some(parent) = parent else {
        return Err(CreateAttachmentError::ParentNotFound);
    };
    let Some(parent_agent) = &parent.agent else {
        return Err(CreateAttachmentError::ParentNotFound);
    };
    let permission_matches = agent.permission.narrowed && agent.permission.policy == parent_agent.permission.policy;
    if permission_matches {
        Ok(parent)
    } else {
        Err(CreateAttachmentError::PermissionBroadened)
    }
}

/// `readMultiAgentRuntimeOwnership`.
pub fn read_multi_agent_runtime_ownership(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    agent_id: &JsString,
) -> Result<Option<MultiAgentRuntimeOwnership>, ControlDbError> {
    let session_path: &JsString = &session_path.into();
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        read_multi_agent_runtime_ownership_row(db, session_path, agent_id)?
            .map(|row| super::multi_agent_runtime_ownership_from_row(&row))
            .transpose()
    })
}

// ---------------------------------------------------------------------------
// Attached runtime acquisition
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct SupervisorSessionAuthoritySnapshot {
    runtime_instance_id: String,
    session_path: Option<String>,
    session_path_asserted_at: Option<String>,
    supervisor_agent_data: Option<String>,
}

enum AttachedRuntimeAgentRead {
    Ok { agent: Box<AgentSnapshot>, data: String },
    Err(AcquireAttachedError),
}

/// `acquireAttachedRuntimeOwnership`.
pub fn acquire_attached_runtime_ownership(
    control_db_path: impl Into<JsString>,
    input: &AcquireAttachedRuntimeOwnershipInput,
) -> Result<AcquireAttachedRuntimeOwnershipResult, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        for _ in 0..ATTACHED_RUNTIME_OWNERSHIP_MAX_ATTEMPTS {
            let Some(supervisor_authority) =
                read_registered_supervisor_session_authority(db, &input.session_path, &input.supervisor)?
            else {
                return Ok(AcquireAttachedRuntimeOwnershipResult::Err {
                    error: AcquireAttachedError::MutationMismatch,
                });
            };
            let (agent, data) = match read_attached_runtime_agent(db, &input.session_path, &input.agent_id)? {
                AttachedRuntimeAgentRead::Ok { agent, data } => (*agent, data),
                AttachedRuntimeAgentRead::Err(error) => {
                    return Ok(AcquireAttachedRuntimeOwnershipResult::Err { error });
                }
            };
            let ownership = read_multi_agent_runtime_ownership_row(db, &input.session_path, &input.agent_id)?;
            match decide_ownership_acquisition(ownership.as_ref(), input)? {
                OwnershipAcquisition::AlreadyOwned(current) => {
                    return Ok(AcquireAttachedRuntimeOwnershipResult::Ok {
                        agent: Box::new(agent),
                        ownership: Box::new(current),
                    });
                }
                OwnershipAcquisition::Held => {
                    return Ok(AcquireAttachedRuntimeOwnershipResult::Err {
                        error: AcquireAttachedError::OwnershipHeld,
                    });
                }
                OwnershipAcquisition::Acquire => {}
            }
            let mut updated_agent = agent.clone();
            updated_agent.revision += 1;
            updated_agent.updated_at = input.now_iso.clone().into();
            let updated_agent_data = super::serialize_agent(&updated_agent)?;
            let process_identity_data = serialize_process_identity(&input.process_identity);
            let persisted = with_immediate_transaction(db, |db| {
                let commit = AcquiredOwnershipCommit {
                    supervisor_authority: &supervisor_authority,
                    expected_agent_data: &data,
                    expected_ownership: ownership.as_ref(),
                    updated_agent: &updated_agent,
                    updated_agent_data: &updated_agent_data,
                    process_identity_data: &process_identity_data,
                };
                commit_acquired_runtime_ownership(db, input, &commit)
            })?;
            if let Some(result) = persisted {
                return Ok(result);
            }
        }
        Ok(AcquireAttachedRuntimeOwnershipResult::Err {
            error: AcquireAttachedError::MutationMismatch,
        })
    })
}

enum OwnershipAcquisition {
    AlreadyOwned(MultiAgentRuntimeOwnership),
    Held,
    Acquire,
}

fn decide_ownership_acquisition(
    ownership: Option<&MultiAgentRuntimeOwnershipRow>,
    input: &AcquireAttachedRuntimeOwnershipInput,
) -> Result<OwnershipAcquisition, ControlDbError> {
    let current_identity = ownership
        .and_then(|ownership| ownership.process_identity.as_deref())
        .and_then(|value| parse_process_identity(value).ok());
    let current_owner_alive = current_identity.as_ref().is_some_and(process_identity_is_alive);
    let identity = ownership_identity(
        &input.session_path,
        &input.agent_id,
        &input.process_identity,
        &input.owner,
    );
    if let Some(current_ownership) = ownership
        && current_owner_alive
        && super::runtime_owner_matches(Some(current_ownership), &identity)
    {
        return Ok(OwnershipAcquisition::AlreadyOwned(
            super::multi_agent_runtime_ownership_from_row(current_ownership)?,
        ));
    }
    let replaces_current_incarnation = current_identity.as_ref().is_some_and(|current| {
        crate::core::runtime_process::is_superseded_runtime_identity(current, &input.process_identity)
    });
    if current_owner_alive && !replaces_current_incarnation {
        return Ok(OwnershipAcquisition::Held);
    }
    Ok(OwnershipAcquisition::Acquire)
}

struct AcquiredOwnershipCommit<'a> {
    supervisor_authority: &'a SupervisorSessionAuthoritySnapshot,
    expected_agent_data: &'a str,
    expected_ownership: Option<&'a MultiAgentRuntimeOwnershipRow>,
    updated_agent: &'a AgentSnapshot,
    updated_agent_data: &'a str,
    process_identity_data: &'a str,
}

fn commit_acquired_runtime_ownership(
    db: &Connection,
    input: &AcquireAttachedRuntimeOwnershipInput,
    commit: &AcquiredOwnershipCommit<'_>,
) -> Result<Option<AcquireAttachedRuntimeOwnershipResult>, ControlDbError> {
    if !supervisor_session_authority_is_current(
        db,
        &input.session_path,
        &input.supervisor,
        commit.supervisor_authority,
    )? {
        return Ok(Some(AcquireAttachedRuntimeOwnershipResult::Err {
            error: AcquireAttachedError::MutationMismatch,
        }));
    }
    let row = read_agent_data_row(db, &input.session_path, &input.agent_id)?;
    let Some(row) = row else {
        return Ok(Some(AcquireAttachedRuntimeOwnershipResult::Err {
            error: AcquireAttachedError::AgentNotFound,
        }));
    };
    if row != commit.expected_agent_data {
        return Ok(None);
    }
    let current = read_multi_agent_runtime_ownership_row(db, &input.session_path, &input.agent_id)?;
    if !super::multi_agent_runtime_ownership_rows_equal(commit.expected_ownership, current.as_ref()) {
        return Ok(None);
    }
    persist_acquired_runtime_ownership(
        db,
        &input.session_path,
        &input.agent_id,
        commit.process_identity_data,
        &input.owner,
    )?;
    db.execute(
        "UPDATE multi_agent_agents SET data = ?, updated_at = ? WHERE session_path = ? AND agent_id = ?",
        params![
            commit.updated_agent_data,
            input.now_iso,
            input.session_path,
            input.agent_id
        ],
    )?;
    Ok(Some(AcquireAttachedRuntimeOwnershipResult::Ok {
        agent: Box::new(commit.updated_agent.clone()),
        ownership: Box::new(ownership_from_identity_parts(
            &input.session_path,
            &input.agent_id,
            &input.owner,
            &input.process_identity,
        )),
    }))
}

fn read_attached_runtime_agent(
    db: &Connection,
    session_path: &JsString,
    agent_id: &JsString,
) -> Result<AttachedRuntimeAgentRead, ControlDbError> {
    let data = read_agent_data_row(db, session_path, agent_id)?;
    let Some(data) = data else {
        return Ok(AttachedRuntimeAgentRead::Err(AcquireAttachedError::AgentNotFound));
    };
    let context = js_format!("multi_agent_agents:{}#{}", session_path, agent_id);

    let agent = parse_agent_snapshot(&data, &context)?;
    if !is_recoverable_runtime_lifecycle(&agent.lifecycle) {
        return Ok(AttachedRuntimeAgentRead::Err(AcquireAttachedError::InvalidAgent));
    }
    Ok(AttachedRuntimeAgentRead::Ok {
        agent: Box::new(agent),
        data,
    })
}

fn read_registered_supervisor_session_authority(
    db: &Connection,
    session_path: &JsString,
    supervisor: &SupervisorRuntimeOwnership,
) -> Result<Option<SupervisorSessionAuthoritySnapshot>, ControlDbError> {
    if !process_identity_is_alive(&supervisor.process_identity) {
        return Ok(None);
    }
    let Some(authority) = read_persisted_supervisor_session_authority(db, session_path, supervisor)? else {
        return Ok(None);
    };
    let Ok(listener_identity) = parse_process_identity(&authority.runtime_instance_id) else {
        return Ok(None);
    };
    if serialize_process_identity(&listener_identity) == serialize_process_identity(&supervisor.process_identity) {
        Ok(Some(authority))
    } else {
        Ok(None)
    }
}

fn read_persisted_supervisor_session_authority(
    db: &Connection,
    session_path: &JsString,
    supervisor: &SupervisorRuntimeOwnership,
) -> Result<Option<SupervisorSessionAuthoritySnapshot>, ControlDbError> {
    let listener: Option<(Option<String>, Option<String>, Option<String>)> = db
        .query_row(
            "SELECT runtime_instance_id, session_path, session_path_asserted_at
			 FROM runtime_mailbox_listeners
			 WHERE recipient_session_id = ? AND recipient_agent_id_key = ? AND pid = ?",
            params![
                supervisor.session_id,
                runtime_mailbox_agent_id_key(supervisor.agent_id.as_ref()),
                supervisor.process_identity.pid
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((runtime_instance_id, listener_session_path, asserted_at)) = listener else {
        return Ok(None);
    };
    let Some(runtime_instance_id) = runtime_instance_id else {
        return Ok(None);
    };
    if supervisor.agent_id.is_none() {
        if !listener_session_path
            .as_ref()
            .is_some_and(|path| *session_path == *path)
            || asserted_at.is_none()
        {
            return Ok(None);
        }
        return Ok(Some(SupervisorSessionAuthoritySnapshot {
            runtime_instance_id,
            session_path: listener_session_path,
            session_path_asserted_at: asserted_at,
            supervisor_agent_data: None,
        }));
    }
    let supervisor_agent = read_agent_data_row(db, session_path, &supervisor.agent_id.clone().unwrap_or_default())?;
    let Some(supervisor_agent_data) = supervisor_agent else {
        return Ok(None);
    };
    let context = js_format!(
        "multi_agent_agents:{}#{}",
        session_path,
        &supervisor.agent_id.clone().unwrap_or_default()
    );
    let agent = parse_agent_snapshot(&supervisor_agent_data, &context)?;
    if agent.transcript.as_ref().map(|transcript| &transcript.session_id) != Some(&supervisor.session_id)
        || !is_active_lifecycle(agent.lifecycle)
    {
        return Ok(None);
    }
    Ok(Some(SupervisorSessionAuthoritySnapshot {
        runtime_instance_id,
        session_path: listener_session_path,
        session_path_asserted_at: asserted_at,
        supervisor_agent_data: Some(supervisor_agent_data),
    }))
}

fn supervisor_session_authority_is_current(
    db: &Connection,
    session_path: &JsString,
    supervisor: &SupervisorRuntimeOwnership,
    expected: &SupervisorSessionAuthoritySnapshot,
) -> Result<bool, ControlDbError> {
    let listener: Option<(Option<String>, Option<String>, Option<String>)> = db
        .query_row(
            "SELECT runtime_instance_id, session_path, session_path_asserted_at
			 FROM runtime_mailbox_listeners
			 WHERE recipient_session_id = ? AND recipient_agent_id_key = ? AND pid = ?",
            params![
                supervisor.session_id,
                runtime_mailbox_agent_id_key(supervisor.agent_id.as_ref()),
                supervisor.process_identity.pid
            ],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((runtime_instance_id, listener_session_path, asserted_at)) = listener else {
        return Ok(false);
    };
    if runtime_instance_id.as_deref() != Some(expected.runtime_instance_id.as_str())
        || listener_session_path != expected.session_path
        || asserted_at != expected.session_path_asserted_at
    {
        return Ok(false);
    }
    if supervisor.agent_id.is_none() {
        return Ok(expected.supervisor_agent_data.is_none());
    }
    let supervisor_agent = read_agent_data_row(db, session_path, &supervisor.agent_id.clone().unwrap_or_default())?;
    Ok(supervisor_agent == expected.supervisor_agent_data)
}

fn persist_acquired_runtime_ownership(
    db: &Connection,
    session_path: &JsString,
    agent_id: &JsString,
    process_identity_data: &str,
    owner: &MultiAgentRuntimeOwnerIdentity,
) -> Result<(), ControlDbError> {
    db.execute(
        "INSERT INTO multi_agent_runtime_owners (
			session_path, agent_id, process_identity, owner_session_id, owner_agent_id
		) VALUES (?, ?, ?, ?, ?)
		ON CONFLICT(session_path, agent_id) DO UPDATE SET
			process_identity = excluded.process_identity,
			owner_session_id = excluded.owner_session_id,
			owner_agent_id = excluded.owner_agent_id",
        params![
            session_path,
            agent_id,
            process_identity_data,
            owner.session_id,
            owner.agent_id
        ],
    )?;
    Ok(())
}

fn ownership_from_identity_parts(
    session_path: &JsString,
    agent_id: &JsString,
    owner: &MultiAgentRuntimeOwnerIdentity,
    process_identity: &crate::core::runtime_process::ProcessIdentity,
) -> MultiAgentRuntimeOwnership {
    MultiAgentRuntimeOwnership {
        agent_id: agent_id.to_owned(),
        owner: MultiAgentRuntimeOwner {
            agent_id: owner.agent_id.clone().map(Into::into),
            session_id: Some(owner.session_id.clone()),
        },
        process_identity: Some(process_identity.clone()),
        session_path: session_path.clone(),
    }
}

// ---------------------------------------------------------------------------
// Metadata updates
// ---------------------------------------------------------------------------

/// `updateMultiAgentAgentActivity`.
pub fn update_multi_agent_agent_activity(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    agent_id: &JsString,
    last_activity: Option<&JsValue>,
    updated_at: impl Into<JsString>,
) -> Result<Option<JsValue>, ControlDbError> {
    let updated_at: &JsString = &updated_at.into();
    let session_path: &JsString = &session_path.into();
    let control_db_path = control_db_path.into();
    let mut metadata = JsObject::new();
    if let Some(last_activity) = last_activity {
        metadata.insert("lastActivity".into(), last_activity.clone());
    }
    update_multi_agent_agent_metadata(&control_db_path, session_path, agent_id, &metadata, updated_at, None)
}

/// `updateMultiAgentAgentCurrentActivity`.
pub fn update_multi_agent_agent_current_activity(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    agent_id: &JsString,
    current_activity: Option<&JsValue>,
    updated_at: impl Into<JsString>,
    ownership: &crate::core::multi_agent_store::AgentCurrentActivityOwner,
    model: Option<&JsValue>,
) -> Result<Option<JsValue>, ControlDbError> {
    let updated_at: &JsString = &updated_at.into();
    let session_path: &JsString = &session_path.into();
    let control_db_path = control_db_path.into();
    let mut metadata = JsObject::new();
    if let Some(model) = model {
        metadata.insert("model".into(), model.clone());
    }
    metadata.insert(
        "currentActivity".into(),
        current_activity.cloned().unwrap_or(JsValue::Null),
    );
    update_multi_agent_agent_metadata(
        &control_db_path,
        session_path,
        agent_id,
        &metadata,
        updated_at,
        Some(ownership),
    )
}

/// `updateMultiAgentAgentModel`.
pub fn update_multi_agent_agent_model(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    agent_id: &JsString,
    model: &JsValue,
    updated_at: impl Into<JsString>,
    ownership: &crate::core::multi_agent_store::AgentCurrentActivityOwner,
) -> Result<Option<JsValue>, ControlDbError> {
    let updated_at: &JsString = &updated_at.into();
    let session_path: &JsString = &session_path.into();
    let control_db_path = control_db_path.into();
    let mut metadata = JsObject::new();
    metadata.insert("model".into(), model.clone());
    update_multi_agent_agent_metadata(
        &control_db_path,
        session_path,
        agent_id,
        &metadata,
        updated_at,
        Some(ownership),
    )
}

/// `updateMultiAgentAgentSlot`.
pub fn update_multi_agent_agent_slot(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    agent_id: &JsString,
    slot: Option<&JsValue>,
    updated_at: impl Into<JsString>,
) -> Result<Option<JsValue>, ControlDbError> {
    let updated_at: &JsString = &updated_at.into();
    let session_path: &JsString = &session_path.into();
    let control_db_path = control_db_path.into();
    let mut metadata = JsObject::new();
    metadata.insert("slot".into(), slot.cloned().unwrap_or(JsValue::Null));
    update_multi_agent_agent_metadata(&control_db_path, session_path, agent_id, &metadata, updated_at, None)
}

/// `updateMultiAgentAgentTranscript`.
pub fn update_multi_agent_agent_transcript(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    agent_id: &JsString,
    transcript: Option<&JsValue>,
    updated_at: impl Into<JsString>,
) -> Result<Option<JsValue>, ControlDbError> {
    let updated_at: &JsString = &updated_at.into();
    let session_path: &JsString = &session_path.into();
    let control_db_path = control_db_path.into();
    let mut metadata = JsObject::new();
    metadata.insert("transcript".into(), transcript.cloned().unwrap_or(JsValue::Null));
    update_multi_agent_agent_metadata(&control_db_path, session_path, agent_id, &metadata, updated_at, None)
}

fn update_multi_agent_agent_metadata(
    control_db_path: &JsString,
    session_path: &JsString,
    agent_id: &JsString,
    metadata: &JsObject,
    updated_at: &JsString,
    activity_ownership: Option<&crate::core::multi_agent_store::AgentCurrentActivityOwner>,
) -> Result<Option<JsValue>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        for _ in 0..MULTI_AGENT_METADATA_UPDATE_MAX_ATTEMPTS {
            let Some((data, row_updated_at)) = read_metadata_agent_row(db, session_path, agent_id)? else {
                return Ok(None);
            };
            let Some(mut agent) = read_metadata_agent(db, session_path, agent_id, &data, metadata, activity_ownership)?
            else {
                return Ok(None);
            };
            apply_metadata_patch(&mut agent, metadata, updated_at);
            let serialized = js_json::stringify_lossless(&JsValue::Object(agent.clone()));
            let update = MetadataUpdate {
                data: &data,
                row_updated_at: &row_updated_at,
                serialized: &serialized,
                updated_at,
                activity_ownership,
            };
            let changed = persist_metadata_update(db, session_path, agent_id, &update)?;
            if changed == 1 {
                return Ok(Some(JsValue::Object(agent)));
            }
        }
        Err(ControlDbError::new(js_format!(
            "Multi-agent metadata changed repeatedly while updating {}#{}",
            session_path,
            agent_id
        )))
    })
}

fn read_metadata_agent_row(
    db: &Connection,
    session_path: &JsString,
    agent_id: &JsString,
) -> Result<Option<(String, String)>, ControlDbError> {
    Ok(db
        .query_row(
            "SELECT data, updated_at FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![session_path, agent_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?)
}

fn read_metadata_agent(
    db: &Connection,
    session_path: &JsString,
    agent_id: &JsString,
    data: &str,
    metadata: &JsObject,
    activity_ownership: Option<&crate::core::multi_agent_store::AgentCurrentActivityOwner>,
) -> Result<Option<JsObject>, ControlDbError> {
    let context = js_format!("multi_agent_agents:{}#{}", session_path, agent_id);
    let agent = super::parse_stored_json_object_lossless(data, &context)?;
    validate_persisted_agent_payload(&agent, &context)?;
    if let Some(ownership) = activity_ownership
        && !multi_agent_activity_ownership_matches(db, session_path, agent_id, ownership)?
    {
        return Ok(None);
    }
    if !permits_multi_agent_current_activity(&agent, metadata) {
        return Ok(None);
    }
    Ok(Some(agent))
}

fn apply_metadata_patch(agent: &mut JsObject, metadata: &JsObject, updated_at: &JsString) {
    for (key, value) in metadata {
        if value.is_null() {
            agent.shift_remove(key);
        } else {
            agent.insert(key.clone(), value.clone());
        }
    }
    agent.insert("updatedAt".into(), JsValue::String(updated_at.clone()));
}

struct MetadataUpdate<'a> {
    data: &'a str,
    row_updated_at: &'a str,
    serialized: &'a str,
    updated_at: &'a JsString,
    activity_ownership: Option<&'a crate::core::multi_agent_store::AgentCurrentActivityOwner>,
}

fn persist_metadata_update(
    db: &Connection,
    session_path: &JsString,
    agent_id: &JsString,
    update: &MetadataUpdate<'_>,
) -> Result<usize, ControlDbError> {
    let Some(ownership) = update.activity_ownership else {
        return Ok(db.execute(
            "UPDATE multi_agent_agents SET data = ?, updated_at = ?
			 WHERE session_path = ? AND agent_id = ? AND data = ? AND updated_at = ?",
            params![
                update.serialized,
                update.updated_at,
                session_path,
                agent_id,
                update.data,
                update.row_updated_at
            ],
        )?);
    };
    Ok(db.execute(
        "UPDATE multi_agent_agents SET data = ?, updated_at = ?
		 WHERE session_path = ? AND agent_id = ? AND data = ? AND updated_at = ?
		 AND EXISTS (
			SELECT 1 FROM multi_agent_runtime_owners
			WHERE session_path = ? AND agent_id = ? AND owner_session_id = ?
			AND owner_agent_id IS NULL AND process_identity = ?
		 )",
        params![
            update.serialized,
            update.updated_at,
            session_path,
            agent_id,
            update.data,
            update.row_updated_at,
            session_path,
            agent_id,
            ownership.owner_session_id,
            serialize_process_identity(&ownership.process_identity)
        ],
    )?)
}

fn multi_agent_activity_ownership_matches(
    db: &Connection,
    session_path: &JsString,
    agent_id: &JsString,
    ownership: &crate::core::multi_agent_store::AgentCurrentActivityOwner,
) -> Result<bool, ControlDbError> {
    let runtime_ownership = read_multi_agent_runtime_ownership_row(db, session_path, agent_id)?;
    Ok(runtime_ownership.is_some_and(|runtime_ownership| {
        runtime_ownership.owner_session_id.as_ref() == Some(&ownership.owner_session_id)
            && runtime_ownership.owner_agent_id.is_none()
            && runtime_ownership.process_identity.as_deref()
                == Some(serialize_process_identity(&ownership.process_identity).as_str())
    }))
}

fn permits_multi_agent_current_activity(agent: &JsObject, metadata: &JsObject) -> bool {
    !metadata.contains_key("currentActivity")
        || metadata.get("currentActivity") == Some(&JsValue::Null)
        || agent
            .get("lifecycle")
            .and_then(JsValue::as_string)
            .is_some_and(|lifecycle| lifecycle == "running" || lifecycle == "steering_pending")
}

// ---------------------------------------------------------------------------
// Bootstrap and mailbox upserts
// ---------------------------------------------------------------------------

/// `bootstrapMultiAgentAgent`.
pub fn bootstrap_multi_agent_agent(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    id: &JsString,
    data: &JsValue,
) -> Result<(), ControlDbError> {
    let session_path: &JsString = &session_path.into();
    let control_db_path = control_db_path.into();
    let context = js_format!("multi_agent_agents:{}#{}", session_path, id);
    let Value::Object(object) = data else {
        return Err(ControlDbError::new(js_format!(
            "Invalid persisted agent payload at {}",
            context
        )));
    };
    validate_persisted_agent_payload(object, &context)?;
    let serialized = js_json::stringify_lossless(data);
    let updated_at = crate::utils::iso_time::now_iso_string();
    with_control_db(&control_db_path, |db| {
        if read_multi_agent_runtime_ownership_row(db, session_path, id)?.is_some() {
            return Err(ControlDbError::new(js_format!(
                "Generic agent upsert cannot mutate process-owned lifecycle row {}#{}",
                session_path,
                id
            )));
        }
        let changed = db.execute(
            "INSERT INTO multi_agent_agents (session_path, agent_id, data, updated_at)
			 SELECT ?, ?, ?, ?
			 WHERE NOT EXISTS (
				SELECT 1 FROM multi_agent_runtime_owners WHERE session_path = ? AND agent_id = ?
			 )
			 ON CONFLICT(session_path, agent_id) DO UPDATE SET
				data = excluded.data,
				updated_at = excluded.updated_at
			 WHERE NOT EXISTS (
				SELECT 1 FROM multi_agent_runtime_owners WHERE session_path = ? AND agent_id = ?
			 )",
            params![
                session_path,
                id,
                serialized,
                updated_at,
                session_path,
                id,
                session_path,
                id
            ],
        )?;
        if changed != 1 {
            return Err(ControlDbError::new(js_format!(
                "Generic agent upsert cannot mutate process-owned lifecycle row {}#{}",
                session_path,
                id
            )));
        }
        Ok(())
    })
}

/// `upsertMultiAgentMailboxMessage`.
pub fn upsert_multi_agent_mailbox_message(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    id: impl Into<JsString>,
    data: &JsValue,
) -> Result<(), ControlDbError> {
    let session_path: &JsString = &session_path.into();
    let control_db_path = control_db_path.into();
    let id: JsString = id.into();
    validate_mailbox_payload(
        data,
        &js_format!("multi_agent_mailbox_messages:{}#{}", session_path, id),
    )?;
    let created_at = data.get("createdAt").filter(|value| !value.is_null());
    if created_at
        .is_some_and(|value| super::is_mailbox_message_expired(value.as_string(), pi_js_compat::date::js_now_millis()))
    {
        return Ok(());
    }
    with_control_db(&control_db_path, |db| {
        for _ in 0..MULTI_AGENT_MAILBOX_UPSERT_MAX_ATTEMPTS {
            let plan = prepare_multi_agent_mailbox_upsert(db, session_path, &id, data)?;
            if super::is_mailbox_message_expired(
                super::parse_stored_json_object_lossless(&plan.serialized, &js_format!("{}#{}", session_path, id))?
                    .get("createdAt")
                    .and_then(Value::as_string)
                    .map(JsString::from)
                    .as_ref(),
                pi_js_compat::date::js_now_millis(),
            ) {
                return Ok(());
            }
            if plan.existing.as_ref().map(|existing| existing.data.as_str()) == Some(plan.serialized.as_str()) {
                return Ok(());
            }
            if with_immediate_transaction(db, |db| commit_multi_agent_mailbox_upsert(db, session_path, &id, &plan))? {
                return Ok(());
            }
        }
        Err(ControlDbError::new(js_format!(
            "Mailbox message changed repeatedly while updating {}#{}",
            session_path,
            id
        )))
    })
}

struct ParentRequestTargetSnapshot {
    agent_id: JsString,
    data: String,
}

struct MultiAgentMailboxRowSnapshot {
    data: String,
    updated_at: String,
}

struct MultiAgentMailboxUpsertPlan {
    existing: Option<MultiAgentMailboxRowSnapshot>,
    parent_target: Option<ParentRequestTargetSnapshot>,
    serialized: String,
}

fn prepare_multi_agent_mailbox_upsert(
    db: &Connection,
    session_path: &JsString,
    id: &JsString,
    data: &JsValue,
) -> Result<MultiAgentMailboxUpsertPlan, ControlDbError> {
    let sql_id = id;
    let parent_target = read_parent_request_target_snapshot(db, session_path, data)?;
    let existing = read_multi_agent_mailbox_row_snapshot(db, session_path, &sql_id)?;
    let mut serialized = js_json::stringify_lossless(data);
    if let Some(existing) = &existing {
        let previous = super::parse_stored_json_object_lossless(&existing.data, &JsString::from("mailbox upsert"))?;
        let next = super::parse_stored_json_object_lossless(&serialized, &JsString::from("mailbox upsert"))?;
        if !super::mailbox::same_mailbox_message_identity(&previous, &next, id) {
            return Err(ControlDbError::new(js_format!(
                "Mailbox message ID collision: {}#{}",
                session_path,
                sql_id
            )));
        }
        serialized = js_json::stringify_lossless(&JsValue::Object(merge_canonical_mailbox_update(&previous, &next)));
    } else if data.get("createdAt").is_none_or(JsValue::is_null) {
        let mut incoming =
            super::parse_stored_json_object_lossless(&serialized, &js_format!("{}#{}", session_path, sql_id))?;
        incoming.insert(
            "createdAt".into(),
            JsValue::String(crate::utils::iso_time::now_iso_string().into()),
        );
        serialized = js_json::stringify_lossless(&JsValue::Object(incoming));
    }
    Ok(MultiAgentMailboxUpsertPlan {
        existing,
        parent_target,
        serialized,
    })
}

fn commit_multi_agent_mailbox_upsert(
    db: &Connection,
    session_path: &JsString,
    id: &JsString,
    plan: &MultiAgentMailboxUpsertPlan,
) -> Result<bool, ControlDbError> {
    if super::is_mailbox_message_expired(
        super::parse_stored_json_object_lossless(&plan.serialized, &js_format!("{}#{}", session_path, id))?
            .get("createdAt")
            .and_then(Value::as_string)
            .map(JsString::from)
            .as_ref(),
        pi_js_compat::date::js_now_millis(),
    ) {
        return Ok(true);
    }
    let has_parent_target = i64::from(plan.parent_target.is_some());
    let has_existing = i64::from(plan.existing.is_some());
    Ok(db.execute(
        "INSERT INTO multi_agent_mailbox_messages (session_path, message_id, data, updated_at)
				 SELECT ?, ?, ?, ?
				 WHERE (? = 0 OR EXISTS (
					SELECT 1 FROM multi_agent_agents
					WHERE session_path = ? AND agent_id = ? AND data = ?
				 ))
				 AND ((? = 0 AND NOT EXISTS (
					SELECT 1 FROM multi_agent_mailbox_messages
					WHERE session_path = ? AND message_id = ?
				 )) OR (? = 1 AND EXISTS (
					SELECT 1 FROM multi_agent_mailbox_messages
					WHERE session_path = ? AND message_id = ? AND data = ? AND updated_at = ?
				 )))
				 ON CONFLICT(session_path, message_id) DO UPDATE SET
					data = excluded.data,
					updated_at = excluded.updated_at
				 WHERE ? = 1
				 AND multi_agent_mailbox_messages.data = ?
				 AND multi_agent_mailbox_messages.updated_at = ?",
        params![
            session_path,
            id,
            plan.serialized,
            crate::utils::iso_time::now_iso_string(),
            has_parent_target,
            session_path,
            plan.parent_target.as_ref().map(|target| &target.agent_id),
            plan.parent_target.as_ref().map(|target| target.data.as_str()),
            has_existing,
            session_path,
            id,
            has_existing,
            session_path,
            id,
            plan.existing.as_ref().map(|existing| existing.data.as_str()),
            plan.existing.as_ref().map(|existing| existing.updated_at.as_str()),
            has_existing,
            plan.existing.as_ref().map(|existing| existing.data.as_str()),
            plan.existing.as_ref().map(|existing| existing.updated_at.as_str())
        ],
    )? == 1)
}

fn read_multi_agent_mailbox_row_snapshot(
    db: &Connection,
    session_path: &JsString,
    id: &JsString,
) -> Result<Option<MultiAgentMailboxRowSnapshot>, ControlDbError> {
    Ok(db
        .query_row(
            "SELECT data, updated_at FROM multi_agent_mailbox_messages WHERE session_path = ? AND message_id = ?",
            params![session_path, id],
            |row| {
                Ok(MultiAgentMailboxRowSnapshot {
                    data: row.get(0)?,
                    updated_at: row.get(1)?,
                })
            },
        )
        .optional()?)
}

fn merge_canonical_mailbox_update(previous: &js_json::JsObject, next: &js_json::JsObject) -> js_json::JsObject {
    let mut merged = next.clone();
    for key in [
        "createdAt",
        "recipientAgentId",
        "recipientSessionId",
        "senderAgentId",
        "senderSessionId",
    ] {
        if let Some(value) = previous.get(&JsString::from(key)) {
            merged.insert(key.into(), value.clone());
        } else {
            merged.shift_remove(&JsString::from(key));
        }
    }
    if next
        .get("status")
        .and_then(JsValue::as_string)
        .is_some_and(|value| value == "pending")
        && previous
            .get("status")
            .and_then(JsValue::as_string)
            .is_some_and(|value| value == "claimed")
    {
        if let Some(value) = previous.get("claimedAt") {
            merged.insert("claimedAt".into(), value.clone());
        }
        if let Some(value) = previous.get("claimantProcessIdentity") {
            merged.insert("claimantProcessIdentity".into(), value.clone());
        }
        merged.insert("status".into(), JsValue::String("claimed".into()));
    }
    merged
}

fn read_parent_request_target_snapshot(
    db: &Connection,
    session_path: &JsString,
    data: &Value,
) -> Result<Option<ParentRequestTargetSnapshot>, ControlDbError> {
    let Value::Object(payload) = data else {
        return Ok(None);
    };
    if payload
        .get("kind")
        .and_then(Value::as_string)
        .is_none_or(|kind| kind != "parent_request")
    {
        return Ok(None);
    }
    let from_agent_id = require_native_string_field(payload, "fromAgentId", &JsString::from("parent_request"))?;
    let to_agent_id = require_native_string_field(payload, "toAgentId", &JsString::from("parent_request"))?;
    let row = read_agent_data_row(db, session_path, &from_agent_id)?;
    let sender = row.as_deref().and_then(super::parse_json_object_lossless);
    if sender
        .as_ref()
        .and_then(|sender| sender.get("parentId"))
        .and_then(Value::as_string)
        .is_none_or(|parent_id| &to_agent_id != parent_id)
    {
        return Err(ControlDbError::new(js_format!(
            "Invalid parent request target at {}#{}",
            session_path,
            from_agent_id
        )));
    }
    Ok(Some(ParentRequestTargetSnapshot {
        agent_id: from_agent_id.into(),
        data: row.unwrap_or_default(),
    }))
}

/// `getMultiAgentMailboxMessageStatus`.
pub fn get_multi_agent_mailbox_message_status(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    message_id: &JsString,
) -> Result<Option<JsString>, ControlDbError> {
    let session_path: &JsString = &session_path.into();
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        let data: Option<String> = db
            .query_row(
                &format!("SELECT data FROM multi_agent_mailbox_messages WHERE session_path = ? AND message_id = ? AND {} > ?",super::retention::MAILBOX_CREATED_AT_MS_SQL),
                params![session_path, message_id,super::retention::mailbox_cutoff_ms()],
                |row| row.get(0),
            )
            .optional()?;
        Ok(data
            .as_deref()
            .and_then(super::parse_json_object_lossless)
            .and_then(|parsed| parsed.get("status").and_then(Value::as_string).cloned()))
    })
}

/// `markMultiAgentMailboxMessageDelivered`.
pub fn mark_multi_agent_mailbox_message_delivered(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    message_id: &JsString,
) -> Result<bool, ControlDbError> {
    let session_path: &JsString = &session_path.into();
    let control_db_path = control_db_path.into();
    Ok(super::consume_runtime_mailbox_message_by_store_ref(
        control_db_path,
        &super::RuntimeMailboxStoreRef {
            session_path: session_path.to_owned().into(),
            message_id: message_id.to_owned().into(),
        },
    )? == 1)
}

/// `markMultiAgentMailboxMessageFailed`.
pub fn mark_multi_agent_mailbox_message_failed(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    message_id: &JsString,
    error: impl Into<JsString>,
) -> Result<bool, ControlDbError> {
    let session_path: &JsString = &session_path.into();
    let control_db_path = control_db_path.into();
    update_multi_agent_mailbox_message_status(
        &control_db_path,
        session_path,
        message_id,
        "failed",
        Some(&error.into()),
    )
}

fn update_multi_agent_mailbox_message_status(
    control_db_path: &JsString,
    session_path: &JsString,
    message_id: &JsString,
    status: &str,
    error: Option<&JsString>,
) -> Result<bool, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let Some(row) = super::read_canonical_mailbox_row_by_store_ref(
            db,
            &super::RuntimeMailboxStoreRef {
                session_path: session_path.to_owned().into(),
                message_id: message_id.to_owned().into(),
            },
        )?
        else {
            return Ok(false);
        };
        let Some(parsed) = super::parse_json_object_lossless(&row.data) else {
            return Ok(false);
        };
        if parsed
            .get("status")
            .and_then(Value::as_string)
            .is_none_or(|status| status != "pending")
        {
            return Ok(false);
        }
        let now = crate::utils::iso_time::now_iso_string();
        let serialized = super::serialize_canonical_mailbox_mutation(&row, |updated| {
            updated.insert(JsString::from("status"), JsValue::String(JsString::from(status)));
            updated.insert(
                JsString::from("updatedAt"),
                JsValue::String(JsString::from(now.as_str())),
            );
            if let Some(error) = error {
                updated.insert(JsString::from("error"), JsValue::String(JsString::from(error)));
            }
        })?;
        with_immediate_transaction(db, |db| {
            super::compare_and_write_serialized_canonical_mailbox_payload(db, &row, &serialized, &now)
        })
    })
}

// ---------------------------------------------------------------------------
// Counters and reads
// ---------------------------------------------------------------------------

/// `writeMultiAgentCounters`.
pub fn write_multi_agent_counters(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    counters: &MultiAgentCounters,
) -> Result<(), ControlDbError> {
    let session_path: &JsString = &session_path.into();
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        db.execute(
            "
			INSERT INTO multi_agent_counters_v2 (
				session_path, next_agent_number, next_message_number, updated_at
			)
			VALUES (?, ?, ?, ?)
			ON CONFLICT(session_path) DO UPDATE SET
				next_agent_number = excluded.next_agent_number,
				next_message_number = excluded.next_message_number,
				updated_at = excluded.updated_at
			",
            params![
                session_path,
                counters.next_agent_number,
                counters.next_message_number,
                crate::utils::iso_time::now_iso_string()
            ],
        )?;
        Ok(())
    })
}

const MULTI_AGENT_COUNTER_ALLOCATION_SQL_AGENT: &str = "INSERT INTO multi_agent_counters_v2
		(session_path, next_agent_number, next_message_number, updated_at)
		VALUES (?, 2, 1, ?)
		ON CONFLICT(session_path) DO UPDATE SET
			next_agent_number = next_agent_number + 1,
			updated_at = excluded.updated_at
		RETURNING next_agent_number - 1 AS value";
const MULTI_AGENT_COUNTER_ALLOCATION_SQL_MESSAGE: &str = "INSERT INTO multi_agent_counters_v2
		(session_path, next_agent_number, next_message_number, updated_at)
		VALUES (?, 1, 2, ?)
		ON CONFLICT(session_path) DO UPDATE SET
			next_message_number = next_message_number + 1,
			updated_at = excluded.updated_at
		RETURNING next_message_number - 1 AS value";

/// `allocateMultiAgentCounter`.
pub fn allocate_multi_agent_counter(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    counter_name: MultiAgentCounterName,
) -> Result<i64, ControlDbError> {
    let session_path: &JsString = &session_path.into();
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        allocate_multi_agent_counter_with_db(db, session_path, counter_name)
    })
}

pub(crate) fn allocate_multi_agent_counter_with_db(
    db: &Connection,
    session_path: impl Into<JsString>,
    counter_name: MultiAgentCounterName,
) -> Result<i64, ControlDbError> {
    let session_path: &JsString = &session_path.into();
    let sql = match counter_name {
        MultiAgentCounterName::Agent => MULTI_AGENT_COUNTER_ALLOCATION_SQL_AGENT,
        MultiAgentCounterName::Message => MULTI_AGENT_COUNTER_ALLOCATION_SQL_MESSAGE,
    };
    let value: Option<i64> = db
        .query_row(
            sql,
            params![session_path, crate::utils::iso_time::now_iso_string()],
            |row| row.get(0),
        )
        .optional()?;
    value.ok_or_else(|| {
        ControlDbError::new(js_format!(
            "Multi-agent {} counter allocation returned no value for {}",
            counter_name.as_str(),
            session_path
        ))
    })
}

/// `readMultiAgentAgent`.
pub fn read_multi_agent_agent(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    agent_id: impl Into<JsString>,
) -> Result<Option<AgentSnapshot>, ControlDbError> {
    let session_path: &JsString = &session_path.into();
    let control_db_path = control_db_path.into();
    let agent_id = agent_id.into();
    with_control_db(&control_db_path, |db| {
        let data = read_agent_data_row(db, session_path, &agent_id)?;
        let Some(data) = data else {
            return Ok(None);
        };
        let context = js_format!(
            "multi_agent_agents:{}#{}",
            session_path,
            js_json::stringify_lossless(&JsValue::String(agent_id.clone()))
        );

        Ok(Some(parse_agent_snapshot(&data, &context)?))
    })
}

/// `readMultiAgentAgents`.
pub fn read_multi_agent_agents(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
    agent_ids: &[JsString],
) -> Result<Vec<AgentSnapshot>, ControlDbError> {
    let session_path: &JsString = &session_path.into();
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        let mut agents = Vec::new();
        for agent_id in agent_ids {
            let data = read_agent_data_row(db, session_path, agent_id)?;
            let Some(data) = data else {
                continue;
            };
            let context = js_format!("multi_agent_agents:{}#{}", session_path, agent_id);

            agents.push(parse_agent_snapshot(&data, &context)?);
        }
        Ok(agents)
    })
}

/// `listDetachedArtifactAgentsUpdatedAtOrBefore`.
pub fn list_detached_artifact_agents_updated_at_or_before(
    control_db_path: impl Into<JsString>,
    updated_at_cutoff: &str,
) -> Result<Vec<DetachedArtifactAgentRecord>, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        let mut statement = db.prepare(
            "SELECT session_path, agent_id, data
				 FROM multi_agent_agents
				 WHERE CASE WHEN json_valid(data) THEN json_extract(data, '$.updatedAt') END <= ?
				   AND json_extract(data, '$.lifecycle') IN ('completed', 'failed', 'aborted')
				   AND EXISTS (
				     SELECT 1
				     FROM json_each(data, '$.result.fileRefs') AS file_ref
				     WHERE json_extract(file_ref.value, '$.label') IN ('Bash output', 'Pyrun output')
				   )
				 ORDER BY CASE WHEN json_valid(data) THEN json_extract(data, '$.updatedAt') END,
				          session_path,
				          agent_id",
        )?;
        let rows = statement
            .query_map(params![updated_at_cutoff], |row| {
                Ok((
                    row.get::<_, JsString>(0)?,
                    row.get::<_, JsString>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut records = Vec::new();
        for (session_path, agent_id, data) in rows {
            let context = js_format!("multi_agent_agents:{}#{}", session_path, agent_id);

            records.push(DetachedArtifactAgentRecord {
                agent: parse_agent_snapshot(&data, &context)?,
                session_path,
            });
        }
        Ok(records)
    })
}

/// `readMultiAgentState`.
pub fn read_multi_agent_state(
    control_db_path: impl Into<JsString>,
    session_path: impl Into<JsString>,
) -> Result<Option<MultiAgentPersistedState>, ControlDbError> {
    let session_path: &JsString = &session_path.into();
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        let read_rows = |table: &str| -> Result<Vec<JsValue>, ControlDbError> {
            let retention = if table == "multi_agent_mailbox_messages" {
                format!(" AND {} > ?", super::retention::MAILBOX_CREATED_AT_MS_SQL)
            } else {
                String::new()
            };
            let mut statement = db.prepare(&format!(
                "SELECT data FROM {table} WHERE session_path = ?{retention} ORDER BY rowid"
            ))?;
            let mut args = vec![rusqlite::types::Value::Text(encode_node_utf8(session_path))];
            if table == "multi_agent_mailbox_messages" {
                args.push(rusqlite::types::Value::Integer(super::retention::mailbox_cutoff_ms()));
            }
            let rows = statement
                .query_map(rusqlite::params_from_iter(args), |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            let mut values = Vec::with_capacity(rows.len());
            for (index, data) in rows.iter().enumerate() {
                let context = js_format!("{}:{}[{}]", table, session_path, index.to_string());
                let parsed = parse_stored_json_object_lossless(data, &context)?;
                if table == "multi_agent_agents" {
                    validate_persisted_agent_payload(&parsed, &context)?;
                } else {
                    validate_mailbox_payload(&Value::Object(parsed.clone()), &context)?;
                }
                values.push(js_json::parse_lossless(data).map_err(|error| ControlDbError::new(error.to_string()))?);
            }
            Ok(values)
        };
        let counters: Option<(i64, i64)> = db
            .query_row(
                "
				SELECT next_agent_number, next_message_number
				FROM multi_agent_counters_v2 WHERE session_path = ?
				",
                params![session_path],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let agents = read_rows("multi_agent_agents")?;
        let mailbox_messages = read_rows("multi_agent_mailbox_messages")?;
        if counters.is_none() && agents.is_empty() && mailbox_messages.is_empty() {
            return Ok(None);
        }
        Ok(Some(MultiAgentPersistedState {
            agents,
            mailbox_messages,
            counters: MultiAgentCounters {
                next_agent_number: counters.map_or(1, |counters| counters.0),
                next_message_number: counters.map_or(1, |counters| counters.1),
            },
        }))
    })
}

// ---------------------------------------------------------------------------
// Dead runtime recovery
// ---------------------------------------------------------------------------

struct SupervisorRecoveryAuthority {
    listener: RuntimeMailboxListenerRow,
    supervisor_session_id: JsString,
    supervisor_agent_id: Option<JsString>,
    supervisor_agent_data: Option<String>,
}

struct DeadRuntimeRecoveryPlan {
    agent_data: String,
    process_identity_data: String,
    terminal_revision: i64,
    terminal_transport: Option<PreparedTerminalTransport>,
    updated_agent: Map,
    updated_agent_data: String,
}

pub(crate) struct PreparedTerminalTransport {
    input: EnqueueStoredRuntimeMailboxMessageInput,
    prepared: PreparedStoredRuntimeMailboxMessage,
}

enum RecoverableMultiAgentRuntime {
    Ok { agent: Map, agent_data: String },
    Err(RecoverDeadRuntimeError),
}

/// `recoverDeadMultiAgentRuntime`.
pub fn recover_dead_multi_agent_runtime(
    control_db_path: impl Into<JsString>,
    input: &RecoverDeadMultiAgentRuntimeInput,
) -> Result<RecoverDeadMultiAgentRuntimeResult, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        for _ in 0..3 {
            let Some(supervisor_authority) =
                read_supervisor_recovery_authority(db, &input.expected_owner.session_path, &input.supervisor)?
            else {
                return Ok(RecoverDeadMultiAgentRuntimeResult::Err {
                    error: RecoverDeadRuntimeError::MutationMismatch,
                });
            };
            let recoverable = read_recoverable_multi_agent_runtime(db, &input.expected_owner, true)?;
            let recoverable = match recoverable {
                RecoverableMultiAgentRuntime::Ok { agent, agent_data } => (agent, agent_data),
                RecoverableMultiAgentRuntime::Err(error) => {
                    return Ok(RecoverDeadMultiAgentRuntimeResult::Err { error });
                }
            };
            let plan = prepare_dead_runtime_recovery(recoverable, &input.expected_owner, &input.now_iso)?;
            if let Some(result) =
                persist_dead_runtime_recovery(db, &input.expected_owner, &plan, Some(&supervisor_authority))?
            {
                return Ok(result);
            }
        }
        Ok(RecoverDeadMultiAgentRuntimeResult::Err {
            error: RecoverDeadRuntimeError::MutationMismatch,
        })
    })
}

fn read_supervisor_recovery_authority(
    db: &Connection,
    session_path: &JsString,
    supervisor: &SupervisorRuntimeOwnership,
) -> Result<Option<SupervisorRecoveryAuthority>, ControlDbError> {
    if !process_identity_is_alive(&supervisor.process_identity) {
        return Ok(None);
    }
    let listener = read_runtime_mailbox_listener_row(
        db,
        &RuntimeMailboxAddress {
            agent_id: supervisor.agent_id.clone().map(Into::into),
            session_id: supervisor.session_id.clone().into(),
        },
    )?;
    let Some(listener) = listener else {
        return Ok(None);
    };
    let Some(runtime_instance_id) = listener.runtime_instance_id.as_deref() else {
        return Ok(None);
    };
    if listener.pid != supervisor.process_identity.pid {
        return Ok(None);
    }
    let Ok(listener_identity) = parse_process_identity(runtime_instance_id) else {
        return Ok(None);
    };
    if serialize_process_identity(&listener_identity) != serialize_process_identity(&supervisor.process_identity) {
        return Ok(None);
    }
    if supervisor.agent_id.is_none() {
        if !listener
            .session_path
            .as_ref()
            .is_some_and(|path| *session_path == *path)
            || listener.session_path_asserted_at.is_none()
        {
            return Ok(None);
        }
        return Ok(Some(SupervisorRecoveryAuthority {
            listener,
            supervisor_session_id: supervisor.session_id.clone(),
            supervisor_agent_id: supervisor.agent_id.clone().map(Into::into),
            supervisor_agent_data: None,
        }));
    }
    let supervisor_row = read_agent_data_row(db, session_path, &supervisor.agent_id.clone().unwrap_or_default())?;
    let Some(supervisor_agent_data) = supervisor_row else {
        return Ok(None);
    };
    let snapshot = parse_stored_json_object_lossless(
        &supervisor_agent_data,
        &js_format!(
            "multi_agent_agents:{}#{}",
            session_path,
            &supervisor.agent_id.clone().unwrap_or_default()
        ),
    )?;
    let agent: AgentSnapshot =
        js_json::from_value(&Value::Object(snapshot)).map_err(|error| ControlDbError::new(error.to_string()))?;
    if agent.transcript.as_ref().map(|transcript| &transcript.session_id) != Some(&supervisor.session_id)
        || !is_active_lifecycle(agent.lifecycle)
    {
        return Ok(None);
    }
    Ok(Some(SupervisorRecoveryAuthority {
        listener,
        supervisor_session_id: supervisor.session_id.clone(),
        supervisor_agent_id: supervisor.agent_id.clone().map(Into::into),
        supervisor_agent_data: Some(supervisor_agent_data),
    }))
}

fn read_recoverable_multi_agent_runtime(
    db: &Connection,
    expected_owner: &MultiAgentRuntimeOwnershipIdentity,
    verify_owner_liveness: bool,
) -> Result<RecoverableMultiAgentRuntime, ControlDbError> {
    let data = read_agent_data_row(db, &expected_owner.session_path, &expected_owner.agent_id)?;
    let Some(data) = data else {
        return Ok(RecoverableMultiAgentRuntime::Err(
            RecoverDeadRuntimeError::AgentNotFound,
        ));
    };
    let agent = parse_stored_json_object_lossless(
        &data,
        &js_format!(
            "multi_agent_agents:{}#{}",
            expected_owner.session_path,
            expected_owner.agent_id
        ),
    )?;
    let lifecycle = agent
        .get("lifecycle")
        .and_then(Value::as_string)
        .and_then(parse_lifecycle);
    let Some(lifecycle) = lifecycle else {
        return Ok(RecoverableMultiAgentRuntime::Err(
            RecoverDeadRuntimeError::InvalidTransition,
        ));
    };
    if !is_recoverable_runtime_lifecycle(&lifecycle) {
        return Ok(RecoverableMultiAgentRuntime::Err(
            RecoverDeadRuntimeError::InvalidTransition,
        ));
    }
    let owner = read_multi_agent_runtime_ownership_row(db, &expected_owner.session_path, &expected_owner.agent_id)?;
    if !super::runtime_owner_matches(owner.as_ref(), expected_owner) {
        return Ok(RecoverableMultiAgentRuntime::Err(
            RecoverDeadRuntimeError::MutationMismatch,
        ));
    }
    if has_active_persisted_descendant(db, &expected_owner.session_path, &expected_owner.agent_id)? {
        return Ok(RecoverableMultiAgentRuntime::Err(
            RecoverDeadRuntimeError::InvalidTransition,
        ));
    }
    if verify_owner_liveness && process_identity_is_alive(&expected_owner.process_identity) {
        return Ok(RecoverableMultiAgentRuntime::Err(RecoverDeadRuntimeError::OwnerAlive));
    }
    Ok(RecoverableMultiAgentRuntime::Ok {
        agent,
        agent_data: data,
    })
}

fn parse_lifecycle(value: &JsString) -> Option<AgentLifecycle> {
    js_json::from_value(&Value::String(value.clone())).ok()
}

fn prepare_dead_runtime_recovery(
    recoverable: (Map, String),
    expected_owner: &MultiAgentRuntimeOwnershipIdentity,
    now_iso: &str,
) -> Result<DeadRuntimeRecoveryPlan, ControlDbError> {
    let (agent, agent_data) = recoverable;
    let revision = agent
        .get("revision")
        .and_then(Value::as_f64)
        .map(|value| value as i64)
        .unwrap_or(0);
    let terminal_revision = revision + 1;
    let cancelling = agent
        .get("lifecycle")
        .and_then(Value::as_string)
        .is_some_and(|lifecycle| lifecycle == "cancelling");
    let mut updated_agent = agent.clone();
    updated_agent.insert(
        "error".into(),
        pi_js_compat::js_json!({
            "code": "lost_runtime",
            "message": if cancelling {
                "Cancellation requested; agent owner process exited before terminal confirmation."
            } else {
                "Agent owner process exited before terminal confirmation."
            },
        }),
    );
    updated_agent.insert(
        "lifecycle".into(),
        Value::String(if cancelling { "aborted" } else { "failed" }.into()),
    );
    let tool_call_id = agent
        .get("worker")
        .and_then(Value::as_object)
        .and_then(|worker| worker.get("toolCallId"))
        .and_then(Value::as_string)
        .cloned();
    if let Some(tool_call_id) = tool_call_id {
        let mut result = agent
            .get("result")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        result.insert("toolCallId".into(), Value::String(tool_call_id));
        updated_agent.insert("result".into(), Value::Object(result));
    }
    updated_agent.insert("revision".into(), Value::Number(terminal_revision as f64));
    updated_agent.insert("updatedAt".into(), Value::String(now_iso.into()));
    updated_agent.shift_remove("worker");
    let terminal_transport = if should_notify_detached_job_owner(&agent) {
        Some(prepare_detached_agent_terminal_transport(
            &expected_owner.session_path,
            &expected_owner.agent_id,
            &expected_owner.owner,
            terminal_revision,
            "lost_runtime",
            &JsString::from(now_iso),
        )?)
    } else {
        None
    };
    Ok(DeadRuntimeRecoveryPlan {
        agent_data,
        process_identity_data: serialize_process_identity(&expected_owner.process_identity),
        terminal_revision,
        terminal_transport,
        updated_agent_data: js_json::stringify_lossless(&Value::Object(updated_agent.clone())),
        updated_agent,
    })
}

fn persist_dead_runtime_recovery(
    db: &Connection,
    expected_owner: &MultiAgentRuntimeOwnershipIdentity,
    plan: &DeadRuntimeRecoveryPlan,
    supervisor_authority: Option<&SupervisorRecoveryAuthority>,
) -> Result<Option<RecoverDeadMultiAgentRuntimeResult>, ControlDbError> {
    with_immediate_transaction(db, |db| {
        if !release_dead_runtime_ownership_if_current(db, expected_owner, plan, supervisor_authority)? {
            return Ok(None);
        }
        let updated_at = plan
            .updated_agent
            .get("updatedAt")
            .and_then(Value::as_string)
            .cloned()
            .unwrap_or_default();
        let changed = db.execute(
            "UPDATE multi_agent_agents SET data = ?, updated_at = ?
				 WHERE session_path = ? AND agent_id = ? AND data = ?",
            params![
                plan.updated_agent_data,
                updated_at,
                expected_owner.session_path,
                expected_owner.agent_id,
                plan.agent_data
            ],
        )?;
        if changed != 1 {
            return Err(ControlDbError::new(js_format!(
                "Recovered agent changed after ownership release {}#{}",
                expected_owner.session_path,
                expected_owner.agent_id
            )));
        }
        db.execute(
            "INSERT INTO multi_agent_terminal_outbox
				(session_path, agent_id, terminal_revision, event_kind, status, attempt_count, updated_at)
			 VALUES (?, ?, ?, 'lost_runtime', 'pending', 0, ?)",
            params![
                expected_owner.session_path,
                expected_owner.agent_id,
                plan.terminal_revision,
                updated_at
            ],
        )?;
        if let Some(transport) = &plan.terminal_transport {
            persist_prepared_terminal_transport(db, transport)?;
        }
        Ok(Some(RecoverDeadMultiAgentRuntimeResult::Ok {
            agent: plan.updated_agent.clone(),
            terminal_revision: plan.terminal_revision,
        }))
    })
}

fn release_dead_runtime_ownership_if_current(
    db: &Connection,
    expected_owner: &MultiAgentRuntimeOwnershipIdentity,
    plan: &DeadRuntimeRecoveryPlan,
    supervisor_authority: Option<&SupervisorRecoveryAuthority>,
) -> Result<bool, ControlDbError> {
    let listener = supervisor_authority.map(|authority| &authority.listener);
    let supervisor_agent_data = supervisor_authority.and_then(|authority| authority.supervisor_agent_data.as_deref());
    Ok(db.execute(
        "WITH RECURSIVE descendants(agent_id) AS (
					SELECT agent_id FROM multi_agent_agents
					WHERE session_path = ? AND json_valid(data)
					AND json_extract(data, '$.parentId') = ?
					UNION
					SELECT child.agent_id FROM multi_agent_agents AS child
					JOIN descendants AS parent
						ON json_valid(child.data) AND json_extract(child.data, '$.parentId') = parent.agent_id
					WHERE child.session_path = ?
				)
				UPDATE multi_agent_runtime_owners
				SET process_identity = NULL, owner_session_id = NULL, owner_agent_id = NULL
				WHERE session_path = ? AND agent_id = ? AND process_identity = ?
				AND owner_session_id = ? AND owner_agent_id IS ?
				AND EXISTS (
					SELECT 1 FROM multi_agent_agents
					WHERE session_path = ? AND agent_id = ? AND data = ?
				)
				AND NOT EXISTS (
					SELECT 1 FROM multi_agent_agents
					WHERE session_path = ? AND NOT json_valid(data)
				)
				AND NOT EXISTS (
					SELECT 1 FROM multi_agent_agents AS active
					JOIN descendants ON descendants.agent_id = active.agent_id
					WHERE active.session_path = ?
					AND json_extract(active.data, '$.lifecycle') NOT IN ('completed', 'failed', 'aborted')
				)
				AND NOT EXISTS (
					SELECT 1 FROM multi_agent_terminal_outbox
					WHERE session_path = ? AND agent_id = ?
				)
				AND (? = 0 OR EXISTS (
					SELECT 1 FROM runtime_mailbox_listeners
					WHERE recipient_session_id = ? AND recipient_agent_id_key = ? AND pid = ?
					AND runtime_instance_id IS ? AND session_path IS ?
					AND session_path_asserted_at IS ? AND updated_at = ?
				))
				AND (? = 0 OR EXISTS (
					SELECT 1 FROM multi_agent_agents
					WHERE session_path = ? AND agent_id = ? AND data = ?
				))",
        params![
            expected_owner.session_path,
            expected_owner.agent_id,
            expected_owner.session_path,
            expected_owner.session_path,
            expected_owner.agent_id,
            plan.process_identity_data,
            expected_owner.owner.session_id,
            expected_owner.owner.agent_id,
            expected_owner.session_path,
            expected_owner.agent_id,
            plan.agent_data,
            expected_owner.session_path,
            expected_owner.session_path,
            expected_owner.session_path,
            expected_owner.agent_id,
            i64::from(supervisor_authority.is_some()),
            supervisor_authority.map(|authority| authority.supervisor_session_id.clone()),
            supervisor_authority
                .and_then(|authority| authority.supervisor_agent_id.clone())
                .unwrap_or_default(),
            listener.map(|listener| listener.pid),
            listener.and_then(|listener| listener.runtime_instance_id.clone()),
            listener.and_then(|listener| listener.session_path.clone()),
            listener.and_then(|listener| listener.session_path_asserted_at.clone()),
            listener.map(|listener| listener.updated_at.clone()),
            i64::from(supervisor_agent_data.is_some()),
            expected_owner.session_path,
            supervisor_authority.and_then(|authority| authority.supervisor_agent_id.clone()),
            supervisor_agent_data
        ],
    )? == 1)
}

pub(super) fn should_notify_detached_job_owner(agent: &Map) -> bool {
    agent.get("detached").and_then(Value::as_bool) == Some(true)
        && agent.get("suppressTerminalNotification").and_then(Value::as_bool) != Some(true)
}

pub(crate) fn prepare_detached_agent_terminal_transport(
    session_path: impl Into<JsString>,
    agent_id: &JsString,
    owner: &MultiAgentRuntimeOwnerIdentity,
    terminal_revision: i64,
    event_kind: &str,
    terminal_at: &JsString,
) -> Result<PreparedTerminalTransport, ControlDbError> {
    let session_path: &JsString = &session_path.into();
    let owner_session_id = &owner.session_id;
    if owner_session_id.is_empty() {
        return Err(ControlDbError::new(js_format!(
            "Detached job {} ownership has no owner session",
            agent_id
        )));
    }
    let message_id = js_format!("terminal:{}:{}:{}", agent_id, terminal_revision.to_string(), event_kind);
    let body = js_json::stringify_lossless(&pi_js_compat::js_json!({
        "agentId": agent_id,
        "eventKind": event_kind,
        "terminalRevision": terminal_revision,
        "type": "multi_agent_terminal",
    }));
    let message = pi_js_compat::js_json!({
        "body": body,
        "createdAt": terminal_at,
        "fromAgentId": agent_id,
        "id": message_id,
        "kind": "system",
        "status": "pending",
        "toAgentId": owner.agent_id.clone().unwrap_or_else(|| "main".into()),
        "updatedAt": terminal_at,
    });
    let input = EnqueueStoredRuntimeMailboxMessageInput {
        recipient_host: None,
        kind: RuntimeMailboxMessageKind::System,
        message: message.into(),
        recipient: RuntimeMailboxAddress {
            agent_id: owner.agent_id.clone(),
            session_id: owner_session_id.clone(),
        },
        sender: RuntimeMailboxAddress {
            agent_id: Some(agent_id.clone()),
            session_id: owner_session_id.clone(),
        },
        store_ref: super::RuntimeMailboxStoreRef {
            message_id,
            session_path: session_path.clone(),
        },
        updated_at: Some(encode_node_utf8(terminal_at)),
    };
    let prepared = prepare_stored_runtime_mailbox_message(&input)?;
    Ok(PreparedTerminalTransport { input, prepared })
}

pub(crate) fn persist_prepared_terminal_transport(
    db: &Connection,
    transport: &PreparedTerminalTransport,
) -> Result<(), ControlDbError> {
    super::retention::assert_mailbox_message_fresh(
        &super::parse_stored_json_object_lossless(
            &transport.prepared.serialized,
            &JsString::from("terminal transport"),
        )?,
        &transport.prepared.context,
    )?;
    db.execute(
        "INSERT INTO multi_agent_mailbox_messages (session_path, message_id, data, updated_at)
		 VALUES (?, ?, ?, ?)",
        params![
            transport.input.store_ref.session_path,
            transport.input.store_ref.message_id,
            transport.prepared.serialized,
            transport.prepared.updated_at
        ],
    )?;
    Ok(())
}

/// `reconcileDeadDetachedAgentRuntimes`.
pub fn reconcile_dead_detached_agent_runtimes(
    control_db_path: impl Into<JsString>,
    now_iso: &str,
) -> Result<i64, ControlDbError> {
    let control_db_path = control_db_path.into();
    with_control_db(&control_db_path, |db| {
        let candidates = read_dead_detached_runtime_candidates(db)?;
        let mut expected_owners = Vec::new();
        for candidate in &candidates {
            if let Some(owner) = read_detached_owner_for_recovery(db, candidate)? {
                expected_owners.push(owner);
            }
        }
        let mut count = 0;
        for expected_owner in expected_owners {
            count += reconcile_dead_detached_agent_runtime(db, &expected_owner, now_iso)?;
        }
        Ok(count)
    })
}

struct DeadDetachedRuntimeCandidate {
    agent_id: JsString,
    data: String,
    owner_agent_id: Option<JsString>,
    owner_session_id: Option<JsString>,
    process_identity: Option<String>,
    session_path: JsString,
}

fn reconcile_dead_detached_agent_runtime(
    db: &Connection,
    expected_owner: &MultiAgentRuntimeOwnershipIdentity,
    now_iso: &str,
) -> Result<i64, ControlDbError> {
    for _ in 0..3 {
        let recoverable = read_recoverable_multi_agent_runtime(db, expected_owner, false)?;
        let (agent, agent_data) = match recoverable {
            RecoverableMultiAgentRuntime::Ok { agent, agent_data } => (agent, agent_data),
            RecoverableMultiAgentRuntime::Err(_) => return Ok(0),
        };
        let plan = prepare_dead_runtime_recovery((agent, agent_data), expected_owner, now_iso)?;
        if let Some(result) = persist_dead_runtime_recovery(db, expected_owner, &plan, None)? {
            return Ok(i64::from(matches!(
                result,
                RecoverDeadMultiAgentRuntimeResult::Ok { .. }
            )));
        }
    }
    Ok(0)
}

fn read_dead_detached_runtime_candidates(db: &Connection) -> Result<Vec<DeadDetachedRuntimeCandidate>, ControlDbError> {
    let mut statement = db.prepare(
        "SELECT agents.session_path, agents.agent_id, agents.data, owners.process_identity,
			 owners.owner_session_id, owners.owner_agent_id
			 FROM multi_agent_agents AS agents
			 JOIN multi_agent_runtime_owners AS owners
			 ON owners.session_path = agents.session_path AND owners.agent_id = agents.agent_id
			 WHERE json_valid(agents.data)
			 AND json_extract(agents.data, '$.lifecycle') IN ('running', 'cancelling')
			 AND json_extract(agents.data, '$.detached') = 1",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok(DeadDetachedRuntimeCandidate {
                session_path: row.get(0)?,
                agent_id: row.get(1)?,
                data: row.get(2)?,
                process_identity: row.get(3)?,
                owner_session_id: row.get(4)?,
                owner_agent_id: row.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

fn read_detached_owner_for_recovery(
    db: &Connection,
    candidate: &DeadDetachedRuntimeCandidate,
) -> Result<Option<MultiAgentRuntimeOwnershipIdentity>, ControlDbError> {
    let (Some(owner_session_id), Some(process_identity)) =
        (candidate.owner_session_id.clone(), candidate.process_identity.clone())
    else {
        return Ok(None);
    };
    let context = js_format!("multi_agent_agents:{}#{}", candidate.session_path, candidate.agent_id);
    let agent = parse_stored_json_object_lossless(&candidate.data, &context)?;
    validate_persisted_agent_payload(&agent, &context)?;
    let process_identity = parse_process_identity(&process_identity)?;
    let worker_adapter = agent
        .get("worker")
        .and_then(Value::as_object)
        .and_then(|worker| worker.get("adapter"))
        .and_then(Value::as_string);
    let worker_handle = agent
        .get("worker")
        .and_then(Value::as_object)
        .and_then(|worker| worker.get("handleId"))
        .and_then(Value::as_string)
        .cloned();
    if worker_adapter.is_none_or(|adapter| adapter != "runtime")
        || worker_handle
            .as_ref()
            .is_none_or(|handle| handle != &JsString::from(process_identity.pid.to_string()))
    {
        return Ok(None);
    }
    if has_terminal_outbox_record(db, &JsString::from(&candidate.session_path), &candidate.agent_id)? {
        return Ok(None);
    }
    if process_identity_is_alive(&process_identity) {
        return Ok(None);
    }
    Ok(Some(MultiAgentRuntimeOwnershipIdentity {
        agent_id: candidate.agent_id.clone(),
        owner: MultiAgentRuntimeOwnerIdentity {
            agent_id: candidate.owner_agent_id.clone(),
            session_id: owner_session_id,
        },
        process_identity,
        session_path: candidate.session_path.clone().into(),
    }))
}

#[cfg(test)]
mod lossless_integration_tests {
    use super::*;

    #[test]
    fn terminal_replay_distinguishes_missing_payload_from_explicit_null() {
        let input = CommitMultiAgentTerminalMutationInput {
            session_path: "/fixture/s".into(),
            agent_id: "bash_1".into(),
            process_identity: crate::core::runtime_process::ProcessIdentity {
                pid: 42,
                start_time_ticks: 100,
                incarnation: None,
            },
            owner: MultiAgentRuntimeOwnerIdentity {
                session_id: "s".into(),
                agent_id: None,
            },
            terminal_lifecycle: "aborted".into(),
            event_kind: "aborted".into(),
            agent_details: AgentDetails {
                error: Some(JsValue::Null),
                result: None,
            },
            updated_at: "2026-10-21T00:00:00.000Z".into(),
        };
        let missing = js_json::parse_lossless(r#"{"revision":2}"#).unwrap();
        assert_eq!(
            terminal_mutation_replay_result(&input, missing.as_object().unwrap()),
            CommitMultiAgentTerminalMutationResult::Err {
                error: LifecycleCommandError::MutationMismatch
            }
        );
        let null = js_json::parse_lossless(r#"{"revision":2,"error":null}"#).unwrap();
        assert_eq!(
            terminal_mutation_replay_result(&input, null.as_object().unwrap()),
            CommitMultiAgentTerminalMutationResult::Ok { terminal_revision: 2 }
        );
    }

    // Frozen session-control-db.ts spreads the existing agent before changing lifecycle metadata.
    #[test]
    fn cancellation_preserves_raw_agent_fields_in_database_and_result() {
        let directory = tempfile::tempdir().unwrap();
        let database: JsString = directory.path().join("control.sqlite").display().to_string().into();
        let original = js_json::parse_lossless(r#"{"id":"bash_1","displayName":"name\ud800","agentType":"background","lifecycle":"running","revision":1,"createdAt":"","updatedAt":"","cwd":"/repo","permission":{"policy":"on-request","narrowed":true},"result":{"summary":"summary\udfff"},"error":{"message":"error\ud800"},"unrelated":{"\ud800":"raw","�":"replacement","null":null}}"#).unwrap();
        bootstrap_multi_agent_agent(&database, "/fixture/s", &"bash_1".into(), &original).unwrap();
        let identity = crate::core::runtime_process::ProcessIdentity {
            pid: 42,
            start_time_ticks: 100,
            incarnation: None,
        };
        with_control_db(&database, |db| {
            db.execute("INSERT INTO multi_agent_runtime_owners (session_path, agent_id, process_identity, owner_session_id, owner_agent_id) VALUES (?, ?, ?, ?, NULL)",
                params!["/fixture/s", "bash_1", serialize_process_identity(&identity), "s"])?;
            Ok(())
        }).unwrap();
        let result = commit_multi_agent_lifecycle_mutation(
            &database,
            &CommitMultiAgentLifecycleMutationInput {
                session_path: "/fixture/s".into(),
                agent_id: "bash_1".into(),
                process_identity: identity,
                owner: MultiAgentRuntimeOwnerIdentity {
                    session_id: "s".into(),
                    agent_id: None,
                },
                requested_lifecycle: "cancelling".into(),
                updated_at: "2026-10-21T00:00:00.000Z".into(),
                detached_cancellation: Some(DetachedCancellation {
                    output_label: "Bash output".into(),
                    reason: Some(JsString::from_utf16(vec![0xdfff])),
                    suppress_terminal_notification: None,
                }),
            },
        )
        .unwrap();
        let CommitMultiAgentLifecycleMutationResult::Ok { agent } = result else {
            panic!("cancellation should commit");
        };
        let returned = JsValue::Object(agent);
        let stored = with_control_db(&database, |db| read_agent_data_row(db, &"/fixture/s".into(), "bash_1"))
            .unwrap()
            .unwrap();
        let stored = js_json::parse_lossless(&stored).unwrap();
        for key in ["displayName", "result", "error", "unrelated"] {
            assert_eq!(stored.get(key), original.get(key), "stored {key}");
            assert_eq!(returned.get(key), original.get(key), "returned {key}");
        }
        assert_eq!(stored.get("lifecycle"), Some(&JsValue::String("cancelling".into())));
    }

    #[test]
    fn cancellation_wire_preserves_reason_and_omits_absent_reason() {
        let reason = JsString::from_utf16(vec![0xd800, 120, 0xdfff]);
        let input = CommitMultiAgentLifecycleMutationInput {
            session_path: "/fixture/s".into(),
            agent_id: "bash_1".into(),
            process_identity: crate::core::runtime_process::ProcessIdentity {
                pid: 42,
                start_time_ticks: 100,
                incarnation: None,
            },
            owner: MultiAgentRuntimeOwnerIdentity {
                session_id: "s".into(),
                agent_id: None,
            },
            requested_lifecycle: "cancelling".into(),
            updated_at: "2026-10-21T00:00:00.000Z".into(),
            detached_cancellation: None,
        };
        for value in [None, Some(reason.clone())] {
            let cancellation = DetachedCancellation {
                output_label: "Bash output".into(),
                reason: value.clone(),
                suppress_terminal_notification: None,
            };
            let (_, wire) = build_detached_cancellation_message(&input, &cancellation, 2).unwrap();
            let message = js_json::parse_lossless(&wire).unwrap();
            let body = message.get("body").unwrap().as_string().unwrap().to_utf8().unwrap();
            let command = js_json::parse_lossless(&body).unwrap();
            let expected = value.map(JsValue::String);
            assert_eq!(command.get("reason"), expected.as_ref());
        }
    }
}

fn has_terminal_outbox_record(
    db: &Connection,
    session_path: &JsString,
    agent_id: &JsString,
) -> Result<bool, ControlDbError> {
    Ok(db
        .query_row(
            "SELECT 1 FROM multi_agent_terminal_outbox WHERE session_path = ? AND agent_id = ? LIMIT 1",
            params![session_path, agent_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}
