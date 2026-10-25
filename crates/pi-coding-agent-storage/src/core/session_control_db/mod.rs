//! Shared control SQLite database (`core/session-control-db.ts`).
//!
//! Every Pi runtime on the machine opens the same `control.sqlite`. The TypeScript and Rust
//! runtimes must agree on schema, migrations, PRAGMAs, SQL text, and transaction boundaries,
//! so this module keeps the SQL statements verbatim and performs all writes inside
//! `BEGIN IMMEDIATE` transactions.

pub(crate) mod agents;
mod channel;
mod health;
mod incoming;
mod lifecycle;
mod mailbox;
mod requests;
mod retention;
use pi_js_compat::js_format;
use retention::{MAILBOX_CREATED_AT_MS_SQL, mailbox_cutoff_ms};
pub use retention::{MAILBOX_MESSAGE_RETENTION_MS, is_mailbox_message_expired};
mod schema;
mod sessions;

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use parking_lot::ReentrantMutex;
use rusqlite::{Connection, Row, params};
use serde::{Deserialize, Serialize};

use crate::core::runtime_process::{
    ProcessIdentity, is_pi_runtime_process_alive, is_process_identity_alive, read_process_identity,
};
use crate::core::session_health::SessionHealthRecord;
use crate::core::sqlite::{
    DEFAULT_SHARED_SQLITE_BUSY_TIMEOUT_MS, configure_shared_sqlite_database, create_sqlite_database,
};
use pi_js_compat::js_json::{self, JsObject, JsString, JsValue, encode_node_utf8};

pub use agents::{
    AcquireAttachedError, AcquireAttachedRuntimeOwnershipInput, AcquireAttachedRuntimeOwnershipResult,
    AcquireMultiAgentRuntimeOwnershipInput, AgentDetails, ClaimMultiAgentTerminalOutboxOptions,
    CommitMultiAgentDetachMarkInput, CommitMultiAgentDetachMarkResult, CommitMultiAgentLifecycleMutationInput,
    CommitMultiAgentLifecycleMutationResult, CommitMultiAgentSteeringDeliveryInput,
    CommitMultiAgentSteeringDeliveryResult, CommitMultiAgentSteeringMutationInput,
    CommitMultiAgentSteeringMutationResult, CommitMultiAgentTerminalMutationInput,
    CommitMultiAgentTerminalMutationResult, CreateAttachmentError, CreateChildError, CreateFailedMultiAgentChildInput,
    CreateFailedMultiAgentChildResult, CreateMultiAgentAttachmentInput, CreateMultiAgentAttachmentResult,
    CreateMultiAgentChildWithRuntimeOwnershipInput, CreateMultiAgentChildWithRuntimeOwnershipResult,
    DetachedArtifactAgentRecord, DetachedCancellation, LifecycleCommandError, MultiAgentCounterName,
    MultiAgentCounters, MultiAgentPersistedState, MultiAgentTerminalOutboxRecord, RecoverDeadMultiAgentRuntimeInput,
    RecoverDeadMultiAgentRuntimeResult, RecoverDeadRuntimeError, SteeringCommandError, SupervisorRuntimeOwnership,
    TerminalOutboxStatus, acquire_attached_runtime_ownership, allocate_multi_agent_counter,
    bootstrap_multi_agent_agent, claim_multi_agent_terminal_outbox, cleanup_multi_agent_terminal_outbox,
    commit_multi_agent_detach_mark, commit_multi_agent_lifecycle_mutation, commit_multi_agent_steering_delivery,
    commit_multi_agent_steering_mutation, commit_multi_agent_terminal_mutation, create_failed_multi_agent_child,
    create_multi_agent_attachment, create_multi_agent_child_with_runtime_ownership,
    deliver_multi_agent_terminal_outbox, fail_multi_agent_terminal_outbox, get_multi_agent_mailbox_message_status,
    list_detached_artifact_agents_updated_at_or_before, mark_multi_agent_mailbox_message_delivered,
    mark_multi_agent_mailbox_message_failed, read_multi_agent_agent, read_multi_agent_agents,
    read_multi_agent_runtime_ownership, read_multi_agent_state, reconcile_dead_detached_agent_runtimes,
    recover_dead_multi_agent_runtime, update_multi_agent_agent_activity, update_multi_agent_agent_current_activity,
    update_multi_agent_agent_model, update_multi_agent_agent_slot, update_multi_agent_agent_transcript,
    upsert_multi_agent_mailbox_message, write_multi_agent_counters,
};
pub use channel::{
    advance_shared_channel_cursor, has_pending_runtime_coordination_message, initialize_shared_channel_cursor_at_tail,
    is_runtime_coordination_mailbox_message, list_shared_channel_messages_after, post_shared_channel_message,
    read_shared_channel_cursor, read_shared_channel_tail,
};
pub use health::{list_session_health, read_session_health, write_session_health};

pub use incoming::{
    claim_codex_restriction_recovery, claim_latest_incoming_message, complete_incoming_message,
    enqueue_incoming_message, fail_incoming_message, read_incoming_message_status,
};
pub use lifecycle::{
    DetachedJobOutcome, DetachedJobOwnershipIdentity, DetachedJobTerminalInput, DetachedJobTerminalOutput,
    FinalizeDetachedJobInput, FinalizeDetachedJobResult, finalize_detached_job, prepare_control_db_for_self_restart,
};
pub(crate) use mailbox::enqueue_lossless_stored_runtime_mailbox_message_in_transaction;
pub use mailbox::{
    RuntimeMailboxListenerRecord, assert_main_session_runtime_available, claim_runtime_mailbox_messages,
    consume_runtime_mailbox_message_by_store_ref, deliver_runtime_mailbox_message, enqueue_runtime_mailbox_message,
    enqueue_stored_runtime_mailbox_message, enqueue_stored_runtime_mailbox_message_in_transaction,
    fail_runtime_mailbox_message, is_known_local_session, list_runtime_mailbox_listeners,
    list_runtime_mailbox_messages, list_runtime_mailbox_messages_for_session, mark_runtime_mailbox_message_delivered,
    mark_runtime_mailbox_signal_keepalive_installed, notify_runtime_mailbox_recipient,
    read_live_runtime_mailbox_session_agent_ids, read_runtime_mailbox_listener, read_runtime_mailbox_message,
    read_runtime_mailbox_message_for_delivery, recover_dead_runtime_mailbox_claims, register_runtime_mailbox_listener,
    release_runtime_mailbox_message_claim, resolve_own_main_runtime_coordination_recipient,
    retire_runtime_mailbox_listener, take_runtime_mailbox_messages_for_delivery,
};
pub use requests::{
    ArchitectRequest, ArchitectRequestStatus, PostArchitectRequestInput, PostSupervisorRequestInput, SupervisorRequest,
    SupervisorRequestKind, SupervisorRequestStatus, SupervisorResponse, cancel_supervisor_request,
    claim_next_supervisor_request, claim_pending_architect_requests, complete_architect_request,
    complete_supervisor_request, has_pending_supervisor_approval_request, list_pending_architect_requests,
    post_architect_request, post_supervisor_request, read_supervisor_request, recover_supervisor_requests,
    renew_architect_request_claims, requeue_supervisor_request,
};
pub use sessions::{
    EmptyMainSessionCandidate, LastControlMessage, ResumeSessionMetadataOptions, SessionMetadata, SessionNameState,
    WritableSessionMetadata, archive_session, archive_sessions_older_than, clear_session_sandbox_profile,
    find_active_session_metadata_by_id, find_active_session_metadata_by_name, list_active_session_metadata,
    list_archived_session_metadata, list_descendant_session_paths, list_empty_main_session_candidates,
    list_resume_session_metadata, list_session_metadata, list_subagent_session_paths, normalize_session_name,
    read_last_message, read_or_migrate_prompt_history, read_prompt_history, read_session_goal, read_session_metadata,
    read_session_name_state, read_session_sandbox_profile, record_prompt_history_entry, relocate_and_archive_session,
    relocate_and_unarchive_session, relocate_session_control_data, remove_session_metadata, remove_sessions_metadata,
    unarchive_session, write_last_message, write_session_goal, write_session_metadata, write_session_model,
    write_session_name, write_session_sandbox_profile, write_session_thinking_level,
};

pub use crate::core::sqlite::ControlDbError;
pub use schema::{CONTROL_DB_SCHEMA_VERSION, LIFECYCLE_PROTOCOL_SCHEMA_VERSION, initialize_schema};

use crate::core::multi_agent_store::{AgentFileReference, AgentSnapshot, SteeringCheckpoint};

pub const RUNTIME_MAILBOX_ROUTING_MAX_ATTEMPTS: usize = 3;
const RUNTIME_MAILBOX_REGISTRATION_MAX_ATTEMPTS: usize = 3;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomingControlMessage {
    pub id: i64,
    pub content: JsString,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMailboxMessageKind {
    Message,
    Ask,
    Reply,
    Steer,
    ParentRequest,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMailboxMessageStatus {
    Pending,
    Claimed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMailboxAddress {
    pub agent_id: Option<JsString>,
    pub session_id: JsString,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMailboxMessage {
    pub id: i64,
    pub recipient: RuntimeMailboxAddress,
    pub sender: RuntimeMailboxAddress,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_host: Option<JsString>,
    pub kind: RuntimeMailboxMessageKind,
    pub body: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_refs: Option<JsValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_checkpoint: Option<SteeringCheckpoint>,
    pub store_ref: RuntimeMailboxStoreRef,
    pub status: RuntimeMailboxMessageStatus,
    pub created_at: JsString,
    pub updated_at: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed_at: Option<JsString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMailboxStoreRef {
    pub message_id: JsString,
    pub session_path: JsString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedChannelMessage {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_id: Option<i64>,
    pub body: String,
    pub created_at: String,
    pub id: i64,
    pub sender: RuntimeMailboxAddress,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PostSharedChannelMessageInput {
    pub sender: RuntimeMailboxAddress,
    pub body: JsString,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueRuntimeMailboxMessageInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient_host: Option<JsString>,
    pub recipient: RuntimeMailboxAddress,
    pub sender: RuntimeMailboxAddress,
    pub kind: RuntimeMailboxMessageKind,
    /// Canonical mailbox row receiving runtime routing and delivery state.
    pub store_ref: RuntimeMailboxStoreRef,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueStoredRuntimeMailboxMessageInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipient_host: Option<JsString>,
    pub recipient: RuntimeMailboxAddress,
    pub sender: RuntimeMailboxAddress,
    pub kind: RuntimeMailboxMessageKind,
    pub store_ref: RuntimeMailboxStoreRef,
    pub message: JsValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeMailboxListener {
    pub session_id: JsString,
    pub agent_id: Option<JsString>,
    pub pid: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_path: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Default)]
pub struct RuntimeMailboxRegistrationOptions {
    pub is_runtime_process_alive:
        Option<Arc<dyn Fn(u32) -> Result<bool, pi_js_compat::callback_error::CallbackError> + Send + Sync>>,
    pub reconcile_runtime_replacement: Option<bool>,
    pub runtime_instance_id: Option<String>,
}

pub(crate) struct RuntimeMailboxListenerRow {
    pub pid: u32,
    pub runtime_instance_id: Option<String>,
    pub session_path: Option<String>,
    pub session_path_asserted_at: Option<String>,
    pub updated_at: String,
}

pub(crate) struct RuntimeMailboxRow {
    pub id: i64,
    pub session_path: String,
    pub message_id: JsString,
    pub data: String,
    pub updated_at: String,
}

pub(crate) struct MultiAgentRuntimeOwnershipRow {
    pub session_path: String,
    pub agent_id: JsString,
    pub process_identity: Option<String>,
    pub owner_session_id: Option<JsString>,
    pub owner_agent_id: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiAgentRuntimeOwnership {
    pub session_path: JsString,
    pub agent_id: JsString,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_identity: Option<ProcessIdentity>,
    pub owner: MultiAgentRuntimeOwner,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiAgentRuntimeOwner {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<JsString>,
    pub agent_id: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiAgentRuntimeOwnerIdentity {
    pub session_id: JsString,
    pub agent_id: Option<JsString>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MultiAgentRuntimeOwnershipIdentity {
    pub session_path: JsString,
    pub agent_id: JsString,
    pub process_identity: ProcessIdentity,
    pub owner: MultiAgentRuntimeOwnerIdentity,
}

/// `getControlDbPath(directory = getUserStateRoot())`.
pub fn get_control_db_path(directory: impl Into<JsString>) -> JsString {
    crate::utils::paths::node_join(directory, "control.sqlite")
}

pub(crate) fn runtime_mailbox_agent_id_key(agent_id: Option<&JsString>) -> JsString {
    agent_id.cloned().unwrap_or_default()
}

pub(crate) fn optional_row<T>(result: rusqlite::Result<T>) -> Result<Option<T>, ControlDbError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub fn with_immediate_transaction<T>(
    db: &Connection,
    operation: impl FnOnce(&Connection) -> Result<T, ControlDbError>,
) -> Result<T, ControlDbError> {
    db.execute_batch("BEGIN IMMEDIATE")?;
    match operation(db) {
        Ok(value) => {
            db.execute_batch("COMMIT")?;
            Ok(value)
        }
        Err(error) => {
            let _ = db.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

struct RetainedControlDb {
    connection: ReentrantMutex<RefCell<Option<Connection>>>,
    active_calls: Mutex<usize>,
    retain_count: Mutex<usize>,
    retention_thread: OnceLock<std::thread::Thread>,
}

/// Keyed by the raw path string, as the source `Map` is.
fn retained_control_dbs() -> &'static Mutex<HashMap<JsString, Arc<RetainedControlDb>>> {
    static RETAINED: OnceLock<Mutex<HashMap<JsString, Arc<RetainedControlDb>>>> = OnceLock::new();
    RETAINED.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn with_control_db<T>(
    control_db_path: &JsString,
    callback: impl FnOnce(&Connection) -> Result<T, ControlDbError>,
) -> Result<T, ControlDbError> {
    let retained = retained_control_dbs()
        .lock()
        .expect("retained control db mutex")
        .get(control_db_path)
        .cloned();
    if let Some(retained) = retained {
        *retained.active_calls.lock().expect("retained call mutex") += 1;
        let result = {
            let guard = retained.connection.lock();
            let connection = guard.borrow();
            match connection.as_ref() {
                Some(connection) => callback(connection),
                None => Err(ControlDbError::new("SQLite database is closed")),
            }
        };
        let mut active_calls = retained.active_calls.lock().expect("retained call mutex");
        *active_calls -= 1;
        let close = *active_calls == 0 && *retained.retain_count.lock().expect("retained count mutex") == 0;
        drop(active_calls);
        if close {
            close_released_control_db(control_db_path, &retained);
        }
        return result;
    }

    let setup_lock = setup_lock(control_db_path);
    let db = {
        let _guard = setup_lock.lock().expect("control db setup mutex");
        let db = create_writable_control_db(control_db_path)?;
        configure_shared_sqlite_database(&db, DEFAULT_SHARED_SQLITE_BUSY_TIMEOUT_MS)?;
        initialize_schema(&db, None)?;
        db
    };
    let result = callback(&db);
    drop(db);
    result
}

fn setup_lock(control_db_path: &JsString) -> Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<JsString, Arc<Mutex<()>>>>> = OnceLock::new();
    let mut locks = LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("control db setup lock map");
    locks
        .entry(control_db_path.clone())
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone()
}

fn close_released_control_db(control_db_path: &JsString, retained: &Arc<RetainedControlDb>) {
    retained_control_dbs()
        .lock()
        .expect("retained control db mutex")
        .remove(control_db_path);
    retained.connection.lock().borrow_mut().take();
    if let Some(thread) = retained.retention_thread.get() {
        thread.unpark();
    }
}

fn create_writable_control_db(control_db_path: &JsString) -> Result<Connection, ControlDbError> {
    let directory = crate::utils::paths::node_dirname(control_db_path.clone());
    create_private_directory(&directory)?;
    create_sqlite_database(&encode_node_utf8(control_db_path))
}

fn create_private_directory(directory: &JsString) -> Result<(), ControlDbError> {
    use pi_js_compat::node_error::NativeNodeError;
    use pi_js_compat::node_fs::{node_fs_error_record, node_path_argument_error};
    use std::os::unix::fs::DirBuilderExt;

    if let Some(error) = node_path_argument_error(directory) {
        return Err(NativeNodeError::from(error).into());
    }
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true).mode(0o700);
    builder
        .create(encode_node_utf8(directory))
        .map_err(|error| NativeNodeError::from(node_fs_error_record(&error, "mkdir", directory)).into())
}

fn open_retained_control_db(control_db_path: &JsString) -> Result<Arc<RetainedControlDb>, ControlDbError> {
    let db = create_writable_control_db(control_db_path)?;
    let result = (|| {
        configure_shared_sqlite_database(&db, DEFAULT_SHARED_SQLITE_BUSY_TIMEOUT_MS)?;
        initialize_schema(&db, None)
    })();
    result?;
    let retained = Arc::new(RetainedControlDb {
        connection: ReentrantMutex::new(RefCell::new(Some(db))),
        active_calls: Mutex::new(0),
        retain_count: Mutex::new(0),
        retention_thread: OnceLock::new(),
    });
    start_retention_timer(&retained, control_db_path)?;
    retained_control_dbs()
        .lock()
        .expect("retained control db mutex")
        .insert(control_db_path.clone(), retained.clone());
    Ok(retained)
}

fn start_retention_timer(retained: &Arc<RetainedControlDb>, control_db_path: &JsString) -> Result<(), ControlDbError> {
    let weak = Arc::downgrade(retained);
    let path = control_db_path.clone();
    let worker = std::thread::Builder::new()
        .name("control-db-retention".into())
        .spawn(move || {
            loop {
                std::thread::park_timeout(std::time::Duration::from_secs(60));
                let Some(retained) = weak.upgrade() else { break };
                let guard = retained.connection.lock();
                let connection = guard.borrow();
                let Some(db) = connection.as_ref() else { break };
                if let Err(error) = retention::cleanup_expired_messages(db) {
                    eprintln!(
                        "Message retention cleanup failed for {}: {error}",
                        encode_node_utf8(&path)
                    );
                }
            }
        })
        .map_err(|error| ControlDbError::new(format!("Cannot start message retention timer: {error}")))?;
    retained.retention_thread.get_or_init(|| worker.thread().clone());
    Ok(())
}

/// `retainControlDbConnection`; dropping the returned guard releases the connection.
pub fn retain_control_db_connection(
    control_db_path: impl Into<JsString>,
) -> Result<RetainedControlDbGuard, ControlDbError> {
    let control_db_path = control_db_path.into();
    let retained = {
        let existing = retained_control_dbs()
            .lock()
            .expect("retained control db mutex")
            .get(&control_db_path)
            .cloned();
        match existing {
            Some(retained) => retained,
            None => open_retained_control_db(&control_db_path)?,
        }
    };
    *retained.retain_count.lock().expect("retained count mutex") += 1;
    Ok(RetainedControlDbGuard {
        control_db_path,
        retained,
        released: false,
    })
}

/// Guard for a retained control-db connection.
pub struct RetainedControlDbGuard {
    control_db_path: JsString,
    retained: Arc<RetainedControlDb>,
    released: bool,
}

impl RetainedControlDbGuard {
    pub fn release(mut self) {
        self.release_inner();
    }

    fn release_inner(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        let mut retain_count = self.retained.retain_count.lock().expect("retained count mutex");
        *retain_count -= 1;
        let close = *retain_count == 0 && *self.retained.active_calls.lock().expect("retained call mutex") == 0;
        drop(retain_count);
        if close {
            close_released_control_db(&self.control_db_path, &self.retained);
        }
    }
}

impl Drop for RetainedControlDbGuard {
    fn drop(&mut self) {
        self.release_inner();
    }
}

pub(crate) fn parse_json_object_lossless(value: impl Into<JsString>) -> Option<JsObject> {
    match js_json::parse_lossless(value).ok()? {
        JsValue::Object(object) => Some(object),
        _ => None,
    }
}

pub(crate) fn require_native_string_field(
    data: &JsObject,
    field: &str,
    context: &JsString,
) -> Result<JsString, ControlDbError> {
    match data.get(field) {
        Some(JsValue::String(value)) => Ok(value.clone()),
        _ => Err(ControlDbError::new(js_format!(
            "Invalid persisted {} at {}: expected a string",
            field,
            context
        ))),
    }
}

pub(crate) fn validate_mailbox_payload(data: &JsValue, context: &JsString) -> Result<(), ControlDbError> {
    let JsValue::Object(payload) = data else {
        return Err(ControlDbError::new(js_format!(
            "Invalid persisted mailbox payload at {}",
            context
        )));
    };
    if payload.is_empty() {
        return Err(ControlDbError::new(js_format!(
            "Invalid persisted mailbox payload at {}: expected fields",
            context
        )));
    }
    reject_legacy_artifact_fields(data, context)?;
    for field in [
        "id",
        "fromAgentId",
        "toAgentId",
        "kind",
        "status",
        "createdAt",
        "updatedAt",
    ] {
        if field == "createdAt" && payload.get(field) == Some(&JsValue::Null) {
            continue;
        }
        if payload.get(field).is_some() {
            require_native_string_field(payload, field, context)?;
        }
    }
    if payload.get("body").is_some() {
        require_native_string_field(payload, "body", context)?;
    }
    parse_file_refs(payload.get("fileRefs"), context)?;
    parse_steering_checkpoint(payload.get("targetCheckpoint"), context)?;
    Ok(())
}

pub(crate) fn validate_persisted_agent_payload(data: &JsObject, context: &JsString) -> Result<(), ControlDbError> {
    if data.is_empty() {
        return Err(ControlDbError::new(js_format!(
            "Invalid persisted agent payload at {}: expected fields",
            context
        )));
    }
    reject_legacy_artifact_fields(&JsValue::Object(data.clone()), context)?;
    if let Some(revision) = data.get("revision") {
        let valid = revision
            .as_f64()
            .is_some_and(|value| value >= 0.0 && value.fract() == 0.0);
        if !valid {
            return Err(ControlDbError::new(js_format!(
                "Invalid persisted revision at {}: expected a non-negative integer",
                context
            )));
        }
    }
    if let Some(policy) = data.get("suppressTerminalNotification")
        && policy.as_bool().is_none()
    {
        return Err(ControlDbError::new(js_format!(
            "Invalid persisted suppressTerminalNotification at {}: expected a boolean",
            context
        )));
    }
    if let Some(permission) = data.get("permission") {
        let JsValue::Object(permission) = permission else {
            return Err(ControlDbError::new(js_format!(
                "Invalid persisted permission at {}",
                context
            )));
        };
        require_native_string_field(permission, "policy", &js_format!("{}.permission", context))?;
        if !permission.get("narrowed").and_then(JsValue::as_bool).is_some() {
            return Err(ControlDbError::new(js_format!(
                "Invalid persisted narrowed at {}.permission: expected a boolean",
                context
            )));
        }
    }
    if let Some(result) = data.get("result") {
        let JsValue::Object(result) = result else {
            return Err(ControlDbError::new(js_format!(
                "Invalid persisted agent result at {}",
                context
            )));
        };
        parse_file_refs(result.get("fileRefs"), &js_format!("{}.result", context))?;
    }
    Ok(())
}

pub(crate) fn reject_legacy_artifact_fields(value: &JsValue, context: &JsString) -> Result<(), ControlDbError> {
    match value {
        JsValue::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                reject_legacy_artifact_fields(item, &js_format!("{}[{}]", context, index.to_string()))?;
            }
            Ok(())
        }
        JsValue::Object(object) => {
            for (key, nested) in object {
                if key == "artifactIds" || key == "artifactRefs" {
                    return Err(ControlDbError::new(js_format!(
                        "Legacy artifact fields are not supported at {}.{}",
                        context,
                        key
                    )));
                }
                reject_legacy_artifact_fields(nested, &js_format!("{}.{}", context, key))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

pub(crate) fn parse_file_refs(
    value: Option<&JsValue>,
    context: &JsString,
) -> Result<Option<Vec<AgentFileReference>>, ControlDbError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let JsValue::Array(items) = value else {
        return Err(ControlDbError::new(js_format!(
            "Invalid file references at {}: expected an array",
            context
        )));
    };
    let mut refs = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        let JsValue::Object(reference) = item else {
            return Err(ControlDbError::new(js_format!(
                "Invalid file reference at {}[{}]",
                context,
                index.to_string()
            )));
        };
        let Some(JsValue::String(path)) = reference.get("path") else {
            return Err(ControlDbError::new(js_format!(
                "Invalid file reference at {}[{}]: path must be absolute",
                context,
                index.to_string()
            )));
        };
        if !path.starts_with("/") {
            return Err(ControlDbError::new(js_format!(
                "Invalid file reference at {}[{}]: path must be absolute",
                context,
                index.to_string()
            )));
        }
        let label = match reference.get("label") {
            None => None,
            Some(JsValue::String(label)) => Some(label.clone()),
            Some(_) => {
                return Err(ControlDbError::new(js_format!(
                    "Invalid file reference at {}[{}]: label must be a string",
                    context,
                    index.to_string()
                )));
            }
        };
        refs.push(AgentFileReference {
            path: path.clone(),
            label,
        });
    }
    Ok(Some(refs))
}

pub(crate) fn parse_steering_checkpoint(
    value: Option<&JsValue>,
    context: &JsString,
) -> Result<Option<SteeringCheckpoint>, ControlDbError> {
    let Some(value) = value else {
        return Ok(None);
    };
    match value {
        JsValue::String(value) => match value.to_utf8().as_deref() {
            Ok("next_model_call") => Ok(Some(SteeringCheckpoint::NextModelCall)),
            Ok("after_tool_result") => Ok(Some(SteeringCheckpoint::AfterToolResult)),
            Ok("when_waiting") => Ok(Some(SteeringCheckpoint::WhenWaiting)),
            _ => Err(ControlDbError::new(js_format!(
                "Invalid steering checkpoint at {}",
                context
            ))),
        },
        _ => Err(ControlDbError::new(js_format!(
            "Invalid steering checkpoint at {}",
            context
        ))),
    }
}

pub(crate) fn to_runtime_mailbox_message_kind(value: impl Into<JsString>) -> RuntimeMailboxMessageKind {
    let value = value.into();
    [
        ("message", RuntimeMailboxMessageKind::Message),
        ("ask", RuntimeMailboxMessageKind::Ask),
        ("reply", RuntimeMailboxMessageKind::Reply),
        ("steer", RuntimeMailboxMessageKind::Steer),
        ("parent_request", RuntimeMailboxMessageKind::ParentRequest),
        ("system", RuntimeMailboxMessageKind::System),
    ]
    .into_iter()
    .find(|(name, _)| value == *name)
    .map_or(RuntimeMailboxMessageKind::Message, |(_, kind)| kind)
}

pub(crate) fn to_runtime_mailbox_message_status(value: impl Into<JsString>) -> RuntimeMailboxMessageStatus {
    match value.into().to_utf8().as_deref() {
        Ok("pending") => RuntimeMailboxMessageStatus::Pending,
        Ok("claimed") => RuntimeMailboxMessageStatus::Claimed,
        Ok("failed") => RuntimeMailboxMessageStatus::Failed,
        _ => RuntimeMailboxMessageStatus::Failed,
    }
}

/// `getRuntimeProcessInstanceId`: throws when the process identity cannot be read.
pub fn get_runtime_process_instance_id() -> Result<&'static str, ControlDbError> {
    static INSTANCE: OnceLock<Result<String, ControlDbError>> = OnceLock::new();
    INSTANCE
        .get_or_init(|| process_instance_id_from(read_process_identity(std::process::id())))
        .as_deref()
        .map_err(Clone::clone)
}

fn process_instance_id_from(
    identity: Result<ProcessIdentity, crate::core::runtime_process::ProcessIdentityError>,
) -> Result<String, ControlDbError> {
    let identity = identity.map_err(|error| ControlDbError::new(error.message))?;
    let value = serde_json::json!({
        "pid": identity.pid,
        "startTimeTicks": identity.start_time_ticks,
        "incarnation": pi_ai::utils::random_uuid::random_uuid()?,
    });
    Ok(js_json::stringify(&value))
}

/// `JSON.stringify` of a process identity. Lone surrogates in the incarnation are written as
/// `\uXXXX` escapes, so the JSON text itself is well-formed.
pub fn serialize_process_identity(identity: &ProcessIdentity) -> String {
    let mut object = JsObject::new();
    object.insert("pid".into(), JsValue::Number(f64::from(identity.pid)));
    object.insert(
        "startTimeTicks".into(),
        JsValue::Number(identity.start_time_ticks as f64),
    );
    if let Some(incarnation) = &identity.incarnation {
        object.insert("incarnation".into(), JsValue::String(incarnation.clone()));
    }
    js_json::stringify_lossless(&JsValue::Object(object))
}

pub(crate) fn parse_process_identity(value: impl Into<JsString>) -> Result<ProcessIdentity, ControlDbError> {
    let parsed = js_json::parse_lossless(value).map_err(|error| ControlDbError::new(error.to_string()))?;
    let JsValue::Object(parsed) = parsed else {
        return Err(ControlDbError::new("Persisted process identity is invalid"));
    };
    let safe_integer = |name: &str| match parsed.get(name) {
        Some(JsValue::Number(value)) if value.fract() == 0.0 && *value >= 0.0 && *value <= 9_007_199_254_740_991.0 => {
            Some(*value)
        }
        _ => None,
    };
    let (Some(pid), Some(start_time_ticks)) = (safe_integer("pid"), safe_integer("startTimeTicks")) else {
        return Err(ControlDbError::new("Persisted process identity is invalid"));
    };
    let incarnation = match parsed.get("incarnation") {
        None => None,
        Some(JsValue::String(value)) => Some(value.clone()),
        Some(_) => {
            return Err(ControlDbError::new("Persisted process incarnation is invalid"));
        }
    };
    Ok(ProcessIdentity {
        pid: pid as u32,
        start_time_ticks: start_time_ticks as u64,
        incarnation,
    })
}

pub(crate) fn trusted_runtime_mailbox_session_path(row: &RuntimeMailboxListenerRow) -> Option<String> {
    match (&row.session_path, &row.session_path_asserted_at) {
        (Some(path), Some(asserted_at)) if asserted_at == &row.updated_at => Some(path.clone()),
        _ => None,
    }
}

pub(crate) fn is_process_alive(pid: u32) -> bool {
    is_pi_runtime_process_alive(pid)
}

pub(crate) fn process_identity_is_alive(identity: &ProcessIdentity) -> bool {
    is_process_identity_alive(identity)
}

pub(crate) fn runtime_owner_matches(
    ownership: Option<&MultiAgentRuntimeOwnershipRow>,
    input: &MultiAgentRuntimeOwnershipIdentity,
) -> bool {
    let Some(ownership) = ownership else {
        return false;
    };
    input.session_path == ownership.session_path
        && ownership.agent_id == input.agent_id
        && ownership.process_identity.as_deref() == Some(serialize_process_identity(&input.process_identity).as_str())
        && ownership.owner_session_id.as_ref() == Some(&input.owner.session_id)
        && ownership.owner_agent_id == input.owner.agent_id
}

pub(crate) fn multi_agent_runtime_ownership_rows_equal(
    expected: Option<&MultiAgentRuntimeOwnershipRow>,
    current: Option<&MultiAgentRuntimeOwnershipRow>,
) -> bool {
    match (expected, current) {
        (None, None) => true,
        (Some(expected), Some(current)) => {
            expected.session_path == current.session_path
                && expected.agent_id == current.agent_id
                && expected.process_identity == current.process_identity
                && expected.owner_session_id == current.owner_session_id
                && expected.owner_agent_id == current.owner_agent_id
        }
        _ => false,
    }
}

pub(crate) fn runtime_mailbox_listener_rows_equal(
    expected: Option<&RuntimeMailboxListenerRow>,
    current: Option<&RuntimeMailboxListenerRow>,
) -> bool {
    match (expected, current) {
        (None, None) => true,
        (Some(expected), Some(current)) => {
            expected.pid == current.pid
                && expected.runtime_instance_id == current.runtime_instance_id
                && expected.session_path == current.session_path
                && expected.session_path_asserted_at == current.session_path_asserted_at
                && expected.updated_at == current.updated_at
        }
        _ => false,
    }
}

pub(crate) fn session_health_rows_equal(
    expected: Option<&SessionHealthRecord>,
    current: Option<&SessionHealthRecord>,
) -> bool {
    match (expected, current) {
        (None, None) => true,
        (Some(expected), Some(current)) => expected == current,
        _ => false,
    }
}

pub(crate) fn read_runtime_mailbox_listener_row(
    db: &Connection,
    recipient: &RuntimeMailboxAddress,
) -> Result<Option<RuntimeMailboxListenerRow>, ControlDbError> {
    optional_row(db.query_row(
        "
			SELECT pid, runtime_instance_id, session_path, session_path_asserted_at, updated_at
			FROM runtime_mailbox_listeners
			WHERE recipient_session_id = ? AND recipient_agent_id_key = ?
			",
        params![
            &recipient.session_id,
            runtime_mailbox_agent_id_key(recipient.agent_id.as_ref())
        ],
        |row| {
            Ok(RuntimeMailboxListenerRow {
                pid: row.get(0)?,
                runtime_instance_id: row.get(1)?,
                session_path: row.get(2)?,
                session_path_asserted_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        },
    ))
}

pub(crate) fn read_multi_agent_runtime_ownership_row(
    db: &Connection,
    session_path: impl Into<JsString>,
    agent_id: &JsString,
) -> Result<Option<MultiAgentRuntimeOwnershipRow>, ControlDbError> {
    let session_path: &JsString = &session_path.into();
    optional_row(db.query_row(
        "SELECT * FROM multi_agent_runtime_owners WHERE session_path = ? AND agent_id = ?",
        params![session_path, agent_id],
        |row| {
            Ok(MultiAgentRuntimeOwnershipRow {
                session_path: row.get("session_path")?,
                agent_id: row.get("agent_id")?,
                process_identity: row.get("process_identity")?,
                owner_session_id: row.get("owner_session_id")?,
                owner_agent_id: row.get("owner_agent_id")?,
            })
        },
    ))
}

pub(crate) fn multi_agent_runtime_ownership_from_row(
    row: &MultiAgentRuntimeOwnershipRow,
) -> Result<MultiAgentRuntimeOwnership, ControlDbError> {
    Ok(MultiAgentRuntimeOwnership {
        agent_id: row.agent_id.clone(),
        owner: MultiAgentRuntimeOwner {
            agent_id: row.owner_agent_id.clone(),
            session_id: row.owner_session_id.clone(),
        },
        process_identity: row
            .process_identity
            .as_deref()
            .map(parse_process_identity)
            .transpose()?,
        session_path: row.session_path.clone().into(),
    })
}

pub(crate) fn read_canonical_mailbox_row_by_store_ref(
    db: &Connection,
    store_ref: &RuntimeMailboxStoreRef,
) -> Result<Option<RuntimeMailboxRow>, ControlDbError> {
    optional_row(db.query_row(
        &format!(
            "SELECT rowid AS id, session_path, message_id, data, updated_at
			 FROM multi_agent_mailbox_messages WHERE session_path = ? AND message_id = ? AND {MAILBOX_CREATED_AT_MS_SQL} > ?"
        ),
        params![&store_ref.session_path, &store_ref.message_id, mailbox_cutoff_ms()],
        mailbox_row_from_row,
    ))
}

pub(crate) fn read_canonical_mailbox_row_by_id(
    db: &Connection,
    id: i64,
) -> Result<Option<RuntimeMailboxRow>, ControlDbError> {
    optional_row(db.query_row(
        &format!(
            "SELECT rowid AS id, session_path, message_id, data, updated_at
			 FROM multi_agent_mailbox_messages WHERE rowid = ? AND {MAILBOX_CREATED_AT_MS_SQL} > ?"
        ),
        params![id, mailbox_cutoff_ms()],
        mailbox_row_from_row,
    ))
}

fn mailbox_row_from_row(row: &Row<'_>) -> rusqlite::Result<RuntimeMailboxRow> {
    Ok(RuntimeMailboxRow {
        id: row.get(0)?,
        session_path: row.get(1)?,
        message_id: row.get(2)?,
        data: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

pub(crate) fn parse_canonical_mailbox_payload(row: &RuntimeMailboxRow) -> Result<JsObject, ControlDbError> {
    let context = js_format!("multi_agent_mailbox_messages:{}#{}", row.session_path, row.message_id);
    let message = parse_stored_json_object_lossless(&row.data, &context)?;
    validate_mailbox_payload(&JsValue::Object(message.clone()), &context)?;
    Ok(message)
}

pub(crate) fn parse_canonical_mailbox_payload_lossless(
    row: &RuntimeMailboxRow,
) -> Result<js_json::JsObject, ControlDbError> {
    let context = js_format!("multi_agent_mailbox_messages:{}#{}", row.session_path, row.message_id);
    parse_stored_json_object_lossless(&row.data, &context)
}

pub(crate) fn parse_stored_json_object_lossless(
    value: &str,
    context: &JsString,
) -> Result<js_json::JsObject, ControlDbError> {
    let parsed = js_json::parse_lossless(value)
        .map_err(|_| ControlDbError::new(js_format!("Invalid persisted JSON at {}", context)))?;
    let js_json::JsValue::Object(message) = parsed else {
        return Err(ControlDbError::new(js_format!(
            "Invalid persisted object at {}",
            context
        )));
    };
    Ok(message)
}

/// Mutate the original canonical UTF-16 payload, never a typed/public projection.
/// The public mailbox projection migration is separate from this persistence boundary.
pub(crate) fn serialize_canonical_mailbox_mutation(
    row: &RuntimeMailboxRow,
    mutate: impl FnOnce(&mut js_json::JsObject),
) -> Result<String, ControlDbError> {
    let mut message = parse_canonical_mailbox_payload_lossless(row)?;
    mutate(&mut message);
    Ok(js_json::stringify_lossless(&js_json::JsValue::Object(message)))
}

pub(crate) fn write_canonical_mailbox_payload(
    db: &Connection,
    id: i64,
    message: &js_json::JsObject,
    updated_at: &str,
) -> Result<(), ControlDbError> {
    let updated = db.execute(
        "UPDATE multi_agent_mailbox_messages SET data = ?, updated_at = ? WHERE rowid = ?",
        params![
            js_json::stringify_lossless(&js_json::JsValue::Object(message.clone())),
            updated_at,
            id
        ],
    )?;
    if updated != 1 {
        return Err(ControlDbError::new(format!("Canonical mailbox mutation lost row {id}")));
    }
    Ok(())
}

pub(crate) fn compare_and_write_serialized_canonical_mailbox_payload(
    db: &Connection,
    row: &RuntimeMailboxRow,
    serialized: &str,
    updated_at: &str,
) -> Result<bool, ControlDbError> {
    let updated = db.execute(
        &format!("UPDATE multi_agent_mailbox_messages SET data = ?, updated_at = ?
			 WHERE rowid = ? AND session_path = ? AND message_id = ? AND data = ? AND updated_at = ? AND {MAILBOX_CREATED_AT_MS_SQL} > ?"),
        params![
            serialized,
            updated_at,
            row.id,
            &row.session_path,
            &row.message_id,
            &row.data,
            &row.updated_at,
            mailbox_cutoff_ms()
        ],
    )?;
    Ok(updated == 1)
}

pub(crate) fn compare_and_delete_canonical_mailbox_payload(
    db: &Connection,
    row: &RuntimeMailboxRow,
) -> Result<bool, ControlDbError> {
    Ok(db.execute(&format!("DELETE FROM multi_agent_mailbox_messages WHERE rowid = ? AND session_path = ? AND message_id = ? AND data = ? AND updated_at = ? AND {MAILBOX_CREATED_AT_MS_SQL} > ?"),params![row.id,row.session_path,row.message_id,row.data,row.updated_at,mailbox_cutoff_ms()])?==1)
}

pub(crate) fn runtime_mailbox_message_from_canonical_row(
    row: &RuntimeMailboxRow,
    message: &JsObject,
) -> Result<RuntimeMailboxMessage, ControlDbError> {
    let context = js_format!("multi_agent_mailbox_messages:{}#{}", row.session_path, row.message_id);
    let text = |key: &str| require_native_string_field(message, key, &context);
    let optional_text = |key: &str| message.get(key).and_then(JsValue::as_string).cloned();
    Ok(RuntimeMailboxMessage {
        id: row.id,
        recipient: RuntimeMailboxAddress {
            agent_id: optional_text("recipientAgentId"),
            session_id: text("recipientSessionId")?,
        },
        sender: RuntimeMailboxAddress {
            agent_id: optional_text("senderAgentId"),
            session_id: text("senderSessionId")?,
        },
        sender_host: optional_text("senderHost"),
        kind: to_runtime_mailbox_message_kind(&require_native_string_field(message, "kind", &context)?),
        body: text("body")?,
        file_refs: parse_file_refs(message.get("fileRefs"), &context)?
            .map(|refs| js_json::to_value(&refs))
            .transpose()
            .map_err(|error| ControlDbError::new(error.to_string()))?,
        target_checkpoint: parse_steering_checkpoint(message.get("targetCheckpoint"), &context)?,
        store_ref: RuntimeMailboxStoreRef {
            message_id: row.message_id.clone().into(),
            session_path: row.session_path.clone().into(),
        },
        status: to_runtime_mailbox_message_status(&require_native_string_field(message, "status", &context)?),
        created_at: text("createdAt")?,
        updated_at: optional_text("updatedAt").unwrap_or_else(|| row.updated_at.clone().into()),
        claimed_at: optional_text("claimedAt"),
        error: optional_text("error"),
    })
}

pub(crate) fn parse_agent_snapshot(serialized: &str, context: &JsString) -> Result<AgentSnapshot, ControlDbError> {
    let validation = parse_stored_json_object_lossless(serialized, context)?;
    validate_persisted_agent_payload(&validation, context)?;
    let value = JsValue::Object(validation);
    js_json::from_value(&value).map_err(|error| {
        ControlDbError::new(js_format!(
            "Invalid persisted agent payload at {}: {}",
            context,
            error.to_string()
        ))
    })
}

pub(crate) fn serialize_agent(agent: &AgentSnapshot) -> Result<String, ControlDbError> {
    agent_value(agent).map(|value| js_json::stringify_lossless(&value))
}

pub(crate) fn agent_value(agent: &AgentSnapshot) -> Result<JsValue, ControlDbError> {
    js_json::to_value(agent).map_err(|error| ControlDbError::new(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::runtime_process::ProcessIdentityError;
    use serde_json::Value;

    fn identity_read_failure(message: &str) -> Result<ProcessIdentity, ProcessIdentityError> {
        Err(ProcessIdentityError {
            message: message.into(),
        })
    }

    #[test]
    fn process_instance_id_propagates_identity_read_failure() {
        assert_eq!(
            process_instance_id_from(identity_read_failure("Invalid process stat for process 1")),
            Err(ControlDbError::new("Invalid process stat for process 1"))
        );
    }

    #[test]
    fn process_instance_id_serializes_identity_in_typescript_key_order() {
        let instance = process_instance_id_from(Ok(ProcessIdentity {
            pid: 4242,
            start_time_ticks: 9001,
            incarnation: None,
        }))
        .expect("identity serializes");
        let parsed = js_json::parse(&instance).expect("instance id is JSON");
        let Value::Object(parsed) = parsed else {
            panic!("instance id must be an object");
        };
        assert_eq!(
            parsed.keys().map(String::as_str).collect::<Vec<_>>(),
            ["pid", "startTimeTicks", "incarnation"]
        );
        assert!(instance.starts_with(r#"{"pid":4242,"startTimeTicks":9001,"incarnation":"#));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn runtime_process_instance_id_uses_the_live_process_identity() {
        let identity = read_process_identity(std::process::id()).expect("live process identity");
        let instance = get_runtime_process_instance_id().expect("runtime process instance id");
        let parsed = js_json::parse(instance).expect("instance id is JSON");
        assert_eq!(parsed["pid"], serde_json::json!(identity.pid));
        assert_eq!(parsed["startTimeTicks"], serde_json::json!(identity.start_time_ticks));
        assert_eq!(get_runtime_process_instance_id().unwrap(), instance);
    }
}
