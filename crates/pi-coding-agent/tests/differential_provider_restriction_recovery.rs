use crate::shared_oracle as oracle;

use pi_ai::{AssistantContent, AssistantMessage, StopReason, TextContent};
use pi_coding_agent::core::provider_restriction_recovery::*;
use pi_coding_agent::core::session_control_db::SupervisorResponse;
use pi_js_compat::abort::AbortController;
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::{self, JsString, JsValue};
use std::sync::{Arc, Mutex};

fn failure() -> AssistantMessage {
    let mut message = AssistantMessage::empty("faux", "openai-codex", "model", 42.5);
    message.stop_reason = StopReason::Error;
    message.error_message = Some("policy restriction".into());
    message
        .content
        .push(AssistantContent::Text(TextContent::new("refused")));
    message
}
fn proposal() -> CodexRestrictionProposal {
    CodexRestrictionProposal {
        task: "summarize the public docs".into(),
        basis_quote: "public docs".into(),
        reason: "narrower".into(),
    }
}
fn source() -> oracle::TsOracle {
    oracle::TsOracle::spawn(
        "provider-restriction-recovery-oracle.ts",
        "packages/coding-agent/src/core/provider-restriction-recovery.ts",
    )
}
fn output(oracle: &mut oracle::TsOracle, request: &JsValue) -> JsValue {
    let response = oracle.request(request);
    assert_eq!(response["ok"], JsValue::Bool(true), "{response:?}");
    response["output"].clone()
}
#[test]
fn validation_and_prompt_match_frozen() {
    let mut oracle = source();
    let original = JsString::from("Investigate safely using public docs");
    let proposal = proposal();
    let failure = failure();
    let formatted = output(
        &mut oracle,
        &json!({"id":0,"op":"format","original":original,"failure":js_json::to_value(&failure).unwrap(),"proposal":js_json::to_value(&proposal).unwrap()}),
    );
    assert_eq!(
        JsValue::String(format_codex_restriction_validation(&original, &failure, &proposal)),
        formatted
    );
    let decisions = [
        "not json",
        "null",
        "[]",
        "{}",
        r#"{"allowed":true,"task":"summarize the public docs","reason":"safe","basisQuote":"public docs"}"#,
        r#"{"allowed":false,"task":"summarize the public docs","reason":"denied","basisQuote":""}"#,
        r#"{"allowed":true,"task":"summarize the public docs","reason":"safe","basisQuote":"missing"}"#,
        r#"{"allowed":true,"task":"other","reason":"safe","basisQuote":"public docs"}"#,
        r#"{"allowed":true,"task":"summarize the public docs","reason":" ","basisQuote":"public docs"}"#,
        r#"{"allowed":true,"task":"summarize the public docs","reason":"safe","basisQuote":"public docs","extra":1}"#,
        r#"{"allowed":1,"task":"summarize the public docs","reason":"safe","basisQuote":"public docs"}"#,
    ];
    for (i, decision) in decisions.iter().enumerate() {
        for stop in [
            StopReason::Stop,
            StopReason::Error,
            StopReason::Aborted,
            StopReason::Length,
            StopReason::ToolUse,
        ] {
            let mut message = AssistantMessage::empty("faux", "faux", "model", 0.0);
            message.stop_reason = stop;
            message
                .content
                .push(AssistantContent::Text(TextContent::new(*decision)));
            let expected = output(
                &mut oracle,
                &json!({"id":i,"op":"validate","original":original,"message":js_json::to_value(&message).unwrap(),"proposal":js_json::to_value(&proposal).unwrap()}),
            );
            assert_eq!(
                js_json::to_value(&parse_codex_restriction_validation(&message, &proposal, &original)).unwrap(),
                expected,
                "{i} {stop:?}"
            );
        }
    }
    let mut message = AssistantMessage::empty("faux", "faux", "model", 0.0);
    let decision = decisions[4];
    message.content = vec![
        AssistantContent::Text(TextContent::new(&decision[..20])),
        AssistantContent::Thinking(pi_ai::ThinkingContent::new("private")),
        AssistantContent::Text(TextContent::new(&decision[20..])),
    ];
    for error in [None, Some(JsString::default()), Some("reported error".into())] {
        message.error_message = error;
        let expected = output(
            &mut oracle,
            &json!({"id":100,"op":"validate","original":original,"message":js_json::to_value(&message).unwrap(),"proposal":js_json::to_value(&proposal).unwrap()}),
        );
        assert_eq!(
            js_json::to_value(&parse_codex_restriction_validation(&message, &proposal, &original)).unwrap(),
            expected
        );
    }
    message.error_message = None;
    message.content.push(AssistantContent::ToolCall(pi_ai::ToolCall::new(
        "call",
        "read",
        Default::default(),
    )));
    let expected = output(
        &mut oracle,
        &json!({"id":101,"op":"validate","original":original,"message":js_json::to_value(&message).unwrap(),"proposal":js_json::to_value(&proposal).unwrap()}),
    );
    assert_eq!(
        js_json::to_value(&parse_codex_restriction_validation(&message, &proposal, &original)).unwrap(),
        expected
    );
}
#[tokio::test]
async fn advisory_request_and_decisions_match_frozen() {
    let mut oracle = source();
    let answers = [
        "not json",
        "null",
        "[]",
        "{}",
        r#"{"kind":"blocked","reason":"no authorization"}"#,
        r#"{"kind":"blocked","reason":" "}"#,
        r#"{"kind":"rescope","task":"summarize public docs","basisQuote":"public docs","reason":"narrower"}"#,
        r#"{"kind":"rescope","task":"  INVESTIGATE\tpublic docs ","basisQuote":"public docs","reason":"same"}"#,
        r#"{"kind":"rescope","task":"other","basisQuote":"absent","reason":"narrower"}"#,
        r#"{"kind":"rescope","task":"other","basisQuote":"public docs","reason":"narrower","extra":true}"#,
    ];
    let mut cases: Vec<JsValue> = answers
        .iter()
        .map(|answer| json!({"response":{"kind":"advisory","answer":answer}}))
        .collect();
    cases.extend([
        json!({"response":{"kind":"error","reason":"service failed"}}),
        json!({"response":{"kind":"complete","reason":"done"}}),
        json!({"preAbort":true,"response":{"kind":"advisory","answer":answers[6]}}),
        json!({"abortDuring":true,"response":{"kind":"advisory","answer":answers[6]}}),
        json!({"throwReason":"transport failure"}),
        json!({"abortDuring":true,"throwReason":"transport failure"}),
        json!({"original":" \u{feff}","response":{"kind":"error","reason":"unused"}}),
        json!({"original":"😀".repeat(4001),"response":{"kind":"error","reason":"unused"}}),
        json!({"error":"","response":{"kind":"error","reason":"unused"}}),
        json!({"error":"\u{feff}","response":{"kind":"error","reason":"unused"}}),
        json!({"activeGoal":"","response":{"kind":"error","reason":"goal included"}}),
        json!({"activeGoal":"authorized goal","response":{"kind":"error","reason":"goal included"}}),
        json!({"original":JsString::from_utf16(vec![0xd800,0x61]),"response":{"kind":"advisory","answer":"{\"kind\":\"rescope\",\"task\":\"different\",\"basisQuote\":\"\\ud800\",\"reason\":\"native quote\"}"}}),
    ]);
    for (i, case) in cases.into_iter().enumerate() {
        let original = case
            .get("original")
            .and_then(JsValue::as_string)
            .cloned()
            .unwrap_or_else(|| "Investigate public docs".into());
        let controller = AbortController::new();
        if case["preAbort"] == JsValue::Bool(true) {
            controller.abort(None);
        }
        let calls = Arc::new(Mutex::new(Vec::new()));
        let captured = Arc::clone(&calls);
        let request_case = case.clone();
        let request_controller = controller.clone();
        let options = RequestCodexRestrictionRescopeOptions {
            requester: Arc::new(move |input| {
                let signal_forwarded = input.signal.as_ref() == Some(&request_controller.signal());
                captured.lock().unwrap().push(json!({"controlDbPath":input.control_db_path,"kind":input.kind,"payload":JsValue::Object(input.payload),"projectId":input.project_id,"senderSessionId":input.sender_session_id,"timeoutMs":input.timeout_ms,"maxAttempts":input.max_attempts,"signalForwarded":signal_forwarded}));
                let case = request_case.clone();
                let controller = request_controller.clone();
                Box::pin(async move {
                    if case["abortDuring"] == JsValue::Bool(true) {
                        controller.abort(None);
                    }
                    if let Some(reason) = case.get("throwReason").and_then(JsValue::as_string) {
                        return Err(reason.clone());
                    }
                    Ok(js_json::from_value::<SupervisorResponse>(&case["response"]).unwrap())
                })
            }),
            control_db_path: "/tmp/recovery-test.db".into(),
            sender_session_id: "session".into(),
            cwd: "/tmp".into(),
            original_user_request: original.clone(),
            failure: {
                let mut failure = failure();
                if let Some(error) = case.get("error").and_then(JsValue::as_string) {
                    failure.error_message = Some(error.clone());
                }
                failure
            },
            signal: Some(controller.signal()),
            active_goal: case.get("activeGoal").and_then(JsValue::as_string).cloned(),
        };
        let mut request = case.clone();
        request.as_object_mut().unwrap().insert("id".into(), json!(i));
        let mut encoded_options =
            json!({"originalUserRequest":original,"failure":js_json::to_value(&options.failure).unwrap()});
        if let Some(goal) = &options.active_goal {
            encoded_options["activeGoal"] = JsValue::String(goal.clone());
        }
        request
            .as_object_mut()
            .unwrap()
            .insert("options".into(), encoded_options);
        let expected = output(&mut oracle, &request);
        let result = request_codex_restriction_rescope(&options).await;
        assert_eq!(
            json!({"result":js_json::to_value(&result).unwrap(),"calls":calls.lock().unwrap().clone()}),
            expected,
            "case {i}"
        );
    }
}
