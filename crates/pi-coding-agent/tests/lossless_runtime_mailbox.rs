//! UTF-16 mailbox content must survive both persistent and in-memory public APIs.
use pi_coding_agent::core::session_control_db::*;
use pi_js_compat::js_json::{self, JsValue};

#[test]
fn enqueue_retry_restore_retains_raw_payload_and_rejects_replacement_collision() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("control.sqlite").display().to_string();
    let wire = r#"{"recipient":{"sessionId":"to","agentId":null},"sender":{"sessionId":"from","agentId":null},"kind":"message","storeRef":{"sessionPath":"/fixture/s","messageId":"m"},"message":{"id":"m","fromAgentId":"main","toAgentId":"main","kind":"message","status":"pending","body":"before\ud800after","fileRefs":[{"path":"/raw/\udfff","label":"\ud800"}],"unknown":{"\ud800":"high","�":"replacement"}}}"#;
    let input: EnqueueStoredRuntimeMailboxMessageInput =
        js_json::from_value(&js_json::parse_lossless(wire).unwrap()).unwrap();
    let id = enqueue_stored_runtime_mailbox_message(&database, &input).unwrap();
    assert_eq!(enqueue_stored_runtime_mailbox_message(&database, &input).unwrap(), id);
    let message = read_runtime_mailbox_message(&database, id).unwrap().unwrap();
    let projection = js_json::to_value(&message).unwrap();
    assert_eq!(projection.get("body"), input.message.get("body"));
    assert_eq!(projection.get("fileRefs"), input.message.get("fileRefs"));
    let state = read_multi_agent_state(&database, "/fixture/s").unwrap().unwrap();
    assert_eq!(state.mailbox_messages[0].get("body"), input.message.get("body"));
    struct Persistence(String);
    impl pi_coding_agent::core::multi_agent_store::SessionPersistence for Persistence {
        fn metadata_control_db_path(&self) -> Option<String> {
            Some(self.0.clone())
        }
        fn session_file(&self) -> Option<String> {
            Some("/fixture/s".into())
        }
    }
    let store = pi_coding_agent::core::multi_agent_store::MultiAgentStore::from_session_manager(
        &Persistence(database.clone()),
        Default::default(),
    )
    .unwrap();
    let restored = store.list_mailbox_messages().unwrap();
    assert_eq!(
        js_json::to_value(&restored[0]).unwrap().get("body"),
        input.message.get("body")
    );
    let mut collision = input.clone();
    collision
        .message
        .as_object_mut()
        .unwrap()
        .insert("body".into(), JsValue::String("before�after".into()));
    assert!(enqueue_stored_runtime_mailbox_message(&database, &collision).is_err());
}

#[test]
fn supervisor_request_and_response_preserve_raw_content() {
    let directory = tempfile::tempdir().unwrap();
    let database = directory.path().join("control.sqlite").display().to_string();
    let payload = js_json::parse_lossless(r#"{"question":"why\ud800","\udfff":"low","�":"replacement"}"#).unwrap();
    let input = PostSupervisorRequestInput {
        sender_session_id: "from".into(),
        project_id: "p".into(),
        kind: SupervisorRequestKind::SupervisorAdvisory,
        payload: payload.as_object().unwrap().clone(),
        deadline_at: "2099-01-01T00:00:00.000Z".into(),
    };
    let id = post_supervisor_request(&database, &input).unwrap();
    let restored = read_supervisor_request(&database, id).unwrap().unwrap();
    assert_eq!(restored.payload, input.payload);
    let response: SupervisorResponse =
        js_json::from_value(&js_json::parse_lossless(r#"{"kind":"advisory","answer":"raw\udfff"}"#).unwrap()).unwrap();
    assert_eq!(
        js_json::stringify_lossless(&js_json::to_value(&response).unwrap()),
        r#"{"answer":"raw\udfff","kind":"advisory"}"#
    );
}

#[test]
fn store_message_serialization_preserves_raw_addresses_file_refs_and_unknown_keys() {
    use pi_coding_agent::core::multi_agent_store::{AgentMailboxMessage, copy_message};
    let raw = js_json::parse_lossless(r#"{"id":"m\udfff","threadId":"t\ud800","fromAgentId":"from\ud800","toAgentId":"to\udfff","kind":"message","status":"pending","createdAt":"2026-10-21T00:00:00.000Z","updatedAt":"2026-10-21T00:00:00.000Z","body":"raw\ud800","fileRefs":[{"path":"/raw/\udfff","label":"\ud800"}],"\ud800":"high","�":"replacement"}"#).unwrap();
    let message: AgentMailboxMessage = js_json::from_value(&raw).unwrap();
    let serialized = js_json::to_value(&message).unwrap();
    assert_eq!(serialized, raw);
    let copied = js_json::to_value(&copy_message(&message)).unwrap();
    for key in ["id", "threadId", "fromAgentId", "toAgentId", "body", "fileRefs"] {
        assert_eq!(copied.get(key), raw.get(key));
    }
    assert_eq!(message.extra.len(), 2);
}

#[test]
fn completion_notification_keeps_summary_units_and_duration() {
    use pi_coding_agent::core::multi_agent_store::{AgentSnapshot, MultiAgentStore, agent_to_value};
    let now = pi_coding_agent::utils::iso_time::now_iso_string();
    let raw = js_json::parse_lossless(r#"{"id":"agent_1","displayName":"Scout","agentType":"scout","lifecycle":"completed","revision":2,"createdAt":"","updatedAt":"","cwd":"/repo","permission":{"policy":"on-request","narrowed":true},"result":{"summary":"  raw\ud800  ","durationMs":42}}"#).unwrap();
    let mut agent: AgentSnapshot = js_json::from_value(&raw).unwrap();
    agent.created_at = now.clone();
    agent.updated_at = now;
    assert_eq!(agent_to_value(&agent).unwrap().get("result"), raw.get("result"));
    let store = MultiAgentStore::new(Default::default());
    store.publish_terminal_outbox_snapshot(&agent).unwrap();
    let messages = store.list_mailbox_messages().unwrap();
    let expected = js_json::parse_lossless(r#""Scout completed: raw\ud800. Duration: 42ms""#).unwrap();
    assert_eq!(messages[0].body.as_ref(), expected.as_string());
}
