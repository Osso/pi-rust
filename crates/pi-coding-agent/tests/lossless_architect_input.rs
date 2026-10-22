//! Native architect tool DTOs; unexecuted until coding-library compilation succeeds.
use pi_coding_agent::core::tools::ask_architect::{AskArchitectToolDetails, AskArchitectToolInput};
use pi_js_compat::js_json::{self, JsString};

#[test]
fn architect_ingress_and_details_keep_raw_units_before_sql() {
    let input = js_json::parse_lossless(r#"{"message":"body-\ud800","project_path":"/project-\udfff"}"#).unwrap();
    let params: AskArchitectToolInput = js_json::from_value(&input).unwrap();
    assert_eq!(params.message, input["message"].as_string().unwrap().clone());
    assert_eq!(params.project_path.as_ref(), input["project_path"].as_string());
    let sender = JsString::from("sender-") + JsString::from_utf16(vec![0xd800]);
    let details = AskArchitectToolDetails {
        request_id: 7,
        sender_session_id: sender.clone(),
    };
    let wire = js_json::to_value(&details).unwrap();
    assert_eq!(wire["senderSessionId"].as_string(), Some(&sender));
}
