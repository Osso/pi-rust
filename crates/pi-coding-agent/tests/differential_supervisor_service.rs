//! Differential: supervisor prompt building and response parsing against the pinned TypeScript module.

use pi_js_compat::js_json::JsString;
#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::session_control_db::{
    SupervisorRequest, SupervisorRequestKind, SupervisorRequestStatus, SupervisorResponse,
};
use pi_coding_agent::supervisor::service::{build_supervisor_prompt, parse_supervisor_response};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;

const SOURCE_MODULE: &str = "packages/coding-agent/src/supervisor/service.ts";

fn request(kind: &str, payload: Value, project_id: &str, deadline_at: &str, id: i64) -> (SupervisorRequestKind, Value) {
    let request_kind = match kind {
        "approval_review" => SupervisorRequestKind::ApprovalReview,
        "goal_set_review" => SupervisorRequestKind::GoalSetReview,
        "supervisor_advisory" => SupervisorRequestKind::SupervisorAdvisory,
        "goal_completion_review" => SupervisorRequestKind::GoalCompletionReview,
        _ => SupervisorRequestKind::GoalIdleReview,
    };
    let request = SupervisorRequest {
        id,
        sender_session_id: "main".to_owned().into(),
        project_id: project_id.to_owned().into(),
        kind: request_kind,
        payload: payload.as_object().cloned().unwrap_or_default(),
        deadline_at: deadline_at.to_owned(),
        status: SupervisorRequestStatus::Claimed,
        created_at: "2026-07-14T12:00:00.000Z".to_owned(),
        claimed_at: Some("2026-07-14T12:00:00.000Z".to_owned()),
        claim_token: Some("runtime".to_owned().into()),
        completed_at: None,
        response: None,
    };
    (
        request_kind,
        pi_js_compat::js_json::to_value(&request).expect("request serializes"),
    )
}

#[test]
fn prompts_and_parsed_responses_match_the_typescript_module() {
    let prompt_cases = vec![
        (
            "approval",
            request(
                "approval_review",
                json!({"toolName": "read"}),
                "pi",
                "2026-07-14T12:00:30.000Z",
                1,
            ),
        ),
        (
            "goal-set",
            request(
                "goal_set_review",
                json!({"currentObjective": "ship the feature", "proposedObjective": "write one test"}),
                "pi",
                "2026-08-01T12:01:00.000Z",
                3,
            ),
        ),
        (
            "goal-idle",
            request(
                "goal_idle_review",
                json!({"objective": "Fix every unresolved Sentry issue", "terminalTurn": []}),
                "pi",
                "2026-07-17T12:03:00.000Z",
                2,
            ),
        ),
        (
            "advisory",
            request(
                "supervisor_advisory",
                json!({"context": "Only the scoped diff", "question": "Is anything missing?"}),
                "other-project",
                "2026-07-28T23:03:00.000Z",
                4,
            ),
        ),
        (
            "completion",
            request(
                "goal_completion_review",
                json!({"objective": "finish"}),
                "pi",
                "2026-07-17T12:03:00.000Z",
                5,
            ),
        ),
    ];
    let parse_cases: Vec<(&str, &str, Value)> = vec![
        (
            "approve-string",
            "approval_review",
            json!("{\"kind\":\"approve\",\"reason\":\"bounded\"}"),
        ),
        (
            "reject-object",
            "approval_review",
            json!({"kind": "reject", "reason": "unsafe"}),
        ),
        (
            "content-text",
            "goal_completion_review",
            json!({"content": [{"type": "text", "text": "{\"kind\":\"complete\",\"reason\":\"done\"}"}]}),
        ),
        (
            "empty-first-text-falls-through",
            "approval_review",
            json!({"kind":"approve","reason":"ok","content":[{"type":"text","text":""},{"type":"text","text":"invalid"}]}),
        ),
        (
            "nonempty-invalid-text-does-not-fall-through",
            "approval_review",
            json!({"kind":"approve","reason":"ok","content":[{"type":"text","text":"invalid"}]}),
        ),
        (
            "wrong-kind",
            "goal_set_review",
            json!({"kind": "continue", "reason": "wrong kind"}),
        ),
        (
            "approval-complete",
            "approval_review",
            json!({"kind": "complete", "reason": "done"}),
        ),
        (
            "continue-valid",
            "goal_idle_review",
            json!({"kind": "continue", "reason": "recovered", "instructions": "Continue the goal."}),
        ),
        (
            "continue-blank",
            "goal_idle_review",
            json!({"kind": "continue", "reason": "recovered", "instructions": "  "}),
        ),
        (
            "pause",
            "goal_idle_review",
            json!({"kind": "pause", "reason": "waiting for user input"}),
        ),
        (
            "error",
            "goal_idle_review",
            json!({"kind": "error", "reason": "service unavailable"}),
        ),
        (
            "advisory",
            "supervisor_advisory",
            json!({"kind": "advisory", "answer": "Nothing is missing."}),
        ),
        (
            "advisory-blank",
            "supervisor_advisory",
            json!({"kind": "advisory", "answer": "  "}),
        ),
        (
            "set",
            "goal_set_review",
            json!({"kind": "set", "reason": "additive", "objective": "add tests"}),
        ),
        (
            "set-blank",
            "goal_set_review",
            json!({"kind": "set", "reason": "x", "objective": " "}),
        ),
        ("not-json", "goal_idle_review", json!("not json")),
        ("null", "goal_idle_review", json!(null)),
    ];

    let mut cases = Vec::new();
    for (name, (_, request_value)) in &prompt_cases {
        cases.push(json!({"kind": "prompt", "name": name, "request": request_value}));
    }
    for (name, kind, response) in &parse_cases {
        cases.push(json!({"kind": "parse", "name": name, "requestKind": kind, "response": response}));
    }
    let mut ts = oracle::TsOracle::spawn("coding-agent-supervisor-service-oracle.ts", SOURCE_MODULE);
    let response = ts.request(&json!({"id": 1, "operation": "supervisor", "cases": cases}));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );
    let ts_results = response["output"]["results"].as_array().expect("results");

    let mut index = 0;
    for (name, (kind, request_value)) in &prompt_cases {
        let rust_request: SupervisorRequest =
            pi_js_compat::js_json::from_value(&request_value.clone()).expect("request deserializes");
        let prompt = build_supervisor_prompt(&rust_request);
        assert_eq!(ts_results[index]["name"], json!(*name));
        assert_eq!(
            prompt,
            *ts_results[index]["value"].as_string().unwrap(),
            "prompt {name}"
        );
        let _ = kind;
        index += 1;
    }
    for (name, kind, raw) in &parse_cases {
        let request_kind = match *kind {
            "approval_review" => SupervisorRequestKind::ApprovalReview,
            "goal_set_review" => SupervisorRequestKind::GoalSetReview,
            "supervisor_advisory" => SupervisorRequestKind::SupervisorAdvisory,
            "goal_completion_review" => SupervisorRequestKind::GoalCompletionReview,
            _ => SupervisorRequestKind::GoalIdleReview,
        };
        let parsed = parse_supervisor_response(request_kind, raw)
            .map(|response| serde_json::to_string(&response).expect("response serializes"))
            .unwrap_or_else(|| "null".to_owned());
        assert_eq!(ts_results[index]["name"], json!(*name));
        assert_eq!(
            JsString::from(parsed),
            *ts_results[index]["value"].as_string().unwrap(),
            "parse {name}"
        );
        index += 1;
    }
    assert_eq!(index, ts_results.len());
}

#[test]
fn instruction_review_exports_match_frozen_with_native_text() {
    use pi_coding_agent::supervisor::service::{
        SUPERVISOR_INSTRUCTION_REJECTION_REASONS, build_supervisor_instruction_review_context,
        parse_supervisor_instruction_review_decision, supervisor_instruction_rejection_feedback,
    };
    let inputs = [
        json!("ACCEPT"),
        json!("REJECT_TASK_ASSIGNMENT"),
        json!("REJECT_IMPLEMENTATION_PRESCRIPTION"),
        json!("REJECT_SEQUENCING_INSTRUCTION"),
        json!("REJECT_AGENT_OR_TOOL_DIRECTION"),
        json!("REJECT_PLAN_OVERRIDE"),
        json!("ACCEPT\n"),
        json!("accept"),
        json!(""),
        json!(null),
        json!(1),
        json!({"kind":"ACCEPT"}),
    ];
    let mut cases = Vec::new();
    let mut actual = Vec::new();
    for (index, input) in inputs.iter().enumerate() {
        let name = format!("decision-{index}");
        cases.push(json!({"kind":"instruction","name":name,"response":input}));
        actual.push(json!({"name":name,"value":parse_supervisor_instruction_review_decision(input)}));
    }
    for (index, reason) in SUPERVISOR_INSTRUCTION_REJECTION_REASONS.iter().enumerate() {
        let name = format!("feedback-{index}");
        cases.push(json!({"kind":"feedback","name":name,"reason":reason}));
        actual.push(json!({"name":name,"value":supervisor_instruction_rejection_feedback(*reason)}));
    }
    let instructions = JsString::from("Observe\n") + JsString::from_utf16(vec![0xd800]);
    cases.push(json!({"kind":"instructionContext","name":"context","instructions":instructions,"timestamp":1234.5}));
    actual.push(json!({"name":"context","value":build_supervisor_instruction_review_context(&instructions,1234.5)}));
    let mut ts = oracle::TsOracle::spawn("coding-agent-supervisor-service-oracle.ts", SOURCE_MODULE);
    let expected = ts.request(&json!({"id":2,"operation":"supervisor","cases":cases}));
    assert_eq!(expected["ok"], json!(true));
    assert_eq!(json!(actual), expected["output"]["results"]);
}

#[test]
fn response_serialization_is_stable() {
    assert_eq!(
        pi_js_compat::js_json::to_value(&SupervisorResponse::Continue {
            instructions: "Continue the goal.".to_owned().into(),
            reason: "recovered".to_owned().into(),
        })
        .unwrap(),
        json!({"kind": "continue", "reason": "recovered", "instructions": "Continue the goal."})
    );
    assert_eq!(
        serde_json::to_string(&SupervisorResponse::Continue {
            instructions: "Continue the goal.".to_owned().into(),
            reason: "recovered".to_owned().into(),
        })
        .unwrap(),
        r#"{"instructions":"Continue the goal.","kind":"continue","reason":"recovered"}"#
    );
    assert_eq!(
        serde_json::to_string(&SupervisorResponse::Advisory {
            answer: "Nothing is missing.".to_owned().into(),
        })
        .unwrap(),
        r#"{"answer":"Nothing is missing.","kind":"advisory"}"#
    );
}
