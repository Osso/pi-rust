//! Frozen runtime-mailbox-transcript.ts: acceptance spans every complete transcript record.
use super::session_control_db::{ControlDbError, RuntimeMailboxStoreRef};
use serde_json::Value;

pub const RUNTIME_MAILBOX_ACCEPTANCE_CUSTOM_TYPE: &str = "runtime_mailbox_acceptance";

pub fn mailbox_store_refs_equal(left: &RuntimeMailboxStoreRef, right: &RuntimeMailboxStoreRef) -> bool {
    left == right
}

/// Return raw references: frozen TS does not validate elements read from persisted JSON.
pub fn read_accepted_runtime_mailbox_store_refs(session_path: Option<&str>) -> Result<Vec<Value>, ControlDbError> {
    let Some(path) = session_path.filter(|s| !s.is_empty()) else {
        return Ok(Vec::new());
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(ControlDbError::new(error.to_string())),
    };
    let transcript = String::from_utf8_lossy(&bytes);
    let mut refs = Vec::new();
    // Ignore the final split item even when it is valid JSON: no LF means no durable record.
    let mut lines = transcript.split('\n').collect::<Vec<_>>();
    lines.pop();
    for line in lines {
        if pi_js_compat::string::js_trim(line).is_empty() {
            continue;
        }
        let entry = pi_js_compat::js_json::parse(line).map_err(|error| ControlDbError::new(error.to_string()))?;
        if entry.is_null() {
            return Err(ControlDbError::new("Cannot read properties of null (reading 'type')"));
        }
        let accepted = match entry.get("type").and_then(Value::as_str) {
            Some("message") => entry.get("mailboxStoreRefs"),
            Some("custom")
                if entry.get("customType").and_then(Value::as_str) == Some(RUNTIME_MAILBOX_ACCEPTANCE_CUSTOM_TYPE) =>
            {
                entry.get("data").and_then(|data| data.get("mailboxStoreRefs"))
            }
            _ => None,
        };
        match accepted {
            Some(Value::Array(values)) => refs.extend(values.iter().cloned()),
            Some(Value::String(value)) => {
                refs.extend(value.chars().map(|character| Value::String(character.to_string())))
            }
            None | Some(Value::Null) => {}
            _ => return Err(ControlDbError::new("mailboxStoreRefs is not iterable")),
        }
    }
    Ok(refs)
}
