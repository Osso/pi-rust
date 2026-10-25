//! Differential: architect system/observation prompts against the pinned TypeScript module.

use crate::shared_oracle as oracle;
use pi_js_compat::js_json::JsString;

use pi_coding_agent::architect::observer::{
    ArchitectObservation, ArchitectObservationReason, ArchitectSessionSnapshot,
};
use pi_coding_agent::architect::prompt::{ARCHITECT_SYSTEM_PROMPT, build_architect_prompt};
use pi_coding_agent::core::session_control_db::{ArchitectRequest, ArchitectRequestStatus};
use pi_js_compat::js_json as json;
use pi_js_compat::js_json::JsValue as Value;

const SOURCE_MODULE: &str = "packages/coding-agent/src/architect/prompt.ts";

fn session(id: &str, cwd: &str, name: Option<&str>, goal: Option<&str>) -> ArchitectSessionSnapshot {
    ArchitectSessionSnapshot {
        cwd: cwd.to_owned().into(),
        goal_json: goal.map(JsString::from),
        id: id.to_owned().into(),
        is_subagent: false,
        name: name.map(JsString::from),
    }
}

fn request(id: i64) -> ArchitectRequest {
    ArchitectRequest {
        id,
        sender_session_id: "main".to_owned().into(),
        project_cwd: Some("/repo".to_owned().into()),
        body: "Architect: assess goal drift".to_owned().into(),
        status: ArchitectRequestStatus::Pending,
        created_at: "2026-07-10T00:00:00.000Z".to_owned(),
        claimed_at: None,
        claim_token: None,
        completed_at: None,
    }
}

fn observations() -> Vec<(&'static str, ArchitectObservation)> {
    vec![
        (
            "request",
            ArchitectObservation {
                requests: vec![request(4)],
                reason: ArchitectObservationReason::ArchitectRequest,
                sessions: vec![session(
                    "main-session",
                    "/repo",
                    Some("main"),
                    Some("{\"objective\":\"Keep tests green\"}"),
                )],
            },
        ),
        (
            "session-change",
            ArchitectObservation {
                requests: Vec::new(),
                reason: ArchitectObservationReason::SessionStateChanged,
                sessions: vec![
                    session("b-session", "/repo/b", None, None),
                    session("a-session", "/repo/a", Some("a"), Some("{\"objective\":\"Ship\"}")),
                ],
            },
        ),
    ]
}

#[test]
fn architect_prompts_match_the_typescript_module() {
    let observations = observations();
    let mut cases = vec![json!({"kind": "system", "name": "system"})];
    for (name, observation) in &observations {
        let reason = match observation.reason {
            ArchitectObservationReason::ArchitectRequest => "architect_request",
            ArchitectObservationReason::SessionStateChanged => "session_state_changed",
        };
        cases.push(json!({
            "kind": "observation",
            "name": name,
            "reason": reason,
            "requests": pi_js_compat::js_json::to_value(&observation.requests).expect("requests serialize"),
            "sessions": pi_js_compat::js_json::to_value(&observation.sessions).expect("sessions serialize"),
        }));
    }
    let mut ts = oracle::TsOracle::spawn("coding-agent-architect-prompt-oracle.ts", SOURCE_MODULE);
    let response = ts.request(&json!({"id": 1, "operation": "architect", "cases": cases}));
    assert_eq!(
        response["ok"],
        pi_js_compat::js_json::JsValue::Bool(true),
        "{response:?}"
    );
    let ts_results: &Vec<Value> = response["output"]["results"].as_array().expect("results");

    assert_eq!(
        JsString::from(ARCHITECT_SYSTEM_PROMPT),
        *ts_results[0]["value"].as_string().unwrap()
    );
    for (index, (name, observation)) in observations.iter().enumerate() {
        let prompt = build_architect_prompt(observation);
        assert_eq!(ts_results[index + 1]["name"], json!(*name));
        assert_eq!(
            JsString::from(prompt),
            *ts_results[index + 1]["value"].as_string().unwrap(),
            "{name}"
        );
    }
}
