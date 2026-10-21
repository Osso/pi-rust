//! Detached job runtime mailbox commands (`core/detached-job-control.ts`).

use serde_json::{Map, Value};

use crate::core::detached_job_runner::DetachedJobOwnershipIdentity;
use crate::core::session_control_db::{
    EnqueueStoredRuntimeMailboxMessageInput, RuntimeMailboxAddress, RuntimeMailboxMessageKind, RuntimeMailboxStoreRef,
    claim_runtime_mailbox_messages, deliver_runtime_mailbox_message, enqueue_stored_runtime_mailbox_message,
    fail_runtime_mailbox_message,
};
use crate::core::sqlite::ControlDbError;

const MAILBOX_CLAIM_LIMIT: i64 = 20;

/// `DetachedJobCancelCommand`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedJobCancelCommand {
    pub identity: DetachedJobOwnershipIdentity,
    pub reason: Option<String>,
    pub mailbox_row_id: i64,
}

/// `DetachedJobStatusCommand`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetachedJobStatusCommand {
    pub identity: DetachedJobOwnershipIdentity,
    pub reply_to: RuntimeMailboxAddress,
    pub request_id: String,
    pub mailbox_row_id: i64,
}

/// `DetachedJobResponseCommand`.
#[derive(Debug, Clone, PartialEq)]
pub struct DetachedJobResponseCommand {
    pub identity: DetachedJobOwnershipIdentity,
    pub request_id: String,
    pub result: Option<Value>,
    pub error: Option<String>,
    pub mailbox_row_id: i64,
}

/// `DetachedJobRuntimeCommand`.
#[derive(Debug, Clone, PartialEq)]
pub enum DetachedJobRuntimeCommand {
    Cancel(DetachedJobCancelCommand),
    Status(DetachedJobStatusCommand),
    Response(DetachedJobResponseCommand),
}

impl DetachedJobRuntimeCommand {
    pub fn identity(&self) -> &DetachedJobOwnershipIdentity {
        match self {
            Self::Cancel(command) => &command.identity,
            Self::Status(command) => &command.identity,
            Self::Response(command) => &command.identity,
        }
    }

    pub fn mailbox_row_id(&self) -> i64 {
        match self {
            Self::Cancel(command) => command.mailbox_row_id,
            Self::Status(command) => command.mailbox_row_id,
            Self::Response(command) => command.mailbox_row_id,
        }
    }
}

#[derive(Debug, Clone)]
enum StoredCommand {
    Cancel {
        identity: DetachedJobOwnershipIdentity,
        reason: Option<String>,
    },
    Status {
        identity: DetachedJobOwnershipIdentity,
        reply_to: RuntimeMailboxAddress,
        request_id: String,
    },
    Response {
        identity: DetachedJobOwnershipIdentity,
        request_id: String,
        result: Option<Value>,
        error: Option<String>,
    },
}

impl StoredCommand {
    fn identity(&self) -> &DetachedJobOwnershipIdentity {
        match self {
            Self::Cancel { identity, .. } | Self::Status { identity, .. } | Self::Response { identity, .. } => identity,
        }
    }
}

/// `claimDetachedJobRuntimeCommands`.
pub fn claim_detached_job_runtime_commands(
    control_db_path: &str,
    recipient: &RuntimeMailboxAddress,
    identity: &DetachedJobOwnershipIdentity,
) -> Result<Vec<DetachedJobRuntimeCommand>, ControlDbError> {
    let mut commands = Vec::new();
    for message in claim_runtime_mailbox_messages(control_db_path, recipient, MAILBOX_CLAIM_LIMIT)? {
        let Some(command) = parse_runtime_command(&message.body) else {
            fail_runtime_mailbox_message(control_db_path, message.id, "Detached job control identity mismatch")?;
            continue;
        };
        if !command_identity_matches(&command, identity) {
            fail_runtime_mailbox_message(control_db_path, message.id, "Detached job control identity mismatch")?;
            continue;
        }
        if !deliver_runtime_mailbox_message(control_db_path, message.id, None)? {
            fail_runtime_mailbox_message(control_db_path, message.id, "Detached job control delivery failed")?;
            continue;
        }
        commands.push(with_mailbox_row(command, message.id));
    }
    Ok(commands)
}

fn with_mailbox_row(command: StoredCommand, mailbox_row_id: i64) -> DetachedJobRuntimeCommand {
    match command {
        StoredCommand::Cancel { identity, reason } => DetachedJobRuntimeCommand::Cancel(DetachedJobCancelCommand {
            identity,
            reason,
            mailbox_row_id,
        }),
        StoredCommand::Status {
            identity,
            reply_to,
            request_id,
        } => DetachedJobRuntimeCommand::Status(DetachedJobStatusCommand {
            identity,
            reply_to,
            request_id,
            mailbox_row_id,
        }),
        StoredCommand::Response {
            identity,
            request_id,
            result,
            error,
        } => DetachedJobRuntimeCommand::Response(DetachedJobResponseCommand {
            identity,
            request_id,
            result,
            error,
            mailbox_row_id,
        }),
    }
}

/// `enqueueDetachedJobStatusRequest`.
pub fn enqueue_detached_job_status_request(
    input: &EnqueueDetachedJobStatusRequestInput,
) -> Result<i64, ControlDbError> {
    let message_id = format!("detached-status-request:{}:{}", input.identity.job_id, input.request_id);
    let body = serde_json::json!({
        "command": "status",
        "identity": input.identity,
        "replyTo": input.requester_address,
        "requestId": input.request_id,
    });
    enqueue_stored_runtime_mailbox_message(
        &input.control_db_path,
        &EnqueueStoredRuntimeMailboxMessageInput {
            recipient_host: None,
            recipient: input.runner_address.clone(),
            sender: input.requester_address.clone(),
            kind: RuntimeMailboxMessageKind::System,
            store_ref: RuntimeMailboxStoreRef {
                message_id: message_id.clone(),
                session_path: input.session_path.clone(),
            },
            message: serde_json::json!({
                "body": serialize_body(&body)?,
                "fromAgentId": input.requester_address.agent_id.clone().unwrap_or_else(|| "main".to_owned()),
                "id": message_id,
                "kind": "system",
                "status": "pending",
                "toAgentId": input.identity.job_id,
            }),
            updated_at: None,
        },
    )
}

/// Input for [`enqueue_detached_job_status_request`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnqueueDetachedJobStatusRequestInput {
    pub control_db_path: String,
    pub identity: DetachedJobOwnershipIdentity,
    pub requester_address: RuntimeMailboxAddress,
    pub request_id: String,
    pub runner_address: RuntimeMailboxAddress,
    pub session_path: String,
}

/// Input for [`enqueue_detached_job_status_response`].
#[derive(Debug, Clone, PartialEq)]
pub struct EnqueueDetachedJobStatusResponseInput {
    pub control_db_path: String,
    pub identity: DetachedJobOwnershipIdentity,
    pub reply_to: RuntimeMailboxAddress,
    pub request_id: String,
    pub runner_address: RuntimeMailboxAddress,
    pub session_path: String,
    pub status: Value,
}

/// `enqueueDetachedJobStatusResponse`.
pub fn enqueue_detached_job_status_response(
    input: &EnqueueDetachedJobStatusResponseInput,
) -> Result<i64, ControlDbError> {
    let message_id = format!("detached-status:{}:{}", input.identity.job_id, input.request_id);
    let body = serde_json::json!({
        "command": "respond",
        "identity": input.identity,
        "requestId": input.request_id,
        "result": input.status,
    });
    enqueue_stored_runtime_mailbox_message(
        &input.control_db_path,
        &EnqueueStoredRuntimeMailboxMessageInput {
            recipient_host: None,
            recipient: input.reply_to.clone(),
            sender: input.runner_address.clone(),
            kind: RuntimeMailboxMessageKind::System,
            store_ref: RuntimeMailboxStoreRef {
                message_id: message_id.clone(),
                session_path: input.session_path.clone(),
            },
            message: serde_json::json!({
                "body": serialize_body(&body)?,
                "fromAgentId": input.identity.job_id,
                "id": message_id,
                "kind": "system",
                "status": "pending",
                "toAgentId": input.reply_to.agent_id.clone().unwrap_or_else(|| "main".to_owned()),
            }),
            updated_at: None,
        },
    )
}

/// `claimDetachedJobControlCommands`.
pub fn claim_detached_job_control_commands(
    control_db_path: &str,
    recipient: &RuntimeMailboxAddress,
    identity: &DetachedJobOwnershipIdentity,
) -> Result<Vec<DetachedJobCancelCommand>, ControlDbError> {
    Ok(
        claim_detached_job_runtime_commands(control_db_path, recipient, identity)?
            .into_iter()
            .filter_map(|command| match command {
                DetachedJobRuntimeCommand::Cancel(command) => Some(command),
                _ => None,
            })
            .collect(),
    )
}

fn serialize_body(body: &Value) -> Result<String, ControlDbError> {
    serde_json::to_string(body).map_err(|error| ControlDbError::new(error.to_string()))
}

fn parse_runtime_command(body: &str) -> Option<StoredCommand> {
    let parsed: Value = serde_json::from_str(body).ok()?;
    let record = parsed.as_object()?;
    let identity = parse_identity(record.get("identity")?)?;
    match record.get("command").and_then(Value::as_str) {
        Some("cancel") => Some(StoredCommand::Cancel {
            identity,
            reason: record.get("reason").and_then(Value::as_str).map(str::to_owned),
        }),
        Some("status") => {
            let request_id = record.get("requestId").and_then(Value::as_str)?;
            let reply_to = parse_mailbox_address(record.get("replyTo")?)?;
            Some(StoredCommand::Status {
                identity,
                reply_to,
                request_id: request_id.to_owned(),
            })
        }
        Some("respond") => {
            let request_id = record.get("requestId").and_then(Value::as_str)?;
            Some(StoredCommand::Response {
                identity,
                request_id: request_id.to_owned(),
                result: record.get("result").cloned(),
                error: record.get("error").and_then(Value::as_str).map(str::to_owned),
            })
        }
        _ => None,
    }
}

fn parse_mailbox_address(value: &Value) -> Option<RuntimeMailboxAddress> {
    let record = value.as_object()?;
    let session_id = record.get("sessionId")?.as_str()?;
    let agent_id = match record.get("agentId") {
        None | Some(Value::Null) => None,
        Some(Value::String(agent_id)) => Some(agent_id.clone()),
        Some(_) => return None,
    };
    Some(RuntimeMailboxAddress {
        agent_id,
        session_id: session_id.to_owned(),
    })
}

fn parse_identity(value: &Value) -> Option<DetachedJobOwnershipIdentity> {
    let record = value.as_object()?;
    let job_id = record.get("jobId")?.as_str()?;
    let owner = record.get("owner")?.as_object()?;
    let owner_session_id = owner.get("sessionId")?.as_str()?;
    let owner_agent_id = match owner.get("agentId") {
        Some(Value::Null) => None,
        Some(Value::String(agent_id)) => Some(agent_id.clone()),
        _ => return None,
    };
    let process_identity = parse_process_identity(record.get("processIdentity")?.as_object()?)?;
    let output_label = record.get("outputLabel")?.as_str()?;
    Some(DetachedJobOwnershipIdentity {
        job_id: job_id.to_owned(),
        owner: crate::core::session_control_db::MultiAgentRuntimeOwnerIdentity {
            agent_id: owner_agent_id,
            session_id: owner_session_id.to_owned(),
        },
        process_identity,
        output_label: output_label.to_owned(),
    })
}

fn parse_process_identity(record: &Map<String, Value>) -> Option<crate::core::runtime_process::ProcessIdentity> {
    let pid = safe_integer(record.get("pid")?)?;
    let start_time_ticks = safe_integer(record.get("startTimeTicks")?)?;
    let incarnation = record.get("incarnation").and_then(Value::as_str).map(str::to_owned);
    Some(crate::core::runtime_process::ProcessIdentity {
        pid: u32::try_from(pid).ok()?,
        start_time_ticks: u64::try_from(start_time_ticks).ok()?,
        incarnation,
    })
}

fn safe_integer(value: &Value) -> Option<i64> {
    let number = value.as_i64()?;
    const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;
    (-MAX_SAFE_INTEGER..=MAX_SAFE_INTEGER)
        .contains(&number)
        .then_some(number)
}

fn command_identity_matches(command: &StoredCommand, current: &DetachedJobOwnershipIdentity) -> bool {
    command.identity() == current
}
