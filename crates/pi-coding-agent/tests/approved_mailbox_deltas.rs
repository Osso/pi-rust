#[path = "support/oracle.rs"]
mod oracle;

use pi_coding_agent::core::session_control_db::*;
use serde_json::{Value, json};

fn input(id: &str) -> EnqueueStoredRuntimeMailboxMessageInput {
    serde_json::from_value(json!({
        "recipient":{"sessionId":"recipient"},"sender":{"sessionId":"sender"},
        "kind":"message","storeRef":{"sessionPath":"/fixture/sender.jsonl","messageId":id},
        "message":{"id":id,"body":"hello","kind":"message","status":"pending",
            "fromAgentId":"main","toAgentId":"main","createdAt":"2026-10-21T00:00:00.000Z"}
    }))
    .unwrap()
}

fn source(delta: bool) -> oracle::TsOracle {
    oracle::TsOracle::spawn_with_environment(
        "control-db-oracle.ts",
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
        assert_eq!(step["ok"], true, "{response}");
    }
    json!({"delivered":response["output"][3]["value"].as_array().unwrap().len(),
        "statuses":response["output"][4]["value"].as_array().unwrap().iter().map(|row| row["status"].clone()).collect::<Vec<_>>()})
}

fn native_delivery_snapshot(limit: usize) -> Value {
    let temp = tempfile::tempdir().unwrap();
    let db = temp.path().join("native.sqlite").display().to_string();
    let recipient = input("one").recipient;
    register_runtime_mailbox_listener(&db, &recipient, std::process::id(), None, Default::default()).unwrap();
    for id in ["one", "two"] {
        enqueue_stored_runtime_mailbox_message(&db, &input(id)).unwrap();
    }
    let delivered = take_runtime_mailbox_messages_for_delivery(&db, &recipient, |_| true, limit).unwrap();
    let rows = list_runtime_mailbox_messages(&db).unwrap();
    json!({"delivered":delivered.len(),"statuses":rows.iter().map(|row| serde_json::to_value(row.status).unwrap()).collect::<Vec<_>>()})
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
