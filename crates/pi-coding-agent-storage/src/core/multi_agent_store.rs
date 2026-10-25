//! Multi-agent in-memory store (`core/multi-agent-store.ts`).
//!
//! The store owns agent snapshots, mailbox messages, lifecycle notifications, and slot
//! selection. When a persistence target is attached, metadata and mailbox writes go
//! through `session_control_db` so the state survives process restarts.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use pi_coding_agent_data::core::desktop_notification::report_callback_error;
use pi_js_compat::callback_error::CallbackError;
use pi_js_compat::js_format;
use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue};
use serde::{Deserialize, Serialize};

use crate::core::runtime_process::ProcessIdentity;
use crate::core::session_control_db::{
    MultiAgentPersistedState, allocate_multi_agent_counter, read_multi_agent_state, update_multi_agent_agent_activity,
    update_multi_agent_agent_current_activity, update_multi_agent_agent_slot, update_multi_agent_agent_transcript,
    upsert_multi_agent_mailbox_message,
};
use crate::utils::paths::node_is_absolute;

pub type AgentLifecycleState = AgentLifecycle;
pub type TerminalAgentLifecycleState = TerminalAgentLifecycle;
pub type MailboxMessageKind = MailboxMessageKindValue;
pub type MailboxMessageStatus = MailboxMessageStatusValue;
pub type AgentSnapshot = AgentNode;
pub type SpawnAgentInput = SpawnAgent;

const DIRECT_MESSAGE_FIELDS: &[&str] = &[
    "id",
    "threadId",
    "fromAgentId",
    "toAgentId",
    "kind",
    "status",
    "createdAt",
    "updatedAt",
    "body",
    "fileRefs",
];
const OUTBOUND_MESSAGE_FIELDS: &[&str] = &[
    "fileRefs",
    "body",
    "createdAt",
    "fromAgentId",
    "id",
    "kind",
    "status",
    "threadId",
    "toAgentId",
    "updatedAt",
];
const LIFECYCLE_MESSAGE_FIELDS: &[&str] = &[
    "body",
    "createdAt",
    "fromAgentId",
    "id",
    "kind",
    "status",
    "threadId",
    "toAgentId",
    "updatedAt",
    "fileRefs",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentLifecycle {
    Running,
    WaitingForInput,
    SteeringPending,
    Cancelling,
    Completed,
    Failed,
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalAgentLifecycle {
    Completed,
    Failed,
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SteeringCheckpoint {
    NextModelCall,
    AfterToolResult,
    WhenWaiting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailboxMessageKindValue {
    Message,
    Ask,
    Reply,
    Steer,
    ParentRequest,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailboxMessageStatusValue {
    Pending,
    Claimed,
    Accepted,
    Rejected,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentActivity {
    pub description: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "phase", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum AgentCurrentActivity {
    Thinking {
        started_at: JsString,
    },
    Tool {
        started_at: JsString,
        tool_call_id: JsString,
        tool_name: JsString,
        #[serde(skip_serializing_if = "Option::is_none")]
        detail: Option<JsString>,
    },
}

impl<'de> Deserialize<'de> for AgentCurrentActivity {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let JsValue::Object(mut object) = JsValue::deserialize(deserializer)? else {
            return Err(serde::de::Error::custom("expected current activity object"));
        };
        let phase: JsString = take_native_field(&mut object, "phase")?;
        match phase.to_utf8().as_deref() {
            Ok("thinking") => Ok(Self::Thinking {
                started_at: take_native_field(&mut object, "startedAt")?,
            }),
            Ok("tool") => Ok(Self::Tool {
                started_at: take_native_field(&mut object, "startedAt")?,
                tool_call_id: take_native_field(&mut object, "toolCallId")?,
                tool_name: take_native_field(&mut object, "toolName")?,
                detail: take_native_field(&mut object, "detail")?,
            }),
            Ok(other) => Err(serde::de::Error::unknown_variant(other, &["thinking", "tool"])),
            Err(error) => Err(serde::de::Error::custom(error)),
        }
    }
}

fn take_native_field<T: serde::de::DeserializeOwned, E: serde::de::Error>(
    object: &mut JsObject,
    key: &str,
) -> Result<T, E> {
    js_json::from_value(&object.shift_remove(key).unwrap_or(JsValue::Null)).map_err(E::custom)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCurrentActivityOwner {
    pub owner_session_id: JsString,
    pub process_identity: ProcessIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentFileReference {
    pub path: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<pi_js_compat::js_json::JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_refs: Option<Vec<AgentFileReference>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorkerAdapter {
    pub adapter: AgentWorkerAdapterKind,
    pub handle_id: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<JsString>,
    /// Tool call in the owning agent that spawned this detached runtime worker, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<JsString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentWorkerAdapterKind {
    Runtime,
    Terminal,
    Subprocess,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTranscriptMetadata {
    pub session_id: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEventStreamMetadata {
    pub path: JsString,
    pub event_count: i64,
    pub truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_limit: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentWorktree {
    pub path: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentModel {
    pub provider_id: String,
    pub model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAccount {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub budget_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_fallback: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<AgentTokenBudget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrency_cap: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<AgentRateLimit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTokenBudget {
    pub limit: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRateLimit {
    pub per_minute: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPermission {
    pub policy: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inherited_from: Option<JsString>,
    pub narrowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSlot {
    pub index: i64,
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentError {
    pub message: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentNode {
    pub id: JsString,
    pub parent_id: Option<JsString>,
    pub display_name: JsString,
    pub agent_type: JsString,
    /// How the agent entered the store; absent means spawned as a child agent.
    pub origin: Option<AgentOrigin>,
    pub lifecycle: AgentLifecycle,
    pub revision: i64,
    pub created_at: JsString,
    pub updated_at: JsString,
    pub cwd: JsString,
    pub worktree: Option<AgentWorktree>,
    pub model: Option<AgentModel>,
    pub account: Option<AgentAccount>,
    pub permission: AgentPermission,
    pub slot: Option<AgentSlot>,
    pub transcript: Option<AgentTranscriptMetadata>,
    pub event_stream: Option<AgentEventStreamMetadata>,
    pub worker: Option<AgentWorkerAdapter>,
    /// True when a background job was detached from its waiting tool call.
    pub detached: Option<bool>,
    pub current_activity: Option<AgentCurrentActivity>,
    pub last_activity: Option<AgentActivity>,
    pub result: Option<AgentResult>,
    pub error: Option<AgentError>,
    /// Fields the TypeScript runtime preserves through `{...agent}` spreads.
    pub extra: JsObject,
}

fn agent_node_json(agent: &AgentNode) -> JsValue {
    let JsValue::Object(mut object) = pi_js_compat::js_json!({
        "id": agent.id,
        "parentId": agent.parent_id,
        "displayName": agent.display_name,
        "agentType": agent.agent_type,
        "origin": agent.origin,
        "lifecycle": agent.lifecycle,
        "revision": agent.revision,
        "createdAt": agent.created_at,
        "updatedAt": agent.updated_at,
        "cwd": agent.cwd,
        "worktree": agent.worktree,
        "model": agent.model,
        "account": agent.account,
        "permission": agent.permission,
        "slot": agent.slot,
        "transcript": agent.transcript,
        "eventStream": agent.event_stream,
        "worker": agent.worker,
        "detached": agent.detached,
        "currentActivity": agent.current_activity,
        "lastActivity": agent.last_activity,
        "result": agent.result,
        "error": agent.error,
    }) else {
        unreachable!("agent fields are an object")
    };
    for (key, present) in [
        ("parentId", agent.parent_id.is_some()),
        ("origin", agent.origin.is_some()),
        ("worktree", agent.worktree.is_some()),
        ("model", agent.model.is_some()),
        ("account", agent.account.is_some()),
        ("slot", agent.slot.is_some()),
        ("transcript", agent.transcript.is_some()),
        ("eventStream", agent.event_stream.is_some()),
        ("worker", agent.worker.is_some()),
        ("detached", agent.detached.is_some()),
        ("currentActivity", agent.current_activity.is_some()),
        ("lastActivity", agent.last_activity.is_some()),
        ("result", agent.result.is_some()),
        ("error", agent.error.is_some()),
    ] {
        if !present {
            object.shift_remove(key);
        }
    }
    object.extend(agent.extra.clone());
    JsValue::Object(object)
}

impl Serialize for AgentNode {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        agent_node_json(self).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AgentNode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let JsValue::Object(mut object) = JsValue::deserialize(deserializer)? else {
            return Err(serde::de::Error::custom("expected agent object"));
        };
        Ok(Self {
            id: take_native_field(&mut object, "id")?,
            parent_id: take_native_field(&mut object, "parentId")?,
            display_name: take_native_field(&mut object, "displayName")?,
            agent_type: take_native_field(&mut object, "agentType")?,
            origin: take_native_field(&mut object, "origin")?,
            lifecycle: take_native_field(&mut object, "lifecycle")?,
            revision: take_native_field(&mut object, "revision")?,
            created_at: take_native_field(&mut object, "createdAt")?,
            updated_at: take_native_field(&mut object, "updatedAt")?,
            cwd: take_native_field(&mut object, "cwd")?,
            worktree: take_native_field(&mut object, "worktree")?,
            model: take_native_field(&mut object, "model")?,
            account: take_native_field(&mut object, "account")?,
            permission: take_native_field(&mut object, "permission")?,
            slot: take_native_field(&mut object, "slot")?,
            transcript: take_native_field(&mut object, "transcript")?,
            event_stream: take_native_field(&mut object, "eventStream")?,
            worker: take_native_field(&mut object, "worker")?,
            detached: take_native_field(&mut object, "detached")?,
            current_activity: take_native_field(&mut object, "currentActivity")?,
            last_activity: take_native_field(&mut object, "lastActivity")?,
            result: take_native_field(&mut object, "result")?,
            error: take_native_field(&mut object, "error")?,
            extra: object,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentOrigin {
    Attached,
    Spawned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpawnAgent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<JsString>,
    pub display_name: JsString,
    pub agent_type: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<AgentOrigin>,
    pub cwd: JsString,
    pub permission: AgentPermission,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worktree: Option<AgentWorktree>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<AgentModel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<AgentAccount>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot: Option<AgentSlot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcript: Option<AgentTranscriptMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_stream: Option<AgentEventStreamMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker: Option<AgentWorkerAdapter>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentMailboxMessage {
    /// Canonical DB delivery returns its original payload; store copyMessage drops these fields.
    pub extra: JsObject,
    pub id: JsString,
    pub thread_id: Option<JsString>,
    pub from_agent_id: JsString,
    pub to_agent_id: JsString,
    pub kind: MailboxMessageKindValue,
    pub status: MailboxMessageStatusValue,
    pub created_at: JsString,
    pub updated_at: Option<JsString>,
    pub body: Option<JsString>,
    pub file_refs: Option<JsValue>,
    pub target_checkpoint: Option<SteeringCheckpoint>,
    pub error: Option<JsString>,
}

impl Serialize for AgentMailboxMessage {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let JsValue::Object(mut fields) = pi_js_compat::js_json!({
            "body": self.body,
            "createdAt": self.created_at,
            "error": self.error,
            "fileRefs": self.file_refs,
            "fromAgentId": self.from_agent_id,
            "id": self.id,
            "kind": self.kind,
            "status": self.status,
            "targetCheckpoint": self.target_checkpoint,
            "threadId": self.thread_id,
            "toAgentId": self.to_agent_id,
            "updatedAt": self.updated_at,
        }) else {
            unreachable!("mailbox fields are an object")
        };
        for (key, present) in [
            ("body", self.body.is_some()),
            ("error", self.error.is_some()),
            ("fileRefs", self.file_refs.is_some()),
            ("targetCheckpoint", self.target_checkpoint.is_some()),
            ("threadId", self.thread_id.is_some()),
            ("updatedAt", self.updated_at.is_some()),
        ] {
            if !present {
                fields.shift_remove(key);
            }
        }
        let mut object = self.extra.clone();
        object.extend(fields);
        JsValue::Object(object).serialize(serializer)
    }
}

fn encode_mailbox_constructor(message: &AgentMailboxMessage, fields: &[&str]) -> Result<JsValue, MultiAgentStoreError> {
    let JsValue::Object(mut object) =
        js_json::to_value(message).map_err(|error| MultiAgentStoreError::new(error.to_string()))?
    else {
        unreachable!("mailbox serializer emits an object")
    };
    let mut ordered = JsObject::new();
    for field in fields {
        if let Some(value) = object.shift_remove(*field) {
            ordered.insert((*field).into(), value);
        }
    }
    ordered.extend(object);
    Ok(JsValue::Object(ordered))
}

impl<'de> Deserialize<'de> for AgentMailboxMessage {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let JsValue::Object(mut object) = JsValue::deserialize(deserializer)? else {
            return Err(serde::de::Error::custom("expected mailbox object"));
        };
        Ok(Self {
            id: take_native_field(&mut object, "id")?,
            thread_id: take_native_field(&mut object, "threadId")?,
            from_agent_id: take_native_field(&mut object, "fromAgentId")?,
            to_agent_id: take_native_field(&mut object, "toAgentId")?,
            kind: take_native_field(&mut object, "kind")?,
            status: take_native_field(&mut object, "status")?,
            created_at: take_native_field(&mut object, "createdAt")?,
            updated_at: take_native_field(&mut object, "updatedAt")?,
            body: take_native_field(&mut object, "body")?,
            file_refs: take_native_field(&mut object, "fileRefs")?,
            target_checkpoint: take_native_field(&mut object, "targetCheckpoint")?,
            error: take_native_field(&mut object, "error")?,
            extra: object,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendSteeringInput {
    pub from_agent_id: JsString,
    pub body: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_checkpoint: Option<SteeringCheckpoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_refs: Option<JsValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactParentInput {
    pub body: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_refs: Option<JsValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMailboxMessageInput {
    pub body: JsString,
    pub to_agent_id: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_refs: Option<JsValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordOutboundSessionMessageInput {
    pub from_agent_id: JsString,
    pub to_agent_id: JsString,
    pub body: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_refs: Option<JsValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSlotProjection {
    pub agent_id: JsString,
    pub agent: AgentSnapshot,
    pub index: i64,
    pub pinned: bool,
    pub revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRowProjection {
    pub agent_id: JsString,
    pub display_name: JsString,
    pub lifecycle: AgentLifecycleState,
    pub revision: i64,
    pub active: bool,
    pub selected: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slot_index: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker_adapter: Option<AgentWorkerAdapterKind>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiAgentProjectionSnapshot {
    pub active_count: usize,
    pub agents: Vec<AgentSnapshot>,
    pub mailbox_messages: Vec<AgentMailboxMessage>,
    pub rows: Vec<AgentRowProjection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_agent_id: Option<JsString>,
    pub slots: Vec<AgentSlotProjection>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentViewSelectionResult {
    Ok(Box<AgentSnapshot>),
    NotFound { agent_id: JsString },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActiveAgentTargetSelectionResult {
    Ok(Box<AgentSnapshot>),
    NotFound { agent_id: JsString },
    Inactive { agent: Box<AgentSnapshot> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentMetadataCommandResult {
    Ok(Box<AgentSnapshot>),
    NotFound {
        agent_id: JsString,
    },
    StaleRevision {
        current: Box<AgentSnapshot>,
        projection: Box<MultiAgentProjectionSnapshot>,
    },
    SlotConflict {
        current: Box<AgentSnapshot>,
        occupant: Box<AgentSnapshot>,
        projection: Box<MultiAgentProjectionSnapshot>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum AgentTranscriptCommandResult {
    Ok(Box<AgentSnapshot>),
    NotFound { agent_id: JsString },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParentContactResult {
    Ok {
        agent: Box<AgentSnapshot>,
        message: Box<AgentMailboxMessage>,
    },
    NotFound {
        agent_id: JsString,
    },
    ParentNotFound {
        current: Box<AgentSnapshot>,
    },
    StaleRevision {
        current: Box<AgentSnapshot>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum MailboxMessageCommandResult {
    Ok {
        agent: Box<AgentSnapshot>,
        message: Box<AgentMailboxMessage>,
    },
    NotFound {
        agent_id: JsString,
    },
    TargetNotFound {
        current: Box<AgentSnapshot>,
        target_id: JsString,
    },
    StaleRevision {
        current: Box<AgentSnapshot>,
    },
    ForbiddenTarget {
        current: Box<AgentSnapshot>,
        target: Box<AgentSnapshot>,
    },
}

impl Serialize for AgentViewSelectionResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        match self {
            Self::Ok(agent) => {
                let mut state = serializer.serialize_struct("AgentViewSelectionResult", 2)?;
                state.serialize_field("ok", &true)?;
                state.serialize_field("agent", agent)?;
                state.end()
            }
            Self::NotFound { agent_id } => {
                let mut state = serializer.serialize_struct("AgentViewSelectionResult", 3)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", "not_found")?;
                state.serialize_field("agentId", agent_id)?;
                state.end()
            }
        }
    }
}

impl Serialize for ActiveAgentTargetSelectionResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        match self {
            Self::Ok(agent) => {
                let mut state = serializer.serialize_struct("ActiveAgentTargetSelectionResult", 2)?;
                state.serialize_field("ok", &true)?;
                state.serialize_field("agent", agent)?;
                state.end()
            }
            Self::NotFound { agent_id } => {
                let mut state = serializer.serialize_struct("ActiveAgentTargetSelectionResult", 3)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", "not_found")?;
                state.serialize_field("agentId", agent_id)?;
                state.end()
            }
            Self::Inactive { agent } => {
                let mut state = serializer.serialize_struct("ActiveAgentTargetSelectionResult", 3)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", "inactive")?;
                state.serialize_field("agent", agent)?;
                state.end()
            }
        }
    }
}

impl Serialize for AgentMetadataCommandResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        match self {
            Self::Ok(agent) => {
                let mut state = serializer.serialize_struct("AgentMetadataCommandResult", 2)?;
                state.serialize_field("ok", &true)?;
                state.serialize_field("agent", agent)?;
                state.end()
            }
            Self::NotFound { agent_id } => {
                let mut state = serializer.serialize_struct("AgentMetadataCommandResult", 3)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", "not_found")?;
                state.serialize_field("agentId", agent_id)?;
                state.end()
            }
            Self::StaleRevision { current, projection } => {
                let mut state = serializer.serialize_struct("AgentMetadataCommandResult", 4)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", "stale_revision")?;
                state.serialize_field("current", current)?;
                state.serialize_field("projection", projection)?;
                state.end()
            }
            Self::SlotConflict {
                current,
                occupant,
                projection,
            } => {
                let mut state = serializer.serialize_struct("AgentMetadataCommandResult", 5)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", "slot_conflict")?;
                state.serialize_field("current", current)?;
                state.serialize_field("occupant", occupant)?;
                state.serialize_field("projection", projection)?;
                state.end()
            }
        }
    }
}

impl Serialize for AgentTranscriptCommandResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        match self {
            Self::Ok(agent) => {
                let mut state = serializer.serialize_struct("AgentTranscriptCommandResult", 2)?;
                state.serialize_field("ok", &true)?;
                state.serialize_field("agent", agent)?;
                state.end()
            }
            Self::NotFound { agent_id } => {
                let mut state = serializer.serialize_struct("AgentTranscriptCommandResult", 3)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", "not_found")?;
                state.serialize_field("agentId", agent_id)?;
                state.end()
            }
        }
    }
}

impl Serialize for ParentContactResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        match self {
            Self::Ok { agent, message } => {
                let mut state = serializer.serialize_struct("ParentContactResult", 3)?;
                state.serialize_field("ok", &true)?;
                state.serialize_field("agent", agent)?;
                state.serialize_field("message", message)?;
                state.end()
            }
            Self::NotFound { agent_id } => {
                let mut state = serializer.serialize_struct("ParentContactResult", 3)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", "not_found")?;
                state.serialize_field("agentId", agent_id)?;
                state.end()
            }
            Self::ParentNotFound { current } => {
                let mut state = serializer.serialize_struct("ParentContactResult", 3)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", "parent_not_found")?;
                state.serialize_field("current", current)?;
                state.end()
            }
            Self::StaleRevision { current } => {
                let mut state = serializer.serialize_struct("ParentContactResult", 3)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", "stale_revision")?;
                state.serialize_field("current", current)?;
                state.end()
            }
        }
    }
}

impl Serialize for MailboxMessageCommandResult {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        match self {
            Self::Ok { agent, message } => {
                let mut state = serializer.serialize_struct("MailboxMessageCommandResult", 3)?;
                state.serialize_field("ok", &true)?;
                state.serialize_field("agent", agent)?;
                state.serialize_field("message", message)?;
                state.end()
            }
            Self::NotFound { agent_id } => {
                let mut state = serializer.serialize_struct("MailboxMessageCommandResult", 3)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", "not_found")?;
                state.serialize_field("agentId", agent_id)?;
                state.end()
            }
            Self::TargetNotFound { current, target_id } => {
                let mut state = serializer.serialize_struct("MailboxMessageCommandResult", 4)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", "target_not_found")?;
                state.serialize_field("current", current)?;
                state.serialize_field("targetId", target_id)?;
                state.end()
            }
            Self::StaleRevision { current } => {
                let mut state = serializer.serialize_struct("MailboxMessageCommandResult", 3)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", "stale_revision")?;
                state.serialize_field("current", current)?;
                state.end()
            }
            Self::ForbiddenTarget { current, target } => {
                let mut state = serializer.serialize_struct("MailboxMessageCommandResult", 4)?;
                state.serialize_field("ok", &false)?;
                state.serialize_field("error", "forbidden_target")?;
                state.serialize_field("current", current)?;
                state.serialize_field("target", target)?;
                state.end()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MultiAgentStoreError {
    pub message: JsString,
    callback: Option<Box<CallbackError>>,
}

impl MultiAgentStoreError {
    pub fn new(message: impl Into<JsString>) -> Self {
        Self {
            message: message.into(),
            callback: None,
        }
    }

    pub fn message(&self) -> &JsString {
        &self.message
    }
}

impl std::fmt::Display for MultiAgentStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&js_json::encode_node_utf8(&self.message))
    }
}

impl std::error::Error for MultiAgentStoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.callback.as_deref().map(|error| error as &dyn std::error::Error)
    }
}

impl From<CallbackError> for MultiAgentStoreError {
    fn from(error: CallbackError) -> Self {
        let message = error
            .error_or_string_message()
            .unwrap_or_else(|error| pi_js_compat::callback_error::throw_callback_uncaught(error));
        Self {
            message,
            callback: Some(Box::new(error)),
        }
    }
}

impl From<crate::core::sqlite::ControlDbError> for MultiAgentStoreError {
    fn from(error: crate::core::sqlite::ControlDbError) -> Self {
        Self::from(CallbackError::from(error))
    }
}

/// Session-manager surface the store persists through.
pub trait SessionPersistence {
    fn metadata_control_db_path(&self) -> Option<JsString>;
    fn session_file(&self) -> Option<JsString>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistenceTarget {
    pub control_db_path: JsString,
    pub session_path: JsString,
}

type StoreClock = Arc<dyn Fn() -> Result<JsString, CallbackError> + Send + Sync>;

#[derive(Clone, Default)]
pub struct MultiAgentStoreOptions {
    pub now: Option<StoreClock>,
}

type AgentTransitionListener = Arc<dyn Fn(&AgentSnapshot, &AgentSnapshot) -> Result<(), CallbackError> + Send + Sync>;
type AgentUpdateListener = Arc<dyn Fn(&AgentSnapshot, &AgentSnapshot) -> Result<(), CallbackError> + Send + Sync>;
type LifecycleNotificationListener = Arc<dyn Fn(&AgentMailboxMessage) -> Result<(), CallbackError> + Send + Sync>;
struct AbortHandler {
    token: Arc<()>,
    handler: Box<dyn Fn() -> Result<(), CallbackError> + Send + Sync>,
}

struct ListenerList<T> {
    listeners: Mutex<BTreeMap<u64, T>>,
    next_id: Mutex<u64>,
}

impl<T> Default for ListenerList<T> {
    fn default() -> Self {
        Self {
            listeners: Mutex::new(BTreeMap::new()),
            next_id: Mutex::new(1),
        }
    }
}

impl<T> ListenerList<T> {
    fn insert(&self, listener: T) -> u64 {
        let mut next_id = self.next_id.lock().expect("listener id mutex");
        let id = *next_id;
        *next_id += 1;
        self.listeners.lock().expect("listener mutex").insert(id, listener);
        id
    }

    fn clear(&self) -> Result<(), MultiAgentStoreError> {
        let removed = std::mem::take(
            &mut *self
                .listeners
                .lock()
                .map_err(|error| MultiAgentStoreError::new(error.to_string()))?,
        );
        drop(removed);
        Ok(())
    }

    fn remove(&self, id: u64) {
        let removed = self.listeners.lock().expect("listener mutex").remove(&id);
        drop(removed);
    }
}

/// Live Set iteration: observe removals and newly appended listeners without locking across callbacks.
fn next_listener<T: Clone>(list: &ListenerList<T>, after: u64) -> Option<(u64, T)> {
    list.listeners
        .lock()
        .expect("listener mutex")
        .range((std::ops::Bound::Excluded(after), std::ops::Bound::Unbounded))
        .next()
        .map(|(id, listener)| (*id, listener.clone()))
}

/// Handle returned by `subscribe_*`; dropping it unsubscribes, matching the TS closure.
pub struct AgentSubscription {
    list: Arc<dyn Fn(u64) + Send + Sync>,
    id: u64,
}

impl AgentSubscription {
    pub fn unsubscribe(self) {
        (self.list)(self.id);
        std::mem::forget(self);
    }
}

impl Drop for AgentSubscription {
    fn drop(&mut self) {
        (self.list)(self.id);
    }
}

const COMPLETION_NOTIFICATION_THREAD_PREFIX: &str = "agent-completed";
const FAILED_NOTIFICATION_THREAD_PREFIX: &str = "agent-failed";
const WAITING_FOR_INPUT_NOTIFICATION_THREAD_PREFIX: &str = "agent-waiting-for-input";
const MAIN_THREAD_AGENT_ID: &str = "main";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleNotificationLifecycle {
    Completed,
    Failed,
    WaitingForInput,
}

#[derive(Default)]
struct StoreState {
    agents: HashMap<JsString, AgentNode>,
    agent_order: Vec<JsString>,
    mailbox_messages: HashMap<JsString, AgentMailboxMessage>,
    mailbox_order: Vec<JsString>,
    abort_handlers: HashMap<JsString, AbortHandler>,
    selected_agent_id: Option<JsString>,
    next_agent_number: i64,
    next_message_number: i64,
    restore_generation: u64,
    persistence: Option<PersistenceTarget>,
}

pub struct MultiAgentStore {
    state: Arc<Mutex<StoreState>>,
    now: StoreClock,
    lifecycle_notifications: Arc<ListenerList<LifecycleNotificationListener>>,
    transitions: Arc<ListenerList<AgentTransitionListener>>,
    updates: Arc<ListenerList<AgentUpdateListener>>,
}

impl MultiAgentStore {
    pub fn new(options: MultiAgentStoreOptions) -> Self {
        Self {
            state: Arc::new(Mutex::new(StoreState {
                next_agent_number: 1,
                next_message_number: 1,
                ..StoreState::default()
            })),
            now: options
                .now
                .unwrap_or_else(|| Arc::new(|| Ok(crate::utils::iso_time::now_iso_string().into()))),
            lifecycle_notifications: Arc::new(ListenerList::default()),
            transitions: Arc::new(ListenerList::default()),
            updates: Arc::new(ListenerList::default()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, StoreState> {
        self.state.lock().expect("multi-agent store mutex")
    }

    fn now(&self) -> Result<JsString, MultiAgentStoreError> {
        (self.now)().map_err(MultiAgentStoreError::from)
    }

    pub fn subscribe_lifecycle_notifications(
        &self,
        listener: impl Fn(&AgentMailboxMessage) -> Result<(), CallbackError> + Send + Sync + 'static,
    ) -> AgentSubscription {
        let id = self.lifecycle_notifications.insert(Arc::new(listener));
        subscription(
            ListenerList::subscription_remover(self.lifecycle_notifications.clone()),
            id,
        )
    }

    pub fn subscribe_agent_transitions(
        &self,
        listener: impl Fn(&AgentSnapshot, &AgentSnapshot) -> Result<(), CallbackError> + Send + Sync + 'static,
    ) -> AgentSubscription {
        let id = self.transitions.insert(Arc::new(listener));
        subscription(ListenerList::subscription_remover(self.transitions.clone()), id)
    }

    pub fn subscribe_agent_updates(
        &self,
        listener: impl Fn(&AgentSnapshot, &AgentSnapshot) -> Result<(), CallbackError> + Send + Sync + 'static,
    ) -> AgentSubscription {
        let id = self.updates.insert(Arc::new(listener));
        subscription(ListenerList::subscription_remover(self.updates.clone()), id)
    }

    pub fn register_agent_abort_handler(
        &self,
        agent_id: &JsString,
        handler: impl Fn() -> Result<(), CallbackError> + Send + Sync + 'static,
    ) -> AbortRegistration {
        let token = Arc::new(());
        self.lock().abort_handlers.insert(
            agent_id.to_owned(),
            AbortHandler {
                token: token.clone(),
                handler: Box::new(handler),
            },
        );
        AbortRegistration {
            state: self.state.clone(),
            agent_id: agent_id.to_owned(),
            token,
        }
    }

    pub fn abort_agent_handle(&self, agent_id: &JsString) -> Result<bool, CallbackError> {
        let handler = self.lock().abort_handlers.remove(agent_id);
        let Some(handler) = handler else {
            return Ok(false);
        };
        (handler.handler)()?;
        Ok(true)
    }

    pub fn allocate_agent_id_for_lifecycle_coordinator(&self, prefix: &str) -> Result<JsString, MultiAgentStoreError> {
        self.create_agent_id(prefix)
    }

    pub fn prepare_steering_message_for_lifecycle_coordinator(
        &self,
        agent_id: &JsString,
        input: &SendSteeringInput,
    ) -> Result<AgentMailboxMessage, MultiAgentStoreError> {
        let timestamp = self.now()?;
        Ok(AgentMailboxMessage {
            extra: JsObject::new(),
            body: Some(input.body.clone()),
            created_at: timestamp.clone(),
            file_refs: validate_mailbox_file_refs(input.file_refs.as_ref(), "steer_agent")?,
            from_agent_id: input.from_agent_id.clone().into(),
            id: self.create_message_id()?.into(),
            kind: MailboxMessageKindValue::Steer,
            status: MailboxMessageStatusValue::Pending,
            target_checkpoint: input.target_checkpoint,
            thread_id: input.thread_id.clone(),
            to_agent_id: agent_id.to_owned().into(),
            updated_at: Some(timestamp),
            error: None,
        })
    }

    pub fn publish_lifecycle_coordinator_steering_delivery(
        &self,
        agent: &AgentSnapshot,
        message: &AgentMailboxMessage,
    ) -> Result<(), MultiAgentStoreError> {
        self.remove_mailbox_message(&message.id);
        self.publish_lifecycle_coordinator_snapshot(agent)
    }

    pub fn publish_lifecycle_coordinator_steering(
        &self,
        agent: &AgentSnapshot,
        message: &AgentMailboxMessage,
    ) -> Result<(), MultiAgentStoreError> {
        let previous = self.lock().agents.get(&agent.id).cloned();
        validate_agent_paths(agent)?;
        let current = copy_agent(agent);
        let expired = self.mailbox_message_is_expired(message)?;
        let mut state = self.lock();
        if !expired {
            state
                .mailbox_messages
                .insert(message.id.clone(), copy_message(message)?);
            if !state.mailbox_order.contains(&message.id) {
                state.mailbox_order.push(message.id.clone());
            }
        }
        state.agents.insert(agent.id.clone(), current.clone());
        if !state.agent_order.contains(&agent.id) {
            state.agent_order.push(agent.id.clone());
        }
        drop(state);
        if let Some(previous) = previous {
            self.notify_agent_update_listeners(&previous, &current);
            self.notify_transition_listeners_if_lifecycle_changed(&previous, &current);
        }
        Ok(())
    }

    pub fn publish_lifecycle_coordinator_snapshot(&self, agent: &AgentSnapshot) -> Result<(), MultiAgentStoreError> {
        validate_agent_paths(agent)?;
        let previous = {
            let mut state = self.lock();
            let previous = state.agents.get(&agent.id).cloned();
            let current = copy_agent(agent);
            state.agents.insert(agent.id.clone(), current.clone());
            if !state.agent_order.contains(&agent.id) {
                state.agent_order.push(agent.id.clone());
            }
            let previous = previous;
            if previous.is_none() {
                clear_selected_terminal_agent(&mut state, &current);
                return Ok(());
            }
            Some((previous.expect("previous agent"), current))
        };
        let Some((previous, current)) = previous else {
            return Ok(());
        };
        self.notify_agent_update_listeners(&previous, &current);
        if previous.lifecycle != current.lifecycle {
            if current.lifecycle == AgentLifecycle::WaitingForInput {
                self.record_waiting_for_input_notification(&current)?;
            }
            self.notify_transition_listeners(&previous, &current);
        }
        let mut state = self.lock();
        clear_selected_terminal_agent(&mut state, &current);
        Ok(())
    }

    pub fn publish_terminal_outbox_snapshot(&self, agent: &AgentSnapshot) -> Result<(), MultiAgentStoreError> {
        validate_agent_paths(agent)?;
        let mut state = self.lock();
        let previous = state.agents.get(&agent.id).cloned();
        let current = copy_agent(agent);
        state.agents.insert(agent.id.clone(), current.clone());
        if !state.agent_order.contains(&agent.id) {
            state.agent_order.push(agent.id.clone());
        }
        drop(state);
        if current
            .extra
            .get("suppressTerminalNotification")
            .and_then(Value::as_bool)
            != Some(true)
        {
            if current.lifecycle == AgentLifecycle::Completed {
                self.retry_or_record_terminal_notification(&current, LifecycleNotificationLifecycle::Completed)?;
            }
            if current.lifecycle == AgentLifecycle::Failed || current.lifecycle == AgentLifecycle::Aborted {
                self.retry_or_record_terminal_notification(&current, LifecycleNotificationLifecycle::Failed)?;
            }
        }
        let Some(previous) = previous else {
            let mut state = self.lock();
            clear_selected_terminal_agent(&mut state, &current);
            return Ok(());
        };
        self.notify_agent_update_listeners(&previous, &current);
        self.notify_transition_listeners_if_lifecycle_changed(&previous, &current);
        let mut state = self.lock();
        clear_selected_terminal_agent(&mut state, &current);
        Ok(())
    }

    pub fn select_agent_view(&self, agent_id: &JsString) -> Option<AgentSnapshot> {
        match self.select_agent_view_with_status(agent_id) {
            AgentViewSelectionResult::Ok(agent) => Some(*agent),
            AgentViewSelectionResult::NotFound { .. } => None,
        }
    }

    pub fn select_agent_view_with_status(&self, agent_id: &JsString) -> AgentViewSelectionResult {
        let mut state = self.lock();
        let Some(agent) = state.agents.get(agent_id) else {
            return AgentViewSelectionResult::NotFound {
                agent_id: agent_id.to_owned(),
            };
        };
        let selected_id = agent.id.clone();
        let snapshot = copy_agent(agent);
        state.selected_agent_id = Some(selected_id);
        AgentViewSelectionResult::Ok(Box::new(snapshot))
    }

    pub fn select_active_agent_target(&self, agent_id: &JsString) -> Option<AgentSnapshot> {
        match self.select_active_agent_target_with_status(agent_id) {
            ActiveAgentTargetSelectionResult::Ok(agent) => Some(*agent),
            _ => None,
        }
    }

    pub fn select_active_agent_target_with_status(&self, agent_id: &JsString) -> ActiveAgentTargetSelectionResult {
        let mut state = self.lock();
        let Some(agent) = state.agents.get(agent_id) else {
            return ActiveAgentTargetSelectionResult::NotFound {
                agent_id: agent_id.to_owned(),
            };
        };
        if !is_active_lifecycle(agent.lifecycle) {
            return ActiveAgentTargetSelectionResult::Inactive {
                agent: Box::new(copy_agent(agent)),
            };
        }
        let selected_id = agent.id.clone();
        let snapshot = copy_agent(agent);
        state.selected_agent_id = Some(selected_id);
        ActiveAgentTargetSelectionResult::Ok(Box::new(snapshot))
    }

    pub fn select_agent_slot(&self, slot_index: i64) -> Option<AgentSnapshot> {
        match self.select_agent_slot_with_status(slot_index) {
            AgentViewSelectionResult::Ok(agent) => Some(*agent),
            AgentViewSelectionResult::NotFound { .. } => None,
        }
    }

    pub fn select_agent_slot_with_status(&self, slot_index: i64) -> AgentViewSelectionResult {
        let mut state = self.lock();
        let Some(selected_agent) = find_agent_for_slot_selection(&state, slot_index) else {
            return AgentViewSelectionResult::NotFound {
                agent_id: slot_index.to_string().into(),
            };
        };
        let selected_id = selected_agent.id.clone();
        let snapshot = copy_agent(&selected_agent);
        state.selected_agent_id = Some(selected_id);
        AgentViewSelectionResult::Ok(Box::new(snapshot))
    }

    pub fn select_active_agent_slot_target(&self, slot_index: i64) -> Option<AgentSnapshot> {
        match self.select_active_agent_slot_target_with_status(slot_index) {
            ActiveAgentTargetSelectionResult::Ok(agent) => Some(*agent),
            _ => None,
        }
    }

    pub fn select_active_agent_slot_target_with_status(&self, slot_index: i64) -> ActiveAgentTargetSelectionResult {
        let mut state = self.lock();
        let Some(selected_agent) = find_agent_for_slot_selection(&state, slot_index) else {
            return ActiveAgentTargetSelectionResult::NotFound {
                agent_id: slot_index.to_string().into(),
            };
        };
        if !is_active_lifecycle(selected_agent.lifecycle) {
            return ActiveAgentTargetSelectionResult::Inactive {
                agent: Box::new(copy_agent(&selected_agent)),
            };
        }
        let selected_id = selected_agent.id.clone();
        let snapshot = copy_agent(&selected_agent);
        state.selected_agent_id = Some(selected_id);
        ActiveAgentTargetSelectionResult::Ok(Box::new(snapshot))
    }

    pub fn get_selected_agent_id(&self) -> Option<JsString> {
        self.lock().selected_agent_id.clone()
    }

    pub fn clear_selected_agent_view(&self) {
        self.lock().selected_agent_id = None;
    }

    pub fn get_agent(&self, agent_id: &JsString) -> Option<AgentSnapshot> {
        self.lock().agents.get(agent_id).map(copy_agent)
    }

    pub fn list_agents(&self) -> Vec<AgentSnapshot> {
        let state = self.lock();
        state
            .agent_order
            .iter()
            .filter_map(|id| state.agents.get(id))
            .map(copy_agent)
            .collect()
    }

    pub fn list_descendants(&self, parent_id: &JsString) -> Vec<AgentSnapshot> {
        let state = self.lock();
        let mut descendants = Vec::new();
        visit_children(&state, parent_id, &mut descendants);
        descendants.iter().map(copy_agent).collect()
    }

    pub fn expire_mailbox_messages(&self, now: f64) -> Result<(), MultiAgentStoreError> {
        let (messages, persistence) = {
            let state = self.lock();
            (
                state
                    .mailbox_order
                    .iter()
                    .filter_map(|id| state.mailbox_messages.get(id))
                    .cloned()
                    .collect::<Vec<_>>(),
                state.persistence.clone(),
            )
        };
        for message in messages {
            let expired = crate::core::session_control_db::is_mailbox_message_expired(Some(&message.created_at), now);
            let vanished = if expired {
                false
            } else if let Some(target) = &persistence {
                crate::core::session_control_db::get_multi_agent_mailbox_message_status(
                    &target.control_db_path,
                    &target.session_path,
                    &message.id,
                )?
                .is_none()
            } else {
                false
            };
            if expired || vanished {
                self.remove_mailbox_message(&message.id);
            }
        }
        Ok(())
    }
    fn remove_mailbox_message(&self, id: &JsString) {
        let mut state = self.lock();
        state.mailbox_messages.remove(id);
        state.mailbox_order.retain(|existing| existing != id);
    }
    fn mailbox_message_is_expired(&self, message: &AgentMailboxMessage) -> Result<bool, MultiAgentStoreError> {
        let now = self.now()?;
        Ok(crate::core::session_control_db::is_mailbox_message_expired(
            Some(&message.created_at),
            pi_js_compat::date::date_parse_utf16(now.units()),
        ))
    }
    pub fn list_mailbox_messages(&self) -> Result<Vec<AgentMailboxMessage>, MultiAgentStoreError> {
        let now = self.now()?;
        self.expire_mailbox_messages(pi_js_compat::date::date_parse_utf16(now.units()))?;
        let state = self.lock();
        state
            .mailbox_order
            .iter()
            .filter_map(|id| state.mailbox_messages.get(id))
            .map(copy_message)
            .collect()
    }
    pub fn list_pending_mailbox_messages_for_agent(
        &self,
        agent_id: &JsString,
    ) -> Result<Vec<AgentMailboxMessage>, MultiAgentStoreError> {
        Ok(self
            .list_mailbox_messages()?
            .into_iter()
            .filter(|message| message.to_agent_id == *agent_id && message.status == MailboxMessageStatusValue::Pending)
            .collect())
    }
    pub fn mark_mailbox_message_delivered(
        &self,
        message_id: impl Into<JsString>,
    ) -> Result<Option<AgentMailboxMessage>, MultiAgentStoreError> {
        let message_id = message_id.into();
        let Some(message) = self.lock().mailbox_messages.get(&message_id).cloned() else {
            return Ok(None);
        };
        if self.mailbox_message_is_expired(&message)? {
            return Ok(None);
        }
        if let Some(target) = self.get_persistence_target() {
            crate::core::session_control_db::consume_runtime_mailbox_message_by_store_ref(
                &target.control_db_path,
                &crate::core::session_control_db::RuntimeMailboxStoreRef {
                    session_path: target.session_path,
                    message_id: message_id.to_owned().into(),
                },
            )?;
        }
        self.remove_mailbox_message(&message_id);
        Ok(Some(copy_message(&message)?))
    }
    pub fn mark_mailbox_message_failed(
        &self,
        message_id: impl Into<JsString>,
        error: impl Into<JsString>,
    ) -> Result<Option<AgentMailboxMessage>, MultiAgentStoreError> {
        let message_id = message_id.into();
        let error = error.into();
        let Some(message) = self.lock().mailbox_messages.get(&message_id).cloned() else {
            return Ok(None);
        };
        if message.status != MailboxMessageStatusValue::Pending || self.mailbox_message_is_expired(&message)? {
            return Ok(None);
        }
        if let Some(target) = self.get_persistence_target()
            && !crate::core::session_control_db::mark_multi_agent_mailbox_message_failed(
                &target.control_db_path,
                &target.session_path,
                &message_id,
                &error,
            )?
        {
            self.remove_mailbox_message(&message_id);
            return Ok(None);
        }
        let updated = AgentMailboxMessage {
            error: Some(error.clone()),
            status: MailboxMessageStatusValue::Failed,
            updated_at: Some(self.now()?),
            ..message
        };
        self.lock()
            .mailbox_messages
            .insert(message_id.to_owned().into(), updated.clone());
        Ok(Some(copy_message(&updated)?))
    }
    pub fn consume_completion_notifications_for_agent(
        &self,
        agent_id: &JsString,
    ) -> Result<Vec<AgentMailboxMessage>, MultiAgentStoreError> {
        self.consume_lifecycle_notifications_for_agent(agent_id, LifecycleNotificationLifecycle::Completed)
    }
    pub fn consume_failure_notifications_for_agent(
        &self,
        agent_id: &JsString,
    ) -> Result<Vec<AgentMailboxMessage>, MultiAgentStoreError> {
        self.consume_lifecycle_notifications_for_agent(agent_id, LifecycleNotificationLifecycle::Failed)
    }
    fn consume_lifecycle_notifications_for_agent(
        &self,
        agent_id: &JsString,
        lifecycle: LifecycleNotificationLifecycle,
    ) -> Result<Vec<AgentMailboxMessage>, MultiAgentStoreError> {
        let mut consumed = Vec::new();
        for message in self.list_mailbox_messages()? {
            if is_pending_lifecycle_notification(&message, agent_id, lifecycle)
                && let Some(delivered) = self.mark_mailbox_message_delivered(&message.id)?
            {
                consumed.push(delivered)
            }
        }
        Ok(consumed)
    }
    pub fn list_pending_lifecycle_notifications_for_agent(
        &self,
        agent_id: &JsString,
        lifecycle: LifecycleNotificationLifecycle,
    ) -> Result<Vec<AgentMailboxMessage>, MultiAgentStoreError> {
        Ok(self
            .list_mailbox_messages()?
            .into_iter()
            .filter(|message| is_pending_lifecycle_notification(message, agent_id, lifecycle))
            .collect())
    }
    pub fn get_projection_snapshot(&self) -> Result<MultiAgentProjectionSnapshot, MultiAgentStoreError> {
        Ok(MultiAgentProjectionSnapshot {
            active_count: self.get_active_agent_count(),
            agents: self.list_agents(),
            mailbox_messages: self.list_mailbox_messages()?,
            rows: self.list_row_projections(),
            selected_agent_id: self.get_selected_agent_id(),
            slots: self.list_slot_projections(),
        })
    }

    pub fn list_active_agents(&self) -> Vec<AgentSnapshot> {
        self.list_agents()
            .into_iter()
            .filter(|agent| is_active_lifecycle(agent.lifecycle))
            .collect()
    }

    pub fn get_active_agent_count(&self) -> usize {
        self.lock()
            .agents
            .values()
            .filter(|agent| is_active_lifecycle(agent.lifecycle))
            .count()
    }

    pub fn pin_agent_slot(
        &self,
        agent_id: &JsString,
        expected_revision: i64,
        slot_index: i64,
    ) -> Result<AgentMetadataCommandResult, MultiAgentStoreError> {
        let current = self.lock().agents.get(agent_id).cloned();
        let Some(current) = current else {
            return Ok(AgentMetadataCommandResult::NotFound {
                agent_id: agent_id.to_owned(),
            });
        };
        if current.revision != expected_revision {
            return Ok(AgentMetadataCommandResult::StaleRevision {
                current: Box::new(copy_agent(&current)),
                projection: Box::new(self.get_projection_snapshot()?),
            });
        }
        if let Some(occupant) = self.find_slot_occupant(slot_index, &current.id) {
            return Ok(AgentMetadataCommandResult::SlotConflict {
                current: Box::new(copy_agent(&current)),
                occupant: Box::new(copy_agent(&occupant)),
                projection: Box::new(self.get_projection_snapshot()?),
            });
        }
        let updated = self.update_agent_metadata(
            &current,
            MetadataUpdate {
                slot: Some(Some(AgentSlot {
                    index: slot_index,
                    pinned: true,
                })),
                ..MetadataUpdate::default()
            },
            None,
        )?;
        Ok(AgentMetadataCommandResult::Ok(Box::new(copy_agent(&updated))))
    }

    pub fn publish_agent_current_activity(
        &self,
        agent_id: &JsString,
        current_activity: Option<AgentCurrentActivity>,
        ownership: Option<&AgentCurrentActivityOwner>,
        model: Option<&AgentModel>,
    ) -> Result<Option<AgentSnapshot>, MultiAgentStoreError> {
        let current = self.lock().agents.get(agent_id).cloned();
        let Some(current) = current else {
            return Ok(None);
        };
        if current_activity.is_some()
            && current.lifecycle != AgentLifecycle::Running
            && current.lifecycle != AgentLifecycle::SteeringPending
        {
            return Ok(None);
        }
        if self.lock().persistence.is_some() && ownership.is_none() {
            return Err(MultiAgentStoreError::new(js_format!(
                "Persisted agent {} activity update requires exact runtime ownership",
                agent_id
            )));
        }
        let updated = self.update_agent_metadata(
            &current,
            MetadataUpdate {
                current_activity: Some(current_activity),
                model: model.cloned(),
                ..MetadataUpdate::default()
            },
            ownership,
        )?;
        Ok(Some(copy_agent(&updated)))
    }

    pub fn publish_agent_model(
        &self,
        agent_id: &JsString,
        model: &AgentModel,
        ownership: Option<&AgentCurrentActivityOwner>,
    ) -> Result<Option<AgentSnapshot>, MultiAgentStoreError> {
        let Some(current) = self.get_agent(agent_id) else {
            return Ok(None);
        };
        Ok(Some(self.update_agent_metadata(
            &current,
            MetadataUpdate {
                model: Some(model.clone()),
                ..MetadataUpdate::default()
            },
            ownership,
        )?))
    }

    pub fn update_agent_transcript(
        &self,
        agent_id: &JsString,
        transcript: &AgentTranscriptMetadata,
    ) -> Result<AgentTranscriptCommandResult, MultiAgentStoreError> {
        let current = self.lock().agents.get(agent_id).cloned();
        let Some(current) = current else {
            return Ok(AgentTranscriptCommandResult::NotFound {
                agent_id: agent_id.to_owned(),
            });
        };
        validate_optional_path(transcript.path.as_ref(), "update_agent_transcript.path")?;
        let updated = self.update_agent_metadata(
            &current,
            MetadataUpdate {
                transcript: Some(Some(
                    copy_transcript(Some(transcript))?.unwrap_or_else(|| transcript.clone()),
                )),
                ..MetadataUpdate::default()
            },
            None,
        )?;
        Ok(AgentTranscriptCommandResult::Ok(Box::new(copy_agent(&updated))))
    }

    pub fn clear_agent_slot(
        &self,
        agent_id: &JsString,
        expected_revision: i64,
    ) -> Result<AgentMetadataCommandResult, MultiAgentStoreError> {
        let current = self.lock().agents.get(agent_id).cloned();
        let Some(current) = current else {
            return Ok(AgentMetadataCommandResult::NotFound {
                agent_id: agent_id.to_owned(),
            });
        };
        if current.revision != expected_revision {
            return Ok(AgentMetadataCommandResult::StaleRevision {
                current: Box::new(copy_agent(&current)),
                projection: Box::new(self.get_projection_snapshot()?),
            });
        }
        let updated = self.update_agent_metadata(
            &current,
            MetadataUpdate {
                slot: Some(None),
                ..MetadataUpdate::default()
            },
            None,
        )?;
        Ok(AgentMetadataCommandResult::Ok(Box::new(copy_agent(&updated))))
    }

    pub fn contact_parent(
        &self,
        agent_id: &JsString,
        input: &ContactParentInput,
    ) -> Result<ParentContactResult, MultiAgentStoreError> {
        let current = self.lock().agents.get(agent_id).cloned();
        let Some(current) = current else {
            return Ok(ParentContactResult::NotFound {
                agent_id: agent_id.to_owned().into(),
            });
        };
        let file_refs = validate_mailbox_file_refs(input.file_refs.as_ref(), "contact_parent")?;
        let Some(parent_id) = current.parent_id.clone().filter(|parent| !parent.is_empty()) else {
            return Ok(ParentContactResult::ParentNotFound {
                current: Box::new(copy_agent(&current)),
            });
        };
        let timestamp = self.now()?;
        let message = AgentMailboxMessage {
            extra: JsObject::new(),
            id: self.create_message_id()?.into(),
            thread_id: input.thread_id.clone(),
            from_agent_id: current.id.clone().into(),
            to_agent_id: parent_id.into(),
            kind: MailboxMessageKindValue::ParentRequest,
            status: MailboxMessageStatusValue::Pending,
            created_at: timestamp.clone(),
            updated_at: Some(timestamp),
            body: Some(input.body.clone()),
            file_refs: file_refs
                .as_ref()
                .map(js_json::to_value)
                .transpose()
                .map_err(|error| MultiAgentStoreError::new(error.to_string()))?,
            target_checkpoint: None,
            error: None,
        };
        self.put_mailbox_message(message.clone(), DIRECT_MESSAGE_FIELDS)?;
        let updated = self.update_agent_metadata(
            &current,
            MetadataUpdate {
                last_activity: Some(Some(AgentActivity {
                    description: "Contacted parent".into(),
                    tool_name: None,
                })),
                ..MetadataUpdate::default()
            },
            None,
        )?;
        Ok(ParentContactResult::Ok {
            agent: Box::new(copy_agent(&updated)),
            message: Box::new(copy_message(&message)?),
        })
    }

    pub fn send_main_thread_mailbox_message(
        &self,
        input: &SendMailboxMessageInput,
    ) -> Result<MailboxMessageCommandResult, MultiAgentStoreError> {
        let target = self.lock().agents.get(&input.to_agent_id).cloned();
        let main_thread = self.create_main_thread_snapshot()?;
        let Some(target) = target else {
            return Ok(MailboxMessageCommandResult::TargetNotFound {
                current: Box::new(main_thread),
                target_id: input.to_agent_id.clone(),
            });
        };
        if !is_main_thread_child(&target) {
            return Ok(MailboxMessageCommandResult::ForbiddenTarget {
                current: Box::new(main_thread),
                target: Box::new(copy_agent(&target)),
            });
        }
        let timestamp = self.now()?;
        let message = AgentMailboxMessage {
            extra: JsObject::new(),
            id: self.create_message_id()?.into(),
            thread_id: input.thread_id.clone(),
            from_agent_id: MAIN_THREAD_AGENT_ID.to_owned().into(),
            to_agent_id: target.id.clone().into(),
            kind: MailboxMessageKindValue::Message,
            status: MailboxMessageStatusValue::Pending,
            created_at: timestamp.clone(),
            updated_at: Some(timestamp),
            body: Some(input.body.clone()),
            file_refs: validate_mailbox_file_refs(input.file_refs.as_ref(), "send_agent_message")?,
            target_checkpoint: None,
            error: None,
        };
        self.put_mailbox_message(message.clone(), DIRECT_MESSAGE_FIELDS)?;
        let updated = self.update_agent_metadata(
            &target,
            MetadataUpdate {
                last_activity: Some(Some(AgentActivity {
                    description: "Received mailbox message".into(),
                    tool_name: None,
                })),
                ..MetadataUpdate::default()
            },
            None,
        )?;
        Ok(MailboxMessageCommandResult::Ok {
            agent: Box::new(copy_agent(&updated)),
            message: Box::new(copy_message(&message)?),
        })
    }

    pub fn send_mailbox_message(
        &self,
        agent_id: &JsString,
        expected_revision: i64,
        input: &SendMailboxMessageInput,
    ) -> Result<MailboxMessageCommandResult, MultiAgentStoreError> {
        let current = self.lock().agents.get(agent_id).cloned();
        let Some(current) = current else {
            return Ok(MailboxMessageCommandResult::NotFound {
                agent_id: agent_id.to_owned().into(),
            });
        };
        if let Some(result) = check_revision(&current, expected_revision) {
            return Ok(result);
        }
        let target = self.lock().agents.get(&input.to_agent_id).cloned();
        let Some(target) = target else {
            return Ok(MailboxMessageCommandResult::TargetNotFound {
                current: Box::new(copy_agent(&current)),
                target_id: input.to_agent_id.clone(),
            });
        };
        if !self.can_send_direct_message(&current.id, &target.id) {
            return Ok(MailboxMessageCommandResult::ForbiddenTarget {
                current: Box::new(copy_agent(&current)),
                target: Box::new(copy_agent(&target)),
            });
        }
        let timestamp = self.now()?;
        let message = AgentMailboxMessage {
            extra: JsObject::new(),
            id: self.create_message_id()?.into(),
            thread_id: input.thread_id.clone(),
            from_agent_id: current.id.clone().into(),
            to_agent_id: target.id.clone().into(),
            kind: MailboxMessageKindValue::Message,
            status: MailboxMessageStatusValue::Pending,
            created_at: timestamp.clone(),
            updated_at: Some(timestamp),
            body: Some(input.body.clone()),
            file_refs: validate_mailbox_file_refs(input.file_refs.as_ref(), "send_agent_message")?,
            target_checkpoint: None,
            error: None,
        };
        self.put_mailbox_message(message.clone(), DIRECT_MESSAGE_FIELDS)?;
        let updated = self.update_agent_metadata(
            &current,
            MetadataUpdate {
                last_activity: Some(Some(AgentActivity {
                    description: "Sent mailbox message".into(),
                    tool_name: None,
                })),
                ..MetadataUpdate::default()
            },
            None,
        )?;
        Ok(MailboxMessageCommandResult::Ok {
            agent: Box::new(copy_agent(&updated)),
            message: Box::new(copy_message(&message)?),
        })
    }

    /// Record a message addressed to another session; the target is not an agent in this store.
    pub fn record_outbound_session_message(
        &self,
        input: &RecordOutboundSessionMessageInput,
    ) -> Result<AgentMailboxMessage, MultiAgentStoreError> {
        let timestamp = self.now()?;
        let message = AgentMailboxMessage {
            extra: JsObject::new(),
            file_refs: validate_mailbox_file_refs(input.file_refs.as_ref(), "record_outbound_session_message")?,
            body: Some(input.body.clone()),
            created_at: timestamp.clone(),
            from_agent_id: input.from_agent_id.clone().into(),
            id: self.create_message_id()?.into(),
            kind: MailboxMessageKindValue::Message,
            status: MailboxMessageStatusValue::Pending,
            thread_id: input.thread_id.clone(),
            to_agent_id: input.to_agent_id.clone(),
            updated_at: Some(timestamp),
            target_checkpoint: None,
            error: None,
        };
        self.put_mailbox_message(message.clone(), OUTBOUND_MESSAGE_FIELDS)?;
        copy_message(&message)
    }

    pub fn get_restore_generation(&self) -> u64 {
        self.lock().restore_generation
    }

    pub fn get_persistence_target(&self) -> Option<PersistenceTarget> {
        self.lock().persistence.clone()
    }

    pub fn invalidate_in_flight_dispatches(&self) {
        self.lock().restore_generation += 1;
    }

    pub fn set_persistence_session_manager(&self, session: Option<&dyn SessionPersistence>) {
        let target = session.and_then(|session| {
            let control_db_path = session.metadata_control_db_path().filter(|path| !path.is_empty())?;
            let session_path = session.session_file().filter(|path| !path.is_empty())?;
            Some(PersistenceTarget {
                control_db_path,
                session_path,
            })
        });
        self.lock().persistence = target;
    }

    pub fn restore_from_session_manager(&self, session: &dyn SessionPersistence) -> Result<(), MultiAgentStoreError> {
        self.set_persistence_session_manager(Some(session));
        self.lock().restore_generation += 1;
        let state = match self.get_persistence_target() {
            Some(target) => read_multi_agent_state(&target.control_db_path, &target.session_path)?,
            None => None,
        };
        self.restore_state(state)
    }

    pub fn from_session_manager(
        session: &dyn SessionPersistence,
        options: MultiAgentStoreOptions,
    ) -> Result<Self, MultiAgentStoreError> {
        let store = Self::new(options);
        store.restore_from_session_manager(session)?;
        Ok(store)
    }

    fn restore_state(&self, state: Option<MultiAgentPersistedState>) -> Result<(), MultiAgentStoreError> {
        let mut current = self.lock();
        current.agents.clear();
        current.agent_order.clear();
        current.mailbox_messages.clear();
        current.mailbox_order.clear();
        current.abort_handlers.clear();
        self.lifecycle_notifications.clear()?;
        self.transitions.clear()?;
        current.selected_agent_id = None;
        current.next_agent_number = state.as_ref().map_or(1, |state| state.counters.next_agent_number);
        current.next_message_number = state.as_ref().map_or(1, |state| state.counters.next_message_number);
        let Some(state) = state else {
            return Ok(());
        };
        for agent in state.agents {
            if let Ok(agent) = js_json::from_value::<AgentSnapshot>(&agent) {
                validate_agent_paths(&agent)?;
                let restored = restore_agent_snapshot(agent);
                current.agent_order.push(restored.id.clone());
                current.agents.insert(restored.id.clone(), restored);
            }
        }
        drop(current);
        let now = self.now()?;
        let now = pi_js_compat::date::date_parse_utf16(now.units());
        for message in state.mailbox_messages {
            if let Ok(message) = js_json::from_value::<AgentMailboxMessage>(&message) {
                if crate::core::session_control_db::is_mailbox_message_expired(Some(&message.created_at), now) {
                    continue;
                }
                let mut current = self.lock();
                current.mailbox_order.push(message.id.clone());
                current
                    .mailbox_messages
                    .insert(message.id.clone(), copy_message(&message)?);
            }
        }
        Ok(())
    }

    fn put_mailbox_message(&self, message: AgentMailboxMessage, fields: &[&str]) -> Result<(), MultiAgentStoreError> {
        if self.mailbox_message_is_expired(&message)? {
            return Ok(());
        }
        let persistence = {
            let mut state = self.lock();
            if !state.mailbox_messages.contains_key(&message.id) {
                state.mailbox_order.push(message.id.clone());
            }
            state.mailbox_messages.insert(message.id.clone(), message.clone());
            state.persistence.clone()
        };
        if let Some(target) = persistence {
            let value = encode_mailbox_constructor(&message, fields)?;
            upsert_multi_agent_mailbox_message(&target.control_db_path, &target.session_path, &message.id, &value)?;
        }
        Ok(())
    }

    fn list_slot_projections(&self) -> Vec<AgentSlotProjection> {
        let state = self.lock();
        let mut slots: Vec<AgentSlotProjection> = state
            .agent_order
            .iter()
            .filter_map(|id| state.agents.get(id))
            .filter_map(|agent| {
                agent.slot.as_ref().map(|slot| AgentSlotProjection {
                    agent_id: agent.id.clone(),
                    agent: copy_agent(agent),
                    index: slot.index,
                    pinned: slot.pinned,
                    revision: agent.revision,
                })
            })
            .collect();
        slots.sort_by(|left, right| {
            left.index.cmp(&right.index).then_with(|| {
                pi_js_compat::collator::locale_compare_utf16(left.agent_id.units(), right.agent_id.units())
            })
        });
        slots
    }

    fn list_row_projections(&self) -> Vec<AgentRowProjection> {
        let state = self.lock();
        let selected = state.selected_agent_id.clone();
        state
            .agent_order
            .iter()
            .filter_map(|id| state.agents.get(id))
            .map(|agent| AgentRowProjection {
                active: is_active_lifecycle(agent.lifecycle),
                agent_id: agent.id.clone(),
                display_name: agent.display_name.clone(),
                lifecycle: agent.lifecycle,
                revision: agent.revision,
                selected: selected.as_ref() == Some(&agent.id),
                slot_index: agent.slot.as_ref().map(|slot| slot.index),
                worker_adapter: agent.worker.as_ref().map(|worker| worker.adapter),
            })
            .collect()
    }

    fn find_slot_occupant(&self, slot_index: i64, excluded_agent_id: &JsString) -> Option<AgentSnapshot> {
        let state = self.lock();
        state
            .agent_order
            .iter()
            .filter_map(|id| state.agents.get(id))
            .find(|agent| {
                agent.id != *excluded_agent_id && agent.slot.as_ref().is_some_and(|slot| slot.index == slot_index)
            })
            .cloned()
    }

    fn can_send_direct_message(&self, from_agent_id: &JsString, to_agent_id: &JsString) -> bool {
        let state = self.lock();
        let sender = state.agents.get(from_agent_id);
        let recipient = state.agents.get(to_agent_id);
        sender.and_then(|agent| agent.parent_id.as_ref()) == Some(to_agent_id)
            || recipient.and_then(|agent| agent.parent_id.as_ref()) == Some(from_agent_id)
    }

    fn create_main_thread_snapshot(&self) -> Result<AgentSnapshot, MultiAgentStoreError> {
        let timestamp = self.now()?;
        Ok(AgentSnapshot {
            agent_type: "main".into(),
            created_at: timestamp.clone(),
            cwd: JsString::default(),
            display_name: "Main thread".into(),
            id: MAIN_THREAD_AGENT_ID.into(),
            lifecycle: AgentLifecycle::Running,
            parent_id: None,
            permission: AgentPermission {
                narrowed: true,
                policy: "on-request".into(),
                inherited_from: None,
            },
            revision: 0,
            updated_at: timestamp,
            origin: None,
            worktree: None,
            model: None,
            account: None,
            slot: None,
            transcript: None,
            event_stream: None,
            worker: None,
            detached: None,
            current_activity: None,
            last_activity: None,
            result: None,
            error: None,
            extra: JsObject::new(),
        })
    }

    fn persist_agent_metadata(
        &self,
        current: &AgentNode,
        updates: &MetadataUpdate,
        updated_at: &JsString,
        activity_ownership: Option<&AgentCurrentActivityOwner>,
    ) -> Result<Option<AgentNode>, MultiAgentStoreError> {
        let Some(target) = self.get_persistence_target() else {
            return Ok(None);
        };
        let encode = |value| js_json::to_value(value).map_err(|error| MultiAgentStoreError::new(error.to_string()));
        let model = updates
            .model
            .as_ref()
            .map(js_json::to_value)
            .transpose()
            .map_err(|error| MultiAgentStoreError::new(error.to_string()))?;
        let persisted = if let Some(activity) = &updates.current_activity {
            let ownership = activity_ownership.ok_or_else(|| {
                MultiAgentStoreError::new(js_format!(
                    "Persisted agent {} activity update requires exact runtime ownership",
                    current.id
                ))
            })?;
            let value = js_json::to_value(activity).map_err(|error| MultiAgentStoreError::new(error.to_string()))?;
            update_multi_agent_agent_current_activity(
                &target.control_db_path,
                &target.session_path,
                &current.id,
                Some(&value),
                updated_at,
                ownership,
                model.as_ref(),
            )?
        } else if let Some(model) = &model {
            let ownership = activity_ownership.ok_or_else(|| {
                MultiAgentStoreError::new(js_format!(
                    "Persisted agent {} model metadata update requires exact runtime ownership",
                    current.id
                ))
            })?;
            crate::core::session_control_db::update_multi_agent_agent_model(
                &target.control_db_path,
                &target.session_path,
                &current.id,
                model,
                updated_at,
                ownership,
            )?
        } else if let Some(transcript) = &updates.transcript {
            let value = js_json::to_value(transcript).map_err(|error| MultiAgentStoreError::new(error.to_string()))?;
            update_multi_agent_agent_transcript(
                &target.control_db_path,
                &target.session_path,
                &current.id,
                Some(&value),
                updated_at,
            )?
        } else if let Some(slot) = &updates.slot {
            let value = js_json::to_value(slot).map_err(|error| MultiAgentStoreError::new(error.to_string()))?;
            update_multi_agent_agent_slot(
                &target.control_db_path,
                &target.session_path,
                &current.id,
                Some(&value),
                updated_at,
            )?
        } else {
            let value = encode(&updates.last_activity)?;
            update_multi_agent_agent_activity(
                &target.control_db_path,
                &target.session_path,
                &current.id,
                Some(&value),
                updated_at,
            )?
        };
        persisted
            .map(|value| js_json::from_value(&value))
            .transpose()
            .map_err(|error| MultiAgentStoreError::new(error.to_string()))
    }

    fn update_agent_metadata(
        &self,
        current: &AgentNode,
        updates: MetadataUpdate,
        activity_ownership: Option<&AgentCurrentActivityOwner>,
    ) -> Result<AgentNode, MultiAgentStoreError> {
        let updated_at = self.now()?;
        let persisted = self.persist_agent_metadata(current, &updates, &updated_at, activity_ownership)?;
        let persistence = self.lock().persistence.is_some();
        if persistence && persisted.is_none() {
            return Err(MultiAgentStoreError::new(js_format!(
                "Persisted agent {} metadata update was rejected",
                current.id
            )));
        }
        let updated = persisted.unwrap_or_else(|| updates.apply(current, &updated_at));
        {
            let mut state = self.lock();
            if !state.agents.contains_key(&updated.id) {
                state.agent_order.push(updated.id.clone());
            }
            state.agents.insert(updated.id.clone(), updated.clone());
        }
        self.notify_agent_update_listeners(current, &updated);
        Ok(updated)
    }

    fn retry_or_record_terminal_notification(
        &self,
        agent: &AgentNode,
        lifecycle: LifecycleNotificationLifecycle,
    ) -> Result<(), MultiAgentStoreError> {
        let existing = self
            .list_pending_lifecycle_notifications_for_agent(&agent.id, lifecycle)?
            .into_iter()
            .next();
        if let Some(existing) = existing {
            self.notify_lifecycle_notification_listeners(&existing)?;
            return Ok(());
        }
        match lifecycle {
            LifecycleNotificationLifecycle::Completed => self.record_completion_notification(agent),
            LifecycleNotificationLifecycle::Failed => self.record_failure_notification(agent),
            LifecycleNotificationLifecycle::WaitingForInput => Ok(()),
        }
    }
    fn record_completion_notification(&self, agent: &AgentNode) -> Result<(), MultiAgentStoreError> {
        if self.has_pending_lifecycle_notification(&agent.id, LifecycleNotificationLifecycle::Completed)? {
            return Ok(());
        }
        self.record_lifecycle_notification(
            agent,
            format_completion_notification_body(agent),
            copy_file_refs(agent.result.as_ref().and_then(|result| result.file_refs.as_deref())),
            completion_notification_thread_id(&agent.id),
        )
    }
    fn record_failure_notification(&self, agent: &AgentNode) -> Result<(), MultiAgentStoreError> {
        if self.has_pending_lifecycle_notification(&agent.id, LifecycleNotificationLifecycle::Failed)? {
            return Ok(());
        }
        self.record_lifecycle_notification(
            agent,
            format_failure_notification_body(agent),
            copy_file_refs(agent.result.as_ref().and_then(|result| result.file_refs.as_deref())),
            failed_notification_thread_id(&agent.id),
        )
    }
    fn record_waiting_for_input_notification(&self, agent: &AgentNode) -> Result<(), MultiAgentStoreError> {
        if self.has_pending_lifecycle_notification(&agent.id, LifecycleNotificationLifecycle::WaitingForInput)? {
            return Ok(());
        }
        self.record_lifecycle_notification(
            agent,
            format_waiting_for_input_notification_body(agent),
            None,
            waiting_for_input_notification_thread_id(&agent.id),
        )
    }
    fn record_lifecycle_notification(
        &self,
        agent: &AgentNode,
        body: JsString,
        file_refs: Option<Vec<AgentFileReference>>,
        thread_id: JsString,
    ) -> Result<(), MultiAgentStoreError> {
        let timestamp = self.now()?;
        if crate::core::session_control_db::is_mailbox_message_expired(
            Some(&agent.updated_at),
            pi_js_compat::date::date_parse_utf16(timestamp.units()),
        ) {
            return Ok(());
        }
        let message = AgentMailboxMessage {
            extra: JsObject::new(),
            body: Some(body.into()),
            created_at: timestamp.clone(),
            from_agent_id: agent.id.clone().into(),
            id: self.create_message_id()?.into(),
            kind: MailboxMessageKindValue::System,
            status: MailboxMessageStatusValue::Pending,
            thread_id: Some(thread_id.into()),
            to_agent_id: agent.parent_id.clone().unwrap_or_else(|| MAIN_THREAD_AGENT_ID.into()),
            updated_at: Some(timestamp),
            file_refs: file_refs
                .as_ref()
                .map(js_json::to_value)
                .transpose()
                .map_err(|error| MultiAgentStoreError::new(error.to_string()))?,
            target_checkpoint: None,
            error: None,
        };
        self.put_mailbox_message(message.clone(), LIFECYCLE_MESSAGE_FIELDS)?;
        self.notify_lifecycle_notification_listeners(&message)?;
        Ok(())
    }

    fn notify_lifecycle_notification_listeners(
        &self,
        message: &AgentMailboxMessage,
    ) -> Result<(), MultiAgentStoreError> {
        let snapshot = copy_message(message)?;
        let mut cursor = 0;
        while let Some((id, listener)) = next_listener(&self.lifecycle_notifications, cursor) {
            cursor = id;
            if let Err(error) = listener(&snapshot) {
                report_callback_error("MultiAgentStore lifecycle notification listener failed:", &error);
            }
        }
        Ok(())
    }

    fn notify_transition_listeners_if_lifecycle_changed(&self, previous: &AgentNode, current: &AgentNode) {
        if previous.lifecycle != current.lifecycle {
            self.notify_transition_listeners(previous, current);
        }
    }

    fn notify_transition_listeners(&self, previous: &AgentNode, current: &AgentNode) {
        let previous_snapshot = copy_agent(previous);
        let current_snapshot = copy_agent(current);
        let mut cursor = 0;
        while let Some((id, listener)) = next_listener(&self.transitions, cursor) {
            cursor = id;
            if let Err(error) = listener(&previous_snapshot, &current_snapshot) {
                report_callback_error("MultiAgentStore transition listener failed:", &error);
            }
        }
    }

    fn notify_agent_update_listeners(&self, previous: &AgentNode, current: &AgentNode) {
        let previous_snapshot = copy_agent(previous);
        let current_snapshot = copy_agent(current);
        let mut cursor = 0;
        while let Some((id, listener)) = next_listener(&self.updates, cursor) {
            cursor = id;
            if let Err(error) = listener(&previous_snapshot, &current_snapshot) {
                report_callback_error("MultiAgentStore agent update listener failed:", &error);
            }
        }
    }

    fn has_pending_lifecycle_notification(
        &self,
        agent_id: &JsString,
        lifecycle: LifecycleNotificationLifecycle,
    ) -> Result<bool, MultiAgentStoreError> {
        Ok(self
            .list_mailbox_messages()?
            .iter()
            .any(|message| is_pending_lifecycle_notification(message, agent_id, lifecycle)))
    }

    fn create_agent_id(&self, prefix: &str) -> Result<JsString, MultiAgentStoreError> {
        let allocated = self.allocate_counter(CounterName::Agent)?;
        Ok(format!("{prefix}_{allocated}").into())
    }

    fn create_message_id(&self) -> Result<JsString, MultiAgentStoreError> {
        let allocated = self.allocate_counter(CounterName::Message)?;
        Ok(format!("message_{allocated}").into())
    }

    fn allocate_counter(&self, counter_name: CounterName) -> Result<i64, MultiAgentStoreError> {
        let persistence = self.lock().persistence.clone();
        if let Some(target) = persistence {
            let allocated =
                allocate_multi_agent_counter(&target.control_db_path, &target.session_path, counter_name.into())?;
            let mut state = self.lock();
            state.update_counter(counter_name, allocated + 1);
            return Ok(allocated);
        }
        let mut state = self.lock();
        let current = match counter_name {
            CounterName::Agent => state.next_agent_number,
            CounterName::Message => state.next_message_number,
        };
        state.update_counter(counter_name, current + 1);
        Ok(current)
    }
}

#[derive(Debug, Clone, Copy)]
enum CounterName {
    Agent,
    Message,
}

impl CounterName {
    fn as_str(self) -> &'static str {
        match self {
            Self::Agent => "agent",
            Self::Message => "message",
        }
    }
}

impl StoreState {
    fn update_counter(&mut self, counter_name: CounterName, next_value: i64) {
        match counter_name {
            CounterName::Agent => self.next_agent_number = next_value,
            CounterName::Message => self.next_message_number = next_value,
        }
    }
}

/// Counter name as the control-db API expects it.
impl From<CounterName> for crate::core::session_control_db::MultiAgentCounterName {
    fn from(value: CounterName) -> Self {
        match value {
            CounterName::Agent => Self::Agent,
            CounterName::Message => Self::Message,
        }
    }
}

impl std::fmt::Display for CounterName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Abort-handler registration; dropping it unregisters the handler.
pub struct AbortRegistration {
    state: Arc<Mutex<StoreState>>,
    agent_id: JsString,
    token: Arc<()>,
}

impl Drop for AbortRegistration {
    fn drop(&mut self) {
        let mut state = self.state.lock().expect("multi-agent store mutex");
        let is_current = state
            .abort_handlers
            .get(&self.agent_id)
            .is_some_and(|handler| Arc::ptr_eq(&handler.token, &self.token));
        if is_current {
            state.abort_handlers.remove(&self.agent_id);
        }
    }
}

#[derive(Default)]
struct MetadataUpdate {
    model: Option<AgentModel>,
    current_activity: Option<Option<AgentCurrentActivity>>,
    last_activity: Option<Option<AgentActivity>>,
    slot: Option<Option<AgentSlot>>,
    transcript: Option<Option<AgentTranscriptMetadata>>,
}

impl MetadataUpdate {
    fn apply(&self, current: &AgentNode, updated_at: &JsString) -> AgentNode {
        let mut updated = current.clone();
        updated.updated_at = updated_at.clone();
        if let Some(model) = &self.model {
            updated.model = Some(model.clone());
        }
        if let Some(current_activity) = &self.current_activity {
            updated.current_activity = current_activity.clone();
        }
        if let Some(last_activity) = &self.last_activity {
            updated.last_activity = last_activity.clone();
        }
        if let Some(slot) = &self.slot {
            updated.slot = slot.clone();
        }
        if let Some(transcript) = &self.transcript {
            updated.transcript = transcript.clone();
        }
        updated
    }
}

fn subscription(list: Arc<dyn Fn(u64) + Send + Sync>, id: u64) -> AgentSubscription {
    AgentSubscription { list, id }
}

impl<T: Send + Sync + 'static> ListenerList<T> {
    fn subscription_remover(list: Arc<Self>) -> Arc<dyn Fn(u64) + Send + Sync> {
        Arc::new(move |id| list.remove(id))
    }
}

fn clear_selected_terminal_agent(state: &mut StoreState, agent: &AgentSnapshot) {
    if state.selected_agent_id.as_ref() == Some(&agent.id) && is_terminal_lifecycle(agent.lifecycle) {
        state.selected_agent_id = None;
    }
}

fn visit_children(state: &StoreState, parent_id: &JsString, descendants: &mut Vec<AgentNode>) {
    for agent_id in &state.agent_order {
        let Some(agent) = state.agents.get(agent_id) else {
            continue;
        };
        if agent.parent_id.as_ref() != Some(parent_id) {
            continue;
        }
        descendants.push(agent.clone());
        visit_children(state, &agent.id, descendants);
    }
}

fn find_agent_for_slot_selection(state: &StoreState, slot_index: i64) -> Option<AgentNode> {
    let pinned = state.agent_order.iter().find_map(|agent_id| {
        let agent = state.agents.get(agent_id)?;
        (agent.slot.as_ref().is_some_and(|slot| slot.index == slot_index) && is_active_lifecycle(agent.lifecycle))
            .then(|| agent.clone())
    });
    pinned.or_else(|| {
        let active: Vec<&AgentNode> = state
            .agent_order
            .iter()
            .filter_map(|agent_id| state.agents.get(agent_id))
            .filter(|agent| is_active_lifecycle(agent.lifecycle))
            .collect();
        usize::try_from(slot_index - 1)
            .ok()
            .and_then(|index| active.get(index))
            .map(|agent| (*agent).clone())
    })
}

fn check_revision(current: &AgentNode, expected_revision: i64) -> Option<MailboxMessageCommandResult> {
    (current.revision != expected_revision).then(|| MailboxMessageCommandResult::StaleRevision {
        current: Box::new(copy_agent(current)),
    })
}

fn is_main_thread_child(agent: &AgentNode) -> bool {
    agent
        .parent_id
        .as_ref()
        .is_none_or(|parent_id| *parent_id == MAIN_THREAD_AGENT_ID)
}

pub fn is_active_lifecycle(lifecycle: AgentLifecycleState) -> bool {
    !is_terminal_lifecycle(lifecycle)
}

fn is_terminal_lifecycle(lifecycle: AgentLifecycle) -> bool {
    matches!(
        lifecycle,
        AgentLifecycle::Completed | AgentLifecycle::Failed | AgentLifecycle::Aborted
    )
}

pub fn format_inactive_agent_selection_message(agent: &AgentSnapshot) -> JsString {
    JsString::from("Agent is not active: ") + &agent.display_name + format!(" ({})", lifecycle_name(agent.lifecycle))
}

pub fn lifecycle_name(lifecycle: AgentLifecycle) -> &'static str {
    match lifecycle {
        AgentLifecycle::Running => "running",
        AgentLifecycle::WaitingForInput => "waiting_for_input",
        AgentLifecycle::SteeringPending => "steering_pending",
        AgentLifecycle::Cancelling => "cancelling",
        AgentLifecycle::Completed => "completed",
        AgentLifecycle::Failed => "failed",
        AgentLifecycle::Aborted => "aborted",
    }
}

fn completion_notification_thread_id(agent_id: &JsString) -> JsString {
    js_format!("{}:{}", COMPLETION_NOTIFICATION_THREAD_PREFIX, agent_id)
}

fn failed_notification_thread_id(agent_id: &JsString) -> JsString {
    js_format!("{}:{}", FAILED_NOTIFICATION_THREAD_PREFIX, agent_id)
}

fn waiting_for_input_notification_thread_id(agent_id: &JsString) -> JsString {
    js_format!("{}:{}", WAITING_FOR_INPUT_NOTIFICATION_THREAD_PREFIX, agent_id)
}

fn is_pending_lifecycle_notification(
    message: &AgentMailboxMessage,
    agent_id: &JsString,
    lifecycle: LifecycleNotificationLifecycle,
) -> bool {
    let is_from_agent = message.from_agent_id == *agent_id;
    let expected_thread_id = lifecycle_notification_thread_id(agent_id, lifecycle);
    let is_lifecycle_notice = message.kind == MailboxMessageKindValue::System
        && message.thread_id.as_ref() == Some(&JsString::from(expected_thread_id));
    is_from_agent && is_lifecycle_notice && message.status == MailboxMessageStatusValue::Pending
}

fn lifecycle_notification_thread_id(agent_id: &JsString, lifecycle: LifecycleNotificationLifecycle) -> JsString {
    match lifecycle {
        LifecycleNotificationLifecycle::Completed => completion_notification_thread_id(agent_id),
        LifecycleNotificationLifecycle::Failed => failed_notification_thread_id(agent_id),
        LifecycleNotificationLifecycle::WaitingForInput => waiting_for_input_notification_thread_id(agent_id),
    }
}

fn format_completion_notification_body(agent: &AgentNode) -> JsString {
    let summary = agent
        .result
        .as_ref()
        .and_then(|result| result.summary.as_ref())
        .map(JsString::trim)
        .filter(|summary| !summary.is_empty());
    let body = match summary {
        Some(summary) => agent.display_name.clone() + " completed: " + summary,
        None => agent.display_name.clone() + " completed.",
    };
    append_agent_duration(body, agent.result.as_ref().and_then(|result| result.duration_ms))
}

fn format_failure_notification_body(agent: &AgentNode) -> JsString {
    let error_message = agent
        .error
        .as_ref()
        .map(|error| error.message.trim())
        .filter(|message| !message.is_empty());
    let body = match error_message {
        Some(message) => agent.display_name.clone() + " failed: " + message,
        None => agent.display_name.clone() + " failed.",
    };
    append_agent_duration(body, agent.result.as_ref().and_then(|result| result.duration_ms))
}

fn append_agent_duration(body: JsString, duration_ms: Option<i64>) -> JsString {
    let Some(duration_ms) = duration_ms else {
        return body;
    };
    let separator = if [".", "!", "?"].iter().any(|suffix| body.ends_with(*suffix)) {
        " "
    } else {
        ". "
    };
    body + format!("{separator}Duration: {duration_ms}ms")
}

fn format_waiting_for_input_notification_body(agent: &AgentNode) -> JsString {
    agent.display_name.clone() + " is waiting for input."
}

fn restore_agent_snapshot(agent: AgentNode) -> AgentNode {
    let restored = copy_agent(&agent);
    if !is_active_lifecycle(restored.lifecycle) || restored.worker.is_none() {
        return restored;
    }
    AgentNode {
        worker: None,
        ..restored
    }
}

pub fn copy_agent(agent: &AgentNode) -> AgentNode {
    agent.clone()
}

pub fn copy_message(message: &AgentMailboxMessage) -> Result<AgentMailboxMessage, MultiAgentStoreError> {
    let mut copied = message.clone();
    copied.extra.clear();
    copied.file_refs = validate_mailbox_file_refs(message.file_refs.as_ref(), "file_reference_copy")?;
    Ok(copied)
}

fn copy_file_refs(refs: Option<&[AgentFileReference]>) -> Option<Vec<AgentFileReference>> {
    refs.map(<[AgentFileReference]>::to_vec)
}

fn copy_transcript(
    transcript: Option<&AgentTranscriptMetadata>,
) -> Result<Option<AgentTranscriptMetadata>, MultiAgentStoreError> {
    let Some(transcript) = transcript else {
        return Ok(None);
    };
    validate_optional_path(transcript.path.as_ref(), "agent_transcript.path")?;
    Ok(Some(transcript.clone()))
}

fn validate_absolute_path(path: &JsString, context: &str) -> Result<(), MultiAgentStoreError> {
    if !node_is_absolute(path) {
        return Err(MultiAgentStoreError::new(js_format!(
            "Invalid path at {}: expected an absolute path, received {}",
            context,
            path
        )));
    }
    Ok(())
}

fn validate_optional_path(path: Option<&JsString>, context: &str) -> Result<(), MultiAgentStoreError> {
    match path {
        Some(path) => validate_absolute_path(path, context),
        None => Ok(()),
    }
}

fn validate_mailbox_file_refs(refs: Option<&JsValue>, context: &str) -> Result<Option<JsValue>, MultiAgentStoreError> {
    let Some(refs) = refs else {
        return Ok(None);
    };
    let references: Vec<AgentFileReference> =
        js_json::from_value(refs).map_err(|error| MultiAgentStoreError::new(error.to_string()))?;
    validate_file_refs(Some(&references), context)?
        .as_ref()
        .map(js_json::to_value)
        .transpose()
        .map_err(|error| MultiAgentStoreError::new(error.to_string()))
}

pub fn validate_file_refs(
    refs: Option<&[AgentFileReference]>,
    context: &str,
) -> Result<Option<Vec<AgentFileReference>>, MultiAgentStoreError> {
    let Some(refs) = refs else {
        return Ok(None);
    };
    let mut validated = Vec::with_capacity(refs.len());
    for (index, reference) in refs.iter().enumerate() {
        if !crate::utils::paths::node_is_absolute(&reference.path) {
            return Err(MultiAgentStoreError::new(js_format!(
                "Invalid file reference at {}[{}]: path must be absolute, received {}",
                context,
                index.to_string(),
                reference.path
            )));
        }
        validated.push(reference.clone());
    }
    Ok(Some(validated))
}

/// Validate copied fields in frozen copyAgent evaluation order before storing an owned snapshot.
pub fn validate_agent_paths(agent: &AgentNode) -> Result<(), MultiAgentStoreError> {
    if let Some(result) = &agent.result {
        validate_file_refs(result.file_refs.as_deref(), "agent_result")?;
    }
    if let Some(event_stream) = &agent.event_stream {
        validate_absolute_path(&event_stream.path, "agent_event_stream.path")?;
    }
    if let Some(transcript) = &agent.transcript {
        validate_optional_path(transcript.path.as_ref(), "agent_transcript.path")?;
    }
    if let Some(worker) = &agent.worker {
        validate_optional_path(worker.cwd.as_ref(), "agent_worker.cwd")?;
    }
    if let Some(worktree) = &agent.worktree {
        validate_absolute_path(&worktree.path, "agent_worktree.path")?;
    }
    Ok(())
}

/// Builds a JSON object for an agent using the store's canonical copy shape.
pub fn agent_to_value(agent: &AgentNode) -> Result<JsValue, MultiAgentStoreError> {
    validate_agent_paths(agent)?;
    js_json::to_value(agent).map_err(|error| MultiAgentStoreError::new(error.to_string()))
}

/// Set of agent ids reachable from `parent_id`, in insertion order.
pub fn descendant_ids(agents: &[AgentNode], parent_id: &JsString) -> Vec<JsString> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    collect_descendant_ids(agents, parent_id, &mut result, &mut seen);
    result
}

fn collect_descendant_ids(
    agents: &[AgentNode],
    parent_id: &JsString,
    result: &mut Vec<JsString>,
    seen: &mut HashSet<JsString>,
) {
    for agent in agents {
        if agent.parent_id.as_ref() != Some(parent_id) || !seen.insert(agent.id.clone()) {
            continue;
        }
        result.push(agent.id.clone());
        collect_descendant_ids(agents, &agent.id, result, seen);
    }
}
