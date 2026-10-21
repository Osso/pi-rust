#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::extensions::types::ToolCallEventResult;
use pi_coding_agent::core::session_control_db::SupervisorResponse;
use pi_coding_agent::supervisor::approval_reviewer::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
#[tokio::test]
async fn approval_escalation_and_rejections_match_frozen() {
    let cases = json!([
        {"decision":{"kind":"approve","reason":"ok"},"human":true,"escalate":true},
        {"decision":{"kind":"reject","reason":"unsafe"},"human":true,"escalate":false},
        {"decision":{"kind":"reject","reason":"unsafe"},"human":true,"escalate":true},
        {"decision":{"kind":"reject","reason":"unsafe"},"human":false,"escalate":true},
        {"decision":{"kind":"error","reason":"unavailable"},"human":true,"escalate":false},
        {"decision":{"kind":"error","reason":"unavailable"},"human":false,"escalate":false},
        {"decision":{"kind":"complete","reason":"not approval"},"human":true,"escalate":false},
        {"decision":{"kind":"complete","reason":"not approval"},"human":false,"escalate":false},
        {"decision":{"kind":"error","reason":"unavailable"},"human":true,"escalate":false,"humanError":"human failure"},
        {"decision":{"kind":"approve","reason":"ok"},"human":true,"escalate":false,"requestError":"request failure"}
    ]);
    let mut ts = oracle::TsOracle::spawn(
        "q16-runtime-oracle.ts",
        "packages/coding-agent/src/supervisor/approval-reviewer.ts",
    );
    let expected = ts.request(&json!({"id":1,"operation":"approvalScenarios","cases":cases}));
    assert_eq!(expected["ok"], true, "{expected}");
    let mut actual = Vec::new();
    for case in cases.as_array().unwrap() {
        let decision: SupervisorResponse = serde_json::from_value(case["decision"].clone()).unwrap();
        let request_error = case["requestError"].as_str().map(str::to_owned);
        let request: SupervisorApprovalCall = Arc::new(move || {
            let result = match &request_error {
                Some(error) => Err(error.clone()),
                None => Ok(decision.clone()),
            };
            Box::pin(async move { result })
        });
        let log = Arc::new(Mutex::new(Vec::<String>::new()));
        let captured = log.clone();
        let human_error = case["humanError"].as_str().map(str::to_owned);
        let human: Option<SupervisorHumanReviewer> = case["human"].as_bool().unwrap().then(|| {
            Arc::new(move |reason: String| {
                captured.lock().unwrap().push(reason.clone());
                let result = match &human_error {
                    Some(error) => Err(error.clone()),
                    None => Ok(Some(ToolCallEventResult {
                        block: Some(true),
                        reason: Some(format!("human: {reason}")),
                        input: None,
                        terminate: None,
                    })),
                };
                Box::pin(async move { result })
                    as pi_ai::request::BoxFuture<'static, Result<Option<ToolCallEventResult>, String>>
            }) as SupervisorHumanReviewer
        });
        let result = review_tool_call_with_supervisor(request, human, case["escalate"].as_bool().unwrap()).await;
        actual.push(match result {Ok(value)=>json!({"value":value.map(|result|json!({"block":result.block,"reason":result.reason})),"log":log.lock().unwrap().clone()}),Err(error)=>json!({"error":error,"log":log.lock().unwrap().clone()})});
    }
    assert_eq!(Value::Array(actual), expected["output"]);
}
