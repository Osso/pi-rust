#[path = "support/repin_fixture_process.rs"]
mod fixture_process;
#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::session_control_db::*;
use serde_json::{Value, json};
fn step(ts: &mut oracle::TsOracle, op: &str, args: Value) -> Value {
    let r = ts.request(&json!({"id":1,"steps":[{"op":op,"args":args}]}));
    assert_eq!(r["output"][0]["ok"], true, "{r}");
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
fn register(db: &str, path: Option<&str>) {
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
    let db = t.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let data = input("retry");
    let id = call(&mut ts, "enqueueStoredRuntimeMailboxMessage", json!([db, data]))
        .as_i64()
        .unwrap();
    assert_eq!(
        enqueue_stored_runtime_mailbox_message(&db, &serde_json::from_value(data.clone()).unwrap()).unwrap(),
        id
    );
    let mut collision = data;
    collision["message"]["body"] = json!("changed");
    assert!(enqueue_stored_runtime_mailbox_message(&db, &serde_json::from_value(collision).unwrap()).is_err());
    assert_eq!(
        call(&mut ts, "readRuntimeMailboxMessage", json!([db, id]))["body"],
        "  hello\n\n"
    );
}
#[test]
fn take_claims_and_payload_cas_delivery_deletes_visible_peer_row() {
    let t = tempfile::tempdir().unwrap();
    let db = t.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let id = call(
        &mut ts,
        "enqueueStoredRuntimeMailboxMessage",
        json!([db, input("claim")]),
    )
    .as_i64()
    .unwrap();
    register(&db, None);
    let messages = take_runtime_mailbox_messages_for_delivery(&db, &address(), |_| true, 20).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].status, RuntimeMailboxMessageStatus::Claimed);
    let read = read_runtime_mailbox_message_for_delivery(&db, id).unwrap().unwrap();
    assert!(read.payload_valid);
    assert!(!deliver_runtime_mailbox_message(&db, id, Some("stale")).unwrap());
    assert!(read_runtime_mailbox_message(&db, id).unwrap().is_some());
    release_runtime_mailbox_message_claim(&db, id).unwrap();
    assert_eq!(
        call(&mut ts, "readRuntimeMailboxMessage", json!([db, id]))["status"],
        "pending"
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
    let db = t.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let mut data = input("remote");
    data["recipientHost"] = json!("fixture-foreign-host");
    let id = call(&mut ts, "enqueueStoredRuntimeMailboxMessage", json!([db, data]))
        .as_i64()
        .unwrap();
    register(&db, None);
    assert!(claim_runtime_mailbox_messages(&db, &address(), 20).unwrap().is_empty());
    let store_ref: RuntimeMailboxStoreRef = serde_json::from_value(input("remote")["storeRef"].clone()).unwrap();
    assert_eq!(
        consume_runtime_mailbox_message_by_store_ref(&db, &store_ref).unwrap(),
        0
    );
    assert_eq!(
        call(&mut ts, "readRuntimeMailboxMessage", json!([db, id]))["status"],
        "pending"
    );
}
#[test]
fn creation_age_not_update_age_expires_peer_rows() {
    let t = tempfile::tempdir().unwrap();
    let db = t.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let id = call(&mut ts, "enqueueStoredRuntimeMailboxMessage", json!([db, input("old")]))
        .as_i64()
        .unwrap();
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
    let db = t.path().join("control.sqlite").display().to_string();
    let transcript = t.path().join("recipient.jsonl");
    let path = transcript.display().to_string();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let data = input("accepted");
    let id = call(&mut ts, "enqueueStoredRuntimeMailboxMessage", json!([db, data]))
        .as_i64()
        .unwrap();
    let owner_identity: pi_coding_agent::core::runtime_process::ProcessIdentity = serde_json::from_str(
        step(&mut ts, "registerSelf", json!([db, address(), path]))
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        call(&mut ts, "claimRuntimeMailboxMessages", json!([db, address(), 20]))[0]["status"],
        "claimed"
    );
    std::fs::write(&transcript,format!("{}\n{}",json!({"type":"custom","customType":"runtime_mailbox_acceptance","data":{"mailboxStoreRefs":[input("accepted")["storeRef"].clone()]}}),"{partial")).unwrap();
    drop(ts); // owned fixture process exits before replacement registration/recovery
    fixture_process::wait_for_owned_fixture_exit(&owner_identity);
    register(&db, Some(&path));
    assert!(
        read_runtime_mailbox_message(&db, id).unwrap().is_none(),
        "durably accepted claim must not replay"
    );
}

#[test]
fn null_birth_defaults_once_and_unknown_status_projects_failed_not_throw() {
    let t = tempfile::tempdir().unwrap();
    let db = t.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let mut data = input("null");
    data["message"]["createdAt"] = Value::Null;
    let id = call(&mut ts, "enqueueStoredRuntimeMailboxMessage", json!([db, data]))
        .as_i64()
        .unwrap();
    assert_eq!(
        enqueue_stored_runtime_mailbox_message(&db, &serde_json::from_value(data).unwrap()).unwrap(),
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
    assert_eq!(peer["status"], "failed");
    assert_eq!(
        serde_json::to_value(read_runtime_mailbox_message(&db, id).unwrap().unwrap()).unwrap(),
        peer
    );
}

#[test]
fn frozen_source_limit_zero_discrepancy_fixture() {
    let t = tempfile::tempdir().unwrap();
    let db = t.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
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
    assert_eq!(
        taken.as_array().unwrap().len(),
        2,
        "source bug repro, not approved Rust contract"
    );
}

#[test]
fn lone_surrogate_receipt_does_not_alias_a_replacement_character_db_key() {
    let t = tempfile::tempdir().unwrap();
    let db = t.path().join("control.sqlite").display().to_string();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    let mut data = input("surrogate");
    data["storeRef"]["sessionPath"] = json!("/fixture/�/source.jsonl");
    let id = call(&mut ts, "enqueueStoredRuntimeMailboxMessage", json!([db, data]))
        .as_i64()
        .unwrap();
    let owner_identity = serde_json::from_str(
        step(&mut ts, "registerSelf", json!([db, address(), null]))
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        call(&mut ts, "claimRuntimeMailboxMessages", json!([db, address(), 20]))[0]["status"],
        "claimed"
    );
    let wire = r#"{"mailboxStoreRefs":[{"sessionPath":"/fixture/\ud800/source.jsonl","messageId":"surrogate"}]}"#;
    let path = step(&mut ts, "createReceipt", json!([t.path().join("receipt"), wire, true]));
    step(&mut ts, "registerSelf", json!([db, address(), path]));
    drop(ts);
    fixture_process::wait_for_owned_fixture_exit(&owner_identity);
    register(&db, path.as_str());
    assert_eq!(
        read_runtime_mailbox_message(&db, id).unwrap().unwrap().status,
        RuntimeMailboxMessageStatus::Pending,
        "different UTF16 identities must not falsely delete the claim"
    );
    let mut peer = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    assert_eq!(
        call(&mut peer, "readRuntimeMailboxMessage", json!([db, id]))["status"],
        "pending"
    );
}
