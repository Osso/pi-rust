//! Shared fixtures for the harness reducer tests.

use pi_agent_core::harness::reducer::{
    LaneReductionInput, LaneReductionResult, RecordLogCorruptionReason, RecordLogSlice, reduce_lane_state,
};
use serde_json::{Value, json};

pub(crate) fn usage() -> Value {
    json!({
        "input": 1,
        "output": 2,
        "cacheRead": 3,
        "cacheWrite": 4,
        "totalTokens": 10,
        "cost": {"input": 0.1, "output": 0.2, "cacheRead": 0.3, "cacheWrite": 0.4, "total": 1.0}
    })
}

pub(crate) fn user_message(text: &str) -> Value {
    json!({"role": "user", "content": [{"type": "text", "text": text}], "timestamp": 1})
}

pub(crate) fn assistant_message(content: Value, stop_reason: &str) -> Value {
    let mut message = json!({
        "role": "assistant",
        "content": content,
        "api": "openai-responses",
        "provider": "openai",
        "model": "test-model",
        "usage": usage(),
        "stopReason": stop_reason,
        "timestamp": 1
    });
    if stop_reason == "deferred" {
        message["deferred"] = json!({
            "provider": "openai",
            "modelId": "test-model",
            "api": "openai-responses",
            "id": "deferred-1"
        });
    }
    message
}

pub(crate) fn tool_result_message(tool_call_id: &str, tool_name: &str) -> Value {
    json!({
        "role": "toolResult",
        "toolCallId": tool_call_id,
        "toolName": tool_name,
        "content": [{"type": "text", "text": "result"}],
        "isError": false,
        "timestamp": 1
    })
}

pub(crate) fn message_target(id: &str, message: Value) -> Value {
    json!({"type": "message", "id": id, "message": message})
}

pub(crate) fn persisted_entry(target: Value, seq: i64, parent_id: Option<&str>) -> Value {
    let mut entry = target;
    entry["parentId"] = parent_id.map_or(Value::Null, |parent| json!(parent));
    entry["seq"] = json!(seq);
    entry["timestamp"] = json!(seq);
    entry
}

pub(crate) fn run_started(seq: i64, id: &str, initial_messages: Vec<Value>) -> Value {
    json!({
        "type": "operation_started",
        "id": id,
        "lane": "main",
        "seq": seq,
        "timestamp": seq,
        "sourceLeafId": null,
        "intent": {"kind": "run", "originalPrompt": [], "initialMessages": initial_messages}
    })
}

pub(crate) fn compaction_started(seq: i64, result_entry_id: &str) -> Value {
    json!({
        "type": "operation_started",
        "id": "compact-1",
        "lane": "main",
        "seq": seq,
        "timestamp": seq,
        "sourceLeafId": "source",
        "intent": {"kind": "compaction", "resultEntryId": result_entry_id}
    })
}

pub(crate) fn navigation_started(seq: i64, summary_entry_id: &str) -> Value {
    json!({
        "type": "operation_started",
        "id": "navigate-1",
        "lane": "main",
        "seq": seq,
        "timestamp": seq,
        "sourceLeafId": "source",
        "intent": {"kind": "navigation", "targetId": "target", "summarize": true, "summaryEntryId": summary_entry_id}
    })
}

pub(crate) fn attempt(seq: i64, run_id: &str, step: &str, attempt_number: i64, result_entry_id: &str) -> Value {
    attempt_with_reason(seq, run_id, step, attempt_number, result_entry_id, Some("manual"))
}

pub(crate) fn attempt_with_reason(
    seq: i64,
    run_id: &str,
    step: &str,
    attempt_number: i64,
    result_entry_id: &str,
    compaction_reason: Option<&str>,
) -> Value {
    let mut record = json!({
        "type": "step_attempt",
        "id": format!("attempt-{seq}"),
        "lane": "main",
        "seq": seq,
        "timestamp": seq,
        "runId": run_id,
        "attempt": attempt_number,
        "resultEntryId": result_entry_id,
        "step": step
    });
    if step == "compaction" {
        record["compactionReason"] = json!(compaction_reason.unwrap_or("manual"));
    } else if let Some(reason) = compaction_reason
        && reason != "manual"
    {
        record["compactionReason"] = json!(reason);
    }
    record
}

pub(crate) fn abort_requested(seq: i64, run_id: &str) -> Value {
    json!({"type": "abort_requested", "id": format!("abort-{seq}"), "lane": "main", "seq": seq, "timestamp": seq, "runId": run_id})
}

pub(crate) fn operation_finished(seq: i64, run_id: &str, outcome: &str) -> Value {
    json!({"type": "operation_finished", "id": format!("finish-{seq}"), "lane": "main", "seq": seq, "timestamp": seq, "runId": run_id, "outcome": outcome})
}

pub(crate) fn tool_started(seq: i64, overrides: Value) -> Value {
    json!({
        "type": "tool_started",
        "id": format!("tool-start-{seq}"),
        "lane": "main",
        "seq": seq,
        "timestamp": seq,
        "runId": overrides.get("runId").cloned().unwrap_or(json!("run-1")),
        "assistantEntryId": overrides.get("assistantEntryId").cloned().unwrap_or(json!("assistant-tools")),
        "toolIndex": overrides.get("toolIndex").cloned().unwrap_or(json!(0)),
        "toolCallId": overrides.get("toolCallId").cloned().unwrap_or(json!("call-1")),
        "toolName": overrides.get("toolName").cloned().unwrap_or(json!("tool-1")),
        "effectiveArgs": {},
        "resultEntryId": overrides.get("resultEntryId").cloned().unwrap_or(json!("tool-result-1")),
        "replay": "never"
    })
}

pub(crate) fn queue_enqueued(seq: i64, target: Value, queue: &str) -> Value {
    let mut record = json!({
        "type": "queue_enqueued",
        "id": format!("queue-{seq}"),
        "lane": "main",
        "seq": seq,
        "timestamp": seq,
        "target": target,
        "queue": queue
    });
    if queue != "nextRun" {
        record["runId"] = json!("run-1");
    }
    record
}

pub(crate) fn queue_cancelled(seq: i64, entry_id: &str, run_id: Option<&str>) -> Value {
    let mut record = json!({
        "type": "queue_cancelled",
        "id": format!("cancel-{seq}"),
        "lane": "main",
        "seq": seq,
        "timestamp": seq,
        "entryId": entry_id
    });
    if let Some(run_id) = run_id {
        record["runId"] = json!(run_id);
    }
    record
}

pub(crate) fn write_deferred(seq: i64, target: Value) -> Value {
    json!({"type": "write_deferred", "id": format!("write-{seq}"), "lane": "main", "seq": seq, "timestamp": seq, "runId": "run-1", "target": target})
}

pub(crate) fn usage_record(seq: i64, result_entry_id: &str, stop_reason: &str, attempt_number: i64) -> Value {
    json!({
        "type": "usage",
        "id": format!("usage-{seq}"),
        "lane": "main",
        "seq": seq,
        "timestamp": seq,
        "cause": "assistant",
        "runId": "run-1",
        "entryId": result_entry_id,
        "attempt": attempt_number,
        "stopReason": stop_reason,
        "usage": usage()
    })
}

pub(crate) fn deferred_fetch_usage(seq: i64, entry_id: &str) -> Value {
    json!({
        "type": "usage",
        "id": format!("deferred-usage-{seq}"),
        "lane": "main",
        "seq": seq,
        "timestamp": seq,
        "cause": "deferred_fetch",
        "runId": "run-1",
        "entryId": entry_id,
        "attempt": 1,
        "stopReason": "error",
        "usage": usage()
    })
}

pub(crate) fn compaction_entry(id: &str, seq: i64) -> Value {
    json!({"type": "compaction", "id": id, "parentId": null, "seq": seq, "timestamp": seq, "summary": "summary", "retainedTail": [], "tokensBefore": 10})
}

pub(crate) fn branch_summary_entry(id: &str, seq: i64) -> Value {
    json!({"type": "branch_summary", "id": id, "parentId": "target", "seq": seq, "timestamp": seq, "fromId": "source", "summary": "summary"})
}

pub(crate) fn recovery_slice(record_values: &[Value], entry_values: &[Value]) -> RecordLogSlice {
    let finished: Vec<&str> = record_values
        .iter()
        .filter(|record| record["type"] == "operation_finished")
        .filter_map(|record| record["runId"].as_str())
        .collect();
    let mut open_operations: Vec<Value> = record_values
        .iter()
        .filter(|record| record["type"] == "operation_started")
        .filter(|record| record["id"].as_str().is_some_and(|id| !finished.contains(&id)))
        .cloned()
        .collect();
    open_operations.sort_by(|left, right| right["seq"].as_i64().cmp(&left["seq"].as_i64()));
    serde_json::from_value(json!({
        "lane": "main",
        "openOperations": open_operations,
        "records": record_values,
        "entries": entry_values,
    }))
    .expect("fixture slice")
}

pub(crate) fn defaults() -> Value {
    json!({
        "model": {"provider": "default-provider", "modelId": "default-model"},
        "thinkingLevel": "off",
        "activeToolNames": ["default-tool"]
    })
}

pub(crate) fn reduction_input(
    record_values: &[Value],
    own_entry_values: &[Value],
    extra_entries: &[Value],
    configuration_entries: &[Value],
    leaf_id: Option<&str>,
) -> LaneReductionInput {
    let slice = recovery_slice(
        record_values,
        &own_entry_values
            .iter()
            .chain(extra_entries.iter())
            .cloned()
            .collect::<Vec<Value>>(),
    );
    let slice_value = serde_json::to_value(&slice).unwrap();
    let leaf_id = match leaf_id {
        Some(leaf_id) => json!(leaf_id),
        None => json!(own_entry_values.last().map(|entry| entry["id"].clone())),
    };
    serde_json::from_value(json!({
        "lane": slice_value["lane"],
        "openOperations": slice_value["openOperations"],
        "records": slice_value["records"],
        "entries": slice_value["entries"],
        "leafId": leaf_id,
        "ownEntries": own_entry_values,
        "configurationEntries": configuration_entries,
        "defaults": defaults(),
    }))
    .expect("fixture reduction input")
}

pub(crate) fn reduce(
    record_values: &[Value],
    own_entry_values: &[Value],
    extra_entries: &[Value],
) -> LaneReductionResult {
    reduce_lane_state(&reduction_input(
        record_values,
        own_entry_values,
        extra_entries,
        &[],
        None,
    ))
    .expect("reduction must succeed")
}

pub(crate) fn assistant_tools_entry() -> Value {
    persisted_entry(
        message_target(
            "assistant-tools",
            assistant_message(
                json!([{"type": "toolCall", "id": "call-1", "name": "tool-1", "arguments": {}}]),
                "toolUse",
            ),
        ),
        3,
        None,
    )
}

pub(crate) struct CorruptionCase {
    pub(crate) reason: RecordLogCorruptionReason,
    pub(crate) message: &'static str,
    pub(crate) slice: RecordLogSlice,
}

pub(crate) fn corruption_cases() -> Vec<CorruptionCase> {
    let mut assistant_with_reason = attempt(2, "run-1", "assistant", 1, "assistant-1");
    assistant_with_reason["compactionReason"] = json!("manual");
    let mut compaction_without_reason = attempt(2, "run-1", "compaction", 1, "compaction-1");
    compaction_without_reason
        .as_object_mut()
        .unwrap()
        .remove("compactionReason");
    let mut deferred_without_handle = assistant_message(json!([]), "deferred");
    deferred_without_handle.as_object_mut().unwrap().remove("deferred");
    let ordered_deferred_entries: Vec<Value> = [
        "zeta", "alpha", "middle", "beta", "omega", "delta", "gamma", "epsilon", "kappa", "iota", "theta", "eta",
        "lambda", "mu", "nu", "xi",
    ]
    .iter()
    .enumerate()
    .map(|(offset, id)| {
        persisted_entry(
            message_target(&format!("{id}-deferred"), deferred_without_handle.clone()),
            offset as i64 + 2,
            None,
        )
    })
    .collect();
    let mut duplicate = tool_started(5, json!({"resultEntryId": "tool-result-2"}));
    duplicate["id"] = json!("tool-start-duplicate");

    vec![
        CorruptionCase {
            reason: RecordLogCorruptionReason::MultipleOpenOperations,
            message: "Lane main has at least two open operations",
            slice: recovery_slice(&[run_started(1, "run-1", vec![]), run_started(2, "run-2", vec![])], &[]),
        },
        CorruptionCase {
            reason: RecordLogCorruptionReason::UnknownOperation,
            message: "Record abort-1 references unknown operation missing",
            slice: recovery_slice(&[abort_requested(1, "missing")], &[]),
        },
        CorruptionCase {
            reason: RecordLogCorruptionReason::RecordAfterFinish,
            message: "Record abort-3 follows the finish of operation run-1",
            slice: recovery_slice(
                &[
                    run_started(1, "run-1", vec![]),
                    operation_finished(2, "run-1", "completed"),
                    abort_requested(3, "run-1"),
                ],
                &[],
            ),
        },
        CorruptionCase {
            reason: RecordLogCorruptionReason::NonConsecutiveAttempt,
            message: "assistant attempt attempt-3 is 3; expected 2",
            slice: recovery_slice(
                &[
                    run_started(1, "run-1", vec![]),
                    attempt(2, "run-1", "assistant", 1, "assistant-1"),
                    attempt(3, "run-1", "assistant", 3, "assistant-2"),
                ],
                &[],
            ),
        },
        CorruptionCase {
            reason: RecordLogCorruptionReason::InvalidCompactionReason,
            message: "assistant attempt attempt-2 has a compaction reason",
            slice: recovery_slice(&[run_started(1, "run-1", vec![]), assistant_with_reason], &[]),
        },
        CorruptionCase {
            reason: RecordLogCorruptionReason::InvalidCompactionReason,
            message: "Compaction attempt attempt-2 has no valid compaction reason",
            slice: recovery_slice(&[run_started(1, "run-1", vec![]), compaction_without_reason], &[]),
        },
        CorruptionCase {
            reason: RecordLogCorruptionReason::QueueAfterAbort,
            message: "steer item queue-1 was enqueued after abort",
            slice: recovery_slice(
                &[
                    run_started(1, "run-1", vec![]),
                    abort_requested(2, "run-1"),
                    queue_enqueued(3, message_target("queue-1", user_message("queued")), "steer"),
                ],
                &[],
            ),
        },
        CorruptionCase {
            reason: RecordLogCorruptionReason::InvalidQueueCancellation,
            message: "Queue cancellation cancel-2 has no pending matching enqueue",
            slice: recovery_slice(
                &[
                    run_started(1, "run-1", vec![]),
                    queue_cancelled(2, "queue-1", Some("run-1")),
                ],
                &[],
            ),
        },
        CorruptionCase {
            reason: RecordLogCorruptionReason::InvalidQueueCancellation,
            message: "Queue cancellation cancel-4 has no pending matching enqueue",
            slice: recovery_slice(
                &[
                    run_started(1, "run-1", vec![]),
                    queue_enqueued(2, message_target("queue-1", user_message("queued")), "steer"),
                    queue_cancelled(4, "queue-1", Some("run-1")),
                ],
                &[persisted_entry(
                    message_target("queue-1", user_message("queued")),
                    3,
                    None,
                )],
            ),
        },
        CorruptionCase {
            reason: RecordLogCorruptionReason::InconsistentStep,
            message: "compaction attempts disagree on their result entry id",
            slice: recovery_slice(
                &[
                    run_started(1, "run-1", vec![]),
                    attempt_with_reason(2, "run-1", "compaction", 1, "compaction-1", Some("threshold")),
                    attempt_with_reason(3, "run-1", "compaction", 2, "compaction-2", Some("threshold")),
                ],
                &[],
            ),
        },
        CorruptionCase {
            reason: RecordLogCorruptionReason::InconsistentStep,
            message: "compaction attempts disagree on their compaction reason",
            slice: recovery_slice(
                &[
                    run_started(1, "run-1", vec![]),
                    attempt_with_reason(2, "run-1", "compaction", 1, "compaction-1", Some("threshold")),
                    attempt_with_reason(3, "run-1", "compaction", 2, "compaction-1", Some("overflow")),
                ],
                &[],
            ),
        },
        CorruptionCase {
            reason: RecordLogCorruptionReason::ToolCallMismatch,
            message: "Tool start tool-start-4 does not match its assistant tool-call ordinal",
            slice: recovery_slice(
                &[
                    run_started(1, "run-1", vec![]),
                    tool_started(4, json!({"toolCallId": "different-call"})),
                ],
                &[assistant_tools_entry()],
            ),
        },
        CorruptionCase {
            reason: RecordLogCorruptionReason::DuplicateToolInvocation,
            message: "Tool invocation assistant-tools:0 is duplicated",
            slice: recovery_slice(
                &[run_started(1, "run-1", vec![]), tool_started(4, json!({})), duplicate],
                &[assistant_tools_entry()],
            ),
        },
        CorruptionCase {
            reason: RecordLogCorruptionReason::ProvisionedEntryMismatch,
            message: "Provisioned entry prompt-1 exists with content different from its intent",
            slice: recovery_slice(
                &[run_started(
                    1,
                    "run-1",
                    vec![message_target("prompt-1", user_message("expected"))],
                )],
                &[persisted_entry(
                    message_target("prompt-1", user_message("different")),
                    2,
                    None,
                )],
            ),
        },
        CorruptionCase {
            reason: RecordLogCorruptionReason::InvalidDeferredHandle,
            message: "Deferred assistant entry assistant-deferred does not carry a handle",
            slice: recovery_slice(
                &[run_started(1, "run-1", vec![])],
                &[persisted_entry(
                    message_target("assistant-deferred", deferred_without_handle.clone()),
                    2,
                    None,
                )],
            ),
        },
        // The first invalid entry in source order must be reported. Entries are
        // inserted with `zeta` first, so lexicographic or hash map iteration
        // would name a different entry.
        CorruptionCase {
            reason: RecordLogCorruptionReason::InvalidDeferredHandle,
            message: "Deferred assistant entry zeta-deferred does not carry a handle",
            slice: recovery_slice(&[run_started(1, "run-1", vec![])], &ordered_deferred_entries),
        },
    ]
}
