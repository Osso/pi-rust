//! RED-intent regressions for source-written canonical JSON; unexecuted until lib compiles.
use crate::shared_oracle as oracle;

use pi_coding_agent::core::session_control_db::{update_multi_agent_agent_slot, upsert_multi_agent_mailbox_message};
use pi_js_compat::js_json::{self, JsString};
use serde_json::json;

fn source_call(source: &mut oracle::TsOracle, name: &str, arguments: &str) {
    let response = source.request(&json!({"id":1,"steps":[{"op":"callWire","args":[name,arguments]}]}));
    assert_eq!(response["output"][0]["ok"], true, "{response}");
}

fn read_agent_wire(source: &mut oracle::TsOracle, path: &str) -> String {
    let response = source.request(&json!({"id":1,"steps":[{"op":"fixtureSql","args":[path,"SELECT data FROM multi_agent_agents WHERE agent_id = ?",["child"]]}]}));
    assert_eq!(response["output"][0]["ok"], true, "{response}");
    response["output"][0]["value"][0]["data"].as_str().unwrap().to_owned()
}

fn bootstrap_raw_child(source: &mut oracle::TsOracle, path: &str) {
    let arguments = format!(
        r#"[{},"/fixture/session","child",{{"id":"child","name":"name-\ud800","parentId":"parent-\udfff","lifecycle":"running","revision":1,"updatedAt":"before","unknown":{{"\ud800":"high","\udfff":"low","�":"replacement"}}}}]"#,
        serde_json::to_string(path).unwrap()
    );
    source_call(source, "bootstrapMultiAgentAgent", &arguments);
}

#[test]
fn source_raw_agent_fields_survive_native_metadata_patch_byte_for_byte() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("control.sqlite").display().to_string();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    bootstrap_raw_child(&mut source, &path);
    let before = read_agent_wire(&mut source, &path);
    let updated = update_multi_agent_agent_slot(&path, "/fixture/session", &"child".into(), None, "after").unwrap();
    assert!(updated.is_some());
    let after = read_agent_wire(&mut source, &path);
    assert_eq!(
        after,
        before.replace(r#""updatedAt":"before""#, r#""updatedAt":"after""#)
    );
}

#[test]
fn parent_request_lookup_matches_native_parent_id_not_replacement_text() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("control.sqlite").display().to_string();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    bootstrap_raw_child(&mut source, &path);
    source_call(
        &mut source,
        "upsertMultiAgentMailboxMessage",
        &format!(
            r#"[{},"/fixture/session","source-msg",{{"id":"source-msg","fromAgentId":"child","toAgentId":"parent-\udfff","kind":"parent_request","status":"pending","body":"body-\ud800"}}]"#,
            serde_json::to_string(&path).unwrap()
        ),
    );
    let message = js_json::parse_lossless(r#"{"id":"native-msg","fromAgentId":"child","toAgentId":"parent-\udfff","kind":"parent_request","status":"pending","body":"body-\ud800"}"#).unwrap();
    upsert_multi_agent_mailbox_message(&path, "/fixture/session", JsString::from("native-msg"), &message).unwrap();
    let mut wrong = message;
    wrong["id"] = pi_js_compat::js_json!("wrong-parent");
    wrong["toAgentId"] = pi_js_compat::js_json!("parent-�");
    let error = upsert_multi_agent_mailbox_message(&path, "/fixture/session", JsString::from("wrong-parent"), &wrong)
        .unwrap_err();
    assert!(error.message().contains("Invalid parent request target"));
}
