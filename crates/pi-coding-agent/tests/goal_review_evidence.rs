#[path = "support/bundled.rs"]
mod bundled;
#[path = "support/oracle.rs"]
mod oracle;
#[path = "support/extensions.rs"]
mod support;

use pi_coding_agent::bundled_extensions::goal::goal_review_evidence::create_goal_review_evidence_controller;
use pi_coding_agent::bundled_extensions::goal::goal_state::{clear_goal, load_or_migrate_active_goal, save_goal};
use pi_coding_agent::bundled_extensions::goal::goal_types::{
    Goal, GoalReviewInput, GoalReviewKind, GoalSupervisorResponse,
};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{JsString, JsValue as Value};
use std::sync::{Arc, Mutex};

async fn run_case(steps: &Value) -> Value {
    let dir = tempfile::tempdir().unwrap();
    let cwd = dir.path().to_str().unwrap();
    let harness = bundled::Harness::new(
        Vec::new(),
        pi_coding_agent::core::extensions::create_extension_runtime(),
        bundled::HarnessOptions::new(cwd),
    )
    .await;
    let ctx = harness.context();
    let manager = ctx.session_manager().unwrap();
    let payloads = Arc::new(Mutex::new(Vec::<Value>::new()));
    let recorded = payloads.clone();
    let controller = create_goal_review_evidence_controller(Arc::new(move |input| {
        recorded.lock().unwrap().push(Value::Object(input.payload));
        Box::pin(async {
            Ok(GoalSupervisorResponse::Error {
                reason: "fixture".into(),
            })
        })
    }));
    let mut snapshots = Vec::new();
    let mut reviewed_goal: Option<Goal> = None;
    let mut count = 0;
    for step in steps.as_array().unwrap() {
        let mut review = Value::Null;
        match step["op"].as_string().unwrap().to_utf8().unwrap().as_str() {
            "goal" => {
                let goal: Option<Goal> = pi_js_compat::js_json::from_value(&step["goal"]).unwrap();
                match goal {
                    Some(goal) => save_goal(&manager, &goal).unwrap(),
                    None => {
                        clear_goal(&JsString::from(cwd), &manager).unwrap();
                    }
                }
            }
            "input" => controller
                .append_input(&pi_js_compat::js_json::from_value(&step["event"]).unwrap(), &ctx)
                .unwrap(),
            "tool" => controller
                .append_tool_result(&pi_js_compat::js_json::from_value(&step["event"]).unwrap(), &ctx)
                .unwrap(),
            "review" => {
                reviewed_goal = load_or_migrate_active_goal(&cwd.into(), &manager).unwrap();
                let reviewed = (controller.review)(GoalReviewInput {
                    kind: GoalReviewKind::GoalIdleReview,
                    ctx: ctx.clone(),
                    payload: step["payload"].as_object().cloned().unwrap_or_default(),
                })
                .await
                .unwrap();
                count = reviewed.evidence_count;
                review = json!({"decision": reviewed.decision, "evidenceCount": reviewed.evidence_count});
            }
            "consume" => controller
                .consume(
                    &ctx,
                    reviewed_goal.as_ref().unwrap(),
                    step["count"].as_f64().map(|count| count as usize).unwrap_or(count),
                )
                .unwrap(),
            _ => panic!("unknown op"),
        }
        let goal = load_or_migrate_active_goal(&cwd.into(), &manager).unwrap();
        snapshots.push(json!({"goal": goal, "review": review, "payloads": payloads.lock().unwrap().clone()}));
    }
    json!(snapshots)
}

fn input(source: &str, text: impl Into<JsString>) -> Value {
    json!({"op": "input", "event": {"source": source, "text": text.into()}})
}

fn tool(name: &str, content: Value, details: Value, is_error: bool, arguments: Value) -> Value {
    json!({"op": "tool", "event": {"toolName": name, "toolCallId": "tool", "content": content, "details": details, "isError": is_error, "input": arguments}})
}

#[tokio::test]
async fn goal_review_evidence_matches_frozen_user_requests_answers_and_consumption() {
    let goal = json!({"objective": "Fix login", "branch": "main", "createdAt": "2026-10-25T00:00:00.000Z"});
    let raw = JsString::from_utf16(vec![0xd800, 0x61, 0xdc00]);
    let answer = json!([{"type": "text", "text": "first answer"}, {"type": "image", "data": "AA==", "mimeType": "image/png"}, {"type": "text", "text": raw}]);
    let cases = vec![
        json!([
            input("interactive", "initial request before a goal"),
            {"op": "goal", "goal": goal},
            {"op": "review", "payload": {"objective": "Fix login", "userRequest": "stale"}},
            input("extension", "not user evidence"), input("interactive", ""),
            tool("ask_questions", answer.clone(), json!({"queued": true}), false, json!({})),
            tool("ask_questions", answer.clone(), json!({"cancelled": true}), false, json!({})),
            tool("ask_questions", answer.clone(), json!({}), true, json!({})),
            tool("ask_questions", answer.clone(), Value::Null, false, json!({})),
            tool("ask_questions", json!([]), json!({}), false, json!({})),
            tool("ask_questions", answer.clone(), json!({}), false, json!({})),
            tool("end_turn", json!([]), json!({}), false, json!({"reason": "  work done  "})),
            tool("end_turn", json!([]), json!({}), false, json!({"reason": "  "})),
            tool("end_turn", json!([]), json!({}), true, json!({"reason": "ignored error"})),
            tool("other", answer.clone(), json!({}), false, json!({"reason": "ignored"})),
            {"op": "review", "payload": {"objective": "Fix login", "userAnswers": ["stale"]}},
            tool("ask_questions", json!([{"type": "text", "text": "late answer"}]), json!({}), false, json!({})),
            {"op": "consume"}, {"op": "review", "payload": {}},
            input("rpc", raw), {"op": "review", "payload": {}}, {"op": "consume", "count": 0},
            {"op": "goal", "goal": {"objective": "Other goal", "branch": "main", "createdAt": "2026-10-25T01:00:00.000Z", "reviewEvidence": [{"kind": "user", "text": "kept"}]}},
            {"op": "consume"}, {"op": "review", "payload": {}}
        ]),
        json!([{"op": "goal", "goal": goal}, input("interactive", "first"), {"op": "review", "payload": {}}, {"op": "consume"}, {"op": "review", "payload": {"conversationEvents": ["kept if no evidence"]}}]),
    ];
    let mut ts = oracle::TsOracle::spawn(
        "goal-review-evidence-oracle.ts",
        "packages/coding-agent/extensions/goal/src/goal-review-evidence.ts",
    );
    let response = ts.request(&json!({"id": 1, "operation": "evidence", "cases": cases}));
    assert_eq!(response["ok"], json!(true), "oracle: {response:?}");
    for (index, steps) in cases.iter().enumerate() {
        assert_eq!(run_case(steps).await, response["output"][index], "case {index}");
    }
}
