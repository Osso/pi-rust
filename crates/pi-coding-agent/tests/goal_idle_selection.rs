#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::bundled_extensions::goal::goal_idle_selection::{GoalIdleSelection, select_goal_for_idle_review};
use pi_coding_agent::bundled_extensions::goal::goal_types::Goal;
use pi_coding_agent::core::extensions::types::AgentEndEvent;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;
use std::cell::RefCell;

#[test]
fn goal_idle_selection_matches_frozen_pending_and_terminal_order() {
    let goal = json!({"objective": "Fix login", "branch": "main", "createdAt": "2026-10-25T00:00:00.000Z"});
    let mut cases = Vec::new();
    for pending in [false, true] {
        for (stop, text) in [("stop", "answer"), ("stop", ""), ("error", ""), ("aborted", "")] {
            cases.push(json!({"goal": goal, "pending": pending, "stopReason": stop, "text": text}));
        }
    }
    cases.push(json!({"goal": null, "pending": true, "stopReason": "aborted", "text": ""}));
    cases.push(json!({"goal": goal, "pending": false, "stopReason": "stop", "text": "", "noAssistant": true}));
    let mut ts = oracle::TsOracle::spawn(
        "goal-idle-selection-oracle.ts",
        "packages/coding-agent/extensions/goal/src/goal-idle-selection.ts",
    );
    let response = ts.request(&json!({"id": 1, "operation": "select", "cases": cases}));
    assert_eq!(response["ok"], json!(true), "oracle: {response:?}");
    for (index, case) in cases.iter().enumerate() {
        let goal: Option<Goal> = pi_js_compat::js_json::from_value(&case["goal"]).unwrap();
        let mut message = pi_ai::providers::faux::faux_assistant_message(
            vec![pi_ai::types::AssistantContent::Text(pi_ai::types::TextContent::new(
                case["text"].as_string().unwrap().clone(),
            ))],
            Default::default(),
        );
        message.stop_reason = match case["stopReason"].as_string().unwrap().to_utf8().unwrap().as_str() {
            "error" => pi_ai::types::StopReason::Error,
            "aborted" => pi_ai::types::StopReason::Aborted,
            _ => pi_ai::types::StopReason::Stop,
        };
        let event = AgentEndEvent {
            messages: if case["noAssistant"].as_bool() == Some(true) {
                Vec::new()
            } else {
                vec![pi_agent_core::agent::AgentMessage::Assistant(Box::new(message))]
            },
            session_continuation: None,
        };
        let calls = RefCell::new(Vec::<Value>::new());
        let pending = || {
            calls.borrow_mut().push(json!(["pending"]));
            case["pending"].as_bool().unwrap()
        };
        let result = select_goal_for_idle_review(GoalIdleSelection {
            event: &event,
            has_pending_messages: &pending,
            select_goal: || {
                calls.borrow_mut().push(json!(["select"]));
                Ok(goal)
            },
            clear_retry: || {
                calls.borrow_mut().push(json!(["clear"]));
            },
            schedule_retry: |goal| {
                calls.borrow_mut().push(json!(["retry", goal]));
            },
            schedule_error_status: |message: &str| {
                calls.borrow_mut().push(json!(["error", message]));
                Ok(())
            },
            report_skipped: |message: &str| {
                calls.borrow_mut().push(json!(["report", message]));
            },
        })
        .expect("selection");
        assert_eq!(
            json!({"result": result, "calls": calls.into_inner()}),
            response["output"][index],
            "case {index}"
        );
    }
}
