//! Native field decoding avoids serde's scalar buffering for tagged records.

use pi_js_compat::js_json::{JsObject, JsValue};

use super::codec::{
    decode_entry_body, decode_field, decode_optional_payload, known_entry_type, require_string, schema_error,
};
use crate::harness::session::types::{EntryBody, JsonlDecodeError, OperationIntent, RecordBody, RecordType};

pub(super) fn decode_record_body(value: &JsObject, kind: RecordType) -> Result<RecordBody, JsonlDecodeError> {
    match kind {
        RecordType::OperationStarted => decode_operation_started(value),
        RecordType::AbortRequested => Ok(RecordBody::AbortRequested {
            id: decode_field(value, "id")?,
            lane: decode_field(value, "lane")?,
            run_id: decode_field(value, "runId")?,
        }),
        RecordType::OperationFinished => Ok(RecordBody::OperationFinished {
            id: decode_field(value, "id")?,
            lane: decode_field(value, "lane")?,
            run_id: decode_field(value, "runId")?,
            outcome: decode_field(value, "outcome")?,
            error: decode_field(value, "error")?,
        }),
        RecordType::StepAttempt => decode_step_attempt(value),
        RecordType::ToolStarted => decode_tool_started(value),
        RecordType::QueueEnqueued => decode_queue_enqueued(value),
        RecordType::QueueCancelled => Ok(RecordBody::QueueCancelled {
            id: decode_field(value, "id")?,
            lane: decode_field(value, "lane")?,
            run_id: decode_field(value, "runId")?,
            entry_id: decode_field(value, "entryId")?,
        }),
        RecordType::WriteDeferred => Ok(RecordBody::WriteDeferred {
            id: decode_field(value, "id")?,
            lane: decode_field(value, "lane")?,
            run_id: decode_field(value, "runId")?,
            target: decode_entry_field(value, "target")?,
        }),
        RecordType::Usage => decode_usage(value),
    }
}

fn decode_step_attempt(value: &JsObject) -> Result<RecordBody, JsonlDecodeError> {
    Ok(RecordBody::StepAttempt {
        id: decode_field(value, "id")?,
        lane: decode_field(value, "lane")?,
        run_id: decode_field(value, "runId")?,
        step: decode_field(value, "step")?,
        attempt: decode_field(value, "attempt")?,
        result_entry_id: decode_field(value, "resultEntryId")?,
        compaction_reason: decode_field(value, "compactionReason")?,
    })
}

fn decode_tool_started(value: &JsObject) -> Result<RecordBody, JsonlDecodeError> {
    let effective_args = require_object(value.get("effectiveArgs"), "effectiveArgs")?.clone();
    Ok(RecordBody::ToolStarted {
        id: decode_field(value, "id")?,
        lane: decode_field(value, "lane")?,
        run_id: decode_field(value, "runId")?,
        assistant_entry_id: decode_field(value, "assistantEntryId")?,
        tool_index: decode_field(value, "toolIndex")?,
        tool_call_id: decode_field(value, "toolCallId")?,
        tool_name: decode_field(value, "toolName")?,
        effective_args,
        result_entry_id: decode_field(value, "resultEntryId")?,
        replay: decode_field(value, "replay")?,
    })
}

fn decode_queue_enqueued(value: &JsObject) -> Result<RecordBody, JsonlDecodeError> {
    Ok(RecordBody::QueueEnqueued {
        id: decode_field(value, "id")?,
        lane: decode_field(value, "lane")?,
        queue: decode_field(value, "queue")?,
        run_id: decode_field(value, "runId")?,
        target: decode_entry_field(value, "target")?,
    })
}

fn decode_usage(value: &JsObject) -> Result<RecordBody, JsonlDecodeError> {
    Ok(RecordBody::Usage {
        id: decode_field(value, "id")?,
        lane: decode_field(value, "lane")?,
        cause: decode_field(value, "cause")?,
        run_id: decode_field(value, "runId")?,
        entry_id: decode_field(value, "entryId")?,
        attempt: decode_field(value, "attempt")?,
        stop_reason: decode_field(value, "stopReason")?,
        tool_call_id: decode_field(value, "toolCallId")?,
        details: decode_optional_payload(value, "details"),
        usage: decode_field(value, "usage")?,
    })
}

fn decode_operation_started(value: &JsObject) -> Result<RecordBody, JsonlDecodeError> {
    let intent = require_object(value.get("intent"), "intent")?;
    Ok(RecordBody::OperationStarted {
        id: decode_field(value, "id")?,
        lane: decode_field(value, "lane")?,
        source_leaf_id: decode_field(value, "sourceLeafId")?,
        intent: decode_operation_intent(intent)?,
    })
}

fn decode_operation_intent(value: &JsObject) -> Result<OperationIntent, JsonlDecodeError> {
    let kind = require_string(value.get("kind"), "operation kind")?;
    match kind.as_str() {
        "run" => decode_run_intent(value),
        "compaction" => Ok(OperationIntent::Compaction {
            custom_instructions: decode_field(value, "customInstructions")?,
            result_entry_id: decode_field(value, "resultEntryId")?,
        }),
        "navigation" => Ok(OperationIntent::Navigation {
            target_id: decode_field(value, "targetId")?,
            summarize: decode_field(value, "summarize")?,
            custom_instructions: decode_field(value, "customInstructions")?,
            label: decode_field(value, "label")?,
            summary_entry_id: decode_field(value, "summaryEntryId")?,
        }),
        _ => Err(schema_error(format!("has unknown operation kind {kind}"))),
    }
}

fn decode_run_intent(value: &JsObject) -> Result<OperationIntent, JsonlDecodeError> {
    let entries = value
        .get("initialMessages")
        .and_then(JsValue::as_array)
        .ok_or_else(|| schema_error("has invalid initialMessages"))?;
    let initial_messages = entries
        .iter()
        .map(|entry| decode_nested_entry(require_object(Some(entry), "initialMessages entry")?))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(OperationIntent::Run {
        original_prompt: decode_field(value, "originalPrompt")?,
        initial_messages,
        system_prompt_override: decode_field(value, "systemPromptOverride")?,
        resume_data: decode_field(value, "resumeData")?,
    })
}

fn decode_entry_field(value: &JsObject, key: &str) -> Result<EntryBody, JsonlDecodeError> {
    decode_nested_entry(require_object(value.get(key), key)?)
}

fn decode_nested_entry(value: &JsObject) -> Result<EntryBody, JsonlDecodeError> {
    let tag = require_string(value.get("type"), "entry type")?;
    let kind = known_entry_type(&tag).ok_or_else(|| schema_error(format!("has unknown entry type {tag}")))?;
    decode_entry_body(value, kind, decode_field(value, "id")?)
}

fn require_object<'a>(value: Option<&'a JsValue>, key: &str) -> Result<&'a JsObject, JsonlDecodeError> {
    value
        .and_then(JsValue::as_object)
        .ok_or_else(|| schema_error(format!("has invalid {key}")))
}
