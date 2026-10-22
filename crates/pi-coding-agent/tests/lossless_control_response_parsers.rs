//! Native response parser authority; unexecuted while the coding library is blocked.
use pi_coding_agent::core::permissions::mcp_permission_prompt::{
    PermissionPromptDecision, parse_permission_prompt_decision,
};
use pi_coding_agent::core::session_control_db::{SupervisorRequestKind, SupervisorResponse};
use pi_coding_agent::supervisor::service::parse_supervisor_response;
use pi_js_compat::js_json::{self, JsString, JsValue};

fn carriers(wire: &JsString) -> [JsValue; 3] {
    [
        js_json::parse_lossless(wire).unwrap(),
        JsValue::String(wire.clone()),
        pi_js_compat::js_json!({"content":[{"type":"text","text":wire}]}),
    ]
}

#[test]
fn supervisor_response_carriers_preserve_literal_unpaired_units() {
    let answer = JsString::from("answer-") + JsString::from_utf16(vec![0xd800, 0xdfff, 0xd801]);
    let wire = JsString::from(r#"{"kind":"advisory","answer":""#) + &answer + r#""}"#;
    for carrier in carriers(&wire) {
        assert_eq!(
            parse_supervisor_response(SupervisorRequestKind::SupervisorAdvisory, &carrier),
            Some(SupervisorResponse::Advisory { answer: answer.clone() })
        );
    }
}

#[test]
fn permission_response_carriers_preserve_raw_argument_keys_and_values() {
    let raw_key = JsString::from_utf16(vec![0xd800]);
    let raw_value = JsString::from_utf16(vec![0xdfff]);
    let wire = JsString::from(r#"{"behavior":"allow","updatedInput":{""#)
        + &raw_key
        + r#"":""#
        + &raw_value
        + r#"","�":"distinct"}}"#;
    let expected = js_json::parse_lossless(&wire).unwrap()["updatedInput"]
        .as_object()
        .unwrap()
        .clone();
    assert_eq!(expected.len(), 2);
    assert_eq!(expected.get(&raw_key), Some(&JsValue::String(raw_value)));
    for carrier in carriers(&wire) {
        assert_eq!(
            parse_permission_prompt_decision(&carrier),
            Some(PermissionPromptDecision::Allow {
                updated_input: Some(expected.clone()),
                updated_permissions: None
            })
        );
    }
}
