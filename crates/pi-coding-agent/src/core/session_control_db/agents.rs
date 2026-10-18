//! Multi-agent persistence: lifecycle mutations, runtime ownership, outbox, counters.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::mailbox::{PreparedStoredRuntimeMailboxMessage, prepare_stored_runtime_mailbox_message};
use super::{
    ControlDbError, EnqueueStoredRuntimeMailboxMessageInput, MultiAgentRuntimeOwner, MultiAgentRuntimeOwnerIdentity,
    MultiAgentRuntimeOwnership, MultiAgentRuntimeOwnershipIdentity, MultiAgentRuntimeOwnershipRow,
    RuntimeMailboxAddress, RuntimeMailboxListenerRow, RuntimeMailboxMessageKind, get_runtime_process_instance_id,
    parse_agent_snapshot, parse_process_identity, parse_stored_json_object, process_identity_is_alive,
    read_multi_agent_runtime_ownership_row, read_runtime_mailbox_listener_row, require_string_field,
    runtime_mailbox_agent_id_key, serialize_process_identity, trusted_runtime_mailbox_session_path,
    validate_mailbox_payload, validate_persisted_agent_payload, with_control_db, with_immediate_transaction,
};
use crate::core::multi_agent_store::{
    AgentFileReference, AgentLifecycle, AgentMailboxMessage, AgentSnapshot, MailboxMessageKindValue,
    MailboxMessageStatusValue, SteeringCheckpoint, is_active_lifecycle,
};
use crate::utils::js_json;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateFailedMultiAgentChildResult {
    Ok { agent: Box<AgentSnapshot> },
    Err { error: CreateChildError },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateMultiAgentChildWithRuntimeOwnershipResult {
    Ok {
        agent: Box<AgentSnapshot>,
        ownership: Box<MultiAgentRuntimeOwnership>,
    },
    Err {
        error: CreateChildError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateMultiAgentAttachmentResult {
    Ok { agent: Box<AgentSnapshot> },
    Err { error: CreateAttachmentError },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcquireAttachedRuntimeOwnershipResult {
    Ok {
        agent: Box<AgentSnapshot>,
        ownership: Box<MultiAgentRuntimeOwnership>,
    },
    Err {
        error: AcquireAttachedError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitMultiAgentSteeringMutationResult {
    Ok {
        agent: Box<AgentSnapshot>,
        message: Box<AgentMailboxMessage>,
    },
    Err {
        error: SteeringCommandError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitMultiAgentSteeringDeliveryResult {
    Ok {
        agent: Box<AgentSnapshot>,
        message: Box<AgentMailboxMessage>,
    },
    Err {
        error: SteeringCommandError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitMultiAgentLifecycleMutationResult {
    Ok { agent: Map<String, Value> },
    Err { error: LifecycleCommandError },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitMultiAgentTerminalMutationResult {
    Ok { terminal_revision: i64 },
    Err { error: LifecycleCommandError },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitMultiAgentDetachMarkResult {
    Ok { agent: Box<AgentSnapshot> },
    Err { error: LifecycleCommandError },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorRuntimeOwnership {
    pub agent_id: Option<String>,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub session_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcquireMultiAgentRuntimeOwnershipInput {
    pub session_path: String,
    pub agent_id: String,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub now_iso: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcquireAttachedRuntimeOwnershipInput {
    pub session_path: String,
    pub agent_id: String,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub now_iso: String,
    pub supervisor: SupervisorRuntimeOwnership,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateMultiAgentChildWithRuntimeOwnershipInput {
    pub session_path: String,
    pub agent_id: String,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub now_iso: String,
    pub agent: AgentSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateMultiAgentAttachmentInput {
    pub agent: AgentSnapshot,
    pub agent_id: String,
    pub now_iso: String,
    pub session_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateFailedMultiAgentChildInput {
    pub agent: AgentSnapshot,
    pub now_iso: String,
    pub session_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitMultiAgentLifecycleMutationInput {
    pub session_path: String,
    pub agent_id: String,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub requested_lifecycle: String,
    pub updated_at: String,
    pub detached_cancellation: Option<DetachedCancellation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedCancellation {
    pub output_label: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitMultiAgentSteeringMutationInput {
    pub session_path: String,
    pub agent_id: String,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub requested_lifecycle: String,
    pub updated_at: String,
    pub body: String,
    pub file_refs: Option<Vec<AgentFileReference>>,
    pub from_agent_id: String,
    pub recipient: RuntimeMailboxAddress,
    pub target_checkpoint: Option<SteeringCheckpoint>,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitMultiAgentSteeringDeliveryInput {
    pub session_path: String,
    pub agent_id: String,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub requested_lifecycle: String,
    pub updated_at: String,
    pub message_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitMultiAgentDetachMarkInput {
    pub agent_id: String,
    pub owner: MultiAgentRuntimeOwnerIdentity,
    pub process_identity: crate::core::runtime_process::ProcessIdentity,
    pub session_path: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AgentDetails {
    pub error: Option<Value>,
    pub result: Option<Value>,
}

impl AgentDetails {
    fn to_map(&self) -> Map<String, Value> {
        let mut map = Map::new();
        if let Some(error) = &self.error {
            map.insert("error".to_owned(), error.clone());
        }
        if let Some(result) = &self.result {
            map.insert("result".to_owned(), result.clone());
        }
        map
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitMultiAgentTerminalMutationInput {
    pub session_path: String,
    pub agent_id: String,
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
    Delivered,
    Pending,
    Poisoned,
}

impl TerminalOutboxStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Claimed => "claimed",
            Self::Delivered => "delivered",
            Self::Pending => "pending",
            Self::Poisoned => "poisoned",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultiAgentTerminalOutboxRecord {
    pub session_path: String,
    pub agent_id: String,
    pub terminal_revision: i64,
    pub event_kind: String,
    pub status: TerminalOutboxStatus,
    pub claim_id: Option<String>,
    pub attempt_count: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClaimMultiAgentTerminalOutboxOptions {
    pub max_attempts: Option<i64>,
    pub session_path: Option<String>,
    pub stale_claim_before: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoverDeadMultiAgentRuntimeInput {
    pub expected_owner: MultiAgentRuntimeOwnershipIdentity,
    pub now_iso: String,
    pub supervisor: SupervisorRuntimeOwnership,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RecoverDeadMultiAgentRuntimeResult {
    Ok {
        agent: Map<String, Value>,
        terminal_revision: i64,
    },
    Err {
        error: RecoverDeadRuntimeError,
    },
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetachedArtifactAgentRecord {
    pub agent: AgentSnapshot,
    pub session_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiAgentCounters {
    pub next_agent_number: i64,
    pub next_message_number: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiAgentPersistedState {
    pub agents: Vec<Value>,
    pub mailbox_messages: Vec<Value>,
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
    agent_id: String,
    attempt_count: i64,
    event_kind: String,
    session_path: String,
    terminal_revision: i64,
}

/// `claimMultiAgentTerminalOutbox`.
pub fn claim_multi_agent_terminal_outbox(
    control_db_path: &str,
    claim_id: &str,
    now_iso: &str,
    options: &ClaimMultiAgentTerminalOutboxOptions,
) -> Result<Option<MultiAgentTerminalOutboxRecord>, ControlDbError> {
    let max_attempts = options.max_attempts.unwrap_or(5);
    with_control_db(control_db_path, |db| {
        if !has_eligible_terminal_outbox_work(db, max_attempts, options)? {
            return Ok(None);
        }
        let row = with_immediate_transaction(db, |db| {
            if let Some(stale_claim_before) = &options.stale_claim_before {
                recover_stale_terminal_outbox_claims(
                    db,
                    max_attempts,
                    now_iso,
                    options.session_path.as_deref(),
                    stale_claim_before,
                )?;
            }
            claim_next_terminal_outbox_row(db, claim_id, now_iso, max_attempts, options.session_path.as_deref())
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
    claim_id: &str,
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
    control_db_path: &str,
    record: &MultiAgentTerminalOutboxRecord,
    error: &str,
    now_iso: &str,
    max_attempts: i64,
) -> Result<bool, ControlDbError> {
    let status = if record.attempt_count >= max_attempts {
        TerminalOutboxStatus::Poisoned
    } else {
        TerminalOutboxStatus::Pending
    };
    update_claimed_terminal_outbox(control_db_path, record, status, now_iso, Some(error))
}

/// `deliverMultiAgentTerminalOutbox`.
pub fn deliver_multi_agent_terminal_outbox(
    control_db_path: &str,
    record: &MultiAgentTerminalOutboxRecord,
    now_iso: &str,
) -> Result<bool, ControlDbError> {
    update_claimed_terminal_outbox(control_db_path, record, TerminalOutboxStatus::Delivered, now_iso, None)
}

fn update_claimed_terminal_outbox(
    control_db_path: &str,
    record: &MultiAgentTerminalOutboxRecord,
    status: TerminalOutboxStatus,
    now_iso: &str,
    error: Option<&str>,
) -> Result<bool, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let delivered_at = (status == TerminalOutboxStatus::Delivered).then_some(now_iso);
        Ok(db.execute(
            "UPDATE multi_agent_terminal_outbox SET status = ?, claim_id = NULL, claimed_at = NULL, delivered_at = ?, last_error = ?, updated_at = ? WHERE session_path = ? AND agent_id = ? AND terminal_revision = ? AND event_kind = ? AND status = 'claimed' AND claim_id = ?",
            params![
                status.as_str(),
                delivered_at,
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
pub fn cleanup_multi_agent_terminal_outbox(control_db_path: &str, older_than: &str) -> Result<i64, ControlDbError> {
    with_control_db(control_db_path, |db| {
        Ok(db.execute(
            "DELETE FROM multi_agent_terminal_outbox
					 WHERE status IN ('delivered', 'poisoned') AND updated_at < ?",
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
    control_db_path: &str,
    input: &CommitMultiAgentSteeringMutationInput,
) -> Result<CommitMultiAgentSteeringMutationResult, ControlDbError> {
    with_control_db(control_db_path, |db| {
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
            agent_id: input.owner.agent_id.clone(),
            session_id: input.owner.session_id.clone(),
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
    if !steering_mutation_has_authority(&agent, ownership.as_ref(), &sender_listener, &recipient_listener, input) {
        return Ok(SteeringMutationPreflight::Result(Box::new(
            CommitMultiAgentSteeringMutationResult::Err {
                error: SteeringCommandError::MutationMismatch,
            },
        )));
    }
    let mut updated_agent = agent.clone();
    updated_agent.lifecycle = AgentLifecycle::SteeringPending;
    updated_agent.revision += 1;
    updated_agent.updated_at = input.updated_at.clone();
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
    let data: Option<String> = db
        .query_row(
            "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![input.session_path, input.agent_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(data) = data else {
        return Ok(None);
    };
    let context = format!("multi_agent_agents:{}#{}", input.session_path, input.agent_id);
    let agent = parse_stored_json_object(&data, &context)?;
    let agent = parse_agent_snapshot(&agent, &context)?;
    Ok(Some((agent, data)))
}

fn steering_mutation_has_authority(
    agent: &AgentSnapshot,
    ownership: Option<&MultiAgentRuntimeOwnershipRow>,
    sender_listener: &RuntimeMailboxListenerRow,
    recipient_listener: &RuntimeMailboxListenerRow,
    input: &CommitMultiAgentSteeringMutationInput,
) -> bool {
    let identity = MultiAgentRuntimeOwnershipIdentity {
        session_path: input.session_path.clone(),
        agent_id: input.agent_id.clone(),
        process_identity: input.process_identity.clone(),
        owner: MultiAgentRuntimeOwnerIdentity {
            session_id: input.owner.session_id.clone(),
            agent_id: input.owner.agent_id.clone(),
        },
    };
    if !super::runtime_owner_matches(ownership, &identity) {
        return false;
    }
    let sender_path_matches = input.owner.agent_id.is_some()
        || trusted_runtime_mailbox_session_path(sender_listener).as_deref() == Some(input.session_path.as_str());
    let expected_sender_id = input.owner.agent_id.as_deref().unwrap_or("supervisor");
    input.from_agent_id == expected_sender_id
        && agent.parent_id.as_deref() == Some(input.owner.agent_id.as_deref().unwrap_or("main"))
        && input.recipient.agent_id.as_deref() == Some(input.agent_id.as_str())
        && agent
            .transcript
            .as_ref()
            .map(|transcript| transcript.session_id.as_str())
            == Some(input.recipient.session_id.as_str())
        && sender_listener.pid == std::process::id()
        && sender_listener.runtime_instance_id.as_deref() == Some(get_runtime_process_instance_id())
        && sender_path_matches
        && runtime_listener_matches_process_identity(
            Some(recipient_listener),
            ownership.and_then(|ownership| ownership.process_identity.as_deref()),
        )
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
        let message_id = format!("message_{allocated}");
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
            &format!("message_{message_number}"),
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
            runtime_mailbox_agent_id_key(input.owner.agent_id.as_deref()),
            plan.sender_listener.pid,
            plan.sender_listener.runtime_instance_id,
            plan.sender_listener.session_path,
            plan.sender_listener.session_path_asserted_at,
            plan.sender_listener.updated_at,
            input.recipient.session_id,
            runtime_mailbox_agent_id_key(input.recipient.agent_id.as_deref()),
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
    message_id: &str,
) -> Result<AgentMailboxMessage, ControlDbError> {
    Ok(AgentMailboxMessage {
        body: Some(input.body.clone()),
        created_at: input.updated_at.clone(),
        file_refs: input.file_refs.clone(),
        from_agent_id: input.from_agent_id.clone(),
        id: message_id.to_owned(),
        kind: MailboxMessageKindValue::Steer,
        status: MailboxMessageStatusValue::Pending,
        target_checkpoint: input.target_checkpoint,
        thread_id: input.thread_id.clone(),
        to_agent_id: input.agent_id.clone(),
        updated_at: input.updated_at.clone(),
        error: None,
    })
}

fn prepare_steering_mailbox_message(
    input: &CommitMultiAgentSteeringMutationInput,
) -> Result<PreparedSteeringMailboxMessage, ControlDbError> {
    let message = build_steering_mailbox_message(input, STEERING_MESSAGE_ID_PLACEHOLDER)?;
    let message_value = serde_json::to_value(&message).map_err(|error| ControlDbError::new(error.to_string()))?;
    let mailbox_input = EnqueueStoredRuntimeMailboxMessageInput {
        kind: RuntimeMailboxMessageKind::Steer,
        message: message_value,
        recipient: input.recipient.clone(),
        sender: RuntimeMailboxAddress {
            agent_id: input.owner.agent_id.clone(),
            session_id: input.owner.session_id.clone(),
        },
        store_ref: super::RuntimeMailboxStoreRef {
            message_id: message.id.clone(),
            session_path: input.session_path.clone(),
        },
        updated_at: Some(input.updated_at.clone()),
    };
    let serialized = prepare_stored_runtime_mailbox_message(&mailbox_input)?.serialized;
    let serialized_id = js_json::stringify(&Value::String(STEERING_MESSAGE_ID_PLACEHOLDER.to_owned()));
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
    session_path: &str,
    message_id: &str,
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
    control_db_path: &str,
    input: &CommitMultiAgentDetachMarkInput,
) -> Result<CommitMultiAgentDetachMarkResult, ControlDbError> {
    with_control_db(control_db_path, |db| {
        for _ in 0..MULTI_AGENT_DETACH_MARK_MAX_ATTEMPTS {
            let data: Option<String> = db
                .query_row(
                    "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
                    params![input.session_path, input.agent_id],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(data) = data else {
                return Ok(CommitMultiAgentDetachMarkResult::Err {
                    error: LifecycleCommandError::AgentNotFound,
                });
            };
            let context = format!("multi_agent_agents:{}#{}", input.session_path, input.agent_id);
            let agent = parse_stored_json_object(&data, &context)?;
            let mut agent_snapshot = parse_agent_snapshot(&agent, &context)?;
            let ownership = read_multi_agent_runtime_ownership_row(db, &input.session_path, &input.agent_id)?;
            let identity = MultiAgentRuntimeOwnershipIdentity {
                session_path: input.session_path.clone(),
                agent_id: input.agent_id.clone(),
                process_identity: input.process_identity.clone(),
                owner: input.owner.clone(),
            };
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
            agent_snapshot.updated_at = input.updated_at.clone();
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

struct SteeringDeliveryPlan {
    agent_data: String,
    message_data: String,
    process_identity_data: String,
    updated_agent: AgentSnapshot,
    updated_agent_data: String,
    updated_message: AgentMailboxMessage,
    updated_message_data: String,
}

/// `commitMultiAgentSteeringDelivery`.
pub fn commit_multi_agent_steering_delivery(
    control_db_path: &str,
    input: &CommitMultiAgentSteeringDeliveryInput,
) -> Result<CommitMultiAgentSteeringDeliveryResult, ControlDbError> {
    with_control_db(control_db_path, |db| {
        for _ in 0..MULTI_AGENT_STEERING_DELIVERY_MAX_ATTEMPTS {
            let plan = match read_steering_delivery_plan(db, input)? {
                Ok(plan) => plan,
                Err(error) => return Ok(CommitMultiAgentSteeringDeliveryResult::Err { error }),
            };
            let persisted = with_immediate_transaction(db, |db| persist_steering_delivery(db, input, &plan))?;
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

fn read_steering_delivery_plan(
    db: &Connection,
    input: &CommitMultiAgentSteeringDeliveryInput,
) -> Result<Result<SteeringDeliveryPlan, SteeringCommandError>, ControlDbError> {
    let agent_row: Option<String> = db
        .query_row(
            "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![input.session_path, input.agent_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(agent_data) = agent_row else {
        return Ok(Err(SteeringCommandError::AgentNotFound));
    };
    let agent_context = format!("multi_agent_agents:{}#{}", input.session_path, input.agent_id);
    let agent_object = parse_stored_json_object(&agent_data, &agent_context)?;
    let agent = parse_agent_snapshot(&agent_object, &agent_context)?;
    let ownership = read_multi_agent_runtime_ownership_row(db, &input.session_path, &input.agent_id)?;
    let identity = MultiAgentRuntimeOwnershipIdentity {
        session_path: input.session_path.clone(),
        agent_id: input.agent_id.clone(),
        process_identity: input.process_identity.clone(),
        owner: input.owner.clone(),
    };
    if !super::runtime_owner_matches(ownership.as_ref(), &identity) {
        return Ok(Err(SteeringCommandError::MutationMismatch));
    }
    if !can_persist_lifecycle_transition(&agent.lifecycle, &input.requested_lifecycle) {
        return Ok(Err(SteeringCommandError::InvalidTransition));
    }
    let message_data: Option<String> = db
        .query_row(
            "SELECT data FROM multi_agent_mailbox_messages WHERE session_path = ? AND message_id = ?",
            params![input.session_path, input.message_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(message_data) = message_data else {
        return Ok(Err(SteeringCommandError::MessageNotFound));
    };
    let message_context = format!(
        "multi_agent_mailbox_messages:{}#{}",
        input.session_path, input.message_id
    );
    let mut message = parse_stored_json_object(&message_data, &message_context)?;
    validate_mailbox_payload(&Value::Object(message.clone()), &message_context)?;
    let is_steering_message = message.get("kind").and_then(Value::as_str) == Some("steer");
    let addressed_to_agent = message.get("toAgentId").and_then(Value::as_str) == Some(input.agent_id.as_str());
    if !is_steering_message || !addressed_to_agent {
        return Ok(Err(SteeringCommandError::MessageNotFound));
    }
    let mut updated_agent_object = agent_object.clone();
    updated_agent_object.insert("lifecycle".to_owned(), Value::String(input.requested_lifecycle.clone()));
    updated_agent_object.insert("revision".to_owned(), Value::from(agent.revision + 1));
    updated_agent_object.insert("updatedAt".to_owned(), Value::String(input.updated_at.clone()));
    let updated_agent: AgentSnapshot = serde_json::from_value(Value::Object(updated_agent_object))
        .map_err(|error| ControlDbError::new(error.to_string()))?;
    message.insert("status".to_owned(), Value::String("delivered".to_owned()));
    message.insert("updatedAt".to_owned(), Value::String(input.updated_at.clone()));
    let updated_message: AgentMailboxMessage = serde_json::from_value(Value::Object(message.clone()))
        .map_err(|error| ControlDbError::new(error.to_string()))?;
    Ok(Ok(SteeringDeliveryPlan {
        agent_data,
        message_data,
        process_identity_data: serialize_process_identity(&input.process_identity),
        updated_agent_data: super::serialize_agent(&updated_agent)?,
        updated_message_data: js_json::stringify(&Value::Object(message)),
        updated_agent,
        updated_message,
    }))
}

fn persist_steering_delivery(
    db: &Connection,
    input: &CommitMultiAgentSteeringDeliveryInput,
    plan: &SteeringDeliveryPlan,
) -> Result<bool, ControlDbError> {
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
    let message_updated = db.execute(
        "UPDATE multi_agent_mailbox_messages SET data = ?, updated_at = ?
		 WHERE session_path = ? AND message_id = ? AND data = ?",
        params![
            plan.updated_message_data,
            input.updated_at,
            input.session_path,
            input.message_id,
            plan.message_data
        ],
    )?;
    if message_updated != 1 {
        return Err(ControlDbError::new(format!(
            "Steering message changed during delivery {}#{}",
            input.session_path, input.message_id
        )));
    }
    Ok(true)
}

// ---------------------------------------------------------------------------
// Lifecycle mutation
// ---------------------------------------------------------------------------

/// `commitMultiAgentLifecycleMutation`.
pub fn commit_multi_agent_lifecycle_mutation(
    control_db_path: &str,
    input: &CommitMultiAgentLifecycleMutationInput,
) -> Result<CommitMultiAgentLifecycleMutationResult, ControlDbError> {
    with_control_db(control_db_path, |db| {
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
    updated: Map<String, Value>,
    updated_data: String,
    cancellation: Option<(String, String)>,
    process_identity_data: String,
}

enum LifecycleMutationRead {
    Plan(LifecycleMutationPlan),
    Unchanged(Map<String, Value>),
    Err(LifecycleCommandError),
}

fn read_lifecycle_mutation_plan(
    db: &Connection,
    input: &CommitMultiAgentLifecycleMutationInput,
) -> Result<LifecycleMutationRead, ControlDbError> {
    let data: Option<String> = db
        .query_row(
            "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![input.session_path, input.agent_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(data) = data else {
        return Ok(LifecycleMutationRead::Err(LifecycleCommandError::AgentNotFound));
    };
    let agent = parse_stored_json_object(
        &data,
        &format!("multi_agent_agents:{}#{}", input.session_path, input.agent_id),
    )?;
    let ownership = read_multi_agent_runtime_ownership_row(db, &input.session_path, &input.agent_id)?;
    let identity = MultiAgentRuntimeOwnershipIdentity {
        session_path: input.session_path.clone(),
        agent_id: input.agent_id.clone(),
        process_identity: input.process_identity.clone(),
        owner: input.owner.clone(),
    };
    if !super::runtime_owner_matches(ownership.as_ref(), &identity) {
        return Ok(LifecycleMutationRead::Err(LifecycleCommandError::MutationMismatch));
    }
    let current_lifecycle = agent.get("lifecycle").and_then(Value::as_str).unwrap_or("");
    if current_lifecycle == input.requested_lifecycle {
        return Ok(LifecycleMutationRead::Unchanged(agent));
    }
    if !can_persist_lifecycle_transition_str(current_lifecycle, &input.requested_lifecycle) {
        return Ok(LifecycleMutationRead::Err(LifecycleCommandError::InvalidTransition));
    }
    let revision = agent.get("revision").and_then(Value::as_i64).unwrap_or(0) + 1;
    let mut updated = agent;
    updated.insert("lifecycle".to_owned(), Value::String(input.requested_lifecycle.clone()));
    updated.insert("revision".to_owned(), Value::from(revision));
    updated.insert("updatedAt".to_owned(), Value::String(input.updated_at.clone()));
    let cancellation = input
        .detached_cancellation
        .as_ref()
        .map(|cancellation| build_detached_cancellation_message(input, cancellation, revision))
        .transpose()?;
    let updated_data = js_json::stringify(&Value::Object(updated.clone()));
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
) -> Result<(String, String), ControlDbError> {
    let message_id = format!("detached-cancel:{}:{cancelling_revision}", input.agent_id);
    let body = js_json::stringify(&serde_json::json!({
        "command": "cancel",
        "identity": {
            "jobId": input.agent_id,
            "outputLabel": cancellation.output_label,
            "owner": {
                "sessionId": input.owner.session_id,
                "agentId": input.owner.agent_id,
            },
            "processIdentity": serde_json::to_value(&input.process_identity)
                .map_err(|error| ControlDbError::new(error.to_string()))?,
        },
        "reason": cancellation.reason,
    }));
    let message = js_json::stringify(&serde_json::json!({
        "body": body,
        "fromAgentId": "main",
        "id": message_id,
        "kind": "system",
        "recipientAgentId": input.agent_id,
        "recipientSessionId": input.owner.session_id,
        "senderAgentId": Value::Null,
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
    control_db_path: &str,
    input: &CommitMultiAgentTerminalMutationInput,
) -> Result<CommitMultiAgentTerminalMutationResult, ControlDbError> {
    with_control_db(control_db_path, |db| {
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
    let agent_row: Option<String> = db
        .query_row(
            "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![input.session_path, input.agent_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(agent_data) = agent_row else {
        return Ok(TerminalMutationRead::Err(LifecycleCommandError::AgentNotFound));
    };
    let agent = parse_stored_json_object(
        &agent_data,
        &format!("multi_agent_agents:{}#{}", input.session_path, input.agent_id),
    )?;
    let ownership = read_multi_agent_runtime_ownership_row(db, &input.session_path, &input.agent_id)?;
    let identity = MultiAgentRuntimeOwnershipIdentity {
        session_path: input.session_path.clone(),
        agent_id: input.agent_id.clone(),
        process_identity: input.process_identity.clone(),
        owner: input.owner.clone(),
    };
    if !super::runtime_owner_matches(ownership.as_ref(), &identity) {
        return Ok(TerminalMutationRead::Err(LifecycleCommandError::MutationMismatch));
    }
    let current_lifecycle = agent.get("lifecycle").and_then(Value::as_str).unwrap_or("");
    if current_lifecycle == input.terminal_lifecycle {
        return Ok(TerminalMutationRead::Replay(terminal_mutation_replay_result(
            db, input, &agent,
        )));
    }
    let terminal_revision = agent.get("revision").and_then(Value::as_i64).unwrap_or(0) + 1;
    if !can_persist_terminal_transition(current_lifecycle, &input.terminal_lifecycle) {
        return Ok(TerminalMutationRead::Err(LifecycleCommandError::InvalidTransition));
    }
    if has_active_persisted_descendant(db, &input.session_path, &input.agent_id)? {
        return Ok(TerminalMutationRead::Err(LifecycleCommandError::InvalidTransition));
    }
    let mut updated = agent;
    for (key, value) in input.agent_details.to_map() {
        updated.insert(key, value);
    }
    updated.remove("currentActivity");
    updated.insert("lifecycle".to_owned(), Value::String(input.terminal_lifecycle.clone()));
    updated.insert("revision".to_owned(), Value::from(terminal_revision));
    updated.insert("updatedAt".to_owned(), Value::String(input.updated_at.clone()));
    Ok(TerminalMutationRead::Plan(TerminalMutationPlan {
        agent_data,
        terminal_revision,
        updated_data: js_json::stringify(&Value::Object(updated)),
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
            session_path: &input.session_path,
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
    pub agent_id: &'a str,
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
    session_path: &str,
    ancestor_id: &str,
) -> Result<bool, ControlDbError> {
    let mut statement = db.prepare("SELECT agent_id, data FROM multi_agent_agents WHERE session_path = ?")?;
    let rows = statement
        .query_map(params![session_path], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut agents = std::collections::HashMap::new();
    for (agent_id, data) in rows {
        let agent = parse_stored_json_object(&data, &format!("multi_agent_agents:{session_path}#{agent_id}"))?;
        agents.insert(agent_id, agent);
    }
    for (agent_id, agent) in &agents {
        if !is_nonterminal_lifecycle(agent.get("lifecycle")) {
            continue;
        }
        let mut parent_id = agent.get("parentId").and_then(Value::as_str).map(str::to_owned);
        let mut visited = std::collections::HashSet::from([agent_id.clone()]);
        while let Some(current_parent) = parent_id {
            if !visited.insert(current_parent.clone()) {
                break;
            }
            if current_parent == ancestor_id {
                return Ok(true);
            }
            parent_id = agents
                .get(&current_parent)
                .and_then(|parent| parent.get("parentId"))
                .and_then(Value::as_str)
                .map(str::to_owned);
        }
    }
    Ok(false)
}

fn is_nonterminal_lifecycle(lifecycle: Option<&Value>) -> bool {
    lifecycle
        .and_then(Value::as_str)
        .is_some_and(|lifecycle| !matches!(lifecycle, "completed" | "failed" | "aborted"))
}

fn terminal_mutation_replay_result(
    db: &Connection,
    input: &CommitMultiAgentTerminalMutationInput,
    agent: &Map<String, Value>,
) -> CommitMultiAgentTerminalMutationResult {
    let terminal_revision = agent.get("revision").and_then(Value::as_i64).unwrap_or(0);
    for (key, value) in input.agent_details.to_map() {
        if js_json::stringify(agent.get(&key).unwrap_or(&Value::Null)) != js_json::stringify(&value) {
            return CommitMultiAgentTerminalMutationResult::Err {
                error: LifecycleCommandError::MutationMismatch,
            };
        }
    }
    let outbox = db
        .query_row(
            "SELECT 1 FROM multi_agent_terminal_outbox
			 WHERE session_path = ? AND agent_id = ? AND terminal_revision = ? AND event_kind = ?",
            params![input.session_path, input.agent_id, terminal_revision, input.event_kind],
            |_| Ok(()),
        )
        .optional();
    match outbox {
        Ok(Some(())) => CommitMultiAgentTerminalMutationResult::Ok { terminal_revision },
        _ => CommitMultiAgentTerminalMutationResult::Err {
            error: LifecycleCommandError::MutationMismatch,
        },
    }
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
    parent_id: String,
}

/// `createFailedMultiAgentChild`.
pub fn create_failed_multi_agent_child(
    control_db_path: &str,
    input: &CreateFailedMultiAgentChildInput,
) -> Result<CreateFailedMultiAgentChildResult, ControlDbError> {
    let context = format!("multi_agent_agents:{}#{}", input.session_path, input.agent.id);
    let agent_value = super::agent_value(&input.agent)?;
    let Value::Object(agent_object) = &agent_value else {
        return Err(ControlDbError::new(format!(
            "Invalid persisted agent payload at {context}"
        )));
    };
    validate_persisted_agent_payload(agent_object, &context)?;
    if input.agent.lifecycle != AgentLifecycle::Failed || input.agent.revision != 1 {
        return Err(ControlDbError::new("Failed child creation requires failed revision 1"));
    }
    let Some(parent_id) = input.agent.parent_id.clone() else {
        return Ok(CreateFailedMultiAgentChildResult::Err {
            error: CreateChildError::ParentNotFound,
        });
    };
    let serialized_agent = js_json::stringify(&agent_value);
    with_control_db(control_db_path, |db| {
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
        Err(ControlDbError::new(format!(
            "Failed child parent changed repeatedly {}#{}",
            input.session_path, input.agent.id
        )))
    })
}

fn read_active_parent_snapshot(
    db: &Connection,
    session_path: &str,
    parent_id: &str,
) -> Result<Option<ActiveParentSnapshot>, ControlDbError> {
    if parent_id == "main" {
        return Ok(Some(ActiveParentSnapshot {
            agent: None,
            data: None,
            parent_id: parent_id.to_owned(),
        }));
    }
    let data: Option<String> = db
        .query_row(
            "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![session_path, parent_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(data) = data else {
        return Ok(None);
    };
    let context = format!("multi_agent_agents:{session_path}#{parent_id}");
    let parent = parse_stored_json_object(&data, &context)?;
    validate_persisted_agent_payload(&parent, &context)?;
    if !is_nonterminal_lifecycle(parent.get("lifecycle")) {
        return Ok(None);
    }
    let agent = parse_agent_snapshot(&parent, &context)?;
    Ok(Some(ActiveParentSnapshot {
        agent: Some(agent),
        data: Some(data),
        parent_id: parent_id.to_owned(),
    }))
}

fn multi_agent_agent_exists(db: &Connection, session_path: &str, agent_id: &str) -> Result<bool, ControlDbError> {
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
    session_path: &str,
    agent_id: &str,
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
    control_db_path: &str,
    input: &CreateMultiAgentChildWithRuntimeOwnershipInput,
) -> Result<CreateMultiAgentChildWithRuntimeOwnershipResult, ControlDbError> {
    let context = format!("multi_agent_agents:{}#{}", input.session_path, input.agent_id);
    let agent_value = super::agent_value(&input.agent)?;
    let Value::Object(agent_object) = &agent_value else {
        return Err(ControlDbError::new(format!(
            "Invalid persisted agent payload at {context}"
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
    let Some(parent_id) = input.agent.parent_id.clone() else {
        return Ok(CreateMultiAgentChildWithRuntimeOwnershipResult::Err {
            error: CreateChildError::ParentNotFound,
        });
    };
    let process_identity_data = serialize_process_identity(&input.process_identity);
    let serialized_agent = js_json::stringify(&agent_value);
    with_control_db(control_db_path, |db| {
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
        Err(ControlDbError::new(format!(
            "Child parent changed repeatedly {}#{}",
            input.session_path, input.agent_id
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
    control_db_path: &str,
    input: &CreateMultiAgentAttachmentInput,
) -> Result<CreateMultiAgentAttachmentResult, ControlDbError> {
    let context = format!("multi_agent_agents:{}#{}", input.session_path, input.agent_id);
    let agent_value = super::agent_value(&input.agent)?;
    let Value::Object(agent_object) = &agent_value else {
        return Err(ControlDbError::new(format!(
            "Invalid persisted agent payload at {context}"
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
    let serialized_agent = js_json::stringify(&agent_value);
    with_control_db(control_db_path, |db| {
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
        Err(ControlDbError::new(format!(
            "Attachment parent changed repeatedly {}#{}",
            input.session_path, input.agent_id
        )))
    })
}

fn read_attachment_parent_snapshot(
    db: &Connection,
    session_path: &str,
    agent: &AgentSnapshot,
) -> Result<ActiveParentSnapshot, CreateAttachmentError> {
    let Some(parent_id) = agent.parent_id.clone() else {
        return Ok(ActiveParentSnapshot {
            agent: None,
            data: None,
            parent_id: "main".to_owned(),
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
    control_db_path: &str,
    session_path: &str,
    agent_id: &str,
) -> Result<Option<MultiAgentRuntimeOwnership>, ControlDbError> {
    with_control_db(control_db_path, |db| {
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
    control_db_path: &str,
    input: &AcquireAttachedRuntimeOwnershipInput,
) -> Result<AcquireAttachedRuntimeOwnershipResult, ControlDbError> {
    with_control_db(control_db_path, |db| {
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
            updated_agent.updated_at = input.now_iso.clone();
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
    let identity = MultiAgentRuntimeOwnershipIdentity {
        session_path: input.session_path.clone(),
        agent_id: input.agent_id.clone(),
        process_identity: input.process_identity.clone(),
        owner: input.owner.clone(),
    };
    if let Some(current_ownership) = ownership
        && current_owner_alive
        && super::runtime_owner_matches(Some(current_ownership), &identity)
    {
        return Ok(OwnershipAcquisition::AlreadyOwned(
            super::multi_agent_runtime_ownership_from_row(current_ownership)?,
        ));
    }
    let replaces_current_incarnation = current_identity
        .as_ref()
        .is_some_and(|current| same_process_with_superseded_incarnation(current, &input.process_identity));
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
    let row: Option<String> = db
        .query_row(
            "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![input.session_path, input.agent_id],
            |row| row.get(0),
        )
        .optional()?;
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
    session_path: &str,
    agent_id: &str,
) -> Result<AttachedRuntimeAgentRead, ControlDbError> {
    let data: Option<String> = db
        .query_row(
            "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![session_path, agent_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(data) = data else {
        return Ok(AttachedRuntimeAgentRead::Err(AcquireAttachedError::AgentNotFound));
    };
    let context = format!("multi_agent_agents:{session_path}#{agent_id}");
    let agent = parse_stored_json_object(&data, &context)?;
    match parse_agent_snapshot(&agent, &context) {
        Ok(agent) => Ok(AttachedRuntimeAgentRead::Ok {
            agent: Box::new(agent),
            data,
        }),
        Err(_) => Ok(AttachedRuntimeAgentRead::Err(AcquireAttachedError::InvalidAgent)),
    }
}

fn read_registered_supervisor_session_authority(
    db: &Connection,
    session_path: &str,
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
    session_path: &str,
    supervisor: &SupervisorRuntimeOwnership,
) -> Result<Option<SupervisorSessionAuthoritySnapshot>, ControlDbError> {
    let listener: Option<(Option<String>, Option<String>, Option<String>)> = db
        .query_row(
            "SELECT runtime_instance_id, session_path, session_path_asserted_at
			 FROM runtime_mailbox_listeners
			 WHERE recipient_session_id = ? AND recipient_agent_id_key = ? AND pid = ?",
            params![
                supervisor.session_id,
                runtime_mailbox_agent_id_key(supervisor.agent_id.as_deref()),
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
        if listener_session_path.as_deref() != Some(session_path) || asserted_at.is_none() {
            return Ok(None);
        }
        return Ok(Some(SupervisorSessionAuthoritySnapshot {
            runtime_instance_id,
            session_path: listener_session_path,
            session_path_asserted_at: asserted_at,
            supervisor_agent_data: None,
        }));
    }
    let supervisor_agent: Option<String> = db
        .query_row(
            "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![session_path, supervisor.agent_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(supervisor_agent_data) = supervisor_agent else {
        return Ok(None);
    };
    let snapshot = parse_stored_json_object(
        &supervisor_agent_data,
        &format!(
            "multi_agent_agents:{session_path}#{}",
            supervisor.agent_id.as_deref().unwrap_or_default()
        ),
    )?;
    let agent = parse_agent_snapshot(&snapshot, "multi_agent_agents:supervisor")?;
    if agent
        .transcript
        .as_ref()
        .map(|transcript| transcript.session_id.as_str())
        != Some(supervisor.session_id.as_str())
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
    session_path: &str,
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
                runtime_mailbox_agent_id_key(supervisor.agent_id.as_deref()),
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
    let supervisor_agent: Option<String> = db
        .query_row(
            "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![session_path, supervisor.agent_id],
            |row| row.get(0),
        )
        .optional()?;
    Ok(supervisor_agent == expected.supervisor_agent_data)
}

fn same_process_with_superseded_incarnation(
    current: &crate::core::runtime_process::ProcessIdentity,
    incoming: &crate::core::runtime_process::ProcessIdentity,
) -> bool {
    current.pid == incoming.pid
        && current.start_time_ticks == incoming.start_time_ticks
        && current.incarnation.is_some()
        && current.incarnation != incoming.incarnation
}

fn persist_acquired_runtime_ownership(
    db: &Connection,
    session_path: &str,
    agent_id: &str,
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
    session_path: &str,
    agent_id: &str,
    owner: &MultiAgentRuntimeOwnerIdentity,
    process_identity: &crate::core::runtime_process::ProcessIdentity,
) -> MultiAgentRuntimeOwnership {
    MultiAgentRuntimeOwnership {
        agent_id: agent_id.to_owned(),
        owner: MultiAgentRuntimeOwner {
            agent_id: owner.agent_id.clone(),
            session_id: Some(owner.session_id.clone()),
        },
        process_identity: Some(process_identity.clone()),
        session_path: session_path.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// Metadata updates
// ---------------------------------------------------------------------------

/// `updateMultiAgentAgentActivity`.
pub fn update_multi_agent_agent_activity(
    control_db_path: &str,
    session_path: &str,
    agent_id: &str,
    last_activity: Option<&Value>,
    updated_at: &str,
) -> Result<Option<Value>, ControlDbError> {
    let mut metadata = Map::new();
    if let Some(last_activity) = last_activity {
        metadata.insert("lastActivity".to_owned(), last_activity.clone());
    }
    update_multi_agent_agent_metadata(control_db_path, session_path, agent_id, &metadata, updated_at, None)
}

/// `updateMultiAgentAgentCurrentActivity`.
pub fn update_multi_agent_agent_current_activity(
    control_db_path: &str,
    session_path: &str,
    agent_id: &str,
    current_activity: Option<&Value>,
    updated_at: &str,
    ownership: &crate::core::multi_agent_store::AgentCurrentActivityOwner,
) -> Result<Option<Value>, ControlDbError> {
    let mut metadata = Map::new();
    metadata.insert(
        "currentActivity".to_owned(),
        current_activity.cloned().unwrap_or(Value::Null),
    );
    update_multi_agent_agent_metadata(
        control_db_path,
        session_path,
        agent_id,
        &metadata,
        updated_at,
        Some(ownership),
    )
}

/// `updateMultiAgentAgentSlot`.
pub fn update_multi_agent_agent_slot(
    control_db_path: &str,
    session_path: &str,
    agent_id: &str,
    slot: Option<&Value>,
    updated_at: &str,
) -> Result<Option<Value>, ControlDbError> {
    let mut metadata = Map::new();
    metadata.insert("slot".to_owned(), slot.cloned().unwrap_or(Value::Null));
    update_multi_agent_agent_metadata(control_db_path, session_path, agent_id, &metadata, updated_at, None)
}

/// `updateMultiAgentAgentTranscript`.
pub fn update_multi_agent_agent_transcript(
    control_db_path: &str,
    session_path: &str,
    agent_id: &str,
    transcript: Option<&Value>,
    updated_at: &str,
) -> Result<Option<Value>, ControlDbError> {
    let mut metadata = Map::new();
    metadata.insert("transcript".to_owned(), transcript.cloned().unwrap_or(Value::Null));
    update_multi_agent_agent_metadata(control_db_path, session_path, agent_id, &metadata, updated_at, None)
}

fn update_multi_agent_agent_metadata(
    control_db_path: &str,
    session_path: &str,
    agent_id: &str,
    metadata: &Map<String, Value>,
    updated_at: &str,
    activity_ownership: Option<&crate::core::multi_agent_store::AgentCurrentActivityOwner>,
) -> Result<Option<Value>, ControlDbError> {
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
            let serialized = js_json::stringify(&Value::Object(agent.clone()));
            let update = MetadataUpdate {
                data: &data,
                row_updated_at: &row_updated_at,
                serialized: &serialized,
                updated_at,
                activity_ownership,
            };
            let changed = persist_metadata_update(db, session_path, agent_id, &update)?;
            if changed == 1 {
                return Ok(Some(Value::Object(agent)));
            }
        }
        Err(ControlDbError::new(format!(
            "Multi-agent metadata changed repeatedly while updating {session_path}#{agent_id}"
        )))
    })
}

fn read_metadata_agent_row(
    db: &Connection,
    session_path: &str,
    agent_id: &str,
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
    session_path: &str,
    agent_id: &str,
    data: &str,
    metadata: &Map<String, Value>,
    activity_ownership: Option<&crate::core::multi_agent_store::AgentCurrentActivityOwner>,
) -> Result<Option<Map<String, Value>>, ControlDbError> {
    let context = format!("multi_agent_agents:{session_path}#{agent_id}");
    let agent = parse_stored_json_object(data, &context)?;
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

fn apply_metadata_patch(agent: &mut Map<String, Value>, metadata: &Map<String, Value>, updated_at: &str) {
    for (key, value) in metadata {
        if value.is_null() {
            agent.remove(key);
        } else {
            agent.insert(key.clone(), value.clone());
        }
    }
    agent.insert("updatedAt".to_owned(), Value::String(updated_at.to_owned()));
}

struct MetadataUpdate<'a> {
    data: &'a str,
    row_updated_at: &'a str,
    serialized: &'a str,
    updated_at: &'a str,
    activity_ownership: Option<&'a crate::core::multi_agent_store::AgentCurrentActivityOwner>,
}

fn persist_metadata_update(
    db: &Connection,
    session_path: &str,
    agent_id: &str,
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
    session_path: &str,
    agent_id: &str,
    ownership: &crate::core::multi_agent_store::AgentCurrentActivityOwner,
) -> Result<bool, ControlDbError> {
    let runtime_ownership = read_multi_agent_runtime_ownership_row(db, session_path, agent_id)?;
    Ok(runtime_ownership.is_some_and(|runtime_ownership| {
        runtime_ownership.owner_session_id.as_deref() == Some(ownership.owner_session_id.as_str())
            && runtime_ownership.owner_agent_id.is_none()
            && runtime_ownership.process_identity.as_deref()
                == Some(serialize_process_identity(&ownership.process_identity).as_str())
    }))
}

fn permits_multi_agent_current_activity(agent: &Map<String, Value>, metadata: &Map<String, Value>) -> bool {
    !metadata.contains_key("currentActivity")
        || metadata.get("currentActivity") == Some(&Value::Null)
        || matches!(
            agent.get("lifecycle").and_then(Value::as_str),
            Some("running" | "steering_pending")
        )
}

// ---------------------------------------------------------------------------
// Bootstrap and mailbox upserts
// ---------------------------------------------------------------------------

/// `bootstrapMultiAgentAgent`.
pub fn bootstrap_multi_agent_agent(
    control_db_path: &str,
    session_path: &str,
    id: &str,
    data: &Value,
) -> Result<(), ControlDbError> {
    let context = format!("multi_agent_agents:{session_path}#{id}");
    let Value::Object(object) = data else {
        return Err(ControlDbError::new(format!(
            "Invalid persisted agent payload at {context}"
        )));
    };
    validate_persisted_agent_payload(object, &context)?;
    let serialized = js_json::stringify(data);
    let updated_at = crate::utils::iso_time::now_iso_string();
    with_control_db(control_db_path, |db| {
        if read_multi_agent_runtime_ownership_row(db, session_path, id)?.is_some() {
            return Err(ControlDbError::new(format!(
                "Generic agent upsert cannot mutate process-owned lifecycle row {session_path}#{id}"
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
            return Err(ControlDbError::new(format!(
                "Generic agent upsert cannot mutate process-owned lifecycle row {session_path}#{id}"
            )));
        }
        Ok(())
    })
}

/// `upsertMultiAgentMailboxMessage`.
pub fn upsert_multi_agent_mailbox_message(
    control_db_path: &str,
    session_path: &str,
    id: &str,
    data: &Value,
) -> Result<(), ControlDbError> {
    validate_mailbox_payload(data, &format!("multi_agent_mailbox_messages:{session_path}#{id}"))?;
    with_control_db(control_db_path, |db| {
        for _ in 0..MULTI_AGENT_MAILBOX_UPSERT_MAX_ATTEMPTS {
            let plan = prepare_multi_agent_mailbox_upsert(db, session_path, id, data)?;
            if plan.existing.as_ref().map(|existing| existing.data.as_str()) == Some(plan.serialized.as_str()) {
                return Ok(());
            }
            if commit_multi_agent_mailbox_upsert(db, session_path, id, &plan)? {
                return Ok(());
            }
        }
        Err(ControlDbError::new(format!(
            "Mailbox message changed repeatedly while updating {session_path}#{id}"
        )))
    })
}

struct ParentRequestTargetSnapshot {
    agent_id: String,
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
    session_path: &str,
    id: &str,
    data: &Value,
) -> Result<MultiAgentMailboxUpsertPlan, ControlDbError> {
    let parent_target = read_parent_request_target_snapshot(db, session_path, data)?;
    let existing = read_multi_agent_mailbox_row_snapshot(db, session_path, id)?;
    let mut serialized = js_json::stringify(data);
    if let Some(existing) = &existing {
        let previous = super::parse_json_object(&existing.data);
        let next = super::parse_json_object(&serialized);
        let (Some(previous), Some(next)) = (previous, next) else {
            return Err(ControlDbError::new(format!(
                "Mailbox message ID collision: {session_path}#{id}"
            )));
        };
        if !super::mailbox::same_mailbox_message_identity(&previous, &next, id) {
            return Err(ControlDbError::new(format!(
                "Mailbox message ID collision: {session_path}#{id}"
            )));
        }
        serialized = js_json::stringify(&Value::Object(merge_canonical_mailbox_update(&previous, &next)));
    }
    Ok(MultiAgentMailboxUpsertPlan {
        existing,
        parent_target,
        serialized,
    })
}

fn commit_multi_agent_mailbox_upsert(
    db: &Connection,
    session_path: &str,
    id: &str,
    plan: &MultiAgentMailboxUpsertPlan,
) -> Result<bool, ControlDbError> {
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
            plan.parent_target.as_ref().map(|target| target.agent_id.as_str()),
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
    session_path: &str,
    id: &str,
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

fn merge_canonical_mailbox_update(previous: &Map<String, Value>, next: &Map<String, Value>) -> Map<String, Value> {
    let mut merged = next.clone();
    for key in [
        "recipientAgentId",
        "recipientSessionId",
        "senderAgentId",
        "senderSessionId",
    ] {
        if let Some(value) = previous.get(key) {
            merged.insert(key.to_owned(), value.clone());
        }
    }
    if next.get("status").and_then(Value::as_str) == Some("pending")
        && previous.get("status").and_then(Value::as_str) == Some("claimed")
    {
        if let Some(value) = previous.get("claimedAt") {
            merged.insert("claimedAt".to_owned(), value.clone());
        }
        if let Some(value) = previous.get("claimantProcessIdentity") {
            merged.insert("claimantProcessIdentity".to_owned(), value.clone());
        }
        merged.insert("status".to_owned(), Value::String("claimed".to_owned()));
    }
    merged
}

fn read_parent_request_target_snapshot(
    db: &Connection,
    session_path: &str,
    data: &Value,
) -> Result<Option<ParentRequestTargetSnapshot>, ControlDbError> {
    let Value::Object(payload) = data else {
        return Ok(None);
    };
    if payload.get("kind").and_then(Value::as_str) != Some("parent_request") {
        return Ok(None);
    }
    let from_agent_id = require_string_field(payload, "fromAgentId", "parent_request")?;
    let to_agent_id = require_string_field(payload, "toAgentId", "parent_request")?;
    let row: Option<String> = db
        .query_row(
            "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![session_path, from_agent_id],
            |row| row.get(0),
        )
        .optional()?;
    let sender = row.as_deref().and_then(super::parse_json_object);
    if sender
        .as_ref()
        .and_then(|sender| sender.get("parentId"))
        .and_then(Value::as_str)
        != Some(to_agent_id.as_str())
    {
        return Err(ControlDbError::new(format!(
            "Invalid parent request target at {session_path}#{from_agent_id}"
        )));
    }
    Ok(Some(ParentRequestTargetSnapshot {
        agent_id: from_agent_id,
        data: row.unwrap_or_default(),
    }))
}

/// `getMultiAgentMailboxMessageStatus`.
pub fn get_multi_agent_mailbox_message_status(
    control_db_path: &str,
    session_path: &str,
    message_id: &str,
) -> Result<Option<String>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let data: Option<String> = db
            .query_row(
                "SELECT data FROM multi_agent_mailbox_messages WHERE session_path = ? AND message_id = ?",
                params![session_path, message_id],
                |row| row.get(0),
            )
            .optional()?;
        Ok(data
            .as_deref()
            .and_then(super::parse_json_object)
            .and_then(|parsed| parsed.get("status").and_then(Value::as_str).map(str::to_owned)))
    })
}

/// `markMultiAgentMailboxMessageDelivered`.
pub fn mark_multi_agent_mailbox_message_delivered(
    control_db_path: &str,
    session_path: &str,
    message_id: &str,
) -> Result<bool, ControlDbError> {
    update_multi_agent_mailbox_message_status(control_db_path, session_path, message_id, "delivered", None)
}

/// `markMultiAgentMailboxMessageFailed`.
pub fn mark_multi_agent_mailbox_message_failed(
    control_db_path: &str,
    session_path: &str,
    message_id: &str,
    error: &str,
) -> Result<bool, ControlDbError> {
    update_multi_agent_mailbox_message_status(control_db_path, session_path, message_id, "failed", Some(error))
}

fn update_multi_agent_mailbox_message_status(
    control_db_path: &str,
    session_path: &str,
    message_id: &str,
    status: &str,
    error: Option<&str>,
) -> Result<bool, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let data: Option<String> = db
            .query_row(
                "SELECT data FROM multi_agent_mailbox_messages WHERE session_path = ? AND message_id = ?",
                params![session_path, message_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(data) = data else {
            return Ok(false);
        };
        let Some(mut parsed) = super::parse_json_object(&data) else {
            return Ok(false);
        };
        if parsed.get("status").and_then(Value::as_str) != Some("pending") {
            return Ok(false);
        }
        let now = crate::utils::iso_time::now_iso_string();
        parsed.insert("status".to_owned(), Value::String(status.to_owned()));
        parsed.insert("updatedAt".to_owned(), Value::String(now.clone()));
        if let Some(error) = error {
            parsed.insert("error".to_owned(), Value::String(error.to_owned()));
        }
        db.execute(
            "UPDATE multi_agent_mailbox_messages
			 SET data = ?, updated_at = ?
			 WHERE session_path = ? AND message_id = ?",
            params![
                js_json::stringify(&Value::Object(parsed)),
                now,
                session_path,
                message_id
            ],
        )?;
        Ok(true)
    })
}

// ---------------------------------------------------------------------------
// Counters and reads
// ---------------------------------------------------------------------------

/// `writeMultiAgentCounters`.
pub fn write_multi_agent_counters(
    control_db_path: &str,
    session_path: &str,
    counters: &MultiAgentCounters,
) -> Result<(), ControlDbError> {
    with_control_db(control_db_path, |db| {
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
    control_db_path: &str,
    session_path: &str,
    counter_name: MultiAgentCounterName,
) -> Result<i64, ControlDbError> {
    with_control_db(control_db_path, |db| {
        allocate_multi_agent_counter_with_db(db, session_path, counter_name)
    })
}

pub(crate) fn allocate_multi_agent_counter_with_db(
    db: &Connection,
    session_path: &str,
    counter_name: MultiAgentCounterName,
) -> Result<i64, ControlDbError> {
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
        ControlDbError::new(format!(
            "Multi-agent {} counter allocation returned no value for {session_path}",
            counter_name.as_str()
        ))
    })
}

/// `readMultiAgentAgent`.
pub fn read_multi_agent_agent(
    control_db_path: &str,
    session_path: &str,
    agent_id: &str,
) -> Result<Option<AgentSnapshot>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let data: Option<String> = db
            .query_row(
                "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
                params![session_path, agent_id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(data) = data else {
            return Ok(None);
        };
        let context = format!("multi_agent_agents:{session_path}#{agent_id}");
        let agent = parse_stored_json_object(&data, &context)?;
        Ok(Some(parse_agent_snapshot(&agent, &context)?))
    })
}

/// `readMultiAgentAgents`.
pub fn read_multi_agent_agents(
    control_db_path: &str,
    session_path: &str,
    agent_ids: &[String],
) -> Result<Vec<AgentSnapshot>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let mut agents = Vec::new();
        for agent_id in agent_ids {
            let data: Option<String> = db
                .query_row(
                    "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
                    params![session_path, agent_id],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(data) = data else {
                continue;
            };
            let context = format!("multi_agent_agents:{session_path}#{agent_id}");
            let agent = parse_stored_json_object(&data, &context)?;
            agents.push(parse_agent_snapshot(&agent, &context)?);
        }
        Ok(agents)
    })
}

/// `listDetachedArtifactAgentsUpdatedAtOrBefore`.
pub fn list_detached_artifact_agents_updated_at_or_before(
    control_db_path: &str,
    updated_at_cutoff: &str,
) -> Result<Vec<DetachedArtifactAgentRecord>, ControlDbError> {
    with_control_db(control_db_path, |db| {
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
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut records = Vec::new();
        for (session_path, agent_id, data) in rows {
            let context = format!("multi_agent_agents:{session_path}#{agent_id}");
            let agent = parse_stored_json_object(&data, &context)?;
            records.push(DetachedArtifactAgentRecord {
                agent: parse_agent_snapshot(&agent, &context)?,
                session_path,
            });
        }
        Ok(records)
    })
}

/// `readMultiAgentState`.
pub fn read_multi_agent_state(
    control_db_path: &str,
    session_path: &str,
) -> Result<Option<MultiAgentPersistedState>, ControlDbError> {
    with_control_db(control_db_path, |db| {
        let read_rows = |table: &str| -> Result<Vec<Value>, ControlDbError> {
            let mut statement = db.prepare(&format!(
                "SELECT data FROM {table} WHERE session_path = ? ORDER BY rowid"
            ))?;
            let rows = statement
                .query_map(params![session_path], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            let mut values = Vec::with_capacity(rows.len());
            for (index, data) in rows.iter().enumerate() {
                let context = format!("{table}:{session_path}[{index}]");
                let parsed = parse_stored_json_object(data, &context)?;
                if table == "multi_agent_agents" {
                    validate_persisted_agent_payload(&parsed, &context)?;
                } else {
                    validate_mailbox_payload(&Value::Object(parsed.clone()), &context)?;
                }
                values.push(Value::Object(parsed));
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
    supervisor_session_id: String,
    supervisor_agent_id: Option<String>,
    supervisor_agent_data: Option<String>,
}

struct DeadRuntimeRecoveryPlan {
    agent_data: String,
    process_identity_data: String,
    terminal_revision: i64,
    terminal_transport: Option<PreparedTerminalTransport>,
    updated_agent: Map<String, Value>,
    updated_agent_data: String,
}

pub(crate) struct PreparedTerminalTransport {
    input: EnqueueStoredRuntimeMailboxMessageInput,
    prepared: PreparedStoredRuntimeMailboxMessage,
}

enum RecoverableMultiAgentRuntime {
    Ok {
        agent: Map<String, Value>,
        agent_data: String,
    },
    Err(RecoverDeadRuntimeError),
}

/// `recoverDeadMultiAgentRuntime`.
pub fn recover_dead_multi_agent_runtime(
    control_db_path: &str,
    input: &RecoverDeadMultiAgentRuntimeInput,
) -> Result<RecoverDeadMultiAgentRuntimeResult, ControlDbError> {
    with_control_db(control_db_path, |db| {
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
    session_path: &str,
    supervisor: &SupervisorRuntimeOwnership,
) -> Result<Option<SupervisorRecoveryAuthority>, ControlDbError> {
    if !process_identity_is_alive(&supervisor.process_identity) {
        return Ok(None);
    }
    let listener = read_runtime_mailbox_listener_row(
        db,
        &RuntimeMailboxAddress {
            agent_id: supervisor.agent_id.clone(),
            session_id: supervisor.session_id.clone(),
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
        if listener.session_path.as_deref() != Some(session_path) || listener.session_path_asserted_at.is_none() {
            return Ok(None);
        }
        return Ok(Some(SupervisorRecoveryAuthority {
            listener,
            supervisor_session_id: supervisor.session_id.clone(),
            supervisor_agent_id: supervisor.agent_id.clone(),
            supervisor_agent_data: None,
        }));
    }
    let supervisor_row: Option<String> = db
        .query_row(
            "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![session_path, supervisor.agent_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(supervisor_agent_data) = supervisor_row else {
        return Ok(None);
    };
    let snapshot = parse_stored_json_object(
        &supervisor_agent_data,
        &format!(
            "multi_agent_agents:{session_path}#{}",
            supervisor.agent_id.as_deref().unwrap_or_default()
        ),
    )?;
    let agent: AgentSnapshot =
        serde_json::from_value(Value::Object(snapshot)).map_err(|error| ControlDbError::new(error.to_string()))?;
    if agent
        .transcript
        .as_ref()
        .map(|transcript| transcript.session_id.as_str())
        != Some(supervisor.session_id.as_str())
        || !is_active_lifecycle(agent.lifecycle)
    {
        return Ok(None);
    }
    Ok(Some(SupervisorRecoveryAuthority {
        listener,
        supervisor_session_id: supervisor.session_id.clone(),
        supervisor_agent_id: supervisor.agent_id.clone(),
        supervisor_agent_data: Some(supervisor_agent_data),
    }))
}

fn read_recoverable_multi_agent_runtime(
    db: &Connection,
    expected_owner: &MultiAgentRuntimeOwnershipIdentity,
    verify_owner_liveness: bool,
) -> Result<RecoverableMultiAgentRuntime, ControlDbError> {
    let data: Option<String> = db
        .query_row(
            "SELECT data FROM multi_agent_agents WHERE session_path = ? AND agent_id = ?",
            params![expected_owner.session_path, expected_owner.agent_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(data) = data else {
        return Ok(RecoverableMultiAgentRuntime::Err(
            RecoverDeadRuntimeError::AgentNotFound,
        ));
    };
    let agent = parse_stored_json_object(
        &data,
        &format!(
            "multi_agent_agents:{}#{}",
            expected_owner.session_path, expected_owner.agent_id
        ),
    )?;
    let lifecycle = agent.get("lifecycle").and_then(Value::as_str).and_then(parse_lifecycle);
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

fn parse_lifecycle(value: &str) -> Option<AgentLifecycle> {
    serde_json::from_value(Value::String(value.to_owned())).ok()
}

fn prepare_dead_runtime_recovery(
    recoverable: (Map<String, Value>, String),
    expected_owner: &MultiAgentRuntimeOwnershipIdentity,
    now_iso: &str,
) -> Result<DeadRuntimeRecoveryPlan, ControlDbError> {
    let (agent, agent_data) = recoverable;
    let revision = agent.get("revision").and_then(Value::as_i64).unwrap_or(0);
    let terminal_revision = revision + 1;
    let cancelling = agent.get("lifecycle").and_then(Value::as_str) == Some("cancelling");
    let mut updated_agent = agent.clone();
    updated_agent.insert(
        "error".to_owned(),
        serde_json::json!({
            "code": "lost_runtime",
            "message": if cancelling {
                "Cancellation requested; agent owner process exited before terminal confirmation."
            } else {
                "Agent owner process exited before terminal confirmation."
            },
        }),
    );
    updated_agent.insert(
        "lifecycle".to_owned(),
        Value::String(if cancelling { "aborted" } else { "failed" }.to_owned()),
    );
    let tool_call_id = agent
        .get("worker")
        .and_then(Value::as_object)
        .and_then(|worker| worker.get("toolCallId"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    if let Some(tool_call_id) = tool_call_id {
        let mut result = agent
            .get("result")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        result.insert("toolCallId".to_owned(), Value::String(tool_call_id));
        updated_agent.insert("result".to_owned(), Value::Object(result));
    }
    updated_agent.insert("revision".to_owned(), Value::from(terminal_revision));
    updated_agent.insert("updatedAt".to_owned(), Value::String(now_iso.to_owned()));
    updated_agent.remove("worker");
    let terminal_transport = if agent.get("detached").and_then(Value::as_bool) == Some(true) {
        Some(prepare_detached_agent_terminal_transport(
            &expected_owner.session_path,
            &expected_owner.agent_id,
            &expected_owner.owner,
            terminal_revision,
            "lost_runtime",
            now_iso,
        )?)
    } else {
        None
    };
    Ok(DeadRuntimeRecoveryPlan {
        agent_data,
        process_identity_data: serialize_process_identity(&expected_owner.process_identity),
        terminal_revision,
        terminal_transport,
        updated_agent_data: js_json::stringify(&Value::Object(updated_agent.clone())),
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
            .and_then(Value::as_str)
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
            return Err(ControlDbError::new(format!(
                "Recovered agent changed after ownership release {}#{}",
                expected_owner.session_path, expected_owner.agent_id
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

pub(crate) fn prepare_detached_agent_terminal_transport(
    session_path: &str,
    agent_id: &str,
    owner: &MultiAgentRuntimeOwnerIdentity,
    terminal_revision: i64,
    event_kind: &str,
    terminal_at: &str,
) -> Result<PreparedTerminalTransport, ControlDbError> {
    let owner_session_id = owner.session_id.as_str();
    if owner_session_id.is_empty() {
        return Err(ControlDbError::new(format!(
            "Detached job {agent_id} ownership has no owner session"
        )));
    }
    let message_id = format!("terminal:{agent_id}:{terminal_revision}:{event_kind}");
    let body = js_json::stringify(&serde_json::json!({
        "agentId": agent_id,
        "eventKind": event_kind,
        "terminalRevision": terminal_revision,
        "type": "multi_agent_terminal",
    }));
    let message = serde_json::json!({
        "body": body,
        "createdAt": terminal_at,
        "fromAgentId": agent_id,
        "id": message_id,
        "kind": "system",
        "status": "pending",
        "toAgentId": owner.agent_id.clone().unwrap_or_else(|| "main".to_owned()),
        "updatedAt": terminal_at,
    });
    let input = EnqueueStoredRuntimeMailboxMessageInput {
        kind: RuntimeMailboxMessageKind::System,
        message,
        recipient: RuntimeMailboxAddress {
            agent_id: owner.agent_id.clone(),
            session_id: owner_session_id.to_owned(),
        },
        sender: RuntimeMailboxAddress {
            agent_id: Some(agent_id.to_owned()),
            session_id: owner_session_id.to_owned(),
        },
        store_ref: super::RuntimeMailboxStoreRef {
            message_id: message_id.clone(),
            session_path: session_path.to_owned(),
        },
        updated_at: Some(terminal_at.to_owned()),
    };
    let prepared = prepare_stored_runtime_mailbox_message(&input)?;
    Ok(PreparedTerminalTransport { input, prepared })
}

pub(crate) fn persist_prepared_terminal_transport(
    db: &Connection,
    transport: &PreparedTerminalTransport,
) -> Result<(), ControlDbError> {
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
pub fn reconcile_dead_detached_agent_runtimes(control_db_path: &str, now_iso: &str) -> Result<i64, ControlDbError> {
    with_control_db(control_db_path, |db| {
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
    agent_id: String,
    data: String,
    owner_agent_id: Option<String>,
    owner_session_id: Option<String>,
    process_identity: Option<String>,
    session_path: String,
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
    let context = format!("multi_agent_agents:{}#{}", candidate.session_path, candidate.agent_id);
    let agent = parse_stored_json_object(&candidate.data, &context)?;
    validate_persisted_agent_payload(&agent, &context)?;
    let process_identity = parse_process_identity(&process_identity)?;
    let worker_adapter = agent
        .get("worker")
        .and_then(Value::as_object)
        .and_then(|worker| worker.get("adapter"))
        .and_then(Value::as_str);
    let worker_handle = agent
        .get("worker")
        .and_then(Value::as_object)
        .and_then(|worker| worker.get("handleId"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    if worker_adapter != Some("runtime") || worker_handle.as_deref() != Some(process_identity.pid.to_string().as_str())
    {
        return Ok(None);
    }
    if has_terminal_outbox_record(db, &candidate.session_path, &candidate.agent_id)? {
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
        session_path: candidate.session_path.clone(),
    }))
}

fn has_terminal_outbox_record(db: &Connection, session_path: &str, agent_id: &str) -> Result<bool, ControlDbError> {
    Ok(db
        .query_row(
            "SELECT 1 FROM multi_agent_terminal_outbox WHERE session_path = ? AND agent_id = ? LIMIT 1",
            params![session_path, agent_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}
