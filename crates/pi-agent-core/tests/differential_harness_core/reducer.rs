use pi_agent_core::harness::reducer::{LaneReductionInput, RecordLogSlice, reduce_lane_state, validate_record_log};
use serde_json::{Value, json};

use super::support::{TsOracle, oracle_request};

#[path = "../support/reducer_fixtures.rs"]
mod fixtures;

use fixtures::*;

fn error_json(error: &pi_agent_core::harness::reducer::RecordLogCorruption) -> Value {
    json!({
        "name": "RecordLogCorruption",
        "message": error.message(),
        "reason": error.reason().as_str(),
    })
}

fn compare_corruption_cases(oracle: &mut TsOracle) {
    for (index, case) in corruption_cases().into_iter().enumerate() {
        let id = 14_500 + index as u64;
        let slice = serde_json::to_value(&case.slice).expect("slice must serialize");
        let request = oracle_request(id, "validate_record_log", json!({"slice": slice}));
        let response = oracle.request(&request);
        assert_eq!(
            response["ok"],
            json!(false),
            "oracle accepted corrupt slice: {response}"
        );
        let rust_error = validate_record_log(&case.slice).expect_err("slice must be corrupt");
        assert_eq!(rust_error.reason(), case.reason);
        assert_eq!(rust_error.message(), case.message);
        assert_eq!(response["error"], error_json(&rust_error), "corruption fixture {id}");
    }
}

struct Driver {
    records: Vec<Value>,
    entries: Vec<Value>,
    own: Vec<Value>,
    start_seq: i64,
}

impl Driver {
    fn new() -> Self {
        Self {
            records: Vec::new(),
            entries: Vec::new(),
            own: Vec::new(),
            start_seq: 0,
        }
    }

    fn record(&mut self, value: Value) {
        if value["type"] == "operation_started" {
            self.own.clear();
            self.start_seq = value["seq"].as_i64().unwrap_or_default();
        }
        if value["type"] == "operation_finished" {
            self.own.clear();
        }
        self.records.push(value);
    }

    fn entry(&mut self, value: Value) {
        self.entries.push(value.clone());
        self.own.push(value);
    }

    fn extra_entries(&self) -> Vec<Value> {
        let own_ids: std::collections::HashSet<&str> =
            self.own.iter().filter_map(|entry| entry["id"].as_str()).collect();
        self.entries
            .iter()
            .filter(|entry| !entry["id"].as_str().is_some_and(|id| own_ids.contains(id)))
            .cloned()
            .collect()
    }

    fn input(&self) -> LaneReductionInput {
        let extra_entries = self.extra_entries();
        let configuration_entries: Vec<Value> = self
            .entries
            .iter()
            .filter(|entry| entry["seq"].as_i64().unwrap_or_default() < self.start_seq && entry["type"] != "message")
            .cloned()
            .collect();
        reduction_input(
            &self.records,
            &self.own,
            &extra_entries,
            &configuration_entries,
            self.entries
                .last()
                .map(|entry| entry["id"].as_str().unwrap_or_default()),
        )
    }
}

fn scripted_actions() -> Vec<(&'static str, Value)> {
    let prompt = message_target("prompt", user_message("fix the bug"));
    let assistant_tools = message_target(
        "assistant-tools",
        assistant_message(
            json!([{"type": "toolCall", "id": "call-1", "name": "tool-1", "arguments": {}}]),
            "toolUse",
        ),
    );
    let steer = message_target("steer", user_message("steer"));
    let next_cancelled = message_target("next-cancelled", user_message("cancelled"));
    let next_pending = message_target("next-pending", user_message("pending"));
    let next_final = message_target("next-final", user_message("final"));
    let write_target = message_target("write-1", user_message("deferred write"));
    vec![
        ("record", run_started(1, "run-1", vec![prompt.clone()])),
        ("entry", persisted_entry(prompt, 2, None)),
        (
            "entry",
            json!({"type": "model_change", "id": "model-change", "parentId": "prompt", "seq": 3, "timestamp": 3, "provider": "persisted", "modelId": "persisted-model"}),
        ),
        ("record", attempt(4, "run-1", "assistant", 1, "assistant-tools")),
        ("entry", persisted_entry(assistant_tools, 5, Some("prompt"))),
        ("record", tool_started(6, json!({}))),
        ("record", usage_record(6, "assistant-tools", "toolUse", 1)),
        ("record", queue_enqueued(7, steer.clone(), "steer")),
        ("entry", persisted_entry(steer, 8, Some("assistant-tools"))),
        (
            "record",
            queue_enqueued(9, message_target("follow", user_message("follow")), "followUp"),
        ),
        ("record", abort_requested(10, "run-1")),
        (
            "entry",
            persisted_entry(
                message_target("tool-result-1", {
                    let mut message = tool_result_message("call-1", "tool-1");
                    message["content"] = json!([{"type": "text", "text": "interrupted"}]);
                    message["isError"] = json!(true);
                    message
                }),
                11,
                Some("assistant-tools"),
            ),
        ),
        ("record", attempt(12, "run-1", "assistant", 1, "assistant-final")),
        ("entry", {
            let mut entry = persisted_entry(
                message_target(
                    "assistant-final",
                    assistant_message(json!([{"type": "text", "text": "done"}]), "stop"),
                ),
                13,
                Some("tool-result-1"),
            );
            entry["terminate"] = json!(true);
            entry
        }),
        ("record", operation_finished(14, "run-1", "completed")),
        ("record", queue_enqueued(15, next_cancelled.clone(), "nextRun")),
        ("record", queue_cancelled(16, "next-cancelled", None)),
        ("record", queue_enqueued(17, next_pending.clone(), "nextRun")),
        ("record", run_started(18, "run-2", vec![next_pending])),
        ("record", attempt(19, "run-2", "assistant", 1, "assistant-deferred")),
        (
            "entry",
            persisted_entry(
                message_target("assistant-deferred", assistant_message(json!([]), "deferred")),
                20,
                Some("next-pending"),
            ),
        ),
        (
            "entry",
            persisted_entry(
                message_target(
                    "assistant-redeemed",
                    assistant_message(json!([{"type": "text", "text": "ready"}]), "stop"),
                ),
                21,
                Some("assistant-deferred"),
            ),
        ),
        ("record", {
            let mut record = write_deferred(22, write_target.clone());
            record["runId"] = json!("run-2");
            record
        }),
        ("record", attempt(23, "run-2", "assistant", 1, "assistant-error")),
        (
            "entry",
            persisted_entry(
                message_target("assistant-error", {
                    let mut message = assistant_message(json!([]), "error");
                    message["errorMessage"] = json!("failed");
                    message
                }),
                24,
                Some("assistant-redeemed"),
            ),
        ),
        ("record", {
            let mut record = deferred_fetch_usage(25, "assistant-error");
            record["runId"] = json!("run-2");
            record
        }),
        ("record", operation_finished(26, "run-2", "failed")),
        ("record", compaction_started(27, "compaction-1")),
        (
            "record",
            attempt_with_reason(28, "compact-1", "compaction", 1, "compaction-1", Some("manual")),
        ),
        ("entry", compaction_entry("compaction-1", 29)),
        ("record", operation_finished(30, "compact-1", "completed")),
        ("record", navigation_started(31, "summary-1")),
        ("entry", branch_summary_entry("summary-1", 32)),
        ("record", operation_finished(33, "navigate-1", "completed")),
        ("record", queue_enqueued(34, next_final, "nextRun")),
    ]
}

fn compare_scripted_sequence(oracle: &mut TsOracle) {
    let mut driver = Driver::new();
    let mut next_id = 14_600_u64;
    for (step, (kind, action)) in scripted_actions().into_iter().enumerate() {
        match kind {
            "record" => driver.record(action),
            "entry" => driver.entry(action),
            other => panic!("unknown action kind {other}"),
        }
        let input = driver.input();
        let input_json = serde_json::to_value(&input).expect("input must serialize");

        let slice = RecordLogSlice {
            lane: input.lane.clone(),
            open_operations: input.open_operations.clone(),
            records: input.records.clone(),
            entries: input.entries.clone(),
        };
        let validate_request = oracle_request(
            next_id,
            "validate_record_log",
            json!({"slice": serde_json::to_value(&slice).unwrap()}),
        );
        next_id += 1;
        let response = oracle.request(&validate_request);
        assert_eq!(
            response["ok"],
            json!(true),
            "step {step} oracle validation failed: {response}"
        );
        validate_record_log(&slice).unwrap_or_else(|error| panic!("step {step} Rust validation failed: {error}"));

        let reduce_request = oracle_request(next_id, "reduce_lane_state", json!({"input": input_json}));
        next_id += 1;
        let response = oracle.request(&reduce_request);
        let expected = super::support::expect_ok(&response).clone();
        let actual = reduce_lane_state(&input).expect("scripted reduction must succeed");
        let actual = serde_json::to_value(&actual).expect("reduction must serialize");
        let actual = serde_json::to_value(&actual).expect("reduction must serialize");
        assert_eq!(actual, expected, "step {step} reduction mismatch for {kind}");
    }
}

fn compare_empty_lane_wrapper() {
    let idle_input: LaneReductionInput = serde_json::from_value(json!({
        "lane": "main",
        "openOperations": [],
        "records": [],
        "entries": [],
        "leafId": null,
        "ownEntries": [],
        "configurationEntries": [],
        "defaults": defaults(),
    }))
    .expect("empty lane input");
    assert_eq!(
        reduce(&[], &[], &[]),
        reduce_lane_state(&idle_input).expect("empty lane must reduce")
    );
}

pub(crate) fn compare_all_fixtures(oracle: &mut TsOracle) {
    compare_empty_lane_wrapper();
    compare_corruption_cases(oracle);
    compare_scripted_sequence(oracle);
}
