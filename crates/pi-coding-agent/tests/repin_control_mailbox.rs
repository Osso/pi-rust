use pi_js_compat::js_json::JsString;
use pi_js_compat::js_json::encode_node_utf8;
#[path = "support/repin_fixture_process.rs"]
mod fixture_process;
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::{js_json, js_json as json};
fn step(ts: &mut oracle::TsOracle, op: &str, args: Value) -> Value {
    let r = ts.request(&json!({"id":1,"steps":[{"op":op,"args":args}]}));
    assert_eq!(r["output"][0]["ok"], json!(true), "{r:?}");
    r["output"][0]["value"].clone()
}
fn call(ts: &mut oracle::TsOracle, name: &str, args: Value) -> Value {
    step(ts, "call", json!([name, args]))
}
fn address() -> RuntimeMailboxAddress {
    RuntimeMailboxAddress {
        session_id: "recipient".into(),
        agent_id: None,
    }
}
fn input(id: &str) -> Value {
    json!({"recipient":address(),"sender":{"sessionId":"sender","agentId":null},"kind":"message","storeRef":{"sessionPath":"/fixture/source.jsonl","messageId":id},"message":{"id":id,"fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"  hello\n\n"}})
}
fn register(db: &str, path: Option<&JsString>) {
    register_runtime_mailbox_listener(
        db,
        &address(),
        std::process::id(),
        path,
        RuntimeMailboxRegistrationOptions::default(),
    )
    .unwrap();
}
#[test]
fn undated_enqueue_retry_after_ts_birth_is_idempotent() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let data = input("retry");
    let id = call(&mut ts, "enqueueStoredRuntimeMailboxMessage", json!([db, data]))
        .as_f64()
        .unwrap() as i64;
    assert_eq!(
        enqueue_stored_runtime_mailbox_message(&db, &js_json::from_value(&data.clone()).unwrap()).unwrap(),
        id
    );
    let mut collision = data;
    collision["message"]["body"] = json!("changed");
    assert!(enqueue_stored_runtime_mailbox_message(&db, &js_json::from_value(&collision).unwrap()).is_err());
    assert_eq!(
        call(&mut ts, "readRuntimeMailboxMessage", json!([db, id]))["body"],
        json!("  hello\n\n")
    );
}
#[test]
fn take_claims_and_payload_cas_delivery_deletes_visible_peer_row() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let id = call(
        &mut ts,
        "enqueueStoredRuntimeMailboxMessage",
        json!([db, input("claim")]),
    )
    .as_f64()
    .unwrap() as i64;
    register(&db, None);
    let messages = take_runtime_mailbox_messages_for_delivery(&db, &address(), |_| Ok(true), 20).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].status, RuntimeMailboxMessageStatus::Claimed);
    let read = read_runtime_mailbox_message_for_delivery(&db, id).unwrap().unwrap();
    assert!(read.payload_valid);
    assert!(!deliver_runtime_mailbox_message(&db, id, Some("stale")).unwrap());
    assert!(read_runtime_mailbox_message(&db, id).unwrap().is_some());
    release_runtime_mailbox_message_claim(&db, id).unwrap();
    assert_eq!(
        call(&mut ts, "readRuntimeMailboxMessage", json!([db, id]))["status"],
        json!("pending")
    );
    claim_runtime_mailbox_messages(&db, &address(), 20).unwrap();
    let read = read_runtime_mailbox_message_for_delivery(&db, id).unwrap().unwrap();
    assert!(deliver_runtime_mailbox_message(&db, id, read.payload_data.as_deref()).unwrap());
    assert!(!deliver_runtime_mailbox_message(&db, id, None).unwrap());
    assert!(call(&mut ts, "readRuntimeMailboxMessage", json!([db, id])).is_null());
}
#[test]
fn foreign_host_cannot_claim_or_consume_local_session_collision() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let mut data = input("remote");
    data["recipientHost"] = json!("fixture-foreign-host");
    let id = call(&mut ts, "enqueueStoredRuntimeMailboxMessage", json!([db, data]))
        .as_f64()
        .unwrap() as i64;
    register(&db, None);
    assert!(claim_runtime_mailbox_messages(&db, &address(), 20).unwrap().is_empty());
    let store_ref: RuntimeMailboxStoreRef = js_json::from_value(&input("remote")["storeRef"].clone()).unwrap();
    assert_eq!(
        consume_runtime_mailbox_message_by_store_ref(&db, &store_ref).unwrap(),
        0
    );
    assert_eq!(
        call(&mut ts, "readRuntimeMailboxMessage", json!([db, id]))["status"],
        json!("pending")
    );
}
#[test]
fn creation_age_not_update_age_expires_peer_rows() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let id = call(&mut ts, "enqueueStoredRuntimeMailboxMessage", json!([db, input("old")]))
        .as_f64()
        .unwrap() as i64;
    let old = pi_coding_agent::utils::iso_time::format_iso_millis(
        pi_js_compat::date::js_now_millis() as i64 - 2 * 86_400_000,
    );
    step(
        &mut ts,
        "fixtureSql",
        json!([
            db,
            "UPDATE multi_agent_mailbox_messages SET data=json_set(data,'$.createdAt',?) WHERE rowid=? RETURNING rowid",
            [old, id]
        ]),
    );
    assert!(read_runtime_mailbox_message(&db, id).unwrap().is_none());
    assert!(list_runtime_mailbox_messages(&db).unwrap().is_empty());
}
#[test]
fn dead_owned_ts_claim_with_durable_complete_record_is_deleted_not_requeued() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let transcript = t.path().join("recipient.jsonl");
    let path = transcript.to_str().expect("utf-8 test path").to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let data = input("accepted");
    let id = call(&mut ts, "enqueueStoredRuntimeMailboxMessage", json!([db, data]))
        .as_f64()
        .unwrap() as i64;
    let owner_identity: pi_coding_agent::core::runtime_process::ProcessIdentity = js_json::from_value(
        &js_json::parse_lossless(
            step(&mut ts, "registerSelf", json!([db, address(), path]))
                .as_string()
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        call(&mut ts, "claimRuntimeMailboxMessages", json!([db, address(), 20]))[0]["status"],
        json!("claimed")
    );
    std::fs::write(&transcript,format!("{}\n{}",js_json::stringify_lossless(&json!({"type":"custom","customType":"runtime_mailbox_acceptance","data":{"mailboxStoreRefs":[input("accepted")["storeRef"].clone()]}})),"{partial")).unwrap();
    drop(ts); // owned fixture process exits before replacement registration/recovery
    fixture_process::wait_for_owned_fixture_exit(&owner_identity);
    register(&db, Some(&JsString::from(&path)));
    assert!(
        read_runtime_mailbox_message(&db, id).unwrap().is_none(),
        "durably accepted claim must not replay"
    );
}

#[test]
fn null_birth_defaults_once_and_unknown_status_projects_failed_not_throw() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let mut data = input("null");
    data["message"]["createdAt"] = Value::Null;
    let id = call(&mut ts, "enqueueStoredRuntimeMailboxMessage", json!([db, data]))
        .as_f64()
        .unwrap() as i64;
    assert_eq!(
        enqueue_stored_runtime_mailbox_message(&db, &js_json::from_value(&data).unwrap()).unwrap(),
        id
    );
    step(
        &mut ts,
        "fixtureSql",
        json!([
            db,
            "UPDATE multi_agent_mailbox_messages SET data=json_set(data,'$.status','delivered','$.senderHost','fixture-peer') WHERE rowid=? RETURNING rowid",
            [id]
        ]),
    );
    let peer = call(&mut ts, "readRuntimeMailboxMessage", json!([db, id]));
    assert_eq!(peer["status"], json!("failed"));
    assert_eq!(
        js_json::to_value(&read_runtime_mailbox_message(&db, id).unwrap().unwrap()).unwrap(),
        peer
    );
}

#[test]
fn frozen_source_limit_zero_discrepancy_fixture() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    call(
        &mut ts,
        "enqueueStoredRuntimeMailboxMessage",
        json!([db, input("zero1")]),
    );
    call(
        &mut ts,
        "enqueueStoredRuntimeMailboxMessage",
        json!([db, input("zero2")]),
    );
    step(&mut ts, "registerSelf", json!([db, address(), null]));
    let taken = step(&mut ts, "takeAll", json!([db, address(), 0]));
    let source_claims: Vec<_> = taken
        .as_array()
        .unwrap()
        .iter()
        .map(|row| json!([row["storeRef"]["messageId"], row["status"]]))
        .collect();
    assert_eq!(
        source_claims,
        vec![json!(["zero1", "claimed"]), json!(["zero2", "claimed"])]
    );
    let native_db = t
        .path()
        .join("native.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    for id in ["zero1", "zero2"] {
        enqueue_stored_runtime_mailbox_message(&native_db, &js_json::from_value(&input(id)).unwrap()).unwrap();
    }
    register(&native_db, None);
    assert!(
        take_runtime_mailbox_messages_for_delivery(&native_db, &address(), |_| Ok(true), 0)
            .unwrap()
            .is_empty()
    );
    let native_rows = list_runtime_mailbox_messages(&native_db).unwrap();
    assert_eq!(native_rows.len(), 2);
    assert!(
        native_rows
            .iter()
            .all(|row| row.status == RuntimeMailboxMessageStatus::Pending)
    );
}

#[test]
fn lone_surrogate_receipt_does_not_alias_a_replacement_character_db_key() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let mut data = input("surrogate");
    data["storeRef"]["sessionPath"] = json!("/fixture/�/source.jsonl");
    let id = call(&mut ts, "enqueueStoredRuntimeMailboxMessage", json!([db, data]))
        .as_f64()
        .unwrap() as i64;
    let owner_identity = js_json::from_value(
        &js_json::parse_lossless(
            step(&mut ts, "registerSelf", json!([db, address(), null]))
                .as_string()
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        call(&mut ts, "claimRuntimeMailboxMessages", json!([db, address(), 20]))[0]["status"],
        json!("claimed")
    );
    let wire = r#"{"mailboxStoreRefs":[{"sessionPath":"/fixture/\ud800/source.jsonl","messageId":"surrogate"}]}"#;
    let path = step(&mut ts, "createReceipt", json!([t.path().join("receipt"), wire, true]));
    step(&mut ts, "registerSelf", json!([db, address(), path]));
    drop(ts);
    fixture_process::wait_for_owned_fixture_exit(&owner_identity);
    register(&db, path.as_string());
    assert_eq!(
        read_runtime_mailbox_message(&db, id).unwrap().unwrap().status,
        RuntimeMailboxMessageStatus::Pending,
        "different UTF16 identities must not falsely delete the claim"
    );
    let mut peer = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    assert_eq!(
        call(&mut peer, "readRuntimeMailboxMessage", json!([db, id]))["status"],
        json!("pending")
    );
}

#[test]
fn frozen_source_empty_host_retry_discrepancy_fixture() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let mut data = input("empty-host");
    data["recipientHost"] = json!("");
    let response=ts.request(&json!({"id":1,"steps":[{"op":"call","args":["enqueueStoredRuntimeMailboxMessage",[db,data]]},{"op":"call","args":["enqueueStoredRuntimeMailboxMessage",[db,data]]}]}));
    assert_eq!(response["output"][0]["ok"], json!(true));
    assert_eq!(response["output"][1]["ok"], json!(false));
    assert_eq!(
        response["output"][1],
        json!({"ok":false,"error":{
            "name":"Error", "message":"Runtime mailbox store reference conflicts with canonical mailbox row: /fixture/source.jsonl#empty-host"
        }})
    );
    let native_retry = enqueue_stored_runtime_mailbox_message(&db, &js_json::from_value(&data).unwrap()).unwrap();
    assert_eq!(json!(native_retry), response["output"][0]["value"]);
    step(&mut ts, "registerSelf", json!([db, address(), null]));
    assert_eq!(
        step(&mut ts, "takeAll", json!([db, address(), 1]))
            .as_array()
            .unwrap()
            .len(),
        1,
        "first empty-host enqueue routes locally; only retry is inconsistent"
    );
}

#[test]
fn cas_payload_changed_by_source_actor_rejects_old_delivery_snapshot() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let data = input("source-race");
    let id = call(&mut ts, "enqueueStoredRuntimeMailboxMessage", json!([db, data]))
        .as_f64()
        .unwrap() as i64;
    register(&db, None);
    claim_runtime_mailbox_messages(&db, &address(), 1).unwrap();
    let before = read_runtime_mailbox_message_for_delivery(&db, id).unwrap().unwrap();
    assert!(before.payload_valid);
    let old = before.payload_data.unwrap();
    let mut changed = data["message"].clone();
    changed["body"] = json!("source changed after claim");
    call(
        &mut ts,
        "upsertMultiAgentMailboxMessage",
        json!([db, "/fixture/source.jsonl", "source-race", changed]),
    );
    assert!(!deliver_runtime_mailbox_message(&db, id, Some(&old)).unwrap());
    let peer = call(&mut ts, "readRuntimeMailboxMessage", json!([db, id]));
    assert_eq!(peer["status"], json!("claimed"));
    assert_eq!(peer["body"], json!("source changed after claim"));
    let current = read_runtime_mailbox_message_for_delivery(&db, id).unwrap().unwrap();
    assert_ne!(current.payload_data.as_deref(), Some(old.as_str()));
    assert!(deliver_runtime_mailbox_message(&db, id, current.payload_data.as_deref()).unwrap());
    assert!(call(&mut ts, "readRuntimeMailboxMessage", json!([db, id])).is_null());
}

#[test]
fn foreign_claimant_fences_release_fail_and_delivery() {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let id = call(
        &mut ts,
        "enqueueStoredRuntimeMailboxMessage",
        json!([db, input("owned-by-peer")]),
    )
    .as_f64()
    .unwrap() as i64;
    step(&mut ts, "registerSelf", json!([db, address(), null]));
    call(&mut ts, "claimRuntimeMailboxMessages", json!([db, address(), 1]));
    let read = read_runtime_mailbox_message_for_delivery(&db, id).unwrap().unwrap();
    assert!(!read.payload_valid);
    release_runtime_mailbox_message_claim(&db, id).unwrap();
    fail_runtime_mailbox_message(&db, id, "wrong owner").unwrap();
    assert!(!deliver_runtime_mailbox_message(&db, id, read.payload_data.as_deref()).unwrap());
    assert_eq!(
        call(&mut ts, "readRuntimeMailboxMessage", json!([db, id]))["status"],
        json!("claimed")
    );
    call(&mut ts, "failRuntimeMailboxMessage", json!([db, id, "peer failure"]));
    assert_eq!(
        read_runtime_mailbox_message(&db, id).unwrap().unwrap().error.as_ref(),
        Some(&JsString::from("peer failure"))
    );
}

#[test]
fn malformed_complete_receipt_after_matching_ref_aborts_recovery_before_delete() {
    use std::io::Write;
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let id = call(
        &mut ts,
        "enqueueStoredRuntimeMailboxMessage",
        json!([db, input("parse-order")]),
    )
    .as_f64()
    .unwrap() as i64;
    let owner = js_json::from_value(
        &js_json::parse_lossless(
            step(&mut ts, "registerSelf", json!([db, address(), null]))
                .as_string()
                .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    call(&mut ts, "claimRuntimeMailboxMessages", json!([db, address(), 1]));
    let wire = serde_json::to_string(&json!({"mailboxStoreRefs":[input("parse-order")["storeRef"]]})).unwrap();
    let path = step(&mut ts, "createReceipt", json!([t.path().join("receipt"), wire, true]));
    step(&mut ts, "registerSelf", json!([db, address(), path]));
    std::fs::OpenOptions::new()
        .append(true)
        .open(encode_node_utf8(path.as_string().unwrap()))
        .unwrap()
        .write_all(b"{malformed\n")
        .unwrap();
    drop(ts);
    fixture_process::wait_for_owned_fixture_exit(&owner);
    let error = register_runtime_mailbox_listener(
        &db,
        &address(),
        std::process::id(),
        path.as_string(),
        Default::default(),
    )
    .unwrap_err();
    assert_eq!(
        read_runtime_mailbox_message(&db, id).unwrap().unwrap().status,
        RuntimeMailboxMessageStatus::Claimed
    );
    let mut peer = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    let expected = peer.request(&json!({"id":1,"steps":[{"op":"acceptedRefsWire","path":path}]}));
    assert_eq!(expected["output"][0]["ok"], json!(false));
    assert_eq!(
        error.message(),
        expected["output"][0]["error"]["message"].as_string().unwrap()
    );
    assert_eq!(
        call(&mut peer, "readRuntimeMailboxMessage", json!([db, id]))["status"],
        json!("claimed")
    );
}

#[test]
fn raw_null_ref_recovery_obeys_some_short_circuit_order() {
    for matching_first in [true, false] {
        let t = tempfile::tempdir().unwrap();
        let db = t
            .path()
            .join("control.sqlite")
            .to_str()
            .expect("utf-8 test path")
            .to_owned();
        let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
        let id = call(
            &mut ts,
            "enqueueStoredRuntimeMailboxMessage",
            json!([db, input("ref-order")]),
        )
        .as_f64()
        .unwrap() as i64;
        let owner = js_json::from_value(
            &js_json::parse_lossless(
                step(&mut ts, "registerSelf", json!([db, address(), null]))
                    .as_string()
                    .unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
        call(&mut ts, "claimRuntimeMailboxMessages", json!([db, address(), 1]));
        let valid = input("ref-order")["storeRef"].clone();
        let refs = if matching_first {
            json!([valid, null])
        } else {
            json!([null, valid])
        };
        let wire = serde_json::to_string(&json!({"mailboxStoreRefs":refs})).unwrap();
        let path = step(&mut ts, "createReceipt", json!([t.path().join("receipt"), wire, true]));
        step(&mut ts, "registerSelf", json!([db, address(), path]));
        drop(ts);
        fixture_process::wait_for_owned_fixture_exit(&owner);
        let recovered = register_runtime_mailbox_listener(
            &db,
            &address(),
            std::process::id(),
            path.as_string(),
            Default::default(),
        );
        if matching_first {
            recovered.unwrap();
            assert!(read_runtime_mailbox_message(&db, id).unwrap().is_none())
        } else {
            assert_eq!(
                recovered.unwrap_err().message(),
                "Cannot read properties of null (reading 'sessionPath')"
            );
            assert_eq!(
                read_runtime_mailbox_message(&db, id).unwrap().unwrap().status,
                RuntimeMailboxMessageStatus::Claimed
            );
        }
    }
}
