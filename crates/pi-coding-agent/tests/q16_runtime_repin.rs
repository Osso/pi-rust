#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::cli::args::Args;
use pi_coding_agent::core::self_restart::{
    SelfRestartHandoff, apply_self_restart_request, consume_self_restart_request,
};
use pi_coding_agent::core::session_control_db::SupervisorRequestKind;
use pi_coding_agent::supervisor::service::parse_supervisor_response;
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[test]
fn empty_content_does_not_hide_a_structured_supervisor_response() {
    let cases = json!([
        ["approval_review", {"kind":"approve","reason":"ok","content":[{"type":"text","text":""}]}],
        ["approval_review", {"kind":"approve","reason":"ok","content":[{"type":"text","text":" "}]}]
    ]);
    let mut ts = oracle::TsOracle::spawn(
        "q16-runtime-oracle.ts",
        "packages/coding-agent/src/supervisor/service.ts",
    );
    let response = ts.request(&json!({"id":1,"operation":"parseSupervisorResponse","cases":cases}));
    assert_eq!(response["ok"], true, "{response}");
    let actual: Vec<Value> = cases
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            serde_json::to_value(parse_supervisor_response(
                SupervisorRequestKind::ApprovalReview,
                &case[1],
            ))
            .unwrap()
        })
        .collect();
    assert_eq!(json!(actual), response["output"]);
}

#[test]
fn restart_rejects_empty_session_and_restores_saved_model_selection() {
    let mut ts = oracle::TsOracle::spawn(
        "q16-runtime-oracle.ts",
        "packages/coding-agent/src/core/self-restart.ts",
    );
    let env = json!({"PI_SELF_RESTART_REQUEST":"1", "PI_SELF_RESTART_SESSION":""});
    let response = ts.request(&json!({"id":2,"operation":"consumeSelfRestartRequest","cases":[[env,100,42]]}));
    assert_eq!(response["ok"], true, "{response}");
    let mut env: BTreeMap<String, String> = serde_json::from_value(env).unwrap();
    assert_eq!(
        serde_json::to_value(consume_self_restart_request(&mut env, 100, 42).map(|value| value.session_file)).unwrap(),
        response["output"][0]
    );
}

#[test]
fn restart_clears_model_overrides() {
    let mut args = Args {
        provider: Some("stale".into()),
        model: Some("old-model".into()),
        thinking: Some(pi_agent_core::harness::ThinkingLevel::High),
        ..Default::default()
    };
    apply_self_restart_request(
        &mut args,
        Some(&SelfRestartHandoff {
            session_file: "saved.jsonl".into(),
            prompt: None,
            old_pid: None,
        }),
    );
    assert_eq!((args.provider, args.model, args.thinking), (None, None, None));
}
