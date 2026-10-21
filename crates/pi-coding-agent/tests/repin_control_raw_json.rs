//! Persisted source JSON is not covered by typed-input exclusions.
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json::{self, JsValue};
use serde_json::json;
#[test]
fn canonical_mailbox_source_surrogate_body_survives_native_projection_and_claim() {
    let t = tempfile::tempdir().unwrap();
    let db = t.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let wire = format!(
        r#"[{},{{"recipient":{{"sessionId":"to","agentId":null}},"sender":{{"sessionId":"from","agentId":null}},"kind":"message","storeRef":{{"sessionPath":"/fixture/s","messageId":"m"}},"message":{{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"before\ud800after"}}}}]"#,
        serde_json::to_string(&db).unwrap()
    );
    let made =
        ts.request(&json!({"id":1,"steps":[{"op":"callWire","args":["enqueueStoredRuntimeMailboxMessage",wire]}]}));
    assert_eq!(made["output"][0]["ok"], true, "{made}");
    let id = made["output"][0]["value"].as_str().unwrap().parse::<i64>().unwrap();
    let args = serde_json::to_string(&json!([db, id])).unwrap();
    let peer = ts.request(&json!({"id":1,"steps":[{"op":"callWire","args":["readRuntimeMailboxMessage",args]}]}));
    assert_eq!(peer["output"][0]["ok"], true, "{peer}");
    let expected = js_json::parse_lossless(peer["output"][0]["value"].as_str().unwrap()).unwrap();
    let native = read_runtime_mailbox_message(&db, id).unwrap().unwrap();
    let native = JsValue::from(&serde_json::to_value(&native).unwrap());
    register_runtime_mailbox_listener(
        &db,
        &RuntimeMailboxAddress {
            session_id: "to".into(),
            agent_id: None,
        },
        std::process::id(),
        None,
        Default::default(),
    )
    .unwrap();
    claim_runtime_mailbox_messages(
        &db,
        &RuntimeMailboxAddress {
            session_id: "to".into(),
            agent_id: None,
        },
        1,
    )
    .unwrap();
    let peer=ts.request(&json!({"id":1,"steps":[{"op":"callWire","args":["readRuntimeMailboxMessage",serde_json::to_string(&json!([db,id])).unwrap()]}]}));
    let after = js_json::parse_lossless(peer["output"][0]["value"].as_str().unwrap()).unwrap();
    let (JsValue::Object(expected), JsValue::Object(native), JsValue::Object(after)) = (expected, native, after) else {
        panic!("object projection")
    };
    let key = js_json::JsString::from("body");
    assert_eq!(
        (native.get(&key), after.get(&key)),
        (expected.get(&key), expected.get(&key)),
        "neither projection nor claim may replace original UTF16 code units"
    );
}
