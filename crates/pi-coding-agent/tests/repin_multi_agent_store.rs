#[path = "support/oracle.rs"]
mod oracle;
use pi_coding_agent::core::multi_agent_store::*;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
struct Session {
    db: String,
    path: String,
}
impl SessionPersistence for Session {
    fn metadata_control_db_path(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some(self.db.as_str().into())
    }
    fn session_file(&self) -> Option<pi_js_compat::js_json::JsString> {
        Some(self.path.as_str().into())
    }
}
fn input() -> RecordOutboundSessionMessageInput {
    serde_json::from_value(json!({"fromAgentId":"main","toAgentId":"main","body":"body"})).unwrap()
}
fn call(ts: &mut oracle::TsOracle, name: &str, args: Value) -> Value {
    let r = ts.request(&json!({"id":1,"steps":[{"op":"call","args":[name,args]}]}));
    assert_eq!(r["output"][0]["ok"], true, "{r}");
    r["output"][0]["value"].clone()
}
#[test]
fn delivered_message_is_removed_and_original_snapshot_returned() {
    let store = MultiAgentStore::new(MultiAgentStoreOptions::default());
    let msg = store.record_outbound_session_message(&input()).unwrap();
    assert_eq!(store.mark_mailbox_message_delivered(&msg.id).unwrap().unwrap(), msg);
    assert!(store.list_mailbox_messages().unwrap().is_empty());
}
#[test]
fn mailbox_expires_at_controlled_creation_age_not_updated_age() {
    let now = Arc::new(Mutex::new(pi_js_compat::date::js_now_millis() as i64));
    let clock = now.clone();
    let store = MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(move || {
            Ok(pi_coding_agent::utils::iso_time::format_iso_millis(*clock.lock().unwrap()).into())
        })),
    });
    store.record_outbound_session_message(&input()).unwrap();
    *now.lock().unwrap() += 86_400_000 - 1;
    assert_eq!(store.list_mailbox_messages().unwrap().len(), 1);
    *now.lock().unwrap() += 1;
    assert!(store.list_mailbox_messages().unwrap().is_empty());
}
#[test]
fn ts_removed_persisted_row_is_evicted_without_resurrection() {
    let tmp = tempfile::tempdir().unwrap();
    let session = Session {
        db: tmp.path().join("control.sqlite").display().to_string(),
        path: "/fixture/s".into(),
    };
    let mut ts = oracle::TsOracle::spawn("control-db-oracle.ts", "packages/coding-agent/src");
    call(&mut ts, "readMultiAgentState", json!([session.db, session.path]));
    let store = MultiAgentStore::new(MultiAgentStoreOptions::default());
    store.set_persistence_session_manager(Some(&session));
    let msg = store.record_outbound_session_message(&input()).unwrap();
    assert_eq!(store.list_mailbox_messages().unwrap().len(), 1);
    call(
        &mut ts,
        "consumeRuntimeMailboxMessageByStoreRef",
        json!([session.db,{"sessionPath":session.path,"messageId":msg.id}]),
    );
    assert!(store.list_mailbox_messages().unwrap().is_empty());
    assert!(store.mark_mailbox_message_failed(&msg.id, "late").unwrap().is_none());
    assert!(
        call(
            &mut ts,
            "getMultiAgentMailboxMessageStatus",
            json!([session.db, session.path, msg.id])
        )
        .is_null()
    );
}
#[test]
fn persisted_tool_activity_roundtrip_preserves_detail() {
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let activity: AgentCurrentActivity = serde_json::from_value(
        json!({"phase":"tool","startedAt":now,"toolCallId":"t","toolName":"read","detail":"/fixture/a.rs"}),
    )
    .unwrap();
    assert_eq!(serde_json::to_value(activity).unwrap()["detail"], "/fixture/a.rs");
}
#[test]
fn terminal_snapshot_suppression_prevents_owner_notifications() {
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let store = MultiAgentStore::new(MultiAgentStoreOptions::default());
    let agent:AgentSnapshot=serde_json::from_value(json!({"id":"child","parentId":"main","displayName":"child","agentType":"worker","lifecycle":"completed","revision":2,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true},"detached":true,"suppressTerminalNotification":true})).unwrap();
    store.publish_terminal_outbox_snapshot(&agent).unwrap();
    assert!(store.list_mailbox_messages().unwrap().is_empty());
}
