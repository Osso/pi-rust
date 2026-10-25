//! Primary schema always comes from the frozen public TS API. Historical secondary
//! tables/rows are explicit fixture mutations, not alternative production schemas.
use pi_js_compat::js_json::JsString;
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
fn new_fixture() -> (tempfile::TempDir, String, oracle::TsOracle) {
    let t = tempfile::tempdir().unwrap();
    let db = t
        .path()
        .join("control.sqlite")
        .to_str()
        .expect("utf-8 test path")
        .to_owned();
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.mts", "packages/coding-agent/src");
    call(&mut ts, "listRuntimeMailboxMessages", json!([db]));
    (t, db, ts)
}
#[test]
fn legacy_delivered_rows_are_removed_while_claimed_recovers_pending() {
    let (_t, db, mut ts) = new_fixture();
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    for id in ["legacy-delivered", "legacy-claimed"] {
        call(
            &mut ts,
            "enqueueStoredRuntimeMailboxMessage",
            json!([db,{"recipient":{"sessionId":"to","agentId":null},"sender":{"sessionId":"from","agentId":null},"kind":"message","storeRef":{"sessionPath":"/fixture/s","messageId":id},"message":{"id":id,"fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","createdAt":now,"body":"legacy"}}]),
        );
    }
    step(&mut ts,"fixtureExec",json!([db,"CREATE TABLE runtime_mailbox_messages AS SELECT rowid AS id, 'to' AS recipient_session_id, NULL AS recipient_agent_id, 'from' AS sender_session_id, NULL AS sender_agent_id, 'message' AS kind, session_path AS store_session_path, message_id AS store_message_id, CASE message_id WHEN 'legacy-delivered' THEN 'delivered' ELSE 'claimed' END AS status, json_extract(data,'$.createdAt') AS created_at, updated_at, NULL AS error FROM multi_agent_mailbox_messages;
 CREATE TABLE multi_agent_terminal_events AS SELECT 'historical-presence-marker' AS fixture;
 INSERT INTO multi_agent_terminal_outbox(session_path,agent_id,terminal_revision,event_kind,status,updated_at) VALUES('/fixture/s','old',1,'completed','delivered','old'),('/fixture/s','old',2,'completed','pending','recent');
 PRAGMA user_version = 13;"]));
    let messages = list_runtime_mailbox_messages(&db).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].store_ref.message_id, "legacy-claimed");
    assert_eq!(messages[0].status, RuntimeMailboxMessageStatus::Pending);
    let rows = step(
        &mut ts,
        "fixtureSql",
        json!([
            db,
            "SELECT status FROM multi_agent_terminal_outbox ORDER BY terminal_revision",
            []
        ]),
    );
    assert_eq!(rows, json!([{"status":"pending"}]));
    assert_eq!(
        call(&mut ts, "listRuntimeMailboxMessages", json!([db]))
            .as_array()
            .unwrap()
            .len(),
        1
    );
}
#[test]
fn missing_parent_and_obsolete_search_column_migrate_without_peer_row_normalization() {
    let (_t, db, mut ts) = new_fixture();
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    call(
        &mut ts,
        "writeSessionMetadata",
        json!([db,{"sessionPath":"/fixture/s","id":"s","cwd":"/fixture","name":"Peer Name","createdAt":now,"modifiedAt":now,"messageCount":1,"firstMessage":"body"}]),
    );
    step(
        &mut ts,
        "fixtureExec",
        json!([
            db,
            "DROP INDEX session_metadata_parent_session_path_idx; ALTER TABLE session_metadata DROP COLUMN parent_session_path; ALTER TABLE session_metadata ADD COLUMN all_messages_text TEXT NOT NULL DEFAULT ''; "
        ]),
    );
    let row = read_session_metadata(&db, "/fixture/s").unwrap().unwrap();
    assert_eq!(row.name.as_ref(), Some(&JsString::from("Peer Name")));
    write_session_name(&db, "/fixture/s", Some(&JsString::from("Rust Name"))).unwrap();
    assert_eq!(
        call(&mut ts, "readSessionMetadata", json!([db, "/fixture/s"]))["name"],
        json!("Rust Name")
    );
}
#[test]
fn legacy_counters_merge_monotonically_and_payload_fields_are_stripped() {
    let (_t, db, mut ts) = new_fixture();
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    call(
        &mut ts,
        "writeMultiAgentCounters",
        json!([db,"/fixture/s",{"nextAgentNumber":2,"nextMessageNumber":3}]),
    );
    call(
        &mut ts,
        "bootstrapMultiAgentAgent",
        json!([db,"/fixture/s","old",{"id":"old","displayName":"old","agentType":"worker","cwd":"/fixture","permission":{"policy":"on-request","narrowed":true},"lifecycle":"completed","revision":3,"createdAt":now,"updatedAt":now}]),
    );
    step(&mut ts,"fixtureExec",json!([db,"CREATE TABLE multi_agent_counters AS SELECT session_path,next_agent_number+5 AS next_agent_number,next_message_number+5 AS next_message_number,updated_at FROM multi_agent_counters_v2;
 DROP TRIGGER multi_agent_agents_reject_legacy_artifact_fields_update;
 UPDATE multi_agent_agents SET data=json_set(data,'$.nested',json('{\"artifactIds\":[1],\"keep\":\"value\"}'));
 PRAGMA user_version=13;"]));
    let state = read_multi_agent_state(&db, "/fixture/s").unwrap().unwrap();
    assert_eq!(state.counters.next_agent_number, 7);
    assert_eq!(state.counters.next_message_number, 8);
    assert_eq!(state.agents[0]["nested"], json!({"keep":"value"}));
    assert_eq!(
        call(&mut ts, "allocateMultiAgentCounter", json!([db, "/fixture/s", "agent"])),
        json!(7)
    );
    assert_eq!(
        allocate_multi_agent_counter(&db, "/fixture/s", MultiAgentCounterName::Message).unwrap(),
        8
    );
    assert_eq!(
        call(&mut ts, "readMultiAgentState", json!([db, "/fixture/s"]))["counters"],
        json!({"nextAgentNumber":8,"nextMessageNumber":9})
    );
}
#[test]
fn migration_quiescence_tracks_owned_fixture_process_then_allows_after_exit() {
    let (_t, db, mut ts) = new_fixture();
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let identity = js_json::parse_lossless(
        call(&mut ts, "getRuntimeProcessInstanceId", json!([]))
            .as_string()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        call(
            &mut ts,
            "createMultiAgentChildWithRuntimeOwnership",
            json!([db,{"sessionPath":"/fixture/s","agentId":"child","processIdentity":identity,"owner":{"sessionId":"owner","agentId":null},"nowIso":now,"agent":{"id":"child","parentId":"main","displayName":"child","agentType":"worker","lifecycle":"running","revision":1,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true}}}])
        )["ok"],
        json!(true)
    );
    step(&mut ts, "fixtureExec", json!([db, "PRAGMA user_version=14;"]));
    let error = read_session_metadata(&db, "/fixture/s").unwrap_err();
    assert!(error.message().contains("while lifecycle owners are active"));
    let peer = ts.request(&json!({"id":1,"steps":[{"op":"call","args":["readSessionMetadata",[db,"/fixture/s"]]}]}));
    assert_eq!(peer["output"][0]["ok"], json!(false));
    assert_eq!(peer["output"][0]["error"]["message"], json!(error.message()));
    drop(ts);
    fixture_process::wait_for_owned_fixture_exit(&js_json::from_value(&identity).unwrap());
    assert!(read_session_metadata(&db, "/fixture/s").unwrap().is_none());
}
