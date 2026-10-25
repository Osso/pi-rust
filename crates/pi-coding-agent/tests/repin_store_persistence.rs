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
fn agent(now: &str, lifecycle: &str, revision: i64) -> Value {
    json!({"id":"child","parentId":"main","displayName":"child","agentType":"worker","lifecycle":lifecycle,"revision":revision,"createdAt":now,"updatedAt":now,"cwd":"/fixture","permission":{"policy":"on-request","narrowed":true}})
}
#[test]
fn restore_clears_source_listener_sets_but_keeps_update_subscribers() {
    let tmp = tempfile::tempdir().unwrap();
    let session = Session {
        db: tmp.path().join("control.sqlite").display().to_string(),
        path: "/fixture/s".into(),
    };
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let running = agent(&now, "running", 1);
    let terminal = agent(&now, "completed", 2);
    let mut ts = oracle::TsOracle::spawn(
        "multi-agent-store-oracle.ts",
        "packages/coding-agent/src/core/multi-agent-store.ts",
    );
    let peer=ts.request(&json!({"id":1,"now":now,"steps":[{"op":"probeRestoreListeners","args":[session.db,session.path,running,terminal]}]}));
    assert_eq!(peer["output"][0]["ok"], true, "{peer}");
    let store = MultiAgentStore::new(MultiAgentStoreOptions::default());
    let counts = Arc::new(Mutex::new([0_u32; 3]));
    let transitions = counts.clone();
    let _t = store.subscribe_agent_transitions(move |_, _| {
        transitions.lock().unwrap()[0] += 1;
        Ok(())
    });
    let lifecycle = counts.clone();
    let _l = store.subscribe_lifecycle_notifications(move |_| {
        lifecycle.lock().unwrap()[1] += 1;
        Ok(())
    });
    let updates = counts.clone();
    let _u = store.subscribe_agent_updates(move |_, _| {
        updates.lock().unwrap()[2] += 1;
        Ok(())
    });
    store.restore_from_session_manager(&session).unwrap();
    store
        .publish_lifecycle_coordinator_snapshot(&serde_json::from_value(running).unwrap())
        .unwrap();
    store
        .publish_terminal_outbox_snapshot(&serde_json::from_value(terminal).unwrap())
        .unwrap();
    let counts = *counts.lock().unwrap();
    assert_eq!(
        json!({"transitions":counts[0],"lifecycle":counts[1],"updates":counts[2]}),
        peer["output"][0]["value"]
    );
}
#[test]
fn persistent_counter_and_mailbox_read_errors_propagate_without_local_fallback() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("blocked.sqlite");
    std::fs::create_dir(&path).unwrap();
    let session = Session {
        db: path.display().to_string(),
        path: "/fixture/s".into(),
    };
    let store = MultiAgentStore::new(MultiAgentStoreOptions::default());
    let input: RecordOutboundSessionMessageInput =
        serde_json::from_value(json!({"fromAgentId":"main","toAgentId":"main","body":"memory"})).unwrap();
    store.record_outbound_session_message(&input).unwrap();
    store.set_persistence_session_manager(Some(&session));
    assert!(store.allocate_agent_id_for_lifecycle_coordinator("agent").is_err());
    assert!(store.list_mailbox_messages().is_err());
    std::fs::remove_dir(&path).unwrap();
    assert_eq!(
        store.allocate_agent_id_for_lifecycle_coordinator("agent").unwrap(),
        "agent_1"
    );
    assert!(store.list_mailbox_messages().unwrap().is_empty());
}
#[test]
fn old_terminal_snapshot_does_not_create_fresh_notification() {
    let now = pi_js_compat::date::js_now_millis() as i64;
    let current = pi_coding_agent::utils::iso_time::format_iso_millis(now);
    let old = pi_coding_agent::utils::iso_time::format_iso_millis(now - 86_400_000);
    let mut snapshot = agent(&current, "completed", 2);
    snapshot["updatedAt"] = json!(old);
    let clock = current.clone();
    let store = MultiAgentStore::new(MultiAgentStoreOptions {
        now: Some(Arc::new(move || clock.clone())),
    });
    store
        .publish_terminal_outbox_snapshot(&serde_json::from_value(snapshot.clone()).unwrap())
        .unwrap();
    assert!(store.list_mailbox_messages().unwrap().is_empty());
    let mut ts = oracle::TsOracle::spawn(
        "multi-agent-store-oracle.ts",
        "packages/coding-agent/src/core/multi-agent-store.ts",
    );
    let peer=ts.request(&json!({"id":1,"now":current,"steps":[{"op":"publishTerminalOutboxSnapshot","args":[snapshot]},{"op":"listMailboxMessages","args":[]}]}));
    assert_eq!(peer["output"][1]["value"], json!([]));
}
