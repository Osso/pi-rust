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
use pi_js_compat::js_json::{JsString, JsValue as Value};

#[test]
fn session_manager_contract_is_public() {
    let _: fn(&Path) -> String = get_control_db_path;
    let _: fn(&JsString, &JsString) -> Result<Option<SessionMetadata>, ControlDbError> =
        |db, path| read_session_metadata(db, path);
    let _: fn(&JsString, &WritableSessionMetadata) -> Result<(), ControlDbError> =
        |db, metadata| write_session_metadata(db, metadata);
    let _: fn(&JsString) -> Result<Vec<SessionMetadata>, ControlDbError> = |db| list_session_metadata(db);
    let _: fn(&JsString) -> Result<Vec<SessionMetadata>, ControlDbError> = |db| list_active_session_metadata(db);
    let _: fn(&JsString) -> Result<Vec<SessionMetadata>, ControlDbError> = |db| list_archived_session_metadata(db);
    let _: fn(&JsString, &JsString) -> Result<Option<JsString>, ControlDbError> =
        |db, path| read_session_goal(db, path);
    let _: fn(&JsString, &JsString, Option<&JsString>) -> Result<(), ControlDbError> =
        |db, path, goal| write_session_goal(db, path, goal);
    let _: fn(&JsString, &JsString, &str, &str) -> Result<(), ControlDbError> =
        |db, path, provider, model| write_session_model(db, path, provider, model);
    let _: fn(&JsString, &JsString, &str) -> Result<(), ControlDbError> =
        |db, path, level| write_session_thinking_level(db, path, level);
    let _: fn(&JsString, &JsString, &JsString) -> Result<Option<SandboxProfileName>, ControlDbError> =
        |db, path, cwd| read_session_sandbox_profile(db, path, cwd);
    let _: fn(&JsString, &JsString, &JsString, SandboxProfileName) -> Result<(), ControlDbError> =
        |db, path, cwd, profile| write_session_sandbox_profile(db, path, cwd, profile);
    let _: fn(&JsString, &JsString, &JsString) -> Result<(), ControlDbError> =
        |db, path, cwd| clear_session_sandbox_profile(db, path, cwd);
    let _: fn(&JsString, &JsString, &JsString) -> Result<(), ControlDbError> =
        |db, source, target| relocate_session_control_data(db, source, target);
    let _: fn(&JsString) -> Result<Vec<SessionHealthRecord>, ControlDbError> = |db| list_session_health(db);
    let _: fn(&JsString, &SessionHealthRecord) -> Result<(), ControlDbError> =
        |db, health| write_session_health(db, health);
    let _: fn(&JsString) -> Result<Vec<RuntimeMailboxListener>, ControlDbError> =
        |db| list_runtime_mailbox_listeners(db);
    let _: fn(&JsString, &RuntimeMailboxAddress, u32) -> Result<bool, ControlDbError> =
        |db, recipient, pid| retire_runtime_mailbox_listener(db, recipient, pid);
    let _: fn(&JsString, &EnqueueRuntimeMailboxMessageInput) -> Result<i64, ControlDbError> =
        |db, input| enqueue_runtime_mailbox_message(db, input);
    let _: fn(&JsString, &JsString, &JsString, &Value) -> Result<(), ControlDbError> =
        |db, path, id, message| upsert_multi_agent_mailbox_message(db, path, id, message);
}
