//! Port of `packages/agent/src/harness/session/jsonl/codec.ts`.

use pi_ai::types::JsonObject;
use serde_json::{Map, Value};

use crate::harness::session::jsonl::types::{HeaderKind, JsonlSessionMetadata, JsonlV4Header};
use crate::harness::session::types::{
    EntryType, JsonlDecodeError, JsonlDecodeKind, OperationKind, RecordType, SessionError, SessionErrorCode,
    SessionMutation,
};

fn syntax_error(message: impl Into<String>) -> JsonlDecodeError {
    JsonlDecodeError::new(JsonlDecodeKind::Syntax, message)
}

fn schema_error(message: impl Into<String>) -> JsonlDecodeError {
    JsonlDecodeError::new(JsonlDecodeKind::Schema, message)
}

fn parse_object(line: &str) -> Result<Map<String, Value>, JsonlDecodeError> {
    let value: Value = serde_json::from_str(line).map_err(|_| syntax_error("is not valid JSON"))?;
    match value {
        Value::Object(object) => Ok(object),
        _ => Err(schema_error("is not a JSON object")),
    }
}

fn require_string(value: Option<&Value>, field: &str) -> Result<String, JsonlDecodeError> {
    match value {
        Some(Value::String(text)) => Ok(text.clone()),
        _ => Err(schema_error(format!("has invalid {field}"))),
    }
}

fn require_sequence(value: Option<&Value>) -> Result<i64, JsonlDecodeError> {
    match value.and_then(Value::as_i64) {
        Some(sequence) if sequence > 0 => Ok(sequence),
        _ => Err(schema_error("has invalid seq")),
    }
}

fn require_timestamp(value: Option<&Value>) -> Result<i64, JsonlDecodeError> {
    match value.and_then(Value::as_i64) {
        Some(timestamp) if timestamp >= 0 => Ok(timestamp),
        _ => Err(schema_error("has invalid timestamp")),
    }
}

fn require_nullable_id(value: Option<&Value>, field: &str) -> Result<Option<String>, JsonlDecodeError> {
    match value {
        Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        _ => Err(schema_error(format!("has invalid {field}"))),
    }
}

fn decode_header(line: &str) -> Result<JsonlV4Header, JsonlDecodeError> {
    let value = parse_object(line)?;
    if value.get("kind").and_then(Value::as_str) != Some("header") {
        return Err(schema_error("is not a header"));
    }
    if value.get("version").and_then(Value::as_u64) != Some(4) {
        return Err(schema_error("has unsupported session version"));
    }
    let parent_session_id = decode_optional_string(&value, "parentSessionId", "has invalid parentSessionId")?;
    let legacy_parent_session_path =
        decode_optional_string(&value, "legacyParentSessionPath", "has invalid legacyParentSessionPath")?;
    if parent_session_id.is_some() && legacy_parent_session_path.is_some() {
        return Err(schema_error("has both parentSessionId and legacyParentSessionPath"));
    }
    Ok(JsonlV4Header {
        kind: HeaderKind::Header,
        version: 4,
        id: require_string(value.get("id"), "id")?,
        created_at: require_timestamp(value.get("createdAt"))?,
        cwd: require_string(value.get("cwd"), "cwd")?,
        parent_session_id,
        legacy_parent_session_path,
        metadata: decode_metadata(&value)?,
    })
}

fn decode_optional_string(
    value: &Map<String, Value>,
    key: &str,
    message: &str,
) -> Result<Option<String>, JsonlDecodeError> {
    match value.get(key) {
        None => Ok(None),
        Some(Value::String(text)) => Ok(Some(text.clone())),
        Some(_) => Err(schema_error(message)),
    }
}

fn decode_metadata(value: &Map<String, Value>) -> Result<Option<JsonObject>, JsonlDecodeError> {
    match value.get("metadata") {
        None => Ok(None),
        Some(Value::Object(object)) => Ok(Some(object.clone())),
        Some(_) => Err(schema_error("has invalid metadata")),
    }
}

/// Parse a session header line.
pub fn parse_header(line: &str) -> Result<JsonlV4Header, JsonlDecodeError> {
    decode_header(line)
}

/// Encode a session header line with its trailing newline.
pub fn encode_header(header: &JsonlV4Header) -> String {
    let mut line = serde_json::to_string(header).expect("session headers are always serializable");
    line.push('\n');
    line
}

/// Project a header and filesystem fields into session metadata.
pub fn metadata_from_header(header: &JsonlV4Header, path: String, modified_at: f64) -> JsonlSessionMetadata {
    JsonlSessionMetadata {
        id: header.id.clone(),
        created_at: header.created_at,
        cwd: header.cwd.clone(),
        path,
        modified_at,
        source_format: 4,
        parent_session_id: header.parent_session_id.clone(),
        legacy_parent_session_path: header.legacy_parent_session_path.clone(),
        metadata: header.metadata.clone(),
    }
}

fn known_entry_type(value: &str) -> Option<EntryType> {
    match value {
        "message" => Some(EntryType::Message),
        "model_change" => Some(EntryType::ModelChange),
        "thinking_level_change" => Some(EntryType::ThinkingLevelChange),
        "active_tools_change" => Some(EntryType::ActiveToolsChange),
        "compaction" => Some(EntryType::Compaction),
        "branch_summary" => Some(EntryType::BranchSummary),
        "custom" => Some(EntryType::Custom),
        _ => None,
    }
}

fn known_record_type(value: &str) -> Option<RecordType> {
    match value {
        "operation_started" => Some(RecordType::OperationStarted),
        "abort_requested" => Some(RecordType::AbortRequested),
        "operation_finished" => Some(RecordType::OperationFinished),
        "step_attempt" => Some(RecordType::StepAttempt),
        "tool_started" => Some(RecordType::ToolStarted),
        "queue_enqueued" => Some(RecordType::QueueEnqueued),
        "queue_cancelled" => Some(RecordType::QueueCancelled),
        "write_deferred" => Some(RecordType::WriteDeferred),
        "usage" => Some(RecordType::Usage),
        _ => None,
    }
}

fn parse_entry_mutation(value: Map<String, Value>) -> Result<SessionMutation, JsonlDecodeError> {
    let lane = match value.get("lane") {
        None => None,
        Some(Value::String(lane)) => Some(lane.clone()),
        Some(_) => return Err(schema_error("has invalid lane")),
    };
    let _id = require_string(value.get("id"), "id")?;
    let entry_type = require_string(value.get("type"), "entry type")?;
    let entry_type =
        known_entry_type(&entry_type).ok_or_else(|| schema_error(format!("has unknown entry type {entry_type}")))?;
    let _ = entry_type;
    let _ = require_nullable_id(value.get("parentId"), "parentId")?;
    let _ = require_timestamp(value.get("timestamp"))?;
    if value.get("type").and_then(Value::as_str) == Some("custom") {
        require_string(value.get("customType"), "customType")?;
    }
    let mut entry_fields = value;
    entry_fields.remove("kind");
    entry_fields.remove("lane");
    let entry = serde_json::from_value::<crate::harness::session::types::Entry>(Value::Object(entry_fields))
        .map_err(|error| schema_error(error.to_string()))?;
    Ok(SessionMutation::Entry { lane, entry })
}

fn parse_record_mutation(value: Map<String, Value>) -> Result<SessionMutation, JsonlDecodeError> {
    let _id = require_string(value.get("id"), "id")?;
    let _lane = require_string(value.get("lane"), "lane")?;
    let record_type = require_string(value.get("type"), "record type")?;
    let record_type = known_record_type(&record_type)
        .ok_or_else(|| schema_error(format!("has unknown record type {record_type}")))?;
    let _ = require_timestamp(value.get("timestamp"))?;
    if record_type == RecordType::OperationStarted {
        let intent = match value.get("intent") {
            Some(Value::Object(intent)) => intent,
            _ => return Err(schema_error("has invalid intent")),
        };
        let operation_kind = require_string(intent.get("kind"), "operation kind")?;
        if !matches!(operation_kind.as_str(), "run" | "compaction" | "navigation") {
            return Err(schema_error(format!("has unknown operation kind {operation_kind}")));
        }
    }
    if record_type == RecordType::OperationFinished {
        require_string(value.get("runId"), "runId")?;
    }
    let mut record_fields = value;
    record_fields.remove("kind");
    let record = serde_json::from_value::<crate::harness::session::types::LaneRecord>(Value::Object(record_fields))
        .map_err(|error| schema_error(error.to_string()))?;
    Ok(SessionMutation::Record { record })
}

fn parse_lane_mutation(value: &Map<String, Value>, seq: i64) -> Result<SessionMutation, JsonlDecodeError> {
    Ok(SessionMutation::Lane {
        seq,
        lane: require_string(value.get("lane"), "lane")?,
        leaf_id: require_nullable_id(value.get("leafId"), "leafId")?,
    })
}

fn parse_fact_mutation(value: &Map<String, Value>, seq: i64) -> Result<SessionMutation, JsonlDecodeError> {
    match value.get("fact").and_then(Value::as_str) {
        Some("name") => parse_name_fact(value, seq),
        Some("label") => parse_label_fact(value, seq),
        _ => Err(schema_error("has unknown fact type")),
    }
}

fn parse_name_fact(value: &Map<String, Value>, seq: i64) -> Result<SessionMutation, JsonlDecodeError> {
    let name = match value.get("name") {
        None => None,
        Some(Value::String(name)) => Some(name.clone()),
        Some(_) => return Err(schema_error("has invalid name")),
    };
    Ok(SessionMutation::Fact {
        seq,
        fact: crate::harness::session::types::FactItem::Name { name },
    })
}

fn parse_label_fact(value: &Map<String, Value>, seq: i64) -> Result<SessionMutation, JsonlDecodeError> {
    let label = match value.get("label") {
        None => None,
        Some(Value::String(label)) => Some(label.clone()),
        Some(_) => return Err(schema_error("has invalid label")),
    };
    Ok(SessionMutation::Fact {
        seq,
        fact: crate::harness::session::types::FactItem::Label {
            target_id: require_string(value.get("targetId"), "targetId")?,
            label,
        },
    })
}

fn decode_mutation(line: &str) -> Result<SessionMutation, JsonlDecodeError> {
    let value = parse_object(line)?;
    let seq = require_sequence(value.get("seq"))?;
    match value.get("kind").and_then(Value::as_str) {
        Some("entry") => parse_entry_mutation(value),
        Some("record") => parse_record_mutation(value),
        Some("lane") => parse_lane_mutation(&value, seq),
        Some("fact") => parse_fact_mutation(&value, seq),
        _ => Err(schema_error("has unknown mutation kind")),
    }
}

/// Parse one mutation line.
pub fn parse_mutation(line: &str) -> Result<SessionMutation, JsonlDecodeError> {
    decode_mutation(line)
}

/// Encode one mutation line with its trailing newline.
pub fn encode_mutation(mutation: &SessionMutation) -> Result<String, SessionError> {
    let mut line = serde_json::to_string(mutation)
        .map_err(|error| SessionError::new(SessionErrorCode::InvalidPayload, format!("Durable payload {error}")))?;
    line.push('\n');
    Ok(line)
}

/// Validate the operation kind of a started record.
pub fn is_known_operation_kind(kind: OperationKind) -> bool {
    matches!(
        kind,
        OperationKind::Run | OperationKind::Compaction | OperationKind::Navigation
    )
}

/// JSON object type used for opaque metadata.
pub type OpaqueMetadata = JsonObject;
