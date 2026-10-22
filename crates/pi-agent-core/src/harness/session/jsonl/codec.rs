//! Port of `packages/agent/src/harness/session/jsonl/codec.ts`.

use pi_ai::types::JsonObject;
use pi_js_compat::js_json::{JsObject, JsValue as Value};

use crate::harness::session::jsonl::types::{HeaderKind, JsonlSessionMetadata, JsonlV4Header};
use crate::harness::session::types::{
    Entry, EntryBody, EntryType, JsonlDecodeError, JsonlDecodeKind, OperationKind, OptionalValue, RecordType,
    SessionError, SessionErrorCode, SessionMutation,
};

fn syntax_error(message: impl Into<String>) -> JsonlDecodeError {
    JsonlDecodeError::new(JsonlDecodeKind::Syntax, message)
}

pub(super) fn schema_error(message: impl Into<String>) -> JsonlDecodeError {
    JsonlDecodeError::new(JsonlDecodeKind::Schema, message)
}

fn parse_object(line: &str) -> Result<JsObject, JsonlDecodeError> {
    let value: Value = serde_json::from_str(line).map_err(|_| syntax_error("is not valid JSON"))?;
    match value {
        Value::Object(object) => Ok(object),
        _ => Err(schema_error("is not a JSON object")),
    }
}

pub(super) fn require_string(value: Option<&Value>, field: &str) -> Result<String, JsonlDecodeError> {
    match value {
        Some(Value::String(text)) => text.to_utf8().map_err(|_| schema_error(format!("has invalid {field}"))),
        _ => Err(schema_error(format!("has invalid {field}"))),
    }
}

/// `Number.isSafeInteger(value)` as an `i64`: JSON integers, plus integral
/// floats (`1.0`), bounded by `Number.MAX_SAFE_INTEGER`.
fn js_safe_integer(value: Option<&Value>) -> Option<i64> {
    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
    let number = value?.as_f64()?;
    if number.fract() != 0.0 || number.abs() > MAX_SAFE_INTEGER {
        return None;
    }
    Some(number as i64)
}

fn require_sequence(value: Option<&Value>) -> Result<i64, JsonlDecodeError> {
    match js_safe_integer(value) {
        Some(sequence) if sequence > 0 => Ok(sequence),
        _ => Err(schema_error("has invalid seq")),
    }
}

fn require_timestamp(value: Option<&Value>) -> Result<i64, JsonlDecodeError> {
    match js_safe_integer(value) {
        Some(timestamp) if timestamp >= 0 => Ok(timestamp),
        _ => Err(schema_error("has invalid timestamp")),
    }
}

fn require_nullable_id(value: Option<&Value>, field: &str) -> Result<Option<String>, JsonlDecodeError> {
    match value {
        Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => text
            .to_utf8()
            .map(Some)
            .map_err(|_| schema_error(format!("has invalid {field}"))),
        _ => Err(schema_error(format!("has invalid {field}"))),
    }
}

fn decode_header(line: &str) -> Result<JsonlV4Header, JsonlDecodeError> {
    let value = parse_object(line)?;
    if !value
        .get("kind")
        .and_then(Value::as_string)
        .is_some_and(|kind| kind == "header")
    {
        return Err(schema_error("is not a header"));
    }
    if value.get("version").and_then(Value::as_f64) != Some(4.0) {
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

fn decode_optional_string(value: &JsObject, key: &str, message: &str) -> Result<Option<String>, JsonlDecodeError> {
    match value.get(key) {
        None => Ok(None),
        Some(Value::String(text)) => text.to_utf8().map(Some).map_err(|_| schema_error(message)),
        Some(_) => Err(schema_error(message)),
    }
}

fn decode_metadata(value: &JsObject) -> Result<Option<JsonObject>, JsonlDecodeError> {
    match value.get("metadata") {
        None => Ok(None),
        Some(metadata @ Value::Object(_)) => metadata
            .to_serde()
            .map_err(|_| schema_error("has invalid metadata"))?
            .as_object()
            .cloned()
            .map(Some)
            .ok_or_else(|| schema_error("has invalid metadata")),
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

pub(super) fn known_entry_type(value: &str) -> Option<EntryType> {
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

fn parse_entry_mutation(value: JsObject) -> Result<SessionMutation, JsonlDecodeError> {
    let lane = match value.get("lane") {
        None => None,
        Some(Value::String(lane)) => Some(lane.to_utf8().map_err(|_| schema_error("has invalid lane"))?),
        Some(_) => return Err(schema_error("has invalid lane")),
    };
    let id = require_string(value.get("id"), "id")?;
    let entry_type = require_string(value.get("type"), "entry type")?;
    let entry_type =
        known_entry_type(&entry_type).ok_or_else(|| schema_error(format!("has unknown entry type {entry_type}")))?;
    let parent_id = require_nullable_id(value.get("parentId"), "parentId")?;
    let timestamp = require_timestamp(value.get("timestamp"))?;
    if value
        .get("type")
        .and_then(Value::as_string)
        .is_some_and(|kind| kind == "custom")
    {
        require_string(value.get("customType"), "customType")?;
    }
    let entry = Entry {
        body: decode_entry_body(&value, entry_type, id)?,
        parent_id,
        seq: require_sequence(value.get("seq"))?,
        timestamp,
    };
    Ok(SessionMutation::Entry { lane, entry })
}

pub(super) fn decode_field<T: serde::de::DeserializeOwned>(value: &JsObject, key: &str) -> Result<T, JsonlDecodeError> {
    pi_js_compat::js_json::from_value(value.get(key).unwrap_or(&Value::Null))
        .map_err(|error| schema_error(format!("has invalid {key}: {error}")))
}

pub(super) fn decode_optional_payload(value: &JsObject, key: &str) -> OptionalValue {
    match value.get(key) {
        None => OptionalValue::Missing,
        Some(payload) => OptionalValue::Present(payload.clone()),
    }
}

pub(super) fn decode_entry_body(value: &JsObject, kind: EntryType, id: String) -> Result<EntryBody, JsonlDecodeError> {
    Ok(match kind {
        EntryType::Message => EntryBody::Message {
            id,
            message: decode_field(value, "message")?,
            terminate: decode_field(value, "terminate")?,
        },
        EntryType::ModelChange => EntryBody::ModelChange {
            id,
            provider: decode_field(value, "provider")?,
            model_id: decode_field(value, "modelId")?,
        },
        EntryType::ThinkingLevelChange => EntryBody::ThinkingLevelChange {
            id,
            thinking_level: decode_field(value, "thinkingLevel")?,
        },
        EntryType::ActiveToolsChange => EntryBody::ActiveToolsChange {
            id,
            active_tool_names: decode_field(value, "activeToolNames")?,
        },
        EntryType::Compaction => decode_compaction_entry(value, id)?,
        EntryType::BranchSummary => decode_branch_summary_entry(value, id)?,
        EntryType::Custom => EntryBody::Custom {
            id,
            custom_type: decode_field(value, "customType")?,
            data: decode_optional_payload(value, "data"),
        },
    })
}

fn decode_compaction_entry(value: &JsObject, id: String) -> Result<EntryBody, JsonlDecodeError> {
    Ok(EntryBody::Compaction {
        id,
        summary: decode_field(value, "summary")?,
        tokens_before: decode_field(value, "tokensBefore")?,
        retained_tail: decode_field(value, "retainedTail")?,
        duration_ms: decode_field(value, "durationMs")?,
        details: decode_optional_payload(value, "details"),
        usage: decode_field(value, "usage")?,
    })
}

fn decode_branch_summary_entry(value: &JsObject, id: String) -> Result<EntryBody, JsonlDecodeError> {
    Ok(EntryBody::BranchSummary {
        id,
        from_id: decode_field(value, "fromId")?,
        summary: decode_field(value, "summary")?,
        details: decode_optional_payload(value, "details"),
        usage: decode_field(value, "usage")?,
    })
}

fn parse_record_mutation(value: JsObject) -> Result<SessionMutation, JsonlDecodeError> {
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
    let record = crate::harness::session::types::LaneRecord {
        body: super::codec_records::decode_record_body(&value, record_type)?,
        seq: require_sequence(value.get("seq"))?,
        timestamp: require_timestamp(value.get("timestamp"))?,
    };
    Ok(SessionMutation::Record { record })
}

fn parse_lane_mutation(value: &JsObject, seq: i64) -> Result<SessionMutation, JsonlDecodeError> {
    Ok(SessionMutation::Lane {
        seq,
        lane: require_string(value.get("lane"), "lane")?,
        leaf_id: require_nullable_id(value.get("leafId"), "leafId")?,
    })
}

fn parse_fact_mutation(value: &JsObject, seq: i64) -> Result<SessionMutation, JsonlDecodeError> {
    match value
        .get("fact")
        .and_then(Value::as_string)
        .and_then(|kind| kind.to_utf8().ok())
        .as_deref()
    {
        Some("name") => parse_name_fact(value, seq),
        Some("label") => parse_label_fact(value, seq),
        _ => Err(schema_error("has unknown fact type")),
    }
}

fn parse_name_fact(value: &JsObject, seq: i64) -> Result<SessionMutation, JsonlDecodeError> {
    let name = match value.get("name") {
        None => None,
        Some(Value::String(name)) => Some(name.to_utf8().map_err(|_| schema_error("has invalid name"))?),
        Some(_) => return Err(schema_error("has invalid name")),
    };
    Ok(SessionMutation::Fact {
        seq,
        fact: crate::harness::session::types::FactItem::Name { name },
    })
}

fn parse_label_fact(value: &JsObject, seq: i64) -> Result<SessionMutation, JsonlDecodeError> {
    let label = match value.get("label") {
        None => None,
        Some(Value::String(label)) => Some(label.to_utf8().map_err(|_| schema_error("has invalid label"))?),
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
    match value
        .get("kind")
        .and_then(Value::as_string)
        .and_then(|kind| kind.to_utf8().ok())
        .as_deref()
    {
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
