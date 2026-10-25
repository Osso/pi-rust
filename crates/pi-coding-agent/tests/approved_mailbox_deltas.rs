#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json::JsValue as Value;
use pi_js_compat::{js_json, js_json as json};

fn input(id: &str) -> EnqueueStoredRuntimeMailboxMessageInput {
    js_json::from_value(&json!({
        "recipient":{"sessionId":"recipient"},"sender":{"sessionId":"sender"},
        "kind":"message","storeRef":{"sessionPath":"/fixture/sender.jsonl","messageId":id},
        "message":{"id":id,"body":"hello","kind":"message","status":"pending",
            "fromAgentId":"main","toAgentId":"main","createdAt":pi_coding_agent::utils::iso_time::now_iso_string()}
    }))
    .unwrap()
}

fn source(delta: bool) -> oracle::TsOracle {
    oracle::TsOracle::spawn_with_environment(
        "control-db-oracle.mts",
        "packages/coding-agent/src",
        &[("PI_APPROVED_DELTA", if delta { "1" } else { "0" }.into())],
    )
}

fn delivery_snapshot(delta: bool, limit: usize) -> Value {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("source.sqlite");
    let mut ts = source(delta);
    let recipient = input("one").recipient;
    let response = ts.request(&json!({"id":1,"steps":[
        {"op":"registerSelf","args":[db,recipient]},
        {"op":"call","args":["enqueueStoredRuntimeMailboxMessage",[db,input("one")]]},
        {"op":"call","args":["enqueueStoredRuntimeMailboxMessage",[db,input("two")]]},
        {"op":"takeAll","args":[db,recipient,limit]},
        {"op":"call","args":["listRuntimeMailboxMessages",[db]]}
    ]}));
    for step in response["output"].as_array().unwrap() {
        assert_eq!(step["ok"], json!(true), "{response:?}");
    }
    json!({"delivered":response["output"][3]["value"].as_array().unwrap().len(),
        "statuses":response["output"][4]["value"].as_array().unwrap().iter().map(|row| row["status"].clone()).collect::<Vec<_>>()})
}

fn native_delivery_snapshot(limit: usize) -> Value {
    let temp = tempfile::tempdir().unwrap();
    let db = temp
        .path()
        .join("native.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let recipient = input("one").recipient;
    register_runtime_mailbox_listener(&db, &recipient, std::process::id(), None, Default::default()).unwrap();
    for id in ["one", "two"] {
        enqueue_stored_runtime_mailbox_message(&db, &input(id)).unwrap();
    }
    let delivered = take_runtime_mailbox_messages_for_delivery(&db, &recipient, |_| Ok(true), limit).unwrap();
    let rows = list_runtime_mailbox_messages(&db).unwrap();
    json!({"delivered":delivered.len(),"statuses":rows.iter().map(|row| js_json::to_value(&row.status).unwrap()).collect::<Vec<_>>()})
}

#[test]
fn zero_limit_matches_approved_delta_without_claims() {
    let expected = json!({"delivered":0,"statuses":["pending","pending"]});
    assert_eq!(delivery_snapshot(true, 0), expected);
    assert_eq!(native_delivery_snapshot(0), expected);
    let positive = json!({"delivered":1,"statuses":["claimed","pending"]});
    assert_eq!(delivery_snapshot(true, 1), positive);
    assert_eq!(native_delivery_snapshot(1), positive);
}

#[test]
fn frozen_zero_limit_has_exact_approved_discrepancy() {
    assert_eq!(
        delivery_snapshot(false, 0),
        json!({"delivered":2,"statuses":["claimed","claimed"]})
    );
    assert_eq!(
        native_delivery_snapshot(0),
        json!({"delivered":0,"statuses":["pending","pending"]})
    );
}

fn source_retry(delta: bool, host: &str) -> Value {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("source.sqlite");
    let mut ts = source(delta);
    let mut first = input("retry");
    first.recipient_host = Some(String::new().into());
    let retry = EnqueueRuntimeMailboxMessageInput {
        recipient_host: Some(host.to_owned().into()),
        recipient: first.recipient.clone(),
        sender: first.sender.clone(),
        kind: first.kind,
        store_ref: first.store_ref.clone(),
    };
    let mut initial = retry.clone();
    initial.recipient_host = Some(String::new().into());
    let response = ts.request(&json!({"id":1,"steps":[
        {"op":"call","args":["upsertMultiAgentMailboxMessage",[db,first.store_ref.session_path,first.store_ref.message_id,first.message]]},
        {"op":"call","args":["enqueueRuntimeMailboxMessage",[db,initial]]},
        {"op":"call","args":["enqueueRuntimeMailboxMessage",[db,retry]]}
    ]}));
    assert_eq!(response["output"][0]["ok"], json!(true), "{response:?}");
    assert_eq!(response["output"][1]["ok"], json!(true), "{response:?}");
    if delta && host.is_empty() {
        assert_eq!(
            enqueue_runtime_mailbox_message(db.to_str().expect("utf-8 test path"), &retry).unwrap(),
            1,
            "Rust must accept the source-created routing row"
        );
    }
    response["output"][2].clone()
}

fn native_retry(host: &str) -> Result<i64, ControlDbError> {
    let temp = tempfile::tempdir().unwrap();
    let db = temp
        .path()
        .join("native.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut first = input("retry");
    first.recipient_host = Some(String::new().into());
    upsert_multi_agent_mailbox_message(
        &db,
        &first.store_ref.session_path,
        &first.store_ref.message_id,
        &first.message,
    )
    .unwrap();
    let retry = EnqueueRuntimeMailboxMessageInput {
        recipient_host: Some(host.to_owned().into()),
        recipient: first.recipient,
        sender: first.sender,
        kind: first.kind,
        store_ref: first.store_ref,
    };
    let mut initial = retry.clone();
    initial.recipient_host = Some(String::new().into());
    let initial_id = enqueue_runtime_mailbox_message(&db, &initial).unwrap();
    let id = enqueue_runtime_mailbox_message(&db, &retry)?;
    assert_eq!(id, initial_id);
    assert_eq!(list_runtime_mailbox_messages(&db).unwrap().len(), 1);
    Ok(id)
}

#[test]
fn empty_host_retry_matches_approved_delta_and_rejects_other_host() {
    assert_eq!(source_retry(true, ""), json!({"ok":true,"value":1}));
    assert_eq!(native_retry("").unwrap(), 1);
    assert_eq!(source_retry(true, "other-host")["ok"], json!(false));
    assert!(native_retry("other-host").is_err());
}

#[test]
fn frozen_empty_host_retry_has_exact_approved_discrepancy() {
    assert_eq!(
        source_retry(false, ""),
        json!({"ok":false,"error":{
            "name":"Error", "message":"Runtime mailbox store reference conflicts with canonical mailbox row: /fixture/sender.jsonl#retry"
        }})
    );
    assert_eq!(native_retry("").unwrap(), 1);
}
