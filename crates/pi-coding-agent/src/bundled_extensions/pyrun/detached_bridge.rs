//! Port of `packages/coding-agent/extensions/pyrun/src/detached-bridge.ts`.

use pi_ai::utils::uuid::random_uuid;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::session_control_db::{
    DetachedJobOwnershipIdentity, EnqueueStoredRuntimeMailboxMessageInput, RuntimeMailboxAddress,
    RuntimeMailboxMessage, RuntimeMailboxMessageKind, RuntimeMailboxStoreRef, enqueue_stored_runtime_mailbox_message,
    read_multi_agent_agent, read_multi_agent_runtime_ownership,
};

pub const DETACHED_PYRUN_BRIDGE_PROTOCOL: &str = "pyrun-bridge-v1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetachedPyrunBridgeRequest {
    pub command: String,
    pub identity: DetachedJobOwnershipIdentity,
    pub method: String,
    pub params: Value,
    pub protocol: String,
    pub reply_to: RuntimeMailboxAddress,
    pub request_id: String,
    /// Absent when the launching tool call had no ID (legacy manifests).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

pub struct EnqueueDetachedPyrunBridgeRequestInput {
    pub control_db_path: String,
    pub identity: DetachedJobOwnershipIdentity,
    pub method: String,
    pub params: Value,
    pub runner_address: RuntimeMailboxAddress,
    pub session_path: String,
    pub tool_call_id: Option<String>,
    pub supervisor_address: RuntimeMailboxAddress,
}

/// `enqueueDetachedPyrunBridgeRequest`.
pub fn enqueue_detached_pyrun_bridge_request(input: EnqueueDetachedPyrunBridgeRequestInput) -> Result<String, String> {
    let request_id = random_uuid();
    let body = DetachedPyrunBridgeRequest {
        command: "request".to_owned(),
        identity: input.identity.clone(),
        method: input.method,
        params: input.params,
        protocol: DETACHED_PYRUN_BRIDGE_PROTOCOL.to_owned(),
        reply_to: input.runner_address.clone(),
        request_id: request_id.clone(),
        tool_call_id: input.tool_call_id,
    };
    persist_bridge_message(PersistBridgeMessageInput {
        body: serde_json::to_value(&body).map_err(|error| error.to_string())?,
        control_db_path: input.control_db_path,
        from_agent_id: input.identity.job_id.clone(),
        message_id: format!("pyrun-request:{}:{request_id}", input.identity.job_id),
        recipient: input.supervisor_address,
        sender: input.runner_address,
        session_path: input.session_path,
        to_agent_id: "main".to_owned(),
    })?;
    Ok(request_id)
}

/// `parseDetachedPyrunBridgeRequest`.
pub fn parse_detached_pyrun_bridge_request(message: &RuntimeMailboxMessage) -> Option<DetachedPyrunBridgeRequest> {
    let parsed: DetachedPyrunBridgeRequest = serde_json::from_str(&message.body).ok()?;
    let has_valid_tool_call_id = parsed
        .tool_call_id
        .as_deref()
        .is_some_and(|tool_call_id| !pi_js_compat::string::js_trim(tool_call_id).is_empty());
    if parsed.protocol != DETACHED_PYRUN_BRIDGE_PROTOCOL || parsed.command != "request" || !has_valid_tool_call_id {
        return None;
    }
    Some(parsed)
}

pub struct ValidateDetachedPyrunBridgeRequestInput<'a> {
    pub control_db_path: &'a str,
    pub message: &'a RuntimeMailboxMessage,
    pub request: &'a DetachedPyrunBridgeRequest,
    pub session_path: &'a str,
    pub supervisor_session_id: &'a str,
}

/// `validateDetachedPyrunBridgeRequest`.
pub fn validate_detached_pyrun_bridge_request(
    input: ValidateDetachedPyrunBridgeRequestInput<'_>,
) -> Result<bool, String> {
    let identity = &input.request.identity;
    if input.message.sender.agent_id.as_deref() != Some(identity.job_id.as_str()) {
        return Ok(false);
    }
    let agent = read_multi_agent_agent(input.control_db_path, input.session_path, &identity.job_id)
        .map_err(|error| error.to_string())?;
    let ownership = read_multi_agent_runtime_ownership(input.control_db_path, input.session_path, &identity.job_id)
        .map_err(|error| error.to_string())?;
    let Some(agent) = agent else {
        return Ok(false);
    };
    let Some(ownership) = ownership else {
        return Ok(false);
    };
    let Some(process_identity) = ownership.process_identity.as_ref() else {
        return Ok(false);
    };
    Ok(
        agent.lifecycle == crate::core::multi_agent_store::AgentLifecycle::Running
            && identity.owner.session_id == input.supervisor_session_id
            && ownership.owner.session_id.as_deref() == Some(identity.owner.session_id.as_str())
            && ownership.owner.agent_id == identity.owner.agent_id
            && process_identity.pid == identity.process_identity.pid
            && process_identity.start_time_ticks == identity.process_identity.start_time_ticks,
    )
}

pub struct EnqueueDetachedPyrunBridgeResponseInput {
    pub control_db_path: String,
    pub error: Option<String>,
    pub request: DetachedPyrunBridgeRequest,
    pub result: Option<Value>,
    pub session_path: String,
    pub supervisor_address: RuntimeMailboxAddress,
}

/// `enqueueDetachedPyrunBridgeResponse`.
pub fn enqueue_detached_pyrun_bridge_response(input: EnqueueDetachedPyrunBridgeResponseInput) -> Result<(), String> {
    let message_id = format!(
        "pyrun-response:{}:{}",
        input.request.identity.job_id, input.request.request_id
    );
    let mut body = serde_json::Map::new();
    body.insert("command".to_owned(), Value::String("respond".to_owned()));
    if let Some(error) = input.error {
        body.insert("error".to_owned(), Value::String(error));
    }
    body.insert(
        "identity".to_owned(),
        serde_json::to_value(&input.request.identity).map_err(|error| error.to_string())?,
    );
    body.insert("requestId".to_owned(), Value::String(input.request.request_id.clone()));
    if let Some(result) = input.result {
        body.insert("result".to_owned(), result);
    }
    persist_bridge_message(PersistBridgeMessageInput {
        body: Value::Object(body),
        control_db_path: input.control_db_path,
        from_agent_id: "main".to_owned(),
        message_id,
        recipient: input.request.reply_to.clone(),
        sender: input.supervisor_address,
        session_path: input.session_path,
        to_agent_id: input.request.identity.job_id.clone(),
    })
}

struct PersistBridgeMessageInput {
    body: Value,
    control_db_path: String,
    from_agent_id: String,
    message_id: String,
    recipient: RuntimeMailboxAddress,
    sender: RuntimeMailboxAddress,
    session_path: String,
    to_agent_id: String,
}

fn persist_bridge_message(input: PersistBridgeMessageInput) -> Result<(), String> {
    let mut message = serde_json::Map::new();
    message.insert(
        "body".to_owned(),
        Value::String(serde_json::to_string(&input.body).map_err(|error| error.to_string())?),
    );
    message.insert("fromAgentId".to_owned(), Value::String(input.from_agent_id.clone()));
    message.insert("id".to_owned(), Value::String(input.message_id.clone()));
    message.insert("kind".to_owned(), Value::String("system".to_owned()));
    message.insert("status".to_owned(), Value::String("pending".to_owned()));
    message.insert("toAgentId".to_owned(), Value::String(input.to_agent_id));
    enqueue_stored_runtime_mailbox_message(
        &input.control_db_path,
        &EnqueueStoredRuntimeMailboxMessageInput {
            recipient: input.recipient,
            sender: input.sender,
            kind: RuntimeMailboxMessageKind::System,
            store_ref: RuntimeMailboxStoreRef {
                message_id: input.message_id,
                session_path: input.session_path,
            },
            message: Value::Object(message),
            updated_at: None,
        },
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}
