//! Compile-time lock for the control-db surface consumed by the session-manager slice.

use std::path::Path;

use pi_coding_agent::core::permissions::presets::SandboxProfileName;
use pi_coding_agent::core::session_control_db::{
    ControlDbError, EnqueueRuntimeMailboxMessageInput, RuntimeMailboxAddress, RuntimeMailboxListener, SessionMetadata,
    WritableSessionMetadata, clear_session_sandbox_profile, enqueue_runtime_mailbox_message, get_control_db_path,
    list_active_session_metadata, list_archived_session_metadata, list_runtime_mailbox_listeners, list_session_health,
    list_session_metadata, read_session_goal, read_session_metadata, read_session_sandbox_profile,
    relocate_session_control_data, retire_runtime_mailbox_listener, upsert_multi_agent_mailbox_message,
    write_session_goal, write_session_health, write_session_metadata, write_session_model,
    write_session_sandbox_profile, write_session_thinking_level,
};
use pi_coding_agent::core::session_health::SessionHealthRecord;
use serde_json::Value;

#[test]
fn session_manager_contract_is_public() {
    let _: fn(&Path) -> String = get_control_db_path;
    let _: fn(&str, &str) -> Result<Option<SessionMetadata>, ControlDbError> = read_session_metadata;
    let _: fn(&str, &WritableSessionMetadata) -> Result<(), ControlDbError> = write_session_metadata;
    let _: fn(&str) -> Result<Vec<SessionMetadata>, ControlDbError> = list_session_metadata;
    let _: fn(&str) -> Result<Vec<SessionMetadata>, ControlDbError> = list_active_session_metadata;
    let _: fn(&str) -> Result<Vec<SessionMetadata>, ControlDbError> = list_archived_session_metadata;
    let _: fn(&str, &str) -> Result<Option<String>, ControlDbError> = read_session_goal;
    let _: fn(&str, &str, Option<&str>) -> Result<(), ControlDbError> = write_session_goal;
    let _: fn(&str, &str, &str, &str) -> Result<(), ControlDbError> = write_session_model;
    let _: fn(&str, &str, &str) -> Result<(), ControlDbError> = write_session_thinking_level;
    let _: fn(&str, &str, &str) -> Result<Option<SandboxProfileName>, ControlDbError> = read_session_sandbox_profile;
    let _: fn(&str, &str, &str, SandboxProfileName) -> Result<(), ControlDbError> = write_session_sandbox_profile;
    let _: fn(&str, &str, &str) -> Result<(), ControlDbError> = clear_session_sandbox_profile;
    let _: fn(&str, &str, &str) -> Result<(), ControlDbError> = relocate_session_control_data;
    let _: fn(&str) -> Result<Vec<SessionHealthRecord>, ControlDbError> = list_session_health;
    let _: fn(&str, &SessionHealthRecord) -> Result<(), ControlDbError> = write_session_health;
    let _: fn(&str) -> Result<Vec<RuntimeMailboxListener>, ControlDbError> = list_runtime_mailbox_listeners;
    let _: fn(&str, &RuntimeMailboxAddress, u32) -> Result<bool, ControlDbError> = retire_runtime_mailbox_listener;
    let _: fn(&str, &EnqueueRuntimeMailboxMessageInput) -> Result<i64, ControlDbError> =
        enqueue_runtime_mailbox_message;
    let _: fn(&str, &str, &str, &Value) -> Result<(), ControlDbError> = upsert_multi_agent_mailbox_message;
}
