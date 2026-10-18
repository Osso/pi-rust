//! Behavioral port of `packages/agent/test/harness/reducer.test.ts`.

#[path = "support/reducer_fixtures.rs"]
mod fixtures;

use fixtures::*;
use pi_agent_core::harness::reducer::{RecordLogSlice, reduce_lane_state, validate_record_log};
use serde_json::{Value, json};

#[test]
fn rejects_corrupt_record_logs_with_the_source_reasons_and_messages() {
    let cases = corruption_cases();
    assert_eq!(cases.len(), 15);
    for case in cases {
        let error = validate_record_log(&case.slice).expect_err("slice must be corrupt");
        assert_eq!(error.reason(), case.reason);
        assert_eq!(error.message(), case.message);
    }
}

fn valid_prefixes(actions: &[Value]) -> Vec<RecordLogSlice> {
    (0..actions.len())
        .map(|index| {
            let prefix = &actions[..=index];
            let record_values: Vec<Value> = prefix
                .iter()
                .filter_map(|action| action.get("record").cloned())
                .collect();
            let entry_values: Vec<Value> = prefix
                .iter()
                .filter_map(|action| action.get("entry").cloned())
                .collect();
            recovery_slice(&record_values, &entry_values)
        })
        .collect()
}

#[test]
fn accepts_every_durable_prefix_of_the_source_traces() {
    let prompt_target = message_target("prompt-1", user_message("fix the bug"));
    let assistant_tool_target = message_target(
        "assistant-tools",
        assistant_message(
            json!([{"type": "toolCall", "id": "call-1", "name": "tool-1", "arguments": {}}]),
            "toolUse",
        ),
    );
    let tool_result_target = message_target("tool-result-1", tool_result_message("call-1", "tool-1"));
    let assistant_final_target = message_target(
        "assistant-final",
        assistant_message(json!([{"type": "text", "text": "done"}]), "stop"),
    );
    let traces: Vec<Vec<Value>> = vec![
        vec![
            json!({"record": run_started(1, "run-1", vec![prompt_target.clone()])}),
            json!({"entry": persisted_entry(prompt_target.clone(), 2, None)}),
            json!({"record": attempt(3, "run-1", "assistant", 1, "assistant-tools")}),
            json!({"entry": persisted_entry(assistant_tool_target.clone(), 4, Some("prompt-1"))}),
            json!({"record": tool_started(5, json!({}))}),
            json!({"entry": persisted_entry(tool_result_target.clone(), 6, Some("assistant-tools"))}),
            json!({"record": attempt(7, "run-1", "assistant", 1, "assistant-final")}),
            json!({"entry": persisted_entry(assistant_final_target.clone(), 8, Some("tool-result-1"))}),
            json!({"record": operation_finished(9, "run-1", "completed")}),
        ],
        vec![
            json!({"record": run_started(1, "run-1", vec![])}),
            json!({"record": attempt(2, "run-1", "assistant", 1, "assistant-attempt-1")}),
            json!({"record": usage_record(3, "assistant-attempt-1", "error", 1)}),
            json!({"record": attempt(4, "run-1", "assistant", 2, "assistant-attempt-2")}),
            json!({"record": usage_record(5, "assistant-attempt-2", "stop", 2)}),
            json!({"entry": persisted_entry(
                message_target("assistant-attempt-2", assistant_message(json!([{"type": "text", "text": "ok"}]), "stop")),
                6,
                None,
            )}),
        ],
        vec![
            json!({"record": run_started(1, "run-1", vec![])}),
            json!({"record": attempt(2, "run-1", "assistant", 1, "assistant-error")}),
            json!({"entry": persisted_entry(
                message_target("assistant-error", {
                    let mut message = assistant_message(json!([]), "error");
                    message["errorMessage"] = json!("failed");
                    message
                }),
                3,
                None,
            )}),
            json!({"record": operation_finished(4, "run-1", "failed")}),
        ],
        vec![
            json!({"record": run_started(1, "run-1", vec![])}),
            json!({"record": attempt(2, "run-1", "assistant", 1, "discarded-overflow")}),
            json!({"record": usage_record(3, "discarded-overflow", "length", 1)}),
            json!({"record": attempt_with_reason(4, "run-1", "compaction", 1, "overflow-compaction", Some("overflow"))}),
            json!({"entry": compaction_entry("overflow-compaction", 5)}),
            json!({"record": attempt(6, "run-1", "assistant", 1, "assistant-after-compaction")}),
            json!({"entry": persisted_entry(
                message_target("assistant-after-compaction", assistant_message(json!([{"type": "text", "text": "fits"}]), "stop")),
                7,
                None,
            )}),
        ],
        vec![
            json!({"record": run_started(1, "run-1", vec![])}),
            json!({"record": queue_enqueued(2, message_target("queue-1", user_message("queued")), "steer")}),
            json!({"entry": persisted_entry(message_target("queue-1", user_message("queued")), 3, None)}),
        ],
        vec![
            json!({"record": run_started(1, "run-1", vec![])}),
            json!({"record": queue_enqueued(2, message_target("queue-1", user_message("queued")), "steer")}),
            json!({"record": queue_cancelled(3, "queue-1", Some("run-1"))}),
        ],
        vec![
            json!({"record": run_started(1, "run-1", vec![])}),
            json!({"record": write_deferred(2, message_target("write-1", user_message("deferred write")))}),
            json!({"entry": persisted_entry(message_target("write-1", user_message("deferred write")), 3, None)}),
        ],
        vec![
            json!({"record": run_started(1, "run-1", vec![])}),
            json!({"record": attempt(2, "run-1", "assistant", 1, "assistant-tools")}),
            json!({"entry": persisted_entry(assistant_tool_target.clone(), 3, None)}),
            json!({"record": tool_started(4, json!({}))}),
            json!({"record": abort_requested(5, "run-1")}),
            json!({"entry": persisted_entry(
                message_target("tool-result-1", {
                    let mut message = tool_result_message("call-1", "tool-1");
                    message["content"] = json!([{"type": "text", "text": "interrupted"}]);
                    message["isError"] = json!(true);
                    message
                }),
                6,
                None,
            )}),
        ],
        vec![
            json!({"record": run_started(1, "run-1", vec![])}),
            json!({"record": attempt_with_reason(2, "run-1", "compaction", 1, "threshold-compaction", Some("threshold"))}),
            json!({"entry": compaction_entry("threshold-compaction", 3)}),
            json!({"record": attempt(4, "run-1", "assistant", 1, "assistant-after-threshold")}),
        ],
        vec![
            json!({"record": compaction_started(1, "compaction-1")}),
            json!({"record": attempt(2, "compact-1", "compaction", 1, "compaction-1")}),
            json!({"entry": compaction_entry("compaction-1", 3)}),
            json!({"record": operation_finished(4, "compact-1", "completed")}),
        ],
        vec![
            json!({"record": navigation_started(1, "summary-1")}),
            json!({"record": attempt(2, "navigate-1", "branch_summary", 1, "summary-1")}),
            json!({"entry": branch_summary_entry("summary-1", 3)}),
            json!({"record": operation_finished(4, "navigate-1", "completed")}),
        ],
        vec![
            json!({"record": run_started(1, "run-1", vec![])}),
            json!({"record": attempt(2, "run-1", "assistant", 1, "assistant-tools")}),
            json!({"entry": persisted_entry(assistant_tool_target.clone(), 3, None)}),
            json!({"entry": persisted_entry(
                message_target("blocked-result", {
                    let mut message = tool_result_message("call-1", "tool-1");
                    message["content"] = json!([{"type": "text", "text": "blocked"}]);
                    message["isError"] = json!(true);
                    message
                }),
                4,
                None,
            )}),
        ],
        vec![
            json!({"record": queue_enqueued(1, message_target("next-1", user_message("later")), "nextRun")}),
            json!({"record": queue_cancelled(2, "next-1", None)}),
        ],
        vec![
            json!({"record": run_started(1, "run-1", vec![])}),
            json!({"record": abort_requested(2, "run-1")}),
            json!({"record": queue_enqueued(3, message_target("next-1", user_message("later")), "nextRun")}),
        ],
        vec![
            json!({"record": run_started(1, "run-1", vec![])}),
            json!({"record": write_deferred(2, message_target("write-1", user_message("deferred write")))}),
            json!({"record": abort_requested(3, "run-1")}),
            json!({"entry": persisted_entry(message_target("write-1", user_message("deferred write")), 4, None)}),
        ],
        vec![
            json!({"record": run_started(1, "run-1", vec![])}),
            json!({"record": queue_enqueued(2, message_target("queue-1", user_message("queued")), "steer")}),
            json!({"record": abort_requested(3, "run-1")}),
        ],
        vec![
            json!({"record": run_started(1, "run-1", vec![])}),
            json!({"record": attempt_with_reason(2, "run-1", "compaction", 1, "threshold-compaction", Some("threshold"))}),
            json!({"record": attempt_with_reason(3, "run-1", "compaction", 2, "threshold-compaction", Some("threshold"))}),
            json!({"entry": compaction_entry("threshold-compaction", 4)}),
        ],
        vec![
            json!({"record": compaction_started(1, "compaction-1")}),
            json!({"entry": compaction_entry("compaction-1", 2)}),
            json!({"record": operation_finished(3, "compact-1", "completed")}),
        ],
        vec![
            json!({"record": navigation_started(1, "summary-1")}),
            json!({"entry": branch_summary_entry("summary-1", 2)}),
            json!({"record": operation_finished(3, "navigate-1", "completed")}),
        ],
        vec![
            json!({"record": run_started(1, "run-1", vec![])}),
            json!({"record": attempt(2, "run-1", "assistant", 1, "assistant-deferred")}),
            json!({"entry": persisted_entry(
                message_target("assistant-deferred", assistant_message(json!([]), "deferred")),
                3,
                None,
            )}),
            json!({"entry": persisted_entry(
                message_target("assistant-redeemed", assistant_message(json!([{"type": "text", "text": "ready"}]), "stop")),
                4,
                None,
            )}),
        ],
        vec![
            json!({"record": run_started(1, "run-1", vec![])}),
            json!({"record": attempt(2, "run-1", "assistant", 1, "assistant-deferred")}),
            json!({"entry": persisted_entry(
                message_target("assistant-deferred", assistant_message(json!([]), "deferred")),
                3,
                None,
            )}),
            json!({"record": abort_requested(4, "run-1")}),
        ],
    ];

    for trace in traces {
        for slice in valid_prefixes(&trace) {
            assert!(validate_record_log(&slice).is_ok(), "prefix must be valid: {slice:?}");
        }
    }
}

#[test]
fn reduces_an_idle_lane_to_pending_next_run_input_and_default_configuration() {
    let pending = message_target("next-pending", user_message("pending"));
    let cancelled = message_target("next-cancelled", user_message("cancelled"));
    let consumed = message_target("next-consumed", user_message("consumed"));
    let record_values = vec![
        queue_enqueued(1, pending.clone(), "nextRun"),
        queue_enqueued(2, cancelled.clone(), "nextRun"),
        queue_cancelled(3, "next-cancelled", None),
        queue_enqueued(4, consumed.clone(), "nextRun"),
    ];
    let input = reduction_input(
        &record_values,
        &[],
        &[persisted_entry(consumed, 5, None)],
        &[],
        Some("idle-leaf"),
    );

    assert_eq!(
        serde_json::to_value(reduce_lane_state(&input).unwrap()).unwrap(),
        json!({
            "laneState": {
                "lane": "main",
                "leafId": "idle-leaf",
                "operation": null,
                "pendingNextRun": [pending]
            },
            "effectiveConfiguration": defaults(),
            "terminalFailure": null
        })
    );
}

#[test]
fn folds_persisted_configuration_over_copied_defaults_in_sequence() {
    let configuration_entries = vec![
        json!({"type": "model_change", "id": "model-change", "parentId": null, "seq": 1, "timestamp": 1, "provider": "persisted-provider", "modelId": "persisted-model"}),
        json!({"type": "thinking_level_change", "id": "thinking-change", "parentId": "model-change", "seq": 2, "timestamp": 2, "thinkingLevel": "high"}),
        json!({"type": "active_tools_change", "id": "tools-change", "parentId": "thinking-change", "seq": 3, "timestamp": 3, "activeToolNames": ["persisted-tool"]}),
    ];
    let input = reduction_input(&[], &[], &[], &configuration_entries, None);
    let result = reduce_lane_state(&input).unwrap();

    assert_eq!(
        serde_json::to_value(&result.effective_configuration).unwrap(),
        json!({
            "model": {"provider": "persisted-provider", "modelId": "persisted-model"},
            "thinkingLevel": "high",
            "activeToolNames": ["persisted-tool"]
        })
    );
}

#[test]
fn applies_committed_operation_owned_configuration_after_the_anchor() {
    let assistant = persisted_entry(
        message_target("assistant-config", {
            let mut message = assistant_message(json!([{"type": "text", "text": "response"}]), "stop");
            message["provider"] = json!("response-provider");
            message["model"] = json!("response-model");
            message
        }),
        2,
        None,
    );
    let tools = json!({"type": "active_tools_change", "id": "operation-tools", "parentId": "assistant-config", "seq": 3, "timestamp": 3, "activeToolNames": ["operation-tool"]});
    let result = reduce(&[run_started(1, "run-1", vec![])], &[assistant, tools], &[]);

    assert_eq!(
        serde_json::to_value(&result.effective_configuration).unwrap(),
        json!({
            "model": {"provider": "response-provider", "modelId": "response-model"},
            "thinkingLevel": "off",
            "activeToolNames": ["operation-tool"]
        })
    );
}

#[test]
fn keeps_captured_next_run_input_with_the_open_run() {
    let captured = message_target("next-captured", user_message("captured"));
    let later = message_target("next-later", user_message("later"));
    let result = reduce(
        &[
            queue_enqueued(1, captured.clone(), "nextRun"),
            run_started(2, "run-1", vec![captured.clone()]),
            queue_enqueued(3, later.clone(), "nextRun"),
        ],
        &[],
        &[],
    );

    let operation = result.lane_state.operation.as_ref().unwrap();
    assert_eq!(
        serde_json::to_value(&result.lane_state.pending_next_run).unwrap(),
        json!([later])
    );
    assert_eq!(
        serde_json::to_value(&operation.missing_initial_messages).unwrap(),
        json!([captured])
    );
}

#[test]
fn derives_missing_input_queues_deferred_writes_and_the_unfinished_attempt() {
    let missing_prompt = message_target("prompt-missing", user_message("missing"));
    let committed_prompt = message_target("prompt-committed", user_message("committed"));
    let steer = message_target("steer-pending", user_message("steer"));
    let consumed_follow_up = message_target("follow-consumed", user_message("follow"));
    let next_run = message_target("next-run", user_message("next"));
    let pending_write = message_target("write-pending", user_message("write"));
    let applied_write = message_target("write-applied", user_message("applied"));
    let record_values = vec![
        run_started(1, "run-1", vec![missing_prompt.clone(), committed_prompt.clone()]),
        queue_enqueued(3, steer.clone(), "steer"),
        queue_enqueued(4, consumed_follow_up.clone(), "followUp"),
        queue_enqueued(5, next_run.clone(), "nextRun"),
        write_deferred(7, pending_write.clone()),
        write_deferred(8, applied_write.clone()),
        attempt(10, "run-1", "assistant", 1, "assistant-pending"),
    ];
    let own_entries = vec![
        persisted_entry(committed_prompt.clone(), 2, None),
        persisted_entry(consumed_follow_up, 6, Some("prompt-committed")),
        persisted_entry(applied_write.clone(), 9, Some("follow-consumed")),
    ];
    let result = reduce(&record_values, &own_entries, &[]);

    let operation = result.lane_state.operation.as_ref().unwrap();
    assert_eq!(
        serde_json::to_value(&result.lane_state.pending_next_run).unwrap(),
        json!([next_run])
    );
    assert_eq!(
        serde_json::to_value(operation).unwrap(),
        json!({
            "id": "run-1",
            "kind": "run",
            "intent": {"kind": "run", "originalPrompt": [], "initialMessages": [missing_prompt, committed_prompt]},
            "aborting": false,
            "step": {"kind": "assistant", "attempts": 1, "resultEntryId": "assistant-pending"},
            "toolBatch": null,
            "missingInitialMessages": [missing_prompt],
            "pendingSteer": [steer],
            "pendingFollowUp": [],
            "pendingWrites": [pending_write],
            "deferred": null,
            "overflowRecoveryUsed": false,
            "newestOwn": {"entryId": applied_write["id"], "type": "message", "role": "user"},
            "targets": {}
        })
    );
}

#[test]
fn kills_steer_and_follow_up_queues_on_abort_while_preserving_writes_and_next_run_input() {
    let steer = message_target("steer-aborted", user_message("steer"));
    let follow_up = message_target("follow-aborted", user_message("follow"));
    let next_run = message_target("next-after-abort", user_message("next"));
    let pending_write = message_target("write-after-abort", user_message("write"));
    let result = reduce(
        &[
            run_started(1, "run-1", vec![]),
            queue_enqueued(2, steer, "steer"),
            queue_enqueued(3, follow_up, "followUp"),
            queue_enqueued(4, next_run.clone(), "nextRun"),
            write_deferred(5, pending_write.clone()),
            abort_requested(6, "run-1"),
        ],
        &[],
        &[],
    );

    let operation = result.lane_state.operation.as_ref().unwrap();
    assert!(operation.aborting);
    assert!(operation.pending_steer.is_empty());
    assert!(operation.pending_follow_up.is_empty());
    assert_eq!(
        serde_json::to_value(&operation.pending_writes).unwrap(),
        json!([pending_write])
    );
    assert_eq!(
        serde_json::to_value(&result.lane_state.pending_next_run).unwrap(),
        json!([next_run])
    );
}

#[test]
fn reduces_an_unfinished_step_of_every_kind() {
    let cases = [
        (
            attempt(2, "run-1", "assistant", 1, "result"),
            json!({"kind": "assistant", "attempts": 1, "resultEntryId": "result"}),
        ),
        (
            attempt_with_reason(2, "run-1", "compaction", 1, "result", Some("overflow")),
            json!({"kind": "compaction", "attempts": 1, "resultEntryId": "result", "compactionReason": "overflow"}),
        ),
        (
            attempt(2, "run-1", "branch_summary", 1, "result"),
            json!({"kind": "branch_summary", "attempts": 1, "resultEntryId": "result"}),
        ),
    ];
    for (record_value, expected) in cases {
        let result = reduce(&[run_started(1, "run-1", vec![]), record_value], &[], &[]);
        assert_eq!(
            serde_json::to_value(&result.lane_state.operation.unwrap().step).unwrap(),
            expected
        );
    }
}

#[test]
fn closes_the_newest_attempt_only_when_its_provisioned_result_exists() {
    let target = message_target(
        "result",
        assistant_message(json!([{"type": "text", "text": "done"}]), "stop"),
    );
    let result = reduce(
        &[
            run_started(1, "run-1", vec![]),
            attempt(2, "run-1", "assistant", 1, "result"),
        ],
        &[persisted_entry(target, 3, None)],
        &[],
    );
    assert!(result.lane_state.operation.unwrap().step.is_none());

    let target = message_target(
        "attempt-2-result",
        assistant_message(json!([{"type": "text", "text": "done"}]), "stop"),
    );
    let result = reduce(
        &[
            run_started(1, "run-1", vec![]),
            attempt(2, "run-1", "assistant", 1, "attempt-1-result"),
            attempt(3, "run-1", "assistant", 2, "attempt-2-result"),
        ],
        &[persisted_entry(target, 4, None)],
        &[],
    );
    assert!(result.lane_state.operation.unwrap().step.is_none());
}

#[test]
fn reduces_tool_batch_state_across_intent_and_result_boundaries() {
    let assistant = assistant_tools_entry();
    let tool_result = persisted_entry(
        message_target("tool-result-1", tool_result_message("call-1", "tool-1")),
        5,
        Some("assistant-tools"),
    );

    let cases: Vec<(Vec<Value>, Vec<Value>, bool)> = vec![
        (
            vec![
                run_started(1, "run-1", vec![]),
                attempt(2, "run-1", "assistant", 1, "assistant-tools"),
            ],
            vec![assistant.clone()],
            false,
        ),
        (
            vec![
                run_started(1, "run-1", vec![]),
                attempt(2, "run-1", "assistant", 1, "assistant-tools"),
                tool_started(4, json!({})),
            ],
            vec![assistant.clone()],
            false,
        ),
        (
            vec![
                run_started(1, "run-1", vec![]),
                attempt(2, "run-1", "assistant", 1, "assistant-tools"),
                tool_started(4, json!({})),
            ],
            vec![assistant.clone(), {
                let mut result = tool_result.clone();
                result["terminate"] = json!(true);
                result
            }],
            true,
        ),
    ];

    for (record_values, own_entries, has_result) in cases {
        let result = reduce(&record_values, &own_entries, &[]);
        let operation = result.lane_state.operation.unwrap();
        let batch = operation.tool_batch.unwrap();
        assert_eq!(batch.assistant_entry_id, "assistant-tools");
        assert!(!batch.truncated);
        assert_eq!(batch.unresolved, !has_result);
        let call = &batch.calls[0];
        assert_eq!(call.tool_index, 0);
        assert_eq!(call.tool_call.id, "call-1");
        assert_eq!(call.tool_call.name, "tool-1");
        assert_eq!(call.result_exists, has_result);
        assert_eq!(call.terminate, has_result.then_some(true));
        assert_eq!(
            call.started.is_some(),
            record_values.iter().any(|record| record["type"] == "tool_started")
        );
    }
}

#[test]
fn does_not_resolve_a_tool_batch_from_a_deferred_write_tool_result() {
    let assistant = assistant_tools_entry();
    let written_result = message_target("written-tool-result", tool_result_message("call-1", "tool-1"));
    let result = reduce(
        &[
            run_started(1, "run-1", vec![]),
            attempt(2, "run-1", "assistant", 1, "assistant-tools"),
            write_deferred(4, written_result.clone()),
        ],
        &[assistant, persisted_entry(written_result, 5, Some("assistant-tools"))],
        &[],
    );

    let batch = result.lane_state.operation.unwrap().tool_batch.unwrap();
    assert!(!batch.calls[0].result_exists);
    assert!(batch.unresolved);
}

#[test]
fn matches_blocked_results_without_tool_start_records_in_source_order() {
    let assistant = persisted_entry(
        message_target(
            "assistant-two-tools",
            assistant_message(
                json!([
                    {"type": "toolCall", "id": "call-1", "name": "tool-1", "arguments": {}},
                    {"type": "toolCall", "id": "call-2", "name": "tool-2", "arguments": {}}
                ]),
                "toolUse",
            ),
        ),
        3,
        None,
    );
    let blocked = persisted_entry(
        message_target("blocked-result", {
            let mut message = tool_result_message("call-1", "tool-1");
            message["content"] = json!([{"type": "text", "text": "blocked"}]);
            message["isError"] = json!(true);
            message
        }),
        4,
        Some("assistant-two-tools"),
    );
    let second_start = tool_started(
        5,
        json!({
            "assistantEntryId": "assistant-two-tools",
            "toolIndex": 1,
            "toolCallId": "call-2",
            "toolName": "tool-2",
            "resultEntryId": "call-2-result"
        }),
    );
    let result = reduce(
        &[
            run_started(1, "run-1", vec![]),
            attempt(2, "run-1", "assistant", 1, "assistant-two-tools"),
            second_start.clone(),
        ],
        &[assistant, blocked],
        &[],
    );

    let batch = result.lane_state.operation.unwrap().tool_batch.unwrap();
    assert_eq!(batch.calls.len(), 2);
    assert_eq!(batch.calls[0].tool_call.id, "call-1");
    assert!(batch.calls[0].result_exists);
    assert_eq!(batch.calls[1].tool_call.id, "call-2");
    assert_eq!(
        serde_json::to_value(batch.calls[1].started.as_ref()).unwrap(),
        second_start
    );
    assert!(!batch.calls[1].result_exists);
}

#[test]
fn marks_a_length_stopped_tool_batch_as_truncated_without_resolving_it() {
    let truncated = persisted_entry(
        message_target(
            "assistant-truncated",
            assistant_message(
                json!([{"type": "toolCall", "id": "call-1", "name": "tool-1", "arguments": {}}]),
                "length",
            ),
        ),
        3,
        None,
    );
    let result = reduce(
        &[
            run_started(1, "run-1", vec![]),
            attempt(2, "run-1", "assistant", 1, "assistant-truncated"),
        ],
        &[truncated],
        &[],
    );

    let batch = result.lane_state.operation.unwrap().tool_batch.unwrap();
    assert!(batch.truncated);
    assert!(batch.unresolved);
}

#[test]
fn detects_an_unredeemed_deferred_handle_only_at_the_operation_tail() {
    let deferred_message = assistant_message(json!([]), "deferred");
    let deferred_entry = persisted_entry(message_target("assistant-deferred", deferred_message.clone()), 3, None);
    let pending = reduce(
        &[
            run_started(1, "run-1", vec![]),
            attempt(2, "run-1", "assistant", 1, "assistant-deferred"),
        ],
        std::slice::from_ref(&deferred_entry),
        &[],
    );
    assert_eq!(
        serde_json::to_value(pending.lane_state.operation.unwrap().deferred).unwrap(),
        deferred_message["deferred"]
    );

    let successor = persisted_entry(
        message_target(
            "assistant-ready",
            assistant_message(json!([{"type": "text", "text": "ready"}]), "stop"),
        ),
        4,
        Some("assistant-deferred"),
    );
    let redeemed = reduce(
        &[
            run_started(1, "run-1", vec![]),
            attempt(2, "run-1", "assistant", 1, "assistant-deferred"),
        ],
        &[deferred_entry, successor],
        &[],
    );
    assert!(redeemed.lane_state.operation.unwrap().deferred.is_none());
}

#[test]
fn derives_terminal_failure_provenance() {
    let step_cases = [
        (
            vec![
                run_started(1, "run-1", vec![]),
                attempt(2, "run-1", "assistant", 1, "assistant-error"),
            ],
            vec![persisted_entry(
                message_target("assistant-error", {
                    let mut message = assistant_message(json!([]), "error");
                    message["errorMessage"] = json!("failed");
                    message
                }),
                3,
                None,
            )],
            "step",
        ),
        (
            vec![
                run_started(1, "run-1", vec![]),
                attempt(2, "run-1", "assistant", 1, "assistant-deferred"),
            ],
            vec![
                persisted_entry(
                    message_target("assistant-deferred", assistant_message(json!([]), "deferred")),
                    3,
                    None,
                ),
                persisted_entry(
                    message_target("deferred-error", {
                        let mut message = assistant_message(json!([]), "error");
                        message["errorMessage"] = json!("expired");
                        message
                    }),
                    4,
                    Some("assistant-deferred"),
                ),
            ],
            "deferred_fetch",
        ),
        (
            vec![
                run_started(1, "run-1", vec![]),
                deferred_fetch_usage(3, "deferred-error"),
            ],
            vec![persisted_entry(
                message_target("deferred-error", {
                    let mut message = assistant_message(json!([]), "error");
                    message["errorMessage"] = json!("expired");
                    message
                }),
                2,
                None,
            )],
            "deferred_fetch",
        ),
    ];
    for (record_values, own_entries, source) in step_cases {
        let result = reduce(&record_values, &own_entries, &[]);
        assert_eq!(result.terminal_failure.unwrap().source.as_str_for_test(), source);
    }
}

#[test]
fn does_not_classify_an_error_shaped_deferred_write_as_terminal_failure() {
    let target = message_target("written-error", {
        let mut message = assistant_message(json!([]), "error");
        message["errorMessage"] = json!("note");
        message
    });
    let result = reduce(
        &[run_started(1, "run-1", vec![]), write_deferred(2, target.clone())],
        &[persisted_entry(target, 3, None)],
        &[],
    );
    assert!(result.terminal_failure.is_none());
}

#[test]
fn derives_structural_target_state() {
    let cases = [
        (
            vec![compaction_started(1, "compaction-1")],
            vec![],
            json!({"result": false}),
        ),
        (
            vec![compaction_started(1, "compaction-1")],
            vec![compaction_entry("compaction-1", 2)],
            json!({"result": true}),
        ),
        (
            vec![navigation_started(1, "summary-1")],
            vec![],
            json!({"summary": false}),
        ),
        (
            vec![navigation_started(1, "summary-1")],
            vec![branch_summary_entry("summary-1", 2)],
            json!({"summary": true}),
        ),
    ];
    for (record_values, entry_values, expected) in cases {
        let result = reduce(&record_values, &entry_values, &[]);
        assert_eq!(
            serde_json::to_value(result.lane_state.operation.unwrap().targets).unwrap(),
            expected
        );
    }
}

#[test]
fn resets_the_overflow_guard_only_after_newer_conversational_input_is_consumed() {
    let initial = message_target("initial", user_message("initial"));
    let steer = message_target("steer", user_message("steer"));
    let record_values = vec![
        run_started(1, "run-1", vec![initial.clone()]),
        attempt_with_reason(3, "run-1", "compaction", 1, "overflow-summary", Some("overflow")),
        queue_enqueued(5, steer.clone(), "steer"),
    ];
    let initial_entry = persisted_entry(initial, 2, None);

    let used = reduce(&record_values, std::slice::from_ref(&initial_entry), &[]);
    assert!(used.lane_state.operation.unwrap().overflow_recovery_used);

    let reset = reduce(
        &record_values,
        &[initial_entry, persisted_entry(steer, 6, Some("initial"))],
        &[],
    );
    assert!(!reset.lane_state.operation.unwrap().overflow_recovery_used);
}

#[test]
fn is_deterministic_and_does_not_alias_its_inputs() {
    let pending = message_target("next", user_message("next"));
    let record_values = vec![queue_enqueued(1, pending, "nextRun")];
    let input = reduction_input(&record_values, &[], &[], &[], None);
    let before = serde_json::to_value(&input).unwrap();
    let first = reduce_lane_state(&input).unwrap();
    let second = reduce_lane_state(&input).unwrap();

    assert_eq!(first, second);
    assert_eq!(serde_json::to_value(&input).unwrap(), before);
    assert_eq!(first.lane_state.pending_next_run[0].id(), "next");
}

trait TerminalFailureSourceTest {
    fn as_str_for_test(&self) -> &'static str;
}

impl TerminalFailureSourceTest for pi_agent_core::harness::reducer::TerminalFailureSource {
    fn as_str_for_test(&self) -> &'static str {
        match self {
            Self::Step => "step",
            Self::DeferredFetch => "deferred_fetch",
        }
    }
}
