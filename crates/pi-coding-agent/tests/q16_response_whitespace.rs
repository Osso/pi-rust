//! Supervisor response whitespace follows ECMAScript, not Rust's Unicode trim.
#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::session_control_db::SupervisorRequestKind;
use pi_coding_agent::supervisor::service::parse_supervisor_response;
use serde_json::{Value, json};

#[test]
fn response_whitespace_matches_frozen_service() {
    let cases = json!([
        ["approval_review", "\u{feff}{\"kind\":\"approve\",\"reason\":\"ok\"}\u{feff}"],
        ["goal_idle_review", {"kind":"continue","reason":"ok","instructions":"\u{feff}"}],
        ["supervisor_advisory", {"kind":"advisory","answer":"\u{feff}"}],
        ["goal_set_review", {"kind":"set","reason":"ok","objective":"\u{feff}Ship\u{feff}"}],
        ["goal_idle_review", {"kind":"continue","reason":"ok","instructions":"\u{85}"}],
        ["approval_review", "\u{85}{\"kind\":\"approve\",\"reason\":\"ok\"}\u{85}"]
    ]);
    let mut source = oracle::TsOracle::spawn(
        "q16-runtime-oracle.ts",
        "packages/coding-agent/src/supervisor/service.ts",
    );
    let expected = source.request(&json!({
        "id": 1,
        "operation": "parseSupervisorResponse",
        "cases": cases,
    }));
    assert_eq!(expected["ok"], true, "{expected}");
    let actual = cases
        .as_array()
        .unwrap()
        .iter()
        .map(|case| {
            let kind: SupervisorRequestKind = serde_json::from_value(case[0].clone()).unwrap();
            serde_json::to_value(parse_supervisor_response(kind, &case[1])).unwrap()
        })
        .collect::<Vec<Value>>();
    assert_eq!(json!(actual), expected["output"]);
}
