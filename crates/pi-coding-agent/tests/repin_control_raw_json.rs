//! Persisted source JSON is not covered by typed-input exclusions.
#[path = "support/repin_fixture_process.rs"]
mod fixture_process;
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json::{self, JsValue};
use serde_json::json;

fn source_call(source: &mut oracle::TsOracle, name: &str, arguments: &str) -> serde_json::Value {
    let response = source.request(&json!({"id":1,"steps":[{"op":"callWire","args":[name,arguments]}]}));
    assert_eq!(response["output"][0]["ok"], true, "{response}");
    response["output"][0]["value"].clone()
}

fn persisted_raw(source: &mut oracle::TsOracle, path: &str) -> js_json::JsObject {
    let response = source.request(
        &json!({"id":1,"steps":[{"op":"fixtureSql","args":[path,"SELECT data FROM multi_agent_mailbox_messages",[]]}]}),
    );
    assert_eq!(response["output"][0]["ok"], true, "{response}");
    let JsValue::Object(value) =
        js_json::parse_lossless(response["output"][0]["value"][0]["data"].as_str().unwrap()).unwrap()
    else {
        panic!("canonical object")
    };
    value
}

#[test]
fn canonical_status_mutations_preserve_source_utf16_body_and_distinct_unknown_keys() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("control.sqlite").display().to_string();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let wire = format!(
        r#"[{},{{"recipient":{{"sessionId":"to","agentId":null}},"sender":{{"sessionId":"from","agentId":null}},"kind":"message","storeRef":{{"sessionPath":"/fixture/s","messageId":"m"}},"message":{{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"before\ud800after","unknown":{{"\ud800":"high","\udfff":"low","�":"replacement","array":["\ud800",{{"\udfff":"\ud800"}}]}}}}}}]"#,
        serde_json::to_string(&path).unwrap()
    );
    let id = source_call(&mut source, "enqueueStoredRuntimeMailboxMessage", &wire)
        .as_str()
        .unwrap()
        .parse::<i64>()
        .unwrap();
    let original = persisted_raw(&mut source, &path);
    let address = RuntimeMailboxAddress {
        session_id: "to".into(),
        agent_id: None,
    };
    register_runtime_mailbox_listener(&path, &address, std::process::id(), None, Default::default()).unwrap();
    for transition in ["claim", "release", "claim", "fail"] {
        match transition {
            "claim" => {
                assert_eq!(claim_runtime_mailbox_messages(&path, &address, 1).unwrap().len(), 1);
            }
            "release" => {
                release_runtime_mailbox_message_claim(&path, id).unwrap();
            }
            "fail" => {
                fail_runtime_mailbox_message(&path, id, "fixture failure").unwrap();
            }
            _ => unreachable!(),
        }
        let updated = persisted_raw(&mut source, &path);
        for key in ["body", "unknown"] {
            let key = js_json::JsString::from(key);
            assert_eq!(
                updated.get(&key),
                original.get(&key),
                "{transition} must preserve exact raw field {key:?}"
            );
        }
    }
}

#[test]
fn source_sql_bind_keys_and_raw_receipts_have_distinct_utf16_identity_on_recovery() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("control.sqlite").display().to_string();
    let mut source = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let wire = format!(
        r#"[{},{{"recipient":{{"sessionId":"to","agentId":null}},"sender":{{"sessionId":"from","agentId":null}},"kind":"message","storeRef":{{"sessionPath":"/fixture/\ud800/s","messageId":"\udfff"}},"message":{{"id":"\udfff","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"raw\ud800","unknown":{{"\ud800":"high","�":"replacement"}}}}}}]"#,
        serde_json::to_string(&path).unwrap()
    );
    source_call(&mut source, "enqueueStoredRuntimeMailboxMessage", &wire);
    let original = persisted_raw(&mut source, &path);
    let rows = source.request(&json!({"id":1,"steps":[{"op":"fixtureSql","args":[path,"SELECT session_path,message_id FROM multi_agent_mailbox_messages",[]]}]}));
    assert_eq!(rows["output"][0]["ok"], true, "{rows}");
    assert_eq!(
        rows["output"][0]["value"],
        json!([{"session_path":"/fixture/�/s","message_id":"�"}]),
        "actual frozen SQLite binding replaces lone surrogates only at TEXT bind boundary"
    );
    assert_eq!(
        original.get(&js_json::JsString::from("id")),
        Some(&JsValue::String(js_json::JsString::from_utf16(vec![0xdfff])))
    );
    let receipt_wire = r#"{"mailboxStoreRefs":[{"sessionPath":"/fixture/\ud800/s","messageId":"\udfff"}]}"#;
    let created = source.request(
        &json!({"id":1,"steps":[{"op":"createReceipt","args":[temporary.path().join("receipts"),receipt_wire,true]}]}),
    );
    assert_eq!(created["output"][0]["ok"], true, "{created}");
    let transcript = created["output"][0]["value"].as_str().unwrap();
    let address = RuntimeMailboxAddress {
        session_id: "to".into(),
        agent_id: None,
    };
    let registered = source.request(&json!({"id":1,"steps":[{"op":"registerSelf","args":[path,address,transcript]}]}));
    assert_eq!(registered["output"][0]["ok"], true, "{registered}");
    let identity = serde_json::from_str(registered["output"][0]["value"].as_str().unwrap()).unwrap();
    source_call(
        &mut source,
        "claimRuntimeMailboxMessages",
        &serde_json::to_string(&json!([path, address, 1])).unwrap(),
    );
    drop(source);
    fixture_process::wait_for_owned_fixture_exit(&identity);
    register_runtime_mailbox_listener(
        &path,
        &address,
        std::process::id(),
        Some(transcript),
        Default::default(),
    )
    .unwrap();
    let mut observer = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let recovered = persisted_raw(&mut observer, &path);
    assert_eq!(
        recovered.get(&js_json::JsString::from("status")),
        Some(&JsValue::String(js_json::JsString::from("pending"))),
        "raw receipt does not strictly equal SQL-replaced row keys; must requeue, not delete"
    );
    for key in ["id", "body", "unknown"] {
        let key = js_json::JsString::from(key);
        assert_eq!(
            recovered.get(&key),
            original.get(&key),
            "recovery must preserve raw identity {key:?}"
        );
    }
}

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
