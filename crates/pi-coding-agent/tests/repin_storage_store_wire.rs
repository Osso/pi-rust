use super::repin_storage_history as oracle;
use pi_coding_agent::core::multi_agent_store::{
    AgentLifecycle, AgentMailboxMessage, ContactParentInput, MailboxMessageCommandResult, MultiAgentStore,
    MultiAgentStoreOptions, ParentContactResult, RecordOutboundSessionMessageInput,
};
use pi_coding_agent::core::session_manager::SessionManager;
use pi_js_compat::js_json::{JsValue, from_value};
use rusqlite::OptionalExtension;
use serde_json::{Value, json};
use std::sync::Arc;

fn execute_native(store: &MultiAgentStore, operation: &str, input: &Value) -> AgentMailboxMessage {
    let value = JsValue::from(input);
    match operation {
        "outbound" => store
            .record_outbound_session_message(&from_value::<RecordOutboundSessionMessageInput>(&value).unwrap())
            .unwrap(),
        "contact" => match store
            .contact_parent(&"agent_2".into(), &from_value::<ContactParentInput>(&value).unwrap())
            .unwrap()
        {
            ParentContactResult::Ok { message, .. } => *message,
            error => panic!("contact: {error:?}"),
        },
        "sendMain" | "sendChild" => {
            let input = from_value(&value).unwrap();
            let result = if operation == "sendMain" {
                store.send_main_thread_mailbox_message(&input)
            } else {
                store.send_mailbox_message(&"agent_1".into(), 1, &input)
            }
            .unwrap();
            match result {
                MailboxMessageCommandResult::Ok { message, .. } => *message,
                error => panic!("send: {error:?}"),
            }
        }
        "notify" => {
            let mut agent = store.get_agent(&"agent_1".into()).unwrap();
            agent.lifecycle = AgentLifecycle::Completed;
            agent.revision = 2;
            agent.result =
                Some(from_value(&JsValue::from(json!({"summary":"Done","fileRefs":input["fileRefs"]}))).unwrap());
            store.publish_terminal_outbox_snapshot(&agent).unwrap();
            store.list_mailbox_messages().unwrap().remove(0)
        }
        "steer" => store
            .prepare_steering_message_for_lifecycle_coordinator(&"agent_1".into(), &from_value(&value).unwrap())
            .unwrap(),
        _ => panic!("unknown operation"),
    }
}

fn record_wire_pair(operation: &str) -> (Value, Value) {
    let directory = tempfile::tempdir().unwrap();
    let mut source = oracle::TsOracle::spawn("storage-store-wire-oracle.mts", "packages/coding-agent/src/core");
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let input = json!({"fromAgentId":"main","toAgentId":if operation=="sendMain"{"agent_1"}else{"agent_2"},"body":"Hello 漢😀","threadId":"thread",
        "fileRefs":[{"path":"/fixture/output.log","label":"Output 漢😀"}]});
    let source_dir = directory.path().join("source");
    let native_dir = directory.path().join("native");
    std::fs::create_dir(&source_dir).unwrap();
    std::fs::create_dir(&native_dir).unwrap();
    let expected=source.request(&json!({"id":1,"op":operation,"scenario":operation,"dir":source_dir,"db":source_dir.join("control.sqlite"),"now":now,"input":input}));
    assert_eq!(expected["ok"], true, "{expected}");
    let db = native_dir.join("control.sqlite");
    let created = source
        .request(&json!({"id":1,"op":"setup","scenario":operation,"dir":native_dir,"db":db,"now":now,"input":input}));
    assert_eq!(created["ok"], true, "{created}");
    let path = created["output"]["path"].as_str().unwrap();
    let mut manager = SessionManager::open(&path.into(), None, None).unwrap();
    manager
        .set_metadata_control_db_path(Some(&db.to_str().unwrap().into()), Default::default())
        .unwrap();
    let native_now = now.clone();
    let store = MultiAgentStore::from_session_manager(
        &manager,
        MultiAgentStoreOptions {
            now: Some(Arc::new(move || Ok(native_now.as_str().into()))),
        },
    )
    .unwrap();
    let message = execute_native(&store, operation, &input);
    let connection = pi_coding_agent::core::sqlite::create_sqlite_database(db.to_str().unwrap()).unwrap();
    let stored: Option<String> = connection
        .query_row(
            "SELECT data FROM multi_agent_mailbox_messages WHERE session_path=? AND message_id=?",
            rusqlite::params![path, message.id],
            |row| row.get(0),
        )
        .optional()
        .unwrap();
    (
        json!({"returned":serde_json::to_string(&message).unwrap(),"stored":stored.map(|data|json!({"data":data})).unwrap_or(Value::Null)}),
        expected["output"].clone(),
    )
}

#[test]
fn outbound_return_wire_matches_frozen_copy_message() {
    let (actual, expected) = record_wire_pair("outbound");
    assert_eq!(actual["returned"], expected["returned"]);
}

#[test]
fn outbound_canonical_json_bytes_match_frozen_constructor_order() {
    let (actual, expected) = record_wire_pair("outbound");
    assert_eq!(actual["stored"], expected["stored"]);
}

#[test]
fn direct_notification_and_steering_return_wire_matches_frozen() {
    for operation in ["contact", "sendMain", "sendChild", "notify", "steer"] {
        let (actual, expected) = record_wire_pair(operation);
        assert_eq!(actual["returned"], expected["returned"], "{operation}");
    }
}

#[test]
fn direct_and_notification_canonical_bytes_match_frozen() {
    for operation in ["contact", "sendMain", "sendChild", "notify", "steer"] {
        let (actual, expected) = record_wire_pair(operation);
        assert_eq!(actual["stored"], expected["stored"], "{operation}");
    }
}
