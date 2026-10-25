//! RED intent for the separately parked canonical claim serialization order.
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_control_db::{
    RuntimeMailboxAddress, claim_runtime_mailbox_messages, register_runtime_mailbox_listener,
};
use pi_js_compat::js_json::{self, JsString, JsValue};

fn source_call(source: &mut oracle::TsOracle, operation: &str, args: &JsValue) -> JsValue {
    let response = source.request(
        &serde_json::json!({"id":1,"steps":[{"op":"callWire","args":[operation,js_json::stringify_lossless(args)]}]}),
    );
    assert_eq!(response["output"][0]["ok"], true, "{response}");
    js_json::parse_lossless(response["output"][0]["value"].as_str().expect("callWire JSON")).unwrap()
}

fn normalize_claim_wire(wire: &str) -> String {
    let JsValue::Object(mut object) = js_json::parse_lossless(wire).unwrap() else {
        panic!("canonical object");
    };
    for key in ["claimedAt", "claimantProcessIdentity", "updatedAt"] {
        assert!(object.contains_key(key));
        object.insert(key.into(), JsValue::String("volatile".into()));
    }
    js_json::stringify_lossless(&JsValue::Object(object))
}

#[test]
fn actual_source_and_native_claim_serialize_missing_delivery_fields_in_the_same_order() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("control.sqlite");
    let db: JsString = path.to_str().expect("utf-8 test path").into();
    let recipient = RuntimeMailboxAddress {
        session_id: "recipient".into(),
        agent_id: None,
    };
    let mut source = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let message = js_json::parse_lossless(r#"{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"body-\ud800","unknown":{"key-\udfff":"value-\ud801"}}"#).unwrap();
    source_call(
        &mut source,
        "enqueueStoredRuntimeMailboxMessage",
        &pi_js_compat::js_json!([db,{"recipient":recipient,"sender":{"sessionId":"sender","agentId":null},"kind":"message","storeRef":{"sessionPath":"/fixture/session","messageId":"m"},"message":message}]),
    );
    let connection = rusqlite::Connection::open(&path).unwrap();
    let (id, original, updated_at): (i64, String, String) = connection
        .query_row(
            "SELECT rowid,data,updated_at FROM multi_agent_mailbox_messages WHERE message_id='m'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    let identity = source_call(&mut source, "getRuntimeProcessInstanceId", &pi_js_compat::js_json!([]));
    let process = js_json::parse_lossless(identity.as_string().unwrap()).unwrap();
    let source_pid = process["pid"].as_u64().unwrap();
    source_call(
        &mut source,
        "registerRuntimeMailboxListener",
        &pi_js_compat::js_json!([db, recipient, source_pid]),
    );
    let claimed = source_call(
        &mut source,
        "claimRuntimeMailboxMessages",
        &pi_js_compat::js_json!([db, recipient, 1]),
    );
    assert_eq!(claimed.as_array().unwrap().len(), 1);
    let source_wire: String = connection
        .query_row(
            "SELECT data FROM multi_agent_mailbox_messages WHERE rowid=?",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        source_call(
            &mut source,
            "retireRuntimeMailboxListener",
            &pi_js_compat::js_json!([db, recipient, source_pid])
        ),
        pi_js_compat::js_json!(true)
    );
    connection
        .execute(
            "UPDATE multi_agent_mailbox_messages SET data=?,updated_at=? WHERE rowid=?",
            rusqlite::params![original, updated_at, id],
        )
        .unwrap();
    register_runtime_mailbox_listener(&db, &recipient, std::process::id(), None, Default::default()).unwrap();
    assert_eq!(claim_runtime_mailbox_messages(&db, &recipient, 1).unwrap().len(), 1);
    let native_wire: String = connection
        .query_row(
            "SELECT data FROM multi_agent_mailbox_messages WHERE rowid=?",
            [id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(normalize_claim_wire(&native_wire), normalize_claim_wire(&source_wire));
}
